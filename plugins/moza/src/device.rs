//! One Moza device attached to the plugin: its HID device (inputs and force feedback, through
//! the kernel) and its serial port (settings).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::shifter::Gate;
use simrig_plugin_sdk::prelude::*;

use crate::models::{self, Kind};
use crate::protocol;
use crate::serial::{self, Link};

/// The kernel driver of force feedback on Moza bases.
pub const FFB_DRIVER: &str = "hid-universal-pidff";

pub const BINDING: Binding = Binding {
    vendor: 0x346e,
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

/// Moza shifters and the buttons they are read on, as Boxflat numbers them: the first 16 are
/// `BTN_JOYSTICK` onwards, the rest of a base's 128 from `KEY_NEXT_FAVORITE` (0x270), as
/// hid-universal-pidff maps them (the kernel's limit of 80 would lose them).
pub mod shifters {
    use simrig_plugin_kit::shifter::Gate;

    pub const SGP: Gate = Gate::Sequential;
    /// Seven gears and reverse, below the seventh.
    pub const HGP: Gate = Gate::HPattern {
        gears: 7,
        reverse_position: "right_down",
    };
    /// The SGP on its own USB: up on button 14, down on 13. Reversing the shift direction in
    /// the shifter's own settings swaps them: mapping by hand puts that right.
    pub const SGP_BUTTONS: [u16; 2] = [0x12d, 0x12c];
    /// The HGP on its own USB: gears 1 to 7 on buttons 6 to 12, reverse on 5.
    pub const HGP_BUTTONS: [u16; 8] = [0x125, 0x126, 0x127, 0x128, 0x129, 0x12a, 0x12b, 0x124];
    /// The HGP plugged into a base: gears 1 to 7 on buttons 114 to 120, reverse on 113. Only
    /// the HGP uses them, so one of them pressed says it is there.
    pub const HGP_ON_BASE: [u16; 8] = [0x2d1, 0x2d2, 0x2d3, 0x2d4, 0x2d5, 0x2d6, 0x2d7, 0x2d0];
    /// What can be plugged into a base. Which buttons a base gives the SGP is not known.
    pub const ON_BASE: &[Gate] = &[SGP, HGP];
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
        // A pedal set on its own USB has fixed axes (Boxflat's table).
        if kind == Kind::Pedals {
            hid = hid.with_layout(&[
                (PedalRole::Throttle, axes::THROTTLE, false),
                (PedalRole::Brake, axes::BRAKE, false),
                (PedalRole::Clutch, axes::CLUTCH, false),
            ]);
        }
        match kind {
            Kind::Sequential => {
                hid = hid.with_shifter(shifters::SGP.layout(&shifters::SGP_BUTTONS));
            }
            Kind::HPattern => {
                hid = hid.with_shifter(shifters::HGP.layout(&shifters::HGP_BUTTONS));
            }
            _ => {}
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

    /// The kinds of shifter the device is or can have plugged in.
    pub fn gates(&self) -> &'static [Gate] {
        match self.kind {
            Kind::Base => shifters::ON_BASE,
            Kind::Sequential => &[shifters::SGP],
            Kind::HPattern => &[shifters::HGP],
            _ => &[],
        }
    }

    /// An HGP plugged into a base shows itself as soon as a gear is engaged.
    fn notice_shifter(&mut self) {
        if self.kind != Kind::Base || self.hid.shifter_layout.is_some() {
            return;
        }
        let engaged = self
            .hid
            .inputs
            .buttons()
            .into_iter()
            .any(|(code, pressed)| pressed && shifters::HGP_ON_BASE.contains(&code));
        if engaged {
            self.hid.shifter_layout = Some(shifters::HGP.layout(&shifters::HGP_ON_BASE));
        }
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
        let changed = self.hid.refresh();
        self.notice_shifter();
        changed
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
