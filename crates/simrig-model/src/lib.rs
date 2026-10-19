//! Domain model shared by the simrig app, the plugin protocol and the plugin SDK.
//!
//! It defines the standard interface of every component type (inputs, parameters, outputs and
//! actions), how plugins report unsupported items, and the self-described custom parameters.
//! It contains no I/O and nothing brand-specific.

pub mod component;
pub mod custom;
pub mod describe;
pub mod device;
pub mod param;
pub mod procedure;
pub mod support;
pub mod value;

pub use component::{
    ComponentKind, HandbrakeAction, HandbrakeInput, HandbrakeParam, ItemClass, ItemDef,
    PedalAction, PedalInput, PedalParam, PedalRole, ShifterAction, ShifterInput, ShifterParam,
    StdItem, SteeringWheelAction, SteeringWheelInput, SteeringWheelOutput, SteeringWheelParam,
    Unit, WheelBaseAction, WheelBaseInput, WheelBaseParam,
};
pub use custom::{Condition, CustomParam};
pub use describe::{
    ActionOutcome, ActionSpec, Appearance, ComponentDescription, Control, ControlKind,
    ControlSymbol, ItemDescription, ItemSpec, RimShape,
};
pub use device::{Bus, DeviceInfo};
pub use param::{ChoiceOption, ParamSpec, ParamType, Persistence, ValueError};
pub use procedure::{
    ActionRole, Element, Level, Preview, ProcedureDescriptor, ProcedureState, ProcedureTarget,
    Step, StepAction,
};
pub use support::{Caveat, CaveatReason, Reason, Support, Unsupported};
pub use value::{Axis, CurvePoint, Gear, Value, ValueKind};

/// Angle in degrees.
pub type Degrees = f64;
/// Percentage, `0.0..=100.0`.
pub type Percent = f64;
/// Force in kilograms.
pub type Kilograms = f64;

/// Prefix of custom parameter ids when addressed as items (`custom.drift_mode`).
pub const CUSTOM_PREFIX: &str = "custom.";
