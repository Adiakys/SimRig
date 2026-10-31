//! One Moza device attached to the plugin: its HID device (inputs and force feedback, through
//! the kernel) and its serial port (settings).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::shifter::Gate;
use simrig_plugin_sdk::prelude::*;

use crate::models::{self, Kind};
use crate::protocol::{self, Command, Target};
use crate::serial::{self, Link};

/// Why the rim's settings are unavailable when no rim answers.
const NO_RIM: &str = "No steering wheel answers on the base's serial port.";

/// How long what is known of the rim holds: rims are taken off and swapped.
const RIM_RECHECK: Duration = Duration::from_secs(5);

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
    pub const HANDBRAKE: u16 = 0x07; // ABS_RUDDER, on its own USB and on a base
    // Pedals plugged into a base.
    pub const BASE_THROTTLE: u16 = 0x02; // ABS_Z
    pub const BASE_BRAKE: u16 = 0x05; // ABS_RZ
    pub const BASE_CLUTCH: u16 = 0x06; // ABS_THROTTLE
    // The analog paddles of the rim mounted on a base.
    pub const LEFT_PADDLE: u16 = 0x04; // ABS_RY
    pub const RIGHT_PADDLE: u16 = 0x03; // ABS_RX
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
    /// The SGP on its own USB: up on button 13, down on 14 (checked on an SGP; Boxflat has them
    /// the other way). Reversing the shift direction in the shifter's own settings swaps them:
    /// mapping by hand puts that right.
    pub const SGP_BUTTONS: [u16; 2] = [0x12c, 0x12d];
    /// The HGP on its own USB: gears 1 to 7 on buttons 6 to 12, reverse on 5.
    pub const HGP_BUTTONS: [u16; 8] = [0x125, 0x126, 0x127, 0x128, 0x129, 0x12a, 0x12b, 0x124];
    /// The SGP plugged into a base, without paddle synchronization: up on button 113, down on
    /// 114 (checked on an SGP). With it, its buttons are the paddles': mapped by hand.
    pub const SGP_ON_BASE: [u16; 2] = [0x2d0, 0x2d1];
    /// The HGP plugged into a base: gears 1 to 7 on buttons 114 to 120, reverse on 113.
    pub const HGP_ON_BASE: [u16; 8] = [0x2d1, 0x2d2, 0x2d3, 0x2d4, 0x2d5, 0x2d6, 0x2d7, 0x2d0];
    /// What can be plugged into a base.
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
    rim: Rim,
    /// A handbrake plugged into the base was seen pulled.
    pub handbrake: bool,
}

/// The rim mounted on the base, as last checked.
enum Rim {
    Unknown,
    /// Answering at a device id; its answers since, missing ones too.
    At {
        id: u8,
        checked: Instant,
        answers: BTreeMap<&'static str, Option<u64>>,
    },
    Absent {
        checked: Instant,
    },
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
            rim: Rim::Unknown,
            handbrake: false,
        };
        device.refresh();
        device
    }

    pub fn name(&self) -> String {
        models::display_name(&self.product)
    }

    /// A part's name: the device's own on its own USB, "<part> on the <base>" plugged into a
    /// base.
    pub fn part_name(&self, part: &str) -> String {
        if self.kind == Kind::Base {
            format!("{part} on the {}", self.name())
        } else {
            self.name()
        }
    }

    /// A part's model: the device's on its own USB; plugged into a base, the base does not say.
    pub fn part_model(&self) -> Option<String> {
        (self.kind != Kind::Base).then(|| models::model_id(&self.product))
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

    /// A shifter plugged into a base shows itself when it is moved. Buttons 113 and 114 are
    /// both the SGP's and the HGP's reverse and first gear: an SGP until a gear only the HGP has
    /// (115 to 120) is engaged.
    fn notice_shifter(&mut self) {
        let hgp = shifters::HGP.layout(&shifters::HGP_ON_BASE);
        if self.kind != Kind::Base || self.hid.shifter_layout.as_ref() == Some(&hgp) {
            return;
        }
        let pressed = |codes: &[u16]| {
            self.hid
                .inputs
                .buttons()
                .into_iter()
                .any(|(code, pressed)| pressed && codes.contains(&code))
        };
        if pressed(&shifters::HGP_ON_BASE[1..7]) {
            self.hid.shifter_layout = Some(hgp);
        } else if self.hid.shifter_layout.is_none() && pressed(&shifters::SGP_ON_BASE) {
            self.hid.shifter_layout = Some(shifters::SGP.layout(&shifters::SGP_ON_BASE));
        }
    }

    /// A handbrake plugged into a base shows itself when it is pulled past half its travel:
    /// the base has the axis whether one is plugged in or not.
    fn notice_handbrake(&mut self) {
        if self.kind != Kind::Base || self.handbrake {
            return;
        }
        let now = self.hid.inputs.axes();
        self.handbrake = now.get(&axes::HANDBRAKE).is_some_and(|a| a.unit() > 0.5);
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
        self.notice_handbrake();
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

    /// The device id the rim mounted on the base answers at, if one does: checked again when
    /// what is known of it is a few seconds old, at the id found last first.
    fn rim(&mut self) -> Option<u8> {
        let last = match &self.rim {
            Rim::At { id, checked, .. } if checked.elapsed() < RIM_RECHECK => return Some(*id),
            Rim::Absent { checked } if checked.elapsed() < RIM_RECHECK => return None,
            Rim::At { id, .. } => Some(*id),
            _ => None,
        };
        // Without the port nothing can be told: checked once it is open.
        self.serial.as_ref()?;
        let probe = protocol::command("wheel-telemetry-mode")?;
        let others = Target::Wheel.ids().iter().copied();
        let found = last
            .into_iter()
            .chain(others.filter(|id| Some(*id) != last))
            .find(|id| self.read(probe, *id).is_some());
        let checked = Instant::now();
        self.rim = match found {
            Some(id) => Rim::At {
                id,
                checked,
                answers: BTreeMap::new(),
            },
            None => Rim::Absent { checked },
        };
        found
    }

    /// The device id a target answers at, or why it cannot be reached.
    fn reach(&mut self, target: Target) -> std::result::Result<u8, Unsupported> {
        if let Some(u) = self.no_settings() {
            return Err(u);
        }
        let id = match target {
            Target::Wheel => self.rim(),
            other => other.ids().first().copied(),
        };
        id.ok_or_else(|| Unsupported::new(Reason::Unavailable).with_detail(NO_RIM))
    }

    /// What the device answered to a command, when it is kept: the base's values until the
    /// port fails, the rim's answers until it is checked again.
    fn kept(&self, command: &Command) -> Option<Option<u64>> {
        match (command.target, &self.rim) {
            (Target::Wheel, Rim::At { answers, .. }) => answers.get(command.name).copied(),
            (Target::Wheel, _) => None,
            _ => self.cache.get(command.name).map(|v| Some(*v)),
        }
    }

    fn keep(&mut self, command: &Command, value: Option<u64>) {
        match (command.target, &mut self.rim) {
            (Target::Wheel, Rim::At { answers, .. }) => {
                answers.insert(command.name, value);
            }
            (Target::Wheel, _) => {}
            // A base that did not answer is asked again.
            _ => {
                if let Some(v) = value {
                    self.cache.insert(command.name, v);
                }
            }
        }
    }

    /// A command's value at a device id; `None` when it does not answer.
    fn read(&mut self, command: &Command, device: u8) -> Option<u64> {
        let (group, id) = command.read?;
        let frame = protocol::read_frame(command, device)?;
        self.serial
            .as_mut()?
            .exchange(&frame, group, device, id)
            .ok()?
    }

    /// A setting's value on the device, or why it cannot be had.
    pub fn value(&mut self, name: &'static str) -> std::result::Result<u64, Unsupported> {
        let command = protocol::command(name).ok_or_else(Unsupported::not_in_driver)?;
        let device = self.reach(command.target)?;
        let value = match self.kept(command) {
            Some(value) => value,
            None => {
                let value = self.read(command, device);
                self.keep(command, value);
                value
            }
        };
        value.ok_or_else(|| {
            Unsupported::new(Reason::Unavailable).with_detail(format!(
                "The {} does not answer for this setting.",
                command.target.noun()
            ))
        })
    }

    pub fn get(&mut self, name: &'static str) -> Option<u64> {
        self.value(name).ok()
    }

    pub fn set(&mut self, name: &'static str, value: u64) -> Result<()> {
        let command = protocol::command(name)
            .ok_or_else(|| Error::NotFound(format!("no command '{name}'")))?;
        let (group, id) = command
            .write
            .ok_or_else(|| Error::failed(format!("'{name}' cannot be written")))?;
        let device = self
            .reach(command.target)
            .map_err(|u| Error::failed(u.explain()))?;
        let frame = protocol::write_frame(command, device, value).ok_or_else(Error::unsupported)?;
        let link = self
            .serial
            .as_mut()
            .ok_or_else(|| Error::failed("No serial port."))?;
        match link.exchange(&frame, group, device, id) {
            Ok(Some(_)) => {
                self.keep(command, Some(value));
                Ok(())
            }
            Ok(None) => Err(Error::failed(format!(
                "The {} did not answer.",
                command.target.noun()
            ))),
            Err(e) => {
                // A port that fails (unplugged, replaced) is opened again at the next refresh.
                self.serial = None;
                self.cache.clear();
                self.rim = Rim::Unknown;
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
