use simrig_plugin_sdk::model::Reason;
use simrig_plugin_sdk::prelude::*;

use crate::sim::{Shared, as_choice, as_f64};

const FLAGS: [(&str, &str); 6] = [
    ("off", "Off"),
    ("green", "Green"),
    ("yellow", "Yellow"),
    ("blue", "Blue"),
    ("red", "Red"),
    ("white", "White"),
];

/// Simulated rims. The formula rim has a display, analog paddles and RPM LEDs but no flag LEDs
/// and no vibration; the GT rim has flag LEDs and vibration but no display and no analog
/// paddles. Swapping them exercises `components/changed`.
pub struct SimWheel {
    shared: Shared,
    gt: bool,
    paddle_mode: String,
    led_brightness: Percent,
    vibration: Percent,
    rpm_leds: Percent,
    flag: String,
    display: String,
}

impl SimWheel {
    pub fn new(shared: Shared, rim: &str) -> Self {
        Self {
            shared,
            gt: rim == "gt",
            paddle_mode: "clutch".into(),
            led_brightness: 80.0,
            vibration: 50.0,
            rpm_leds: 0.0,
            flag: "off".into(),
            display: String::new(),
        }
    }

    fn buttons_count(&self) -> usize {
        if self.gt { 10 } else { 12 }
    }
}

impl Component for SimWheel {
    fn id(&self) -> String {
        format!("{}/wheel", crate::RIG)
    }

    fn name(&self) -> String {
        if self.gt {
            "Simulated GT rim"
        } else {
            "Simulated formula rim"
        }
        .into()
    }

    fn model(&self) -> Option<String> {
        Some(
            if self.gt {
                "sim/gt-rim"
            } else {
                "sim/formula-rim"
            }
            .into(),
        )
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.shared.save(&self.id(), item, value, host)
    }
}

impl SteeringWheel for SimWheel {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let count = self.buttons_count();
        let pressed = (self.shared.get().t() * 8.0) as usize % count;
        Support::Supported((0..count).map(|i| i == pressed).collect())
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
        if self.gt {
            return Support::not_in_hardware("The GT rim has no analog paddles.");
        }
        let s = self.shared.get();
        Support::Supported(vec![s.unit_wave(0.0), s.unit_wave(0.5)])
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::Supported(if self.gt { "sim-gt" } else { "sim-formula" }.into())
    }

    fn param(&mut self, param: SteeringWheelParam) -> Support<ParamSpec> {
        match param {
            SteeringWheelParam::AnalogPaddleMode if self.gt => {
                Support::not_in_hardware("The GT rim has no analog paddles.")
            }
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
            SteeringWheelParam::Vibration if self.gt => {
                Support::Supported(ParamSpec::float(self.vibration, 0.0, 100.0, 1.0).persistent())
            }
            SteeringWheelParam::Vibration => {
                Support::not_in_hardware("The formula rim has no vibration motors.")
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
            SteeringWheelParam::Vibration => self.vibration = as_f64(value),
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
            SteeringWheelOutput::FlagLeds if self.gt => Support::Supported(ParamType::Choice {
                options: FLAGS
                    .iter()
                    .map(|(id, label)| ChoiceOption::new(id, label))
                    .collect(),
            }),
            SteeringWheelOutput::FlagLeds => {
                Support::Unsupported(Unsupported::new(Reason::NeedsComponent {
                    what: "a rim with flag LEDs".into(),
                }))
            }
            SteeringWheelOutput::Display if self.gt => {
                Support::not_in_hardware("The GT rim has no display.")
            }
            SteeringWheelOutput::Display => Support::Supported(ParamType::Text { max_len: 16 }),
        }
    }

    fn write_output(&mut self, output: SteeringWheelOutput, value: &Value) -> Result<()> {
        match output {
            SteeringWheelOutput::RpmLeds => self.rpm_leds = as_f64(value),
            SteeringWheelOutput::FlagLeds => self.flag = as_choice(value),
            SteeringWheelOutput::Display => {
                self.display = value.as_str().unwrap_or_default().into()
            }
        }
        eprintln!(
            "wheel outputs: rpm {:.0}% flag {} display {:?}",
            self.rpm_leds, self.flag, self.display
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
