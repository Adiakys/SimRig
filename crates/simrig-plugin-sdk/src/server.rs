//! The adapter side of the protocol: reads requests from stdin, answers on stdout.

use std::collections::HashMap;
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
    API_VERSION, ActionParams, AttachParams, AttachResult, ComponentParams, DetachParams,
    DiscoverResult, InitializeParams, InitializeResult, InputState, ItemParams, Message,
    ParamsChanged, RpcError, SetParams, SubscribeParams, code, method, read_message, write_message,
};

use crate::Adapter;
use crate::dispatch;
use crate::interfaces::AnyComponent;

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
    components: Vec<Attached>,
    subscriptions: HashMap<String, Subscription>,
    initialized: bool,
}

type Reply = Result<Json, RpcError>;

/// Incoming message, or a read error description. `Ok(None)` is end of stream.
pub type Incoming = Result<Option<Message>, String>;

impl<A: Adapter> Server<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            components: Vec::new(),
            subscriptions: HashMap::new(),
            initialized: false,
        }
    }

    /// Serves messages from `rx` until shutdown or end of stream.
    pub fn serve(&mut self, rx: Receiver<Incoming>, out: &mut impl Write) -> io::Result<()> {
        loop {
            let incoming = match self.next_tick() {
                Some(deadline) => {
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(m) => Some(m),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
                None => match rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => return Ok(()),
                },
            };
            match incoming {
                None => self.send_due_states(out)?,
                Some(Ok(None)) => return Ok(()),
                Some(Err(e)) => {
                    let error = RpcError::new(code::PARSE_ERROR, e);
                    write_message(out, &Message::error_response(None, error))?;
                }
                Some(Ok(Some(message))) => {
                    if !self.handle(message, out)? {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Handles one message; returns `false` after `shutdown`.
    fn handle(&mut self, message: Message, out: &mut impl Write) -> io::Result<bool> {
        let Some(name) = message.method.clone() else {
            return Ok(true); // the host does not answer us; ignore stray responses
        };
        let params = message.params.unwrap_or(Json::Null);
        let component = params
            .get("component")
            .and_then(Json::as_str)
            .map(str::to_owned);
        let reply = self.dispatch(&name, params);
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
            && [method::STD_SET, method::CUSTOM_SET, method::ACTION_RUN].contains(&name.as_str())
        {
            if let Some(id) = component {
                self.notify_params_changed(&id, out)?;
            }
        }
        Ok(true)
    }

    fn dispatch(&mut self, name: &str, params: Json) -> Reply {
        if !self.initialized && name != method::INITIALIZE {
            return Err(RpcError::new(code::INVALID_REQUEST, "not initialized"));
        }
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
            method::DISCOVER => reply(DiscoverResult {
                devices: self.adapter.discover(),
            }),
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
                reply(dispatch::get_param(c, &p.item).map_err(|e| e.to_rpc())?)
            }
            method::STD_SET => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::set_param(c, &p.item, &p.value).map_err(|e| e.to_rpc())?;
                reply(())
            }
            method::CUSTOM_SET => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::set_custom(c, &p.item, &p.value).map_err(|e| e.to_rpc())?;
                reply(())
            }
            method::ACTION_RUN => {
                let p: ActionParams = parse(params)?;
                let c = self.component(&p.component)?;
                reply(dispatch::run_action(c, &p.item, &p.args).map_err(|e| e.to_rpc())?)
            }
            method::OUTPUT_WRITE => {
                let p: SetParams = parse(params)?;
                let c = self.component(&p.component)?;
                dispatch::write_output(c, &p.item, &p.value).map_err(|e| e.to_rpc())?;
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
        for c in &components {
            let id = c.id();
            if self.components.iter().any(|a| a.component.id() == id) {
                let msg = format!("duplicate component id '{id}'");
                return Err(RpcError::new(code::INTERNAL, msg));
            }
        }
        let descriptions = components.iter_mut().map(dispatch::describe).collect();
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
        let before = self.components.len();
        self.components.retain(|a| a.device != device);
        if self.components.len() != before {
            self.adapter.detach(device);
        }
        let live: Vec<String> = self.components.iter().map(|a| a.component.id()).collect();
        self.subscriptions.retain(|id, _| live.contains(id));
    }

    fn component(&mut self, id: &str) -> Result<&mut AnyComponent, RpcError> {
        self.components
            .iter_mut()
            .find(|a| a.component.id() == id)
            .map(|a| &mut a.component)
            .ok_or_else(|| RpcError::new(code::NOT_FOUND, format!("unknown component '{id}'")))
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

fn parse<T: DeserializeOwned>(params: Json) -> Result<T, RpcError> {
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
    }

    impl WheelBase for Base {
        fn angle(&mut self) -> Support<Degrees> {
            Support::Supported(12.0)
        }
        fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
            match param {
                WheelBaseParam::Rotation => {
                    Support::Supported(ParamSpec::float(self.rotation, 90.0, 1080.0, 10.0))
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
        assert_eq!(reason("ffb_strength"), Some(Reason::NotInDriver));
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
