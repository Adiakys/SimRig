//! Guided procedures defined by the plugin, driven through the host as the app does.

mod common;

use std::collections::BTreeMap;

use common::{Env, GENERIC_PLUGIN, poll_until};
use simrig_host::{Host, HostError, HostEvent, PluginState, Target};
use simrig_model::{Element, ProcedureState, Step, Value};

fn step(state: ProcedureState) -> Step {
    match state {
        ProcedureState::Step(step) => step,
        other => panic!("expected a step, got {other:?}"),
    }
}

/// Waits for the plugin to move a procedure on by itself, until `done` holds.
fn wait_step(host: &mut Host, session: &str, done: impl Fn(&Step) -> bool) -> Step {
    let mut last = None;
    poll_until(host, "procedure update", |e| match e {
        HostEvent::ProcedureUpdated {
            session: s,
            state: ProcedureState::Step(step),
        } if s == session => done(step),
        _ => false,
    })
    .into_iter()
    .for_each(|e| {
        if let HostEvent::ProcedureUpdated {
            state: ProcedureState::Step(step),
            ..
        } = e
        {
            last = Some(step);
        }
    });
    last.unwrap()
}

fn values(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn component_procedures_are_listed_and_applied() {
    let env = Env::new("calibration");
    let mut host = env.start();
    let pedals = Target::Component("sim/pedals0/pedals".into());
    let procedures = host.procedures(&pedals).unwrap();
    assert_eq!(procedures.len(), 3);
    assert!(
        host.find("pedals")
            .unwrap()
            .description
            .procedures
            .iter()
            .any(|p| p.id == "calibrate-brake")
    );

    let (session, state) = host.start_procedure(&pedals, "calibrate-brake").unwrap();
    let first = step(state);
    assert_eq!(first.progress, Some([1, 3]));
    assert!(
        first
            .elements
            .iter()
            .any(|e| matches!(e, Element::Value { .. }))
    );
    assert!(
        host.submit_procedure(&session, "fly", BTreeMap::new())
            .is_err()
    );

    step(
        host.submit_procedure(&session, "next", BTreeMap::new())
            .unwrap(),
    );
    let summary = step(
        host.submit_procedure(&session, "next", BTreeMap::new())
            .unwrap(),
    );
    assert!(
        summary.actions.iter().any(|a| a.id == "finish"),
        "{summary:?}"
    );
    let done = host
        .submit_procedure(&session, "finish", BTreeMap::new())
        .unwrap();
    assert!(matches!(done, ProcedureState::Finished { .. }));

    // The plugin's result was applied to the component, validated like any change.
    let min = host.param("pedals", "brake.calibration_min").unwrap().value;
    let max = host.param("pedals", "brake.calibration_max").unwrap().value;
    assert!(matches!((min, max), (Value::Int(lo), Value::Int(hi)) if lo < 3000 && hi > 60000));
    assert!(matches!(
        host.submit_procedure(&session, "next", BTreeMap::new()),
        Err(HostError::NotFound(_))
    ));
}

#[test]
fn procedures_can_be_cancelled() {
    let env = Env::new("cancel");
    let mut host = env.start();
    let pedals = Target::Component("sim/pedals0/pedals".into());
    let (session, _) = host.start_procedure(&pedals, "calibrate-throttle").unwrap();
    assert_eq!(
        host.submit_procedure(&session, "cancel", BTreeMap::new())
            .unwrap(),
        ProcedureState::Cancelled
    );
    let (session, _) = host.start_procedure(&pedals, "calibrate-throttle").unwrap();
    host.cancel_procedure(&session).unwrap();
    assert!(
        host.submit_procedure(&session, "next", BTreeMap::new())
            .is_err()
    );
}

#[test]
fn a_device_procedure_turns_a_generic_device_into_a_component() {
    let env = Env::new("setup");
    let mut host = env.start();
    let device = host.target("sim/generic0").unwrap();
    assert_eq!(
        device,
        Target::Device {
            plugin: "sim".into(),
            device: "generic0".into()
        }
    );
    assert!(host.components().iter().all(|c| c.device.id != "generic0"));
    assert_eq!(
        host.procedures(&device).unwrap()[0].label,
        "Set up this device"
    );

    let (session, state) = host.start_procedure(&device, "setup").unwrap();
    let kind = step(state);
    // fields are validated: 9 gears is out of range
    let bad = values(&[
        ("kind", Value::Choice("shifter".into())),
        ("gears", Value::Int(9)),
    ]);
    assert!(matches!(
        host.submit_procedure(&session, "next", bad),
        Err(HostError::InvalidValue(_))
    ));
    let mut answers = kind.defaults();
    answers.insert("gears".into(), Value::Int(5));
    let waiting = step(host.submit_procedure(&session, "next", answers).unwrap());
    assert!(waiting.waiting);

    // The plugin detects each position by itself and moves on.
    let summary = wait_step(&mut host, &session, |s| s.title == "Check the mapping");
    let rows = summary.elements.iter().find_map(|e| match e {
        Element::Table { rows, .. } => Some(rows.len()),
        _ => None,
    });
    assert_eq!(rows, Some(6), "5 gears and reverse");
    let done = host
        .submit_procedure(&session, "finish", BTreeMap::new())
        .unwrap();
    assert!(
        matches!(&done, ProcedureState::Finished { message: Some(m) } if m.contains("not saved")),
        "{done:?}"
    );

    poll_until(
        &mut host,
        "generic shifter",
        |e| matches!(e, HostEvent::ComponentsChanged { device, .. } if device == "generic0"),
    );
    assert_eq!(
        host.param("generic0/shifter", "gears").unwrap().value,
        Value::Int(5)
    );
    assert_eq!(host.procedures(&device).unwrap()[0].label, "Set up again");
}

#[test]
fn a_saved_setup_is_restored_by_the_driver() {
    let env = Env::new("setup-saved");
    let mut host = env.start();
    host.install_driver("sim").unwrap();
    let device = host.target("sim/generic0").unwrap();
    let (session, state) = host.start_procedure(&device, "setup").unwrap();
    let mut answers = step(state).defaults();
    answers.insert("kind".into(), Value::Choice("handbrake".into()));
    host.submit_procedure(&session, "next", answers).unwrap();
    wait_step(&mut host, &session, |s| s.title == "Check the mapping");
    let done = host
        .submit_procedure(&session, "finish", BTreeMap::new())
        .unwrap();
    assert!(
        matches!(&done, ProcedureState::Finished { message: Some(m) } if !m.contains("not saved")),
        "{done:?}"
    );
    drop(host);

    let mut host = env.start();
    assert!(
        host.find("generic0/handbrake").is_ok(),
        "setup not restored"
    );
    host.uninstall_driver("sim").unwrap();
}

#[test]
fn generic_plugins_take_only_devices_assigned_to_them() {
    let env = Env::new("on-request");
    env.add_generic_plugin();
    env.plug_unknown_usb("3-2");
    let mut host = env.start();
    let generic = |host: &Host| {
        host.plugins()
            .iter()
            .find(|p| p.manifest.id == GENERIC_PLUGIN)
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(generic(&host), PluginState::Idle);
    let unknown = host
        .usb_devices()
        .into_iter()
        .find(|(d, _)| d.vid == 0x1234)
        .unwrap();
    assert_eq!(unknown.1, None, "nobody claims an unknown device");

    // The user picks the device for the generic plugin: applied at once.
    host.choose_plugin("usb:1234:0001", Some(GENERIC_PLUGIN))
        .unwrap();
    assert_eq!(generic(&host), PluginState::Running);
    let target = host
        .target(&format!("{GENERIC_PLUGIN}/usb-1234-0001"))
        .unwrap();
    assert_eq!(host.procedures(&target).unwrap()[0].id, "setup");

    // Clearing the choice gives the device back.
    host.choose_plugin("usb:1234:0001", None).unwrap();
    assert_eq!(generic(&host), PluginState::Idle);
    assert!(
        host.target(&format!("{GENERIC_PLUGIN}/usb-1234-0001"))
            .is_err()
    );
}
