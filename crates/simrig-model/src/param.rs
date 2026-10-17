use serde::{Deserialize, Serialize};

use crate::value::{CurvePoint, Value, ValueKind};

/// Whether a value survives with the app closed. How it is persisted is up to the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    /// The driver (or the device) remembers it with the app closed and across replugs.
    Persistent,
    /// Lasts until the device is unplugged, or until a game changes it.
    Volatile,
}

/// A parameter as declared by a plugin: its type and range, current value and persistence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    pub ty: ParamType,
    pub value: Value,
    pub persistence: Persistence,
    /// For force-related parameters: the highest value the app may ever apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_limit: Option<f64>,
}

impl ParamSpec {
    pub fn new(ty: ParamType, value: Value) -> Self {
        Self {
            ty,
            value,
            persistence: Persistence::Volatile,
            force_limit: None,
        }
    }

    pub fn float(value: f64, min: f64, max: f64, step: f64) -> Self {
        Self::new(ParamType::Float { min, max, step }, Value::Float(value))
    }

    pub fn int(value: i64, min: i64, max: i64, step: i64) -> Self {
        Self::new(ParamType::Int { min, max, step }, Value::Int(value))
    }

    pub fn bool(value: bool) -> Self {
        Self::new(ParamType::Bool, Value::Bool(value))
    }

    pub fn choice(value: &str, options: &[(&str, &str)]) -> Self {
        let options = options
            .iter()
            .map(|(id, label)| ChoiceOption::new(id, label))
            .collect();
        Self::new(ParamType::Choice { options }, Value::Choice(value.into()))
    }

    pub fn curve(points: Vec<CurvePoint>, max_points: u8) -> Self {
        Self::new(ParamType::Curve { max_points }, Value::Curve(points))
    }

    pub fn persistent(mut self) -> Self {
        self.persistence = Persistence::Persistent;
        self
    }

    pub fn with_force_limit(mut self, limit: f64) -> Self {
        self.force_limit = Some(limit);
        self
    }
}

/// The type and valid range of a parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamType {
    Bool,
    Int { min: i64, max: i64, step: i64 },
    Float { min: f64, max: f64, step: f64 },
    Choice { options: Vec<ChoiceOption> },
    Curve { max_points: u8 },
    Text { max_len: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
}

impl ChoiceOption {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// A value that does not fit a parameter's type or range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueError(pub String);

impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ValueError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ValueError> {
    Err(ValueError(msg.into()))
}

impl ParamType {
    pub fn value_kind(&self) -> ValueKind {
        match self {
            ParamType::Bool => ValueKind::Bool,
            ParamType::Int { .. } => ValueKind::Int,
            ParamType::Float { .. } => ValueKind::Float,
            ParamType::Choice { .. } => ValueKind::Choice,
            ParamType::Curve { .. } => ValueKind::Curve,
            ParamType::Text { .. } => ValueKind::Text,
        }
    }

    /// Checks that `value` has the right type and is within range.
    pub fn validate(&self, value: &Value) -> Result<(), ValueError> {
        match (self, value) {
            (ParamType::Bool, Value::Bool(_)) => Ok(()),
            (ParamType::Int { min, max, .. }, Value::Int(v)) => {
                if v < min || v > max {
                    return err(format!("{v} is outside {min}..={max}"));
                }
                Ok(())
            }
            (ParamType::Float { min, max, .. }, Value::Float(v)) => {
                if !v.is_finite() || v < min || v > max {
                    return err(format!("{v} is outside {min}..={max}"));
                }
                Ok(())
            }
            (ParamType::Choice { options }, Value::Choice(v)) => {
                if options.iter().any(|o| &o.id == v) {
                    return Ok(());
                }
                let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
                err(format!("'{v}' is not one of: {}", ids.join(", ")))
            }
            (ParamType::Curve { max_points }, Value::Curve(points)) => {
                if points.len() < 2 || points.len() > usize::from(*max_points) {
                    return err(format!("a curve needs 2..={max_points} points"));
                }
                let in_range =
                    |p: &CurvePoint| (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y);
                if !points.iter().all(in_range) {
                    return err("curve points must be within 0..1");
                }
                if !points.windows(2).all(|w| w[0].x < w[1].x) {
                    return err("curve points must have increasing x");
                }
                Ok(())
            }
            (ParamType::Text { max_len }, Value::Text(v)) => {
                if v.chars().count() > *max_len {
                    return err(format!("text longer than {max_len} characters"));
                }
                Ok(())
            }
            (ty, v) => err(format!(
                "expected {:?}, got {:?}",
                ty.value_kind(),
                v.kind()
            )),
        }
    }

    /// Parses a textual value (as typed by a user) and validates it.
    ///
    /// Curves are written as `x:y` pairs separated by commas: `0:0,0.5:0.3,1:1`.
    pub fn parse(&self, text: &str) -> Result<Value, ValueError> {
        let text = text.trim();
        let value = match self {
            ParamType::Bool => match text.to_ascii_lowercase().as_str() {
                "true" | "on" | "yes" | "1" => Value::Bool(true),
                "false" | "off" | "no" | "0" => Value::Bool(false),
                _ => return err(format!("'{text}' is not a boolean")),
            },
            ParamType::Int { .. } => match text.parse() {
                Ok(v) => Value::Int(v),
                Err(_) => return err(format!("'{text}' is not an integer")),
            },
            ParamType::Float { .. } => match text.parse() {
                Ok(v) => Value::Float(v),
                Err(_) => return err(format!("'{text}' is not a number")),
            },
            ParamType::Choice { .. } => Value::Choice(text.to_string()),
            ParamType::Text { .. } => Value::Text(text.to_string()),
            ParamType::Curve { .. } => {
                let mut points = Vec::new();
                for pair in text.split(',') {
                    let parsed = pair.split_once(':').and_then(|(x, y)| {
                        Some(CurvePoint::new(
                            x.trim().parse().ok()?,
                            y.trim().parse().ok()?,
                        ))
                    });
                    match parsed {
                        Some(p) => points.push(p),
                        None => return err(format!("'{pair}' is not an x:y point")),
                    }
                }
                Value::Curve(points)
            }
        };
        self.validate(&value)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_range() {
        let ty = ParamType::Float {
            min: 90.0,
            max: 2520.0,
            step: 10.0,
        };
        assert!(ty.validate(&Value::Float(900.0)).is_ok());
        assert!(ty.validate(&Value::Float(3000.0)).is_err());
        assert!(ty.validate(&Value::Int(900)).is_err());
    }

    #[test]
    fn parse_values() {
        assert_eq!(ParamType::Bool.parse("on").unwrap(), Value::Bool(true));
        let curve = ParamType::Curve { max_points: 8 }
            .parse("0:0, 0.5:0.3, 1:1")
            .unwrap();
        assert_eq!(
            curve,
            Value::Curve(vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(0.5, 0.3),
                CurvePoint::new(1.0, 1.0),
            ])
        );
        assert!(
            ParamType::Curve { max_points: 8 }
                .parse("0.5:0,0.2:1")
                .is_err()
        );
        let choice = ParamSpec::choice("a", &[("a", "A"), ("b", "B")]).ty;
        assert!(choice.parse("b").is_ok());
        assert!(choice.parse("c").is_err());
    }
}
