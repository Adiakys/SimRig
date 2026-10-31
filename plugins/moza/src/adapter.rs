//! The adapter: Moza devices as components. A base gives a wheel base, the pedals plugged into
//! it and a shifter plugged into it (an HGP once a gear is engaged, any once mapped by hand);
//! pedal sets, handbrakes and shifters on their own USB give theirs.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use simrig_plugin_kit::shifter::ShifterMap;
use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use crate::components::{self, Base, Handbrake};
use crate::device::{MozaDevice, Shared};
use crate::models::{self, Kind};

/// What a device showed last, to notice changes.
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    driver: Option<String>,
    settings: bool,
    pedals: Vec<PedalRole>,
    shifter: Option<ShifterMap>,
    /// The inputs can be read (the event node is given to the user a moment after binding).
    inputs: bool,
}

pub struct MozaAdapter {
    config: PathBuf,
    devices: BTreeMap<String, (Shared, Shape)>,
}

impl MozaAdapter {
    pub fn new(config: PathBuf) -> Self {
        Self {
            config,
            devices: BTreeMap::new(),
        }
    }

    pub fn from_args() -> Self {
        Self::new(simrig_plugin_kit::config_path("moza"))
    }

    fn shape(device: &Shared) -> Shape {
        let d = device.borrow();
        Shape {
            driver: d.hid.driver(),
            settings: d.no_settings().is_none(),
            pedals: d.hid.pedal_map.roles(),
            shifter: d.hid.shifter(),
            inputs: d.hid.inputs_ready(),
        }
    }

    fn components(device: &Shared) -> Vec<AnyComponent> {
        let d = device.borrow();
        let mapped = !d.hid.pedal_map.pedals.is_empty();
        let pedals = || AnyComponent::Pedals(Box::new(components::pedals(device)));
        let shifter = || AnyComponent::Shifter(Box::new(components::shifter(device)));
        match d.kind {
            Kind::Base => {
                let mut list = vec![AnyComponent::WheelBase(Box::new(Base {
                    device: device.clone(),
                }))];
                if mapped {
                    list.push(pedals());
                }
                // A shifter plugged into the base, once it is known to be there.
                if d.hid.shifter().is_some() {
                    list.push(shifter());
                }
                list
            }
            Kind::Pedals => vec![pedals()],
            Kind::Handbrake => vec![AnyComponent::Handbrake(Box::new(Handbrake {
                device: device.clone(),
            }))],
            Kind::Sequential | Kind::HPattern => vec![shifter()],
            Kind::Other => Vec::new(),
        }
    }
}

fn product(usb: &Path) -> String {
    std::fs::read_to_string(usb.join("product"))
        .map(|p| p.trim().to_string())
        .unwrap_or_default()
}

impl Adapter for MozaAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "moza".into(),
            name: "Moza".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
        let Bus::Usb { vid, sysfs, .. } = &device.bus else {
            return Err(Error::NotFound(format!(
                "'{}' is not a USB device",
                device.id
            )));
        };
        if *vid != models::VENDOR {
            return Err(Error::NotFound(format!("{vid:04x} is not Moza")));
        }
        let usb = PathBuf::from(sysfs);
        let shared: Shared = Rc::new(RefCell::new(MozaDevice::new(
            &device.id,
            &product(&usb),
            usb,
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
        match self.devices.get(device_id) {
            Some((device, _)) if device.borrow().kind == Kind::Base => {
                components::procedures(device)
            }
            _ => Vec::new(),
        }
    }

    fn start_device_procedure(&mut self, device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        let (device, _) = self
            .devices
            .get(device_id)
            .ok_or_else(|| Error::NotFound(format!("unknown device '{device_id}'")))?;
        components::start(device, id)
    }

    fn take_changes(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for (id, (device, shape)) in &mut self.devices {
            device.borrow_mut().refresh();
            let now = Self::shape(device);
            if now == *shape {
                continue;
            }
            for kind in ["base", "pedals", "handbrake", "shifter"] {
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
