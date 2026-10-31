//! The settings of the base and of its rim SimRig shows, over the serial commands
//! (`protocol::COMMANDS`), with the ranges and scales of Boxflat's base and wheel panels.

use simrig_plugin_kit::settings::choice_options;
use simrig_plugin_sdk::prelude::{SteeringWheelParam, WheelBaseParam};

#[derive(Clone, Copy, Debug)]
pub enum Place {
    Standard(WheelBaseParam),
    Rim(SteeringWheelParam),
    Custom { group: &'static str, advanced: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// The serial command (`protocol::command`).
    pub command: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// The device's value for one unit shown.
    pub scale: f64,
    /// An on/off switch (0 or 1 on the device).
    pub switch: bool,
    /// A choice: the device's value of each option, and its label.
    pub choices: &'static [(u64, &'static str)],
    pub place: Place,
    pub help: &'static str,
}

impl Setting {
    pub fn options(&self) -> Vec<(&'static str, &'static str)> {
        choice_options(self.choices.iter().map(|(_, label)| *label))
    }

    /// The device's value shown while the setting is unavailable: the first option, else the
    /// lowest value.
    pub fn rest(&self) -> u64 {
        match self.choices.first() {
            Some((value, _)) => *value,
            None => (self.min * self.scale) as u64,
        }
    }
}

const fn percent(command: &'static str, label: &'static str, place: Place) -> Setting {
    Setting {
        command,
        label,
        unit: "%",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        scale: 1.0,
        switch: false,
        choices: &[],
        place,
        help: "",
    }
}

const fn choice(
    command: &'static str,
    label: &'static str,
    choices: &'static [(u64, &'static str)],
    place: Place,
) -> Setting {
    Setting {
        unit: "",
        choices,
        ..percent(command, label, place)
    }
}

const fn custom(group: &'static str) -> Place {
    Place::Custom {
        group,
        advanced: false,
    }
}

const fn advanced(group: &'static str) -> Place {
    Place::Custom {
        group,
        advanced: true,
    }
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        command: "limit",
        label: "Rotation",
        unit: "°",
        min: 90.0,
        max: 2700.0,
        step: 2.0,
        // The base stores half the rotation.
        scale: 0.5,
        switch: false,
        choices: &[],
        place: Place::Standard(WheelBaseParam::Rotation),
        help: "",
    },
    Setting {
        scale: 10.0,
        ..percent(
            "ffb-strength",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        )
    },
    percent(
        "damper",
        "Natural damper",
        Place::Standard(WheelBaseParam::NaturalDamper),
    ),
    percent(
        "friction",
        "Natural friction",
        Place::Standard(WheelBaseParam::NaturalFriction),
    ),
    percent(
        "spring-gain",
        "Spring effect strength",
        Place::Standard(WheelBaseParam::SpringStrength),
    ),
    percent(
        "damper-gain",
        "Damper effect strength",
        Place::Standard(WheelBaseParam::DamperStrength),
    ),
    Setting {
        min: 50.0,
        step: 5.0,
        help: "The share of the base's torque the forces use.",
        ..percent("torque", "Torque output", custom("Force feedback"))
    },
    Setting {
        max: 200.0,
        scale: 10.0,
        help: "How fast the wheel may turn by itself.",
        ..percent("speed", "Maximum wheel speed", custom("Force feedback"))
    },
    Setting {
        unit: "",
        min: 100.0,
        max: 500.0,
        step: 50.0,
        help: "The weight the wheel feels like it has.",
        ..percent("inertia", "Natural inertia", custom("Force feedback"))
    },
    percent("spring", "Centering spring", custom("Force feedback")),
    Setting {
        unit: "",
        max: 10.0,
        help: "How much of the road's detail reaches the wheel.",
        ..percent(
            "road-sensitivity",
            "Road sensitivity",
            custom("Force feedback"),
        )
    },
    percent(
        "friction-gain",
        "Friction effect strength",
        custom("Game effects"),
    ),
    percent(
        "inertia-gain",
        "Inertia effect strength",
        custom("Game effects"),
    ),
    Setting {
        unit: "",
        max: 1.0,
        switch: true,
        help: "Slows the wheel down when it spins with no hands on it.",
        ..percent("protection", "Hands-off protection", advanced("Protection"))
    },
    Setting {
        unit: "",
        min: 100.0,
        max: 4000.0,
        step: 50.0,
        help: "The inertia of the mounted wheel, for the hands-off protection.",
        ..percent(
            "natural-inertia",
            "Steering wheel inertia",
            advanced("Protection"),
        )
    },
    percent("speed-damping", "Speed damping", advanced("Protection")),
    Setting {
        unit: "km/h",
        max: 400.0,
        help: "The car speed from which speed damping applies.",
        ..percent(
            "speed-damping-point",
            "Speed damping from",
            advanced("Protection"),
        )
    },
    Setting {
        unit: "",
        min: 1.0,
        max: 10.0,
        help: "How hard the end of the rotation feels.",
        ..percent(
            "soft-limit-stiffness",
            "End stop stiffness",
            advanced("Protection"),
        )
    },
];

/// The settings of the rim mounted on the base, as Boxflat's wheel panel has them.
pub const RIM: &[Setting] = &[
    choice(
        "wheel-paddles-mode",
        "Analog paddle mode",
        &[(1, "Buttons"), (2, "Combined axis"), (3, "Separate axes")],
        Place::Rim(SteeringWheelParam::AnalogPaddleMode),
    ),
    percent(
        "wheel-rpm-brightness",
        "LED brightness",
        Place::Rim(SteeringWheelParam::LedBrightness),
    ),
    Setting {
        min: 5.0,
        max: 95.0,
        help: "With the paddles on a combined axis: the share of it the left paddle covers.",
        ..percent(
            "wheel-clutch-point",
            "Combined axis split",
            custom("Paddles"),
        )
    },
    choice(
        "wheel-knob-mode",
        "Rotary encoders",
        &[(0, "Buttons"), (1, "Knob")],
        custom("Controls"),
    ),
    choice(
        "wheel-stick-mode",
        "Left stick",
        &[(0, "Buttons"), (256, "Directional pad")],
        custom("Controls"),
    ),
];

pub fn standard(param: WheelBaseParam) -> Option<&'static Setting> {
    SETTINGS
        .iter()
        .find(|s| matches!(s.place, Place::Standard(p) if p == param))
}

pub fn rim(param: SteeringWheelParam) -> Option<&'static Setting> {
    RIM.iter()
        .find(|s| matches!(s.place, Place::Rim(p) if p == param))
}

pub fn custom_id(setting: &Setting) -> String {
    setting.command.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_is_a_command() {
        for s in SETTINGS.iter().chain(RIM) {
            let c = crate::protocol::command(s.command).expect(s.command);
            assert!(c.read.is_some() && c.write.is_some(), "{}", s.command);
        }
    }
}
