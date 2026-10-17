//! One Thrustmaster wheel attached to the plugin: the HID device hid-tmff2 is bound to.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::prelude::WheelBaseParam;

use crate::models::{Kind, Model};

pub const BINDING: Binding = Binding {
    vendor: "044F",
    drivers: &["tmff2"],
    rich: &["range", "input"],
    unbound: "The hid-tmff2 driver is not installed, or not bound to the wheel.",
};

/// hid-tmff2's settings (the same attributes on every wheel).
pub const SETTINGS: &[Setting] = &[
    Setting {
        // The 16-bit gain, shown as a percentage.
        scale: 655.35,
        ..Setting::percent(
            "gain",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        )
    },
    Setting::percent(
        "spring_level",
        "Spring effect strength",
        Place::Standard(WheelBaseParam::SpringStrength),
    ),
    Setting::percent(
        "damper_level",
        "Damper effect strength",
        Place::Standard(WheelBaseParam::DamperStrength),
    ),
    Setting::percent(
        "friction_level",
        "Friction effect strength",
        Place::Custom {
            group: "Force feedback",
            advanced: false,
        },
    ),
];

pub struct TmDevice {
    pub hid: HidDevice,
    pub model: &'static Model,
}

pub type Shared = Rc<RefCell<TmDevice>>;

impl HasHid for TmDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl TmDevice {
    pub fn new(id: &str, model: &'static Model, usb: PathBuf, config: PathBuf) -> Self {
        let mut hid = HidDevice::new(
            id,
            usb,
            BINDING,
            config,
            &format!("{:04x}", model.pid),
            Some(0),
        );
        if let Some(layout) = model.pedals {
            hid = hid.with_layout(layout);
        }
        Self { hid, model }
    }

    pub fn max_range(&self) -> f64 {
        match self.model.kind {
            Kind::Wheel { max_range, .. } => f64::from(max_range),
            Kind::StartingUp => 900.0,
        }
    }

    /// The rotation, between this wheel's limits.
    pub fn rotation(&self) -> Setting {
        let min = match self.model.kind {
            Kind::Wheel { min_range, .. } => f64::from(min_range),
            Kind::StartingUp => 40.0,
        };
        Setting::rotation("range", min)
    }

    pub fn setting(&self, param: WheelBaseParam) -> Option<Setting> {
        if param == WheelBaseParam::Rotation {
            return Some(self.rotation());
        }
        SETTINGS
            .iter()
            .find(|s| s.place == Place::Standard(param))
            .copied()
    }
}
