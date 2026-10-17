//! The adapter: Fanatec devices handled by hid-fanatecff, as components. A base gives a wheel
//! base, the mounted rim and the pedals plugged into it; a USB pedal set gives pedals.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use crate::components::{Base, Pedals, Wheel};
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
        let mut args = std::env::args().skip(1);
        let mut config = None;
        while let Some(arg) = args.next() {
            if arg == "--config" {
                config = args.next().map(PathBuf::from);
            }
        }
        let config = config.unwrap_or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .unwrap_or_else(std::env::temp_dir)
                .join("simrig/fanatec.toml")
        });
        Self::new(config)
    }

    fn shape(device: &Shared) -> Shape {
        let d = device.borrow();
        Shape {
            bound: d.paths.is_some(),
            rim: d.wheel_id(),
            pedals: d.pedal_map.roles(),
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
                if !d.pedal_map.pedals.is_empty() {
                    list.push(AnyComponent::Pedals(Box::new(Pedals {
                        device: device.clone(),
                        on_base: true,
                    })));
                }
                list
            }
            Kind::Pedals if !d.pedal_map.pedals.is_empty() => {
                vec![AnyComponent::Pedals(Box::new(Pedals {
                    device: device.clone(),
                    on_base: false,
                }))]
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
            .map(|(device, _)| vec![crate::components::map_pedals_descriptor(device)])
            .unwrap_or_default()
    }

    fn start_device_procedure(&mut self, device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        match (self.devices.get(device_id), id) {
            (Some((device, _)), "map-pedals") => Ok(Box::new(crate::components::MapPedals::new(
                device.clone(),
                matches!(device.borrow().model.kind, Kind::Base { .. }),
            ))),
            _ => Err(Error::NotFound(format!("unknown procedure '{id}'"))),
        }
    }

    fn take_changes(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for (id, (device, shape)) in &mut self.devices {
            device.borrow_mut().refresh();
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
