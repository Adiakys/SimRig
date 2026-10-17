//! One Moza device attached to the plugin: its HID device (inputs and force feedback, through
//! the kernel) and its serial port (settings).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::pedals::{MappedAxis, PedalMap};
use simrig_plugin_sdk::prelude::*;

use crate::models::{self, Kind};
use crate::protocol;
use crate::serial::{self, Link};

/// The kernel driver of force feedback on Moza bases.
pub const FFB_DRIVER: &str = "hid-universal-pidff";

pub const BINDING: Binding = Binding {
    vendor: "346E",
    // Accessories stay on hid-generic; a base without hid-universal-pidff has inputs, no forces.
    drivers: &[FFB_DRIVER, "hid-generic"],
    rich: &["input"],
    unbound: "The device has no HID driver bound: plug it in again.",
};

/// evdev axes of Moza devices, as Boxflat reads them.
pub mod axes {
    pub const STEERING: u16 = 0x00; // ABS_X
    pub const THROTTLE: u16 = 0x03; // ABS_RX
    pub const BRAKE: u16 = 0x04; // ABS_RY
    pub const CLUTCH: u16 = 0x05; // ABS_RZ
    pub const HANDBRAKE: u16 = 0x07; // ABS_RUDDER
    // Pedals plugged into a base.
    pub const BASE_THROTTLE: u16 = 0x02; // ABS_Z
    pub const BASE_BRAKE: u16 = 0x05; // ABS_RZ
    pub const BASE_CLUTCH: u16 = 0x06; // ABS_THROTTLE
}

pub struct MozaDevice {
    pub hid: HidDevice,
    pub kind: Kind,
    pub product: String,
    serial: Option<Box<dyn Link>>,
    /// Why the serial port cannot be used, while it cannot.
    serial_error: Option<String>,
    /// Values read or written, by command: the base answers slowly.
    cache: BTreeMap<&'static str, u64>,
}

pub type Shared = Rc<RefCell<MozaDevice>>;

impl HasHid for MozaDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl MozaDevice {
    pub fn new(id: &str, product: &str, usb: PathBuf, config: PathBuf) -> Self {
        let kind = models::kind(product);
        let steering = (kind == Kind::Base).then_some(axes::STEERING);
        let key = models::model_id(product);
        let mut hid = HidDevice::new(id, usb, BINDING, config, &key, steering);
        if kind == Kind::Base {
            // Pedals plugged into the base (Boxflat's table).
            hid = hid.with_layout(&[
                (PedalRole::Throttle, axes::BASE_THROTTLE, false),
                (PedalRole::Brake, axes::BASE_BRAKE, false),
                (PedalRole::Clutch, axes::BASE_CLUTCH, false),
            ]);
        }
        // A pedal set on its own USB has fixed axes: no mapping needed.
        if kind == Kind::Pedals && hid.pedal_map.pedals.is_empty() {
            hid.pedal_map = PedalMap {
                pedals: [
                    (PedalRole::Throttle, axes::THROTTLE),
                    (PedalRole::Brake, axes::BRAKE),
                    (PedalRole::Clutch, axes::CLUTCH),
                ]
                .into_iter()
                .map(|(role, code)| {
                    (
                        role.key().to_string(),
                        MappedAxis {
                            code,
                            inverted: false,
                        },
                    )
                })
                .collect(),
            };
        }
        let mut device = Self {
            hid,
            kind,
            product: product.into(),
            serial: None,
            serial_error: None,
            cache: BTreeMap::new(),
        };
        device.refresh();
        device
    }

    pub fn name(&self) -> String {
        models::display_name(&self.product)
    }

    /// Looks for the HID driver and the serial port again; true when the HID binding changed.
    pub fn refresh(&mut self) -> bool {
        if self.kind == Kind::Base && self.serial.is_none() {
            match serial::find(&self.hid.usb) {
                None => self.serial_error = Some("The base has no serial port.".into()),
                Some(path) => match serial::open(&path) {
                    Ok(link) => {
                        self.serial = Some(link);
                        self.serial_error = None;
                    }
                    Err(e) => {
                        self.serial_error = Some(format!("Cannot open {}: {e}.", path.display()))
                    }
                },
            }
        }
        self.hid.refresh()
    }

    /// Why settings are unavailable, if they are.
    pub fn no_settings(&self) -> Option<Unsupported> {
        let error = self.serial_error.as_ref()?;
        let reason = if error.contains("ermission denied") {
            // SimRig's udev rules give the user its serial port.
            Reason::NeedsDriver
        } else {
            Reason::Unavailable
        };
        Some(Unsupported::new(reason).with_detail(error))
    }

    /// A setting's value on the device.
    pub fn get(&mut self, name: &'static str) -> Option<u64> {
        if let Some(v) = self.cache.get(name) {
            return Some(*v);
        }
        let command = protocol::command(name)?;
        let (group, id) = command.read?;
        let frame = protocol::read_frame(command)?;
        let value = self
            .serial
            .as_mut()?
            .exchange(&frame, group, command.target, id)
            .ok()??;
        self.cache.insert(command.name, value);
        Some(value)
    }

    pub fn set(&mut self, name: &'static str, value: u64) -> Result<()> {
        let command = protocol::command(name)
            .ok_or_else(|| Error::NotFound(format!("no command '{name}'")))?;
        let (group, id) = command
            .write
            .ok_or_else(|| Error::failed(format!("'{name}' cannot be written")))?;
        let frame = protocol::write_frame(command, value).ok_or_else(Error::unsupported)?;
        let link = self.serial.as_mut().ok_or_else(|| {
            Error::failed(
                self.serial_error
                    .clone()
                    .unwrap_or_else(|| "No serial port.".into()),
            )
        })?;
        match link.exchange(&frame, group, command.target, id) {
            Ok(Some(_)) => {
                self.cache.insert(command.name, value);
                Ok(())
            }
            Ok(None) => Err(Error::failed("The base did not answer.")),
            Err(e) => {
                // A port that fails (unplugged, replaced) is opened again at the next refresh.
                self.serial = None;
                self.cache.clear();
                Err(Error::failed(e.to_string()))
            }
        }
    }

    /// Force feedback works only with the PIDFF driver bound to the base.
    pub fn no_force(&self) -> Option<Unsupported> {
        if let Some(u) = self.hid.unbound() {
            return Some(u);
        }
        (self.hid.driver().as_deref() != Some(FFB_DRIVER)).then(|| {
            Unsupported::new(Reason::NeedsDriver).with_detail(
                "Force feedback needs the hid-universal-pidff driver (in the kernel from 6.15).",
            )
        })
    }
}
