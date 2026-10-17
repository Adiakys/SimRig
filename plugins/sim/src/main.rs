//! A complete simulated rig: wheel base, steering wheel, pedals, shifter and handbrake.
//!
//! Inputs move by themselves (waves with optional noise), and items are deliberately
//! unsupported with each kind of reason, so the app and the wizard can be developed and tested
//! without hardware. It also simulates what real hardware does to the app: a rim swap (custom
//! parameter `rim` of the base), a device unplugged (`handbrake_connected`), and a driver
//! (`driver/`) that must be installed before settings can be saved.
//!
//! Options (for tests and multi-brand setups):
//! `--id <plugin id>`, `--devices base,pedals,shifter,handbrake`, `--usb-pedals` (every USB
//! device the host attaches becomes a pedal set).

mod base;
mod handbrake;
mod pedals;
mod procedures;
mod shifter;
mod sim;
mod wheel;

use std::process::ExitCode;

use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use std::collections::BTreeMap;

use crate::sim::{Identity, NO_RIM, Setup, Shared};

pub const RIG: &str = "rig0";
pub const PEDALS: &str = "pedals0";
pub const SHIFTER: &str = "shifter0";
pub const HANDBRAKE: &str = "hb0";
pub const GENERIC: &str = "generic0";

struct Options {
    id: String,
    devices: Vec<String>,
    usb_pedals: bool,
    usb_generic: bool,
}

impl Options {
    fn parse() -> Self {
        let mut options = Options {
            id: "sim".into(),
            devices: ["base", "pedals", "shifter", "handbrake", "generic"]
                .map(String::from)
                .to_vec(),
            usb_pedals: false,
            usb_generic: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--id" => options.id = args.next().unwrap_or(options.id),
                "--devices" => {
                    let list = args.next().unwrap_or_default();
                    options.devices = list
                        .split(',')
                        .filter(|d| !d.is_empty())
                        .map(String::from)
                        .collect();
                }
                "--usb-pedals" => options.usb_pedals = true,
                "--usb-generic" => options.usb_generic = true,
                other => eprintln!("ignoring unknown option {other}"),
            }
        }
        options
    }

    fn has(&self, device: &str) -> bool {
        self.devices.iter().any(|d| d == device)
    }
}

struct SimAdapter {
    options: Options,
    shared: Shared,
    rig_attached: bool,
    /// The rim mounted on the attached base.
    mounted_rim: String,
    /// Attached generic devices and the setup their components reflect.
    generic: BTreeMap<String, Option<Setup>>,
}

impl SimAdapter {
    fn device(id: &str, name: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: name.into(),
            bus: Bus::Virtual,
        }
    }

    /// Restores the settings the simulated driver saved for this component.
    fn restore(&self, mut component: AnyComponent) -> AnyComponent {
        let prefix = format!("{}.", component.id());
        let saved: Vec<(String, String)> = self.shared.get().saved.clone().into_iter().collect();
        for (key, text) in saved {
            if let Some(item) = key.strip_prefix(&prefix) {
                if let Err(e) = component.apply_text(item, &text) {
                    eprintln!("cannot restore {key}: {e}");
                }
            }
        }
        component
    }

    /// The component of a generic device, once it was set up.
    fn generic_component(&self, device: &str, setup: &Setup) -> AnyComponent {
        let s = self.shared.clone();
        let component = match setup {
            Setup::Shifter { gears } => AnyComponent::Shifter(Box::new(shifter::SimShifter::new(
                s,
                Identity::new(
                    format!("{device}/shifter"),
                    "Generic shifter",
                    "sim/generic-shifter",
                ),
                *gears,
            ))),
            Setup::Handbrake => AnyComponent::Handbrake(Box::new(handbrake::SimHandbrake::new(
                s,
                Identity::new(
                    format!("{device}/handbrake"),
                    "Generic handbrake",
                    "sim/generic-handbrake",
                ),
            ))),
        };
        self.restore(component)
    }

    fn wheel(&self, rim: &str) -> Option<AnyComponent> {
        (rim != NO_RIM).then(|| {
            self.restore(AnyComponent::SteeringWheel(Box::new(wheel::SimWheel::new(
                self.shared.clone(),
                rim,
            ))))
        })
    }
}

impl Adapter for SimAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.options.id.clone(),
            name: "Simulated rig".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn discover(&mut self) -> Vec<DeviceInfo> {
        let mut devices = Vec::new();
        if self.options.has("base") {
            devices.push(Self::device(RIG, "Simulated base"));
        }
        if self.options.has("pedals") {
            devices.push(Self::device(PEDALS, "Simulated pedals"));
        }
        if self.options.has("shifter") {
            devices.push(Self::device(SHIFTER, "Simulated shifter"));
        }
        if self.options.has("handbrake") && self.shared.get().handbrake_connected {
            devices.push(Self::device(HANDBRAKE, "Simulated handbrake"));
        }
        if self.options.has("generic") {
            devices.push(Self::device(GENERIC, "Simulated unconfigured USB device"));
        }
        devices
    }

    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
        let s = &self.shared;
        let components = match (&device.bus, device.id.as_str()) {
            (Bus::Virtual, RIG) => {
                self.rig_attached = true;
                self.mounted_rim = s.get().rim.clone();
                let mut list = vec![AnyComponent::WheelBase(Box::new(base::SimBase::new(
                    s.clone(),
                )))];
                list.extend(self.wheel(&self.mounted_rim.clone()));
                list
            }
            (Bus::Virtual, PEDALS) => vec![AnyComponent::Pedals(Box::new(pedals::SimPedals::new(
                s.clone(),
                &format!("{PEDALS}/pedals"),
                "Simulated load-cell pedals",
                "sim/pedals",
            )))],
            (Bus::Virtual, SHIFTER) => {
                vec![AnyComponent::Shifter(Box::new(shifter::SimShifter::new(
                    s.clone(),
                    Identity::new(
                        format!("{SHIFTER}/shifter"),
                        "Simulated H/sequential shifter",
                        "sim/shifter",
                    ),
                    6,
                )))]
            }
            (Bus::Virtual, HANDBRAKE) => vec![AnyComponent::Handbrake(Box::new(
                handbrake::SimHandbrake::new(
                    s.clone(),
                    Identity::new(
                        format!("{HANDBRAKE}/handbrake"),
                        "Simulated handbrake",
                        "sim/handbrake",
                    ),
                ),
            ))],
            (Bus::Virtual, GENERIC) | (Bus::Usb { .. }, _)
                if device.id == GENERIC || self.options.usb_generic =>
            {
                let setup = s.get().setups.get(&device.id).cloned();
                self.generic.insert(device.id.clone(), setup.clone());
                return Ok(setup
                    .map(|setup| self.generic_component(&device.id, &setup))
                    .into_iter()
                    .collect());
            }
            (Bus::Usb { .. }, _) if self.options.usb_pedals => {
                vec![AnyComponent::Pedals(Box::new(pedals::SimPedals::new(
                    s.clone(),
                    &format!("{}/pedals", device.id),
                    &device.name,
                    "sim/usb-pedals",
                )))]
            }
            _ => return Err(Error::NotFound(format!("unknown device '{}'", device.id))),
        };
        Ok(components.into_iter().map(|c| self.restore(c)).collect())
    }

    fn detach(&mut self, device_id: &str) {
        if device_id == RIG {
            self.rig_attached = false;
        }
        self.generic.remove(device_id);
    }

    fn device_procedures(&mut self, device_id: &str) -> Vec<ProcedureDescriptor> {
        match self.generic.get(device_id) {
            Some(setup) => {
                let label = if setup.is_some() {
                    "Set up again"
                } else {
                    "Set up this device"
                };
                vec![ProcedureDescriptor::new("setup", label).help(
                    "Choose what the device is, then map its inputs by moving it when asked.",
                )]
            }
            None => Vec::new(),
        }
    }

    fn start_device_procedure(&mut self, device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        if id != "setup" || !self.generic.contains_key(device_id) {
            return Err(Error::NotFound(format!("unknown procedure '{id}'")));
        }
        Ok(Box::new(procedures::DeviceSetup::new(
            self.shared.clone(),
            device_id,
        )))
    }

    fn take_changes(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        // Generic devices that were (re)configured by their setup procedure.
        let setups = self.shared.get().setups.clone();
        for (device, attached) in self.generic.clone() {
            let current = setups.get(&device).cloned();
            if current == attached {
                continue;
            }
            for kind in ["shifter", "handbrake"] {
                changes.push(Change::ComponentRemoved {
                    component: format!("{device}/{kind}"),
                });
            }
            if let Some(setup) = &current {
                changes.push(Change::ComponentAdded {
                    device: device.clone(),
                    component: self.generic_component(&device, setup),
                });
            }
            self.generic.insert(device, current);
        }
        let rim = self.shared.get().rim.clone();
        if !self.rig_attached || rim == self.mounted_rim {
            return changes;
        }
        if self.mounted_rim != NO_RIM {
            changes.push(Change::ComponentRemoved {
                component: format!("{RIG}/wheel"),
            });
        }
        if let Some(component) = self.wheel(&rim) {
            changes.push(Change::ComponentAdded {
                device: RIG.into(),
                component,
            });
        }
        self.mounted_rim = rim;
        changes
    }
}

fn main() -> ExitCode {
    let shared = Shared::default();
    let mounted_rim = shared.get().rim.clone();
    simrig_plugin_sdk::run(SimAdapter {
        options: Options::parse(),
        shared,
        rig_attached: false,
        mounted_rim,
        generic: BTreeMap::new(),
    })
}
