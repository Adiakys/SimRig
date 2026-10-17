//! Checks that a plugin's component description respects the standard interfaces. The host
//! runs them on every attach and reports violations as problems; the SDK test suite of each
//! plugin should keep them at zero.

use std::collections::HashSet;

use simrig_model::{
    CUSTOM_PREFIX, ComponentDescription, ComponentKind, ItemSpec, Reason, Support, ValueKind,
};

/// Returns a human-readable list of violations; empty when the description conforms.
pub fn check(desc: &ComponentDescription) -> Vec<String> {
    let mut problems = Vec::new();
    let mut fail = |msg: String| problems.push(format!("{}: {msg}", desc.id));

    match (desc.kind, desc.pedals.is_empty()) {
        (ComponentKind::Pedals, true) => fail("a pedal set must declare its pedals".into()),
        (kind, false) if kind != ComponentKind::Pedals => {
            fail("only pedals may declare pedals".into())
        }
        _ => {}
    }

    let expected = desc.kind.item_ids(&desc.pedals);
    let ids: Vec<&str> = desc.items.iter().map(|i| i.id.as_str()).collect();
    for id in &expected {
        if !ids.contains(&id.as_str()) {
            fail(format!("standard item '{id}' missing"));
        }
    }
    let mut seen = HashSet::new();
    for item in &desc.items {
        if !seen.insert(&item.id) {
            fail(format!("item '{}' listed twice", item.id));
        }
        let Some(def) = desc.kind.item_def(&item.id) else {
            fail(format!(
                "'{}' is not a standard item of {}",
                item.id,
                desc.kind.key()
            ));
            continue;
        };
        if def.class != item.class {
            fail(format!(
                "'{}' has class {:?}, expected {:?}",
                item.id, item.class, def.class
            ));
        }
        match &item.support {
            Support::Supported(ItemSpec::Param(spec)) => {
                if spec.ty.value_kind() != def.kind {
                    fail(format!(
                        "'{}' is {:?}, expected {:?}",
                        item.id,
                        spec.ty.value_kind(),
                        def.kind
                    ));
                }
                if let Err(e) = spec.ty.validate(&spec.value) {
                    fail(format!("'{}' current value is invalid: {e}", item.id));
                }
            }
            Support::Supported(ItemSpec::Output { ty }) if ty.value_kind() != def.kind => {
                fail(format!(
                    "output '{}' is {:?}, expected {:?}",
                    item.id,
                    ty.value_kind(),
                    def.kind
                ));
            }
            Support::Unsupported(u) => {
                if let Reason::DependsOn { item: other, .. } = &u.reason {
                    if !refers_to_item(desc, other) {
                        fail(format!("'{}' depends on unknown item '{other}'", item.id));
                    }
                }
            }
            _ => {}
        }
    }

    // Safety: whoever can move the wheel must be able to stop it.
    if desc.kind == ComponentKind::WheelBase
        && supported(desc, "test_effect")
        && !supported(desc, "stop_force")
    {
        fail("test_effect is supported but stop_force is not".into());
    }

    let mut procedure_ids = HashSet::new();
    for p in &desc.procedures {
        if p.id.trim().is_empty() || !procedure_ids.insert(&p.id) {
            fail(format!("procedure '{}' has an empty or repeated id", p.id));
        }
        if p.label.trim().is_empty() {
            fail(format!("procedure '{}' has no label", p.id));
        }
    }

    let mut custom_ids = HashSet::new();
    for param in &desc.custom {
        let id = &param.id;
        if !custom_ids.insert(id) {
            fail(format!("custom parameter '{id}' listed twice"));
        }
        if param.label.trim().is_empty() {
            fail(format!("custom parameter '{id}' has no label"));
        }
        if param.ty.value_kind() == ValueKind::Empty {
            fail(format!("custom parameter '{id}' has no type"));
        }
        for (what, value) in [("value", &param.value), ("default", &param.default)] {
            if let Err(e) = param.ty.validate(value) {
                fail(format!("custom parameter '{id}' {what} is invalid: {e}"));
            }
        }
        if let Some(cond) = &param.enabled_if {
            if !refers_to_item(desc, &cond.item) {
                fail(format!(
                    "custom parameter '{id}' depends on unknown item '{}'",
                    cond.item
                ));
            }
        }
    }
    problems
}

fn supported(desc: &ComponentDescription, id: &str) -> bool {
    desc.item(id).is_some_and(|i| i.support.is_supported())
}

fn refers_to_item(desc: &ComponentDescription, id: &str) -> bool {
    match id.strip_prefix(CUSTOM_PREFIX) {
        Some(custom) => desc.custom_param(custom).is_some(),
        None => desc.item(id).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simrig_model::{ActionSpec, ItemDescription, ParamSpec};

    fn base(items: Vec<ItemDescription>) -> ComponentDescription {
        ComponentDescription {
            id: "base".into(),
            kind: ComponentKind::WheelBase,
            name: "Base".into(),
            model: None,
            pedals: vec![],
            items,
            custom: vec![],
            procedures: vec![],
        }
    }

    fn all_unsupported() -> Vec<ItemDescription> {
        ComponentKind::WheelBase
            .items()
            .into_iter()
            .map(|d| ItemDescription {
                id: d.key.into(),
                class: d.class,
                support: Support::not_in_driver(),
            })
            .collect()
    }

    #[test]
    fn all_unsupported_conforms() {
        assert!(check(&base(all_unsupported())).is_empty());
    }

    #[test]
    fn detects_violations() {
        let mut items = all_unsupported();
        items.retain(|i| i.id != "firmware");
        for item in &mut items {
            match item.id.as_str() {
                "test_effect" => {
                    item.support = Support::Supported(ItemSpec::Action(ActionSpec::default()))
                }
                "rotation" => {
                    item.support = Support::Supported(ItemSpec::Param(ParamSpec::bool(true)))
                }
                _ => {}
            }
        }
        let problems = check(&base(items));
        assert!(problems.iter().any(|p| p.contains("'firmware' missing")));
        assert!(problems.iter().any(|p| p.contains("stop_force")));
        assert!(problems.iter().any(|p| p.contains("'rotation' is Bool")));
    }
}
