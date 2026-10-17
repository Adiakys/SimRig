//! Text rendering of components. Unsupported items are shown, disabled, with their reason.

use std::collections::BTreeMap;
use std::fmt::Write;

use simrig_host::HostComponent;
use simrig_model::{ItemClass, ItemSpec, ParamType, Persistence, Support, Unit, Value};

pub fn unit(c: &HostComponent, item: &str) -> Option<Unit> {
    c.description.kind.item_def(item).and_then(|d| d.unit)
}

pub fn value_with_unit(value: &Value, unit: Option<Unit>) -> String {
    match unit.map(Unit::symbol) {
        Some(symbol) if !symbol.is_empty() && value.as_f64().is_some() => {
            format!("{value} {symbol}")
        }
        _ => value.to_string(),
    }
}

fn range(ty: &ParamType) -> String {
    match ty {
        ParamType::Bool => "on/off".into(),
        ParamType::Int { min, max, step } => format!("{min}..{max} step {step}"),
        ParamType::Float { min, max, step } => format!("{min}..{max} step {step}"),
        ParamType::Choice { options } => options
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>()
            .join(" | "),
        ParamType::Curve { max_points } => format!("2..{max_points} points x:y"),
        ParamType::Text { max_len } => format!("text, max {max_len}"),
    }
}

fn persistence(p: Persistence) -> &'static str {
    match p {
        Persistence::Persistent => "persistent",
        Persistence::Volatile => "until unplugged",
    }
}

fn label(c: &HostComponent, item: &str) -> String {
    let Some(def) = c.description.kind.item_def(item) else {
        return item.into();
    };
    match item.split_once('.') {
        Some((pedal, _)) => format!("{} {}", capitalize(pedal), def.label.to_lowercase()),
        None => def.label.into(),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub fn component(c: &HostComponent, inputs: &BTreeMap<String, Value>) -> String {
    let d = &c.description;
    let mut out = String::new();
    let _ = writeln!(out, "{} — {} ({})", c.address, d.name, d.kind.label());
    if let Some(model) = &d.model {
        let _ = writeln!(
            out,
            "model {model}, device {} ({})",
            c.device.name, c.device.id
        );
    }
    for (class, title) in [
        (ItemClass::Input, "Inputs"),
        (ItemClass::Param, "Parameters"),
        (ItemClass::Output, "Outputs"),
        (ItemClass::Action, "Actions"),
    ] {
        let items: Vec<_> = d.items.iter().filter(|i| i.class == class).collect();
        if items.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n{title}");
        for item in items {
            let name = format!("{} ({})", label(c, &item.id), item.id);
            let detail = match &item.support {
                Support::Unsupported(u) => format!("✗ unavailable — {}", u.explain()),
                Support::Supported(ItemSpec::Input) => match inputs.get(&item.id) {
                    Some(v) => value_with_unit(v, unit(c, &item.id)),
                    None => "-".into(),
                },
                Support::Supported(ItemSpec::Param(spec)) => format!(
                    "{}   [{}] {}",
                    value_with_unit(&spec.value, unit(c, &item.id)),
                    range(&spec.ty),
                    persistence(spec.persistence)
                ),
                Support::Supported(ItemSpec::Output { ty }) => format!("[{}]", range(ty)),
                Support::Supported(ItemSpec::Action(a)) => match &a.confirm {
                    Some(text) => format!("asks confirmation: {text}"),
                    None => String::new(),
                },
            };
            let _ = writeln!(out, "  {name:<44} {detail}");
        }
    }
    if !d.custom.is_empty() {
        let _ = writeln!(out, "\nCustom parameters (plugin '{}')", c.plugin);
        let mut custom: Vec<_> = d.custom.iter().collect();
        custom.sort_by_key(|p| (p.group.clone(), p.order));
        let mut group = None;
        for p in custom {
            if group != Some(&p.group) {
                group = Some(&p.group);
                let _ = writeln!(out, "  [{}]", p.group.as_deref().unwrap_or("General"));
            }
            let name = format!(
                "{} (custom.{}){}",
                p.label,
                p.id,
                if p.advanced { " ⚙" } else { "" }
            );
            let detail = match &p.availability {
                Support::Unsupported(u) => format!("✗ unavailable — {}", u.explain()),
                Support::Supported(()) => {
                    let unit = p
                        .unit
                        .as_deref()
                        .map(|u| format!(" {u}"))
                        .unwrap_or_default();
                    format!(
                        "{}{unit}   [{}] {}",
                        p.value,
                        range(&p.ty),
                        persistence(p.persistence)
                    )
                }
            };
            let _ = writeln!(out, "    {name:<42} {detail}");
        }
    }
    out
}
