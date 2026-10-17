//! Standard component interfaces. Every method defaults to "unsupported, not in driver".

use simrig_model::{
    ActionOutcome, ActionSpec, Axis, ComponentKind, CustomParam, Degrees, Gear, HandbrakeAction,
    HandbrakeParam, Kilograms, ParamSpec, ParamType, PedalAction, PedalParam, PedalRole,
    ShifterAction, ShifterParam, SteeringWheelAction, SteeringWheelOutput, SteeringWheelParam,
    Support, Value, WheelBaseAction, WheelBaseParam,
};

use crate::error::{Error, Result};

/// Shared by all component types.
pub trait Component {
    /// Unique within the plugin, stable across restarts.
    fn id(&self) -> String;

    fn name(&self) -> String;

    /// Catalog model id, e.g. `fanatec/csl-dd`.
    fn model(&self) -> Option<String> {
        None
    }

    /// Parameters beyond the standard interface, with their current values.
    fn custom_params(&mut self) -> Vec<CustomParam> {
        Vec::new()
    }

    /// Called with a value already validated against the parameter's type and range.
    fn set_custom(&mut self, _id: &str, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
}

pub trait WheelBase: Component {
    fn angle(&mut self) -> Support<Degrees> {
        Support::not_in_driver()
    }
    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }
    fn status(&mut self) -> Support<String> {
        Support::not_in_driver()
    }
    fn firmware(&mut self) -> Support<String> {
        Support::not_in_driver()
    }
    fn param(&mut self, _param: WheelBaseParam) -> Support<ParamSpec> {
        Support::not_in_driver()
    }
    /// Called with a value already validated against the parameter's spec.
    fn set_param(&mut self, _param: WheelBaseParam, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    fn action(&mut self, _action: WheelBaseAction) -> Support<ActionSpec> {
        Support::not_in_driver()
    }
    fn run_action(&mut self, _action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        Err(Error::unsupported())
    }
}

pub trait SteeringWheel: Component {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        Support::not_in_driver()
    }
    fn encoders(&mut self) -> Support<Vec<i64>> {
        Support::not_in_driver()
    }
    fn shift_paddles(&mut self) -> Support<[bool; 2]> {
        Support::not_in_driver()
    }
    fn analog_paddles(&mut self) -> Support<Vec<f64>> {
        Support::not_in_driver()
    }
    fn rim_id(&mut self) -> Support<String> {
        Support::not_in_driver()
    }
    fn param(&mut self, _param: SteeringWheelParam) -> Support<ParamSpec> {
        Support::not_in_driver()
    }
    fn set_param(&mut self, _param: SteeringWheelParam, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    /// Type of the values the output accepts.
    fn output(&mut self, _output: SteeringWheelOutput) -> Support<ParamType> {
        Support::not_in_driver()
    }
    fn write_output(&mut self, _output: SteeringWheelOutput, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    fn action(&mut self, _action: SteeringWheelAction) -> Support<ActionSpec> {
        Support::not_in_driver()
    }
    fn run_action(&mut self, _action: SteeringWheelAction, _args: &Value) -> Result<ActionOutcome> {
        Err(Error::unsupported())
    }
}

pub trait Pedals: Component {
    /// Which pedals this set has.
    fn pedals(&self) -> Vec<PedalRole>;

    fn position(&mut self, _pedal: PedalRole) -> Support<Axis> {
        Support::not_in_driver()
    }
    fn force(&mut self, _pedal: PedalRole) -> Support<Kilograms> {
        Support::not_in_driver()
    }
    fn param(&mut self, _pedal: PedalRole, _param: PedalParam) -> Support<ParamSpec> {
        Support::not_in_driver()
    }
    fn set_param(&mut self, _pedal: PedalRole, _param: PedalParam, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    fn action(&mut self, _pedal: PedalRole, _action: PedalAction) -> Support<ActionSpec> {
        Support::not_in_driver()
    }
    fn run_action(
        &mut self,
        _pedal: PedalRole,
        _action: PedalAction,
        _args: &Value,
    ) -> Result<ActionOutcome> {
        Err(Error::unsupported())
    }
}

pub trait Shifter: Component {
    fn gear(&mut self) -> Support<Gear> {
        Support::not_in_driver()
    }
    /// Lever position, both axes in `-1.0..=1.0`.
    fn position_xy(&mut self) -> Support<[f64; 2]> {
        Support::not_in_driver()
    }
    /// Sequential mode: `[up, down]`.
    fn sequential(&mut self) -> Support<[bool; 2]> {
        Support::not_in_driver()
    }
    fn param(&mut self, _param: ShifterParam) -> Support<ParamSpec> {
        Support::not_in_driver()
    }
    fn set_param(&mut self, _param: ShifterParam, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    fn action(&mut self, _action: ShifterAction) -> Support<ActionSpec> {
        Support::not_in_driver()
    }
    fn run_action(&mut self, _action: ShifterAction, _args: &Value) -> Result<ActionOutcome> {
        Err(Error::unsupported())
    }
}

pub trait Handbrake: Component {
    fn position(&mut self) -> Support<Axis> {
        Support::not_in_driver()
    }
    fn pressed(&mut self) -> Support<bool> {
        Support::not_in_driver()
    }
    fn param(&mut self, _param: HandbrakeParam) -> Support<ParamSpec> {
        Support::not_in_driver()
    }
    fn set_param(&mut self, _param: HandbrakeParam, _value: &Value) -> Result<()> {
        Err(Error::unsupported())
    }
    fn action(&mut self, _action: HandbrakeAction) -> Support<ActionSpec> {
        Support::not_in_driver()
    }
    fn run_action(&mut self, _action: HandbrakeAction, _args: &Value) -> Result<ActionOutcome> {
        Err(Error::unsupported())
    }
}

/// A component of any type, as returned by [`Adapter::attach`](crate::Adapter::attach).
pub enum AnyComponent {
    WheelBase(Box<dyn WheelBase>),
    SteeringWheel(Box<dyn SteeringWheel>),
    Pedals(Box<dyn Pedals>),
    Shifter(Box<dyn Shifter>),
    Handbrake(Box<dyn Handbrake>),
}

impl AnyComponent {
    pub fn kind(&self) -> ComponentKind {
        match self {
            AnyComponent::WheelBase(_) => ComponentKind::WheelBase,
            AnyComponent::SteeringWheel(_) => ComponentKind::SteeringWheel,
            AnyComponent::Pedals(_) => ComponentKind::Pedals,
            AnyComponent::Shifter(_) => ComponentKind::Shifter,
            AnyComponent::Handbrake(_) => ComponentKind::Handbrake,
        }
    }

    pub fn component(&self) -> &dyn Component {
        match self {
            AnyComponent::WheelBase(c) => &**c,
            AnyComponent::SteeringWheel(c) => &**c,
            AnyComponent::Pedals(c) => &**c,
            AnyComponent::Shifter(c) => &**c,
            AnyComponent::Handbrake(c) => &**c,
        }
    }

    pub fn component_mut(&mut self) -> &mut dyn Component {
        match self {
            AnyComponent::WheelBase(c) => &mut **c,
            AnyComponent::SteeringWheel(c) => &mut **c,
            AnyComponent::Pedals(c) => &mut **c,
            AnyComponent::Shifter(c) => &mut **c,
            AnyComponent::Handbrake(c) => &mut **c,
        }
    }

    pub fn id(&self) -> String {
        self.component().id()
    }

    pub fn pedals(&self) -> Vec<PedalRole> {
        match self {
            AnyComponent::Pedals(p) => p.pedals(),
            _ => Vec::new(),
        }
    }
}
