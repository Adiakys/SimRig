//! The adapter side of the protocol: reads requests from stdin, answers on stdout.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value as Json;
use simrig_model::DeviceInfo;
use simrig_protocol::{
    API_VERSION, ActionParams, AttachParams, AttachResult, ComponentParams, ComponentsChanged,
    DetachParams, DevicesChanged, DiscoverResult, InitializeParams, InitializeResult, InputState,
    ItemParams, Message, ParamsChanged, PrivilegedRequest, PrivilegedResult, RpcError, SetParams,
    SubscribeParams, code, method, read_message, write_message,
};

use crate::dispatch;
use crate::error::{Error, Result};
use crate::interfaces::AnyComponent;
use crate::{Adapter, Change};

/// How often the adapter is asked for changes when nothing else happens.
const CHANGE_POLL: Duration = Duration::from_millis(250);
/// How long a privileged operation may take (it may wait for the user's password).
const PRIVILEGED_TIMEOUT: Duration = Duration::from_secs(600);

/// Runs the adapter on stdin/stdout until the host sends `shutdown` or closes stdin.
pub fn run<A: Adapter>(adapter: A) -> ExitCode {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        loop {
            let next = read_message(&mut reader);
            let end = matches!(next, Ok(None));
            if tx.send(next.map_err(|e| e.to_string())).is_err() || end {
                break;
            }
        }
    });
    let stdout = io::stdout();
    match Server::new(adapter).serve(rx, &mut stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("simrig plugin: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Incoming message, or a read error description. `Ok(None)` is end of stream.
pub type Incoming = std::result::Result<Option<Message>, String>;

/// The way back to the host while handling a request, e.g. to run a privileged operation of
/// the plugin's driver during [`Component::save`](crate::Component::save).
pub struct HostLink<'a> {
    rx: &'a Receiver<Incoming>,
    out: &'a mut dyn Write,
    next_id: &'a mut u64,
    backlog: &'a mut VecDeque<Incoming>,
}

impl HostLink<'_> {
    /// Asks the host to run `operation`, declared in the plugin's `driver.toml`, with `args`.
    /// The host may ask the user for a password; returns what the operation printed.
    pub fn privileged(&mut self, operation: &str, args: &[(&str, &str)]) -> Result<String> {
        let id = *self.next_id;
        *self.next_id += 1;
        let args: BTreeMap<String, String> = args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let request = PrivilegedRequest {
            operation: operation.into(),
            args,
        };
        write_message(self.out, &Message::request(id, method::PRIVILEGED, request))
            .map_err(|e| Error::failed(e.to_string()))?;
        let deadline = Instant::now() + PRIVILEGED_TIMEOUT;
        loop {
            let incoming = self
                .rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| Error::failed("no answer from the host"))?;
            match incoming {
                Ok(Some(m)) if m.is_response() && m.id == Some(id) => {
                    if let Some(e) = m.error {
                        return Err(match e.code {
                            code::DENIED => Error::Failed(format!("not authorized: {}", e.message)),
                            _ => Error::Failed(e.message),
                        });
                    }
                    let result: PrivilegedResult = m
                        .result
                        .and_then(|r| serde_json::from_value(r).ok())
                        .ok_or_else(|| Error::failed("invalid answer from the host"))?;
                    return Ok(result.output);
                }
                Ok(None) => return Err(Error::failed("the host went away")),
                // Anything else is handled after the current request.
                other => self.backlog.push_back(other),
            }
        }
    }
}

struct Attached {
    device: String,
    component: AnyComponent,
}

struct Subscription {
    interval: Duration,
    next: Instant,
}

/// Protocol server around an [`Adapter`]. [`run`] wires it to stdio; tests can drive it
/// directly with [`Server::serve`].
pub struct Server<A: Adapter> {
    adapter: A,
    devices: Vec<String>,
    components: Vec<Attached>,
    subscriptions: HashMap<String, Subscription>,
    /// Virtual devices last reported to the host, once it asked for them.
    discovered: Option<Vec<DeviceInfo>>,
    backlog: VecDeque<Incoming>,
    next_id: u64,
    initialized: bool,
}

type Reply = std::result::Result<Json, RpcError>;

impl<A: Adapter> Server<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            devices: Vec::new(),
            components: Vec::new(),
            subscriptions: HashMap::new(),
            discovered: None,
            backlog: VecDeque::new(),
            next_id: 1,
            initialized: false,
        }
    }

    /// Serves messages from `rx` until shutdown or end of stream.
    pub fn serve(&mut self, rx: Receiver<Incoming>, out: &mut impl Write) -> io::Result<()> {
        loop {
            let incoming = match self.backlog.pop_front() {
                Some(m) => Some(m),
                None => {
                    let deadline = self
                        .next_tick()
                        .unwrap_or(Instant::now() + CHANGE_POLL)
                        .min(Instant::now() + CHANGE_POLL);
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(m) => Some(m),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
            };
            match incoming {
                None => self.send_due_states(out)?,
                Some(Ok(None)) => return Ok(()),
                Some(Err(e)) => {
                    let error = RpcError::new(code::PARSE_ERROR, e);
                    write_message(out, &Message::error_response(None, error))?;
                }
                Some(Ok(Some(message))) => {
                    if !self.handle(message, &rx, out)? {
                        return Ok(());
                    }
                }
            }
            if self.initialized {
                self.flush_changes(out)?;
            }
        }
    }

    /// Handles one message; returns `false` after `shutdown`.
    fn handle(
        &mut self,
        message: Message,
        rx: &Receiver<Incoming>,
        out: &mut impl Write,
    ) -> io::Result<bool> {
        let Some(name) = message.method.clone() else {
            return Ok(true); // a stray response: nothing waits for it
        };
        let params = message.params.unwrap_or(Json::Null);
        let component = params
            .get("component")
            .and_then(Json::as_str)
            .map(str::to_owned);
        let reply = self.dispatch(&name, params, rx, out);
        if let Some(id) = message.id {
            let response = match &reply {
                Ok(result) => Message::response(id, result),
                Err(error) => Message::error_response(Some(id), error.clone()),
            };
            write_message(out, &response)?;
        }
        if reply.is_ok() && name == method::SHUTDOWN {
            return Ok(false);
        }
        // Setting a value or running an action may change which items are available (e.g.
        // `DependsOn`): send the fresh description so the UI stays live.
        if reply.is_ok()
            && [
                method::STD_SET,
                method::CUSTOM_SET,
                method::ACTION_RUN,
                method::SAVE,
            ]
            .contains(&name.as_str())
        {
            if let Some(id) = component {
                self.notify_params_changed(&id, out)?;
            }
        }
        Ok(true)
    }

    fn dispatch(
        &mut self,
        name: &str,
        params: Json,
        rx: &Receiver<Incoming>,
        out: &mut impl Write,
    ) -> Reply {
        if !self.initialized && name != method::INITIALIZE {
            return Err(RpcError::new(code::INVALID_REQUEST, "not initialized"));
        }
        let rpc = |e: Error| e.to_rpc();
        match name {
            method::INITIALIZE => {
                let p: InitializeParams = parse(params)?;
                if p.api_version != API_VERSION {
                    let msg = format!(
                        "plugin speaks API {API_VERSION}, host speaks {}",
                        p.api_version
                    );
                    return Err(RpcError::new(code::INCOMPATIBLE, msg));
                }
                self.initialized = true;
                reply(InitializeResult {
                    api_version: API_VERSION,
                    plugin: self.adapter.info(),
                })
            }
            method::DISCOVER => {
                let devices = self.adapter.discover();
                self.discovered = Some(devices.clone());
                reply(DiscoverResult { devices })
            }
            method::ATTACH => {
                let p: AttachParams = parse(params)?;
                self.attach(p.device)
            }
            method::DETACH => {
                let p: DetachParams = parse(params)?;
                self.detach(&p.device);
                reply(())
            }
            method::DESCRIBE => {
                let p: ComponentParams = parse(params)?;
                reply(dispatch::describe(self.component(&p.component)?))
            }
            method::STD_GET => {
                let p: ItemParams = parse(params)?;
                let c = self.component(&p.component)?;
                reply(dispatch::get_param(c, &p.item).map_err(rpc)?)
            }
            method::STD_SET => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::set_param(c, &p.item, &p.value).map_err(rpc)?;
                reply(())
            }
            method::CUSTOM_SET => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::set_custom(c, &p.item, &p.value).map_err(rpc)?;
                reply(())
            }
            method::SAVE => {
                let p: ItemParams = parse(params)?;
                let Server {
                    components,
                    backlog,
                    next_id,
                    ..
                } = self;
                let c = find(components, &p.component)?;
                let mut link = HostLink {
                    rx,
                    out,
                    next_id,
                    backlog,
                };
                dispatch::save(c, &p.item, &mut link).map_err(rpc)?;
                reply(())
            }
            method::ACTION_RUN => {
                let p: ActionParams = parse(params)?;
                let c = self.component(&p.component)?;
                reply(dispatch::run_action(c, &p.item, &p.args).map_err(rpc)?)
            }
            method::OUTPUT_WRITE => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::write_output(c, &p.item, &p.value).map_err(rpc)?;
                reply(())
            }
            method::INPUT_READ => {
                let p: ComponentParams = parse(params)?;
                let values = dispatch::read_inputs(self.component(&p.component)?);
                reply(InputState {
                    component: p.component,
                    values,
                })
            }
            method::INPUT_SUBSCRIBE => {
                let p: SubscribeParams = parse(params)?;
                self.component(&p.component)?;
                if !(p.rate_hz > 0.0 && p.rate_hz <= 1000.0) {
                    return Err(RpcError::new(
                        code::INVALID_PARAMS,
                        "rate_hz must be in (0, 1000]",
                    ));
                }
                let interval = Duration::from_secs_f64(1.0 / p.rate_hz);
                self.subscriptions.insert(
                    p.component,
                    Subscription {
                        interval,
                        next: Instant::now(),
                    },
                );
                reply(())
            }
            method::INPUT_UNSUBSCRIBE => {
                let p: ComponentParams = parse(params)?;
                self.subscriptions.remove(&p.component);
                reply(())
            }
            method::SHUTDOWN => reply(()),
            other => Err(RpcError::new(
                code::METHOD_NOT_FOUND,
                format!("unknown method '{other}'"),
            )),
        }
    }

    fn attach(&mut self, device: DeviceInfo) -> Reply {
        self.detach(&device.id);
        let mut components = self.adapter.attach(&device).map_err(|e| e.to_rpc())?;
        for (i, c) in components.iter().enumerate() {
            let id = c.id();
            let duplicate = components[..i].iter().any(|o| o.id() == id)
                || self.components.iter().any(|a| a.component.id() == id);
            if duplicate {
                return Err(RpcError::new(
                    code::INTERNAL,
                    format!("duplicate component id '{id}'"),
                ));
            }
        }
        let descriptions = components.iter_mut().map(dispatch::describe).collect();
        self.devices.push(device.id.clone());
        self.components
            .extend(components.into_iter().map(|component| Attached {
                device: device.id.clone(),
                component,
            }));
        reply(AttachResult {
            components: descriptions,
        })
    }

    fn detach(&mut self, device: &str) {
        if let Some(i) = self.devices.iter().position(|d| d == device) {
            self.devices.remove(i);
            self.components.retain(|a| a.device != device);
            self.adapter.detach(device);
        }
        let live: Vec<String> = self.components.iter().map(|a| a.component.id()).collect();
        self.subscriptions.retain(|id, _| live.contains(id));
    }

    fn component(&mut self, id: &str) -> std::result::Result<&mut AnyComponent, RpcError> {
        find(&mut self.components, id)
    }

    /// Applies the adapter's own changes and tells the host.
    fn flush_changes(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(previous) = &self.discovered {
            let devices = self.adapter.discover();
            if &devices != previous {
                self.discovered = Some(devices.clone());
                write_message(
                    out,
                    &Message::notification(method::DEVICES_CHANGED, DevicesChanged { devices }),
                )?;
            }
        }
        let mut touched: Vec<String> = Vec::new();
        for change in self.adapter.take_changes() {
            match change {
                Change::ComponentAdded { device, component } => {
                    if !self.devices.contains(&device) {
                        continue;
                    }
                    let id = component.id();
                    self.components.retain(|a| a.component.id() != id);
                    touched.push(device.clone());
                    self.components.push(Attached { device, component });
                }
                Change::ComponentRemoved { component } => {
                    if let Some(i) = self
                        .components
                        .iter()
                        .position(|a| a.component.id() == component)
                    {
                        touched.push(self.components.remove(i).device);
                        self.subscriptions.remove(&component);
                    }
                }
            }
        }
        touched.sort();
        touched.dedup();
        for device in touched {
            let components = self
                .components
                .iter_mut()
                .filter(|a| a.device == device)
                .map(|a| dispatch::describe(&mut a.component))
                .collect();
            let changed = ComponentsChanged { device, components };
            write_message(
                out,
                &Message::notification(method::COMPONENTS_CHANGED, changed),
            )?;
        }
        Ok(())
    }

    fn next_tick(&self) -> Option<Instant> {
        self.subscriptions.values().map(|s| s.next).min()
    }

    fn send_due_states(&mut self, out: &mut impl Write) -> io::Result<()> {
        let now = Instant::now();
        let due: Vec<String> = self
            .subscriptions
            .iter()
            .filter(|(_, s)| s.next <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            if let Some(sub) = self.subscriptions.get_mut(&id) {
                sub.next = now + sub.interval;
            }
            if let Ok(c) = self.component(&id) {
                let values = dispatch::read_inputs(c);
                let state = InputState {
                    component: id,
                    values,
                };
                write_message(out, &Message::notification(method::INPUT_STATE, state))?;
            }
        }
        Ok(())
    }

    fn notify_params_changed(&mut self, id: &str, out: &mut impl Write) -> io::Result<()> {
        if let Ok(c) = self.component(id) {
            let changed = ParamsChanged {
                component: dispatch::describe(c),
            };
            write_message(out, &Message::notification(method::PARAMS_CHANGED, changed))?;
        }
        Ok(())
    }
}

fn find<'a>(
    components: &'a mut [Attached],
    id: &str,
) -> std::result::Result<&'a mut AnyComponent, RpcError> {
    components
        .iter_mut()
        .find(|a| a.component.id() == id)
        .map(|a| &mut a.component)
        .ok_or_else(|| RpcError::new(code::NOT_FOUND, format!("unknown component '{id}'")))
}

fn parse<T: DeserializeOwned>(params: Json) -> std::result::Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::new(code::INVALID_PARAMS, e.to_string()))
}

fn reply(result: impl Serialize) -> Reply {
    serde_json::to_value(result).map_err(|e| RpcError::new(code::INTERNAL, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use simrig_protocol::{AttachResult, read_message};

    struct Base {
        rotation: f64,
    }

    impl Component for Base {
        fn id(&self) -> String {
            "base".into()
        }
        fn name(&self) -> String {
            "Test base".into()
        }
        fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
            let output = host.privileged(
                "save-setting",
                &[("key", item), ("value", &value.to_text())],
            )?;
            assert_eq!(output, "saved");
            Ok(())
        }
    }

    impl WheelBase for Base {
        fn angle(&mut self) -> Support<Degrees> {
            Support::Supported(12.0)
        }
        fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
            match param {
                WheelBaseParam::Rotation => Support::Supported(
                    ParamSpec::float(self.rotation, 90.0, 1080.0, 10.0).persistent(),
                ),
                WheelBaseParam::FfbStrength => {
                    Support::Supported(ParamSpec::float(50.0, 0.0, 100.0, 1.0))
                }
                WheelBaseParam::NaturalInertia => Support::not_in_hardware("No inertia filter."),
                _ => Support::not_in_driver(),
            }
        }
        fn set_param(&mut self, _param: WheelBaseParam, value: &Value) -> Result<()> {
            self.rotation = value.as_f64().unwrap_or(self.rotation);
            Ok(())
        }
    }

    struct TestAdapter;

    impl Adapter for TestAdapter {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                id: "test".into(),
                name: "Test".into(),
                version: "0".into(),
            }
        }
        fn attach(&mut self, _device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
            Ok(vec![AnyComponent::WheelBase(Box::new(Base {
                rotation: 900.0,
            }))])
        }
    }

    fn exchange(requests: Vec<Message>) -> Vec<Message> {
        let (tx, rx) = channel();
        for r in requests {
            tx.send(Ok(Some(r))).unwrap();
        }
        drop(tx);
        let mut out = Vec::new();
        Server::new(TestAdapter).serve(rx, &mut out).unwrap();
        let mut reader = io::Cursor::new(out);
        std::iter::from_fn(|| read_message(&mut reader).unwrap()).collect()
    }

    fn init() -> Message {
        let p = InitializeParams {
            api_version: API_VERSION,
            host_name: "test".into(),
            host_version: "0".into(),
            locale: "en".into(),
        };
        Message::request(1, method::INITIALIZE, p)
    }

    fn attach() -> Message {
        let device = DeviceInfo {
            id: "dev".into(),
            name: "Dev".into(),
            bus: simrig_model::Bus::Virtual,
        };
        Message::request(2, method::ATTACH, AttachParams { device })
    }

    #[test]
    fn describe_reports_reasons() {
        let replies = exchange(vec![init(), attach()]);
        let result: AttachResult =
            serde_json::from_value(replies[1].result.clone().unwrap()).unwrap();
        let base = &result.components[0];
        // every standard item of the kind is listed, supported or not
        assert_eq!(base.items.len(), base.kind.items().len());
        let reason = |id: &str| match &base.item(id).unwrap().support {
            Support::Unsupported(u) => Some(u.reason.clone()),
            Support::Supported(_) => None,
        };
        assert_eq!(reason("rotation"), None);
        assert_eq!(reason("angle"), None);
        assert_eq!(reason("natural_inertia"), Some(Reason::NotInHardware));
        assert_eq!(reason("spring_strength"), Some(Reason::NotInDriver));
        assert_eq!(reason("stop_force"), Some(Reason::NotInDriver));
    }

    #[test]
    fn set_validates_and_notifies() {
        let set = |id, v| {
            Message::request(
                id,
                method::STD_SET,
                SetParams {
                    component: "base".into(),
                    item: "rotation".into(),
                    value: Value::Float(v),
                },
            )
        };
        let replies = exchange(vec![init(), attach(), set(3, 5000.0), set(4, 540.0)]);
        assert_eq!(replies[2].error.as_ref().unwrap().code, code::INVALID_VALUE);
        assert!(replies[3].error.is_none());
        assert_eq!(replies[4].method.as_deref(), Some(method::PARAMS_CHANGED));
        let changed: ParamsChanged =
            serde_json::from_value(replies[4].params.clone().unwrap()).unwrap();
        match &changed.component.item("rotation").unwrap().support {
            Support::Supported(simrig_model::ItemSpec::Param(spec)) => {
                assert_eq!(spec.value, Value::Float(540.0))
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn save_goes_through_the_host() {
        let save = |id, item: &str| {
            Message::request(
                id,
                method::SAVE,
                ItemParams {
                    component: "base".into(),
                    item: item.into(),
                },
            )
        };
        // The host's answer to the plugin's privileged request (the plugin's first request id).
        let granted = Message::response(
            1,
            PrivilegedResult {
                output: "saved".into(),
            },
        );
        let replies = exchange(vec![
            init(),
            attach(),
            save(3, "rotation"),
            granted,
            save(4, "ffb_strength"),
        ]);
        let request = &replies[2];
        assert_eq!(request.method.as_deref(), Some(method::PRIVILEGED));
        let request: PrivilegedRequest =
            serde_json::from_value(request.params.clone().unwrap()).unwrap();
        assert_eq!(request.args["value"], "900");
        assert_eq!(replies[3].id, Some(3));
        assert!(replies[3].error.is_none(), "{:?}", replies[3].error);
        // ffb_strength is volatile: it cannot be saved
        let refused = replies.iter().find(|m| m.id == Some(4)).unwrap();
        assert_eq!(refused.error.as_ref().unwrap().code, code::INVALID_VALUE);
    }

    #[test]
    fn requires_initialize_and_matching_version() {
        let replies = exchange(vec![attach()]);
        assert_eq!(
            replies[0].error.as_ref().unwrap().code,
            code::INVALID_REQUEST
        );
        let mut bad = init();
        bad.params.as_mut().unwrap()["api_version"] = (API_VERSION + 1).into();
        let replies = exchange(vec![bad]);
        assert_eq!(replies[0].error.as_ref().unwrap().code, code::INCOMPATIBLE);
    }
}
