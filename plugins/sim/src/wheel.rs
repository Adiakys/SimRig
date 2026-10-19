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

    fn appearance(&self) -> simrig_plugin_sdk::model::Appearance {
        use simrig_plugin_sdk::model::{Appearance, RimShape};
        Appearance::rim(if self.gt {
            RimShape::Gt
        } else {
            RimShape::Formula
        })
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.shared.save(&self.id(), item, value, host)
    }

    /// Like a real rim's: buttons named where a plugin would know them, encoders that step
    /// with a pair of buttons, a hat switch, and on the formula rim two analog paddles.
    fn controls(&mut self) -> Vec<Control> {
        const NAMES: [&str; 8] = [
            "Shift down",
            "Shift up",
            "Cross",
            "Square",
            "Circle",
            "Triangle",
            "Pit",
            "Radio",
        ];
        let count = self.buttons_count();
        let mut controls: Vec<Control> = (0..count)
            .map(|i| {
                let c = Control::new(ControlKind::Button, i as u32 + 1, "buttons", i)
                    .code(format!("BTN_TRIGGER_HAPPY{}", i + 1));
                match NAMES.get(i) {
                    Some(name) => c.label(*name),
                    None => c,
                }
            })
            .collect();
        controls.push(Control::new(ControlKind::Hat, 1, "hats", 0).code("ABS_HAT0X/ABS_HAT0Y"));
        for i in 0..2 {
            let mut encoder = Control::new(ControlKind::Encoder, i + 1, "encoders", i as usize)
                .label(format!("Encoder {}", i + 1));
            encoder.buttons = vec![count as u32 - 3 + 2 * i, count as u32 - 2 + 2 * i];
            controls.push(encoder);
        }
        if !self.gt {
            for (i, name) in ["Left clutch paddle", "Right clutch paddle"]
                .iter()
                .enumerate()
            {
                controls.push(
                    Control::new(ControlKind::Axis, i as u32 + 4, "analog_paddles", i)
                        .code(if i == 0 { "ABS_RX" } else { "ABS_RY" })
                        .label(*name)
                        .bits(12),
                );
            }
        }
        controls
    }
}

impl SteeringWheel for SimWheel {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        // One button at a time, briefly, every second and a half.
        let count = self.buttons_count();
        let beat = self.shared.get().t() / 1.5;
        let pressed = (beat.fract() < 0.15).then_some(beat as usize * 7 % count);
        Support::Supported((0..count).map(|i| Some(i) == pressed).collect())
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

    fn hats(&mut self) -> Support<Vec<i64>> {
        // A push every few seconds, round the eight directions.
        let beat = self.shared.get().t() / 3.0;
        Support::Supported(vec![if beat.fract() < 0.1 {
            beat as i64 % 8
        } else {
            -1
        }])
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
