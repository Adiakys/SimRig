//! The adapter: Fanatec devices handled by hid-fanatecff, as components. A base gives a wheel
//! base, the mounted rim and the pedals plugged into it; a USB pedal set gives pedals.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use simrig_plugin_kit::pedals;

use crate::components::{Base, Wheel};
use crate::device::{FanatecDevice, Shared};
use crate::models::{self, Kind};

/// What a device showed last, to notice changes (a rim swap, the driver bound, pedals mapped).
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    bound: bool,
    rim: Option<u16>,
    pedals: Vec<PedalRole>,
}

pub struct FanatecAdapter {
    config: PathBuf,
    devices: BTreeMap<String, (Shared, Shape)>,
}

impl FanatecAdapter {
    pub fn new(config: PathBuf) -> Self {
        Self {
            config,
            devices: BTreeMap::new(),
        }
    }

    /// `--config <file>`, else `$XDG_CONFIG_HOME/simrig/fanatec.toml`.
    pub fn from_args() -> Self {
        Self::new(simrig_plugin_kit::config_path("fanatec"))
    }

    fn shape(device: &Shared) -> Shape {
        let d = device.borrow();
        Shape {
            bound: d.hid.hid.is_some(),
            rim: d.wheel_id(),
            pedals: d.hid.pedal_map.roles(),
        }
    }

    fn components(device: &Shared) -> Vec<AnyComponent> {
        let d = device.borrow();
        match d.model.kind {
            Kind::Base { .. } => {
                let mut list = vec![AnyComponent::WheelBase(Box::new(Base {
                    device: device.clone(),
                }))];
                if let Some(rim) = d.wheel_id() {
                    list.push(AnyComponent::SteeringWheel(Box::new(Wheel {
                        device: device.clone(),
                        rim,
                    })));
                }
                // Pedals plugged into the base, once mapped (until then, a procedure).
                if !d.hid.pedal_map.pedals.is_empty() {
                    list.push(AnyComponent::Pedals(Box::new(crate::components::pedals(
                        device,
                    ))));
                }
                list
            }
            Kind::Pedals if !d.hid.pedal_map.pedals.is_empty() => {
                vec![AnyComponent::Pedals(Box::new(crate::components::pedals(
                    device,
                )))]
            }
            // A pedal set not mapped yet: set up by its procedure.
            Kind::Pedals => Vec::new(),
        }
    }
}

impl Adapter for FanatecAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "fanatec".into(),
            name: "Fanatec".into(),
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
        let shared: Shared = Rc::new(RefCell::new(FanatecDevice::new(
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
            // Simplest and always right: the device's components again.
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
