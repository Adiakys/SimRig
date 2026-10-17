use serde::{Deserialize, Serialize};

use crate::component::{ComponentKind, ItemClass, PedalRole};
use crate::custom::CustomParam;
use crate::param::{ParamSpec, ParamType};
use crate::procedure::ProcedureDescriptor;
use crate::support::Support;

/// Everything the app needs to render a component: the state of every standard item
/// (supported or not, with the reason) and the custom parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentDescription {
    /// Unique within the plugin.
    pub id: String,
    pub kind: ComponentKind,
    pub name: String,
    /// Catalog model id, e.g. `fanatec/csl-dd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// For pedals: which pedals the set has.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pedals: Vec<PedalRole>,
    pub items: Vec<ItemDescription>,
    #[serde(default)]
    pub custom: Vec<CustomParam>,
    /// Guided procedures the plugin offers on this component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub procedures: Vec<ProcedureDescriptor>,
}

impl ComponentDescription {
    pub fn item(&self, id: &str) -> Option<&ItemDescription> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn custom_param(&self, id: &str) -> Option<&CustomParam> {
        self.custom.iter().find(|p| p.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDescription {
    /// Standard item id: `rotation`, or `brake.position` for pedals.
    pub id: String,
    pub class: ItemClass,
    pub support: Support<ItemSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum ItemSpec {
    Input,
    Param(ParamSpec),
    Output { ty: ParamType },
    Action(ActionSpec),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    /// When set, the app must show this text and get an explicit confirmation before running
    /// the action (e.g. effects that move the wheel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

impl ActionSpec {
    pub fn confirm(text: &str) -> Self {
        Self {
            confirm: Some(text.into()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ActionOutcome {
    pub fn message(text: impl Into<String>) -> Self {
        Self {
            message: Some(text.into()),
        }
    }
}
