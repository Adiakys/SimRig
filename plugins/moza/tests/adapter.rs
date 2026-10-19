//! End to end through the host: a fake R9 base (inputs, force feedback driver, serial port
//! answering like the real one), SR-P pedals and an HBP handbrake.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::Value;

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "moza",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-moza")),
        name,
        &["universal-pidff"],
    )
}

/// An R9 on hid-universal-pidff, whose serial port knows `settings`.
fn r9(env: &PluginEnv, settings: &str) -> PathBuf {
    let hid = env.plug(
        "1-3",
        0x346e,
        0x0002,
        "Gudsen MOZA R9 Base",
        "1-3:1.0/0003:346E:0002.0001",
        Some("hid-universal-pidff"),
    );
    fs::write(env.root.join("usb/1-3/simrig-fake-serial"), settings).unwrap();
    // Steering a quarter to the left.
    env.input(
        &hid,
        "abs 0 16383 0 65535\nabs 2 0 0 65535\nabs 5 0 0 65535\nkey 288 0\n",
    );
    hid
}

fn serial(env: &PluginEnv) -> String {
    fs::read_to_string(env.root.join("usb/1-3/simrig-fake-serial")).unwrap()
}

#[test]
fn an_r9_base_and_its_settings() {
    let env = env("r9");
    r9(
        &env,
        "limit 450\nmax-angle 450\nffb-strength 800\ndamper 10\ntorque 100\nprotection 1\n",
    );
    let mut host = env.start();
    until(&mut host, "base and its pedals", |h| {
        h.components().len() == 2
    });
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "MOZA R9 Base");

    // A quarter of a 900° rotation to the left.
    let inputs = host.read_inputs(&base).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle + 225.0).abs() < 0.1, "{angle}");

    // Settings read from the base, in the units shown.
    assert_eq!(
        host.param(&base, "rotation").unwrap().value,
        Value::Float(900.0)
    );
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(80.0)
    );
    // Written back in the base's units: the rotation as half, with the games' limit.
    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    host.set(&base, "ffb_strength", Value::Float(65.0)).unwrap();
    host.set(&base, "custom.protection", Value::Bool(false))
        .unwrap();
    let stored = serial(&env);
    for line in [
        "limit 270",
        "max-angle 270",
        "ffb-strength 650",
        "protection 0",
    ] {
        assert!(stored.lines().any(|l| l == line), "{line}: {stored}");
    }
    // A setting the base does not answer for.
    assert!(reason(&mut host, &base, "spring_strength").contains("does not answer"));
    // Centering goes to the base.
    host.run_action(&base, "set_center", Value::Empty).unwrap();
    assert!(serial(&env).contains("calibration 1"));
}

#[test]
fn without_hid_universal_pidff_there_is_no_force_feedback() {
    let env = env("noffb");
    let hid = r9(&env, "limit 450\n");
    env.bind(&hid, "hid-generic");
    let mut host = env.start();
    until(&mut host, "base and its pedals", |h| {
        h.components().len() == 2
    });
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "test_effect").contains("hid-universal-pidff"));
    // Settings and inputs still work.
    assert_eq!(
        host.param(&base, "rotation").unwrap().value,
        Value::Float(900.0)
    );
}

#[test]
fn pedals_handbrake_and_pedals_on_the_base() {
    let env = env("accessories");
    let pedals = env.plug(
        "1-4",
        0x346e,
        0x0003,
        "Gudsen MOZA SR-P Pedals",
        "1-4:1.0/0003:346E:0003.0002",
        Some("hid-generic"),
    );
    env.input(
        &pedals,
        "abs 3 32767 0 65535\nabs 4 0 0 65535\nabs 5 0 0 65535\n",
    );
    let handbrake = env.plug(
        "1-5",
        0x346e,
        0x0022,
        "HBP Handbrake",
        "1-5:1.0/0003:346E:0022.0003",
        Some("hid-generic"),
    );
    env.input(&handbrake, "abs 7 65535 0 65535\n");
    let base = r9(&env, "limit 450\n");
    let mut host = env.start();
    until(&mut host, "components", |h| h.components().len() == 4);

    // The SR-P's axes are known: no mapping.
    let srp = "moza/usb-346e-0003/pedals";
    assert_eq!(host.find(srp).unwrap().description.pedals.len(), 3);
    let inputs = host.read_inputs(srp).unwrap();
    let Some(Value::Axis(throttle)) = inputs.get("throttle.position") else {
        panic!("{inputs:?}")
    };
    assert!((throttle.value - 0.5).abs() < 0.01);
    let hb = host.find("handbrake").unwrap().address.clone();
    let inputs = host.read_inputs(&hb).unwrap();
    assert!(matches!(inputs.get("position"), Some(Value::Axis(a)) if a.value == 1.0));

    // Pedals plugged into the base are recognised too (Boxflat's table): brake on ABS_RZ.
    env.input(
        &base,
        "abs 0 32767 0 65535\nabs 2 0 0 65535\nabs 5 65535 0 65535\nabs 6 0 0 65535\n",
    );
    let on_base = "moza/usb-346e-0002/pedals";
    assert_eq!(host.find(on_base).unwrap().description.pedals.len(), 3);
    let inputs = host.read_inputs(on_base).unwrap();
    let Some(Value::Axis(brake)) = inputs.get("brake.position") else {
        panic!("{inputs:?}")
    };
    assert!(brake.value > 0.99);
}
