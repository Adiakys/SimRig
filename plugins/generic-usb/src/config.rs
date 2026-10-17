//! The setup of one device, as its setup procedure saved it: what it is and how its raw reports
//! map to gears or to a handbrake. The driver's service reads the same file to drive the virtual
//! joystick, so the app and the games always agree.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A device as the user knows it, and which of its HID interfaces carries the inputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    /// Product name, shown in the app and in the virtual joystick's name.
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Index of the HID interface among the device's hidraw nodes, in interface order.
    #[serde(default)]
    pub interface: usize,
    pub setup: Setup,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Setup {
    /// H-pattern shifter: one signature per forward gear, then reverse.
    HShifter {
        gears: u8,
        /// `left_up` or `right_down`: where reverse sits in the gate, for the drawing.
        reverse_position: String,
        forward: Vec<Signature>,
        reverse: Signature,
    },
    Sequential {
        up: Signature,
        down: Signature,
    },
    Handbrake(HandbrakeSetup),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandbrakeSetup {
    pub input: HandbrakeInput,
    /// Raw values at rest and fully pulled (an axis may go either way).
    pub calibration_min: i64,
    pub calibration_max: i64,
    /// Percent of travel ignored at rest.
    #[serde(default)]
    pub deadzone: f64,
    /// Response curve, points in 0..1 with increasing x.
    #[serde(default = "linear")]
    pub curve: Vec<[f64; 2]>,
    /// `analog` (an axis for games) or `button` (pressed past the threshold).
    #[serde(default = "analog")]
    pub mode: String,
    /// Percent of travel where the button engages, in button mode.
    #[serde(default = "half")]
    pub button_threshold: f64,
}

fn linear() -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

fn analog() -> String {
    "analog".into()
}

fn half() -> f64 {
    50.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HandbrakeInput {
    /// An unsigned little-endian field of the report.
    Axis { index: usize, size: u8 },
    /// An on/off handbrake: the bits it sets when pulled.
    Button { pressed: Signature },
}

/// Bits of a report that identify a position: every byte must match `value` under `mask`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub bytes: Vec<ByteMatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteMatch {
    pub index: usize,
    pub mask: u8,
    pub value: u8,
}

impl Signature {
    pub fn matches(&self, report: &[u8]) -> bool {
        !self.bytes.is_empty()
            && self
                .bytes
                .iter()
                .all(|b| report.get(b.index).is_some_and(|v| v & b.mask == b.value))
    }

    /// How specific it is: the number of bits it checks.
    pub fn bits(&self) -> u32 {
        self.bytes.iter().map(|b| b.mask.count_ones()).sum()
    }

    /// Short text for tables: `byte 2 = 0x04`, or `byte 2 bit 3` when one bit decides.
    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .bytes
            .iter()
            .map(|b| {
                if b.mask.count_ones() == 1 {
                    let bit = b.mask.trailing_zeros();
                    let state = if b.value == 0 { " off" } else { "" };
                    format!("byte {} bit {bit}{state}", b.index)
                } else if b.mask == 0xff {
                    format!("byte {} = 0x{:02x}", b.index, b.value)
                } else {
                    format!("byte {} & 0x{:02x} = 0x{:02x}", b.index, b.mask, b.value)
                }
            })
            .collect();
        parts.join(", ")
    }
}

/// Stable key of a device, as the host uses it: `usb:vvvv:pppp[:serial]`.
pub fn device_key(vid: u16, pid: u16, serial: Option<&str>) -> String {
    match serial {
        Some(serial) => format!("usb:{vid:04x}:{pid:04x}:{serial}"),
        None => format!("usb:{vid:04x}:{pid:04x}"),
    }
}

/// File name of a device's setup: `vvvv-pppp[-serial]`, serial reduced to `[a-z0-9]`.
pub fn file_stem(vid: u16, pid: u16, serial: Option<&str>) -> String {
    let mut stem = format!("{vid:04x}-{pid:04x}");
    if let Some(serial) = serial {
        let clean: String = serial
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .take(32)
            .collect();
        if !clean.is_empty() {
            stem.push('-');
            stem.push_str(&clean);
        }
    }
    stem
}

impl DeviceConfig {
    pub fn file_stem(&self) -> String {
        file_stem(self.vid, self.pid, self.serial.as_deref())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("a device setup always serializes")
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let config: DeviceConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    /// Checks what serde cannot: counts and ranges.
    pub fn validate(&self) -> Result<(), String> {
        match &self.setup {
            Setup::HShifter { gears, forward, .. } => {
                if !(1..=10).contains(gears) || forward.len() != usize::from(*gears) {
                    return Err(format!(
                        "{} forward gears mapped for a {gears}-gear shifter",
                        forward.len()
                    ));
                }
            }
            Setup::Sequential { .. } => {}
            Setup::Handbrake(h) => {
                if let HandbrakeInput::Axis { size, .. } = h.input {
                    if size != 1 && size != 2 {
                        return Err(format!("axis of {size} bytes"));
                    }
                }
                if h.curve.len() < 2 || !h.curve.windows(2).all(|w| w[0][0] < w[1][0]) {
                    return Err("invalid response curve".into());
                }
            }
        }
        Ok(())
    }

    /// The setup saved in `dir` for this device, if any.
    pub fn load(dir: &Path, vid: u16, pid: u16, serial: Option<&str>) -> Option<Self> {
        let path = dir.join(format!("{}.toml", file_stem(vid, pid, serial)));
        let text = fs::read_to_string(path).ok()?;
        match Self::parse(&text) {
            Ok(config) => Some(config),
            Err(e) => {
                eprintln!("ignoring the setup of {vid:04x}:{pid:04x}: {e}");
                None
            }
        }
    }
}

/// Where the driver keeps device setups, inside its data directory.
pub fn devices_dir(driver_data: &Path) -> PathBuf {
    driver_data.join("devices")
}

/// Base64 (standard alphabet, padded): setups travel to the driver's privileged operation as
/// one argument made only of characters its pattern allows.
pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gear(index: usize, bit: u8) -> Signature {
        Signature {
            bytes: vec![ByteMatch {
                index,
                mask: 1 << bit,
                value: 1 << bit,
            }],
        }
    }

    pub fn shifter() -> DeviceConfig {
        DeviceConfig {
            name: "USB Game Controller".into(),
            vid: 0x1234,
            pid: 0x0001,
            serial: None,
            interface: 0,
            setup: Setup::HShifter {
                gears: 2,
                reverse_position: "right_down".into(),
                forward: vec![gear(2, 0), gear(2, 1)],
                reverse: gear(2, 6),
            },
        }
    }

    #[test]
    fn toml_roundtrip() {
        let config = shifter();
        let text = config.to_toml();
        assert_eq!(DeviceConfig::parse(&text).unwrap(), config);
        assert!(text.contains("kind = \"h_shifter\""), "{text}");
    }

    #[test]
    fn invalid_setups_are_refused() {
        let mut config = shifter();
        if let Setup::HShifter { gears, .. } = &mut config.setup {
            *gears = 6;
        }
        assert!(DeviceConfig::parse(&config.to_toml()).is_err());
        assert!(DeviceConfig::parse("name = 'x'\nvid = 1\npid = 2\nextra = 1").is_err());
    }

    #[test]
    fn signatures() {
        let g = gear(2, 2);
        assert!(g.matches(&[0, 0, 0b100, 0]));
        assert!(g.matches(&[0, 0, 0b101, 0]));
        assert!(!g.matches(&[0, 0, 0b001, 0]));
        assert!(!g.matches(&[0, 0]));
        assert_eq!(g.describe(), "byte 2 bit 2");
        assert!(!Signature::default().matches(&[0]));
    }

    #[test]
    fn keys_and_files() {
        assert_eq!(device_key(0x1234, 1, None), "usb:1234:0001");
        assert_eq!(file_stem(0x1234, 1, Some("AB-12/x")), "1234-0001-ab12x");
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
