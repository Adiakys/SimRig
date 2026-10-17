//! The adapter: Thrustmaster wheels as components. A base gives a wheel base, its rim and the
//! pedals plugged into it (once mapped); a wheel starting up, none.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::pedals;
use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use crate::components::{Base, Wheel};
use crate::device::{Shared, TmDevice};
use crate::models;

/// What a device showed last, to notice changes (the driver bound, pedals mapped).
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    driver: Option<String>,
    pedals: Vec<PedalRole>,
    /// The inputs can be read (the event node is given to the user a moment after binding).
    inputs: bool,
    /// Which of the driver's attributes exist (it creates them while it binds).
    exposed: Vec<bool>,
}

pub struct ThrustmasterAdapter {
    config: PathBuf,
    devices: BTreeMap<String, (Shared, Shape)>,
}

impl ThrustmasterAdapter {
    pub fn new(config: PathBuf) -> Self {
        Self {
            config,
            devices: BTreeMap::new(),
        }
    }

    pub fn from_args() -> Self {
        Self::new(simrig_plugin_kit::config_path("thrustmaster"))
    }

    fn shape(device: &Shared) -> Shape {
        let d = device.borrow();
        Shape {
            driver: d.hid.driver(),
            pedals: d.hid.pedal_map.roles(),
            inputs: d.hid.inputs_ready(),
            exposed: d.hid.exposed(&[
                "range",
                "gain",
                "spring_level",
                "damper_level",
                "friction_level",
            ]),
        }
    }

    fn components(device: &Shared) -> Vec<AnyComponent> {
        // A wheel starting up comes back with its own id.
        if device.borrow().model.kind == models::Kind::StartingUp {
            return Vec::new();
        }
        let mut list = vec![
            AnyComponent::WheelBase(Box::new(Base {
                device: device.clone(),
            })),
            AnyComponent::SteeringWheel(Box::new(Wheel {
                device: device.clone(),
            })),
        ];
        if !device.borrow().hid.pedal_map.pedals.is_empty() {
            list.push(AnyComponent::Pedals(Box::new(crate::components::pedals(
                device,
            ))));
        }
        list
    }
}

impl Adapter for ThrustmasterAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "thrustmaster".into(),
            name: "Thrustmaster".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
        let Bus::Usb {
            vid, pid, sysfs, ..
        } = &device.bus
        else {
            return Err(Error::NotFound(format!(
                "'{}' is not a USB device",
                device.id
            )));
        };
        let model = models::model(*pid)
            .filter(|_| *vid == models::VENDOR)
            .ok_or_else(|| Error::NotFound(format!("{vid:04x}:{pid:04x} is not supported")))?;
        let shared: Shared = Rc::new(RefCell::new(TmDevice::new(
            &device.id,
            model,
            PathBuf::from(sysfs),
            self.config.clone(),
        )));
        let shape = Self::shape(&shared);
        self.devices
            .insert(device.id.clone(), (shared.clone(), shape));
        Ok(Self::components(&shared))
    }

    fn detach(&mut self, device_id: &str) {
        self.devices.remove(device_id);
    }

    fn device_procedures(&mut self, device_id: &str) -> Vec<ProcedureDescriptor> {
        self.devices
            .get(device_id)
            .map(|(device, _)| vec![pedals::descriptor(device)])
            .unwrap_or_default()
    }

    fn start_device_procedure(&mut self, device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        let (device, _) = self
            .devices
            .get(device_id)
            .ok_or_else(|| Error::NotFound(format!("unknown device '{device_id}'")))?;
        pedals::start(device, id)
    }

    fn take_changes(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for (id, (device, shape)) in &mut self.devices {
            device.borrow_mut().hid.refresh();
            let now = Self::shape(device);
            if now == *shape {
                continue;
            }
            for kind in ["base", "wheel", "pedals"] {
                changes.push(Change::ComponentRemoved {
                    component: format!("{id}/{kind}"),
                });
            }
            changes.extend(Self::components(device).into_iter().map(|component| {
                Change::ComponentAdded {
                    device: id.clone(),
                    component,
                }
            }));
            *shape = now;
        }
        changes
    }
}
