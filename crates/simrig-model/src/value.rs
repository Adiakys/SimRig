use std::fmt;

use serde::{Deserialize, Serialize};

/// A value exchanged between the app and plugins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Empty,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Choice(String),
    Curve(Vec<CurvePoint>),
    Axis(Axis),
    Gear(Gear),
    Buttons(Vec<bool>),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
}

/// The type of a [`Value`], without its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Empty,
    Bool,
    Int,
    Float,
    Text,
    Choice,
    Curve,
    Axis,
    Gear,
    Buttons,
    Ints,
    Floats,
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Empty => ValueKind::Empty,
            Value::Bool(_) => ValueKind::Bool,
            Value::Int(_) => ValueKind::Int,
            Value::Float(_) => ValueKind::Float,
            Value::Text(_) => ValueKind::Text,
            Value::Choice(_) => ValueKind::Choice,
            Value::Curve(_) => ValueKind::Curve,
            Value::Axis(_) => ValueKind::Axis,
            Value::Gear(_) => ValueKind::Gear,
            Value::Buttons(_) => ValueKind::Buttons,
            Value::Ints(_) => ValueKind::Ints,
            Value::Floats(_) => ValueKind::Floats,
        }
    }

    /// Numeric view of the value, for `Int` and `Float`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Canonical text form, the inverse of [`ParamType::parse`](crate::ParamType::parse) for
    /// parameter values.
    pub fn to_text(&self) -> String {
        match self {
            Value::Bool(v) => v.to_string(),
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Text(v) | Value::Choice(v) => v.clone(),
            Value::Curve(points) => {
                let points: Vec<String> =
                    points.iter().map(|p| format!("{}:{}", p.x, p.y)).collect();
                points.join(",")
            }
            other => other.to_string(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(v) | Value::Choice(v) => Some(v),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Empty => f.write_str("-"),
            Value::Bool(v) => write!(f, "{}", if *v { "on" } else { "off" }),
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v:.2}"),
            Value::Text(v) => write!(f, "\"{v}\""),
            Value::Choice(v) => f.write_str(v),
            Value::Curve(points) => {
                let points: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.2}:{:.2}", p.x, p.y))
                    .collect();
                f.write_str(&points.join(","))
            }
            Value::Axis(a) => write!(f, "{:.3} (raw {})", a.value, a.raw),
            Value::Gear(g) => write!(f, "{g}"),
            Value::Buttons(b) => {
                let s: String = b.iter().map(|on| if *on { '●' } else { '·' }).collect();
                f.write_str(&s)
            }
            Value::Ints(v) => {
                let s: Vec<String> = v.iter().map(i64::to_string).collect();
                f.write_str(&s.join(" "))
            }
            Value::Floats(v) => {
                let s: Vec<String> = v.iter().map(|x| format!("{x:.2}")).collect();
                f.write_str(&s.join(" "))
            }
        }
    }
}

/// A point of a response curve; both coordinates in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

impl CurvePoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An analog axis: raw device value and calibrated value in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    pub raw: i64,
    pub value: f64,
}

/// A gear of an H-pattern shifter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gear {
    Reverse,
    Neutral,
    Forward(u8),
}

impl fmt::Display for Gear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gear::Reverse => f.write_str("R"),
            Gear::Neutral => f.write_str("N"),
            Gear::Forward(n) => write!(f, "{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let values = [
            Value::Empty,
            Value::Float(1.5),
            Value::Choice("analog".into()),
            Value::Gear(Gear::Forward(3)),
            Value::Curve(vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)]),
        ];
        for v in values {
            let json = serde_json::to_string(&v).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), v);
        }
    }

    #[test]
    fn text_roundtrip() {
        use crate::ParamType;
        let cases = [
            (
                ParamType::Float {
                    min: 0.0,
                    max: 2520.0,
                    step: 0.5,
                },
                Value::Float(540.5),
            ),
            (
                ParamType::Int {
                    min: 0,
                    max: 10,
                    step: 1,
                },
                Value::Int(7),
            ),
            (ParamType::Bool, Value::Bool(true)),
            (
                ParamType::Curve { max_points: 4 },
                Value::Curve(vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.75)]),
            ),
            (
                ParamType::Text { max_len: 16 },
                Value::Text("P1 +0.4".into()),
            ),
        ];
        for (ty, value) in cases {
            assert_eq!(ty.parse(&value.to_text()).unwrap(), value);
        }
    }

    #[test]
    fn gear_display() {
        assert_eq!(Gear::Reverse.to_string(), "R");
        assert_eq!(Gear::Forward(5).to_string(), "5");
    }
}
