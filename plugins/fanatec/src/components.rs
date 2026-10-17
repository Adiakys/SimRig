//! Fanatec devices as standard components, read from and written to hid-fanatecff's sysfs
//! attributes and event node.

use simrig_plugin_sdk::model::StdItem;
use simrig_plugin_sdk::prelude::*;

use crate::device::{Shared, save_map};
use crate::models;
use crate::sysfs;
use crate::tuning::{self, Entry, Place};

const NO_ATTR: &str = "Not exposed by the installed hid-fanatec for this device.";

/// A tuning entry as a parameter (percent-like entries are floats, the rest whole numbers).
fn tuning_spec(device: &Shared, entry: &Entry) -> Support<ParamSpec> {
    let d = device.borrow();
    if let Some(u) = d.unbound() {
        return Support::Unsupported(u);
    }
    let Some(value) = d.tuning(entry.attr).and_then(|p| sysfs::read_int(&p)) else {
        return Support::not_in_driver();
    };
    let max = if entry.attr == "SEN" {
        d.max_range() as i64
    } else {
        entry.max
    };
    // The base stores its tuning menu in its own memory, slot by slot.
    Support::Supported(if entry.unit == "%" {
        ParamSpec::float(value as f64, entry.min as f64, max as f64, 1.0).persistent()
    } else {
        ParamSpec::int(value, entry.min, max, 1).persistent()
    })
}

fn write_tuning(device: &Shared, entry: &Entry, value: &Value) -> Result<()> {
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
        format!("{}/base", self.device.borrow().id)
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
        vec![map_pedals_descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        if id != "map-pedals" {
            return Err(Error::NotFound(format!("unknown procedure '{id}'")));
        }
        Ok(Box::new(MapPedals::new(self.device.clone(), true)))
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
        if let Some(u) = d.unbound() {
            return Support::Unsupported(u);
        }
        let Some(axis) = d.inputs.axes().get(&0).copied() else {
            return Support::Unsupported(
                Unsupported::new(Reason::Unavailable)
                    .with_detail(d.inputs.error().unwrap_or("No steering axis.")),
            );
        };
        let range = d.range().unwrap_or(d.max_range());
        Support::Supported((axis.unit() - 0.5) * range)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(if d.paths.is_some() {
            "ok".into()
        } else {
            "the driver is not bound".into()
        })
    }

    fn firmware(&mut self) -> Support<String> {
        let d = self.device.borrow();
        match d.attr("fw_version").and_then(|p| sysfs::read(&p)) {
            Some(v) if v == "0" => Support::Unsupported(
                Unsupported::new(Reason::Unavailable).with_detail("The base does not report it."),
            ),
            Some(v) => Support::Supported(v),
            None => d
                .unbound()
                .map(Support::Unsupported)
                .unwrap_or_else(Support::not_in_driver),
        }
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        let item = param.key();
        if param == WheelBaseParam::Rotation {
            let d = self.device.borrow();
            if let Some(u) = d.unbound() {
                return Support::Unsupported(u);
            }
            let Some(range) = d.range() else {
                return Support::not_in_driver();
            };
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
            let path = self
                .device
                .borrow()
                .attr("range")
                .ok_or_else(|| Error::failed(NO_ATTR))?;
            let degrees = value
                .as_f64()
                .ok_or_else(|| Error::InvalidValue("expected degrees".into()))?;
            return sysfs::write(&path, &format!("{}", degrees.round() as i64))
                .map_err(|e| Error::failed(e.to_string()));
        }
        write_tuning(
            &self.device,
            standard_entry(Place::Base(param.key())),
            value,
        )
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        if let Some(u) = self.device.borrow().unbound() {
            return Support::Unsupported(u);
        }
        match action {
            WheelBaseAction::SetCenter => Support::Unsupported(Unsupported::not_in_driver().with_detail(
                "hid-fanatecff has no centering command: hold the wheel straight and use the base's own menu.",
            )),
            WheelBaseAction::TestEffect => Support::Supported(ActionSpec::confirm(
                "The wheel turns left and right by itself for two seconds. Hold it firmly.",
            )),
            WheelBaseAction::StopForce => Support::Supported(ActionSpec::default()),
            WheelBaseAction::Reset => {
                if self.device.borrow().tuning("RESET").is_some() {
                    Support::Supported(ActionSpec::confirm(
                        "The tuning menu of the current slot goes back to the base's defaults.",
                    ))
                } else {
                    Support::not_in_driver()
                }
            }
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        let mut d = self.device.borrow_mut();
        match action {
            WheelBaseAction::TestEffect => {
                d.inputs
                    .test_effect()
                    .map_err(|e| Error::failed(e.to_string()))?;
                Ok(ActionOutcome::message("Force feedback test running."))
            }
            WheelBaseAction::StopForce => {
                d.inputs
                    .stop_force()
                    .map_err(|e| Error::failed(e.to_string()))?;
                Ok(ActionOutcome::message("Forces stopped."))
            }
            WheelBaseAction::Reset => {
                let path = d.tuning("RESET").ok_or_else(|| Error::failed(NO_ATTR))?;
                sysfs::write(&path, "1").map_err(|e| Error::failed(e.to_string()))?;
                Ok(ActionOutcome::message("Tuning menu reset."))
            }
            WheelBaseAction::SetCenter => Err(Error::unsupported()),
        }
    }
}

pub struct Wheel {
    pub device: Shared,
    pub rim: u16,
}

impl Component for Wheel {
    fn id(&self) -> String {
        format!("{}/wheel", self.device.borrow().id)
    }

    fn name(&self) -> String {
        let d = self.device.borrow();
        let from_driver = d
            .paths
            .as_ref()
            .and_then(|p| p.wheel())
            .and_then(|w| sysfs::read(&w.join("name")));
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
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        let buttons = d.inputs.buttons();
        if buttons.is_empty() {
            return Support::not_in_driver();
        }
        Support::Supported(buttons.into_iter().map(|(_, pressed)| pressed).collect())
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
                Support::Supported(
                    ParamSpec::choice(&v.to_string(), &tuning::PADDLE_MODES).persistent(),
                )
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
            SteeringWheelOutput::Display if self.device.borrow().attr("display").is_some() => {
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
                let path = self
                    .device
                    .borrow()
                    .attr("display")
                    .ok_or_else(|| Error::failed(NO_ATTR))?;
                sysfs::write(&path, text).map_err(|e| Error::failed(e.to_string()))
            }
            _ => Err(Error::unsupported()),
        }
    }
}

pub struct Pedals {
    pub device: Shared,
    /// The base's pedal inputs, or a pedal set of its own.
    pub on_base: bool,
}

impl Component for Pedals {
    fn id(&self) -> String {
        format!("{}/pedals", self.device.borrow().id)
    }

    fn name(&self) -> String {
        let d = self.device.borrow();
        if self.on_base {
            format!("Pedals on the {}", d.model.name)
        } else {
            d.model.name.into()
        }
    }

    fn model(&self) -> Option<String> {
        (!self.on_base).then(|| self.device.borrow().model.model.into())
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        vec![map_pedals_descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        if id != "map-pedals" {
            return Err(Error::NotFound(format!("unknown procedure '{id}'")));
        }
        Ok(Box::new(MapPedals::new(self.device.clone(), self.on_base)))
    }
}

impl simrig_plugin_sdk::Pedals for Pedals {
    fn pedals(&self) -> Vec<PedalRole> {
        self.device.borrow().pedal_map.roles()
    }

    fn position(&mut self, pedal: PedalRole) -> Support<Axis> {
        let d = self.device.borrow();
        if let Some(u) = d.unbound() {
            return Support::Unsupported(u);
        }
        let Some(mapped) = d.pedal_map.pedals.get(pedal.key()).copied() else {
            return Support::Unsupported(
                Unsupported::not_in_driver().with_detail("Map the pedals first."),
            );
        };
        let Some(axis) = d.inputs.axes().get(&mapped.code).copied() else {
            return Support::Unsupported(Unsupported::new(Reason::Unavailable));
        };
        let unit = axis.unit();
        Support::Supported(Axis {
            raw: i64::from(axis.value),
            value: if mapped.inverted { 1.0 - unit } else { unit },
        })
    }

    fn force(&mut self, _pedal: PedalRole) -> Support<Kilograms> {
        Support::not_in_driver()
    }

    fn param(&mut self, _pedal: PedalRole, param: PedalParam) -> Support<ParamSpec> {
        let detail = match param {
            PedalParam::CalibrationMin | PedalParam::CalibrationMax => {
                "Fanatec pedals calibrate themselves (see the base's menu)."
            }
            PedalParam::DeadzoneLow | PedalParam::DeadzoneHigh | PedalParam::Curve => {
                "hid-fanatecff has no dead zones or curves: set them in the game."
            }
            PedalParam::MaxForce => "Set as \"Brake force (load cell)\" in the base's settings.",
            PedalParam::Inverted | PedalParam::Vibration => return Support::not_in_driver(),
        };
        Support::Unsupported(Unsupported::not_in_driver().with_detail(detail))
    }

    fn action(&mut self, _pedal: PedalRole, action: PedalAction) -> Support<ActionSpec> {
        match action {
            PedalAction::Calibrate => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("Fanatec pedals calibrate themselves (see the base's menu)."),
            ),
            PedalAction::VibrationTest => Support::not_in_driver(),
        }
    }
}

pub fn map_pedals_descriptor(device: &Shared) -> ProcedureDescriptor {
    let d = device.borrow();
    let mapped = !d.pedal_map.pedals.is_empty();
    let label = match (mapped, d.model.kind) {
        (true, _) => "Map the pedals again",
        (false, models::Kind::Base { .. }) => "Map the pedals connected to the base",
        (false, models::Kind::Pedals) => "Map the pedals",
    };
    let mut descriptor = ProcedureDescriptor::new("map-pedals", label)
        .help("Press each pedal when asked, so SimRig knows which input is which.");
    if let Some(u) = d.unbound() {
        descriptor = descriptor.availability(Support::Unsupported(u));
    }
    descriptor
}

/// The guided mapping: at rest, then each pedal pressed in turn; the axis that moves most is
/// that pedal. Saved in the user's configuration (it only tells SimRig how to read them).
pub struct MapPedals {
    device: Shared,
    on_base: bool,
    rest: std::collections::BTreeMap<u16, f64>,
    /// The pedal being pressed, by index in `ORDER`.
    step: usize,
    map: crate::device::PedalMap,
}

const ORDER: [PedalRole; 3] = [PedalRole::Throttle, PedalRole::Brake, PedalRole::Clutch];
/// How far an axis must move to count as the pedal pressed.
const PRESSED: f64 = 0.5;

impl MapPedals {
    pub fn new(device: Shared, on_base: bool) -> Self {
        Self {
            device,
            on_base,
            rest: Default::default(),
            step: 0,
            map: Default::default(),
        }
    }

    fn axes(&self) -> std::collections::BTreeMap<u16, f64> {
        self.device
            .borrow()
            .inputs
            .axes()
            .into_iter()
            // A base's first axis is the steering.
            .filter(|(code, _)| !(self.on_base && *code == 0))
            .map(|(code, a)| (code, a.unit()))
            .collect()
    }

    fn table(&self) -> Element {
        Element::Table {
            columns: vec!["Pedal".into(), "Input".into()],
            rows: ORDER
                .iter()
                .map(|role| {
                    let input = match self.map.pedals.get(role.key()) {
                        Some(m) => format!(
                            "axis {}{}",
                            m.code,
                            if m.inverted { ", inverted" } else { "" }
                        ),
                        None => "not mapped".into(),
                    };
                    vec![crate::role_label(*role).into(), input]
                })
                .collect(),
        }
    }

    fn press_step(&self) -> Step {
        let role = ORDER[self.step];
        let name = crate::role_label(role).to_lowercase();
        Step::new(&format!("Press the {name} all the way"))
            .text("Hold it until it is detected, then release it.")
            .element(self.table())
            .preview(Preview::new(ComponentKind::Pedals).target(role.key()))
            .waiting()
            .progress(self.step as u32 + 1, ORDER.len() as u32)
            .action(StepAction::new(
                "skip",
                &format!("No {name}"),
                ActionRole::Secondary,
            ))
            .action(StepAction::cancel())
    }

    fn summary(&self) -> Step {
        Step::new("Check the pedals")
            .element(self.table())
            .action(StepAction::new("save", "Save", ActionRole::Primary))
            .action(StepAction::new(
                "restart",
                "Start again",
                ActionRole::Secondary,
            ))
            .action(StepAction::cancel())
    }

    fn next(&mut self) -> Outcome {
        self.step += 1;
        if self.step < ORDER.len() {
            Outcome::Step(self.press_step())
        } else {
            Outcome::Step(self.summary())
        }
    }
}

impl Procedure for MapPedals {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        if let Some(u) = self.device.borrow().unbound() {
            return Err(Error::failed(u.explain()));
        }
        Ok(Outcome::Step(
            Step::new("Release every pedal")
                .text("Feet off the pedals, then continue.")
                .action(StepAction::next())
                .action(StepAction::cancel()),
        ))
    }

    fn submit(
        &mut self,
        action: &str,
        _values: &std::collections::BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        match action {
            "cancel" => Ok(Outcome::Cancelled),
            "next" => {
                self.rest = self.axes();
                self.step = 0;
                self.map = Default::default();
                Ok(Outcome::Step(self.press_step()))
            }
            "skip" => Ok(self.next()),
            "restart" => self.start(host),
            "save" => {
                let mut d = self.device.borrow_mut();
                save_map(&d.config, d.model.pid, &self.map)
                    .map_err(|e| Error::failed(e.to_string()))?;
                d.pedal_map = self.map.clone();
                let n = self.map.pedals.len();
                Ok(Outcome::finished(format!(
                    "{n} pedal{} mapped.",
                    if n == 1 { "" } else { "s" }
                )))
            }
            other => Err(Error::InvalidValue(format!("unexpected action '{other}'"))),
        }
    }

    fn poll(&mut self) -> Option<Outcome> {
        if self.step >= ORDER.len() || self.rest.is_empty() {
            return None;
        }
        let taken: Vec<u16> = self.map.pedals.values().map(|m| m.code).collect();
        let moved = self
            .axes()
            .into_iter()
            .filter(|(code, _)| !taken.contains(code))
            .filter_map(|(code, now)| {
                let rest = *self.rest.get(&code)?;
                let delta = now - rest;
                (delta.abs() >= PRESSED).then_some((code, delta))
            })
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))?;
        self.map.pedals.insert(
            ORDER[self.step].key().into(),
            crate::device::MappedAxis {
                code: moved.0,
                inverted: moved.1 < 0.0,
            },
        );
        Some(self.next())
    }
}
