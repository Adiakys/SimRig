//! PXN bases as standard components: inputs and force feedback through the kernel, settings
//! of the VD series over their control interface.

use simrig_plugin_kit::input::FfControl;
use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_sdk::model::{Appearance, RimShape};
use simrig_plugin_sdk::prelude::*;

use crate::device::{PxnDevice, Shared};
use crate::settings::{self, Place, Setting};

fn spec(device: &Shared, setting: &Setting) -> Support<ParamSpec> {
    let mut d = device.borrow_mut();
    if let Some(u) = d.no_settings() {
        return Support::Unsupported(u);
    }
    let Some(raw) = d.get(setting.field) else {
        return Support::Unsupported(
            Unsupported::new(Reason::Unavailable).with_detail("The base does not answer."),
        );
    };
    // Whether the base keeps its settings when unplugged is not known yet: they are shown as
    // lasting until then.
    let spec = if setting.switch {
        ParamSpec::bool(raw != 0)
    } else {
        ParamSpec::float(
            f64::from(raw).clamp(setting.min, setting.max),
            setting.min,
            setting.max,
            setting.step,
        )
    };
    Support::Supported(if d.model.verified {
        spec
    } else {
        spec.with_caveat(
            Caveat::unverified()
                .with_detail("The settings protocol is confirmed on the VD6 and VD4 only."),
        )
    })
}

fn write(device: &Shared, setting: &Setting, value: &Value) -> Result<()> {
    let raw = match value {
        Value::Bool(on) => u16::from(*on),
        v => v
            .as_f64()
            .ok_or_else(|| Error::InvalidValue("expected a number".into()))?
            .round()
            .clamp(0.0, f64::from(u16::MAX)) as u16,
    };
    device.borrow_mut().set(setting.field, raw)
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
        self.device.borrow().model.name.into()
    }

    fn model(&self) -> Option<String> {
        self.device.borrow().model.model.map(Into::into)
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
            let mut p = CustomParam::new(setting.field, setting.label, ty, value)
                .group(group)
                .order(order as i32);
            if !setting.unit.is_empty() {
                p = p.unit(setting.unit);
            }
            if !setting.help.is_empty() {
                p = p.help(setting.help);
            }
            if advanced {
                p = p.advanced();
            }
            match support {
                Support::Supported(s) => p = p.caveat(s.caveat),
                Support::Unsupported(u) => p = p.availability(Support::Unsupported(u)),
            }
            params.push(p);
        }
        params
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = settings::custom_setting(id)
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        write(&self.device, setting, value)
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
        let mut d = self.device.borrow_mut();
        let range = if d.model.vd {
            d.range()
        } else {
            f64::from(d.model.max_range)
        };
        d.hid.angle(range)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(match (d.no_force(), d.hid.unbound()) {
            (_, Some(_)) => "no driver bound".into(),
            (Some(_), None) => "no force feedback driver".into(),
            (None, None) => "ok".into(),
        })
    }

    fn firmware(&mut self) -> Support<String> {
        Support::not_in_driver()
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        if let Some(spec) = self.ff_gain(param) {
            return Support::Supported(spec);
        }
        match settings::standard_setting(param) {
            Some(setting) => spec(&self.device, setting),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        if self.ff_gain(param).is_some() {
            return self.device.borrow_mut().hid.set_ff(FfControl::Gain, value);
        }
        let setting = settings::standard_setting(param).ok_or_else(Error::unsupported)?;
        write(&self.device, setting, value)
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        let d = self.device.borrow();
        match action {
            WheelBaseAction::TestEffect | WheelBaseAction::StopForce => match d.no_force() {
                Some(u) => Support::Unsupported(u),
                None => d
                    .hid
                    .force_action(action)
                    .unwrap_or_else(Support::not_in_driver),
            },
            WheelBaseAction::SetCenter => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("Not available through SimRig yet: set it on the base."),
            ),
            WheelBaseAction::Reset => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        self.device
            .borrow_mut()
            .hid
            .run_force_action(action)
            .unwrap_or_else(|| Err(Error::unsupported()))
    }
}

/// The rim: its buttons come through the base.
pub struct Wheel {
    pub device: Shared,
}

impl Component for Wheel {
    fn controls(&mut self) -> Vec<Control> {
        self.device.borrow().hid.controls(&[], &[])
    }

    fn id(&self) -> String {
        format!("{}/wheel", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        format!("{} rim", self.device.borrow().model.name)
    }

    fn appearance(&self) -> Appearance {
        Appearance::rim(RimShape::Round)
    }
}

impl SteeringWheel for Wheel {
    fn hats(&mut self) -> Support<Vec<i64>> {
        self.device.borrow().hid.hats()
    }

    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        d.hid.buttons()
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::not_in_driver()
    }
}

/// The pedals plugged into a PXN base.
pub fn pedals(device: &Shared) -> PedalSet<PxnDevice> {
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", device.borrow().model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
