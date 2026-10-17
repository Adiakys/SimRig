use simrig_plugin_sdk::model::Reason;
use simrig_plugin_sdk::prelude::*;

use crate::sim::{Shared, as_choice, as_f64};

const BUTTONS: usize = 12;

/// Simulated formula rim: buttons, encoders, paddles, RPM LEDs and a display. It has no
/// vibration motors and no flag LEDs.
pub struct SimWheel {
    shared: Shared,
    paddle_mode: String,
    led_brightness: Percent,
    rpm_leds: Percent,
    display: String,
}

impl SimWheel {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            paddle_mode: "clutch".into(),
            led_brightness: 80.0,
            rpm_leds: 0.0,
            display: String::new(),
        }
    }
}

impl Component for SimWheel {
    fn id(&self) -> String {
        format!("{}/wheel", crate::DEVICE_ID)
    }

    fn name(&self) -> String {
        "Simulated formula rim".into()
    }

    fn model(&self) -> Option<String> {
        Some("sim/formula-rim".into())
    }
}

impl SteeringWheel for SimWheel {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let pressed = (self.shared.get().t() * 8.0) as usize % BUTTONS;
        Support::Supported((0..BUTTONS).map(|i| i == pressed).collect())
    }

    fn encoders(&mut self) -> Support<Vec<i64>> {
        let t = self.shared.get().t();
        Support::Supported(vec![(t * 4.0) as i64, -(t * 2.0) as i64])
    }

    fn shift_paddles(&mut self) -> Support<[bool; 2]> {
        let phase = self.shared.get().t().fract();
        Support::Supported([phase < 0.1, (0.5..0.6).contains(&phase)])
    }

    fn analog_paddles(&mut self) -> Support<Vec<f64>> {
        let s = self.shared.get();
        Support::Supported(vec![s.unit_wave(0.0), s.unit_wave(0.5)])
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::Supported("sim-formula".into())
    }

    fn param(&mut self, param: SteeringWheelParam) -> Support<ParamSpec> {
        match param {
            SteeringWheelParam::AnalogPaddleMode => Support::Supported(
                ParamSpec::choice(
                    &self.paddle_mode,
                    &[
                        ("clutch", "Dual clutch"),
                        ("brake_throttle", "Brake and throttle"),
                        ("buttons", "Buttons"),
                    ],
                )
                .persistent(),
            ),
            SteeringWheelParam::Vibration => {
                Support::not_in_hardware("This rim has no vibration motors.")
            }
            SteeringWheelParam::LedBrightness => Support::Supported(
                ParamSpec::float(self.led_brightness, 0.0, 100.0, 1.0).persistent(),
            ),
        }
    }

    fn set_param(&mut self, param: SteeringWheelParam, value: &Value) -> Result<()> {
        match param {
            SteeringWheelParam::AnalogPaddleMode => self.paddle_mode = as_choice(value),
            SteeringWheelParam::LedBrightness => self.led_brightness = as_f64(value),
            SteeringWheelParam::Vibration => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn output(&mut self, output: SteeringWheelOutput) -> Support<ParamType> {
        match output {
            SteeringWheelOutput::RpmLeds => Support::Supported(ParamType::Float {
                min: 0.0,
                max: 100.0,
                step: 1.0,
            }),
            SteeringWheelOutput::FlagLeds => {
                Support::Unsupported(Unsupported::new(Reason::NeedsComponent {
                    what: "a rim with flag LEDs".into(),
                }))
            }
            SteeringWheelOutput::Display => Support::Supported(ParamType::Text { max_len: 16 }),
        }
    }

    fn write_output(&mut self, output: SteeringWheelOutput, value: &Value) -> Result<()> {
        match output {
            SteeringWheelOutput::RpmLeds => self.rpm_leds = as_f64(value),
            SteeringWheelOutput::Display => {
                self.display = value.as_str().unwrap_or_default().into()
            }
            SteeringWheelOutput::FlagLeds => return Err(Error::unsupported()),
        }
        eprintln!(
            "wheel outputs: rpm {:.0}% display {:?}",
            self.rpm_leds, self.display
        );
        Ok(())
    }

    fn action(&mut self, action: SteeringWheelAction) -> Support<ActionSpec> {
        match action {
            SteeringWheelAction::LedTest => Support::Supported(ActionSpec::default()),
            SteeringWheelAction::CalibratePaddles => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: SteeringWheelAction, _args: &Value) -> Result<ActionOutcome> {
        match action {
            SteeringWheelAction::LedTest => {
                Ok(ActionOutcome::message("All LEDs lit for one second."))
            }
            SteeringWheelAction::CalibratePaddles => Err(Error::unsupported()),
        }
    }
}
