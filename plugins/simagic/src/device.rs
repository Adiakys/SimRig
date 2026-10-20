//! One Simagic base attached to the plugin: the HID device simagic-ff (or, with an older
//! firmware, the kernel's generic driver) is bound to, and its settings.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::prelude::*;

use crate::models::{self, Model};

const NEEDS_DRIVER: &str = "The base needs simagic-ff: install the Simagic driver with the base \
                            plugged in.";

const EFFECTS: Place = Place::Custom {
    group: "Force feedback",
    advanced: false,
};
const BASE: Place = Place::Custom {
    group: "Base",
    advanced: false,
};
const ADVANCED: Place = Place::Custom {
    group: "Base",
    advanced: true,
};

const fn percent(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting::percent(attr, label, place)
}

/// A game effect's strength: simagic-ff takes 0 to 200%.
const fn game(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        max: Some(200.0),
        ..Setting::percent(attr, label, place)
    }
}

/// simagic-ff's settings (`hid-simagic-sysfs.c`), the rotation apart.
pub const SETTINGS: &[Setting] = &[
    percent(
        "ff_strength",
        "Force feedback strength",
        Place::Standard(WheelBaseParam::FfbStrength),
    ),
    percent(
        "mechanical_damper",
        "Natural damper",
        Place::Standard(WheelBaseParam::NaturalDamper),
    ),
    percent(
        "mechanical_friction",
        "Natural friction",
        Place::Standard(WheelBaseParam::NaturalFriction),
    ),
    percent(
        "mechanical_inertia",
        "Natural inertia",
        Place::Standard(WheelBaseParam::NaturalInertia),
    ),
    game(
        "game_centering",
        "Spring effect strength",
        Place::Standard(WheelBaseParam::SpringStrength),
    ),
    game(
        "game_damper",
        "Damper effect strength",
        Place::Standard(WheelBaseParam::DamperStrength),
    ),
    game("game_friction", "Friction effect strength", EFFECTS),
    game("game_inertia", "Inertia effect strength", EFFECTS),
    Setting {
        help: "Fine detail of the forces: road texture, kerbs.",
        ..percent("feedback_detail", "Feedback detail", EFFECTS)
    },
    Setting {
        unit: "",
        max: Some(20.0),
        help: "Smooths the forces: 0 is the least, 20 the most.",
        ..percent("filter_level", "Force feedback filter", EFFECTS)
    },
    Setting {
        help: "Limits how fast the forces may change.",
        ..percent("slew_rate_control", "Slew rate limit", ADVANCED)
    },
    Setting {
        help: "Pulls the wheel back to the center, even without a game.",
        ..percent("mechanical_centering", "Natural centering", BASE)
    },
    Setting {
        help: "Extra damping around the center.",
        ..percent("center_damper", "Center damper", ADVANCED)
    },
    Setting {
        help: "The highest speed the wheel may spin at by itself.",
        ..percent("wheel_rotation_speed", "Wheel speed limit", ADVANCED)
    },
    Setting {
        unit: "°",
        min: 90.0,
        max: None,
        step: 10.0,
        help: "Where the base stops the wheel, within the rotation.",
        ..percent("angle_lock", "End stop angle", ADVANCED)
    },
    Setting {
        unit: "",
        max: Some(2.0),
        help: "How hard the end stop is: 0 is the softest, 2 the hardest.",
        ..percent("angle_lock_strength", "End stop strength", ADVANCED)
    },
    Setting {
        unit: "",
        max: Some(1.0),
        switch: true,
        ..percent("ring_light_enabled", "Ring light", BASE)
    },
];

/// Every attribute simagic-ff exposes, for the adapter to notice when they show up.
pub const ATTRS: &[&str] = &["max_angle", "ff_strength", "fw_version"];

/// What drives the base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// simagic-ff: force feedback and settings.
    SimagicFf,
    /// A firmware from before v159, on the kernel's generic driver: force feedback, no
    /// settings.
    OldFirmware,
    /// The kernel's generic driver on a newer firmware: inputs only, until simagic-ff is there.
    NeedsDriver,
    /// No HID driver bound.
    Unbound,
}

pub struct SimagicDevice {
    pub hid: HidDevice,
    pub model: Model,
}

pub type Shared = Rc<RefCell<SimagicDevice>>;

impl HasHid for SimagicDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

/// The HID report descriptor declares standard force feedback (a Usage Page item for the PID
/// page, 0x0F): a firmware from before v159.
pub fn declares_pid(descriptor: &[u8]) -> bool {
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xfe {
            // A long item: its size follows.
            let size = descriptor.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        let size = [0, 1, 2, 4][usize::from(prefix & 0x03)];
        let data = descriptor.get(i + 1..i + 1 + size).unwrap_or_default();
        // Usage Page: tag 0, type global.
        if prefix & 0xfc == 0x04 {
            let page = data
                .iter()
                .rev()
                .fold(0u32, |acc, b| (acc << 8) | u32::from(*b));
            if page == 0x0f {
                return true;
            }
        }
        i += 1 + size;
    }
    false
}

impl SimagicDevice {
    pub fn new(id: &str, model: Model, usb: PathBuf, config: PathBuf) -> Self {
        let binding = Binding {
            vendor: model.vid,
            drivers: &["simagic-ff", "hid-generic"],
            rich: &["max_angle", "input"],
            unbound: "No HID driver handles the base: plug it in again.",
        };
        let key = format!("{:04x}-{:04x}", model.vid, model.pid);
        let hid = HidDevice::new(id, usb, binding, config, &key, Some(0));
        Self { hid, model }
    }

    pub fn drive(&self) -> Drive {
        let Some(hid) = &self.hid.hid else {
            return Drive::Unbound;
        };
        if self.hid.driver().as_deref() == Some("simagic-ff") {
            return Drive::SimagicFf;
        }
        let descriptor = std::fs::read(hid.join("report_descriptor")).unwrap_or_default();
        if declares_pid(&descriptor) {
            Drive::OldFirmware
        } else {
            Drive::NeedsDriver
        }
    }

    /// Inputs only on the kernel's generic driver: simagic-ff, unless it does not know the base.
    fn needs_driver(&self) -> Unsupported {
        if self.model.model.is_some() {
            Unsupported::new(Reason::NeedsDriver).with_detail(NEEDS_DRIVER)
        } else {
            Unsupported::not_in_driver().with_detail("simagic-ff does not know this base yet.")
        }
    }

    /// Why settings are unavailable, if they are.
    pub fn no_settings(&self) -> Option<Unsupported> {
        match self.drive() {
            Drive::SimagicFf => None,
            Drive::Unbound => self.hid.unbound(),
            Drive::OldFirmware => Some(Unsupported::not_in_driver().with_detail(
                "With a firmware older than v159 the base works with the kernel's own driver, \
                 which has no settings: set them in Simagic's software, or update the firmware \
                 for SimRig to show them.",
            )),
            Drive::NeedsDriver => Some(self.needs_driver()),
        }
    }

    /// Why force feedback does not work, if it does not.
    pub fn no_force(&self) -> Option<Unsupported> {
        match self.drive() {
            Drive::Unbound => self.hid.unbound(),
            Drive::NeedsDriver => Some(self.needs_driver()),
            _ => (!self.hid.inputs.force_feedback()).then(|| {
                Unsupported::new(Reason::Unavailable)
                    .with_detail("The driver gives the base no force feedback.")
            }),
        }
    }

    /// The rotation the base applies, in degrees.
    pub fn range(&self) -> f64 {
        self.hid
            .read_int("max_angle")
            .map(|r| r as f64)
            .unwrap_or(900.0)
    }

    pub fn rotation() -> Setting {
        Setting {
            max: Some(models::MAX_RANGE),
            ..Setting::rotation("max_angle", 90.0)
        }
    }

    pub fn setting(param: WheelBaseParam) -> Option<Setting> {
        if param == WheelBaseParam::Rotation {
            return Some(Self::rotation());
        }
        SETTINGS
            .iter()
            .find(|s| s.place == Place::Standard(param))
            .copied()
    }

    pub fn custom(id: &str) -> Option<Setting> {
        SETTINGS
            .iter()
            .find(|s| s.custom_id() == id && matches!(s.place, Place::Custom { .. }))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pid_usage_page_tells_the_old_firmware() {
        assert!(declares_pid(&[
            0x05, 0x01, 0x09, 0x04, 0x05, 0x0f, 0x09, 0x92
        ]));
        assert!(declares_pid(&[0x06, 0x0f, 0x00]));
        assert!(!declares_pid(&[0x05, 0x01, 0x09, 0x04, 0x06, 0x00, 0xff]));
        // 05 0F inside another item's data is not a Usage Page.
        assert!(!declares_pid(&[0x27, 0x05, 0x0f, 0x00, 0x00, 0x05, 0x01]));
    }
}
