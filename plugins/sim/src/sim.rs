//! State shared by the simulated components: time base, wave speed and noise.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::f64::consts::TAU;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use simrig_plugin_sdk::prelude::*;

pub const NO_RIM: &str = "none";

/// Id, name and catalog model of a simulated component.
#[derive(Clone, Debug)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub model: String,
}

impl Identity {
    pub fn new(id: impl Into<String>, name: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            model: model.into(),
        }
    }
}

/// How a generic (unconfigured) device was set up by its setup procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    Shifter {
        gears: i64,
    },
    SequentialShifter,
    Handbrake,
    /// 1: throttle; 2: throttle and brake; 3: with a clutch.
    Pedals {
        count: i64,
    },
}

impl Setup {
    pub fn to_text(&self) -> String {
        match self {
            Setup::Shifter { gears } => format!("shifter:{gears}"),
            Setup::SequentialShifter => "sequential".into(),
            Setup::Handbrake => "handbrake".into(),
            Setup::Pedals { count } => format!("pedals:{count}"),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.split_once(':') {
            Some(("shifter", gears)) => Some(Setup::Shifter {
                gears: gears.parse().ok()?,
            }),
            Some(("pedals", count)) => Some(Setup::Pedals {
                count: count.parse::<i64>().ok()?.clamp(1, 3),
            }),
            None if text == "handbrake" => Some(Setup::Handbrake),
            None if text == "sequential" => Some(Setup::SequentialShifter),
            _ => None,
        }
    }

    /// The pedals of a pedal set of `count` pedals, as they are found on real sets.
    pub fn pedal_roles(count: i64) -> Vec<PedalRole> {
        match count {
            1 => vec![PedalRole::Throttle],
            2 => vec![PedalRole::Throttle, PedalRole::Brake],
            _ => PedalRole::ALL.to_vec(),
        }
    }
}

/// The simulated driver: it remembers saved settings in its data directory (given by the host
/// in `SIMRIG_DRIVER_DATA`), written by its `save-setting` privileged operation.
pub struct SimDriver {
    data: PathBuf,
}

impl SimDriver {
    fn from_env() -> Option<Self> {
        std::env::var_os("SIMRIG_DRIVER_DATA").map(|d| SimDriver {
            data: PathBuf::from(d),
        })
    }

    pub fn installed(&self) -> bool {
        self.data.join("installed").is_file()
    }

    /// `key=value` lines of the settings file.
    fn saved(&self) -> BTreeMap<String, String> {
        let text = fs::read_to_string(self.data.join("settings")).unwrap_or_default();
        text.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

pub struct SimState {
    start: Instant,
    /// Frequency of the input waves, in Hz.
    pub wave_speed: f64,
    /// Noise amplitude added to analog inputs, `0.0..=1.0`.
    pub noise: f64,
    pub advanced_mode: bool,
    pub demo_filter: i64,
    /// Rim mounted on the base: `formula`, `gt` or `none`.
    pub rim: String,
    pub handbrake_connected: bool,
    pub driver: Option<SimDriver>,
    /// Settings saved by the driver, `<component id>.<item>` → text value.
    pub saved: BTreeMap<String, String>,
    /// Generic devices that were set up, by device id.
    pub setups: BTreeMap<String, Setup>,
    /// During a guided calibration the simulated pedal follows the instructions: `(pedal
    /// index, position 0..1)`.
    pub pedal_override: Option<(usize, f64)>,
    rng: u64,
}

impl Default for SimState {
    fn default() -> Self {
        let driver = SimDriver::from_env();
        let saved = driver
            .as_ref()
            .filter(|d| d.installed())
            .map(SimDriver::saved)
            .unwrap_or_default();
        Self {
            start: Instant::now(),
            wave_speed: 0.2,
            noise: 0.0,
            advanced_mode: false,
            demo_filter: 3,
            rim: "formula".into(),
            handbrake_connected: true,
            setups: saved
                .iter()
                .filter_map(|(k, v)| {
                    Some((k.strip_suffix(".setup")?.to_string(), Setup::parse(v)?))
                })
                .collect(),
            driver,
            saved,
            pedal_override: None,
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

    /// Saves a value through the simulated driver's privileged operation, like a real driver
    /// writing a system file.
    pub fn save(
        &self,
        component: &str,
        item: &str,
        value: &Value,
        host: &mut HostLink<'_>,
    ) -> Result<()> {
        let installed = self.get().driver.as_ref().is_some_and(SimDriver::installed);
        if !installed {
            return Err(Error::Unsupported(Unsupported::new(Reason::NeedsDriver)));
        }
        let key = format!("{component}.{item}");
        let text = value.to_text();
        host.privileged("save-setting", &[("key", &key), ("value", &text)])?;
        self.get().saved.insert(key, text);
        Ok(())
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
