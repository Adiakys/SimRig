use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use simrig_model::{
    ActionOutcome, CUSTOM_PREFIX, ComponentDescription, DeviceInfo, ParamSpec, Value,
};
use simrig_protocol::{
    API_VERSION, ActionParams, AttachParams, AttachResult, ComponentParams, DiscoverResult,
    InitializeParams, InitializeResult, InputState, ItemParams, Message, ParamsChanged, SetParams,
    SubscribeParams, method,
};

use crate::conformance;
use crate::error::HostError;
use crate::manifest::{self, Manifest, PluginLocation};
use crate::process::PluginProcess;
use crate::usb;

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
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            plugin_dirs: Vec::new(),
            default_dirs: true,
            exec_dirs: Vec::new(),
            locale: "en".into(),
            timeout: Duration::from_secs(5),
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
}

/// A component of the rig, addressed as `<plugin>/<component id>`.
#[derive(Clone, Debug)]
pub struct HostComponent {
    pub address: String,
    pub plugin: String,
    pub device: DeviceInfo,
    pub description: ComponentDescription,
}

pub struct Host {
    plugins: Vec<Plugin>,
    components: Vec<HostComponent>,
    problems: Vec<String>,
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
        let (found, problems) = manifest::discover(&dirs);
        let mut host = Host {
            plugins: Vec::new(),
            components: Vec::new(),
            problems,
        };
        let usb_devices = usb::scan();
        let manifests: Vec<&Manifest> = found.iter().map(|f| &f.manifest).collect();
        let (mut assigned, claim_problems) = manifest::assign_usb(&manifests, &usb_devices);
        host.problems.extend(claim_problems);

        for (f, devices) in found.into_iter().zip(assigned.iter_mut()) {
            let mut plugin = Plugin {
                manifest: f.manifest,
                dir: f.dir,
                location: f.location,
                state: PluginState::Idle,
                info: None,
                process: None,
            };
            let devices: Vec<DeviceInfo> = devices.drain(..).map(|d| d.device_info()).collect();
            if !devices.is_empty() || plugin.manifest.matching.virtual_devices {
                if let Err(e) = host.start_plugin(&mut plugin, devices, options) {
                    host.problems
                        .push(format!("plugin '{}': {e}", plugin.manifest.id));
                    plugin.state = PluginState::Failed(e.to_string());
                    if let Some(process) = plugin.process.take() {
                        process.shutdown();
                    }
                }
            }
            host.plugins.push(plugin);
        }
        host
    }

    fn start_plugin(
        &mut self,
        plugin: &mut Plugin,
        devices: Vec<DeviceInfo>,
        options: &HostOptions,
    ) -> Result<(), HostError> {
        let id = plugin.manifest.id.clone();
        if plugin.manifest.api != API_VERSION {
            return Err(HostError::NotFound(format!(
                "manifest declares API {}, this app speaks API {API_VERSION}",
                plugin.manifest.api
            )));
        }
        let exec =
            find_exec(&plugin.manifest.exec, &plugin.dir, &options.exec_dirs).ok_or_else(|| {
                HostError::NotFound(format!("adapter '{}' not found", plugin.manifest.exec))
            })?;
        let process = plugin.process.insert(PluginProcess::spawn(
            &id,
            &exec,
            &plugin.dir,
            options.timeout,
        )?);

        let init: InitializeResult = process.call(
            method::INITIALIZE,
            InitializeParams {
                api_version: API_VERSION,
                host_name: "simrig".into(),
                host_version: env!("CARGO_PKG_VERSION").into(),
                locale: options.locale.clone(),
            },
        )?;
        if init.plugin.id != id {
            self.problems
                .push(format!("plugin '{id}' reports id '{}'", init.plugin.id));
        }
        plugin.info = Some(init.plugin);

        let mut devices = devices;
        if plugin.manifest.matching.virtual_devices {
            let discovered: DiscoverResult = process.call(method::DISCOVER, ())?;
            devices.extend(discovered.devices);
        }
        for device in devices {
            let attached: Result<AttachResult, _> = process.call(
                method::ATTACH,
                AttachParams {
                    device: device.clone(),
                },
            );
            match attached {
                Ok(result) => {
                    for description in result.components {
                        self.problems.extend(
                            conformance::check(&description)
                                .into_iter()
                                .map(|p| format!("plugin '{id}': {p}")),
                        );
                        self.components.push(HostComponent {
                            address: format!("{id}/{}", description.id),
                            plugin: id.clone(),
                            device: device.clone(),
                            description,
                        });
                    }
                }
                Err(e) => self
                    .problems
                    .push(format!("plugin '{id}': cannot attach {}: {e}", device.name)),
            }
        }
        plugin.state = PluginState::Running;
        Ok(())
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

    /// Asks the plugin for a fresh description of the component.
    pub fn describe(&mut self, address: &str) -> Result<&ComponentDescription, HostError> {
        let (plugin, local) = self.route(address)?;
        let description: ComponentDescription = self
            .process(&plugin)?
            .call(method::DESCRIBE, ComponentParams { component: local })?;
        self.store_description(&plugin, description);
        self.drain(&plugin);
        Ok(&self.find(address)?.description)
    }

    pub fn param(&mut self, address: &str, item: &str) -> Result<ParamSpec, HostError> {
        let (plugin, local) = self.route(address)?;
        self.process(&plugin)?.call(
            method::STD_GET,
            ItemParams {
                component: local,
                item: item.into(),
            },
        )
    }

    /// Sets a standard parameter, or a custom one when `item` is `custom.<id>`.
    pub fn set(&mut self, address: &str, item: &str, value: Value) -> Result<(), HostError> {
        let (plugin, local) = self.route(address)?;
        let (name, item) = match item.strip_prefix(CUSTOM_PREFIX) {
            Some(custom) => (method::CUSTOM_SET, custom),
            None => (method::STD_SET, item),
        };
        self.process(&plugin)?.call::<_, ()>(
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

    pub fn run_action(
        &mut self,
        address: &str,
        item: &str,
        args: Value,
    ) -> Result<ActionOutcome, HostError> {
        let (plugin, local) = self.route(address)?;
        let outcome = self.process(&plugin)?.call(
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
        let (plugin, local) = self.route(address)?;
        self.process(&plugin)?.call(
            method::OUTPUT_WRITE,
            SetParams {
                component: local,
                item: item.into(),
                value,
            },
        )
    }

    pub fn read_inputs(&mut self, address: &str) -> Result<BTreeMap<String, Value>, HostError> {
        let (plugin, local) = self.route(address)?;
        let state: InputState = self
            .process(&plugin)?
            .call(method::INPUT_READ, ComponentParams { component: local })?;
        Ok(state.values)
    }

    pub fn subscribe(&mut self, address: &str, rate_hz: f64) -> Result<(), HostError> {
        let (plugin, local) = self.route(address)?;
        self.process(&plugin)?.call(
            method::INPUT_SUBSCRIBE,
            SubscribeParams {
                component: local,
                rate_hz,
            },
        )
    }

    pub fn unsubscribe(&mut self, address: &str) -> Result<(), HostError> {
        let (plugin, local) = self.route(address)?;
        self.process(&plugin)?.call(
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
        let (plugin, local) = self.route(address)?;
        let deadline = Instant::now() + timeout;
        loop {
            let Some(message) = self.process(&plugin)?.next_notification(deadline) else {
                return Ok(None);
            };
            if message.method.as_deref() == Some(method::INPUT_STATE) {
                if let Some(state) = parse::<InputState>(&message) {
                    if state.component == local {
                        return Ok(Some(state.values));
                    }
                }
            } else {
                self.apply_notification(&plugin, message);
            }
        }
    }

    /// Stops all adapters. Also done on drop.
    pub fn shutdown(&mut self) {
        for plugin in &mut self.plugins {
            if let Some(process) = plugin.process.take() {
                process.shutdown();
            }
        }
    }

    fn route(&self, address: &str) -> Result<(String, String), HostError> {
        let c = self.find(address)?;
        let local = c.address[c.plugin.len() + 1..].to_string();
        Ok((c.plugin.clone(), local))
    }

    fn process(&mut self, plugin: &str) -> Result<&mut PluginProcess, HostError> {
        self.plugins
            .iter_mut()
            .find(|p| p.manifest.id == plugin)
            .and_then(|p| p.process.as_mut())
            .ok_or_else(|| HostError::PluginNotRunning(plugin.into()))
    }

    /// Applies notifications already received (e.g. `params/changed` after a set).
    fn drain(&mut self, plugin: &str) {
        let pending = match self.process(plugin) {
            Ok(p) => p.pending_notifications(),
            Err(_) => return,
        };
        for message in pending {
            self.apply_notification(plugin, message);
        }
    }

    fn apply_notification(&mut self, plugin: &str, message: Message) {
        if message.method.as_deref() == Some(method::PARAMS_CHANGED) {
            if let Some(changed) = parse::<ParamsChanged>(&message) {
                self.store_description(plugin, changed.component);
            }
        }
    }

    fn store_description(&mut self, plugin: &str, description: ComponentDescription) {
        let address = format!("{plugin}/{}", description.id);
        if let Some(c) = self.components.iter_mut().find(|c| c.address == address) {
            c.description = description;
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
