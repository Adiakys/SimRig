//! Live inputs through the device's event node: axes and buttons read on request (no thread),
//! and the force feedback test.
//!
//! Tests use a fake device: a HID directory outside `/sys` with a `simrig-fake-input` file of
//! lines `abs <code> <value> <min> <max>`, `key <code> <0|1>` and `ff [gain] [autocenter]`
//! (force feedback, and the controls the driver takes). Levels set on a fake device are written
//! to `simrig-fake-ff`, as `gain <level>` and `autocenter <level>` lines.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use evdev::{
    Device, FFEffect, FFEffectCode, FFEffectData, FFEffectKind, FFEnvelope, FFReplay, FFTrigger,
    FFWaveform,
};

pub const FAKE_INPUT: &str = "simrig-fake-input";
pub const FAKE_FF: &str = "simrig-fake-ff";

/// The device-wide force feedback controls of the kernel's input interface, which most force
/// feedback drivers take: games set them, and so can SimRig where a driver has no attribute for
/// them. They cannot be read back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FfControl {
    /// The strength of every effect (`FF_GAIN`).
    Gain,
    /// The spring that pulls the wheel to the center (`FF_AUTOCENTER`).
    Autocenter,
}

impl FfControl {
    fn code(self) -> FFEffectCode {
        match self {
            FfControl::Gain => FFEffectCode::FF_GAIN,
            FfControl::Autocenter => FFEffectCode::FF_AUTOCENTER,
        }
    }

    fn key(self) -> &'static str {
        match self {
            FfControl::Gain => "gain",
            FfControl::Autocenter => "autocenter",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AxisState {
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

impl AxisState {
    /// Position in 0..1 over the axis's range.
    pub fn unit(&self) -> f64 {
        let span = f64::from(self.max) - f64::from(self.min);
        if span <= 0.0 {
            return 0.0;
        }
        ((f64::from(self.value) - f64::from(self.min)) / span).clamp(0.0, 1.0)
    }
}

pub enum Inputs {
    Device {
        device: Box<Device>,
        /// The force feedback test running, stopped when dropped.
        effect: Option<FFEffect>,
    },
    Fake(PathBuf),
    None(String),
}

impl Inputs {
    pub fn open(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            return Inputs::None("The device has no input node.".into());
        };
        if path.file_name().is_some_and(|n| n == FAKE_INPUT) {
            return Inputs::Fake(path);
        }
        match Device::open(&path) {
            Ok(device) => Inputs::Device {
                device: Box::new(device),
                effect: None,
            },
            Err(e) => Inputs::None(format!("Cannot read {}: {e}.", path.display())),
        }
    }

    /// Why inputs are unavailable, if they are.
    pub fn error(&self) -> Option<&str> {
        match self {
            Inputs::None(why) => Some(why),
            _ => None,
        }
    }

    /// Every absolute axis by evdev code.
    pub fn axes(&self) -> BTreeMap<u16, AxisState> {
        match self {
            Inputs::Device { device, .. } => {
                let (Some(supported), Ok(state)) =
                    (device.supported_absolute_axes(), device.get_abs_state())
                else {
                    return BTreeMap::new();
                };
                supported
                    .iter()
                    .filter_map(|axis| {
                        let info = state.get(usize::from(axis.0))?;
                        Some((
                            axis.0,
                            AxisState {
                                value: info.value,
                                min: info.minimum,
                                max: info.maximum,
                            },
                        ))
                    })
                    .collect()
            }
            Inputs::Fake(path) => fake(path)
                .into_iter()
                .filter_map(|line| match words(&line).as_slice() {
                    ["abs", code, value, min, max] => Some((
                        code.parse().ok()?,
                        AxisState {
                            value: value.parse().ok()?,
                            min: min.parse().ok()?,
                            max: max.parse().ok()?,
                        },
                    )),
                    _ => None,
                })
                .collect(),
            Inputs::None(_) => BTreeMap::new(),
        }
    }

    /// Every button the device has, in code order, with whether it is pressed.
    pub fn buttons(&self) -> Vec<(u16, bool)> {
        match self {
            Inputs::Device { device, .. } => {
                let Some(keys) = device.supported_keys() else {
                    return Vec::new();
                };
                let pressed = device.get_key_state().unwrap_or_default();
                keys.iter()
                    .map(|k| (k.code(), pressed.contains(k)))
                    .collect()
            }
            Inputs::Fake(path) => fake(path)
                .into_iter()
                .filter_map(|line| match words(&line).as_slice() {
                    ["key", code, state] => Some((code.parse().ok()?, *state == "1")),
                    _ => None,
                })
                .collect(),
            Inputs::None(_) => Vec::new(),
        }
    }

    /// The device takes force feedback effects (a driver handles them).
    pub fn force_feedback(&self) -> bool {
        match self {
            Inputs::Device { device, .. } => device
                .supported_ff()
                .is_some_and(|ff| ff.iter().next().is_some()),
            Inputs::Fake(path) => fake(path).iter().any(|l| words(l).first() == Some(&"ff")),
            Inputs::None(_) => false,
        }
    }

    /// The driver takes this control.
    pub fn takes(&self, control: FfControl) -> bool {
        match self {
            Inputs::Device { device, .. } => device
                .supported_ff()
                .is_some_and(|ff| ff.contains(control.code())),
            Inputs::Fake(path) => fake(path).iter().any(|l| {
                let w = words(l);
                w.first() == Some(&"ff") && w.contains(&control.key())
            }),
            Inputs::None(_) => false,
        }
    }

    /// Sets a control, `level` in 0..=65535.
    pub fn set_control(&mut self, control: FfControl, level: u16) -> io::Result<()> {
        match self {
            Inputs::Device { device, .. } => match control {
                FfControl::Gain => device.set_ff_gain(level),
                FfControl::Autocenter => device.set_ff_autocenter(level),
            },
            Inputs::Fake(path) => {
                let file = path.with_file_name(FAKE_FF);
                let mut lines: Vec<String> = fs::read_to_string(&file)
                    .unwrap_or_default()
                    .lines()
                    .filter(|l| l.split_whitespace().next() != Some(control.key()))
                    .map(str::to_string)
                    .collect();
                lines.push(format!("{} {level}", control.key()));
                fs::write(file, lines.join("\n") + "\n")
            }
            Inputs::None(why) => Err(io::Error::other(why.clone())),
        }
    }

    /// A gentle side-to-side test: a sine of a quarter of full strength for two seconds.
    pub fn test_effect(&mut self) -> io::Result<()> {
        let Inputs::Device { device, effect } = self else {
            // Nothing to move on fake or missing devices.
            return Ok(());
        };
        *effect = None;
        let data = FFEffectData {
            direction: 0x4000,
            trigger: FFTrigger::default(),
            replay: FFReplay {
                length: 2000,
                delay: 0,
            },
            kind: FFEffectKind::Periodic {
                waveform: FFWaveform::Sine,
                period: 500,
                magnitude: 0x2000,
                offset: 0,
                phase: 0,
                envelope: FFEnvelope {
                    attack_length: 0,
                    attack_level: 0,
                    fade_length: 0,
                    fade_level: 0,
                },
            },
        };
        let mut uploaded = device.upload_ff_effect(data)?;
        uploaded.play(1)?;
        *effect = Some(uploaded);
        Ok(())
    }

    /// Stops the test effect (removing it from the device).
    pub fn stop_force(&mut self) -> io::Result<()> {
        if let Inputs::Device { effect, .. } = self {
            if let Some(mut e) = effect.take() {
                e.stop()?;
            }
        }
        Ok(())
    }
}

fn fake(path: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split_whitespace().map(str::to_string).collect())
        .collect()
}

fn words(line: &[String]) -> Vec<&str> {
    line.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_positions() {
        let a = AxisState {
            value: 512,
            min: 0,
            max: 1024,
        };
        assert_eq!(a.unit(), 0.5);
        assert_eq!(AxisState::default().unit(), 0.0);
    }
}
