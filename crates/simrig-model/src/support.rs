use serde::{Deserialize, Serialize};

use crate::value::Value;

/// Whether an item is supported. Unsupported items always carry a reason, so the UI can show
/// them disabled with an explanation instead of hiding them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support<T> {
    Supported(T),
    Unsupported(Unsupported),
}

impl<T> Support<T> {
    /// Default answer of every SDK interface method: the plugin did not implement the item.
    pub fn not_in_driver() -> Self {
        Support::Unsupported(Unsupported::new(Reason::NotInDriver))
    }

    /// The hardware physically lacks the feature.
    pub fn not_in_hardware(detail: impl Into<String>) -> Self {
        Support::Unsupported(Unsupported::new(Reason::NotInHardware).with_detail(detail))
    }

    /// The item is available only when another item has a given value.
    pub fn depends_on(item: impl Into<String>, value: Value) -> Self {
        Support::Unsupported(Unsupported::new(Reason::DependsOn {
            item: item.into(),
            value,
        }))
    }

    pub fn is_supported(&self) -> bool {
        matches!(self, Support::Supported(_))
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Support<U> {
        match self {
            Support::Supported(v) => Support::Supported(f(v)),
            Support::Unsupported(u) => Support::Unsupported(u),
        }
    }

    pub fn supported(self) -> Option<T> {
        match self {
            Support::Supported(v) => Some(v),
            Support::Unsupported(_) => None,
        }
    }

    pub fn as_ref(&self) -> Support<&T> {
        match self {
            Support::Supported(v) => Support::Supported(v),
            Support::Unsupported(u) => Support::Unsupported(u.clone()),
        }
    }
}

/// Why an item is unsupported, plus an optional plugin-provided detail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unsupported {
    pub reason: Reason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Unsupported {
    pub fn new(reason: Reason) -> Self {
        Self {
            reason,
            detail: None,
        }
    }

    pub fn not_in_driver() -> Self {
        Self::new(Reason::NotInDriver)
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Full explanation: the core text for the reason, followed by the plugin's detail.
    pub fn explain(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{} {}", self.reason.explain(), detail),
            None => self.reason.explain(),
        }
    }
}

/// The standard reasons for an unsupported item. The core owns their (translated) texts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    /// The hardware does not have this feature.
    NotInHardware,
    /// The driver or plugin does not support this (yet).
    NotInDriver,
    /// The device needs a newer firmware.
    NeedsFirmware { min: String },
    /// Another component must be connected or mounted.
    NeedsComponent { what: String },
    /// Available only when another item of the same component has a given value.
    DependsOn { item: String, value: Value },
    /// Temporarily unavailable (device busy, reconnecting…).
    Unavailable,
}

impl Reason {
    pub fn explain(&self) -> String {
        match self {
            Reason::NotInHardware => "Not available on this hardware.".into(),
            Reason::NotInDriver => "Not supported by the driver (yet).".into(),
            Reason::NeedsFirmware { min } => format!("Requires firmware {min} or later."),
            Reason::NeedsComponent { what } => format!("Requires {what}."),
            Reason::DependsOn { item, value } => format!("Available when {item} is {value}."),
            Reason::Unavailable => "Temporarily unavailable.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_includes_detail() {
        let u = Unsupported::new(Reason::NotInHardware).with_detail("No load cell.");
        assert_eq!(u.explain(), "Not available on this hardware. No load cell.");
    }

    #[test]
    fn serde_shape() {
        let s: Support<()> = Support::depends_on("mode", Value::Choice("button".into()));
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["unsupported"]["reason"]["kind"], "depends_on");
        assert_eq!(serde_json::from_value::<Support<()>>(json).unwrap(), s);
    }
}
