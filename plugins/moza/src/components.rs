//! Moza devices as standard components: settings over the serial port, inputs and force
//! feedback through the kernel.

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_sdk::prelude::*;

use crate::device::{MozaDevice, Shared, axes};
use crate::models;
use crate::settings::{self, Place, Setting};

fn spec(device: &Shared, setting: &Setting) -> Support<ParamSpec> {
    let mut d = device.borrow_mut();
    if let Some(u) = d.no_settings() {
        return Support::Unsupported(u);
    }
    let Some(raw) = d.get(setting.command) else {
        return Support::Unsupported(
            Unsupported::new(Reason::Unavailable)
                .with_detail("The base does not answer for this setting."),
        );
    };
    // The base keeps its settings in its own memory.
    Support::Supported(
        if setting.switch {
            ParamSpec::bool(raw != 0)
        } else {
            let value = (raw as f64 / setting.scale).clamp(setting.min, setting.max);
            ParamSpec::float(value, setting.min, setting.max, setting.step)
        }
        .persistent(),
    )
}

fn write(device: &Shared, setting: &Setting, value: &Value) -> Result<()> {
    let raw = match value {
        Value::Bool(on) => u64::from(*on),
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

pub struct Base {
    pub device: Shared,
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
        let mut params = Vec::new();
        for (order, setting) in settings::SETTINGS.iter().enumerate() {
            let Place::Custom { group, advanced } = setting.place else {
                continue;
            };
            let support = spec(&self.device, setting);
            let (ty, value) = match &support {
                Support::Supported(s) => (s.ty.clone(), s.value.clone()),
                Support::Unsupported(_) if setting.switch => (ParamType::Bool, Value::Bool(false)),
                Support::Unsupported(_) => (
                    ParamType::Float {
                        min: setting.min,
                        max: setting.max,
                        step: setting.step,
                    },
                    Value::Float(setting.min),
                ),
            };
            let mut p = CustomParam::new(&settings::custom_id(setting), setting.label, ty, value)
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

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = settings::SETTINGS
            .iter()
            .find(|s| settings::custom_id(s) == id)
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        write(&self.device, setting, value)
    }

    fn save(&mut self, _item: &str, _value: &Value, _host: &mut HostLink<'_>) -> Result<()> {
        // Already kept: the base stores what it is sent.
        Ok(())
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        vec![pedals::descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        pedals::start(&self.device, id)
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
        match settings::standard(param) {
            Some(setting) => spec(&self.device, setting),
            None => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("The base has no such setting."),
            ),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
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

pub struct Handbrake {
    pub device: Shared,
}

impl Component for Handbrake {
    fn id(&self) -> String {
        format!("{}/handbrake", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        self.device.borrow().name()
    }

    fn model(&self) -> Option<String> {
        Some(models::model_id(&self.device.borrow().product))
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

/// Moza pedals: plugged into a base, or a pedal set on its own USB.
pub fn pedals(device: &Shared) -> PedalSet<MozaDevice> {
    let d = device.borrow();
    let on_base = d.kind == models::Kind::Base;
    PedalSet {
        device: device.clone(),
        name: if on_base {
            format!("Pedals on the {}", d.name())
        } else {
            d.name()
        },
        model: (!on_base).then(|| models::model_id(&d.product)),
        notes: PedalNotes {
            calibration: "Not available through SimRig yet: the pedals keep their own calibration.",
            shaping: "Not available through SimRig yet: set dead zones and curves in the game.",
            max_force: "Not available through SimRig yet.",
        },
    }
}
