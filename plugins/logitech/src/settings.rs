//! The wheel settings SimRig shows, over the drivers' sysfs attributes, by driver family.

use simrig_plugin_kit::input::FfControl;
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::prelude::WheelBaseParam;

use crate::models::Family::{self, *};

const NEW_LG4FF: &str = "Needs new-lg4ff, which the Logitech driver builds for this wheel.";

const EFFECTS: Place = Place::Custom {
    group: "Force feedback",
    advanced: false,
};

const CENTERING_HELP: &str = "Pulls the wheel back to the center; games usually set it themselves.";

/// The strength of the forces through the input interface, for drivers without an attribute.
const fn ff_gain(missing: &'static str) -> Setting {
    Setting {
        ff: Some(FfControl::Gain),
        missing,
        ..Setting::percent(
            "gain",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        )
    }
}

/// The centering spring through the input interface, for drivers without an attribute.
const fn ff_autocenter(missing: &'static str) -> Setting {
    Setting {
        ff: Some(FfControl::Autocenter),
        missing,
        help: CENTERING_HELP,
        ..Setting::percent("autocenter", "Centering spring", EFFECTS)
    }
}

const NO_FORCES: &str = "The wheel has no force feedback, or its driver does not take it.";

/// The 16-bit levels of new-lg4ff, shown as percentages.
const fn lg4ff_level(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        scale: 655.35,
        missing: NEW_LG4FF,
        ..Setting::percent(attr, label, place)
    }
}

const fn lg4ff_percent(attr: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        missing: NEW_LG4FF,
        ..Setting::percent(attr, label, place)
    }
}

pub const SETTINGS: &[(Family, Setting)] = &[
    // lg4ff wheels: the kernel's driver has the rotation only; new-lg4ff the rest.
    (Lg4ff, Setting::rotation("range", 40.0)),
    // The kernel's driver takes the strength and centering through the input interface;
    // new-lg4ff has attributes for them.
    (
        Lg4ff,
        Setting {
            ff: Some(FfControl::Gain),
            ..lg4ff_level(
                "gain",
                "Force feedback strength",
                Place::Standard(WheelBaseParam::FfbStrength),
            )
        },
    ),
    (
        Lg4ff,
        lg4ff_percent(
            "spring_level",
            "Spring effect strength",
            Place::Standard(WheelBaseParam::SpringStrength),
        ),
    ),
    (
        Lg4ff,
        lg4ff_percent(
            "damper_level",
            "Damper effect strength",
            Place::Standard(WheelBaseParam::DamperStrength),
        ),
    ),
    (
        Lg4ff,
        lg4ff_percent("friction_level", "Friction effect strength", EFFECTS),
    ),
    (
        Lg4ff,
        Setting {
            help: CENTERING_HELP,
            ff: Some(FfControl::Autocenter),
            ..lg4ff_level("autocenter", "Centering spring", EFFECTS)
        },
    ),
    (
        Lg4ff,
        Setting {
            unit: "",
            max: Some(1.0),
            switch: true,
            help: "The wheel's LEDs show how strong the forces are, and when they clip.",
            ..lg4ff_percent(
                "ffb_leds",
                "Force feedback on the LEDs",
                Place::Custom {
                    group: "Force feedback",
                    advanced: true,
                },
            )
        },
    ),
    (
        Lg4ff,
        Setting {
            help: "For games that read the pedals on one axis: the throttle one way, the other \
                   pedal the other way.",
            missing: NEW_LG4FF,
            ..Setting::choice(
                "combine_pedals",
                "Combined pedals",
                &["Off", "Throttle and brake", "Throttle and clutch"],
                Place::Custom {
                    group: "Pedals",
                    advanced: true,
                },
            )
        },
    ),
    // G920, G923 for Xbox: the kernel's hid-logitech-hidpp has the rotation as an attribute,
    // the strength and centering through the input interface.
    (Hidpp, Setting::rotation("range", 180.0)),
    (Hidpp, ff_gain(NO_FORCES)),
    (Hidpp, ff_autocenter(NO_FORCES)),
    // G PRO, RS50: the TrueForce driver.
    (DirectDrive, Setting::rotation("wheel_range", 90.0)),
    (
        DirectDrive,
        Setting::percent(
            "wheel_strength",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        ),
    ),
    (
        DirectDrive,
        Setting::percent(
            "wheel_damping",
            "Natural damper",
            Place::Standard(WheelBaseParam::NaturalDamper),
        ),
    ),
    (
        DirectDrive,
        Setting {
            help: "Engine and road texture in the rim, on top of the force feedback.",
            ..Setting::percent("wheel_trueforce", "TrueForce", EFFECTS)
        },
    ),
    (
        DirectDrive,
        Setting {
            unit: "",
            min: 1.0,
            max: Some(15.0),
            help: "Smooths the forces: 1 is the least, 15 the most.",
            ..Setting::percent("wheel_ffb_filter", "Force feedback filter", EFFECTS)
        },
    ),
    // WingMan wheels: the strength and centering through the input interface, on iforce.
    (Classic, ff_gain(NO_FORCES)),
    (Classic, ff_autocenter(NO_FORCES)),
];

/// The settings of a family, in order.
pub fn of(family: Family) -> impl Iterator<Item = &'static Setting> {
    SETTINGS
        .iter()
        .filter(move |(f, _)| *f == family)
        .map(|(_, s)| s)
}

pub fn standard(family: Family, param: WheelBaseParam) -> Option<&'static Setting> {
    of(family).find(|s| s.place == Place::Standard(param))
}

pub fn find_custom(family: Family, id: &str) -> Option<&'static Setting> {
    of(family).find(|s| s.custom_id() == id && matches!(s.place, Place::Custom { .. }))
}
