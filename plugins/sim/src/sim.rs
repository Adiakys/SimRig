//! State shared by the simulated components: time base, wave speed and noise.

use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;
use std::time::Instant;

use simrig_plugin_sdk::prelude::*;

pub struct SimState {
    start: Instant,
    /// Frequency of the input waves, in Hz.
    pub wave_speed: f64,
    /// Noise amplitude added to analog inputs, `0.0..=1.0`.
    pub noise: f64,
    pub advanced_mode: bool,
    pub demo_filter: i64,
    rng: u64,
}

impl Default for SimState {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            wave_speed: 0.2,
            noise: 0.0,
            advanced_mode: false,
            demo_filter: 3,
            rng: 0x2545_f491_4f6c_dd1d,
        }
    }
}

impl SimState {
    /// Seconds since start, scaled by the wave speed.
    pub fn t(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * self.wave_speed
    }

    /// Sine wave in `-1.0..=1.0`, phase-shifted by `offset` periods.
    pub fn wave(&self, offset: f64) -> f64 {
        ((self.t() + offset) * TAU).sin()
    }

    /// Wave in `0.0..=1.0`, phase-shifted by `offset` periods.
    pub fn unit_wave(&self, offset: f64) -> f64 {
        (self.wave(offset) + 1.0) / 2.0
    }

    /// Uniform noise in `-noise..=noise` (xorshift).
    pub fn jitter(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let unit = (self.rng >> 11) as f64 / (1u64 << 53) as f64;
        (unit * 2.0 - 1.0) * self.noise
    }
}

#[derive(Clone, Default)]
pub struct Shared(Rc<RefCell<SimState>>);

impl Shared {
    pub fn get(&self) -> std::cell::RefMut<'_, SimState> {
        self.0.borrow_mut()
    }
}

pub const RAW_MAX: i64 = 65535;

/// Calibration, dead zones, inversion and response curve, as a device firmware would apply them.
pub struct AxisShape {
    pub cal_min: i64,
    pub cal_max: i64,
    pub deadzone_low: Percent,
    pub deadzone_high: Percent,
    pub curve: Vec<CurvePoint>,
    pub inverted: bool,
}

impl Default for AxisShape {
    fn default() -> Self {
        Self {
            cal_min: 0,
            cal_max: RAW_MAX,
            deadzone_low: 0.0,
            deadzone_high: 0.0,
            curve: linear_curve(),
            inverted: false,
        }
    }
}

pub fn linear_curve() -> Vec<CurvePoint> {
    vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)]
}

impl AxisShape {
    pub fn apply(&self, raw: i64) -> Axis {
        let span = (self.cal_max - self.cal_min).max(1) as f64;
        let mut v = ((raw - self.cal_min) as f64 / span).clamp(0.0, 1.0);
        if self.inverted {
            v = 1.0 - v;
        }
        let low = self.deadzone_low / 100.0;
        let high = 1.0 - self.deadzone_high / 100.0;
        v = if high > low {
            ((v - low) / (high - low)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Axis {
            raw,
            value: interpolate(&self.curve, v),
        }
    }

    /// Checks a new calibration bound against the other one.
    pub fn check_calibration(&self, min: i64, max: i64) -> Result<()> {
        if min >= max {
            return Err(Error::InvalidValue(format!(
                "calibration minimum {min} must be below maximum {max}"
            )));
        }
        Ok(())
    }
}

fn interpolate(curve: &[CurvePoint], x: f64) -> f64 {
    for w in curve.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b.x {
            let t = if b.x > a.x {
                (x - a.x) / (b.x - a.x)
            } else {
                0.0
            };
            return (a.y + t * (b.y - a.y)).clamp(0.0, 1.0);
        }
    }
    curve.last().map_or(x, |p| p.y)
}

/// Raw position of a simulated analog axis.
pub fn raw_from_unit(unit: f64) -> i64 {
    (unit.clamp(0.0, 1.0) * RAW_MAX as f64).round() as i64
}

pub fn as_f64(value: &Value) -> f64 {
    value.as_f64().unwrap_or_default()
}

pub fn as_choice(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_applies_deadzone_and_curve() {
        let shape = AxisShape {
            deadzone_low: 10.0,
            curve: vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(0.5, 0.25),
                CurvePoint::new(1.0, 1.0),
            ],
            ..AxisShape::default()
        };
        assert_eq!(shape.apply(0).value, 0.0);
        assert_eq!(shape.apply(raw_from_unit(0.05)).value, 0.0);
        assert!((shape.apply(raw_from_unit(0.55)).value - 0.25).abs() < 1e-3);
        assert_eq!(shape.apply(RAW_MAX).value, 1.0);
    }
}
