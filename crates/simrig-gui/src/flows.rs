//! Flows and management screens: add a device, plugins and drivers, general settings and the
//! first run.

use std::collections::BTreeSet;

use slint::{ComponentHandle, ModelRc, VecModel};

use simrig_host::{PluginLocation, PluginState, Target};
use simrig_model::{Bus, ComponentKind, ProcedureDescriptor};

use crate::app::App;
use crate::backend::{Command, PluginView, Snapshot};
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
    /// A USB device assigned to a generic plugin, until the plugin attaches it.
    assigning: Option<String>,
    busy: bool,
    error: String,
    /// What the plugin said when the setup finished.
    pub done_message: String,
    /// The first run: 0 welcome, 1 devices, 2 your rig.
    pub first_step: i32,
    first_installing: BTreeSet<String>,
}

impl Flows {
    /// Devices handled by a plugin but without components yet: they need a setup.
    pub fn devices_to_set_up(&self, snapshot: &Snapshot) -> usize {
        snapshot
            .plugins
            .iter()
            .flat_map(|p| p.devices.iter().map(move |d| (p, d)))
            .filter(|(p, d)| !has_components(snapshot, &p.id, &d.id))
            .count()
    }
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
                    ("driver", format!("{} · needs a driver", plugin.name))
                } else if !set_up {
                    ("setup", format!("{} · to set up", plugin.name))
                } else {
                    ("ready", format!("{} · {kinds}", plugin.name))
                };
                found.push(ui::FoundDevice {
                    key: format!("{}/{}", plugin.id, device.id).into(),
                    name: device.name.clone().into(),
                    detail: detail.into(),
                    state: state.into(),
                    plugin: plugin.id.clone().into(),
                    plugin_name: plugin.name.clone().into(),
                    is_new: !key.is_empty() && !self.flows.known_usb.contains(&key),
                    preview,
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
                found.push(ui::FoundDevice {
                    key: usb.key.clone().into(),
                    name: usb.name.clone().into(),
                    detail: format!(
                        "USB {:04x}:{:04x} · {}",
                        usb.vid,
                        usb.pid,
                        if generic {
                            "no plugin knows it"
                        } else {
                            "no plugin knows it, and no generic plugin is installed"
                        }
                    )
                    .into(),
                    state: if generic { "unknown" } else { "unsupported" }.into(),
                    is_new: new,
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

        // The device being assigned to a generic plugin has been attached: go on.
        if let Some(key) = self.flows.assigning.clone() {
            let attached = self.snapshot.usb.iter().find(|u| u.key == key);
            if let Some(plugin) = attached.and_then(|u| u.plugin.clone()) {
                let device = attached.map(|u| u.device.clone()).unwrap_or_default();
                let name = attached.map(|u| u.name.clone()).unwrap_or_default();
                self.flows.assigning = None;
                self.flows.adding = Some(Adding {
                    plugin,
                    device,
                    name,
                });
                self.add_next();
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
        self.flows.assigning = Some(usb_key.to_string());
        self.flows.error.clear();
        self.send(Command::ChoosePlugin {
            device: usb_key.to_string(),
            plugin: Some(plugin.id.clone()),
        });
        self.refresh_flows();
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
        if plugin.as_ref().is_some_and(needs_driver) {
            self.flows.step = DRIVER;
        } else if !has_components(&self.snapshot, &adding.plugin, &adding.device) {
            self.flows.step = SETUP;
            self.flows.busy = true;
            self.send(Command::ListProcedures(Target::Device {
                plugin: adding.plugin.clone(),
                device: adding.device.clone(),
            }));
        } else {
            self.flows.step = DONE;
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
        let setup = available
            .iter()
            .find(|p| p.id == "setup")
            .or(available.first())
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
                ui::PluginRow {
                    id: p.id.clone().into(),
                    name: p.name.clone().into(),
                    version: p.version.clone().into(),
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
        let window = self.window();
        let rig = window.global::<Rig>();
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
