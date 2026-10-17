use simrig_plugin_sdk::model::{ParamType, Reason};
use simrig_plugin_sdk::prelude::*;

use crate::sim::{Shared, as_f64};

/// Simulated direct-drive base. It has no inertia filter (`NotInHardware`) and owns the
/// simulation's custom parameters.
pub struct SimBase {
    shared: Shared,
    rotation: Degrees,
    ffb_strength: Percent,
    natural_damper: Percent,
    natural_friction: Percent,
    spring_strength: Percent,
    damper_strength: Percent,
    center: Degrees,
    test_effect: bool,
}

impl SimBase {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            rotation: 900.0,
            ffb_strength: 75.0,
            natural_damper: 10.0,
            natural_friction: 5.0,
            spring_strength: 0.0,
            damper_strength: 50.0,
            center: 0.0,
            test_effect: false,
        }
    }

    fn raw_angle(&self) -> Degrees {
        let mut s = self.shared.get();
        let swing = self.rotation / 2.0 * 0.6;
        s.wave(0.0) * swing + s.jitter() * 10.0
    }

    fn reset(&mut self) {
        let shared = self.shared.clone();
        *self = Self::new(shared);
    }
}

impl Component for SimBase {
    fn id(&self) -> String {
        format!("{}/base", crate::RIG)
    }

    fn name(&self) -> String {
        "Simulated direct-drive base".into()
    }

    fn model(&self) -> Option<String> {
        Some("sim/dd-base".into())
    }

    fn custom_params(&mut self) -> Vec<CustomParam> {
        let s = self.shared.get();
        let advanced = Value::Bool(s.advanced_mode);
        let rims = [
            ("formula", "Formula rim"),
            ("gt", "GT rim"),
            (crate::sim::NO_RIM, "No rim"),
        ];
        vec![
            CustomParam::new(
                "rim",
                "Mounted rim",
                ParamType::Choice {
                    options: rims
                        .iter()
                        .map(|(id, label)| ChoiceOption::new(id, label))
                        .collect(),
                },
                Value::Choice(s.rim.clone()),
            )
            .default_value(Value::Choice("formula".into()))
            .group("Simulation")
            .order(10)
            .help("Simulates swapping the rim on the base."),
            CustomParam::new(
                "handbrake_connected",
                "Handbrake connected",
                ParamType::Bool,
                Value::Bool(s.handbrake_connected),
            )
            .default_value(Value::Bool(true))
            .group("Simulation")
            .order(11)
            .help("Simulates plugging and unplugging the handbrake."),
            CustomParam::new(
                "wave_speed",
                "Wave speed",
                ParamType::Float {
                    min: 0.05,
                    max: 2.0,
                    step: 0.05,
                },
                Value::Float(s.wave_speed),
            )
            .default_value(Value::Float(0.2))
            .unit("Hz")
            .group("Simulation")
            .help("How fast the simulated inputs move."),
            CustomParam::new(
                "noise",
                "Noise",
                ParamType::Float {
                    min: 0.0,
                    max: 1.0,
                    step: 0.01,
                },
                Value::Float(s.noise),
            )
            .default_value(Value::Float(0.0))
            .group("Simulation")
            .order(1)
            .help("Random noise added to analog inputs."),
            CustomParam::new(
                "advanced_mode",
                "Advanced mode",
                ParamType::Bool,
                advanced.clone(),
            )
            .default_value(Value::Bool(false))
            .group("Developer")
            .advanced(),
            CustomParam::new(
                "demo_filter",
                "Demo filter",
                ParamType::Int {
                    min: 0,
                    max: 10,
                    step: 1,
                },
                Value::Int(s.demo_filter),
            )
            .default_value(Value::Int(3))
            .group("Developer")
            .order(1)
            .advanced()
            .enabled_if("custom.advanced_mode", Value::Bool(true), &advanced),
            CustomParam::new("turbo", "Turbo mode", ParamType::Bool, Value::Bool(false))
                .group("Developer")
                .order(2)
                .availability(Support::Unsupported(
                    Unsupported::new(Reason::NeedsFirmware { min: "2.0".into() })
                        .with_detail("The simulated base runs firmware 1.0."),
                )),
        ]
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let mut s = self.shared.get();
        match (id, value) {
            ("rim", Value::Choice(v)) => s.rim = v.clone(),
            ("handbrake_connected", Value::Bool(v)) => s.handbrake_connected = *v,
            ("wave_speed", Value::Float(v)) => s.wave_speed = *v,
            ("noise", Value::Float(v)) => s.noise = *v,
            ("advanced_mode", Value::Bool(v)) => s.advanced_mode = *v,
            ("demo_filter", Value::Int(v)) => s.demo_filter = *v,
            _ => return Err(Error::unsupported()),
        }
        Ok(())
    }
    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.shared.save(&self.id(), item, value, host)
    }
}

impl WheelBase for SimBase {
    fn angle(&mut self) -> Support<Degrees> {
        Support::Supported(self.raw_angle() - self.center)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        let s = self.shared.get();
        let swing = self.rotation / 2.0 * 0.6;
        let omega = std::f64::consts::TAU * s.wave_speed;
        Support::Supported(s.wave(0.25) * swing * omega)
    }

    fn status(&mut self) -> Support<String> {
        let status = if self.test_effect {
            "test effect running"
        } else {
            "ok"
        };
        Support::Supported(status.into())
    }

    fn firmware(&mut self) -> Support<String> {
        Support::Supported("1.0 (simulated)".into())
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        let percent = |v| ParamSpec::float(v, 0.0, 100.0, 1.0).persistent();
        Support::Supported(match param {
            WheelBaseParam::Rotation => {
                ParamSpec::float(self.rotation, 90.0, 2520.0, 10.0).persistent()
            }
            WheelBaseParam::FfbStrength => percent(self.ffb_strength).with_force_limit(100.0),
            WheelBaseParam::NaturalDamper => percent(self.natural_damper),
            WheelBaseParam::NaturalFriction => percent(self.natural_friction),
            WheelBaseParam::NaturalInertia => {
                return Support::not_in_hardware("The simulated base has no inertia filter.");
            }
            WheelBaseParam::SpringStrength => percent(self.spring_strength),
            WheelBaseParam::DamperStrength => percent(self.damper_strength),
        })
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let v = as_f64(value);
        match param {
            WheelBaseParam::Rotation => self.rotation = v,
            WheelBaseParam::FfbStrength => self.ffb_strength = v,
            WheelBaseParam::NaturalDamper => self.natural_damper = v,
            WheelBaseParam::NaturalFriction => self.natural_friction = v,
            WheelBaseParam::SpringStrength => self.spring_strength = v,
            WheelBaseParam::DamperStrength => self.damper_strength = v,
            WheelBaseParam::NaturalInertia => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        Support::Supported(match action {
            WheelBaseAction::TestEffect => {
                ActionSpec::confirm("The wheel will turn by itself. Hold it firmly.")
            }
            _ => ActionSpec::default(),
        })
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        Ok(match action {
            WheelBaseAction::SetCenter => {
                self.center = self.raw_angle();
                ActionOutcome::message("Center set to the current position.")
            }
            WheelBaseAction::TestEffect => {
                self.test_effect = true;
                ActionOutcome::message("Test effect started.")
            }
            WheelBaseAction::StopForce => {
                self.test_effect = false;
                ActionOutcome::message("Force stopped.")
            }
            WheelBaseAction::Reset => {
                self.reset();
                ActionOutcome::message("Settings reset to defaults.")
            }
        })
    }
}
