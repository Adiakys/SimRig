//! One Thrustmaster wheel attached to the plugin: the HID device hid-tmff2 is bound to.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_kit::input::FfControl;
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::prelude::WheelBaseParam;

use crate::models::{Driver, Kind, Model};

const fn binding(drivers: &'static [&'static str], unbound: &'static str) -> Binding {
    Binding {
        vendor: 0x044f,
        drivers,
        rich: &["range", "input"],
        unbound,
    }
}

const TMFF2: Binding = binding(
    &["tmff2"],
    "The hid-tmff2 driver is not installed, or not bound to the wheel.",
);
const T150: Binding = binding(
    &["hid-t150"],
    "The hid-t150 driver is not installed, or not bound to the wheel: install the Thrustmaster \
     driver with the wheel plugged in.",
);
const GENERIC: Binding = binding(
    &["hid-generic"],
    "No HID driver handles the wheel: plug it in again.",
);

const EFFECTS: Place = Place::Custom {
    group: "Force feedback",
    advanced: false,
};

/// The 16-bit gain, shown as a percentage; the input interface's when there is no attribute.
const GAIN: Setting = Setting {
    scale: 655.35,
    ff: Some(FfControl::Gain),
    ..Setting::percent(
        "gain",
        "Force feedback strength",
        Place::Standard(WheelBaseParam::FfbStrength),
    )
};

/// The 16-bit centering spring; the input interface's when there is no attribute.
const AUTOCENTER: Setting = Setting {
    scale: 655.35,
    ff: Some(FfControl::Autocenter),
    help: "Pulls the wheel back to the center; games usually set it themselves.",
    missing: "Not taken by the installed driver.",
    ..Setting::percent("autocenter", "Centering spring", EFFECTS)
};

/// hid-tmff2's settings (the same attributes on every wheel); the centering spring through the
/// input interface.
pub const TMFF2_SETTINGS: &[Setting] = &[
    GAIN,
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
    Setting::percent("friction_level", "Friction effect strength", EFFECTS),
    AUTOCENTER,
];

/// hid-t150's (besides the rotation).
pub const T150_SETTINGS: &[Setting] = &[GAIN, AUTOCENTER];

/// Every attribute a driver may expose, for the adapter to notice when they show up.
pub const ATTRS: &[&str] = &[
    "range",
    "gain",
    "autocenter",
    "spring_level",
    "damper_level",
    "friction_level",
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
            match model.driver {
                Driver::Tmff2 => TMFF2,
                Driver::T150 => T150,
                Driver::Generic { .. } => GENERIC,
            },
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

    /// The rotation, between this wheel's limits, when its driver sets it.
    pub fn rotation(&self) -> Option<Setting> {
        let min = match self.model.kind {
            Kind::Wheel { min_range, .. } => f64::from(min_range),
            Kind::StartingUp => 40.0,
        };
        (!matches!(self.model.driver, Driver::Generic { .. }))
            .then(|| Setting::rotation("range", min))
    }

    /// The settings of this wheel's driver, the rotation apart.
    pub fn settings(&self) -> Vec<Setting> {
        match self.model.driver {
            Driver::Tmff2 => TMFF2_SETTINGS.to_vec(),
            Driver::T150 => T150_SETTINGS.to_vec(),
            // Through the input interface only, if a driver ever takes them.
            Driver::Generic { why } => [GAIN, AUTOCENTER]
                .map(|s| Setting { missing: why, ..s })
                .to_vec(),
        }
    }

    pub fn setting(&self, param: WheelBaseParam) -> Option<Setting> {
        if param == WheelBaseParam::Rotation {
            return self.rotation();
        }
        self.settings()
            .into_iter()
            .find(|s| s.place == Place::Standard(param))
    }

    pub fn custom(&self, id: &str) -> Option<Setting> {
        self.settings()
            .into_iter()
            .find(|s| s.custom_id() == id && matches!(s.place, Place::Custom { .. }))
    }
}
