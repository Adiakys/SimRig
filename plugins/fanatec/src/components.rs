//! Fanatec devices as standard components, read from and written to hid-fanatecff's sysfs
//! attributes and event node.

use simrig_plugin_sdk::model::StdItem;
use simrig_plugin_sdk::prelude::*;

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::sysfs;

use crate::device::{FanatecDevice, Shared, TUNING_UNREAD};
use crate::models;
use crate::tuning::{self, Entry, Place};

const NO_ATTR: &str = "Not exposed by the installed hid-fanatec for this device.";

/// A tuning entry as a parameter (percent-like entries are floats, the rest whole numbers).
fn tuning_spec(device: &Shared, entry: &Entry) -> Support<ParamSpec> {
    let d = device.borrow();
    if let Some(u) = d.hid.unbound() {
        return Support::Unsupported(u);
    }
    let Some(value) = d.tuning(entry.attr).and_then(|p| sysfs::read_int(&p)) else {
        return Support::not_in_driver();
    };
    if let Some(u) = d.tuning_unread() {
        return Support::Unsupported(u);
    }
    let max = if entry.attr == "SEN" {
        d.max_range() as i64
    } else {
        entry.max
    };
    // Within its range: the driver reports the base's automatic rotation as its maximum plus
    // ten (2530 on a CSL DD), and a newer firmware may go beyond the table.
    let value = value.clamp(entry.min, max);
    // The base stores its tuning menu in its own memory, slot by slot.
    Support::Supported(if entry.unit == "%" {
        ParamSpec::float(value as f64, entry.min as f64, max as f64, 1.0).persistent()
    } else {
        ParamSpec::int(value, entry.min, max, 1).persistent()
    })
}

fn write_tuning(device: &Shared, entry: &Entry, value: &Value) -> Result<()> {
    if device.borrow().tuning_unread().is_some() {
        return Err(Error::failed(TUNING_UNREAD));
    }
    let path = device
        .borrow()
        .tuning(entry.attr)
        .ok_or_else(|| Error::failed(NO_ATTR))?;
    let v = value
        .as_f64()
        .ok_or_else(|| Error::InvalidValue("expected a number".into()))?;
    sysfs::write(&path, &format!("{}", v.round() as i64)).map_err(|e| Error::failed(e.to_string()))
}

fn standard_entry(place: Place) -> &'static Entry {
    tuning::standard(place).expect("every standard tuning entry is in the table")
}

pub struct Base {
    pub device: Shared,
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
        let mut params = Vec::new();
        for (order, entry) in tuning::ENTRIES.iter().enumerate() {
            let Place::Custom { group, advanced } = entry.place else {
                continue;
            };
            let spec = tuning_spec(&self.device, entry);
            let (ty, value) = match &spec {
                Support::Supported(s) => (s.ty.clone(), s.value.clone()),
                Support::Unsupported(_) => (
                    ParamType::Int {
                        min: entry.min,
                        max: entry.max,
                        step: 1,
                    },
                    Value::Int(entry.min),
                ),
            };
            let mut p = CustomParam::new(&tuning::custom_id(entry), entry.label, ty, value)
                .group(group)
                .order(order as i32)
                .persistent();
            if !entry.unit.is_empty() {
                p = p.unit(entry.unit);
            }
            if !entry.help.is_empty() {
                p = p.help(entry.help);
            }
            if advanced {
                p = p.advanced();
            }
            if let Support::Unsupported(u) = spec {
                p = p.availability(Support::Unsupported(u));
            }
            params.push(p);
        }
        params
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        vec![pedals::descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        pedals::start(&self.device, id)
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let entry = tuning::ENTRIES
            .iter()
            .find(|e| tuning::custom_id(e) == id)
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        write_tuning(&self.device, entry, value)
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        if item != "rotation" {
            // The tuning menu lives in the base's memory: already kept.
            return Ok(());
        }
        let degrees = value
            .as_f64()
            .ok_or_else(|| Error::InvalidValue("expected degrees".into()))?;
        host.privileged(
            "save-range",
            &[("degrees", &format!("{}", degrees.round() as i64))],
        )?;
        Ok(())
    }
}

impl WheelBase for Base {
    fn angle(&mut self) -> Support<Degrees> {
        let d = self.device.borrow();
        d.hid.angle(d.range().unwrap_or(d.max_range()))
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(if d.hid.hid.is_some() {
            "ok".into()
        } else {
            "the driver is not bound".into()
        })
    }

    fn firmware(&mut self) -> Support<String> {
        let d = self.device.borrow();
        match d.hid.attr("fw_version").and_then(|p| sysfs::read(&p)) {
            Some(v) if v == "0" => Support::Unsupported(
                Unsupported::new(Reason::Unavailable).with_detail("The base does not report it."),
            ),
            Some(v) => Support::Supported(v),
            None => d
                .hid
                .unbound()
                .map(Support::Unsupported)
                .unwrap_or_else(Support::not_in_driver),
        }
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        let item = param.key();
        if param == WheelBaseParam::Rotation {
            let d = self.device.borrow();
            if let Some(u) = d.hid.unbound() {
                return Support::Unsupported(u);
            }
            let Some(range) = d.range() else {
                return Support::not_in_driver();
            };
            // On bases with a tuning menu the driver sets the rotation through it.
            if let Some(u) = d.tuning_unread() {
                return Support::Unsupported(u);
            }
            // Saved as the driver's start-up rotation (a module option): restored at plug-in.
            return Support::Supported(
                ParamSpec::float(range, 90.0, d.max_range(), 10.0).persistent(),
            );
        }
        match tuning::standard(Place::Base(item)) {
            Some(entry) => tuning_spec(&self.device, entry),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        if param == WheelBaseParam::Rotation {
            if self.device.borrow().tuning_unread().is_some() {
                return Err(Error::failed(TUNING_UNREAD));
            }
            let degrees = value
                .as_f64()
                .ok_or_else(|| Error::InvalidValue("expected degrees".into()))?;
            return self
                .device
                .borrow()
                .hid
                .write("range", &format!("{}", degrees.round() as i64));
        }
        write_tuning(
            &self.device,
            standard_entry(Place::Base(param.key())),
            value,
        )
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
            WheelBaseAction::SetCenter => Support::Unsupported(Unsupported::not_in_driver().with_detail(
                "hid-fanatecff has no centering command: hold the wheel straight and use the base's own menu.",
            )),
            WheelBaseAction::Reset => {
                if d.tuning("RESET").is_some() {
                    Support::Supported(ActionSpec::confirm(
                        "The tuning menu of the current slot goes back to the base's defaults.",
                    ))
                } else {
                    Support::not_in_driver()
                }
            }
            WheelBaseAction::TestEffect | WheelBaseAction::StopForce => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        let mut d = self.device.borrow_mut();
        if let Some(outcome) = d.hid.run_force_action(action) {
            return outcome;
        }
        match action {
            WheelBaseAction::Reset => {
                let path = d.tuning("RESET").ok_or_else(|| Error::failed(NO_ATTR))?;
                sysfs::write(&path, "1").map_err(|e| Error::failed(e.to_string()))?;
                Ok(ActionOutcome::message("Tuning menu reset."))
            }
            _ => Err(Error::unsupported()),
        }
    }
}

pub struct Wheel {
    pub device: Shared,
    pub rim: u16,
}

impl Component for Wheel {
    fn controls(&mut self) -> Vec<Control> {
        self.device.borrow().hid.controls(&[], &[])
    }

    fn id(&self) -> String {
        format!("{}/wheel", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        let d = self.device.borrow();
        let from_driver = d.wheel_dir().and_then(|w| sysfs::read(&w.join("name")));
        from_driver
            .or_else(|| models::rim(self.rim).map(|(n, _)| n.to_string()))
            .unwrap_or_else(|| format!("Fanatec steering wheel {:04x}", self.rim))
    }

    fn appearance(&self) -> simrig_plugin_sdk::model::Appearance {
        simrig_plugin_sdk::model::Appearance::rim(
            models::rim(self.rim)
                .map(|(_, shape)| shape)
                .unwrap_or(simrig_plugin_sdk::model::RimShape::Round),
        )
    }
}

impl SteeringWheel for Wheel {
    fn hats(&mut self) -> Support<Vec<i64>> {
        self.device.borrow().hid.hats()
    }

    fn buttons(&mut self) -> Support<Vec<bool>> {
        self.device.borrow().hid.buttons()
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::Supported(format!("0x{:04x}", self.rim))
    }

    fn param(&mut self, param: SteeringWheelParam) -> Support<ParamSpec> {
        match param {
            SteeringWheelParam::AnalogPaddleMode => {
                let d = self.device.borrow();
                let Some(v) = d.tuning("ACP").and_then(|p| sysfs::read_int(&p)) else {
                    return Support::not_in_driver();
                };
                if let Some(u) = d.tuning_unread() {
                    return Support::Unsupported(u);
                }
                // 0: a rim without analog paddles, or the base has not sent this entry of
                // its menu yet (it comes with the rest a moment later).
                let current = v.to_string();
                if !tuning::PADDLE_MODES.iter().any(|(id, _)| *id == current) {
                    return Support::not_in_hardware(
                        "The steering wheel reports no analog paddle mode.",
                    );
                }
                Support::Supported(ParamSpec::choice(&current, &tuning::PADDLE_MODES).persistent())
            }
            SteeringWheelParam::Vibration => {
                tuning_spec(&self.device, standard_entry(Place::Wheel("vibration")))
            }
            SteeringWheelParam::LedBrightness => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("The rim's LEDs are driven by games."),
            ),
        }
    }

    fn set_param(&mut self, param: SteeringWheelParam, value: &Value) -> Result<()> {
        match (param, value) {
            (SteeringWheelParam::AnalogPaddleMode, Value::Choice(mode)) => {
                if self.device.borrow().tuning_unread().is_some() {
                    return Err(Error::failed(TUNING_UNREAD));
                }
                let path = self
                    .device
                    .borrow()
                    .tuning("ACP")
                    .ok_or_else(|| Error::failed(NO_ATTR))?;
                sysfs::write(&path, mode).map_err(|e| Error::failed(e.to_string()))
            }
            (SteeringWheelParam::Vibration, v) => {
                write_tuning(&self.device, standard_entry(Place::Wheel("vibration")), v)
            }
            _ => Err(Error::unsupported()),
        }
    }

    fn output(&mut self, output: SteeringWheelOutput) -> Support<ParamType> {
        match output {
            SteeringWheelOutput::Display if self.device.borrow().hid.attr("display").is_some() => {
                Support::Supported(ParamType::Text { max_len: 3 })
            }
            SteeringWheelOutput::RpmLeds | SteeringWheelOutput::FlagLeds => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("The rim's LEDs are driven by games."),
            ),
            _ => Support::not_in_driver(),
        }
    }

    fn write_output(&mut self, output: SteeringWheelOutput, value: &Value) -> Result<()> {
        match (output, value) {
            (SteeringWheelOutput::Display, Value::Text(text)) => {
                self.device.borrow().hid.write("display", text)
            }
            _ => Err(Error::unsupported()),
        }
    }
}

/// Fanatec pedals: plugged into a base, or a USB pedal set of their own.
pub fn pedals(device: &Shared) -> PedalSet<FanatecDevice> {
    let d = device.borrow();
    let on_base = matches!(d.model.kind, models::Kind::Base { .. });
    PedalSet {
        device: device.clone(),
        name: if on_base {
            format!("Pedals on the {}", d.model.name)
        } else {
            d.model.name.into()
        },
        model: (!on_base).then(|| d.model.model.into()),
        notes: PedalNotes {
            calibration: "Fanatec pedals calibrate themselves (see the base's menu).",
            shaping: "hid-fanatecff has no dead zones or curves: set them in the game.",
            max_force: "Set as \"Brake force (load cell)\" in the base's settings.",
        },
    }
}
