//! Logitech wheels as standard components, over their kernel driver's sysfs attributes and
//! event node.

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::sysfs;
use simrig_plugin_sdk::model::{Appearance, RimShape, StdItem};
use simrig_plugin_sdk::prelude::*;

use crate::device::{LogitechDevice, Shared};
use crate::models::Family;
use crate::settings::{self, Place, Setting};

fn max(device: &LogitechDevice, setting: &Setting) -> f64 {
    setting.max.unwrap_or(f64::from(device.model.max_range))
}

fn spec(device: &Shared, setting: &Setting) -> Support<ParamSpec> {
    let d = device.borrow();
    if let Some(u) = d.hid.unbound() {
        return Support::Unsupported(u);
    }
    let Some(raw) = d.hid.read_int(setting.attr) else {
        return Support::Unsupported(Unsupported::not_in_driver().with_detail(setting.missing));
    };
    let max = max(&d, setting);
    // Kept by the driver until the wheel is unplugged; saved as a udev rule.
    Support::Supported(
        if setting.switch {
            ParamSpec::bool(raw != 0)
        } else {
            let value = (raw as f64 / setting.scale).round().clamp(setting.min, max);
            ParamSpec::float(value, setting.min, max, setting.step)
        }
        .persistent(),
    )
}

fn raw(setting: &Setting, value: &Value) -> Result<i64> {
    match value {
        Value::Bool(on) => Ok(i64::from(*on)),
        v => {
            let v = v
                .as_f64()
                .ok_or_else(|| Error::InvalidValue("expected a number".into()))?;
            Ok((v * setting.scale).round().max(0.0) as i64)
        }
    }
}

fn write(device: &Shared, setting: &Setting, value: &Value) -> Result<()> {
    let raw = raw(setting, value)?;
    device.borrow().hid.write(setting.attr, &raw.to_string())
}

pub struct Base {
    pub device: Shared,
}

impl Base {
    fn family(&self) -> Family {
        self.device.borrow().model.family
    }

    fn setting(&self, item: &str) -> Option<&'static Setting> {
        let family = self.family();
        match item.strip_prefix("custom.") {
            Some(id) => settings::find_custom(family, id),
            None => WheelBaseParam::from_key(item).and_then(|p| settings::standard(family, p)),
        }
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
        Some(self.device.borrow().model.model.into())
    }

    fn custom_params(&mut self) -> Vec<CustomParam> {
        let family = self.family();
        let mut params = Vec::new();
        for (order, setting) in settings::SETTINGS.iter().enumerate() {
            let Place::Custom { group, advanced } = setting.place else {
                continue;
            };
            if setting.family != family {
                continue;
            }
            let support = spec(&self.device, setting);
            let (ty, value) = match &support {
                Support::Supported(s) => (s.ty.clone(), s.value.clone()),
                Support::Unsupported(_) if setting.switch => (ParamType::Bool, Value::Bool(false)),
                Support::Unsupported(_) => (
                    ParamType::Float {
                        min: setting.min,
                        max: max(&self.device.borrow(), setting),
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
        let setting = settings::find_custom(self.family(), id)
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        write(&self.device, setting, value)
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        let setting = self
            .setting(item)
            .ok_or_else(|| Error::NotFound(format!("no setting '{item}'")))?;
        let raw = raw(setting, value)?;
        let pid = format!("{:04x}", self.device.borrow().model.pid);
        // A udev rule sets it again whenever the driver binds to the wheel.
        host.privileged(
            "save-setting",
            &[
                ("pid", &pid),
                ("attr", setting.attr),
                ("value", &raw.to_string()),
            ],
        )?;
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
        let d = self.device.borrow();
        let range = d.range().unwrap_or(f64::from(d.model.max_range));
        d.hid.angle(range)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(match d.hid.driver() {
            Some(driver) => format!("ok ({driver})"),
            None => "no driver bound".into(),
        })
    }

    fn firmware(&mut self) -> Support<String> {
        let d = self.device.borrow();
        // The TrueForce driver: "base: U1 65.03.B0038" and the motor's on a second line.
        let firmware = d
            .hid
            .attr("wheel_firmware")
            .and_then(|p| sysfs::read(&p))
            .and_then(|text| {
                text.lines()
                    .next()
                    .map(|l| l.trim_start_matches("base:").trim().to_string())
            });
        match firmware {
            Some(v) if !v.is_empty() => Support::Supported(v),
            _ => Support::not_in_driver(),
        }
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        match settings::standard(self.family(), param) {
            Some(setting) => spec(&self.device, setting),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let setting = settings::standard(self.family(), param).ok_or_else(Error::unsupported)?;
        write(&self.device, setting, value)
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        if let Some(spec) = d.hid.force_action(action) {
            return spec;
        }
        match action {
            WheelBaseAction::SetCenter if d.hid.attr("wheel_calibrate_here").is_some() => {
                Support::Supported(ActionSpec::confirm(
                    "Hold the wheel straight: its current position becomes the center.",
                ))
            }
            WheelBaseAction::SetCenter => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("The wheel finds its center by itself when it is plugged in."),
            ),
            _ => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        let mut d = self.device.borrow_mut();
        if let Some(outcome) = d.hid.run_force_action(action) {
            return outcome;
        }
        match action {
            WheelBaseAction::SetCenter => {
                d.hid.write("wheel_calibrate_here", "1")?;
                Ok(ActionOutcome::message("Center set."))
            }
            _ => Err(Error::unsupported()),
        }
    }
}

/// The rim of a Logitech wheel: part of the wheel, its buttons on the same device.
pub struct Wheel {
    pub device: Shared,
}

impl Component for Wheel {
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
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        d.hid.buttons()
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::not_in_hardware("The rim is part of the wheel.")
    }
}

/// The pedals plugged into a Logitech wheel.
pub fn pedals(device: &Shared) -> PedalSet<LogitechDevice> {
    let d = device.borrow();
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", d.model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
