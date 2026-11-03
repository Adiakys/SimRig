//! The host runs on its own thread: plugin calls block (up to the plugin timeout) and must
//! never freeze the interface. The interface sends [`Command`]s; the thread answers with
//! [`Update`]s, delivered on the UI thread by the `deliver` callback.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use simrig_host::{
    ApplyOutcome, DeviceFindings, DriverStatus, DriverUpdate, Host, HostComponent, HostEvent,
    HostOptions, Image, PluginLocation, PluginState, Preset, SystemInfo, Target,
};
use simrig_model::{DeviceInfo, ProcedureState, Value};

/// Live inputs per second, for every component of the rig.
const LIVE_RATE_HZ: f64 = 60.0;
const FRAME: Duration = Duration::from_millis(16);
/// USB hotplug is checked less often than plugins.
const USB_SCAN: Duration = Duration::from_millis(750);
/// What games need is checked again now and then too: access and drivers change with udev, not
/// only with the rig.
const DEVICE_CHECKS: Duration = Duration::from_secs(3);

pub enum Command {
    Set {
        address: String,
        item: String,
        value: Value,
    },
    /// Saves items into the driver, so they are restored at plug-in.
    Save {
        address: String,
        items: Vec<String>,
    },
    RunAction {
        address: String,
        item: String,
    },
    StartProcedure {
        target: Target,
        procedure: String,
    },
    Submit {
        session: String,
        action: String,
        values: BTreeMap<String, Value>,
    },
    Cancel {
        session: String,
    },
    ApplyPreset(String),
    SavePreset(String),
    DeletePreset(String),
    SetExcluded {
        address: String,
        excluded: bool,
    },
    ChoosePlugin {
        device: String,
        plugin: Option<String>,
    },
    InstallDriver(String),
    RemoveDriver(String),
    RestartPlugin(String),
    /// Guided procedures a device offers (e.g. the setup of a generic device).
    ListProcedures(Target),
    /// Ignores a problem games may have with a device, or shows it again.
    SetIgnored {
        device: String,
        key: String,
        ignored: bool,
    },
}

/// The whole rig, sent again whenever something structural changes.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub components: Vec<ComponentView>,
    pub plugins: Vec<PluginView>,
    pub usb: Vec<UsbView>,
    pub presets: Vec<String>,
    pub problems: Vec<String>,
    pub system: Option<SystemInfo>,
    /// What may keep games from using the devices.
    pub findings: Vec<DeviceFindings>,
}

#[derive(Clone, Debug)]
pub struct ComponentView {
    pub component: HostComponent,
    pub image: Image,
    pub plugin_name: String,
    /// The plugin is installed system-wide: saving may ask for the password.
    pub system_plugin: bool,
    /// Its model is not confirmed on real hardware yet.
    pub untested: bool,
}

#[derive(Clone, Debug)]
pub struct PluginView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub location: PluginLocation,
    pub state: PluginState,
    pub devices: Vec<DeviceInfo>,
    pub has_driver: bool,
    pub driver: Option<DriverStatus>,
    /// The plugin ships a newer driver than the one installed.
    pub driver_update: Option<DriverUpdate>,
    /// The driver the plugin ships: its name and version.
    pub driver_name: Option<String>,
    pub driver_version: Option<String>,
    /// Upstream projects of the driver's sources: name and page.
    pub driver_sources: Vec<(String, String)>,
    /// Generic plugins handle a device only when the user assigns it.
    pub on_request: bool,
}

#[derive(Clone, Debug)]
pub struct UsbView {
    pub key: String,
    /// The device id the plugin knows it by, once attached.
    pub device: String,
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub plugin: Option<String>,
    /// Plugins that can handle it, most specific first.
    pub candidates: Vec<String>,
    /// The plugin that handles it when none is chosen.
    pub automatic: Option<String>,
    /// The plugin of its maker, which may take a model it does not list.
    pub maker: Option<String>,
    /// The plugin the user chose for it, if any (otherwise the most specific one handles it).
    pub chosen: Option<String>,
    /// Guided procedures its plugin offers on it (setup, forget…).
    pub procedures: Vec<simrig_model::ProcedureDescriptor>,
}

pub enum Update {
    Snapshot(Box<Snapshot>),
    /// One component changed (a value set, availability of other items…).
    Component(Box<ComponentView>),
    /// Latest inputs of the components that moved since the previous frame.
    Live(Vec<(String, BTreeMap<String, Value>)>),
    Procedure {
        session: String,
        state: ProcedureState,
    },
    ProcedureFailed(String),
    /// A command finished: a short confirmation or an error, for a toast.
    Done {
        ok: bool,
        text: String,
    },
    /// The outcome of applying a preset.
    PresetApplied {
        name: String,
        saved: usize,
        applied: usize,
        ignored: Vec<String>,
    },
    DriverDone {
        plugin: String,
        ok: bool,
        text: String,
    },
    Procedures {
        target: Target,
        procedures: Vec<simrig_model::ProcedureDescriptor>,
    },
    /// What games need changed (see [`Snapshot::findings`]).
    Findings(Vec<DeviceFindings>),
}

pub struct Backend {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Backend {
    pub fn start(options: HostOptions, deliver: impl Fn(Update) + Send + 'static) -> Self {
        let (commands, rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("simrig-host".into())
            .spawn(move || run(options, rx, deliver))
            .expect("start the host thread");
        Self {
            commands,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        // The thread only stops when the backend is dropped.
        let _ = self.commands.send(command);
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Closing the channel stops the thread, which stops every adapter: the app closed
        // leaves nothing running.
        let (dead, _) = mpsc::channel();
        self.commands = dead;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What must be sent again after commands and events.
#[derive(Default)]
struct Dirty {
    rig: bool,
    /// Driver states are read by running hooks: only after driver or plugin changes.
    drivers: bool,
    components: BTreeSet<String>,
    /// Check what games need again now (an ignored problem changed).
    games: bool,
}

fn run(options: HostOptions, rx: Receiver<Command>, deliver: impl Fn(Update)) {
    let mut host = Host::start(&options);
    let mut subscribed: HashSet<String> = HashSet::new();
    let mut drivers: Drivers = BTreeMap::new();
    let system = host.system();
    resubscribe(&mut host, &mut subscribed);
    refresh_drivers(&host, &mut drivers);
    let full = |host: &mut Host, drivers: &Drivers, findings: &mut Vec<DeviceFindings>| {
        let mut s = snapshot(host, drivers);
        s.system = Some(system.clone());
        *findings = host.device_checks(&statuses(drivers));
        s.findings = findings.clone();
        Box::new(s)
    };
    let mut findings = Vec::new();
    deliver(Update::Snapshot(full(&mut host, &drivers, &mut findings)));

    let mut live: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut last_frame = Instant::now();
    let mut last_scan = Instant::now();
    let mut last_check = Instant::now();
    loop {
        let wait = FRAME.saturating_sub(last_frame.elapsed());
        let mut dirty = Dirty::default();
        match rx.recv_timeout(wait) {
            Ok(command) => {
                // Coalesce: a slider sends many values, only the latest of each item matters.
                let mut batch = vec![command];
                batch.extend(rx.try_iter());
                let batch = coalesce(batch);
                for command in batch {
                    execute(&mut host, command, &deliver, &mut dirty);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let events = if last_scan.elapsed() >= USB_SCAN {
            last_scan = Instant::now();
            host.poll()
        } else {
            host.poll_plugins()
        };
        for event in events {
            match event {
                HostEvent::Inputs { address, values } => {
                    live.entry(address).or_default().extend(values);
                }
                HostEvent::ProcedureUpdated { session, state } => {
                    deliver(Update::Procedure { session, state });
                }
                HostEvent::DescriptionChanged { address } => {
                    dirty.components.insert(address);
                }
                HostEvent::PluginStarted { .. }
                | HostEvent::PluginStopped { .. }
                | HostEvent::PluginFailed { .. } => {
                    dirty.rig = true;
                    dirty.drivers = true;
                }
                _ => dirty.rig = true,
            }
        }
        if dirty.drivers {
            refresh_drivers(&host, &mut drivers);
        }
        if dirty.rig || dirty.drivers {
            resubscribe(&mut host, &mut subscribed);
            deliver(Update::Snapshot(full(&mut host, &drivers, &mut findings)));
            last_check = Instant::now();
        } else {
            for address in dirty.components {
                if let Some(view) = component_view(&mut host, &address) {
                    deliver(Update::Component(Box::new(view)));
                }
            }
            if dirty.games || last_check.elapsed() >= DEVICE_CHECKS {
                last_check = Instant::now();
                let now = host.device_checks(&statuses(&drivers));
                if now != findings {
                    findings = now;
                    deliver(Update::Findings(findings.clone()));
                }
            }
        }
        if last_frame.elapsed() >= FRAME {
            last_frame = Instant::now();
            if !live.is_empty() {
                deliver(Update::Live(
                    std::mem::take(&mut live).into_iter().collect(),
                ));
            }
        }
    }
    host.shutdown();
}

/// Keeps only the last `Set` of each item, in order.
fn coalesce(batch: Vec<Command>) -> Vec<Command> {
    let mut last: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (i, command) in batch.iter().enumerate() {
        if let Command::Set { address, item, .. } = command {
            last.insert((address.clone(), item.clone()), i);
        }
    }
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, command)| match command {
            Command::Set { address, item, .. } => {
                last.get(&(address.clone(), item.clone())) == Some(i)
            }
            _ => true,
        })
        .map(|(_, command)| command)
        .collect()
}

/// Subscribes to the live inputs of every component not subscribed yet.
fn resubscribe(host: &mut Host, subscribed: &mut HashSet<String>) {
    let addresses: Vec<String> = host
        .components()
        .iter()
        .map(|c| c.address.clone())
        .collect();
    subscribed.retain(|a| addresses.contains(a));
    for address in addresses {
        if !subscribed.contains(&address) && host.subscribe(&address, LIVE_RATE_HZ).is_ok() {
            subscribed.insert(address);
        }
    }
}

/// What the app shows of a plugin's driver.
#[derive(Clone, Debug, Default)]
struct DriverView {
    status: Option<DriverStatus>,
    update: Option<DriverUpdate>,
    /// The driver shipped: its name and version.
    name: Option<String>,
    version: Option<String>,
    sources: Vec<(String, String)>,
}

type Drivers = BTreeMap<String, DriverView>;

fn statuses(drivers: &Drivers) -> BTreeMap<String, Option<DriverStatus>> {
    drivers
        .iter()
        .map(|(id, d)| (id.clone(), d.status.clone()))
        .collect()
}

fn refresh_drivers(host: &Host, drivers: &mut Drivers) {
    drivers.clear();
    for p in host.plugins() {
        let id = &p.manifest.id;
        let contract = host.driver_contract(id).ok().flatten();
        drivers.insert(
            id.clone(),
            DriverView {
                status: host.driver_status(id).ok().flatten(),
                update: host.driver_update(id).ok().flatten(),
                name: contract
                    .as_ref()
                    .map(|c| c.name.clone().unwrap_or(c.id.clone())),
                sources: contract
                    .as_ref()
                    .map(|c| {
                        c.sources
                            .iter()
                            .map(|s| (s.name.clone(), s.url.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                version: contract.map(|c| simrig_host::version_text(&c.version, c.revision)),
            },
        );
    }
}

/// A component with a fresh description (values may have changed in the plugin).
fn component_view(host: &mut Host, address: &str) -> Option<ComponentView> {
    let _ = host.describe(address);
    let component = host.find(address).ok()?.clone();
    let plugin = host
        .plugins()
        .iter()
        .find(|p| p.manifest.id == component.plugin);
    let plugin_name = plugin
        .map(|p| p.manifest.name.clone())
        .unwrap_or_else(|| component.plugin.clone());
    let system_plugin = plugin.is_some_and(|p| p.location == PluginLocation::System);
    let untested =
        plugin.is_some_and(|p| p.manifest.untested(component.description.model.as_deref()));
    let image = host.image(address).unwrap_or(Image::Generic {
        kind: component.description.kind,
        rim: component.description.appearance.rim,
    });
    Some(ComponentView {
        component,
        image,
        plugin_name,
        system_plugin,
        untested,
    })
}

fn snapshot(host: &mut Host, drivers: &Drivers) -> Snapshot {
    let addresses: Vec<String> = host
        .components()
        .iter()
        .map(|c| c.address.clone())
        .collect();
    let components = addresses
        .iter()
        .filter_map(|a| component_view(host, a))
        .collect();

    let mut plugins = Vec::new();
    for p in host.plugins() {
        let id = p.manifest.id.clone();
        let view = drivers.get(&id).cloned().unwrap_or_default();
        plugins.push(PluginView {
            id: id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            location: p.location,
            state: p.state.clone(),
            devices: p.devices.clone(),
            has_driver: p.manifest.driver.is_some(),
            driver: view.status,
            driver_update: view.update,
            driver_name: view.name,
            driver_version: view.version,
            driver_sources: view.sources,
            on_request: p.manifest.matching.on_request,
        });
    }

    let mut usb = Vec::new();
    for (device, plugin) in host.usb_devices() {
        let keys = device.keys();
        let chosen = keys
            .iter()
            .find_map(|k| host.rig_config().device_plugins.get(k).cloned());
        let id = device.device_info().id;
        let procedures = match &plugin {
            Some(p) => host
                .procedures(&Target::Device {
                    plugin: p.clone(),
                    device: id.clone(),
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };
        usb.push(UsbView {
            key: keys.into_iter().next().unwrap_or_default(),
            device: id,
            name: device.name.clone(),
            vid: device.vid,
            pid: device.pid,
            candidates: host.candidate_plugins(&device),
            automatic: host.automatic_plugin(&device),
            maker: host.maker_plugin(&device),
            chosen,
            procedures,
            plugin,
        });
    }

    let presets = host
        .presets()
        .list()
        .map(|list| list.into_iter().map(|p| p.name).collect())
        .unwrap_or_default();

    Snapshot {
        components,
        plugins,
        usb,
        presets,
        problems: host.problems().to_vec(),
        system: None,
        findings: Vec::new(),
    }
}

/// Runs a command and records what must be sent again.
fn execute(host: &mut Host, command: Command, deliver: &impl Fn(Update), dirty: &mut Dirty) {
    let fail = |text: String| deliver(Update::Done { ok: false, text });
    let rig = match command {
        Command::Set {
            address,
            item,
            value,
        } => {
            if let Err(e) = host.set(&address, &item, value) {
                fail(e.to_string());
            }
            dirty.components.insert(address);
            false
        }
        Command::Save { address, items } => {
            let mut errors = Vec::new();
            for item in &items {
                if let Err(e) = host.save(&address, item) {
                    errors.push(e.to_string());
                }
            }
            match errors.first() {
                None => deliver(Update::Done {
                    ok: true,
                    text: "Saved in the driver: restored every time the device is plugged in."
                        .into(),
                }),
                Some(e) => fail(e.clone()),
            }
            dirty.components.insert(address);
            false
        }
        Command::RunAction { address, item } => {
            match host.run_action(&address, &item, Value::Empty) {
                Ok(outcome) => {
                    if let Some(text) = outcome.message {
                        deliver(Update::Done { ok: true, text });
                    }
                }
                Err(e) => fail(e.to_string()),
            }
            dirty.components.insert(address);
            false
        }
        Command::StartProcedure { target, procedure } => {
            match host.start_procedure(&target, &procedure) {
                Ok((session, state)) => deliver(Update::Procedure { session, state }),
                Err(e) => deliver(Update::ProcedureFailed(e.to_string())),
            }
            false
        }
        Command::Submit {
            session,
            action,
            values,
        } => {
            match host.submit_procedure(&session, &action, values) {
                Ok(state) => {
                    // A finished procedure may have changed the rig (a device set up).
                    let finished = !matches!(state, ProcedureState::Step(_));
                    deliver(Update::Procedure { session, state });
                    finished
                }
                Err(e) => {
                    deliver(Update::ProcedureFailed(e.to_string()));
                    false
                }
            }
        }
        Command::Cancel { session } => {
            let _ = host.cancel_procedure(&session);
            deliver(Update::Procedure {
                session,
                state: ProcedureState::Cancelled,
            });
            true
        }
        Command::ApplyPreset(name) => {
            match host.presets().get(&name) {
                Ok(preset) => {
                    let entries = host.apply_preset(&preset);
                    let mut saved = 0;
                    let mut applied = 0;
                    let mut ignored = Vec::new();
                    for e in entries {
                        match e.outcome {
                            ApplyOutcome::Saved => saved += 1,
                            ApplyOutcome::Applied { .. } => applied += 1,
                            ApplyOutcome::Ignored(why) => {
                                ignored.push(format!("{} {}: {why}", e.target, e.item))
                            }
                        }
                    }
                    deliver(Update::PresetApplied {
                        name,
                        saved,
                        applied,
                        ignored,
                    });
                }
                Err(e) => fail(e.to_string()),
            }
            true
        }
        Command::SavePreset(name) => {
            match host
                .capture_preset(&name)
                .and_then(|p: Preset| host.presets().save(&p))
            {
                Ok(_) => deliver(Update::Done {
                    ok: true,
                    text: format!("Preset \"{name}\" saved."),
                }),
                Err(e) => fail(e.to_string()),
            }
            true
        }
        Command::DeletePreset(name) => {
            if let Err(e) = host.presets().delete(&name) {
                fail(e.to_string());
            }
            true
        }
        Command::SetExcluded { address, excluded } => {
            if let Err(e) = host.set_excluded(&address, excluded) {
                fail(e.to_string());
            }
            true
        }
        Command::ChoosePlugin { device, plugin } => {
            if let Err(e) = host.choose_plugin(&device, plugin.as_deref()) {
                fail(e.to_string());
            }
            true
        }
        Command::InstallDriver(plugin) => {
            let update = host.driver_update(&plugin).ok().flatten().is_some();
            let result = host.install_driver(&plugin);
            let done = if update {
                "Driver updated."
            } else {
                "Driver installed."
            };
            deliver(driver_done(plugin, result, done));
            dirty.drivers = true;
            true
        }
        Command::RemoveDriver(plugin) => {
            let result = host.uninstall_driver(&plugin);
            deliver(driver_done(plugin, result, "Driver removed."));
            dirty.drivers = true;
            true
        }
        Command::ListProcedures(target) => {
            let procedures = host.procedures(&target).unwrap_or_default();
            deliver(Update::Procedures { target, procedures });
            false
        }
        Command::SetIgnored {
            device,
            key,
            ignored,
        } => {
            if let Err(e) = host.set_ignored(&device, &key, ignored) {
                fail(e.to_string());
            }
            dirty.games = true;
            false
        }
        Command::RestartPlugin(plugin) => {
            if let Err(e) = host.restart_plugin(&plugin) {
                fail(e.to_string());
            }
            dirty.drivers = true;
            true
        }
    };
    dirty.rig |= rig;
}

fn driver_done(
    plugin: String,
    result: Result<String, simrig_host::HostError>,
    success: &str,
) -> Update {
    match result {
        Ok(_) => Update::DriverDone {
            plugin,
            ok: true,
            text: success.into(),
        },
        Err(e) => Update::DriverDone {
            plugin,
            ok: false,
            text: e.to_string(),
        },
    }
}
