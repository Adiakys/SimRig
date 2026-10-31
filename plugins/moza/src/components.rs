//! Moza devices as standard components: settings over the serial port, inputs and force
//! feedback through the kernel.

use simrig_plugin_kit::input::FfControl;
use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::shifter::{self, ButtonShifter};
use simrig_plugin_sdk::model::{Appearance, Control, RimShape};
use simrig_plugin_sdk::prelude::*;

use crate::device::{MozaDevice, Shared, axes};
use crate::models;
use crate::settings::{self, Place, Setting};

fn spec(device: &Shared, setting: &Setting) -> Support<ParamSpec> {
    match device.borrow_mut().value(setting.command) {
        Ok(raw) => Support::Supported(shown(setting, raw)),
        Err(u) => Support::Unsupported(u),
    }
}

/// A setting as shown for the device's value. The base and the rim keep their settings in their
/// own memory.
fn shown(setting: &Setting, raw: u64) -> ParamSpec {
    if setting.switch {
        ParamSpec::bool(raw != 0)
    } else if !setting.choices.is_empty() {
        // The option nearest to the value.
        let index = (0..setting.choices.len())
            .min_by_key(|i| setting.choices[*i].0.abs_diff(raw))
            .unwrap_or(0);
        ParamSpec::choice(&index.to_string(), &setting.options())
    } else {
        let value = (raw as f64 / setting.scale).clamp(setting.min, setting.max);
        ParamSpec::float(value, setting.min, setting.max, setting.step)
    }
    .persistent()
}

fn write(device: &Shared, setting: &Setting, value: &Value) -> Result<()> {
    let raw = match value {
        Value::Bool(on) => u64::from(*on),
        Value::Choice(id) => id
            .parse::<usize>()
            .ok()
            .and_then(|i| setting.choices.get(i))
            .map(|(raw, _)| *raw)
            .ok_or_else(|| Error::InvalidValue(format!("no choice '{id}'")))?,
        v => {
            let v = v
                .as_f64()
                .ok_or_else(|| Error::InvalidValue("expected a number".into()))?;
            (v * setting.scale).round().max(0.0) as u64
        }
    };
    let mut d = device.borrow_mut();
    d.set(setting.command, raw)?;
    if setting.command == "limit" {
        // The widest rotation games may ask for follows the limit.
        d.set("max-angle", raw)?;
    }
    Ok(())
}

/// The `Place::Custom` settings of a list, as custom parameters.
fn custom_params(device: &Shared, list: &[Setting]) -> Vec<CustomParam> {
    let mut params = Vec::new();
    for (order, setting) in list.iter().enumerate() {
        let Place::Custom { group, advanced } = setting.place else {
            continue;
        };
        let support = spec(device, setting);
        let now = match &support {
            Support::Supported(s) => s.clone(),
            Support::Unsupported(_) => shown(setting, setting.rest()),
        };
        let mut p = CustomParam::new(
            &settings::custom_id(setting),
            setting.label,
            now.ty,
            now.value,
        )
        .group(group)
        .order(order as i32)
        .persistent();
        if !setting.unit.is_empty() {
            p = p.unit(setting.unit);
        }
        if !setting.help.is_empty() {
            p = p.help(setting.help);
        }
        if advanced {
            p = p.advanced();
        }
        if let Support::Unsupported(u) = support {
            p = p.availability(Support::Unsupported(u));
        }
        params.push(p);
    }
    params
}

fn set_custom(device: &Shared, list: &[Setting], id: &str, value: &Value) -> Result<()> {
    let setting = list
        .iter()
        .find(|s| settings::custom_id(s) == id)
        .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
    write(device, setting, value)
}

pub struct Base {
    pub device: Shared,
}

impl Base {
    /// The force feedback strength through the input interface, when the base's settings
    /// cannot be reached and the driver takes it.
    fn ff_gain(&self, param: WheelBaseParam) -> Option<ParamSpec> {
        let d = self.device.borrow();
        if param != WheelBaseParam::FfbStrength || d.no_settings().is_none() {
            return None;
        }
        d.hid.ff_spec(FfControl::Gain)
    }
}

impl Component for Base {
    fn id(&self) -> String {
        format!("{}/base", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        self.device.borrow().name()
    }

    fn model(&self) -> Option<String> {
        Some(models::model_id(&self.device.borrow().product))
    }

    fn custom_params(&mut self) -> Vec<CustomParam> {
        custom_params(&self.device, settings::SETTINGS)
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        set_custom(&self.device, settings::SETTINGS, id, value)
    }

    fn save(&mut self, _item: &str, _value: &Value, _host: &mut HostLink<'_>) -> Result<()> {
        // Already kept: the base stores what it is sent.
        Ok(())
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        procedures(&self.device)
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        start(&self.device, id)
    }
}

/// What a base offers: mapping the pedals or a shifter plugged into it.
pub fn procedures(device: &Shared) -> Vec<ProcedureDescriptor> {
    let mut list = vec![pedals::descriptor(device)];
    list.extend(shifter::descriptors(device, device.borrow().gates()));
    list
}

pub fn start(device: &Shared, id: &str) -> Result<Box<dyn Procedure>> {
    if id == pedals::MAP_PEDALS {
        pedals::start(device, id)
    } else {
        shifter::start(device, id, device.borrow().gates())
    }
}

impl WheelBase for Base {
    fn angle(&mut self) -> Support<Degrees> {
        let rotation = self
            .device
            .borrow_mut()
            .get("limit")
            .map(|half| half as f64 * 2.0)
            .unwrap_or(900.0);
        self.device.borrow().hid.angle(rotation)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(match (d.no_force(), d.no_settings()) {
            (None, None) => "ok".into(),
            (Some(_), _) => "no force feedback driver".into(),
            (None, Some(_)) => "settings unavailable".into(),
        })
    }

    fn firmware(&mut self) -> Support<String> {
        Support::not_in_driver()
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        if let Some(spec) = self.ff_gain(param) {
            return Support::Supported(spec);
        }
        match settings::standard(param) {
            Some(setting) => spec(&self.device, setting),
            None => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("The base has no such setting."),
            ),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        if self.ff_gain(param).is_some() {
            return self.device.borrow_mut().hid.set_ff(FfControl::Gain, value);
        }
        let setting = settings::standard(param).ok_or_else(Error::unsupported)?;
        write(&self.device, setting, value)
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        let d = self.device.borrow();
        match action {
            WheelBaseAction::SetCenter => match d.no_settings() {
                Some(u) => Support::Unsupported(u),
                None => Support::Supported(ActionSpec::confirm(
                    "Hold the wheel straight: its current position becomes the center.",
                )),
            },
            WheelBaseAction::TestEffect | WheelBaseAction::StopForce => match d.no_force() {
                Some(u) => Support::Unsupported(u),
                None => d
                    .hid
                    .force_action(action)
                    .unwrap_or_else(Support::not_in_driver),
            },
            WheelBaseAction::Reset => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        let mut d = self.device.borrow_mut();
        if action == WheelBaseAction::SetCenter {
            d.set("calibration", 1)?;
            return Ok(ActionOutcome::message("Center set."));
        }
        d.hid
            .run_force_action(action)
            .unwrap_or_else(|| Err(Error::unsupported()))
    }
}

/// The rim mounted on a base: its controls come through the base, its settings over the base's
/// serial port.
pub struct Wheel {
    pub device: Shared,
}

/// The rim's analog paddles, on the base's axes (Boxflat's table).
const PADDLES: [(u16, &str); 2] = [
    (axes::LEFT_PADDLE, "Left paddle"),
    (axes::RIGHT_PADDLE, "Right paddle"),
];

impl Component for Wheel {
    fn id(&self) -> String {
        format!("{}/wheel", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        self.device.borrow().part_name("Steering wheel")
    }

    fn controls(&mut self) -> Vec<Control> {
        self.device.borrow().hid.controls(&PADDLES, &[])
    }

    fn appearance(&self) -> Appearance {
        Appearance::rim(RimShape::Round)
    }

    fn custom_params(&mut self) -> Vec<CustomParam> {
        custom_params(&self.device, settings::RIM)
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        set_custom(&self.device, settings::RIM, id, value)
    }

    fn save(&mut self, _item: &str, _value: &Value, _host: &mut HostLink<'_>) -> Result<()> {
        // Already kept: the rim stores what it is sent.
        Ok(())
    }
}

impl SteeringWheel for Wheel {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        d.hid.buttons()
    }

    fn hats(&mut self) -> Support<Vec<i64>> {
        self.device.borrow().hid.hats()
    }

    fn analog_paddles(&mut self) -> Support<Vec<f64>> {
        let codes = PADDLES.map(|(code, _)| code);
        self.device.borrow().hid.analog_paddles(&codes)
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::Unsupported(
            Unsupported::not_in_driver().with_detail("The base does not say which rim is mounted."),
        )
    }

    fn param(&mut self, param: SteeringWheelParam) -> Support<ParamSpec> {
        match settings::rim(param) {
            Some(setting) => spec(&self.device, setting),
            None => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("No serial command for it is known."),
            ),
        }
    }

    fn set_param(&mut self, param: SteeringWheelParam, value: &Value) -> Result<()> {
        let setting = settings::rim(param).ok_or_else(Error::unsupported)?;
        write(&self.device, setting, value)
    }
}

pub struct Handbrake {
    pub device: Shared,
}

impl Component for Handbrake {
    fn id(&self) -> String {
        format!("{}/handbrake", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        self.device.borrow().part_name("Handbrake")
    }

    fn model(&self) -> Option<String> {
        self.device.borrow().part_model()
    }
}

impl simrig_plugin_sdk::Handbrake for Handbrake {
    fn position(&mut self) -> Support<Axis> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        match d.hid.inputs.axes().get(&axes::HANDBRAKE) {
            Some(axis) => Support::Supported(Axis {
                raw: i64::from(axis.value),
                value: axis.unit(),
            }),
            None => Support::Unsupported(
                Unsupported::new(Reason::Unavailable)
                    .with_detail(d.hid.inputs.error().unwrap_or("No handbrake axis.")),
            ),
        }
    }

    fn pressed(&mut self) -> Support<bool> {
        Support::not_in_driver()
    }
}

/// A Moza shifter: the SGP or HGP on its own USB, or one plugged into a base (the HGP once a
/// gear is engaged, any once mapped by hand).
pub fn shifter(device: &Shared) -> ButtonShifter<MozaDevice> {
    let d = device.borrow();
    ButtonShifter {
        device: device.clone(),
        name: d.part_name("Shifter"),
        model: d.part_model(),
        gates: d.gates(),
    }
}

/// Moza pedals: plugged into a base, or a pedal set on its own USB.
pub fn pedals(device: &Shared) -> PedalSet<MozaDevice> {
    let d = device.borrow();
    PedalSet {
        device: device.clone(),
        name: d.part_name("Pedals"),
        model: d.part_model(),
        notes: PedalNotes {
            calibration: "Not available through SimRig yet: the pedals keep their own calibration.",
            shaping: "Not available through SimRig yet: set dead zones and curves in the game.",
            max_force: "Not available through SimRig yet.",
        },
    }
}
