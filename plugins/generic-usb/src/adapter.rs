//! The adapter: devices the user assigned to this plugin, their setup procedure, and the
//! components they become once set up.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use crate::config::{self, DeviceConfig, HandbrakeInput, Setup};
use crate::decode::State;
use crate::hid::{Reports, Source};
use crate::setup::DeviceSetup;

pub const PLUGIN_ID: &str = "generic-usb";

/// One assigned device: how to read it, its saved setup and the changes not saved yet.
pub struct DeviceState {
    pub id: String,
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub sysfs: PathBuf,
    pub reports: Option<Reports>,
    /// Why the device cannot be read (no permission yet, no HID interface).
    pub read_error: Option<String>,
    /// The setup the driver uses.
    pub saved: Option<DeviceConfig>,
    /// The setup shown in the app: the saved one with the changes not saved yet.
    pub draft: Option<DeviceConfig>,
    /// The kind of components the device has now (to notice a new setup).
    pub attached_kind: Option<&'static str>,
}

pub type Shared = Rc<RefCell<DeviceState>>;

impl DeviceState {
    /// Opens the device's HID interfaces, again if needed (e.g. once reading was allowed). A
    /// set-up device read straight from USB belongs to the driver's service: read through its
    /// relay; the app claims it only while it is not set up.
    pub fn reopen(&mut self) {
        let relay = crate::usbraw::relay_path(&self.file_stem());
        match Reports::open(&self.sysfs, Some(&relay), self.saved.is_none()) {
            Ok(reports) => {
                self.reports = Some(reports);
                self.read_error = None;
            }
            Err(e) => {
                self.reports = None;
                self.read_error = Some(if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "SimRig is not allowed to read this device yet.".into()
                } else {
                    e.to_string()
                });
            }
        }
    }

    /// The live state of the device, decoded with the setup shown in the app.
    pub fn state(&self) -> Option<State> {
        let config = self.draft.as_ref()?;
        let report = self.reports.as_ref()?.latest(config.interface)?;
        Some(config.decode(&report))
    }

    pub fn file_stem(&self) -> String {
        config::file_stem(self.vid, self.pid, self.serial.as_deref())
    }

    /// Saves the setup shown in the app into the driver (a privileged operation: the service
    /// that drives the virtual joystick reads it).
    pub fn save(&mut self, host: &mut HostLink<'_>) -> Result<()> {
        let config = self
            .draft
            .clone()
            .ok_or_else(|| Error::failed("the device is not set up"))?;
        save_config(&config, host)?;
        self.saved = Some(config);
        Ok(())
    }
}

pub fn save_config(config: &DeviceConfig, host: &mut HostLink<'_>) -> Result<()> {
    host.privileged(
        "save-device",
        &[
            ("file", &config.file_stem()),
            ("vid", &format!("{:04x}", config.vid)),
            ("pid", &format!("{:04x}", config.pid)),
            ("config", &config::base64(config.to_toml().as_bytes())),
        ],
    )?;
    Ok(())
}

/// The kind of component a setup makes.
pub fn kind_of(setup: &Setup) -> &'static str {
    match setup {
        Setup::HShifter { .. } | Setup::Sequential { .. } => "shifter",
        Setup::Handbrake(_) => "handbrake",
    }
}

pub struct GenericAdapter {
    /// The driver's data directory: device setups live in `devices/`.
    data: PathBuf,
    devices: BTreeMap<String, Shared>,
}

impl GenericAdapter {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            devices: BTreeMap::new(),
        }
    }

    pub fn from_env() -> Self {
        let data = std::env::var_os("SIMRIG_DRIVER_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("simrig-generic-usb"));
        Self::new(data)
    }

    /// The driver is installed: its install hook leaves this mark in its data directory.
    pub fn driver_installed(data: &Path) -> bool {
        data.join("installed").is_file()
    }

    fn components(state: &Shared) -> Vec<AnyComponent> {
        let s = state.borrow();
        let Some(config) = &s.draft else {
            return Vec::new();
        };
        let component = match &config.setup {
            Setup::HShifter { .. } | Setup::Sequential { .. } => AnyComponent::Shifter(Box::new(
                crate::components::GenericShifter::new(state.clone()),
            )),
            Setup::Handbrake(_) => AnyComponent::Handbrake(Box::new(
                crate::components::GenericHandbrake::new(state.clone()),
            )),
        };
        vec![component]
    }
}

impl Adapter for GenericAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: PLUGIN_ID.into(),
            name: "Generic USB".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
        let Bus::Usb {
            vid,
            pid,
            sysfs,
            serial,
        } = &device.bus
        else {
            return Err(Error::NotFound(format!(
                "'{}' is not a USB device",
                device.id
            )));
        };
        let saved = DeviceConfig::load(
            &config::devices_dir(&self.data),
            *vid,
            *pid,
            serial.as_deref(),
        );
        let mut state = DeviceState {
            id: device.id.clone(),
            name: device.name.clone(),
            vid: *vid,
            pid: *pid,
            serial: serial.clone(),
            sysfs: PathBuf::from(sysfs),
            reports: None,
            read_error: None,
            draft: saved.clone(),
            attached_kind: saved.as_ref().map(|c| kind_of(&c.setup)),
            saved,
        };
        state.reopen();
        let shared = Rc::new(RefCell::new(state));
        self.devices.insert(device.id.clone(), shared.clone());
        Ok(Self::components(&shared))
    }

    fn detach(&mut self, device_id: &str) {
        self.devices.remove(device_id);
    }

    fn device_procedures(&mut self, device_id: &str) -> Vec<ProcedureDescriptor> {
        let Some(state) = self.devices.get(device_id) else {
            return Vec::new();
        };
        let set_up = state.borrow().saved.is_some();
        let mut procedures = vec![setup_descriptor(set_up)];
        if set_up {
            procedures.push(forget_descriptor());
        }
        procedures
    }

    fn start_device_procedure(&mut self, device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        match (self.devices.get(device_id), id) {
            (Some(state), "setup") => Ok(Box::new(DeviceSetup::new(
                state.clone(),
                Self::driver_installed(&self.data),
            ))),
            (Some(state), "forget") => Ok(Box::new(Forget {
                state: state.clone(),
            })),
            _ => Err(Error::NotFound(format!("unknown procedure '{id}'"))),
        }
    }

    fn take_changes(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for state in self.devices.values() {
            let mut s = state.borrow_mut();
            let broken = s.reports.as_ref().is_some_and(|r| !r.errors().is_empty());
            // Set up and claimed by the app: the driver's service takes it from here.
            let handed = s.saved.is_some()
                && s.reports
                    .as_ref()
                    .is_some_and(|r| r.source() == Source::Usb);
            if handed {
                s.reports = None;
            }
            // Not readable yet (no permission, the service starting, a reader that stopped):
            // tried again at every poll.
            if s.reports.is_none() || broken {
                s.reopen();
            }
        }
        for (device, state) in &self.devices {
            let now = state.borrow().draft.as_ref().map(|c| kind_of(&c.setup));
            let before = state.borrow().attached_kind;
            if now == before {
                continue;
            }
            if let Some(kind) = before {
                changes.push(Change::ComponentRemoved {
                    component: format!("{device}/{kind}"),
                });
            }
            changes.extend(Self::components(state).into_iter().map(|component| {
                Change::ComponentAdded {
                    device: device.clone(),
                    component,
                }
            }));
            state.borrow_mut().attached_kind = now;
        }
        changes
    }
}

pub fn setup_descriptor(set_up: bool) -> ProcedureDescriptor {
    let label = if set_up {
        "Set up again"
    } else {
        "Set up this device"
    };
    ProcedureDescriptor::new("setup", label)
        .help("Choose what the device is, then map it by moving it when asked.")
}

/// What the virtual joystick of a setup is called, as games list it.
pub fn joystick_name(config: &DeviceConfig) -> String {
    let what = match &config.setup {
        Setup::HShifter { .. } => "shifter",
        Setup::Sequential { .. } => "sequential shifter",
        Setup::Handbrake(h) => match h.input {
            HandbrakeInput::Axis { .. } if h.mode == "analog" => "handbrake",
            _ => "handbrake (button)",
        },
    };
    format!("SimRig {what}: {}", config.name)
}

pub fn forget_descriptor() -> ProcedureDescriptor {
    ProcedureDescriptor::new("forget", "Forget the setup")
        .help("The driver stops serving the device: games no longer see its joystick.")
}

/// Forgets a device's setup: the driver stops serving it and the device has no components
/// until it is set up again.
pub struct Forget {
    pub state: Shared,
}

impl Procedure for Forget {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        let name = self.state.borrow().name.clone();
        let joystick = self
            .state
            .borrow()
            .saved
            .as_ref()
            .map(joystick_name)
            .unwrap_or_default();
        Ok(Outcome::Step(
            Step::new(&format!("Forget the setup of {name}?"))
                .text(format!(
                    "Games no longer see “{joystick}”, with SimRig closed too. The device can be \
                     set up again at any time."
                ))
                .action(StepAction::new("forget", "Forget", ActionRole::Primary))
                .action(StepAction::cancel()),
        ))
    }

    fn submit(
        &mut self,
        action: &str,
        _values: &BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        if action != "forget" {
            return Ok(Outcome::Cancelled);
        }
        let file = self.state.borrow().file_stem();
        host.privileged("forget-device", &[("file", &file)])?;
        let mut s = self.state.borrow_mut();
        s.saved = None;
        s.draft = None;
        Ok(Outcome::finished(format!(
            "{} is no longer set up: it can be set up again from Add device.",
            s.name
        )))
    }
}
