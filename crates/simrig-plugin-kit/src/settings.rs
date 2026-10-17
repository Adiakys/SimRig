//! Wheel base settings that are sysfs attributes of the HID device, as standard or custom
//! parameters.

use simrig_plugin_sdk::prelude::*;

use crate::device::HidDevice;

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
    /// device is unplugged, and plugins save them for the next plug-in.
    pub fn spec(&self, hid: &HidDevice, max_range: f64) -> Support<ParamSpec> {
        if let Some(u) = hid.unbound() {
            return Support::Unsupported(u);
        }
        let Some(raw) = hid.read_int(self.attr) else {
            return Support::Unsupported(Unsupported::not_in_driver().with_detail(self.missing));
        };
        let max = self.max(max_range);
        Support::Supported(
            if self.switch {
                ParamSpec::bool(raw != 0)
            } else {
                let value = (raw as f64 / self.scale).round().clamp(self.min, max);
                ParamSpec::float(value, self.min, max, self.step)
            }
            .persistent(),
        )
    }

    /// The attribute's value for a parameter value.
    pub fn raw(&self, value: &Value) -> Result<i64> {
        match value {
            Value::Bool(on) => Ok(i64::from(*on)),
            v => {
                let v = v
                    .as_f64()
                    .ok_or_else(|| Error::InvalidValue("expected a number".into()))?;
                Ok((v * self.scale).round().max(0.0) as i64)
            }
        }
    }

    pub fn write(&self, hid: &HidDevice, value: &Value) -> Result<()> {
        hid.write(self.attr, &self.raw(value)?.to_string())
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
            .order(order)
            .persistent();
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
