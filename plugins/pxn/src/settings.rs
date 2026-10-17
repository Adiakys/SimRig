//! The VD base settings SimRig shows, over the fields of the base settings block, with the
//! ranges of Pitwall's base page.

use simrig_plugin_sdk::prelude::WheelBaseParam;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Standard(WheelBaseParam),
    Custom { group: &'static str, advanced: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// The field of the block (`protocol::FIELDS`).
    pub field: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub switch: bool,
    pub place: Place,
    pub help: &'static str,
}

const fn percent(field: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        field,
        label,
        unit: "%",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        switch: false,
        place,
        help: "",
    }
}

const fn custom(group: &'static str) -> Place {
    Place::Custom {
        group,
        advanced: false,
    }
}

const fn standard(param: WheelBaseParam) -> Place {
    Place::Standard(param)
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        unit: "°",
        min: 90.0,
        max: 2700.0,
        step: 10.0,
        ..percent(
            "max_steering_angle",
            "Rotation",
            standard(WheelBaseParam::Rotation),
        )
    },
    percent(
        "overall_ffb",
        "Force feedback strength",
        standard(WheelBaseParam::FfbStrength),
    ),
    percent(
        "mechanical_damper",
        "Natural damper",
        standard(WheelBaseParam::NaturalDamper),
    ),
    percent(
        "mechanical_friction",
        "Natural friction",
        standard(WheelBaseParam::NaturalFriction),
    ),
    percent(
        "inertia",
        "Natural inertia",
        standard(WheelBaseParam::NaturalInertia),
    ),
    percent(
        "spring_force",
        "Spring effect strength",
        standard(WheelBaseParam::SpringStrength),
    ),
    percent(
        "damper_force",
        "Damper effect strength",
        standard(WheelBaseParam::DamperStrength),
    ),
    Setting {
        help: "The share of the base's torque the forces use.",
        ..percent("max_torque", "Torque output", custom("Force feedback"))
    },
    Setting {
        unit: "",
        max: 255.0,
        help: "How fast the wheel may turn by itself.",
        ..percent("max_speed", "Maximum wheel speed", custom("Force feedback"))
    },
    percent("center_force", "Centering spring", custom("Force feedback")),
    percent("stop_force", "End stop force", custom("Force feedback")),
    percent("road_feel", "Road feel", custom("Force feedback")),
    Setting {
        unit: "",
        max: 10.0,
        help: "Smooths the forces: 0 is the least, 10 the most.",
        ..percent(
            "filter_level",
            "Force feedback filter",
            custom("Force feedback"),
        )
    },
    percent(
        "sine_force",
        "Vibration effect strength",
        custom("Game effects"),
    ),
    percent(
        "inertia_force",
        "Inertia effect strength",
        custom("Game effects"),
    ),
    percent(
        "game_friction",
        "Friction effect strength",
        custom("Game effects"),
    ),
    Setting {
        help: "How much the forces drop when no hands are on the wheel.",
        ..percent(
            "hands_off_protection",
            "Hands-off protection",
            Place::Custom {
                group: "Protection",
                advanced: true,
            },
        )
    },
    Setting {
        unit: "",
        max: 1.0,
        switch: true,
        help: "Turns every force the other way.",
        ..percent(
            "ffb_reverse",
            "Reverse forces",
            Place::Custom {
                group: "Protection",
                advanced: true,
            },
        )
    },
];

pub fn standard_setting(param: WheelBaseParam) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.place == Place::Standard(param))
}

pub fn custom_setting(id: &str) -> Option<&'static Setting> {
    SETTINGS
        .iter()
        .find(|s| s.field == id && matches!(s.place, Place::Custom { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_is_a_field() {
        for s in SETTINGS {
            assert!(crate::protocol::FIELDS.contains(&s.field), "{}", s.field);
        }
    }
}
