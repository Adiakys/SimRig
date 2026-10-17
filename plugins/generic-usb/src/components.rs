//! A set-up device as standard components. Values come from its raw reports decoded with the
//! setup shown in the app; changes apply in the app at once and reach games once saved.

use simrig_plugin_sdk::prelude::*;

use crate::adapter::{Shared, setup_descriptor};
use crate::config::{HandbrakeInput, HandbrakeSetup, Setup};
use crate::setup::DeviceSetup;

const AGAIN: &str = "Run “Set up again” to map it differently.";

fn unavailable<T>(state: &Shared) -> Option<Support<T>> {
    let s = state.borrow();
    if s.reports.is_none() {
        let detail = s
            .read_error
            .clone()
            .unwrap_or_else(|| "The device cannot be read.".into());
        return Some(Support::Unsupported(
            Unsupported::new(Reason::Unavailable).with_detail(detail),
        ));
    }
    None
}

/// The setup procedure, offered on the component too.
fn procedures(state: &Shared) -> Vec<ProcedureDescriptor> {
    let set_up = state.borrow().saved.is_some();
    let mut list = vec![setup_descriptor(set_up)];
    if set_up {
        list.push(crate::adapter::forget_descriptor());
    }
    list
}

fn start(state: &Shared, id: &str) -> Result<Box<dyn Procedure>> {
    if id == "forget" {
        return Ok(Box::new(crate::adapter::Forget {
            state: state.clone(),
        }));
    }
    if id != "setup" {
        return Err(Error::NotFound(format!("unknown procedure '{id}'")));
    }
    let data = std::env::var_os("SIMRIG_DRIVER_DATA").map(std::path::PathBuf::from);
    let installed = data.is_some_and(|d| crate::adapter::GenericAdapter::driver_installed(&d));
    Ok(Box::new(DeviceSetup::new(state.clone(), installed)))
}

pub struct GenericShifter {
    state: Shared,
}

impl GenericShifter {
    pub fn new(state: Shared) -> Self {
        Self { state }
    }

    fn sequential_mode(&self) -> bool {
        matches!(
            self.state.borrow().draft.as_ref().map(|c| &c.setup),
            Some(Setup::Sequential { .. })
        )
    }
}

impl Component for GenericShifter {
    fn id(&self) -> String {
        format!("{}/shifter", self.state.borrow().id)
    }

    fn name(&self) -> String {
        self.state.borrow().name.clone()
    }

    fn save(&mut self, _item: &str, _value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.state.borrow_mut().save(host)
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        procedures(&self.state)
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        start(&self.state, id)
    }
}

impl Shifter for GenericShifter {
    fn gear(&mut self) -> Support<Gear> {
        if self.sequential_mode() {
            return Support::depends_on("mode", Value::Choice("h_pattern".into()));
        }
        if let Some(u) = unavailable(&self.state) {
            return u;
        }
        let s = self.state.borrow();
        let gears = match s.draft.as_ref().map(|c| &c.setup) {
            Some(Setup::HShifter { gears, .. }) => *gears,
            _ => return Support::not_in_driver(),
        };
        let slot = s.state().and_then(|st| st.slot);
        Support::Supported(match slot {
            None => Gear::Neutral,
            Some(i) if i == usize::from(gears) => Gear::Reverse,
            Some(i) => Gear::Forward(i as u8 + 1),
        })
    }

    fn position_xy(&mut self) -> Support<[f64; 2]> {
        Support::not_in_hardware("The positions are switches, not a lever position.")
    }

    fn sequential(&mut self) -> Support<[bool; 2]> {
        if !self.sequential_mode() {
            return Support::depends_on("mode", Value::Choice("sequential".into()));
        }
        if let Some(u) = unavailable(&self.state) {
            return u;
        }
        Support::Supported(
            self.state
                .borrow()
                .state()
                .map(|s| s.sequential)
                .unwrap_or_default(),
        )
    }

    fn param(&mut self, param: ShifterParam) -> Support<ParamSpec> {
        let s = self.state.borrow();
        let Some(config) = &s.draft else {
            return Support::not_in_driver();
        };
        match (param, &config.setup) {
            // Set by the setup: one value, shown so the gate is drawn right.
            (ShifterParam::Gears, Setup::HShifter { gears, .. }) => {
                let g = i64::from(*gears);
                Support::Supported(ParamSpec::int(g, g, g, 1).persistent())
            }
            (ShifterParam::Gears, Setup::Sequential { .. }) => {
                Support::depends_on("mode", Value::Choice("h_pattern".into()))
            }
            (
                ShifterParam::ReversePosition,
                Setup::HShifter {
                    reverse_position, ..
                },
            ) => Support::Supported(
                ParamSpec::choice(
                    reverse_position,
                    &[("left_up", "Left, up"), ("right_down", "Right, down")],
                )
                .persistent(),
            ),
            (ShifterParam::ReversePosition, Setup::Sequential { .. }) => {
                Support::depends_on("mode", Value::Choice("h_pattern".into()))
            }
            (ShifterParam::Mode, Setup::HShifter { .. }) => Support::Supported(
                ParamSpec::choice("h_pattern", &[("h_pattern", "H-pattern")]).persistent(),
            ),
            (ShifterParam::Mode, Setup::Sequential { .. }) => Support::Supported(
                ParamSpec::choice("sequential", &[("sequential", "Sequential")]).persistent(),
            ),
            (ShifterParam::EngageThreshold, _) => {
                Support::not_in_hardware("The positions are switches: there is no threshold.")
            }
            _ => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: ShifterParam, value: &Value) -> Result<()> {
        let mut s = self.state.borrow_mut();
        let Some(config) = &mut s.draft else {
            return Err(Error::unsupported());
        };
        match (param, &mut config.setup, value) {
            (
                ShifterParam::ReversePosition,
                Setup::HShifter {
                    reverse_position, ..
                },
                Value::Choice(v),
            ) => {
                *reverse_position = v.clone();
                Ok(())
            }
            // Gears and mode take a new mapping.
            (ShifterParam::Gears | ShifterParam::Mode, ..) => Err(Error::failed(AGAIN)),
            _ => Err(Error::unsupported()),
        }
    }

    fn action(&mut self, action: ShifterAction) -> Support<ActionSpec> {
        match action {
            ShifterAction::CalibrateGears => Support::Unsupported(
                Unsupported::not_in_driver().with_detail("The gears are mapped by “Set up again”."),
            ),
        }
    }
}

pub struct GenericHandbrake {
    state: Shared,
}

impl GenericHandbrake {
    pub fn new(state: Shared) -> Self {
        Self { state }
    }

    fn with_setup<T>(&self, f: impl FnOnce(&HandbrakeSetup) -> T) -> Option<T> {
        match self.state.borrow().draft.as_ref().map(|c| &c.setup) {
            Some(Setup::Handbrake(h)) => Some(f(h)),
            _ => None,
        }
    }

    fn is_axis(&self) -> bool {
        self.with_setup(|h| matches!(h.input, HandbrakeInput::Axis { .. }))
            .unwrap_or(false)
    }
}

const BUTTON_ONLY: &str = "This handbrake is an on/off switch.";

impl Component for GenericHandbrake {
    fn id(&self) -> String {
        format!("{}/handbrake", self.state.borrow().id)
    }

    fn name(&self) -> String {
        self.state.borrow().name.clone()
    }

    fn save(&mut self, _item: &str, _value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.state.borrow_mut().save(host)
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        procedures(&self.state)
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        start(&self.state, id)
    }
}

impl Handbrake for GenericHandbrake {
    fn position(&mut self) -> Support<Axis> {
        if let Some(u) = unavailable(&self.state) {
            return u;
        }
        let state = self.state.borrow().state().unwrap_or_default();
        Support::Supported(Axis {
            raw: state.raw.unwrap_or(if state.pressed { 1 } else { 0 }),
            value: state.position,
        })
    }

    fn pressed(&mut self) -> Support<bool> {
        let button_mode = self.with_setup(|h| h.mode == "button").unwrap_or(false);
        if self.is_axis() && !button_mode {
            return Support::depends_on("mode", Value::Choice("button".into()));
        }
        if let Some(u) = unavailable(&self.state) {
            return u;
        }
        Support::Supported(self.state.borrow().state().is_some_and(|s| s.pressed))
    }

    fn param(&mut self, param: HandbrakeParam) -> Support<ParamSpec> {
        let axis = self.is_axis();
        let Some(spec) = self.with_setup(|h| {
            let full = match h.input {
                HandbrakeInput::Axis { size: 2, .. } => 65_535,
                _ => 255,
            };
            match param {
                HandbrakeParam::CalibrationMin if axis => {
                    Support::Supported(ParamSpec::int(h.calibration_min, 0, full, 1).persistent())
                }
                HandbrakeParam::CalibrationMax if axis => {
                    Support::Supported(ParamSpec::int(h.calibration_max, 0, full, 1).persistent())
                }
                HandbrakeParam::Deadzone if axis => {
                    Support::Supported(ParamSpec::float(h.deadzone, 0.0, 50.0, 0.5).persistent())
                }
                HandbrakeParam::Curve if axis => Support::Supported(
                    ParamSpec::curve(
                        h.curve
                            .iter()
                            .map(|p| CurvePoint::new(p[0], p[1]))
                            .collect(),
                        8,
                    )
                    .persistent(),
                ),
                HandbrakeParam::Mode if axis => Support::Supported(
                    ParamSpec::choice(&h.mode, &[("analog", "Analog"), ("button", "Button")])
                        .persistent(),
                ),
                HandbrakeParam::Mode => Support::Supported(
                    ParamSpec::choice("button", &[("button", "Button")]).persistent(),
                ),
                HandbrakeParam::ButtonThreshold if axis && h.mode == "button" => {
                    Support::Supported(
                        ParamSpec::float(h.button_threshold, 5.0, 95.0, 1.0).persistent(),
                    )
                }
                HandbrakeParam::ButtonThreshold if axis => {
                    Support::depends_on("mode", Value::Choice("button".into()))
                }
                _ => Support::not_in_hardware(BUTTON_ONLY),
            }
        }) else {
            return Support::not_in_driver();
        };
        spec
    }

    fn set_param(&mut self, param: HandbrakeParam, value: &Value) -> Result<()> {
        let mut s = self.state.borrow_mut();
        let Some(Setup::Handbrake(h)) = s.draft.as_mut().map(|c| &mut c.setup) else {
            return Err(Error::unsupported());
        };
        match (param, value) {
            (HandbrakeParam::CalibrationMin, Value::Int(v)) => h.calibration_min = *v,
            (HandbrakeParam::CalibrationMax, Value::Int(v)) => h.calibration_max = *v,
            (HandbrakeParam::Deadzone, Value::Float(v)) => h.deadzone = *v,
            (HandbrakeParam::Curve, Value::Curve(points)) => {
                h.curve = points.iter().map(|p| [p.x, p.y]).collect();
            }
            (HandbrakeParam::Mode, Value::Choice(v)) => h.mode = v.clone(),
            (HandbrakeParam::ButtonThreshold, Value::Float(v)) => h.button_threshold = *v,
            _ => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn action(&mut self, action: HandbrakeAction) -> Support<ActionSpec> {
        match action {
            HandbrakeAction::Calibrate => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("Its travel is measured by “Set up again”, or set by hand."),
            ),
        }
    }
}
