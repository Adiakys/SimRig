//! Flows and management screens: add a device, plugins and drivers, general settings and the
//! first run.

use std::collections::{BTreeMap, BTreeSet};

use slint::{ComponentHandle, ModelRc, VecModel};

use simrig_host::{PluginLocation, PluginState, Target};
use simrig_model::{Bus, ComponentKind, ProcedureDescriptor};

use crate::app::App;
use crate::backend::{Command, PluginView, Snapshot, UsbView};
use crate::settings::ThemeMode;
use crate::ui::{self, Rig, Screen};

/// Steps of the add-device flow.
pub const CONNECT: i32 = 0;
pub const DRIVER: i32 = 1;
pub const SETUP: i32 = 2;
pub const DONE: i32 = 3;

/// The device being added: a plugin's device, once a plugin handles it.
#[derive(Clone, Debug, PartialEq)]
pub struct Adding {
    pub plugin: String,
    pub device: String,
    pub name: String,
}

#[derive(Default)]
pub struct Flows {
    pub step: i32,
    /// USB devices plugged in before the flow started: the new ones are listed first.
    known_usb: BTreeSet<String>,
    show_all: bool,
    pub adding: Option<Adding>,
    /// A USB device and the plugin it was given to, until the plugin attaches it.
    assigning: Option<(String, String)>,
    busy: bool,
    error: String,
    /// USB devices a plugin did not take, by key: the plugin and what it said.
    refused: BTreeMap<String, (String, String)>,
    /// What the plugin said when the setup finished.
    pub done_message: String,
    /// Drivers installed in this session: the result can arrive before the rig is sent again.
    installed: BTreeSet<String>,
    /// The first run: 0 welcome, 1 devices, 2 your rig.
    pub first_step: i32,
    pub first_installing: BTreeSet<String>,
}

fn has_components(snapshot: &Snapshot, plugin: &str, device: &str) -> bool {
    snapshot
        .components
        .iter()
        .any(|c| c.component.plugin == plugin && c.component.device.id == device)
}

/// The component drawn for a device: its wheel base first, as the rig lists them.
fn first_component(snapshot: &Snapshot, plugin: &str, device: &str) -> Option<String> {
    snapshot
        .components
        .iter()
        .filter(|c| c.component.plugin == plugin && c.component.device.id == device)
        .min_by_key(|c| c.component.description.kind)
        .map(|c| c.component.address.clone())
}

fn needs_driver(plugin: &PluginView) -> bool {
    plugin.has_driver
        && !plugin
            .driver
            .as_ref()
            .is_some_and(|s| s.installed || s.provided_by_system)
}

fn kinds_text(snapshot: &Snapshot, plugin: &str, device: &str) -> String {
    let mut kinds: Vec<ComponentKind> = snapshot
        .components
        .iter()
        .filter(|c| c.component.plugin == plugin && c.component.device.id == device)
        .map(|c| c.component.description.kind)
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
        .iter()
        .map(|k| k.label().to_lowercase())
        .collect::<Vec<_>>()
        .join(", ")
}

impl App {
    // ------------------------------------------------------------------ device lists

    /// The devices of the rig and, in the add flow, the USB devices no plugin handles.
    fn found(&mut self, unknown: bool) -> Vec<ui::FoundDevice> {
        let s = self.snapshot.clone();
        let s = &s;
        let mut found = Vec::new();
        for plugin in &s.plugins {
            for device in &plugin.devices {
                let kinds = kinds_text(s, &plugin.id, &device.id);
                let preview = first_component(s, &plugin.id, &device.id)
                    .map(|a| self.device(&a))
                    .unwrap_or_default();
                let set_up = has_components(s, &plugin.id, &device.id);
                let key = match &device.bus {
                    Bus::Usb { vid, pid, .. } => format!("usb:{vid:04x}:{pid:04x}"),
                    Bus::Virtual => String::new(),
                };
                let (state, detail) = if needs_driver(plugin) {
                    ("driver", format!("{} plugin · needs a driver", plugin.name))
                } else if !set_up {
                    ("setup", format!("{} plugin · to set up", plugin.name))
                } else {
                    ("ready", format!("{} plugin · {kinds}", plugin.name))
                };
                let usb = s
                    .usb
                    .iter()
                    .find(|u| u.plugin.as_ref() == Some(&plugin.id) && u.device == device.id);
                let (choices, choice) = usb
                    .map(|u| self.plugin_choices(u, None))
                    .unwrap_or_default();
                let untested = s.components.iter().any(|c| {
                    c.component.plugin == plugin.id
                        && c.component.device.id == device.id
                        && c.untested
                });
                found.push(ui::FoundDevice {
                    key: format!("{}/{}", plugin.id, device.id).into(),
                    name: device.name.clone().into(),
                    detail: detail.into(),
                    state: state.into(),
                    plugin: plugin.id.clone().into(),
                    plugin_name: plugin.name.clone().into(),
                    is_new: !key.is_empty() && !self.flows.known_usb.contains(&key),
                    preview,
                    usb: usb.map(|u| u.key.clone()).unwrap_or_default().into(),
                    choices: ModelRc::new(VecModel::from(choices)),
                    choice,
                    refused: usb.map(|u| self.refused(u)).unwrap_or_default().into(),
                    untested,
                    ..Default::default()
                });
            }
        }
        if unknown {
            for usb in s.usb.iter().filter(|u| u.plugin.is_none()) {
                let new = !self.flows.known_usb.contains(&usb.key);
                if !new && !self.flows.show_all {
                    continue;
                }
                let generic = s.plugins.iter().any(|p| p.on_request);
                // The maker's plugin is offered first, until it has said no.
                let maker = usb
                    .maker
                    .clone()
                    .filter(|m| self.flows.refused.get(&usb.key).is_none_or(|(p, _)| p != m));
                let maker_name = maker.as_deref().map(|m| self.plugin_name(m));
                let knows = match &maker_name {
                    Some(name) => format!("the {name} plugin knows its maker, not this model"),
                    None if generic => "no plugin knows it".into(),
                    None => "no plugin knows it, and no generic plugin is installed".into(),
                };
                let (choices, choice) = self.plugin_choices(usb, maker.as_deref());
                found.push(ui::FoundDevice {
                    key: usb.key.clone().into(),
                    name: usb.name.clone().into(),
                    detail: format!("USB {:04x}:{:04x} · {knows}", usb.vid, usb.pid).into(),
                    state: if generic { "unknown" } else { "unsupported" }.into(),
                    usb: usb.key.clone().into(),
                    maker: maker.clone().unwrap_or_default().into(),
                    maker_name: maker_name.unwrap_or_default().into(),
                    choices: ModelRc::new(VecModel::from(choices)),
                    choice,
                    refused: self.refused(usb).into(),
                    is_new: new,
                    plugins: ModelRc::new(VecModel::from(
                        self.brand_candidates(&usb.key)
                            .iter()
                            .map(|c| self.plugin_name(c).into())
                            .collect::<Vec<slint::SharedString>>(),
                    )),
                    ..Default::default()
                });
            }
        }
        // New devices first, then as the rig lists them.
        found.sort_by_key(|f| !f.is_new);
        found
    }

    /// Keeps every flow screen in sync with the rig.
    pub fn refresh_flows(&mut self) {
        let window = self.window();
        let rig = window.global::<Rig>();

        // The device being assigned has been attached: go on. Or its plugin refused it.
        if let Some((key, chosen)) = self.flows.assigning.clone() {
            let usb = self.snapshot.usb.iter().find(|u| u.key == key).cloned();
            if let Some(usb) = usb {
                if let Some(plugin) = usb.plugin.clone() {
                    self.flows.assigning = None;
                    self.flows.adding = Some(Adding {
                        plugin,
                        device: usb.device,
                        name: usb.name,
                    });
                    self.add_next();
                } else if usb.chosen.as_ref() == Some(&chosen) {
                    let refused = format!("cannot attach {}: ", usb.name);
                    let reason = self
                        .snapshot
                        .problems
                        .iter()
                        .rev()
                        .find_map(|p| p.split_once(&refused).map(|(_, r)| r.to_string()));
                    self.flows.assigning = None;
                    self.flows
                        .refused
                        .insert(key.clone(), (chosen, reason.unwrap_or_default()));
                    // Back to the plugin that handles it by itself, if any.
                    self.send(Command::ChoosePlugin {
                        device: key,
                        plugin: None,
                    });
                }
            }
        }

        rig.set_found(ModelRc::new(VecModel::from(self.found(true))));
        rig.set_show_all(self.flows.show_all);
        rig.set_add_step(self.flows.step);
        rig.set_add_busy(self.flows.busy || self.flows.assigning.is_some());
        rig.set_add_error(self.flows.error.clone().into());
        rig.set_add_done_message(self.flows.done_message.clone().into());
        if let Some(adding) = self.flows.adding.clone() {
            let plugin = self
                .snapshot
                .plugins
                .iter()
                .find(|p| p.id == adding.plugin)
                .cloned();
            let preview = first_component(&self.snapshot, &adding.plugin, &adding.device)
                .map(|a| self.device(&a))
                .unwrap_or_default();
            rig.set_adding(ui::FoundDevice {
                preview,
                key: format!("{}/{}", adding.plugin, adding.device).into(),
                name: adding.name.clone().into(),
                plugin: adding.plugin.clone().into(),
                plugin_name: plugin
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_default()
                    .into(),
                ..Default::default()
            });
            rig.set_add_driver_name(
                plugin
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_default()
                    .into(),
            );
            rig.set_add_privileged(
                plugin
                    .as_ref()
                    .is_some_and(|p| p.location == PluginLocation::System),
            );
            let done: Vec<ui::Device> = self
                .snapshot
                .components
                .iter()
                .filter(|c| {
                    c.component.plugin == adding.plugin && c.component.device.id == adding.device
                })
                .map(|c| c.component.address.clone())
                .collect::<Vec<_>>()
                .into_iter()
                .map(|a| self.device(&a))
                .collect();
            rig.set_add_done(ModelRc::new(VecModel::from(done)));
        }

        self.refresh_plugins();
        self.refresh_settings();
        self.refresh_first_run();
    }

    // ------------------------------------------------------------------ add a device

    pub fn start_add(&mut self) {
        self.flows.known_usb = self.snapshot.usb.iter().map(|u| u.key.clone()).collect();
        self.flows.step = CONNECT;
        self.flows.adding = None;
        self.flows.assigning = None;
        self.flows.done_message.clear();
        self.flows.busy = false;
        self.flows.error.clear();
        self.flows.refused.clear();
        self.flows.show_all = false;
        self.refresh_flows();
        self.go(Screen::Add);
    }

    pub fn add_show_all(&mut self, on: bool) {
        self.flows.show_all = on;
        self.refresh_flows();
    }

    /// A device of the list was picked: `plugin/device` of a handled device.
    pub fn add_choose(&mut self, key: &str) {
        let Some((plugin, device)) = key.split_once('/') else {
            return;
        };
        let name = self
            .snapshot
            .plugins
            .iter()
            .find(|p| p.id == plugin)
            .and_then(|p| p.devices.iter().find(|d| d.id == device))
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.flows.adding = Some(Adding {
            plugin: plugin.into(),
            device: device.into(),
            name,
        });
        self.add_next();
    }

    /// An unknown USB device is given to the generic plugin, on the user's request only.
    pub fn add_generic(&mut self, usb_key: &str) {
        let Some(plugin) = self.snapshot.plugins.iter().find(|p| p.on_request) else {
            return;
        };
        let plugin = plugin.id.clone();
        self.assign(usb_key, plugin);
    }

    /// An unknown USB device is given to a brand plugin, which accepts it or says why not.
    pub fn add_assign(&mut self, usb_key: &str, index: i32) {
        let Some(plugin) = usize::try_from(index)
            .ok()
            .and_then(|i| self.brand_candidates(usb_key).into_iter().nth(i))
        else {
            return;
        };
        self.assign(usb_key, plugin);
    }

    /// A USB device is given to a plugin of its menu, or back to the automatic one (empty).
    pub fn add_pick(&mut self, usb_key: &str, plugin: &str) {
        if plugin.is_empty() {
            self.flows.refused.remove(usb_key);
            self.send(Command::ChoosePlugin {
                device: usb_key.to_string(),
                plugin: None,
            });
        } else {
            self.assign(usb_key, plugin.to_string());
        }
    }

    fn assign(&mut self, usb_key: &str, plugin: String) {
        self.flows.assigning = Some((usb_key.to_string(), plugin.clone()));
        self.flows.refused.remove(usb_key);
        self.flows.error.clear();
        self.send(Command::ChoosePlugin {
            device: usb_key.to_string(),
            plugin: Some(plugin),
        });
        self.refresh_flows();
    }

    /// The plugins a USB device can be given to, generic ones aside, by name.
    fn brand_candidates(&self, usb_key: &str) -> Vec<String> {
        let Some(usb) = self.snapshot.usb.iter().find(|u| u.key == usb_key) else {
            return Vec::new();
        };
        let mut brands: Vec<String> = usb
            .candidates
            .iter()
            .filter(|c| {
                self.snapshot
                    .plugins
                    .iter()
                    .any(|p| &&p.id == c && !p.on_request)
            })
            .cloned()
            .collect();
        brands.sort_by_key(|c| self.plugin_name(c));
        brands
    }

    /// The plugins of a device's menu: "Automatic" when a plugin knows the model, then the
    /// other brand plugins, then the generic ones; `skip` is offered apart (the maker's plugin).
    /// Returns them with the index of the current one (-1 when none is).
    fn plugin_choices(&self, usb: &UsbView, skip: Option<&str>) -> (Vec<ui::PluginChoice>, i32) {
        let mut choices = Vec::new();
        if let Some(auto) = &usb.automatic {
            choices.push(ui::PluginChoice {
                id: "".into(),
                label: "Automatic".into(),
                detail: format!("{} plugin, which knows this model", self.plugin_name(auto)).into(),
                section: "".into(),
            });
        }
        let on_request = |id: &str| {
            self.snapshot
                .plugins
                .iter()
                .any(|p| p.id == id && p.on_request)
        };
        for (generic, section) in [(false, "OTHER PLUGINS"), (true, "GENERIC")] {
            let mut ids: Vec<&String> = usb
                .candidates
                .iter()
                .filter(|c| on_request(c) == generic)
                .filter(|c| Some(c.as_str()) != skip && Some(*c) != usb.automatic.as_ref())
                .collect();
            ids.sort_by_key(|c| self.plugin_name(c));
            for (i, id) in ids.into_iter().enumerate() {
                choices.push(ui::PluginChoice {
                    id: id.clone().into(),
                    label: self.plugin_name(id).into(),
                    detail: "".into(),
                    section: if i == 0 { section } else { "" }.into(),
                });
            }
        }
        let current = usb.chosen.as_deref().unwrap_or_default();
        let choice = choices
            .iter()
            .position(|c| c.id == current)
            .map_or(-1, |i| i as i32);
        (choices, choice)
    }

    /// Why the plugin a USB device was last given to did not take it.
    fn refused(&self, usb: &UsbView) -> String {
        match self.flows.refused.get(&usb.key) {
            Some((plugin, reason)) if reason.is_empty() => {
                format!("Not taken by the {} plugin.", self.plugin_name(plugin))
            }
            Some((plugin, reason)) => {
                format!(
                    "Not taken by the {} plugin: {reason}.",
                    self.plugin_name(plugin)
                )
            }
            None => String::new(),
        }
    }

    fn plugin_name(&self, id: &str) -> String {
        self.snapshot
            .plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// Moves to the first step the device still needs.
    fn add_next(&mut self) {
        let Some(adding) = self.flows.adding.clone() else {
            return;
        };
        let plugin = self
            .snapshot
            .plugins
            .iter()
            .find(|p| p.id == adding.plugin)
            .cloned();
        let installed = self.flows.installed.contains(&adding.plugin);
        if plugin.as_ref().is_some_and(needs_driver) && !installed {
            self.flows.step = DRIVER;
        } else {
            // Its setup, if it has one: devices without components yet always do; others may
            // (pedals the plugin does not recognise, to map).
            self.flows.step = SETUP;
            self.flows.busy = true;
            self.send(Command::ListProcedures(Target::Device {
                plugin: adding.plugin.clone(),
                device: adding.device.clone(),
            }));
        }
        self.flows.error.clear();
        self.refresh_flows();
    }

    pub fn add_install_driver(&mut self) {
        let Some(adding) = self.flows.adding.clone() else {
            return;
        };
        self.flows.busy = true;
        self.flows.error.clear();
        self.send(Command::InstallDriver(adding.plugin));
        self.refresh_flows();
    }

    /// The procedures of the device arrived: run its setup, or finish.
    pub fn flow_procedures(&mut self, target: Target, procedures: Vec<ProcedureDescriptor>) {
        let Target::Device { plugin, device } = &target else {
            return;
        };
        let current = self
            .flows
            .adding
            .as_ref()
            .is_some_and(|a| a.plugin == *plugin && a.device == *device);
        if !current || self.flows.step != SETUP {
            return;
        }
        self.flows.busy = false;
        let available: Vec<&ProcedureDescriptor> = procedures
            .iter()
            .filter(|p| p.availability.is_supported())
            .collect();
        let setup = if has_components(&self.snapshot, plugin, device) {
            // Already working: only what the plugin marks as part of its setup.
            available.iter().find(|p| p.setup)
        } else {
            // Nothing works yet: its setup, else whatever it offers.
            available
                .iter()
                .find(|p| p.setup || p.id == "setup")
                .or(available.first())
        }
        .map(|p| p.id.clone());
        match setup {
            Some(id) => {
                // The procedure screen shows the steps of the flow.
                self.start_procedure(target, &id);
            }
            None => {
                self.flows.step = DONE;
                self.refresh_flows();
            }
        }
    }

    /// The setup procedure ended: finished goes to Done, cancelled back to Connect.
    pub fn flow_procedure_done(&mut self, finished: bool) {
        self.flows.step = if finished { DONE } else { CONNECT };
        if !finished {
            self.flows.adding = None;
        }
        self.window().set_screen(Screen::Add);
        self.refresh_flows();
    }

    pub fn add_finish(&mut self, open_settings: bool) {
        let first = self.flows.adding.as_ref().and_then(|a| {
            self.snapshot
                .components
                .iter()
                .find(|c| c.component.plugin == a.plugin && c.component.device.id == a.device)
                .map(|c| c.component.address.clone())
        });
        self.flows.adding = None;
        self.flows.step = CONNECT;
        match first {
            Some(address) if open_settings => self.open_component(&address),
            _ => self.go(Screen::Home),
        }
    }

    pub fn driver_done(&mut self, plugin: String, ok: bool, text: String) {
        self.flows.first_installing.remove(&plugin);
        let in_flow = self
            .flows
            .adding
            .as_ref()
            .is_some_and(|a| a.plugin == plugin)
            && self.flows.step == DRIVER;
        if ok {
            self.flows.installed.insert(plugin.clone());
        } else {
            self.flows.installed.remove(&plugin);
        }
        if in_flow {
            self.flows.busy = false;
            if ok {
                self.add_next();
            } else {
                self.flows.error = text;
                self.refresh_flows();
            }
            return;
        }
        self.toast(&text, if ok { "success" } else { "error" });
        self.refresh_flows();
    }

    // ------------------------------------------------------------------ plugins

    fn refresh_plugins(&mut self) {
        let rows: Vec<ui::PluginRow> = self
            .snapshot
            .plugins
            .iter()
            .map(|p| {
                let (state, tone, error) = match &p.state {
                    PluginState::Running => ("Running", "success", String::new()),
                    PluginState::Idle => ("Waiting for its devices", "neutral", String::new()),
                    PluginState::Failed(e) => ("Not responding", "error", e.clone()),
                };
                let devices = match p.devices.len() {
                    0 if p.on_request => "Handles the devices you assign to it".to_string(),
                    0 => "No device connected".to_string(),
                    1 => p.devices[0].name.clone(),
                    n => format!("{n} devices"),
                };
                let (driver_text, driver_state, driver_tone) = match &p.driver {
                    _ if !p.has_driver => ("No driver needed", "none", "neutral"),
                    Some(s) if s.provided_by_system => {
                        ("Driver provided by the system", "system", "success")
                    }
                    Some(s) if s.needs_reboot => ("Driver needs a restart", "reboot", "warning"),
                    Some(s) if s.installed => ("Driver installed", "installed", "success"),
                    _ => ("Driver not installed", "missing", "warning"),
                };
                // The driver's version says what runs: the one recorded at install while an
                // update waits, the system's own when it provides the driver, else the one
                // shipped (installed or not).
                let installed = p
                    .driver_update
                    .as_ref()
                    .map(|u| u.installed.clone())
                    .or_else(|| {
                        p.driver
                            .as_ref()
                            .filter(|s| s.installed && s.provided_by_system)
                            .and_then(|s| s.version.clone())
                    });
                let version = match (&p.driver_name, installed.or(p.driver_version.clone())) {
                    (Some(name), Some(v)) => format!("{name} {v}"),
                    _ => p.version.clone(),
                };
                let update_text = match &p.driver_update {
                    Some(u) if u.installed != u.available => {
                        format!("Driver update: {} → {}", u.installed, u.available)
                    }
                    Some(_) => "Driver update available".to_string(),
                    None => String::new(),
                };
                ui::PluginRow {
                    id: p.id.clone().into(),
                    name: p.name.clone().into(),
                    version: version.into(),
                    update: p.driver_update.is_some(),
                    sources: ModelRc::new(VecModel::from(
                        p.driver_sources
                            .iter()
                            .map(|(name, url)| ui::Link {
                                name: name.into(),
                                url: url.into(),
                            })
                            .collect::<Vec<_>>(),
                    )),
                    update_text: update_text.into(),
                    origin: match p.location {
                        PluginLocation::System => "Installed with SimRig",
                        PluginLocation::User => "Added by you",
                        PluginLocation::Explicit => "Development",
                    }
                    .into(),
                    state: state.into(),
                    tone: tone.into(),
                    error: error.into(),
                    devices: devices.into(),
                    description: p.description.clone().unwrap_or_default().into(),
                    has_driver: p.has_driver,
                    driver_text: driver_text.into(),
                    driver_state: driver_state.into(),
                    driver_tone: driver_tone.into(),
                    privileged: p.location == PluginLocation::System,
                    busy: self.flows.first_installing.contains(&p.id),
                }
            })
            .collect();
        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_drivers_outdated(rows.iter().filter(|r| r.update).count() as i32);
        rig.set_plugins(ModelRc::new(VecModel::from(rows)));
        let problems: Vec<slint::SharedString> =
            self.snapshot.problems.iter().map(Into::into).collect();
        rig.set_problems(ModelRc::new(VecModel::from(problems)));
    }

    pub fn install_driver(&mut self, plugin: &str) {
        self.flows.first_installing.insert(plugin.to_string());
        self.send(Command::InstallDriver(plugin.to_string()));
        self.refresh_plugins();
    }

    /// Updates every driver that has an update (one password: polkit keeps it a while).
    pub fn update_drivers(&mut self) {
        let outdated: Vec<String> = self
            .snapshot
            .plugins
            .iter()
            .filter(|p| p.driver_update.is_some())
            .map(|p| p.id.clone())
            .collect();
        for plugin in outdated {
            self.install_driver(&plugin);
        }
    }

    pub fn remove_driver(&mut self, plugin: &str) {
        self.flows.first_installing.insert(plugin.to_string());
        self.send(Command::RemoveDriver(plugin.to_string()));
        self.refresh_plugins();
    }

    // ------------------------------------------------------------------ settings

    fn refresh_settings(&mut self) {
        let rows: Vec<ui::ExcludedRow> = self
            .snapshot
            .components
            .iter()
            .map(|c| ui::ExcludedRow {
                address: c.component.address.clone().into(),
                label: format!(
                    "{} · {}",
                    c.component.description.kind.label(),
                    c.component.description.name
                )
                .into(),
                detail: format!("{} · {}", c.plugin_name, c.component.device.name).into(),
                excluded: c.component.excluded,
            })
            .collect();
        let usb: Vec<ui::UsbRow> = self
            .snapshot
            .usb
            .iter()
            .filter(|u| u.plugin.is_some() || u.chosen.is_some())
            .map(|u| {
                let name = |id: &str| {
                    self.snapshot
                        .plugins
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| id.to_string())
                };
                let mut options = vec!["Automatic".to_string()];
                options.extend(u.candidates.iter().map(|c| name(c)));
                let selected = u
                    .chosen
                    .as_ref()
                    .and_then(|c| u.candidates.iter().position(|x| x == c))
                    .map(|i| i as i32 + 1)
                    .unwrap_or(0);
                let handled = match &u.plugin {
                    Some(p) => format!("handled by {}", name(p)),
                    None => "not handled".into(),
                };
                ui::UsbRow {
                    key: u.key.clone().into(),
                    name: u.name.clone().into(),
                    detail: format!("USB {:04x}:{:04x} · {handled}", u.vid, u.pid).into(),
                    plugin: u.plugin.clone().unwrap_or_default().into(),
                    device: u.device.clone().into(),
                    options: ModelRc::new(VecModel::from(
                        options
                            .into_iter()
                            .map(Into::into)
                            .collect::<Vec<slint::SharedString>>(),
                    )),
                    selected,
                    procedures: ModelRc::new(VecModel::from(
                        u.procedures
                            .iter()
                            .filter(|p| p.availability.is_supported())
                            .map(|p| ui::DeviceProcedure {
                                id: p.id.clone().into(),
                                label: p.label.clone().into(),
                            })
                            .collect::<Vec<_>>(),
                    )),
                }
            })
            .collect();
        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_usb_devices(ModelRc::new(VecModel::from(usb)));
        rig.set_composition(ModelRc::new(VecModel::from(rows)));
        rig.set_theme_index(self.settings.theme.index());
        rig.set_reduced_motion(self.settings.reduced_motion);
    }

    pub fn set_excluded(&mut self, address: &str, excluded: bool) {
        self.send(Command::SetExcluded {
            address: address.to_string(),
            excluded,
        });
    }

    /// Settings: which plugin handles a USB device (0 = automatic, then the candidates).
    pub fn choose_plugin(&mut self, key: &str, index: i32) {
        let Some(usb) = self.snapshot.usb.iter().find(|u| u.key == key) else {
            return;
        };
        let plugin = usize::try_from(index - 1)
            .ok()
            .and_then(|i| usb.candidates.get(i).cloned());
        self.send(Command::ChoosePlugin {
            device: key.to_string(),
            plugin,
        });
    }

    pub fn start_device_procedure(&mut self, plugin: &str, device: &str, id: &str) {
        self.start_procedure(
            Target::Device {
                plugin: plugin.to_string(),
                device: device.to_string(),
            },
            id,
        );
    }

    pub fn delete_preset(&mut self, name: &str) {
        if self.settings.preset.as_deref() == Some(name) {
            self.settings.preset = None;
            self.save_settings();
            let window = self.window();
            let rig = window.global::<Rig>();
            rig.set_preset("".into());
            rig.set_preset_note("".into());
        }
        self.send(Command::DeletePreset(name.to_string()));
    }

    pub fn set_theme_index(&mut self, index: i32) {
        self.set_theme(ThemeMode::from_index(index));
        self.refresh_settings();
    }

    // ------------------------------------------------------------------ first run

    pub fn needs_first_run(&self) -> bool {
        !self.settings.setup_done
    }

    fn refresh_first_run(&mut self) {
        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_first_step(self.flows.first_step);
        if let Some(system) = &self.snapshot.system {
            let mut checks = vec![ui::SystemCheck {
                label: "Distribution".into(),
                value: system
                    .distro
                    .clone()
                    .unwrap_or_else(|| "Unknown".into())
                    .into(),
                tone: "success".into(),
                ..Default::default()
            }];
            checks.push(ui::SystemCheck {
                label: "Kernel".into(),
                value: system
                    .kernel
                    .clone()
                    .unwrap_or_else(|| "Unknown".into())
                    .into(),
                tone: "success".into(),
                ..Default::default()
            });
            checks.push(match system.secure_boot {
                Some(true) => ui::SystemCheck {
                    label: "Secure Boot".into(),
                    value: "On".into(),
                    tone: "warning".into(),
                    note: "Drivers with a kernel module ask you to confirm a key at the next \
                           restart, once."
                        .into(),
                },
                Some(false) => ui::SystemCheck {
                    label: "Secure Boot".into(),
                    value: "Off".into(),
                    tone: "success".into(),
                    ..Default::default()
                },
                None => ui::SystemCheck {
                    label: "Secure Boot".into(),
                    value: "Not available".into(),
                    tone: "success".into(),
                    ..Default::default()
                },
            });
            let session = [system.desktop.clone(), system.session.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            checks.push(ui::SystemCheck {
                label: "Session".into(),
                value: if session.is_empty() {
                    "Unknown".into()
                } else {
                    session.into()
                },
                tone: "success".into(),
                ..Default::default()
            });
            checks.push(if system.pkexec {
                ui::SystemCheck {
                    label: "Installing drivers".into(),
                    value: "Possible".into(),
                    tone: "success".into(),
                    note: "Asks for your password only when a driver is installed or removed."
                        .into(),
                }
            } else {
                ui::SystemCheck {
                    label: "Installing drivers".into(),
                    value: "Not possible from the app".into(),
                    tone: "error".into(),
                    note: "pkexec is missing: install polkit, or install the drivers from your \
                           distribution."
                        .into(),
                }
            });
            rig.set_system(ModelRc::new(VecModel::from(checks)));
        }
        let found = self.found(false);
        let missing: BTreeSet<String> = self
            .snapshot
            .plugins
            .iter()
            .filter(|p| needs_driver(p) && !p.devices.is_empty())
            .map(|p| p.id.clone())
            .collect();
        rig.set_first_found(ModelRc::new(VecModel::from(found)));
        rig.set_drivers_missing(missing.len() as i32);
        rig.set_first_busy(!self.flows.first_installing.is_empty());
    }

    pub fn first_next(&mut self) {
        self.flows.first_step = (self.flows.first_step + 1).min(2);
        self.refresh_first_run();
    }

    pub fn first_back(&mut self) {
        if self.flows.first_step == 0 {
            return;
        }
        self.flows.first_step -= 1;
        self.refresh_first_run();
    }

    /// One password for every driver the connected devices need.
    pub fn first_install_drivers(&mut self) {
        let missing: Vec<String> = self
            .snapshot
            .plugins
            .iter()
            .filter(|p| needs_driver(p) && !p.devices.is_empty())
            .map(|p| p.id.clone())
            .collect();
        for plugin in missing {
            self.flows.first_installing.insert(plugin.clone());
            self.send(Command::InstallDriver(plugin));
        }
        self.refresh_first_run();
    }

    pub fn first_run_done(&mut self) {
        self.settings.setup_done = true;
        self.save_settings();
        self.flows.first_step = 0;
        self.go(Screen::Home);
    }
}
