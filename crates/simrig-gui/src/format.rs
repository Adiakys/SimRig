//! How values read in the interface: numbers with their unit, live inputs, item labels.

use simrig_model::{ComponentKind, PedalRole, Unit, Value};

/// A number with the precision its step allows: `540`, `0.25`, `12.5`.
pub fn number(value: f64, step: f64) -> String {
    let decimals = if step <= 0.0 || step >= 1.0 {
        0
    } else {
        // 0.5 → 1, 0.05 → 2, 0.01 → 2
        (-(step.log10()).floor()).clamp(1.0, 3.0) as usize
    };
    let text = format!("{value:.decimals$}");
    // "−" reads better than "-" in a large figure.
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().any(|c| c != '0' && c != '.') => format!("−{rest}"),
        Some(rest) => rest.to_string(),
        None => text,
    }
}

pub fn unit(unit: Option<Unit>) -> String {
    match unit {
        Some(Unit::Raw) | Some(Unit::Count) | None => String::new(),
        Some(u) => u.symbol().to_string(),
    }
}

/// A live input as text, with the unit to draw next to it.
pub fn input(value: &Value, unit_of: Option<Unit>) -> (String, String) {
    match value {
        Value::Float(v) => {
            let precision = match unit_of {
                Some(Unit::Degrees) | Some(Unit::DegreesPerSecond) | Some(Unit::Percent) => 1.0,
                _ => 0.1,
            };
            (number(*v, precision), unit(unit_of))
        }
        Value::Int(v) => (v.to_string(), unit(unit_of)),
        Value::Axis(a) => (number(a.value * 100.0, 1.0), "%".into()),
        Value::Gear(g) => (g.to_string(), String::new()),
        Value::Bool(b) => ((if *b { "On" } else { "Off" }).into(), String::new()),
        Value::Text(t) | Value::Choice(t) => (t.clone(), String::new()),
        Value::Buttons(b) => {
            let pressed = b.iter().filter(|p| **p).count();
            (format!("{pressed} of {} pressed", b.len()), String::new())
        }
        Value::Ints(v) => (
            v.iter().map(i64::to_string).collect::<Vec<_>>().join("  "),
            String::new(),
        ),
        Value::Floats(v) => (
            v.iter()
                .map(|x| number(*x * 100.0, 1.0))
                .collect::<Vec<_>>()
                .join("  "),
            "%".into(),
        ),
        Value::Empty | Value::Curve(_) => ("–".into(), String::new()),
    }
}

pub fn role_label(role: PedalRole) -> &'static str {
    match role {
        PedalRole::Throttle => "Throttle",
        PedalRole::Brake => "Brake",
        PedalRole::Clutch => "Clutch",
    }
}

/// The label of a standard item; pedal items without the pedal (they sit under its heading).
pub fn item_label(kind: ComponentKind, id: &str) -> String {
    kind.item_def(id)
        .map(|d| d.label.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// Short help under some standard items, in plain words for beginners.
pub fn item_help(kind: ComponentKind, key: &str) -> &'static str {
    match (kind, key) {
        (ComponentKind::WheelBase, "rotation") => {
            "How far the wheel turns from one lock to the other."
        }
        (ComponentKind::WheelBase, "ffb_strength") => {
            "Overall strength of the forces the game sends."
        }
        (ComponentKind::WheelBase, "natural_damper") => {
            "Resistance that grows with speed, even without a game."
        }
        (ComponentKind::Pedals, "curve") => "How the pedal responds along its travel.",
        (ComponentKind::Pedals, "deadzone_low") => {
            "Travel ignored at the start, so a resting foot reads zero."
        }
        (ComponentKind::Pedals, "deadzone_high") => "Travel ignored at the end, to reach 100%.",
        (ComponentKind::Pedals, "max_force") => "The force that reads as fully pressed.",
        (ComponentKind::Shifter, "gears") => "The forward gears of the gate.",
        (ComponentKind::Shifter, "reverse_position") => "Where reverse sits in the gate.",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(540.0, 10.0), "540");
        assert_eq!(number(-38.4, 1.0), "−38");
        assert_eq!(number(-0.2, 1.0), "0");
        assert_eq!(number(12.5, 0.5), "12.5");
        assert_eq!(number(0.2, 0.05), "0.20");
    }
}
