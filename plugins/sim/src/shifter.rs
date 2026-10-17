use simrig_plugin_sdk::prelude::*;

use crate::sim::{Shared, as_choice, as_f64};

const H_PATTERN: &str = "h_pattern";
const SEQUENTIAL: &str = "sequential";

/// Simulated analog shifter switchable between H-pattern and sequential. Items that only make
/// sense in one mode are reported as `DependsOn` the mode in the other.
pub struct SimShifter {
    shared: Shared,
    mode: String,
    gears: i64,
    reverse_position: String,
    engage_threshold: Percent,
}

impl SimShifter {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            mode: H_PATTERN.into(),
            gears: 6,
            reverse_position: "left_up".into(),
            engage_threshold: 50.0,
        }
    }

    fn is_h(&self) -> bool {
        self.mode == H_PATTERN
    }

    fn only_in<T>(&self, mode: &str, value: impl FnOnce() -> T) -> Support<T> {
        if self.mode == mode {
            Support::Supported(value())
        } else {
            Support::depends_on("mode", Value::Choice(mode.into()))
        }
    }

    /// Cycles N, 1, N, 2, … N, R.
    fn current_gear(&self) -> Gear {
        let steps = self.gears * 2 + 2;
        let step = (self.shared.get().t() * 4.0) as i64 % steps;
        match step {
            s if s % 2 == 0 => Gear::Neutral,
            s if s == steps - 1 => Gear::Reverse,
            s => Gear::Forward(((s + 1) / 2) as u8),
        }
    }

    /// Lever position of a gear: columns left to right, odd gears up.
    fn lever(&self, gear: Gear) -> [f64; 2] {
        let columns = ((self.gears + 1) / 2 + 1) as f64;
        let x = |column: f64| -1.0 + 2.0 * (column + 0.5) / columns;
        match gear {
            Gear::Neutral => [0.0, 0.0],
            Gear::Forward(n) => {
                let n = f64::from(n) - 1.0;
                [
                    x((n / 2.0).floor()),
                    if n as i64 % 2 == 0 { 1.0 } else { -1.0 },
                ]
            }
            Gear::Reverse if self.reverse_position == "left_up" => [-1.0, 1.0],
            Gear::Reverse => [x(columns - 1.0), -1.0],
        }
    }

    fn sequential_pulse(&self) -> [bool; 2] {
        let phase = self.shared.get().t().fract();
        [phase < 0.1, (0.5..0.6).contains(&phase)]
    }
}

impl Component for SimShifter {
    fn id(&self) -> String {
        format!("{}/shifter", crate::DEVICE_ID)
    }

    fn name(&self) -> String {
        "Simulated H/sequential shifter".into()
    }

    fn model(&self) -> Option<String> {
        Some("sim/shifter".into())
    }
}

impl Shifter for SimShifter {
    fn gear(&mut self) -> Support<Gear> {
        self.only_in(H_PATTERN, || self.current_gear())
    }

    fn position_xy(&mut self) -> Support<[f64; 2]> {
        Support::Supported(if self.is_h() {
            self.lever(self.current_gear())
        } else {
            match self.sequential_pulse() {
                [true, _] => [0.0, 0.8],
                [_, true] => [0.0, -0.8],
                _ => [0.0, 0.0],
            }
        })
    }

    fn sequential(&mut self) -> Support<[bool; 2]> {
        self.only_in(SEQUENTIAL, || self.sequential_pulse())
    }

    fn param(&mut self, param: ShifterParam) -> Support<ParamSpec> {
        match param {
            ShifterParam::Gears => self.only_in(H_PATTERN, || {
                ParamSpec::int(self.gears, 4, 8, 1).persistent()
            }),
            ShifterParam::ReversePosition => self.only_in(H_PATTERN, || {
                ParamSpec::choice(
                    &self.reverse_position,
                    &[("left_up", "Left, up"), ("right_down", "Right, down")],
                )
                .persistent()
            }),
            ShifterParam::Mode => Support::Supported(
                ParamSpec::choice(
                    &self.mode,
                    &[(H_PATTERN, "H-pattern"), (SEQUENTIAL, "Sequential")],
                )
                .persistent(),
            ),
            ShifterParam::EngageThreshold => Support::Supported(
                ParamSpec::float(self.engage_threshold, 10.0, 90.0, 1.0).persistent(),
            ),
        }
    }

    fn set_param(&mut self, param: ShifterParam, value: &Value) -> Result<()> {
        match (param, value) {
            (ShifterParam::Gears, Value::Int(v)) => self.gears = *v,
            (ShifterParam::ReversePosition, v) => self.reverse_position = as_choice(v),
            (ShifterParam::Mode, v) => self.mode = as_choice(v),
            (ShifterParam::EngageThreshold, v) => self.engage_threshold = as_f64(v),
            _ => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn action(&mut self, _action: ShifterAction) -> Support<ActionSpec> {
        self.only_in(H_PATTERN, ActionSpec::default)
    }

    fn run_action(&mut self, _action: ShifterAction, _args: &Value) -> Result<ActionOutcome> {
        Ok(ActionOutcome::message(format!(
            "Learned {} gears and reverse.",
            self.gears
        )))
    }
}
