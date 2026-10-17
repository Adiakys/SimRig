//! The wheel settings SimRig shows, over the drivers' sysfs attributes.

use simrig_plugin_sdk::prelude::WheelBaseParam;

use crate::models::Family;

#[derive(Clone, Copy, Debug)]
pub enum Place {
    Standard(WheelBaseParam),
    Custom { group: &'static str, advanced: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// The sysfs attribute of the HID device.
    pub attr: &'static str,
    pub family: Family,
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f64,
    /// `None`: the wheel's widest rotation.
    pub max: Option<f64>,
    pub step: f64,
    /// The attribute's value for one unit shown.
    pub scale: f64,
    /// An on/off switch (0 or 1).
    pub switch: bool,
    pub place: Place,
    pub help: &'static str,
    /// Why the attribute may be missing.
    pub missing: &'static str,
}

const NEW_LG4FF: &str = "Needs new-lg4ff, which the Logitech driver builds for this wheel.";

const fn percent(attr: &'static str, family: Family, label: &'static str, place: Place) -> Setting {
    Setting {
        attr,
        family,
        label,
        unit: "%",
        min: 0.0,
        max: Some(100.0),
        step: 1.0,
        scale: 1.0,
        switch: false,
        place,
        help: "",
        missing: "Not exposed by the installed driver.",
    }
}

const fn rotation(attr: &'static str, family: Family, min: f64) -> Setting {
    Setting {
        unit: "°",
        min,
        max: None,
        step: 10.0,
        ..percent(
            attr,
            family,
            "Rotation",
            Place::Standard(WheelBaseParam::Rotation),
        )
    }
}

/// The 16-bit levels of new-lg4ff, shown as percentages.
const fn lg4ff_level(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        scale: 655.35,
        missing: NEW_LG4FF,
        ..percent(attr, Family::Lg4ff, label, place)
    }
}

const fn lg4ff_percent(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        missing: NEW_LG4FF,
        ..percent(attr, Family::Lg4ff, label, place)
    }
}

const EFFECTS: Place = Place::Custom {
    group: "Force feedback",
    advanced: false,
};

pub const SETTINGS: &[Setting] = &[
    // lg4ff wheels: the kernel's driver has the rotation only; new-lg4ff the rest.
    rotation("range", Family::Lg4ff, 40.0),
    lg4ff_level(
        "gain",
        "Force feedback strength",
        Place::Standard(WheelBaseParam::FfbStrength),
    ),
    lg4ff_percent(
        "spring_level",
        "Spring effect strength",
        Place::Standard(WheelBaseParam::SpringStrength),
    ),
    lg4ff_percent(
        "damper_level",
        "Damper effect strength",
        Place::Standard(WheelBaseParam::DamperStrength),
    ),
    lg4ff_percent("friction_level", "Friction effect strength", EFFECTS),
    Setting {
        help: "Pulls the wheel back to the center; games usually set it themselves.",
        ..lg4ff_level("autocenter", "Centering spring", EFFECTS)
    },
    Setting {
        unit: "",
        max: Some(1.0),
        switch: true,
        help: "The wheel's LEDs show how strong the forces are, and when they clip.",
        place: Place::Custom {
            group: "Force feedback",
            advanced: true,
        },
        ..lg4ff_percent("ffb_leds", "Force feedback on the LEDs", EFFECTS)
    },
    // G920, G923 for Xbox: the kernel's hid-logitech-hidpp has the rotation only.
    rotation("range", Family::Hidpp, 180.0),
    // G PRO, RS50: the TrueForce driver.
    rotation("wheel_range", Family::DirectDrive, 90.0),
    percent(
        "wheel_strength",
        Family::DirectDrive,
        "Force feedback strength",
        Place::Standard(WheelBaseParam::FfbStrength),
    ),
    percent(
        "wheel_damping",
        Family::DirectDrive,
        "Natural damper",
        Place::Standard(WheelBaseParam::NaturalDamper),
    ),
    Setting {
        help: "Engine and road texture in the rim, on top of the force feedback.",
        ..percent("wheel_trueforce", Family::DirectDrive, "TrueForce", EFFECTS)
    },
    Setting {
        unit: "",
        min: 1.0,
        max: Some(15.0),
        help: "Smooths the forces: 1 is the least, 15 the most.",
        ..percent(
            "wheel_ffb_filter",
            Family::DirectDrive,
            "Force feedback filter",
            EFFECTS,
        )
    },
];

pub fn standard(family: Family, param: WheelBaseParam) -> Option<&'static Setting> {
    SETTINGS
        .iter()
        .find(|s| s.family == family && matches!(s.place, Place::Standard(p) if p == param))
}

pub fn custom_id(setting: &Setting) -> String {
    setting.attr.trim_start_matches("wheel_").to_string()
}

pub fn find_custom(family: Family, id: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| {
        s.family == family && custom_id(s) == id && matches!(s.place, Place::Custom { .. })
    })
}
