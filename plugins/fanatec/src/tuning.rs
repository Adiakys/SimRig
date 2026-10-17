//! The base's tuning menu, as hid-fanatecff exposes it (`ftec_tuning` class, `hid-ftec.h`):
//! which entries are standard items and which are the plugin's own settings.

/// Where an entry shows in SimRig.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A standard parameter of the wheel base.
    Base(&'static str),
    /// A standard parameter of the steering wheel.
    Wheel(&'static str),
    /// A setting of the plugin, on the wheel base, in a group.
    Custom { group: &'static str, advanced: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// The sysfs attribute.
    pub attr: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub min: i64,
    pub max: i64,
    pub unit: &'static str,
    pub place: Place,
}

const FFB: &str = "Force feedback";
const PEDALS: &str = "Pedals";
const BASE: &str = "Base";

pub const ENTRIES: &[Entry] = &[
    e(
        "FF",
        "Force feedback strength",
        "",
        0,
        100,
        "%",
        Place::Base("ffb_strength"),
    ),
    e(
        "NDP",
        "Natural damper",
        "",
        0,
        100,
        "%",
        Place::Base("natural_damper"),
    ),
    e(
        "NFR",
        "Natural friction",
        "",
        0,
        100,
        "%",
        Place::Base("natural_friction"),
    ),
    e(
        "NIN",
        "Natural inertia",
        "",
        0,
        100,
        "%",
        Place::Base("natural_inertia"),
    ),
    e(
        "SPR",
        "Spring effect strength",
        "",
        0,
        120,
        "%",
        Place::Base("spring_strength"),
    ),
    e(
        "DPR",
        "Damper effect strength",
        "",
        0,
        120,
        "%",
        Place::Base("damper_strength"),
    ),
    e(
        "SHO",
        "Vibration",
        "",
        0,
        100,
        "%",
        Place::Wheel("vibration"),
    ),
    e(
        "FOR",
        "Force effect strength",
        "Strength of the game's force effects.",
        0,
        120,
        "%",
        custom(FFB, false),
    ),
    e(
        "FEI",
        "Force effect intensity",
        "Sharpness of the force effects, in steps of 10.",
        0,
        100,
        "%",
        custom(FFB, false),
    ),
    e(
        "FFS",
        "Force feedback scaling",
        "0: linear, 1: peak.",
        0,
        1,
        "",
        custom(FFB, true),
    ),
    e(
        "INT",
        "Interpolation filter",
        "Smooths the force feedback signal.",
        0,
        20,
        "",
        custom(FFB, true),
    ),
    e("FUL", "Full force", "", 0, 100, "%", custom(FFB, true)),
    e(
        "DRI",
        "Drift mode",
        "Assists counter-steering; negative values add resistance.",
        -5,
        3,
        "",
        custom(FFB, true),
    ),
    e(
        "BLI",
        "Brake level indicator",
        "Vibration of the brake pedal near its limit; 101 turns it off.",
        0,
        101,
        "%",
        custom(PEDALS, false),
    ),
    e(
        "brF",
        "Brake force (load cell)",
        "How much force reads as a full brake.",
        0,
        100,
        "%",
        custom(PEDALS, false),
    ),
    e(
        "SEN",
        "Sensitivity (base menu)",
        "Rotation set in the base's own menu. The rotation above is the one SimRig applies.",
        90,
        2520,
        "°",
        custom(BASE, true),
    ),
    e(
        "SLOT",
        "Tuning slot",
        "The base keeps five sets of these settings.",
        1,
        5,
        "",
        custom(BASE, false),
    ),
];

/// The analog paddle mode (`ACP`), a standard choice of the steering wheel.
pub const PADDLE_MODES: [(&str, &str); 4] = [
    ("1", "Clutch bite point"),
    ("2", "Clutch and handbrake"),
    ("3", "Brake and throttle"),
    ("4", "Analog axes"),
];

const fn custom(group: &'static str, advanced: bool) -> Place {
    Place::Custom { group, advanced }
}

const fn e(
    attr: &'static str,
    label: &'static str,
    help: &'static str,
    min: i64,
    max: i64,
    unit: &'static str,
    place: Place,
) -> Entry {
    Entry {
        attr,
        label,
        help,
        min,
        max,
        unit,
        place,
    }
}

pub fn standard(place: Place) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.place == place)
}

/// The plugin's own id of a custom entry: `tuning_<attr>`, lowercase.
pub fn custom_id(entry: &Entry) -> String {
    format!("tuning_{}", entry.attr.to_ascii_lowercase())
}
