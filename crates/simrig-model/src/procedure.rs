//! Guided procedures defined by plugins: calibrations, mappings, first-time setup…
//!
//! The core knows nothing about what a procedure does. The plugin drives it step by step; each
//! step is described with generic elements (fields, live values, tables, notices, actions) that
//! the app renders without knowing the procedure.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::param::ParamType;
use crate::support::Support;
use crate::value::Value;

/// A procedure a plugin offers on a component or on a device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProcedureDescriptor {
    /// Unique within its component or device.
    pub id: String,
    /// Already translated by the plugin.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// A procedure can be unavailable too, always with a reason.
    pub availability: Support<()>,
}

impl ProcedureDescriptor {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            help: None,
            availability: Support::Supported(()),
        }
    }

    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn availability(mut self, availability: Support<()>) -> Self {
        self.availability = availability;
        self
    }
}

/// What a procedure runs on, by plugin-local id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ProcedureTarget {
    Device(String),
    Component(String),
}

/// Where a running procedure is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProcedureState {
    Step(Step),
    Finished {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Cancelled,
}

/// One screen of a procedure.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Buttons. The values of all fields are sent with the chosen one.
    #[serde(default)]
    pub actions: Vec<StepAction>,
    /// The plugin advances by itself (e.g. waiting for the user to move a lever); the app keeps
    /// showing the step and its actions (typically Cancel) until an update arrives.
    #[serde(default)]
    pub waiting: bool,
    /// Step `n` of `total`, when the plugin knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<[u32; 2]>,
}

impl Step {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn element(mut self, element: Element) -> Self {
        self.elements.push(element);
        self
    }

    pub fn action(mut self, action: StepAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn waiting(mut self) -> Self {
        self.waiting = true;
        self
    }

    pub fn progress(mut self, n: u32, total: u32) -> Self {
        self.progress = Some([n, total]);
        self
    }

    /// Checks the values sent with an action: every field present, valid for its type.
    pub fn validate(&self, values: &BTreeMap<String, Value>) -> Result<(), String> {
        for element in &self.elements {
            if let Element::Field { id, ty, .. } = element {
                let value = values
                    .get(id)
                    .ok_or_else(|| format!("missing value for '{id}'"))?;
                ty.validate(value).map_err(|e| format!("'{id}': {e}"))?;
            }
        }
        Ok(())
    }

    /// Default values of the fields, as initially shown.
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.elements
            .iter()
            .filter_map(|e| match e {
                Element::Field { id, value, .. } => Some((id.clone(), value.clone())),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "element", rename_all = "snake_case")]
pub enum Element {
    /// Something the user fills; its value is sent with the chosen action.
    Field {
        id: String,
        label: String,
        ty: ParamType,
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        help: Option<String>,
    },
    /// A read-only value; live when the plugin sends updates.
    Value {
        label: String,
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// E.g. the summary of a mapping.
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Notice {
        level: Level,
        text: String,
    },
}

impl Element {
    pub fn field(id: &str, label: &str, ty: ParamType, value: Value) -> Self {
        Element::Field {
            id: id.into(),
            label: label.into(),
            ty,
            value,
            help: None,
        }
    }

    pub fn value(label: &str, value: Value) -> Self {
        Element::Value {
            label: label.into(),
            value,
            unit: None,
        }
    }

    pub fn notice(level: Level, text: impl Into<String>) -> Self {
        Element::Notice {
            level,
            text: text.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepAction {
    pub id: String,
    pub label: String,
    pub role: ActionRole,
    /// Shown before running the action, which needs an explicit confirmation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

impl StepAction {
    pub fn new(id: &str, label: &str, role: ActionRole) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            role,
            confirm: None,
        }
    }

    pub fn next() -> Self {
        Self::new("next", "Next", ActionRole::Primary)
    }

    pub fn finish() -> Self {
        Self::new("finish", "Finish", ActionRole::Primary)
    }

    pub fn cancel() -> Self {
        Self::new("cancel", "Cancel", ActionRole::Cancel)
    }
}

/// How the app presents an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRole {
    Primary,
    Secondary,
    /// Ends the procedure without changes. The app may also cancel on its own (window closed).
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_fields() {
        let step = Step::new("Kind")
            .element(Element::field(
                "kind",
                "Kind",
                ParamType::Choice { options: vec![] },
                Value::Choice("x".into()),
            ))
            .element(Element::value("Detected", Value::Int(3)));
        assert!(step.validate(&BTreeMap::new()).is_err());
        let ok = Step::new("Gears").element(Element::field(
            "gears",
            "Gears",
            ParamType::Int {
                min: 4,
                max: 8,
                step: 1,
            },
            Value::Int(6),
        ));
        assert!(ok.validate(&ok.defaults()).is_ok());
        assert!(
            ok.validate(&BTreeMap::from([("gears".to_string(), Value::Int(9))]))
                .is_err()
        );
    }

    #[test]
    fn serde_shape() {
        let state = ProcedureState::Step(
            Step::new("Press the brake")
                .waiting()
                .action(StepAction::cancel()),
        );
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["state"], "step");
        assert_eq!(
            serde_json::from_value::<ProcedureState>(json).unwrap(),
            state
        );
    }
}
