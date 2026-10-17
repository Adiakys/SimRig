//! From a raw report to what the device means: the engaged gear, the sequential paddles, the
//! handbrake's position. Shared by the adapter (what the app shows) and the driver's service
//! (what games get).

use crate::config::{DeviceConfig, HandbrakeInput, HandbrakeSetup, Setup};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    /// H-pattern: the engaged slot, `Some(0)` for 1st, `Some(gears)` for reverse; `None` in
    /// neutral.
    pub slot: Option<usize>,
    /// Sequential: up and down.
    pub sequential: [bool; 2],
    /// Handbrake: the raw value of its field (axis setups only).
    pub raw: Option<i64>,
    /// Handbrake: travel after calibration, dead zone and curve, `0.0..=1.0`.
    pub position: f64,
    /// Handbrake: pulled (button setups, or past the threshold in button mode).
    pub pressed: bool,
}

/// An unsigned little-endian field.
pub fn field(report: &[u8], index: usize, size: u8) -> Option<i64> {
    let bytes = report.get(index..index + usize::from(size))?;
    Some(
        bytes
            .iter()
            .rev()
            .fold(0i64, |acc, b| (acc << 8) | i64::from(*b)),
    )
}

impl DeviceConfig {
    pub fn decode(&self, report: &[u8]) -> State {
        let mut state = State::default();
        match &self.setup {
            Setup::HShifter {
                forward, reverse, ..
            } => {
                // The most specific matching slot wins (a gear may share bits with another).
                state.slot = forward
                    .iter()
                    .chain(std::iter::once(reverse))
                    .enumerate()
                    .filter(|(_, s)| s.matches(report))
                    .max_by_key(|(_, s)| s.bits())
                    .map(|(i, _)| i);
            }
            Setup::Sequential { up, down } => {
                state.sequential = [up.matches(report), down.matches(report)];
            }
            Setup::Handbrake(h) => match &h.input {
                HandbrakeInput::Axis { index, size } => {
                    state.raw = field(report, *index, *size);
                    state.position = state.raw.map(|raw| h.travel(raw)).unwrap_or(0.0);
                    state.pressed =
                        h.mode == "button" && state.position * 100.0 >= h.button_threshold;
                }
                HandbrakeInput::Button { pressed } => {
                    state.pressed = pressed.matches(report);
                    state.position = if state.pressed { 1.0 } else { 0.0 };
                }
            },
        }
        state
    }
}

impl HandbrakeSetup {
    /// Travel `0.0..=1.0` of a raw value: calibration, then dead zone, then curve.
    pub fn travel(&self, raw: i64) -> f64 {
        let (min, max) = (self.calibration_min as f64, self.calibration_max as f64);
        if (max - min).abs() < f64::EPSILON {
            return 0.0;
        }
        let linear = ((raw as f64 - min) / (max - min)).clamp(0.0, 1.0);
        let dead = (self.deadzone / 100.0).clamp(0.0, 0.95);
        let after_dead = ((linear - dead) / (1.0 - dead)).clamp(0.0, 1.0);
        curve(&self.curve, after_dead)
    }
}

/// Piecewise linear interpolation of a response curve.
pub fn curve(points: &[[f64; 2]], x: f64) -> f64 {
    let Some(first) = points.first() else {
        return x;
    };
    if x <= first[0] {
        return first[1];
    }
    for w in points.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        if x <= x1 {
            let t = (x - x0) / (x1 - x0).max(f64::EPSILON);
            return y0 + t * (y1 - y0);
        }
    }
    points.last().map(|p| p[1]).unwrap_or(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ByteMatch, Signature};

    fn bit(index: usize, bit: u8) -> Signature {
        Signature {
            bytes: vec![ByteMatch {
                index,
                mask: 1 << bit,
                value: 1 << bit,
            }],
        }
    }

    fn config(setup: Setup) -> DeviceConfig {
        DeviceConfig {
            name: "x".into(),
            vid: 1,
            pid: 2,
            serial: None,
            interface: 0,
            setup,
        }
    }

    #[test]
    fn gears_from_bits() {
        let c = config(Setup::HShifter {
            gears: 2,
            reverse_position: "right_down".into(),
            forward: vec![bit(1, 0), bit(1, 1)],
            reverse: bit(1, 7),
        });
        assert_eq!(c.decode(&[0, 0b0000_0000]).slot, None);
        assert_eq!(c.decode(&[0, 0b0000_0010]).slot, Some(1));
        assert_eq!(c.decode(&[0, 0b1000_0000]).slot, Some(2));
    }

    #[test]
    fn handbrake_axis_inverted_with_deadzone() {
        let h = HandbrakeSetup {
            input: HandbrakeInput::Axis { index: 1, size: 2 },
            calibration_min: 1000,
            calibration_max: 0,
            deadzone: 10.0,
            curve: vec![[0.0, 0.0], [1.0, 1.0]],
            mode: "analog".into(),
            button_threshold: 50.0,
        };
        let c = config(Setup::Handbrake(h));
        // 1000 at rest (little endian 0x03e8), 0 pulled.
        assert_eq!(c.decode(&[9, 0xe8, 0x03]).position, 0.0);
        assert_eq!(c.decode(&[9, 0, 0]).position, 1.0);
        let mid = c.decode(&[9, 0xf4, 0x01]); // 500 → 0.5 → (0.5-0.1)/0.9
        assert!((mid.position - 0.4 / 0.9).abs() < 1e-9);
        assert_eq!(mid.raw, Some(500));
        assert!(!mid.pressed);
    }

    #[test]
    fn curve_interpolates() {
        let c = [[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]];
        assert_eq!(curve(&c, 0.25), 0.125);
        assert_eq!(curve(&c, 0.75), 0.625);
        assert_eq!(curve(&c, 2.0), 1.0);
    }
}
