use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use simrig_model::{
    ActionOutcome, Bus, CUSTOM_PREFIX, ComponentDescription, DeviceInfo, ItemSpec, ParamSpec,
    ParamType, Persistence, ProcedureDescriptor, ProcedureState, ProcedureTarget, Reason, Support,
    Value,
};
use simrig_protocol::{
    API_VERSION, ActionParams, AttachParams, AttachResult, ComponentParams, ComponentsChanged,
    DetachParams, DeviceParams, DevicesChanged, DiscoverResult, InitializeParams, InitializeResult,
    InputState, ItemParams, Message, ParamsChanged, ProcedureCancelParams, ProcedureSession,
    ProcedureStartParams, ProcedureSubmitParams, ProceduresResult, SetParams, SubscribeParams,
    method,
};

use crate::catalog::{Catalog, Image};
use crate::conformance;
use crate::drivers::{DriverManager, find_helper};
use crate::error::HostError;
use crate::manifest::{self, Manifest, PluginLocation};
use crate::paths;
use crate::presets::{ApplyEntry, ApplyOutcome, Preset, PresetStore, from_toml, to_toml};
use crate::process::{PluginProcess, Spawn};
use crate::rig::RigConfig;
use crate::system::SystemInfo;
use crate::usb::{self, UsbDevice};
use simrig_driver::{DriverStatus, InstalledDriver};

/// Events kept for [`Host::poll`] when nobody polls (e.g. the CLI); older ones are dropped.
const MAX_EVENTS: usize = 10_000;

#[derive(Clone, Debug)]
pub struct HostOptions {
    /// Extra plugin directories, searched before the default ones.
    pub plugin_dirs: Vec<PathBuf>,
    /// Also search the user and system plugin directories (and `SIMRIG_PLUGIN_PATH`).
    pub default_dirs: bool,
    /// Extra directories where adapter executables may live (development builds).
    pub exec_dirs: Vec<PathBuf>,
    pub locale: String,
    /// How long to wait for a plugin to answer a request.
    pub timeout: Duration,
    /// USB devices in sysfs (tests use a fake tree).
    pub usb_root: PathBuf,
    /// Rig composition and presets.
    pub config_dir: PathBuf,
    /// User plugins and images.
    pub data_dir: PathBuf,
    /// Installed drivers of development plugins (system plugins use `/var/lib/simrig`).
    pub dev_state_dir: PathBuf,
    /// `simrig-helper`; found next to the executable or in `/usr/libexec/simrig` by default.
    pub helper: Option<PathBuf>,
}

impl Default for HostOptions {
    fn default() -> Self {
        let fallback = || std::env::temp_dir().join("simrig");
        Self {
            plugin_dirs: Vec::new(),
            default_dirs: true,
            exec_dirs: Vec::new(),
            locale: "en".into(),
            timeout: Duration::from_secs(5),
            usb_root: PathBuf::from(usb::DEFAULT_ROOT),
            config_dir: paths::config_dir().unwrap_or_else(fallback),
            data_dir: paths::data_dir().unwrap_or_else(fallback),
            dev_state_dir: paths::state_dir().unwrap_or_else(fallback).join("dev"),
            helper: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginState {
    Running,
    /// No matching device connected: the adapter is not started.
    Idle,
    Failed(String),
}

pub struct Plugin {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub location: PluginLocation,
    pub state: PluginState,
    pub info: Option<simrig_protocol::PluginInfo>,
    /// Devices attached to the adapter.
    pub devices: Vec<DeviceInfo>,
    process: Option<PluginProcess>,
}

impl Plugin {
    /// Last lines the adapter wrote on stderr.
    pub fn log(&self) -> Vec<String> {
        self.process
            .as_ref()
            .map(PluginProcess::stderr_tail)
            .unwrap_or_default()
    }

    fn call<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &mut self,
        name: &str,
        params: P,
    ) -> Result<R, HostError> {
        self.process
            .as_mut()
            .ok_or_else(|| HostError::PluginNotRunning(self.manifest.id.clone()))?
            .call(name, params)
    }
}

/// A component of the rig, addressed as `<plugin>/<component id>`.
#[derive(Clone, Debug)]
pub struct HostComponent {
    pub address: String,
    pub plugin: String,
    pub device: DeviceInfo,
    pub description: ComponentDescription,
    /// Excluded from the rig by the user (still listed, ignored by presets).
    pub excluded: bool,
}

/// What a procedure runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A component, by address.
    Component(String),
    /// A device of a plugin, e.g. one that has no components until it is set up.
    Device { plugin: String, device: String },
}

/// Something that changed in the rig, reported by [`Host::poll`].
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    PluginStarted {
        plugin: String,
    },
    /// No more devices for the plugin: its adapter was stopped.
    PluginStopped {
        plugin: String,
    },
    PluginFailed {
        plugin: String,
        error: String,
    },
    DeviceAdded {
        plugin: String,
        device: DeviceInfo,
    },
    DeviceRemoved {
        plugin: String,
        device: String,
    },
    /// The components of a device changed (e.g. a rim swap).
    ComponentsChanged {
        plugin: String,
        device: String,
    },
    /// The description of a component changed (values, availability, custom parameters).
    DescriptionChanged {
        address: String,
    },
    /// A running procedure moved on by itself (live value, detected input, finished…).
    ProcedureUpdated {
        session: String,
        state: ProcedureState,
    },
    /// Live inputs of a subscribed component.
    Inputs {
        address: String,
        values: BTreeMap<String, Value>,
    },
}

pub struct Host {
    options: HostOptions,
    plugins: Vec<Plugin>,
    components: Vec<HostComponent>,
    problems: Vec<String>,
    events: Vec<HostEvent>,
    usb: Vec<UsbDevice>,
    rig: RigConfig,
    catalog: Catalog,
    drivers: DriverManager,
    presets: PresetStore,
}

impl Host {
    /// Discovers plugins, starts those with a matching device and attaches the devices.
    /// Problems (bad manifests, failing plugins, conformance violations) are collected in
    /// [`Host::problems`] instead of aborting.
    pub fn start(options: &HostOptions) -> Self {
        let mut dirs: Vec<(PathBuf, PluginLocation)> = options
            .plugin_dirs
            .iter()
            .map(|d| (d.clone(), PluginLocation::Explicit))
            .collect();
        if options.default_dirs {
            if let Some(path) = std::env::var_os("SIMRIG_PLUGIN_PATH") {
                dirs.extend(std::env::split_paths(&path).map(|d| (d, PluginLocation::Explicit)));
            }
            dirs.extend(manifest::default_dirs());
        }
        let dev_roots: Vec<PathBuf> = dirs
            .iter()
            .filter(|(_, l)| *l == PluginLocation::Explicit)
            .map(|(d, _)| d.clone())
            .collect();
        let (found, mut problems) = manifest::discover(&dirs);

        let rig = RigConfig::load(&options.config_dir).unwrap_or_else(|e| {
            problems.push(e.to_string());
            RigConfig::default()
        });
        let mut catalog = Catalog::default();
        for f in &found {
            problems.extend(catalog.load_plugin(&f.manifest.id, &f.dir));
        }
        let helper = options.helper.clone().or_else(find_helper);
        let mut host = Host {
            options: options.clone(),
            plugins: found
                .into_iter()
                .map(|f| Plugin {
                    manifest: f.manifest,
                    dir: f.dir,
                    location: f.location,
                    state: PluginState::Idle,
                    info: None,
                    devices: Vec::new(),
                    process: None,
                })
                .collect(),
            components: Vec::new(),
            problems,
            events: Vec::new(),
            usb: Vec::new(),
            rig,
            catalog,
            drivers: DriverManager::new(helper, options.dev_state_dir.clone(), dev_roots),
            presets: PresetStore::new(&options.config_dir),
        };
        let usb = usb::scan_dir(&options.usb_root);
        let assigned = host.assign(&usb);
        host.usb = usb;
        for (i, devices) in assigned.into_iter().enumerate() {
            if !devices.is_empty() || host.plugins[i].manifest.matching.virtual_devices {
                host.start_plugin(i, devices.iter().map(UsbDevice::device_info).collect());
            }
        }
        // The initial rig is the starting state, not a change.
        host.events.clear();
        host
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    pub fn components(&self) -> &[HostComponent] {
        &self.components
    }

    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    pub fn options(&self) -> &HostOptions {
        &self.options
    }

    // ---------------------------------------------------------------- plugin lifecycle

    fn problem(&mut self, problem: String) {
        if !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }

    fn event(&mut self, event: HostEvent) {
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    fn assign(&mut self, devices: &[UsbDevice]) -> Vec<Vec<UsbDevice>> {
        let manifests: Vec<&Manifest> = self.plugins.iter().map(|p| &p.manifest).collect();
        let (assigned, problems) =
            manifest::assign_usb(&manifests, devices, &self.rig.device_plugins);
        for p in problems {
            self.problem(p);
        }
        assigned
    }

    fn index(&self, plugin: &str) -> Result<usize, HostError> {
        self.plugins
            .iter()
            .position(|p| p.manifest.id == plugin)
            .ok_or_else(|| HostError::NotFound(format!("no plugin '{plugin}'")))
    }

    fn start_plugin(&mut self, i: usize, devices: Vec<DeviceInfo>) {
        let id = self.plugins[i].manifest.id.clone();
        match self.try_start(i, devices) {
            Ok(()) => self.event(HostEvent::PluginStarted { plugin: id }),
            Err(e) => {
                self.problem(format!("plugin '{id}': {e}"));
                self.fail_plugin(i, e.to_string());
            }
        }
    }

    fn try_start(&mut self, i: usize, devices: Vec<DeviceInfo>) -> Result<(), HostError> {
        let plugin = &self.plugins[i];
        let id = plugin.manifest.id.clone();
        if plugin.manifest.api != API_VERSION {
            return Err(HostError::NotFound(format!(
                "manifest declares API {}, this app speaks API {API_VERSION}",
                plugin.manifest.api
            )));
        }
        let exec = find_exec(&plugin.manifest.exec, &plugin.dir, &self.options.exec_dirs)
            .ok_or_else(|| {
                HostError::NotFound(format!("adapter '{}' not found", plugin.manifest.exec))
            })?;
        let has_driver = plugin.manifest.driver.is_some();
        let mut env = Vec::new();
        if let (true, Some(layout)) = (has_driver, self.drivers.layout(plugin.location)) {
            env.push((
                "SIMRIG_DRIVER_DATA".to_string(),
                layout.data_dir(&id).into_os_string(),
            ));
        }
        let privileged = if has_driver {
            self.drivers.runner(&id, plugin.location).ok()
        } else {
            None
        };
        let process = PluginProcess::spawn(Spawn {
            plugin: &id,
            exec: &exec,
            args: &plugin.manifest.args,
            dir: &plugin.dir,
            env,
            timeout: self.options.timeout,
            privileged,
        })?;

        let plugin = &mut self.plugins[i];
        plugin.process = Some(process);
        let init: InitializeResult = plugin.call(
            method::INITIALIZE,
            InitializeParams {
                api_version: API_VERSION,
                host_name: "simrig".into(),
                host_version: env!("CARGO_PKG_VERSION").into(),
                locale: self.options.locale.clone(),
            },
        )?;
        if init.plugin.id != id {
            let reported = init.plugin.id.clone();
            self.problem(format!("plugin '{id}' reports id '{reported}'"));
        }
        let plugin = &mut self.plugins[i];
        plugin.info = Some(init.plugin);
        let mut devices = devices;
        if plugin.manifest.matching.virtual_devices {
            let discovered: DiscoverResult = plugin.call(method::DISCOVER, ())?;
            devices.extend(discovered.devices);
        }
        plugin.state = PluginState::Running;
        for device in devices {
            self.attach(i, device);
        }
        Ok(())
    }

    fn fail_plugin(&mut self, i: usize, error: String) {
        let id = self.plugins[i].manifest.id.clone();
        if let Some(process) = self.plugins[i].process.take() {
            process.shutdown();
        }
        self.plugins[i].state = PluginState::Failed(error.clone());
        self.plugins[i].devices.clear();
        self.components.retain(|c| c.plugin != id);
        self.event(HostEvent::PluginFailed { plugin: id, error });
    }

    fn stop_plugin(&mut self, i: usize) {
        let id = self.plugins[i].manifest.id.clone();
        if let Some(process) = self.plugins[i].process.take() {
            process.shutdown();
        }
        self.plugins[i].state = PluginState::Idle;
        self.plugins[i].devices.clear();
        self.components.retain(|c| c.plugin != id);
        self.event(HostEvent::PluginStopped { plugin: id });
    }

    /// Restarts a plugin's adapter (e.g. after a failure or a driver install).
    pub fn restart_plugin(&mut self, plugin: &str) -> Result<(), HostError> {
        let i = self.index(plugin)?;
        let usb_devices: Vec<DeviceInfo> = match self.plugins[i].state {
            PluginState::Running => self.plugins[i]
                .devices
                .iter()
                .filter(|d| matches!(d.bus, Bus::Usb { .. }))
                .cloned()
                .collect(),
            _ => {
                let usb = self.usb.clone();
                self.assign(&usb)[i]
                    .iter()
                    .map(UsbDevice::device_info)
                    .collect()
            }
        };
        if self.plugins[i].process.is_some() {
            self.stop_plugin(i);
        }
        if !usb_devices.is_empty() || self.plugins[i].manifest.matching.virtual_devices {
            self.start_plugin(i, usb_devices);
        }
        match &self.plugins[i].state {
            PluginState::Failed(e) => Err(HostError::Driver(e.clone())),
            _ => Ok(()),
        }
    }

    fn attach(&mut self, i: usize, device: DeviceInfo) {
        let id = self.plugins[i].manifest.id.clone();
        let result: Result<AttachResult, _> = self.plugins[i].call(
            method::ATTACH,
            AttachParams {
                device: device.clone(),
            },
        );
        match result {
            Ok(result) => {
                self.plugins[i].devices.retain(|d| d.id != device.id);
                self.plugins[i].devices.push(device.clone());
                self.add_components(&id, &device, result.components);
                self.event(HostEvent::DeviceAdded { plugin: id, device });
            }
            Err(e) => self.problem(format!("plugin '{id}': cannot attach {}: {e}", device.name)),
        }
    }

    fn detach(&mut self, i: usize, device: &str) {
        let id = self.plugins[i].manifest.id.clone();
        let _: Result<(), _> = self.plugins[i].call(
            method::DETACH,
            DetachParams {
                device: device.into(),
            },
        );
        self.plugins[i].devices.retain(|d| d.id != device);
        self.components
            .retain(|c| !(c.plugin == id && c.device.id == device));
        self.event(HostEvent::DeviceRemoved {
            plugin: id,
            device: device.into(),
        });
    }

    fn add_components(
        &mut self,
        plugin: &str,
        device: &DeviceInfo,
        descriptions: Vec<ComponentDescription>,
    ) {
        for description in descriptions {
            for p in conformance::check(&description) {
                self.problem(format!("plugin '{plugin}': {p}"));
            }
            let address = format!("{plugin}/{}", description.id);
            self.components.retain(|c| c.address != address);
            self.components.push(HostComponent {
                excluded: self.rig.excluded.contains(&address),
                address,
                plugin: plugin.into(),
                device: device.clone(),
                description,
            });
        }
    }

    // ---------------------------------------------------------------- changes

    /// Applies what changed since the last call: adapters that crashed, devices plugged or
    /// unplugged, rim swaps, virtual devices, descriptions and live inputs. Call it regularly
    /// (the GUI does it on a timer).
    pub fn poll(&mut self) -> Vec<HostEvent> {
        for i in 0..self.plugins.len() {
            let exited = self.plugins[i]
                .process
                .as_mut()
                .is_some_and(PluginProcess::has_exited);
            if exited {
                let tail = self.plugins[i].log().last().cloned().unwrap_or_default();
                self.fail_plugin(i, format!("adapter exited {tail}").trim().to_string());
            }
        }
        for i in 0..self.plugins.len() {
            let pending = self.plugins[i]
                .process
                .as_ref()
                .map(PluginProcess::pending_notifications)
                .unwrap_or_default();
            for message in pending {
                self.handle_notification(i, message);
            }
        }
        self.rescan_usb();
        std::mem::take(&mut self.events)
    }

    fn rescan_usb(&mut self) {
        let current = usb::scan_dir(&self.options.usb_root);
        let removed: Vec<UsbDevice> = self
            .usb
            .iter()
            .filter(|d| !current.contains(d))
            .cloned()
            .collect();
        let added: Vec<UsbDevice> = current
            .iter()
            .filter(|d| !self.usb.contains(d))
            .cloned()
            .collect();
        self.usb = current;
        for device in removed {
            let info = device.device_info();
            let owner = self.plugins.iter().position(|p| {
                p.devices
                    .iter()
                    .any(|d| d.id == info.id && d.bus == info.bus)
            });
            if let Some(i) = owner {
                self.detach(i, &info.id);
                if self.plugins[i].devices.is_empty()
                    && !self.plugins[i].manifest.matching.virtual_devices
                {
                    self.stop_plugin(i);
                }
            }
        }
        if added.is_empty() {
            return;
        }
        for (i, devices) in self.assign(&added).into_iter().enumerate() {
            if devices.is_empty() {
                continue;
            }
            let devices: Vec<DeviceInfo> = devices.iter().map(UsbDevice::device_info).collect();
            if self.plugins[i].state == PluginState::Running {
                for device in devices {
                    self.attach(i, device);
                }
            } else {
                self.start_plugin(i, devices);
            }
        }
    }

    fn handle_notification(&mut self, i: usize, message: Message) {
        let plugin = self.plugins[i].manifest.id.clone();
        match message.method.as_deref() {
            Some(method::PARAMS_CHANGED) => {
                if let Some(changed) = parse::<ParamsChanged>(&message) {
                    let address = format!("{plugin}/{}", changed.component.id);
                    self.store_description(&plugin, changed.component);
                    self.event(HostEvent::DescriptionChanged { address });
                }
            }
            Some(method::COMPONENTS_CHANGED) => {
                if let Some(changed) = parse::<ComponentsChanged>(&message) {
                    let Some(device) = self.plugins[i]
                        .devices
                        .iter()
                        .find(|d| d.id == changed.device)
                        .cloned()
                    else {
                        return;
                    };
                    self.components
                        .retain(|c| !(c.plugin == plugin && c.device.id == device.id));
                    self.add_components(&plugin, &device, changed.components);
                    self.event(HostEvent::ComponentsChanged {
                        plugin,
                        device: device.id,
                    });
                }
            }
            Some(method::DEVICES_CHANGED) => {
                if let Some(changed) = parse::<DevicesChanged>(&message) {
                    let current: Vec<DeviceInfo> = self.plugins[i]
                        .devices
                        .iter()
                        .filter(|d| d.bus == Bus::Virtual)
                        .cloned()
                        .collect();
                    for gone in current.iter().filter(|d| !changed.devices.contains(d)) {
                        self.detach(i, &gone.id);
                    }
                    for new in changed.devices.into_iter().filter(|d| !current.contains(d)) {
                        self.attach(i, new);
                    }
                }
            }
            Some(method::PROCEDURE_UPDATE) => {
                if let Some(update) = parse::<ProcedureSession>(&message) {
                    let session = format!("{plugin}:{}", update.session);
                    self.event(HostEvent::ProcedureUpdated {
                        session,
                        state: update.state,
                    });
                }
            }
            Some(method::INPUT_STATE) => {
                if let Some(state) = parse::<InputState>(&message) {
                    let address = format!("{plugin}/{}", state.component);
                    self.event(HostEvent::Inputs {
                        address,
                        values: state.values,
                    });
                }
            }
            _ => {}
        }
    }

    /// Applies notifications already received (e.g. `params/changed` after a set).
    fn drain(&mut self, plugin: &str) {
        let Ok(i) = self.index(plugin) else { return };
        let pending = self.plugins[i]
            .process
            .as_ref()
            .map(PluginProcess::pending_notifications)
            .unwrap_or_default();
        for message in pending {
            self.handle_notification(i, message);
        }
    }

    fn store_description(&mut self, plugin: &str, description: ComponentDescription) {
        let address = format!("{plugin}/{}", description.id);
        if let Some(c) = self.components.iter_mut().find(|c| c.address == address) {
            c.description = description;
        }
    }

    // ---------------------------------------------------------------- components

    /// Resolves a component by full address (`sim/rig0/base`) or by a unique suffix
    /// (`base`, `rig0/base`).
    pub fn find(&self, query: &str) -> Result<&HostComponent, HostError> {
        if let Some(c) = self.components.iter().find(|c| c.address == query) {
            return Ok(c);
        }
        let suffix = format!("/{query}");
        let matches: Vec<&HostComponent> = self
            .components
            .iter()
            .filter(|c| c.address.ends_with(&suffix))
            .collect();
        match matches.as_slice() {
            [one] => Ok(one),
            [] => Err(HostError::NotFound(format!("no component '{query}'"))),
            many => {
                let names: Vec<&str> = many.iter().map(|c| c.address.as_str()).collect();
                Err(HostError::NotFound(format!(
                    "'{query}' is ambiguous: {}",
                    names.join(", ")
                )))
            }
        }
    }

    fn route(&self, address: &str) -> Result<(usize, String), HostError> {
        let c = self.find(address)?;
        let local = c.address[c.plugin.len() + 1..].to_string();
        Ok((self.index(&c.plugin)?, local))
    }

    /// Asks the plugin for a fresh description of the component.
    pub fn describe(&mut self, address: &str) -> Result<&ComponentDescription, HostError> {
        let (i, local) = self.route(address)?;
        let description: ComponentDescription =
            self.plugins[i].call(method::DESCRIBE, ComponentParams { component: local })?;
        let plugin = self.plugins[i].manifest.id.clone();
        self.store_description(&plugin, description);
        self.drain(&plugin);
        Ok(&self.find(address)?.description)
    }

    pub fn param(&mut self, address: &str, item: &str) -> Result<ParamSpec, HostError> {
        let (i, local) = self.route(address)?;
        self.plugins[i].call(
            method::STD_GET,
            ItemParams {
                component: local,
                item: item.into(),
            },
        )
    }

    /// Sets a standard parameter, or a custom one when `item` is `custom.<id>`. Never needs
    /// privileges: it goes through the driver's interfaces, like a game.
    pub fn set(&mut self, address: &str, item: &str, value: Value) -> Result<(), HostError> {
        let (i, local) = self.route(address)?;
        let (name, item) = match item.strip_prefix(CUSTOM_PREFIX) {
            Some(custom) => (method::CUSTOM_SET, custom),
            None => (method::STD_SET, item),
        };
        self.plugins[i].call::<_, ()>(
            name,
            SetParams {
                component: local,
                item: item.into(),
                value,
            },
        )?;
        // Other items may have changed availability (`DependsOn`): refresh the description.
        self.describe(address)?;
        Ok(())
    }

    /// Makes the current value of a persistent item the one the driver restores at plug-in.
    /// Depending on the driver this may ask for authorization (e.g. to write a system file).
    pub fn save(&mut self, address: &str, item: &str) -> Result<(), HostError> {
        let (i, local) = self.route(address)?;
        self.plugins[i].call::<_, ()>(
            method::SAVE,
            ItemParams {
                component: local,
                item: item.into(),
            },
        )?;
        self.describe(address)?;
        Ok(())
    }

    pub fn run_action(
        &mut self,
        address: &str,
        item: &str,
        args: Value,
    ) -> Result<ActionOutcome, HostError> {
        let (i, local) = self.route(address)?;
        let outcome = self.plugins[i].call(
            method::ACTION_RUN,
            ActionParams {
                component: local,
                item: item.into(),
                args,
            },
        )?;
        self.describe(address)?;
        Ok(outcome)
    }

    pub fn write_output(
        &mut self,
        address: &str,
        item: &str,
        value: Value,
    ) -> Result<(), HostError> {
        let (i, local) = self.route(address)?;
        self.plugins[i].call(
            method::OUTPUT_WRITE,
            SetParams {
                component: local,
                item: item.into(),
                value,
            },
        )
    }

    pub fn read_inputs(&mut self, address: &str) -> Result<BTreeMap<String, Value>, HostError> {
        let (i, local) = self.route(address)?;
        let state: InputState =
            self.plugins[i].call(method::INPUT_READ, ComponentParams { component: local })?;
        Ok(state.values)
    }

    /// Live inputs then arrive as [`HostEvent::Inputs`] from [`Host::poll`], or through
    /// [`Host::next_input_state`].
    pub fn subscribe(&mut self, address: &str, rate_hz: f64) -> Result<(), HostError> {
        let (i, local) = self.route(address)?;
        self.plugins[i].call(
            method::INPUT_SUBSCRIBE,
            SubscribeParams {
                component: local,
                rate_hz,
            },
        )
    }

    pub fn unsubscribe(&mut self, address: &str) -> Result<(), HostError> {
        let (i, local) = self.route(address)?;
        self.plugins[i].call(
            method::INPUT_UNSUBSCRIBE,
            ComponentParams { component: local },
        )
    }

    /// Waits for the next live input state of a subscribed component.
    pub fn next_input_state(
        &mut self,
        address: &str,
        timeout: Duration,
    ) -> Result<Option<BTreeMap<String, Value>>, HostError> {
        let (i, local) = self.route(address)?;
        let deadline = Instant::now() + timeout;
        loop {
            let message = match &self.plugins[i].process {
                Some(process) => process.next_notification(deadline),
                None => {
                    return Err(HostError::PluginNotRunning(
                        self.plugins[i].manifest.id.clone(),
                    ));
                }
            };
            let Some(message) = message else {
                return Ok(None);
            };
            if message.method.as_deref() == Some(method::INPUT_STATE) {
                if let Some(state) = parse::<InputState>(&message) {
                    if state.component == local {
                        return Ok(Some(state.values));
                    }
                }
            } else {
                self.handle_notification(i, message);
            }
        }
    }

    // ---------------------------------------------------------------- rig composition

    pub fn rig_config(&self) -> &RigConfig {
        &self.rig
    }

    /// Excludes a component from the rig (or includes it back). Saved in `rig.toml`.
    pub fn set_excluded(&mut self, address: &str, excluded: bool) -> Result<(), HostError> {
        let address = self.find(address)?.address.clone();
        if let Some(c) = self.components.iter_mut().find(|c| c.address == address) {
            c.excluded = excluded;
        }
        if excluded {
            self.rig.excluded.insert(address);
        } else {
            self.rig.excluded.remove(&address);
        }
        self.rig.save(&self.options.config_dir)
    }

    /// Chooses which plugin handles a device (`usb:vvvv:pppp[:serial]`), or clears the
    /// choice. Takes effect the next time the device is plugged in or the app starts.
    /// Chooses which plugin handles a device (`usb:vvvv:pppp[:serial]`), or clears the
    /// choice. A generic plugin (`on_request`) gets a device only this way. Applied at once if
    /// the device is connected, and remembered for the next time.
    pub fn choose_plugin(
        &mut self,
        device_key: &str,
        plugin: Option<&str>,
    ) -> Result<(), HostError> {
        match plugin {
            Some(p) => {
                self.index(p)?;
                self.rig.device_plugins.insert(device_key.into(), p.into());
            }
            None => {
                self.rig.device_plugins.remove(device_key);
            }
        }
        self.rig.save(&self.options.config_dir)?;
        let connected: Vec<UsbDevice> = self
            .usb
            .iter()
            .filter(|d| d.keys().iter().any(|k| k == device_key))
            .cloned()
            .collect();
        for device in connected {
            let info = device.device_info();
            if let Some(owner) = self
                .plugins
                .iter()
                .position(|p| p.devices.iter().any(|d| d.id == info.id))
            {
                self.detach(owner, &info.id);
                if self.plugins[owner].devices.is_empty()
                    && !self.plugins[owner].manifest.matching.virtual_devices
                {
                    self.stop_plugin(owner);
                }
            }
            for (i, devices) in self
                .assign(std::slice::from_ref(&device))
                .into_iter()
                .enumerate()
            {
                for device in devices {
                    if self.plugins[i].state == PluginState::Running {
                        self.attach(i, device.device_info());
                    } else {
                        self.start_plugin(i, vec![device.device_info()]);
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds a catalog model to the rig before it is plugged in (or removes it).
    pub fn set_planned(&mut self, model: &str, planned: bool) -> Result<(), HostError> {
        if planned {
            self.catalog
                .model(model)
                .ok_or_else(|| HostError::NotFound(format!("no model '{model}'")))?;
            if !self.rig.planned.iter().any(|m| m == model) {
                self.rig.planned.push(model.into());
            }
        } else {
            self.rig.planned.retain(|m| m != model);
        }
        self.rig.save(&self.options.config_dir)
    }

    /// USB devices currently connected, with the plugin handling each one.
    pub fn usb_devices(&self) -> Vec<(UsbDevice, Option<String>)> {
        self.usb
            .iter()
            .map(|d| {
                let id = d.device_info().id;
                let owner = self
                    .plugins
                    .iter()
                    .find(|p| p.devices.iter().any(|x| x.id == id))
                    .map(|p| p.manifest.id.clone());
                (d.clone(), owner)
            })
            .collect()
    }

    /// Devices attached to running plugins (USB and virtual), as `(plugin, device)`.
    pub fn devices(&self) -> Vec<(String, DeviceInfo)> {
        self.plugins
            .iter()
            .flat_map(|p| {
                p.devices
                    .iter()
                    .map(move |d| (p.manifest.id.clone(), d.clone()))
            })
            .collect()
    }

    // ---------------------------------------------------------------- procedures

    /// Resolves a procedure target: a component (address or unique suffix) or a device
    /// (`<plugin>/<device id>`).
    pub fn target(&self, query: &str) -> Result<Target, HostError> {
        if let Ok(c) = self.find(query) {
            return Ok(Target::Component(c.address.clone()));
        }
        let (plugin, device) = query
            .split_once('/')
            .ok_or_else(|| HostError::NotFound(format!("no component or device '{query}'")))?;
        let i = self.index(plugin)?;
        if !self.plugins[i].devices.iter().any(|d| d.id == device) {
            return Err(HostError::NotFound(format!("no device '{query}'")));
        }
        Ok(Target::Device {
            plugin: plugin.into(),
            device: device.into(),
        })
    }

    /// Procedures offered on a target. Component procedures also come with its description.
    pub fn procedures(&mut self, target: &Target) -> Result<Vec<ProcedureDescriptor>, HostError> {
        match target {
            Target::Component(address) => Ok(self.describe(address)?.procedures.clone()),
            Target::Device { plugin, device } => {
                let i = self.index(plugin)?;
                let result: ProceduresResult = self.plugins[i].call(
                    method::PROCEDURES,
                    DeviceParams {
                        device: device.clone(),
                    },
                )?;
                Ok(result.procedures)
            }
        }
    }

    /// Starts a procedure; returns the session id (`<plugin>:<session>`) and its first state.
    pub fn start_procedure(
        &mut self,
        target: &Target,
        procedure: &str,
    ) -> Result<(String, ProcedureState), HostError> {
        let (i, target) = match target {
            Target::Component(address) => {
                let (i, local) = self.route(address)?;
                (i, ProcedureTarget::Component(local))
            }
            Target::Device { plugin, device } => {
                (self.index(plugin)?, ProcedureTarget::Device(device.clone()))
            }
        };
        let started: ProcedureSession = self.plugins[i].call(
            method::PROCEDURE_START,
            ProcedureStartParams {
                target,
                procedure: procedure.into(),
            },
        )?;
        let plugin = self.plugins[i].manifest.id.clone();
        self.drain(&plugin);
        Ok((format!("{plugin}:{}", started.session), started.state))
    }

    /// Sends the user's action and field values for the current step.
    pub fn submit_procedure(
        &mut self,
        session: &str,
        action: &str,
        values: BTreeMap<String, Value>,
    ) -> Result<ProcedureState, HostError> {
        let (i, local) = self.session(session)?;
        let state: ProcedureState = self.plugins[i].call(
            method::PROCEDURE_SUBMIT,
            ProcedureSubmitParams {
                session: local,
                action: action.into(),
                values,
            },
        )?;
        let plugin = self.plugins[i].manifest.id.clone();
        self.drain(&plugin);
        Ok(state)
    }

    pub fn cancel_procedure(&mut self, session: &str) -> Result<(), HostError> {
        let (i, local) = self.session(session)?;
        self.plugins[i].call(
            method::PROCEDURE_CANCEL,
            ProcedureCancelParams { session: local },
        )
    }

    fn session(&self, session: &str) -> Result<(usize, String), HostError> {
        let (plugin, local) = session
            .split_once(':')
            .ok_or_else(|| HostError::NotFound(format!("no procedure session '{session}'")))?;
        Ok((self.index(plugin)?, local.into()))
    }

    // ---------------------------------------------------------------- catalog

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The picture of a component: the user's, the model's, or the generic one of its type.
    pub fn image(&self, address: &str) -> Result<Image, HostError> {
        let c = self.find(address)?;
        let user_images = self.options.data_dir.join("images");
        let d = &c.description;
        Ok(self.catalog.image(
            d.model.as_deref(),
            d.kind,
            &d.appearance,
            Some(&user_images),
        ))
    }

    // ---------------------------------------------------------------- drivers

    /// Runs the driver's status hook (as the user). `None` when the plugin has no driver.
    pub fn driver_status(&self, plugin: &str) -> Result<Option<DriverStatus>, HostError> {
        let p = &self.plugins[self.index(plugin)?];
        self.drivers.status(&p.manifest, &p.dir, p.location)
    }

    /// Installs a plugin's driver; asks for authorization for system plugins. The adapter is
    /// restarted so it sees the driver.
    pub fn install_driver(&mut self, plugin: &str) -> Result<String, HostError> {
        let i = self.index(plugin)?;
        let p = &self.plugins[i];
        if self
            .drivers
            .status(&p.manifest, &p.dir, p.location)?
            .is_some_and(|s| s.provided_by_system)
        {
            return Ok("an equivalent driver is already provided by the system".into());
        }
        let output = self.drivers.install(&p.manifest, &p.dir, p.location)?;
        self.restart_plugin(plugin)?;
        Ok(output)
    }

    /// Removes a plugin's driver; works also for plugins no longer installed.
    pub fn uninstall_driver(&mut self, plugin: &str) -> Result<String, HostError> {
        let location = match self.index(plugin) {
            Ok(i) => self.plugins[i].location,
            Err(_) => self
                .drivers
                .installed()
                .into_iter()
                .find(|(_, d)| d.plugin == plugin)
                .map(|(l, _)| l)
                .ok_or_else(|| {
                    HostError::NotFound(format!("no driver installed for '{plugin}'"))
                })?,
        };
        let output = self.drivers.uninstall(plugin, location)?;
        if self.index(plugin).is_ok() {
            self.restart_plugin(plugin)?;
        }
        Ok(output)
    }

    pub fn installed_drivers(&self) -> Vec<(PluginLocation, InstalledDriver)> {
        self.drivers.installed()
    }

    pub fn system(&self) -> SystemInfo {
        SystemInfo::detect()
    }

    // ---------------------------------------------------------------- presets

    pub fn presets(&self) -> &PresetStore {
        &self.presets
    }

    /// A preset with the current value of every saveable parameter of the rig. Types present
    /// once go in their section (portable across brands); repeated types per component.
    pub fn capture_preset(&mut self, name: &str) -> Result<Preset, HostError> {
        let addresses: Vec<String> = self
            .components
            .iter()
            .filter(|c| !c.excluded)
            .map(|c| c.address.clone())
            .collect();
        for address in &addresses {
            self.describe(address)?;
        }
        let mut preset = Preset {
            name: name.into(),
            ..Preset::default()
        };
        let active: Vec<&HostComponent> = self.components.iter().filter(|c| !c.excluded).collect();
        for c in &active {
            let d = &c.description;
            let unique = active
                .iter()
                .filter(|o| o.description.kind == d.kind)
                .count()
                == 1;
            for item in &d.items {
                let Support::Supported(ItemSpec::Param(spec)) = &item.support else {
                    continue;
                };
                let Some(value) =
                    to_toml(&spec.value).filter(|_| spec.persistence == Persistence::Persistent)
                else {
                    continue;
                };
                let section = if unique {
                    preset.kinds.entry(d.kind).or_default()
                } else {
                    preset.components.entry(c.address.clone()).or_default()
                };
                section.insert(item.id.clone(), value);
            }
            for p in d.custom.iter().filter(|p| {
                p.availability.is_supported() && p.persistence == Persistence::Persistent
            }) {
                if let Some(value) = to_toml(&p.value) {
                    preset
                        .custom
                        .entry(c.plugin.clone())
                        .or_default()
                        .insert(p.id.clone(), value);
                }
            }
        }
        Ok(preset)
    }

    /// Applies a preset: each value is set and, when persistent, saved into the driver.
    /// Values depending on another one (`DependsOn`) are retried after the others.
    pub fn apply_preset(&mut self, preset: &Preset) -> Vec<ApplyEntry> {
        let mut entries = Vec::new();
        let ignored = |target: &str, item: &str, why: String| ApplyEntry {
            target: target.into(),
            item: item.into(),
            outcome: ApplyOutcome::Ignored(why),
        };
        let active: Vec<(String, String)> = self
            .components
            .iter()
            .filter(|c| !c.excluded)
            .map(|c| (c.address.clone(), c.plugin.clone()))
            .collect();
        let kinds: BTreeSet<_> = self
            .components
            .iter()
            .filter(|c| !c.excluded)
            .map(|c| c.description.kind)
            .collect();
        for (kind, values) in &preset.kinds {
            if !kinds.contains(kind) {
                let why = format!("no {} in the rig", kind.label().to_lowercase());
                entries.extend(
                    values
                        .keys()
                        .map(|item| ignored(kind.key(), item, why.clone())),
                );
            }
        }
        for (address, values) in &preset.components {
            if !active.iter().any(|(a, _)| a == address) {
                entries.extend(
                    values
                        .keys()
                        .map(|item| ignored(address, item, "not in the rig".into())),
                );
            }
        }
        for (plugin, values) in &preset.custom {
            for id in values.keys() {
                let present = self.components.iter().any(|c| {
                    !c.excluded && &c.plugin == plugin && c.description.custom_param(id).is_some()
                });
                if !present {
                    entries.push(ignored(
                        plugin,
                        &format!("custom.{id}"),
                        "no component has it".into(),
                    ));
                }
            }
        }

        let mut work: Vec<(String, String, toml::Value)> = Vec::new();
        for (address, plugin) in &active {
            let Ok(c) = self.find(address) else { continue };
            let mut values = preset
                .kinds
                .get(&c.description.kind)
                .cloned()
                .unwrap_or_default();
            values.extend(preset.components.get(address).cloned().unwrap_or_default());
            for (id, value) in preset.custom.get(plugin).into_iter().flatten() {
                if c.description.custom_param(id).is_some() {
                    values.insert(format!("{CUSTOM_PREFIX}{id}"), value.clone());
                }
            }
            work.extend(
                values
                    .into_iter()
                    .map(|(item, v)| (address.clone(), item, v)),
            );
        }
        let mut retry = Vec::new();
        for (address, item, value) in work {
            let entry = self.apply_value(&address, &item, &value);
            match &entry {
                (_, true) => retry.push((address, item, value)),
                (e, false) => entries.push(e.clone()),
            }
        }
        for (address, item, value) in retry {
            entries.push(self.apply_value(&address, &item, &value).0);
        }
        entries
    }

    /// Applies one value; the flag says it was ignored because it depends on another item.
    fn apply_value(
        &mut self,
        address: &str,
        item: &str,
        value: &toml::Value,
    ) -> (ApplyEntry, bool) {
        let entry = |outcome| ApplyEntry {
            target: address.into(),
            item: item.into(),
            outcome,
        };
        let Ok(c) = self.find(address) else {
            return (entry(ApplyOutcome::Ignored("not in the rig".into())), false);
        };
        let state: Result<(ParamType, Persistence), Support<()>> =
            match item.strip_prefix(CUSTOM_PREFIX) {
                Some(id) => match c.description.custom_param(id) {
                    Some(p) => match &p.availability {
                        Support::Supported(()) => Ok((p.ty.clone(), p.persistence)),
                        Support::Unsupported(u) => Err(Support::Unsupported(u.clone())),
                    },
                    None => {
                        return (
                            entry(ApplyOutcome::Ignored("no such parameter".into())),
                            false,
                        );
                    }
                },
                None => match c.description.item(item).map(|i| &i.support) {
                    Some(Support::Supported(ItemSpec::Param(spec))) => {
                        Ok((spec.ty.clone(), spec.persistence))
                    }
                    Some(Support::Unsupported(u)) => Err(Support::Unsupported(u.clone())),
                    Some(_) => {
                        return (
                            entry(ApplyOutcome::Ignored("not a parameter".into())),
                            false,
                        );
                    }
                    None => {
                        return (
                            entry(ApplyOutcome::Ignored("no such parameter".into())),
                            false,
                        );
                    }
                },
            };
        let (ty, persistence) = match state {
            Ok(s) => s,
            Err(Support::Unsupported(u)) => {
                let depends = matches!(u.reason, Reason::DependsOn { .. });
                return (entry(ApplyOutcome::Ignored(u.explain())), depends);
            }
            Err(Support::Supported(())) => unreachable!("only unsupported states are errors"),
        };
        let value = match from_toml(value, &ty) {
            Ok(v) => v,
            Err(e) => return (entry(ApplyOutcome::Ignored(e)), false),
        };
        if let Err(e) = self.set(address, item, value) {
            return (entry(ApplyOutcome::Ignored(e.to_string())), false);
        }
        let outcome = match persistence {
            Persistence::Volatile => ApplyOutcome::Applied {
                note: Some("lasts until unplugged".into()),
            },
            Persistence::Persistent => match self.save(address, item) {
                Ok(()) => ApplyOutcome::Saved,
                Err(e) => ApplyOutcome::Applied {
                    note: Some(format!("not saved: {e}")),
                },
            },
        };
        (entry(outcome), false)
    }

    /// Stops all adapters. Also done on drop.
    pub fn shutdown(&mut self) {
        for plugin in &mut self.plugins {
            if let Some(process) = plugin.process.take() {
                process.shutdown();
            }
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn parse<T: serde::de::DeserializeOwned>(message: &Message) -> Option<T> {
    serde_json::from_value(message.params.clone()?).ok()
}

/// The adapter executable: in the plugin directory, then in the extra exec directories, then
/// next to the running executable (development builds keep all binaries in `target/<profile>`).
fn find_exec(exec: &str, plugin_dir: &Path, exec_dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut dirs = vec![plugin_dir.to_path_buf()];
    dirs.extend(exec_dirs.iter().cloned());
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        if dir.ends_with("deps") {
            dirs.extend(dir.parent().map(Path::to_path_buf));
        }
        dirs.push(dir);
    }
    dirs.into_iter().map(|d| d.join(exec)).find(|p| p.is_file())
}
