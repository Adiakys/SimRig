//! End-to-end: the host starts the simulated rig as an external plugin process and talks to
//! it through the protocol, exactly as the app does. Every test uses its own temporary
//! configuration, state and sysfs directories.

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::{Env, USB_PLUGIN, poll_until, reason};
use simrig_host::{ApplyOutcome, Host, HostError, HostEvent, Image, PluginState, Preset};
use simrig_model::{ComponentKind, ItemSpec, Reason, Support, Value};

#[test]
fn starts_a_conforming_rig() {
    let env = Env::new("start");
    let host = env.start();
    assert!(
        host.problems().is_empty(),
        "problems: {:?}",
        host.problems()
    );
    assert_eq!(host.plugins()[0].state, PluginState::Running);
    let kinds: Vec<ComponentKind> = host
        .components()
        .iter()
        .map(|c| c.description.kind)
        .collect();
    assert_eq!(kinds, ComponentKind::ALL.to_vec());
    assert_eq!(host.find("base").unwrap().address, "sim/rig0/base");
    assert_eq!(host.catalog().models().len(), 7);
    // No image shipped: the app draws the generic one of the type.
    assert_eq!(
        host.image("base").unwrap(),
        Image::Generic {
            kind: ComponentKind::WheelBase,
            rim: None
        }
    );
    // The plugin tells the rim shape: the core picks the matching drawing.
    assert_eq!(
        host.image("wheel").unwrap(),
        Image::Generic {
            kind: ComponentKind::SteeringWheel,
            rim: Some(simrig_model::RimShape::Formula)
        }
    );
}

#[test]
fn standard_parameters_are_validated() {
    let env = Env::new("params");
    let mut host = env.start();
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
    assert!(
        host.set("pedals", "brake.calibration_min", Value::Int(65535))
            .is_err()
    );
}

#[test]
fn the_rim_lists_its_controls() {
    let env = Env::new("controls");
    let mut host = env.start();
    // The hat's live value comes with the other inputs.
    let rim = &host.find("wheel").unwrap().description;
    let kinds = |k: simrig_model::ControlKind| rim.controls.iter().filter(|c| c.kind == k).count();
    assert_eq!(kinds(simrig_model::ControlKind::Button), 12);
    assert_eq!(kinds(simrig_model::ControlKind::Hat), 1);
    assert_eq!(kinds(simrig_model::ControlKind::Encoder), 2);
    assert_eq!(kinds(simrig_model::ControlKind::Axis), 2);
    let address = host.find("wheel").unwrap().address.clone();
    assert!(matches!(
        host.read_inputs(&address).unwrap().get("hats"),
        Some(Value::Ints(h)) if h.len() == 1
    ));
}

#[test]
fn unsupported_items_carry_every_kind_of_reason() {
    let env = Env::new("reasons");
    let host = env.start();
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
    let env = Env::new("depends");
    let mut host = env.start();
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
    assert_eq!(reason(&host, "shifter", "sequential"), None);
    let inputs = host.read_inputs("shifter").unwrap();
    assert!(inputs.contains_key("sequential") && !inputs.contains_key("gear"));

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
fn actions_and_outputs_need_no_privileges() {
    let env = Env::new("actions");
    let mut host = env.start();
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
    let env = Env::new("inputs");
    let mut host = env.start();
    host.subscribe("pedals", 50.0).unwrap();
    for _ in 0..5 {
        let values = host
            .next_input_state("pedals", Duration::from_secs(2))
            .unwrap()
            .expect("input state");
        assert!(values.contains_key("brake.position") && values.contains_key("brake.force"));
        assert!(!values.contains_key("throttle.force"));
    }
    // Through poll, as the GUI gets them.
    poll_until(
        &mut host,
        "inputs",
        |e| matches!(e, HostEvent::Inputs { address, .. } if address == "sim/pedals0/pedals"),
    );
    host.unsubscribe("pedals").unwrap();
}

#[test]
fn rim_swap_changes_the_components() {
    let env = Env::new("rim");
    let mut host = env.start();
    assert_eq!(
        host.find("wheel").unwrap().description.model.as_deref(),
        Some("sim/formula-rim")
    );

    host.set("base", "custom.rim", Value::Choice("gt".into()))
        .unwrap();
    poll_until(
        &mut host,
        "rim swap",
        |e| matches!(e, HostEvent::ComponentsChanged { device, .. } if device == "rig0"),
    );
    assert_eq!(
        host.find("wheel").unwrap().description.model.as_deref(),
        Some("sim/gt-rim")
    );
    assert_eq!(reason(&host, "wheel", "flag_leds"), None);
    assert_eq!(
        reason(&host, "wheel", "display"),
        Some(Reason::NotInHardware)
    );
    host.write_output("wheel", "flag_leds", Value::Choice("yellow".into()))
        .unwrap();

    host.set("base", "custom.rim", Value::Choice("none".into()))
        .unwrap();
    poll_until(&mut host, "rim removed", |e| {
        matches!(e, HostEvent::ComponentsChanged { .. })
    });
    assert!(host.find("wheel").is_err());
    assert!(host.find("base").is_ok());
}

#[test]
fn virtual_devices_come_and_go() {
    let env = Env::new("virtual");
    let mut host = env.start();
    host.set("base", "custom.handbrake_connected", Value::Bool(false))
        .unwrap();
    poll_until(
        &mut host,
        "handbrake unplugged",
        |e| matches!(e, HostEvent::DeviceRemoved { device, .. } if device == "hb0"),
    );
    assert!(host.find("handbrake").is_err());

    host.set("base", "custom.handbrake_connected", Value::Bool(true))
        .unwrap();
    poll_until(
        &mut host,
        "handbrake plugged",
        |e| matches!(e, HostEvent::DeviceAdded { device, .. } if device.id == "hb0"),
    );
    assert!(host.find("handbrake").is_ok());
}

#[test]
fn drivers_install_save_and_uninstall_without_leftovers() {
    let env = Env::new("driver");
    let mut host = env.start();
    let status = host.driver_status("sim").unwrap().unwrap();
    assert!(!status.installed && status.user_access == Some(true));

    // Live changes never need the driver or privileges; saving needs the driver.
    host.set("base", "rotation", Value::Float(540.0)).unwrap();
    match host.save("base", "rotation") {
        Err(HostError::Unsupported(u)) => assert_eq!(u.reason, Reason::NeedsDriver),
        other => panic!("expected NeedsDriver, got {other:?}"),
    }

    host.install_driver("sim").unwrap();
    assert!(host.driver_status("sim").unwrap().unwrap().installed);
    assert_eq!(host.installed_drivers().len(), 1);
    // The adapter was restarted: the live value is back to its default.
    assert_eq!(
        host.param("base", "rotation").unwrap().value,
        Value::Float(900.0)
    );
    host.set("base", "rotation", Value::Float(540.0)).unwrap();
    host.save("base", "rotation").unwrap();
    // A volatile value cannot be saved.
    assert!(matches!(
        host.save("base", "custom.noise"),
        Err(HostError::InvalidValue(_))
    ));
    drop(host);

    // The driver remembers it: a new session (the app was closed) starts with it.
    let mut host = env.start();
    assert_eq!(
        host.param("base", "rotation").unwrap().value,
        Value::Float(540.0)
    );

    host.uninstall_driver("sim").unwrap();
    assert!(host.installed_drivers().is_empty());
    assert!(!env.root.join("state").exists(), "driver state left behind");
    assert_eq!(
        host.param("base", "rotation").unwrap().value,
        Value::Float(900.0)
    );
}

#[test]
fn a_driver_of_another_version_or_revision_can_be_updated() {
    let env = Env::new("driver-update");
    let mut host = env.start();
    // Not installed: nothing to update.
    assert_eq!(host.driver_update("sim").unwrap(), None);
    host.install_driver("sim").unwrap();
    assert_eq!(host.driver_update("sim").unwrap(), None);

    // Files of the installed copy that differ do not count: only the version and SimRig's
    // revision do (a rebuilt service, a recipe used only at install time…).
    let hook = env.root.join("state/drivers/sim/driver/hooks/install");
    let old = fs::read_to_string(&hook).unwrap();
    fs::write(&hook, format!("{old}\n# older\n")).unwrap();
    assert_eq!(host.driver_update("sim").unwrap(), None);

    // Installed before a new version, then before a new revision of it.
    let registry = env.root.join("state/installed.toml");
    let text = fs::read_to_string(&registry).unwrap();
    fs::write(
        &registry,
        text.replace("version = \"1.0\"", "version = \"0.9\""),
    )
    .unwrap();
    let update = host.driver_update("sim").unwrap().unwrap();
    assert_eq!(
        (update.installed.as_str(), update.available.as_str()),
        ("0.9", "1.0")
    );
    fs::write(&registry, text.replace("revision = 0", "revision = 3")).unwrap();
    let update = host.driver_update("sim").unwrap().unwrap();
    assert_eq!(
        (update.installed.as_str(), update.available.as_str()),
        ("1.0 r3", "1.0")
    );

    // Installing again is the update.
    host.install_driver("sim").unwrap();
    assert_eq!(host.driver_update("sim").unwrap(), None);
    assert_eq!(fs::read_to_string(&hook).unwrap(), old);
    assert_eq!(host.installed_drivers().len(), 1);
}

#[test]
fn usb_hotplug_with_mixed_brands() {
    let env = Env::new("usb");
    env.add_usb_plugin();
    let mut host = env.start();
    let usb_plugin = |host: &Host| {
        host.plugins()
            .iter()
            .find(|p| p.manifest.id == USB_PLUGIN)
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(usb_plugin(&host), PluginState::Idle);

    env.plug_usb("3-1");
    poll_until(
        &mut host,
        "usb pedals",
        |e| matches!(e, HostEvent::DeviceAdded { plugin, .. } if plugin == USB_PLUGIN),
    );
    assert_eq!(usb_plugin(&host), PluginState::Running);
    // Two pedal sets from two "brands": addressed by plugin, never by type alone.
    assert!(host.find("pedals").is_err());
    let usb_pedals = "sim-usb/usb-fffe-0001/pedals";
    assert_eq!(
        host.find(usb_pedals).unwrap().description.model.as_deref(),
        Some("sim/usb-pedals")
    );

    let preset = Preset::parse(
        &format!("name = 'Mixed'\n[pedals]\n'brake.deadzone_low' = 5\n[component.'{usb_pedals}']\n'brake.deadzone_low' = 8\n"),
        Path::new("mixed.toml"),
    )
    .unwrap();
    let report = host.apply_preset(&preset);
    assert_eq!(report.len(), 2, "{report:?}");
    assert_eq!(
        host.param("sim/pedals0/pedals", "brake.deadzone_low")
            .unwrap()
            .value,
        Value::Float(5.0)
    );
    assert_eq!(
        host.param(usb_pedals, "brake.deadzone_low").unwrap().value,
        Value::Float(8.0)
    );

    env.unplug_usb("3-1");
    let events = poll_until(&mut host, "usb plugin stopped", |e| {
        matches!(e, HostEvent::PluginStopped { .. })
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HostEvent::DeviceRemoved { plugin, .. } if plugin == USB_PLUGIN))
    );
    assert_eq!(usb_plugin(&host), PluginState::Idle);
    assert!(host.find(usb_pedals).is_err());
}

/// Choices about a device follow it to another port: its address does not hold the port.
#[test]
fn a_device_moved_to_another_port_keeps_its_address() {
    let env = Env::new("usb-port");
    env.add_usb_plugin();
    let mut host = env.start();
    let usb_pedals = "sim-usb/usb-fffe-0001/pedals";
    env.plug_usb("3-1");
    poll_until(
        &mut host,
        "usb pedals",
        |e| matches!(e, HostEvent::DeviceAdded { plugin, .. } if plugin == USB_PLUGIN),
    );
    host.set_excluded(usb_pedals, true).unwrap();

    env.unplug_usb("3-1");
    poll_until(&mut host, "unplugged", |e| {
        matches!(e, HostEvent::DeviceRemoved { .. })
    });
    env.plug_usb("2-4");
    poll_until(
        &mut host,
        "usb pedals again",
        |e| matches!(e, HostEvent::DeviceAdded { plugin, .. } if plugin == USB_PLUGIN),
    );
    assert!(host.find(usb_pedals).unwrap().excluded);
}

/// A rig saved when addresses held the port: its choices move to the device's id.
#[test]
fn addresses_with_the_port_are_migrated() {
    let env = Env::new("usb-legacy");
    env.add_usb_plugin();
    fs::create_dir_all(env.root.join("config")).unwrap();
    fs::write(
        env.root.join("config/rig.toml"),
        "excluded = [\"sim-usb/usb-3-1/pedals\"]\n",
    )
    .unwrap();
    env.plug_usb("3-1");
    let mut host = env.start();
    let address = "sim-usb/usb-fffe-0001/pedals";
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while host.find(address).is_err() && std::time::Instant::now() < deadline {
        host.poll();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(host.find(address).unwrap().excluded);
    let rig = fs::read_to_string(env.root.join("config/rig.toml")).unwrap();
    assert!(rig.contains("sim-usb/usb-fffe-0001/pedals"), "{rig}");
    assert!(!rig.contains("usb-3-1"), "{rig}");
}

#[test]
fn excluded_components_are_remembered() {
    let env = Env::new("exclude");
    let mut host = env.start();
    host.set_excluded("handbrake", true).unwrap();
    drop(host);
    let mut host = env.start();
    assert!(host.find("handbrake").unwrap().excluded);
    let preset = host.capture_preset("Without handbrake").unwrap();
    assert!(!preset.kinds.contains_key(&ComponentKind::Handbrake));
    host.set_excluded("handbrake", false).unwrap();
    assert!(!host.find("handbrake").unwrap().excluded);
}

#[test]
fn presets_capture_apply_and_retry_dependencies() {
    let env = Env::new("presets");
    let mut host = env.start();
    host.install_driver("sim").unwrap();
    host.set("base", "rotation", Value::Float(540.0)).unwrap();
    host.set("shifter", "gears", Value::Int(5)).unwrap();
    let preset = host.capture_preset("GT3").unwrap();
    assert_eq!(
        preset.kinds[&ComponentKind::WheelBase]["rotation"],
        toml::Value::Float(540.0)
    );
    host.presets().save(&preset).unwrap();

    // Another session, with the shifter in sequential mode: `gears` depends on the mode,
    // which the preset sets back to H-pattern, so it is applied on the second pass.
    drop(host);
    let mut host = env.start();
    host.set("shifter", "mode", Value::Choice("sequential".into()))
        .unwrap();
    let preset = host.presets().get("gt3").unwrap();
    let report = host.apply_preset(&preset);
    let outcome = |item: &str| {
        report
            .iter()
            .find(|e| e.item == item)
            .map(|e| e.outcome.clone())
    };
    assert_eq!(outcome("rotation"), Some(ApplyOutcome::Saved));
    assert_eq!(outcome("gears"), Some(ApplyOutcome::Saved));
    assert_eq!(host.param("shifter", "gears").unwrap().value, Value::Int(5));
    assert_eq!(host.presets().list().unwrap().len(), 1);

    // Export, delete, import.
    let file = env.root.join("gt3-export.toml");
    host.presets().export("GT3", &file).unwrap();
    host.presets().delete("GT3").unwrap();
    assert!(host.presets().list().unwrap().is_empty());
    host.presets().import(&file, false).unwrap();
    assert!(host.presets().import(&file, false).is_err());
    host.uninstall_driver("sim").unwrap();
}
