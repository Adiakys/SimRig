use simrig_plugin_sdk::prelude::*;

use crate::sim::{AxisShape, Identity, RAW_MAX, Shared, as_choice, as_f64, raw_from_unit};

const ANALOG: &str = "analog";
const BUTTON: &str = "button";

/// Simulated handbrake with an analog and a button mode.
pub struct SimHandbrake {
    shared: Shared,
    identity: Identity,
    shape: AxisShape,
    mode: String,
    button_threshold: Percent,
}

impl SimHandbrake {
    pub fn new(shared: Shared, identity: Identity) -> Self {
        Self {
            shared,
            identity,
            shape: AxisShape::default(),
            mode: ANALOG.into(),
            button_threshold: 50.0,
        }
    }

    fn only_in<T>(&self, mode: &str, value: impl FnOnce() -> T) -> Support<T> {
        if self.mode == mode {
            Support::Supported(value())
        } else {
            Support::depends_on("mode", Value::Choice(mode.into()))
        }
    }

    fn axis(&self) -> Axis {
        // A pull every couple of periods.
        let mut s = self.shared.get();
        let pull = s.wave(0.0).max(0.0).powi(3) + s.jitter() * 0.05;
        self.shape.apply(raw_from_unit(pull))
    }
}

impl Component for SimHandbrake {
    fn id(&self) -> String {
        self.identity.id.clone()
    }

    fn name(&self) -> String {
        self.identity.name.clone()
    }

    fn model(&self) -> Option<String> {
        Some(self.identity.model.clone())
    }
    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.shared.save(&self.id(), item, value, host)
    }
}

impl Handbrake for SimHandbrake {
    fn position(&mut self) -> Support<Axis> {
        Support::Supported(self.axis())
    }

    fn pressed(&mut self) -> Support<bool> {
        self.only_in(BUTTON, || {
            self.axis().value * 100.0 >= self.button_threshold
        })
    }

    fn param(&mut self, param: HandbrakeParam) -> Support<ParamSpec> {
        match param {
            HandbrakeParam::CalibrationMin => {
                Support::Supported(ParamSpec::int(self.shape.cal_min, 0, RAW_MAX, 1).persistent())
            }
            HandbrakeParam::CalibrationMax => {
                Support::Supported(ParamSpec::int(self.shape.cal_max, 0, RAW_MAX, 1).persistent())
            }
            HandbrakeParam::Curve => self.only_in(ANALOG, || {
                ParamSpec::curve(self.shape.curve.clone(), 8).persistent()
            }),
            HandbrakeParam::Deadzone => self.only_in(ANALOG, || {
                ParamSpec::float(self.shape.deadzone_low, 0.0, 50.0, 0.5).persistent()
            }),
            HandbrakeParam::Mode => Support::Supported(
                ParamSpec::choice(&self.mode, &[(ANALOG, "Analog axis"), (BUTTON, "Button")])
                    .persistent(),
            ),
            HandbrakeParam::ButtonThreshold => self.only_in(BUTTON, || {
                ParamSpec::float(self.button_threshold, 5.0, 95.0, 1.0).persistent()
            }),
        }
    }

    fn set_param(&mut self, param: HandbrakeParam, value: &Value) -> Result<()> {
        match (param, value) {
            (HandbrakeParam::CalibrationMin, Value::Int(v)) => {
                self.shape.check_calibration(*v, self.shape.cal_max)?;
                self.shape.cal_min = *v;
            }
            (HandbrakeParam::CalibrationMax, Value::Int(v)) => {
                self.shape.check_calibration(self.shape.cal_min, *v)?;
                self.shape.cal_max = *v;
            }
            (HandbrakeParam::Curve, Value::Curve(points)) => self.shape.curve = points.clone(),
            (HandbrakeParam::Deadzone, v) => self.shape.deadzone_low = as_f64(v),
            (HandbrakeParam::Mode, v) => self.mode = as_choice(v),
            (HandbrakeParam::ButtonThreshold, v) => self.button_threshold = as_f64(v),
            _ => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn action(&mut self, _action: HandbrakeAction) -> Support<ActionSpec> {
        Support::Supported(ActionSpec::default())
    }

    fn run_action(&mut self, _action: HandbrakeAction, _args: &Value) -> Result<ActionOutcome> {
        self.shape.cal_min = 0;
        self.shape.cal_max = RAW_MAX;
        Ok(ActionOutcome::message(
            "Handbrake calibrated to the full travel.",
        ))
    }
}
