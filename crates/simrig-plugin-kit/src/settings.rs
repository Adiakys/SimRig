//! Wheel base settings that are sysfs attributes of the HID device, as standard or custom
//! parameters.

use simrig_plugin_sdk::prelude::*;

use crate::device::HidDevice;
use crate::input::FfControl;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Standard(WheelBaseParam),
    Custom { group: &'static str, advanced: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    /// The sysfs attribute of the HID device.
    pub attr: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f64,
    /// `None`: the wheel's widest rotation.
    pub max: Option<f64>,
    pub step: f64,
    /// The attribute's value for one unit shown.
    pub scale: f64,
    /// An on/off switch (0 or 1).
    pub switch: bool,
    pub place: Place,
    pub help: &'static str,
    /// Why the attribute may be missing.
    pub missing: &'static str,
    /// What stands in for the attribute when the driver has none: a force feedback control of
    /// the input interface (a percentage), if the driver takes it.
    pub ff: Option<FfControl>,
    /// A choice: the labels of the attribute's values 0, 1, 2…
    pub choices: &'static [&'static str],
}

impl Setting {
    /// A percentage, the attribute holding it as it is.
    pub const fn percent(attr: &'static str, label: &'static str, place: Place) -> Self {
        Self {
            attr,
            label,
            unit: "%",
            min: 0.0,
            max: Some(100.0),
            step: 1.0,
            scale: 1.0,
            switch: false,
            place,
            help: "",
            missing: "Not exposed by the installed driver.",
            ff: None,
            choices: &[],
        }
    }

    /// A choice between `choices`, the attribute holding the index.
    pub const fn choice(
        attr: &'static str,
        label: &'static str,
        choices: &'static [&'static str],
        place: Place,
    ) -> Self {
        Self {
            unit: "",
            max: Some((choices.len() - 1) as f64),
            choices,
            ..Self::percent(attr, label, place)
        }
    }

    /// The rotation in degrees, up to the wheel's widest.
    pub const fn rotation(attr: &'static str, min: f64) -> Self {
        Self {
            unit: "°",
            min,
            max: None,
            step: 10.0,
            ..Self::percent(attr, "Rotation", Place::Standard(WheelBaseParam::Rotation))
        }
    }

    /// The id of a custom parameter: the attribute, without a `wheel_` prefix.
    pub fn custom_id(&self) -> String {
        self.attr.trim_start_matches("wheel_").to_string()
    }

    pub fn max(&self, max_range: f64) -> f64 {
        self.max.unwrap_or(max_range)
    }

    /// The parameter as the attribute says now. Persistent: drivers keep these until the
    /// device is unplugged, and plugins save them for the next plug-in. Without the attribute,
    /// the force feedback control standing in for it, until unplugged.
    pub fn spec(&self, hid: &HidDevice, max_range: f64) -> Support<ParamSpec> {
        if let Some(u) = hid.unbound() {
            return Support::Unsupported(u);
        }
        let Some(raw) = hid.read_int(self.attr) else {
            if let Some(spec) = self.ff.and_then(|c| hid.ff_spec(c)) {
                return Support::Supported(spec);
            }
            return Support::Unsupported(Unsupported::not_in_driver().with_detail(self.missing));
        };
        let max = self.max(max_range);
        Support::Supported(
            if self.switch {
                ParamSpec::bool(raw != 0)
            } else if !self.choices.is_empty() {
                let index = (raw.max(0) as usize).min(self.choices.len() - 1);
                ParamSpec::choice(&index.to_string(), &self.options())
            } else {
                let value = (raw as f64 / self.scale).round().clamp(self.min, max);
                ParamSpec::float(value, self.min, max, self.step)
            }
            .persistent(),
        )
    }

    /// The options of a choice: ids `0`, `1`… and their labels.
    fn options(&self) -> Vec<(&'static str, &'static str)> {
        const IDS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
        IDS.iter()
            .copied()
            .zip(self.choices.iter().copied())
            .collect()
    }

    /// The attribute's value for a parameter value.
    pub fn raw(&self, value: &Value) -> Result<i64> {
        match value {
            Value::Bool(on) => Ok(i64::from(*on)),
            Value::Choice(id) => id
                .parse()
                .map_err(|_| Error::InvalidValue(format!("no choice '{id}'"))),
            v => {
                let v = v
                    .as_f64()
                    .ok_or_else(|| Error::InvalidValue("expected a number".into()))?;
                Ok((v * self.scale).round().max(0.0) as i64)
            }
        }
    }

    pub fn write(&self, hid: &mut HidDevice, value: &Value) -> Result<()> {
        if hid.attr(self.attr).is_none()
            && let Some(control) = self.ff
            && hid.ff_spec(control).is_some()
        {
            return hid.set_ff(control, value);
        }
        hid.write(self.attr, &self.raw(value)?.to_string())
    }

    /// Whether a value set now can be saved for the next plug-in: not when a force feedback
    /// control stands in for the attribute.
    pub fn saved_by_attr(&self, hid: &HidDevice) -> bool {
        hid.attr(self.attr).is_some()
    }

    /// As a custom parameter (for `Place::Custom` settings).
    pub fn custom_param(&self, hid: &HidDevice, max_range: f64, order: i32) -> Option<CustomParam> {
        let Place::Custom { group, advanced } = self.place else {
            return None;
        };
        let support = self.spec(hid, max_range);
        let (ty, value) = match &support {
            Support::Supported(s) => (s.ty.clone(), s.value.clone()),
            Support::Unsupported(_) if self.switch => (ParamType::Bool, Value::Bool(false)),
            Support::Unsupported(_) if !self.choices.is_empty() => {
                let spec = ParamSpec::choice("0", &self.options());
                (spec.ty, spec.value)
            }
            Support::Unsupported(_) => (
                ParamType::Float {
                    min: self.min,
                    max: self.max(max_range),
                    step: self.step,
                },
                Value::Float(self.min),
            ),
        };
        let mut p = CustomParam::new(&self.custom_id(), self.label, ty, value)
            .group(group)
            .order(order);
        match &support {
            Support::Supported(s) => {
                p.persistence = s.persistence;
                p = p.caveat(s.caveat.clone());
            }
            Support::Unsupported(_) => p = p.persistent(),
        }
        if !self.unit.is_empty() {
            p = p.unit(self.unit);
        }
        if !self.help.is_empty() {
            p = p.help(self.help);
        }
        if advanced {
            p = p.advanced();
        }
        if let Support::Unsupported(u) = support {
            p = p.availability(Support::Unsupported(u));
        }
        Some(p)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::device::Binding;
    use crate::input::{FAKE_FF, FAKE_INPUT};
    use simrig_plugin_sdk::model::ControlKind;

    const BINDING: Binding = Binding {
        vendor: "046D",
        drivers: &["logitech", "iforce"],
        rich: &["input"],
        unbound: "No driver.",
    };

    const GAIN: Setting = Setting {
        ff: Some(FfControl::Gain),
        ..Setting::percent(
            "gain",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        )
    };

    const COMBINE: Setting = Setting::choice(
        "combine_pedals",
        "Combined pedals",
        &["Off", "Throttle and brake", "Throttle and clutch"],
        Place::Custom {
            group: "Pedals",
            advanced: false,
        },
    );

    /// A USB device with one device bound to `driver` at `dir` below it.
    fn plug(name: &str, dir: &str, driver: &str, input: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("simrig-kit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let usb = root.join("usb/1-2");
        let hid = usb.join(dir);
        fs::create_dir_all(&hid).unwrap();
        fs::create_dir_all(root.join("drivers").join(driver)).unwrap();
        std::os::unix::fs::symlink(root.join("drivers").join(driver), hid.join("driver")).unwrap();
        fs::write(hid.join(FAKE_INPUT), input).unwrap();
        (root, hid)
    }

    fn device(root: &Path) -> HidDevice {
        HidDevice::new(
            "d",
            root.join("usb/1-2"),
            BINDING,
            root.join("c.toml"),
            "k",
            Some(0),
        )
    }

    #[test]
    fn the_attribute_comes_first() {
        let (root, hid) = plug(
            "attr",
            "1-2:1.0/0003:046D:C24F.0001",
            "logitech",
            "ff gain\n",
        );
        fs::write(hid.join("gain"), "40\n").unwrap();
        let mut d = device(&root);
        let spec = GAIN.spec(&d, 900.0).supported().unwrap();
        assert_eq!(spec.value, Value::Float(40.0));
        assert!(spec.caveat.is_none());
        GAIN.write(&mut d, &Value::Float(10.0)).unwrap();
        assert_eq!(fs::read_to_string(hid.join("gain")).unwrap(), "10\n");
        assert!(!hid.join(FAKE_FF).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn force_feedback_control_stands_in_for_a_missing_attribute() {
        let (root, hid) = plug("ff", "1-2:1.0/0003:046D:C24F.0001", "logitech", "ff gain\n");
        let mut d = device(&root);
        let spec = GAIN.spec(&d, 900.0).supported().unwrap();
        assert_eq!(spec.value, Value::Float(100.0));
        assert_eq!(spec.persistence, Persistence::Volatile);
        assert!(spec.caveat.is_some());
        assert!(!GAIN.saved_by_attr(&d));

        GAIN.write(&mut d, &Value::Float(50.0)).unwrap();
        assert_eq!(
            fs::read_to_string(hid.join(FAKE_FF)).unwrap(),
            "gain 32768\n"
        );
        // The last value set is what is shown.
        assert_eq!(
            GAIN.spec(&d, 900.0).supported().unwrap().value,
            Value::Float(50.0)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn without_the_control_it_is_unsupported() {
        let (root, _) = plug("noff", "1-2:1.0/0003:046D:C24F.0001", "logitech", "ff\n");
        let mut d = device(&root);
        let reason = match GAIN.spec(&d, 900.0) {
            Support::Unsupported(u) => u.explain(),
            Support::Supported(_) => panic!("supported"),
        };
        assert!(reason.contains("Not exposed"));
        assert!(GAIN.write(&mut d, &Value::Float(50.0)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn choices() {
        let (root, hid) = plug("choice", "1-2:1.0/0003:046D:C24F.0001", "logitech", "");
        fs::write(hid.join("combine_pedals"), "2\n").unwrap();
        let mut d = device(&root);
        let p = COMBINE.custom_param(&d, 900.0, 0).unwrap();
        assert_eq!(p.value, Value::Choice("2".into()));
        match &p.ty {
            ParamType::Choice { options } => assert_eq!(options[1].label, "Throttle and brake"),
            ty => panic!("{ty:?}"),
        }
        COMBINE.write(&mut d, &Value::Choice("1".into())).unwrap();
        assert_eq!(
            fs::read_to_string(hid.join("combine_pedals")).unwrap(),
            "1\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn controls_as_games_number_them() {
        let (root, _) = plug(
            "controls",
            "1-2:1.0/0003:046D:C24F.0001",
            "logitech",
            "abs 0 0 0 65535\nabs 3 128 0 255\nabs 16 1 -1 1\nabs 17 0 -1 1\n\
             key 256 1\nkey 288 0\nkey 289 0\n",
        );
        let d = device(&root);
        let controls = d.controls(&[(3, "Left paddle")], &[(289, "Shift up")]);
        let summary: Vec<(ControlKind, u32, Option<&str>, &str, usize)> = controls
            .iter()
            .map(|c| {
                (
                    c.kind,
                    c.number,
                    c.code.as_deref(),
                    c.item.as_str(),
                    c.index,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (ControlKind::Button, 1, Some("BTN_TRIGGER"), "buttons", 0),
                (ControlKind::Button, 2, Some("BTN_THUMB"), "buttons", 1),
                (ControlKind::Button, 3, Some("BTN_0"), "buttons", 2),
                (ControlKind::Hat, 1, Some("ABS_HAT0X/ABS_HAT0Y"), "hats", 0),
                // ABS_X is axis 1, ABS_RX axis 2 (hats are not counted).
                (ControlKind::Axis, 2, Some("ABS_RX"), "analog_paddles", 0),
            ]
        );
        assert_eq!(controls[1].label.as_deref(), Some("Shift up"));
        assert_eq!(controls[4].bits, Some(8));
        // Buttons in the same order; the hat to the right.
        assert_eq!(d.buttons().supported(), Some(vec![false, false, true]));
        assert_eq!(d.hats().supported(), Some(vec![2]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_usb_driver_of_its_own() {
        let (root, _) = plug("iforce", "1-2:1.0", "iforce", "ff gain autocenter\n");
        let d = device(&root);
        assert!(d.unbound().is_none());
        assert!(d.ff_spec(FfControl::Autocenter).is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
