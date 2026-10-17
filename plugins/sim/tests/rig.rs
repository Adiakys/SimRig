//! End-to-end: the host starts the simulated rig as an external plugin process and talks to
//! it through the protocol, exactly as the app does.

use std::path::{Path, PathBuf};
use std::time::Duration;

use simrig_host::{Host, HostError, HostOptions, PluginState};
use simrig_model::{ComponentKind, ItemSpec, Reason, Support, Value};

fn start() -> Host {
    let exec = PathBuf::from(env!("CARGO_BIN_EXE_simrig-plugin-sim"));
    let options = HostOptions {
        plugin_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))],
        default_dirs: false,
        exec_dirs: vec![exec.parent().unwrap_or(Path::new(".")).to_path_buf()],
        ..HostOptions::default()
    };
    Host::start(&options)
}

fn reason(host: &Host, address: &str, item: &str) -> Option<Reason> {
    match &host
        .find(address)
        .unwrap()
        .description
        .item(item)
        .unwrap()
        .support
    {
        Support::Supported(_) => None,
        Support::Unsupported(u) => Some(u.reason.clone()),
    }
}

#[test]
fn starts_a_conforming_rig() {
    let host = start();
    assert!(
        host.problems().is_empty(),
        "problems: {:?}",
        host.problems()
    );
    assert_eq!(host.plugins().len(), 1);
    assert_eq!(host.plugins()[0].state, PluginState::Running);
    let kinds: Vec<ComponentKind> = host
        .components()
        .iter()
        .map(|c| c.description.kind)
        .collect();
    assert_eq!(kinds, ComponentKind::ALL.to_vec());
    assert_eq!(host.find("base").unwrap().address, "sim/rig0/base");
}

#[test]
fn standard_parameters_are_validated() {
    let mut host = start();
    host.set("base", "rotation", Value::Float(540.0)).unwrap();
    assert_eq!(
        host.param("base", "rotation").unwrap().value,
        Value::Float(540.0)
    );
    assert!(matches!(
        host.set("base", "rotation", Value::Float(9000.0)),
        Err(HostError::InvalidValue(_))
    ));
    assert!(matches!(
        host.set("base", "rotation", Value::Bool(true)),
        Err(HostError::InvalidValue(_))
    ));
    match host.set("base", "natural_inertia", Value::Float(10.0)) {
        Err(HostError::Unsupported(u)) => {
            assert_eq!(u.reason, Reason::NotInHardware);
            assert!(u.detail.is_some());
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
    // the plugin rejects inconsistent values its own way
    assert!(
        host.set("pedals", "brake.calibration_min", Value::Int(65535))
            .is_err()
    );
}

#[test]
fn unsupported_items_carry_every_kind_of_reason() {
    let host = start();
    assert_eq!(
        reason(&host, "base", "natural_inertia"),
        Some(Reason::NotInHardware)
    );
    assert_eq!(
        reason(&host, "pedals", "throttle.vibration"),
        Some(Reason::NotInDriver)
    );
    assert!(matches!(
        reason(&host, "wheel", "flag_leds"),
        Some(Reason::NeedsComponent { .. })
    ));
    let base = &host.find("base").unwrap().description;
    let turbo = base.custom_param("turbo").unwrap();
    assert!(
        matches!(&turbo.availability, Support::Unsupported(u) if matches!(u.reason, Reason::NeedsFirmware { .. }))
    );
}

#[test]
fn dependencies_follow_the_mode() {
    let mut host = start();
    assert_eq!(reason(&host, "shifter", "gear"), None);
    assert!(matches!(
        reason(&host, "shifter", "sequential"),
        Some(Reason::DependsOn { .. })
    ));

    host.set("shifter", "mode", Value::Choice("sequential".into()))
        .unwrap();
    assert!(matches!(
        reason(&host, "shifter", "gear"),
        Some(Reason::DependsOn { .. })
    ));
    assert!(matches!(
        reason(&host, "shifter", "gears"),
        Some(Reason::DependsOn { .. })
    ));
    assert_eq!(reason(&host, "shifter", "sequential"), None);
    let inputs = host.read_inputs("shifter").unwrap();
    assert!(inputs.contains_key("sequential") && !inputs.contains_key("gear"));

    // custom parameters too
    let demo = |host: &Host| {
        host.find("base")
            .unwrap()
            .description
            .custom_param("demo_filter")
            .unwrap()
            .availability
            .is_supported()
    };
    assert!(!demo(&host));
    assert!(
        host.set("base", "custom.demo_filter", Value::Int(5))
            .is_err()
    );
    host.set("base", "custom.advanced_mode", Value::Bool(true))
        .unwrap();
    assert!(demo(&host));
    host.set("base", "custom.demo_filter", Value::Int(5))
        .unwrap();
}

#[test]
fn actions_and_outputs() {
    let mut host = start();
    let test_effect = host
        .find("base")
        .unwrap()
        .description
        .item("test_effect")
        .unwrap()
        .support
        .clone();
    assert!(matches!(test_effect, Support::Supported(ItemSpec::Action(a)) if a.confirm.is_some()));
    host.run_action("base", "test_effect", Value::Empty)
        .unwrap();
    assert_eq!(
        host.read_inputs("base").unwrap()["status"],
        Value::Text("test effect running".into())
    );
    host.run_action("base", "stop_force", Value::Empty).unwrap();
    assert_eq!(
        host.read_inputs("base").unwrap()["status"],
        Value::Text("ok".into())
    );
    assert!(
        host.run_action("wheel", "calibrate_paddles", Value::Empty)
            .is_err()
    );

    host.write_output("wheel", "display", Value::Text("P1 +0.4".into()))
        .unwrap();
    assert!(
        host.write_output(
            "wheel",
            "display",
            Value::Text("far too long for the display".into())
        )
        .is_err()
    );
}

#[test]
fn live_inputs() {
    let mut host = start();
    host.subscribe("pedals", 50.0).unwrap();
    let mut states = 0;
    while let Some(values) = host
        .next_input_state("pedals", Duration::from_secs(2))
        .unwrap()
    {
        assert!(values.contains_key("brake.position"));
        assert!(values.contains_key("brake.force"));
        assert!(!values.contains_key("throttle.force"));
        states += 1;
        if states == 5 {
            break;
        }
    }
    assert_eq!(states, 5);
    host.unsubscribe("pedals").unwrap();
}
