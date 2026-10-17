//! Component types and their standard interfaces.
//!
//! Every component type has a fixed set of standard items, grouped by class (inputs,
//! parameters, outputs, actions). Their keys, labels, value kinds and units are defined here
//! once; plugins implement them (or report them unsupported) and the UI renders them.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::value::ValueKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    WheelBase,
    SteeringWheel,
    Pedals,
    Shifter,
    Handbrake,
}

impl ComponentKind {
    pub const ALL: [ComponentKind; 5] = [
        ComponentKind::WheelBase,
        ComponentKind::SteeringWheel,
        ComponentKind::Pedals,
        ComponentKind::Shifter,
        ComponentKind::Handbrake,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ComponentKind::WheelBase => "wheel_base",
            ComponentKind::SteeringWheel => "steering_wheel",
            ComponentKind::Pedals => "pedals",
            ComponentKind::Shifter => "shifter",
            ComponentKind::Handbrake => "handbrake",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ComponentKind::WheelBase => "Wheel base",
            ComponentKind::SteeringWheel => "Steering wheel",
            ComponentKind::Pedals => "Pedals",
            ComponentKind::Shifter => "Shifter",
            ComponentKind::Handbrake => "Handbrake",
        }
    }

    /// Standard items of this component type. For pedals these are per-pedal items: their ids
    /// are prefixed with the pedal role (`brake.position`).
    pub fn items(self) -> Vec<ItemDef> {
        fn defs<T: StdItem>() -> impl Iterator<Item = ItemDef> {
            T::ALL.iter().map(|i| i.def())
        }
        match self {
            ComponentKind::WheelBase => defs::<WheelBaseInput>()
                .chain(defs::<WheelBaseParam>())
                .chain(defs::<WheelBaseAction>())
                .collect(),
            ComponentKind::SteeringWheel => defs::<SteeringWheelInput>()
                .chain(defs::<SteeringWheelParam>())
                .chain(defs::<SteeringWheelOutput>())
                .chain(defs::<SteeringWheelAction>())
                .collect(),
            ComponentKind::Pedals => defs::<PedalInput>()
                .chain(defs::<PedalParam>())
                .chain(defs::<PedalAction>())
                .collect(),
            ComponentKind::Shifter => defs::<ShifterInput>()
                .chain(defs::<ShifterParam>())
                .chain(defs::<ShifterAction>())
                .collect(),
            ComponentKind::Handbrake => defs::<HandbrakeInput>()
                .chain(defs::<HandbrakeParam>())
                .chain(defs::<HandbrakeAction>())
                .collect(),
        }
    }

    /// Expected item ids of a component of this kind (pedal items expanded per role).
    pub fn item_ids(self, pedals: &[PedalRole]) -> Vec<String> {
        let items = self.items();
        if self == ComponentKind::Pedals {
            pedals
                .iter()
                .flat_map(|role| {
                    items
                        .iter()
                        .map(move |d| format!("{}.{}", role.key(), d.key))
                })
                .collect()
        } else {
            items.iter().map(|d| d.key.to_string()).collect()
        }
    }

    /// Looks up the definition of a standard item id of this kind.
    pub fn item_def(self, id: &str) -> Option<ItemDef> {
        let key = match self {
            ComponentKind::Pedals => {
                let (role, key) = id.split_once('.')?;
                PedalRole::from_key(role)?;
                key
            }
            _ => id,
        };
        self.items().into_iter().find(|d| d.key == key)
    }
}

impl fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PedalRole {
    Throttle,
    Brake,
    Clutch,
}

impl PedalRole {
    pub const ALL: [PedalRole; 3] = [PedalRole::Throttle, PedalRole::Brake, PedalRole::Clutch];

    pub fn key(self) -> &'static str {
        match self {
            PedalRole::Throttle => "throttle",
            PedalRole::Brake => "brake",
            PedalRole::Clutch => "clutch",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    Input,
    Param,
    Output,
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Degrees,
    DegreesPerSecond,
    Percent,
    Kilograms,
    Raw,
    Count,
}

impl Unit {
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Degrees => "°",
            Unit::DegreesPerSecond => "°/s",
            Unit::Percent => "%",
            Unit::Kilograms => "kg",
            Unit::Raw => "raw",
            Unit::Count => "",
        }
    }
}

/// Definition of a standard item, owned by the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDef {
    pub key: &'static str,
    pub label: &'static str,
    pub class: ItemClass,
    /// Value kind of inputs, parameters and outputs; `Empty` for actions.
    pub kind: ValueKind,
    pub unit: Option<Unit>,
}

/// An enum listing the standard items of one class of one component type.
pub trait StdItem: Copy + Sized + 'static {
    const ALL: &'static [Self];

    fn def(self) -> ItemDef;

    fn key(self) -> &'static str {
        self.def().key
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|i| i.key() == key)
    }
}

macro_rules! std_items {
    ($(#[$meta:meta])* $name:ident: $class:ident {
        $($variant:ident => $key:literal, $label:literal, $kind:ident, $unit:expr;)*
    }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),*
        }

        impl StdItem for $name {
            const ALL: &'static [Self] = &[$(Self::$variant),*];

            fn def(self) -> ItemDef {
                match self {
                    $(Self::$variant => ItemDef {
                        key: $key,
                        label: $label,
                        class: ItemClass::$class,
                        kind: ValueKind::$kind,
                        unit: $unit,
                    },)*
                }
            }
        }
    };
}

use Unit::*;

std_items! {
    /// Standard inputs of a wheel base.
    WheelBaseInput: Input {
        Angle => "angle", "Steering angle", Float, Some(Degrees);
        AngularVelocity => "angular_velocity", "Angular velocity", Float, Some(DegreesPerSecond);
        Status => "status", "Status", Text, None;
        Firmware => "firmware", "Firmware", Text, None;
    }
}

std_items! {
    /// Standard parameters of a wheel base.
    WheelBaseParam: Param {
        Rotation => "rotation", "Rotation", Float, Some(Degrees);
        FfbStrength => "ffb_strength", "Force feedback strength", Float, Some(Percent);
        NaturalDamper => "natural_damper", "Natural damper", Float, Some(Percent);
        NaturalFriction => "natural_friction", "Natural friction", Float, Some(Percent);
        NaturalInertia => "natural_inertia", "Natural inertia", Float, Some(Percent);
        SpringStrength => "spring_strength", "Spring effect strength", Float, Some(Percent);
        DamperStrength => "damper_strength", "Damper effect strength", Float, Some(Percent);
    }
}

std_items! {
    /// Standard actions of a wheel base.
    WheelBaseAction: Action {
        SetCenter => "set_center", "Set center", Empty, None;
        TestEffect => "test_effect", "Test force feedback", Empty, None;
        StopForce => "stop_force", "Stop force", Empty, None;
        Reset => "reset", "Reset settings", Empty, None;
    }
}

std_items! {
    /// Standard inputs of a steering wheel (rim).
    SteeringWheelInput: Input {
        Buttons => "buttons", "Buttons", Buttons, None;
        Encoders => "encoders", "Encoders", Ints, None;
        ShiftPaddles => "shift_paddles", "Shift paddles", Buttons, None;
        AnalogPaddles => "analog_paddles", "Analog paddles", Floats, None;
        RimId => "rim_id", "Rim", Text, None;
    }
}

std_items! {
    /// Standard parameters of a steering wheel.
    SteeringWheelParam: Param {
        AnalogPaddleMode => "analog_paddle_mode", "Analog paddle mode", Choice, None;
        Vibration => "vibration", "Vibration", Float, Some(Percent);
        LedBrightness => "led_brightness", "LED brightness", Float, Some(Percent);
    }
}

std_items! {
    /// Standard outputs of a steering wheel.
    SteeringWheelOutput: Output {
        RpmLeds => "rpm_leds", "RPM LEDs", Float, Some(Percent);
        FlagLeds => "flag_leds", "Flag LEDs", Choice, None;
        Display => "display", "Display", Text, None;
    }
}

std_items! {
    /// Standard actions of a steering wheel.
    SteeringWheelAction: Action {
        LedTest => "led_test", "Test LEDs", Empty, None;
        CalibratePaddles => "calibrate_paddles", "Calibrate paddles", Empty, None;
    }
}

std_items! {
    /// Standard inputs of each pedal.
    PedalInput: Input {
        Position => "position", "Position", Axis, None;
        Force => "force", "Force", Float, Some(Kilograms);
    }
}

std_items! {
    /// Standard parameters of each pedal.
    PedalParam: Param {
        CalibrationMin => "calibration_min", "Calibration minimum", Int, Some(Raw);
        CalibrationMax => "calibration_max", "Calibration maximum", Int, Some(Raw);
        DeadzoneLow => "deadzone_low", "Dead zone (low)", Float, Some(Percent);
        DeadzoneHigh => "deadzone_high", "Dead zone (high)", Float, Some(Percent);
        Curve => "curve", "Response curve", Curve, None;
        Inverted => "inverted", "Inverted", Bool, None;
        MaxForce => "max_force", "Maximum force", Float, Some(Kilograms);
        Vibration => "vibration", "Vibration", Float, Some(Percent);
    }
}

std_items! {
    /// Standard actions of each pedal.
    PedalAction: Action {
        Calibrate => "calibrate", "Calibrate", Empty, None;
        VibrationTest => "vibration_test", "Test vibration", Empty, None;
    }
}

std_items! {
    /// Standard inputs of a shifter.
    ShifterInput: Input {
        Gear => "gear", "Gear", Gear, None;
        PositionXy => "position_xy", "Lever position", Floats, None;
        Sequential => "sequential", "Sequential up/down", Buttons, None;
    }
}

std_items! {
    /// Standard parameters of a shifter.
    ShifterParam: Param {
        Gears => "gears", "Forward gears", Int, Some(Count);
        ReversePosition => "reverse_position", "Reverse position", Choice, None;
        Mode => "mode", "Mode", Choice, None;
        EngageThreshold => "engage_threshold", "Engage threshold", Float, Some(Percent);
    }
}

std_items! {
    /// Standard actions of a shifter.
    ShifterAction: Action {
        CalibrateGears => "calibrate_gears", "Calibrate gears", Empty, None;
    }
}

std_items! {
    /// Standard inputs of a handbrake.
    HandbrakeInput: Input {
        Position => "position", "Position", Axis, None;
        Pressed => "pressed", "Pressed", Bool, None;
    }
}

std_items! {
    /// Standard parameters of a handbrake.
    HandbrakeParam: Param {
        CalibrationMin => "calibration_min", "Calibration minimum", Int, Some(Raw);
        CalibrationMax => "calibration_max", "Calibration maximum", Int, Some(Raw);
        Curve => "curve", "Response curve", Curve, None;
        Deadzone => "deadzone", "Dead zone", Float, Some(Percent);
        Mode => "mode", "Mode", Choice, None;
        ButtonThreshold => "button_threshold", "Button threshold", Float, Some(Percent);
    }
}

std_items! {
    /// Standard actions of a handbrake.
    HandbrakeAction: Action {
        Calibrate => "calibrate", "Calibrate", Empty, None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_are_unique_per_kind() {
        for kind in ComponentKind::ALL {
            let ids = kind.item_ids(&PedalRole::ALL);
            let unique: HashSet<_> = ids.iter().collect();
            assert_eq!(ids.len(), unique.len(), "duplicate item in {kind}");
        }
    }

    #[test]
    fn item_lookup() {
        let def = ComponentKind::Pedals.item_def("brake.max_force").unwrap();
        assert_eq!(def.unit, Some(Unit::Kilograms));
        assert!(ComponentKind::Pedals.item_def("max_force").is_none());
        assert_eq!(
            WheelBaseParam::from_key("rotation"),
            Some(WheelBaseParam::Rotation)
        );
    }
}
