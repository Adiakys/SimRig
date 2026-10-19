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
    /// Hints for the core's generic picture (e.g. the shape of a rim).
    #[serde(default, skip_serializing_if = "Appearance::is_default")]
    pub appearance: Appearance,
    /// The physical controls the component's inputs come from (a rim's buttons, hat switches,
    /// encoders, analog paddles), as games number them, for the app to list them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<Control>,
}

/// One physical control of a component, and where its live value is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Control {
    pub kind: ControlKind,
    /// As games number it among controls of its kind (buttons, axes, hats), from 1.
    pub number: u32,
    /// The kernel's name for it (`BTN_THUMB`, `ABS_RX`), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// What it is on the device ("Shift up"), when the plugin knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The standard input carrying its live value (`buttons`, `hats`…), and its position there.
    pub item: String,
    pub index: usize,
    /// An axis's resolution, in bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bits: Option<u8>,
    /// The buttons an encoder reports its steps with (their numbers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    Button,
    Axis,
    Hat,
    Encoder,
}

impl Control {
    pub fn new(kind: ControlKind, number: u32, item: &str, index: usize) -> Self {
        Self {
            kind,
            number,
            code: None,
            label: None,
            item: item.into(),
            index,
            bits: None,
            buttons: Vec::new(),
        }
    }

    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn bits(mut self, bits: u8) -> Self {
        self.bits = Some(bits);
        self
    }
}

/// How the core draws a component when the plugin ships no picture of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    /// Shape of a steering wheel; the core draws a round wheel when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rim: Option<RimShape>,
}

impl Appearance {
    pub fn rim(shape: RimShape) -> Self {
        Self { rim: Some(shape) }
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RimShape {
    Round,
    /// GT3-style: cut flat at the top and bottom, grips on the sides.
    Gt,
    Formula,
}

impl RimShape {
    pub fn key(self) -> &'static str {
        match self {
            RimShape::Round => "round",
            RimShape::Gt => "gt",
            RimShape::Formula => "formula",
        }
    }
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
