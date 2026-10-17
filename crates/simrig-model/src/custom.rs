use serde::{Deserialize, Serialize};

use crate::param::{ParamType, Persistence};
use crate::support::Support;
use crate::value::Value;

/// A plugin-defined parameter beyond the standard interface. It is fully self-described so
/// the app can render it without knowing anything about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomParam {
    /// Unique within the plugin, e.g. `drift_mode`.
    pub id: String,
    /// Already translated by the plugin.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    pub ty: ParamType,
    /// Free unit label: `%`, `Nm`, `Hz`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub value: Value,
    pub default: Value,
    /// UI section, e.g. "Force feedback filters".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default)]
    pub order: i32,
    pub persistence: Persistence,
    /// Placed in the collapsible "Advanced" section.
    #[serde(default)]
    pub advanced: bool,
    /// Enabled only when another item has a given value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_if: Option<Condition>,
    /// A custom parameter can be unsupported too, always with a reason.
    pub availability: Support<()>,
    /// Mandatory when the parameter controls force.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_limit: Option<f64>,
}

impl CustomParam {
    pub fn new(id: &str, label: &str, ty: ParamType, value: Value) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            help: None,
            ty,
            unit: None,
            default: value.clone(),
            value,
            group: None,
            order: 0,
            persistence: Persistence::Volatile,
            advanced: false,
            enabled_if: None,
            availability: Support::Supported(()),
            force_limit: None,
        }
    }

    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn group(mut self, group: &str) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn default_value(mut self, default: Value) -> Self {
        self.default = default;
        self
    }

    pub fn persistent(mut self) -> Self {
        self.persistence = Persistence::Persistent;
        self
    }

    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    /// Enables the parameter only when `item` equals `value`. If `current` (the item's current
    /// value) does not match, the parameter is reported as unsupported with `DependsOn`.
    pub fn enabled_if(mut self, item: &str, value: Value, current: &Value) -> Self {
        if current != &value {
            self.availability = Support::depends_on(item, value.clone());
        }
        self.enabled_if = Some(Condition {
            item: item.into(),
            equals: value,
        });
        self
    }

    pub fn availability(mut self, availability: Support<()>) -> Self {
        self.availability = availability;
        self
    }
}

/// `item` (a standard item key or `custom.<id>`) must equal `equals`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub item: String,
    pub equals: Value,
}
