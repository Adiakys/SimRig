//! Generic access to the standard items of any component, by item id.

use std::collections::BTreeMap;

use simrig_model::{
    ActionOutcome, ActionSpec, ComponentDescription, ComponentKind, HandbrakeAction,
    HandbrakeInput, HandbrakeParam, ItemClass, ItemDescription, ItemSpec, ParamSpec, ParamType,
    PedalAction, PedalInput, PedalParam, PedalRole, ShifterAction, ShifterInput, ShifterParam,
    StdItem, SteeringWheelAction, SteeringWheelInput, SteeringWheelOutput, SteeringWheelParam,
    Support, Value, WheelBaseAction, WheelBaseInput, WheelBaseParam,
};

use crate::error::{Error, Result};
use crate::interfaces::{AnyComponent, Handbrake, Pedals, Shifter, SteeringWheel, WheelBase};

/// Uniform view of a component's standard items. `None` means "no such item".
trait Facade {
    fn input(&mut self, id: &str) -> Option<Support<Value>>;
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>>;
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>>;
    fn output(&mut self, _id: &str) -> Option<Support<ParamType>> {
        None
    }
    fn write_output(&mut self, _id: &str, _value: &Value) -> Option<Result<()>> {
        None
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>>;
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>>;
}

fn facade(c: &mut AnyComponent) -> Box<dyn Facade + '_> {
    match c {
        AnyComponent::WheelBase(b) => Box::new(WheelBaseFacade(&mut **b)),
        AnyComponent::SteeringWheel(w) => Box::new(SteeringWheelFacade(&mut **w)),
        AnyComponent::Pedals(p) => Box::new(PedalsFacade(&mut **p)),
        AnyComponent::Shifter(s) => Box::new(ShifterFacade(&mut **s)),
        AnyComponent::Handbrake(h) => Box::new(HandbrakeFacade(&mut **h)),
    }
}

struct WheelBaseFacade<'a>(&'a mut dyn WheelBase);

impl Facade for WheelBaseFacade<'_> {
    fn input(&mut self, id: &str) -> Option<Support<Value>> {
        Some(match WheelBaseInput::from_key(id)? {
            WheelBaseInput::Angle => self.0.angle().map(Value::Float),
            WheelBaseInput::AngularVelocity => self.0.angular_velocity().map(Value::Float),
            WheelBaseInput::Status => self.0.status().map(Value::Text),
            WheelBaseInput::Firmware => self.0.firmware().map(Value::Text),
        })
    }
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>> {
        Some(self.0.param(WheelBaseParam::from_key(id)?))
    }
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        Some(self.0.set_param(WheelBaseParam::from_key(id)?, value))
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>> {
        Some(self.0.action(WheelBaseAction::from_key(id)?))
    }
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>> {
        Some(self.0.run_action(WheelBaseAction::from_key(id)?, args))
    }
}

struct SteeringWheelFacade<'a>(&'a mut dyn SteeringWheel);

impl Facade for SteeringWheelFacade<'_> {
    fn input(&mut self, id: &str) -> Option<Support<Value>> {
        Some(match SteeringWheelInput::from_key(id)? {
            SteeringWheelInput::Buttons => self.0.buttons().map(Value::Buttons),
            SteeringWheelInput::Encoders => self.0.encoders().map(Value::Ints),
            SteeringWheelInput::ShiftPaddles => {
                self.0.shift_paddles().map(|p| Value::Buttons(p.to_vec()))
            }
            SteeringWheelInput::AnalogPaddles => self.0.analog_paddles().map(Value::Floats),
            SteeringWheelInput::RimId => self.0.rim_id().map(Value::Text),
        })
    }
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>> {
        Some(self.0.param(SteeringWheelParam::from_key(id)?))
    }
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        Some(self.0.set_param(SteeringWheelParam::from_key(id)?, value))
    }
    fn output(&mut self, id: &str) -> Option<Support<ParamType>> {
        Some(self.0.output(SteeringWheelOutput::from_key(id)?))
    }
    fn write_output(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        Some(
            self.0
                .write_output(SteeringWheelOutput::from_key(id)?, value),
        )
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>> {
        Some(self.0.action(SteeringWheelAction::from_key(id)?))
    }
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>> {
        Some(self.0.run_action(SteeringWheelAction::from_key(id)?, args))
    }
}

struct PedalsFacade<'a>(&'a mut dyn Pedals);

impl PedalsFacade<'_> {
    /// Splits `brake.position` into a pedal this set has and the item key.
    fn split<'i>(&self, id: &'i str) -> Option<(PedalRole, &'i str)> {
        let (role, key) = id.split_once('.')?;
        let role = PedalRole::from_key(role)?;
        self.0.pedals().contains(&role).then_some((role, key))
    }
}

impl Facade for PedalsFacade<'_> {
    fn input(&mut self, id: &str) -> Option<Support<Value>> {
        let (role, key) = self.split(id)?;
        Some(match PedalInput::from_key(key)? {
            PedalInput::Position => self.0.position(role).map(Value::Axis),
            PedalInput::Force => self.0.force(role).map(Value::Float),
        })
    }
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>> {
        let (role, key) = self.split(id)?;
        Some(self.0.param(role, PedalParam::from_key(key)?))
    }
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        let (role, key) = self.split(id)?;
        Some(self.0.set_param(role, PedalParam::from_key(key)?, value))
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>> {
        let (role, key) = self.split(id)?;
        Some(self.0.action(role, PedalAction::from_key(key)?))
    }
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>> {
        let (role, key) = self.split(id)?;
        Some(self.0.run_action(role, PedalAction::from_key(key)?, args))
    }
}

struct ShifterFacade<'a>(&'a mut dyn Shifter);

impl Facade for ShifterFacade<'_> {
    fn input(&mut self, id: &str) -> Option<Support<Value>> {
        Some(match ShifterInput::from_key(id)? {
            ShifterInput::Gear => self.0.gear().map(Value::Gear),
            ShifterInput::PositionXy => self.0.position_xy().map(|p| Value::Floats(p.to_vec())),
            ShifterInput::Sequential => self.0.sequential().map(|s| Value::Buttons(s.to_vec())),
        })
    }
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>> {
        Some(self.0.param(ShifterParam::from_key(id)?))
    }
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        Some(self.0.set_param(ShifterParam::from_key(id)?, value))
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>> {
        Some(self.0.action(ShifterAction::from_key(id)?))
    }
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>> {
        Some(self.0.run_action(ShifterAction::from_key(id)?, args))
    }
}

struct HandbrakeFacade<'a>(&'a mut dyn Handbrake);

impl Facade for HandbrakeFacade<'_> {
    fn input(&mut self, id: &str) -> Option<Support<Value>> {
        Some(match HandbrakeInput::from_key(id)? {
            HandbrakeInput::Position => self.0.position().map(Value::Axis),
            HandbrakeInput::Pressed => self.0.pressed().map(Value::Bool),
        })
    }
    fn param(&mut self, id: &str) -> Option<Support<ParamSpec>> {
        Some(self.0.param(HandbrakeParam::from_key(id)?))
    }
    fn set_param(&mut self, id: &str, value: &Value) -> Option<Result<()>> {
        Some(self.0.set_param(HandbrakeParam::from_key(id)?, value))
    }
    fn action(&mut self, id: &str) -> Option<Support<ActionSpec>> {
        Some(self.0.action(HandbrakeAction::from_key(id)?))
    }
    fn run_action(&mut self, id: &str, args: &Value) -> Option<Result<ActionOutcome>> {
        Some(self.0.run_action(HandbrakeAction::from_key(id)?, args))
    }
}

fn not_found(what: &str, id: &str) -> Error {
    Error::NotFound(format!("unknown {what} '{id}'"))
}

/// Ids and classes of all standard items of a component, in catalog order.
fn standard_items(kind: ComponentKind, pedals: &[PedalRole]) -> Vec<(String, ItemClass)> {
    let defs = kind.items();
    if kind == ComponentKind::Pedals {
        pedals
            .iter()
            .flat_map(|role| {
                defs.iter()
                    .map(move |d| (format!("{}.{}", role.key(), d.key), d.class))
            })
            .collect()
    } else {
        defs.iter().map(|d| (d.key.to_string(), d.class)).collect()
    }
}

pub(crate) fn describe(c: &mut AnyComponent) -> ComponentDescription {
    let kind = c.kind();
    let pedals = c.pedals();
    let (id, name, model, custom) = {
        let base = c.component_mut();
        (base.id(), base.name(), base.model(), base.custom_params())
    };
    let mut f = facade(c);
    let items = standard_items(kind, &pedals)
        .into_iter()
        .map(|(item, class)| {
            let support = match class {
                ItemClass::Input => f.input(&item).map(|s| s.map(|_| ItemSpec::Input)),
                ItemClass::Param => f.param(&item).map(|s| s.map(ItemSpec::Param)),
                ItemClass::Output => f.output(&item).map(|s| s.map(|ty| ItemSpec::Output { ty })),
                ItemClass::Action => f.action(&item).map(|s| s.map(ItemSpec::Action)),
            }
            .unwrap_or_else(Support::not_in_driver);
            ItemDescription {
                id: item,
                class,
                support,
            }
        })
        .collect();
    ComponentDescription {
        id,
        kind,
        name,
        model,
        pedals,
        items,
        custom,
    }
}

/// All supported inputs and their current values.
pub(crate) fn read_inputs(c: &mut AnyComponent) -> BTreeMap<String, Value> {
    let kind = c.kind();
    let pedals = c.pedals();
    let mut f = facade(c);
    standard_items(kind, &pedals)
        .into_iter()
        .filter(|(_, class)| *class == ItemClass::Input)
        .filter_map(|(id, _)| f.input(&id).and_then(Support::supported).map(|v| (id, v)))
        .collect()
}

fn supported<T>(support: Support<T>) -> Result<T> {
    match support {
        Support::Supported(v) => Ok(v),
        Support::Unsupported(u) => Err(Error::Unsupported(u)),
    }
}

fn check_force_limit(limit: Option<f64>, value: &Value) -> Result<()> {
    match (limit, value.as_f64()) {
        (Some(limit), Some(v)) if v > limit => Err(Error::InvalidValue(format!(
            "{v} exceeds the force limit {limit}"
        ))),
        _ => Ok(()),
    }
}

pub(crate) fn get_param(c: &mut AnyComponent, id: &str) -> Result<ParamSpec> {
    supported(
        facade(c)
            .param(id)
            .ok_or_else(|| not_found("parameter", id))?,
    )
}

/// Validates the value against the parameter's spec, then sets it.
pub(crate) fn set_param(c: &mut AnyComponent, id: &str, value: &Value) -> Result<()> {
    let mut f = facade(c);
    let spec = supported(f.param(id).ok_or_else(|| not_found("parameter", id))?)?;
    spec.ty.validate(value)?;
    check_force_limit(spec.force_limit, value)?;
    f.set_param(id, value)
        .unwrap_or_else(|| Err(not_found("parameter", id)))
}

pub(crate) fn set_custom(c: &mut AnyComponent, id: &str, value: &Value) -> Result<()> {
    let component = c.component_mut();
    let param = component
        .custom_params()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| not_found("custom parameter", id))?;
    supported(param.availability)?;
    param.ty.validate(value)?;
    check_force_limit(param.force_limit, value)?;
    component.set_custom(id, value)
}

pub(crate) fn run_action(c: &mut AnyComponent, id: &str, args: &Value) -> Result<ActionOutcome> {
    let mut f = facade(c);
    supported(f.action(id).ok_or_else(|| not_found("action", id))?)?;
    f.run_action(id, args)
        .unwrap_or_else(|| Err(not_found("action", id)))
}

pub(crate) fn write_output(c: &mut AnyComponent, id: &str, value: &Value) -> Result<()> {
    let mut f = facade(c);
    let ty = supported(f.output(id).ok_or_else(|| not_found("output", id))?)?;
    ty.validate(value)?;
    f.write_output(id, value)
        .unwrap_or_else(|| Err(not_found("output", id)))
}
