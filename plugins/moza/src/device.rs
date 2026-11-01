//! One Moza device attached to the plugin: its HID device (inputs and force feedback, through
//! the kernel) and its serial port (settings).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::shifter::{Gate, ShifterMap};
use simrig_plugin_sdk::prelude::*;

use crate::models::{self, Kind};
use crate::protocol::{self, Command, Target};
use crate::serial::{self, Link};

/// Why the rim's settings are unavailable when no rim answers.
const NO_RIM: &str = "No steering wheel answers on the base's serial port (device ids 23, 21 and 19 \
                      asked).";

/// How long what is known of the rim holds: rims are taken off and swapped.
const RIM_RECHECK: Duration = Duration::from_secs(5);

/// How long a base without a rim answering is left before asking again: asking every id takes
/// seconds.
const NO_RIM_RECHECK: Duration = Duration::from_secs(30);

/// How often the base is asked what is plugged into it: often while a part answers, seldom
/// while none does (each silent part costs a wait).
const PARTS_ASKED: Duration = Duration::from_secs(3);
const NO_PARTS_ASKED: Duration = Duration::from_secs(30);

/// How long a rim has to answer when it is looked for: through the base it can take longer than
/// the base itself (Boxflat waits until its next poll, seconds later).
const RIM_ANSWER: Duration = Duration::from_secs(1);

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
    /// 114 (checked on an SGP).
    pub const SGP_ON_BASE: [u16; 2] = [0x2d0, 0x2d1];
    /// With paddle synchronization, the paddles' buttons: up on 14, down on 13 (Boxflat's
    /// `MOZA_GEAR_UP` and `MOZA_GEAR_DOWN`, not checked).
    pub const SGP_PADDLES: [u16; 2] = [0x12d, 0x12c];
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
    /// Parts plugged into the base seen in use since the plugin started: the base has their
    /// inputs whether they are plugged in or not.
    seen: Parts,
    /// Parts that answered on the base's serial port at least once: then the port tells.
    answered: Parts,
    /// Parts answering on the base's serial port when last asked.
    plugged: Parts,
    /// When the base was last asked what is plugged into it.
    parts_asked: Option<Instant>,
    /// The shifter mapping made by hand when the plugin started: one made since says the
    /// shifter is there.
    mapped: Option<ShifterMap>,
}

/// One flag for each part a base can have plugged in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parts {
    pub shifter: bool,
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
            seen: Parts::default(),
            answered: Parts::default(),
            plugged: Parts::default(),
            parts_asked: None,
            mapped: None,
        };
        device.mapped = device.hid.shifter_map.clone();
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

    /// The parts plugged into a base to show: those answering on its serial port, and those
    /// seen in use (a part that answered before and is used again shows at once, until the port
    /// says otherwise).
    pub fn parts(&self) -> Parts {
        Parts {
            shifter: self.plugged.shifter || self.seen.shifter,
            handbrake: self.plugged.handbrake || self.seen.handbrake,
        }
    }

    /// A part that answered before, seen in use again while it was not answering: the base is
    /// asked at once.
    fn used_again(&mut self, answered: bool, plugged: bool) {
        if answered && !plugged {
            self.parts_asked = None;
        }
    }

    /// Asks the base's serial port which parts are plugged in, as Boxflat does: a shifter
    /// answers at 26 with its lever's height (past 100 an HGP), its paddle synchronization and
    /// direction; a handbrake at 27.
    fn ask_parts(&mut self) {
        if self.kind != Kind::Base || self.serial.is_none() {
            return;
        }
        let any = self.plugged.shifter || self.plugged.handbrake;
        let every = if any { PARTS_ASKED } else { NO_PARTS_ASKED };
        if self.parts_asked.is_some_and(|at| at.elapsed() < every) {
            return;
        }
        self.parts_asked = Some(Instant::now());
        let ask = |d: &mut Self, name: &str| {
            let command = protocol::command(name)?;
            d.read(command, command.target.ids()[0], serial::ANSWER)
        };
        let lever = ask(self, "sequential-output-y");
        self.plugged.shifter = lever.is_some();
        // A part that answered before and no longer does was unplugged.
        if lever.is_none() && self.answered.shifter {
            self.seen.shifter = false;
        }
        if let Some(height) = lever {
            self.answered.shifter = true;
            self.hid.shifter_layout = Some(if height > 100 {
                shifters::HGP.layout(&shifters::HGP_ON_BASE)
            } else {
                let paddles = ask(self, "sequential-paddle-sync") == Some(2);
                let reversed = ask(self, "sequential-direction") == Some(1);
                let [up, down] = if paddles {
                    shifters::SGP_PADDLES
                } else {
                    shifters::SGP_ON_BASE
                };
                shifters::SGP.layout(&if reversed { [down, up] } else { [up, down] })
            });
        }
        self.plugged.handbrake = ask(self, "handbrake-direction").is_some();
        if !self.plugged.handbrake && self.answered.handbrake {
            self.seen.handbrake = false;
        }
        self.answered.handbrake |= self.plugged.handbrake;
    }

    /// A shifter plugged into a base shows itself when it is moved. Until it answers on the
    /// serial port, which tells its kind, buttons 113 and 114 are both the SGP's and the HGP's
    /// reverse and first gear: an SGP until a gear only the HGP has (115 to 120) is engaged.
    fn notice_shifter(&mut self) {
        if self.kind != Kind::Base {
            return;
        }
        if self.hid.shifter_map != self.mapped {
            self.seen.shifter = true;
        }
        let hgp = shifters::HGP.layout(&shifters::HGP_ON_BASE);
        let known = self.hid.shifter_map.is_some()
            || self.answered.shifter
            || self.hid.shifter_layout.as_ref() == Some(&hgp);
        if self.seen.shifter && known {
            return;
        }
        let pressed: Vec<u16> = self
            .hid
            .inputs
            .buttons()
            .into_iter()
            .filter_map(|(code, pressed)| pressed.then_some(code))
            .collect();
        let any = |codes: &[u16]| codes.iter().any(|c| pressed.contains(c));
        if !self.answered.shifter && self.hid.shifter_layout.as_ref() != Some(&hgp) {
            if any(&shifters::HGP_ON_BASE[1..7]) {
                self.hid.shifter_layout = Some(hgp);
            } else if self.hid.shifter_layout.is_none() && any(&shifters::SGP_ON_BASE) {
                self.hid.shifter_layout = Some(shifters::SGP.layout(&shifters::SGP_ON_BASE));
            }
        }
        // Mapped by hand or known, it is there once one of its buttons is pressed.
        let buttons: Vec<u16> = self
            .hid
            .shifter()
            .into_iter()
            .flat_map(|m| m.into_values())
            .collect();
        if any(&buttons) {
            self.seen.shifter = true;
            self.used_again(self.answered.shifter, self.plugged.shifter);
        }
    }

    /// A handbrake plugged into a base shows itself when it is pulled past half its travel:
    /// the base has the axis whether one is plugged in or not.
    fn notice_handbrake(&mut self) {
        if self.kind != Kind::Base || self.seen.handbrake {
            return;
        }
        let now = self.hid.inputs.axes();
        self.seen.handbrake = now.get(&axes::HANDBRAKE).is_some_and(|a| a.unit() > 0.5);
        if self.seen.handbrake {
            self.used_again(self.answered.handbrake, self.plugged.handbrake);
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
        self.ask_parts();
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
            Rim::Absent { checked } if checked.elapsed() < NO_RIM_RECHECK => return None,
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
            .find(|id| self.read(probe, *id, RIM_ANSWER).is_some());
        // What the same rim did not answer stays unanswered: asking again costs a wait each.
        let missing = match (&self.rim, found) {
            (Rim::At { id, answers, .. }, Some(again)) if *id == again => answers
                .iter()
                .filter(|(_, v)| v.is_none())
                .map(|(k, v)| (*k, *v))
                .collect(),
            _ => BTreeMap::new(),
        };
        let checked = Instant::now();
        self.rim = match found {
            Some(id) => Rim::At {
                id,
                checked,
                answers: missing,
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

    /// A command's value at a device id; `None` when it does not answer within `wait`.
    fn read(&mut self, command: &Command, device: u8, wait: Duration) -> Option<u64> {
        let (group, id) = command.read?;
        let frame = protocol::read_frame(command, device)?;
        self.serial
            .as_mut()?
            .exchange(&frame, (group, device, id), wait)
            .ok()?
    }

    /// A setting's value on the device, or why it cannot be had.
    pub fn value(&mut self, name: &'static str) -> std::result::Result<u64, Unsupported> {
        let command = protocol::command(name).ok_or_else(Unsupported::not_in_driver)?;
        let device = self.reach(command.target)?;
        let value = match self.kept(command) {
            Some(value) => value,
            None => {
                let value = self.read(command, device, serial::ANSWER);
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
        match link.exchange(&frame, (group, device, id), serial::ANSWER) {
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
