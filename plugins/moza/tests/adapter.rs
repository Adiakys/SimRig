//! End to end through the host: a fake R9 base (inputs, force feedback driver, serial port
//! answering like the real one) and the rim mounted on it, SR-P pedals, an HBP handbrake, SGP
//! and HGP shifters on their own USB and plugged into the base.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::Target;
use simrig_host::testing::{PluginEnv, reason, step, until};
use simrig_model::{Gear, ProcedureState, Support, Value};

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

/// Waits for what a base gives with nothing moved: itself, its rim and its pedals.
fn base_ready(host: &mut simrig_host::Host) {
    until(host, "base, its rim and its pedals", |h| {
        h.components().len() == 3
    });
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
    base_ready(&mut host);
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
fn the_rim_on_the_base_and_its_settings() {
    let env = env("rim");
    let base = r9(
        &env,
        "limit 450\nwheel-telemetry-mode 1\nwheel-paddles-mode 2\nwheel-clutch-point 50\n\
         wheel-stick-mode 256\nwheel-rpm-brightness 80\n",
    );
    // Left paddle at rest, right one pulled, the first button pressed.
    env.input(
        &base,
        "abs 0 32767 0 65535\nabs 3 65535 0 65535\nabs 4 0 0 65535\nkey 288 1\n",
    );
    let mut host = env.start();
    base_ready(&mut host);
    let wheel = "moza/usb-346e-0002/wheel";
    let description = host.find(wheel).unwrap().description.clone();
    assert_eq!(description.name, "Steering wheel on the MOZA R9 Base");
    assert!(
        description
            .controls
            .iter()
            .any(|c| c.label.as_deref() == Some("Left paddle"))
    );

    // Its controls come through the base.
    let inputs = host.read_inputs(wheel).unwrap();
    assert_eq!(
        inputs.get("analog_paddles"),
        Some(&Value::Floats(vec![0.0, 1.0]))
    );
    assert!(matches!(inputs.get("buttons"), Some(Value::Buttons(b)) if b[0]));

    // Its settings, read from the rim and written to it in its units.
    assert_eq!(
        host.param(wheel, "analog_paddle_mode").unwrap().value,
        Value::Choice("1".into())
    );
    assert_eq!(
        host.param(wheel, "led_brightness").unwrap().value,
        Value::Float(80.0)
    );
    let described = host.describe(wheel).unwrap();
    let stick = described.custom_param("wheel_stick_mode").unwrap();
    assert_eq!(stick.value, Value::Choice("1".into()));
    host.set(wheel, "analog_paddle_mode", Value::Choice("2".into()))
        .unwrap();
    host.set(wheel, "led_brightness", Value::Float(40.0))
        .unwrap();
    host.set(wheel, "custom.wheel_stick_mode", Value::Choice("0".into()))
        .unwrap();
    host.set(wheel, "custom.wheel_clutch_point", Value::Float(35.0))
        .unwrap();
    let stored = serial(&env);
    for line in [
        "wheel-paddles-mode 3",
        "wheel-rpm-brightness 40",
        "wheel-stick-mode 0",
        "wheel-clutch-point 35",
    ] {
        assert!(stored.lines().any(|l| l == line), "{line}: {stored}");
    }
    // A setting this rim does not answer for, and one no command is known for.
    let described = host.describe(wheel).unwrap();
    let knob = &described
        .custom_param("wheel_knob_mode")
        .unwrap()
        .availability;
    assert!(matches!(knob, Support::Unsupported(u) if u.explain().contains("does not answer")));
    assert!(reason(&mut host, wheel, "vibration").contains("No serial command"));
}

#[test]
fn without_a_rim_answering_its_settings_are_unavailable() {
    let env = env("no-rim");
    r9(&env, "limit 450\n");
    let mut host = env.start();
    base_ready(&mut host);
    let wheel = "moza/usb-346e-0002/wheel";
    assert!(reason(&mut host, wheel, "analog_paddle_mode").contains("No steering wheel answers"));
    assert!(
        host.set(wheel, "led_brightness", Value::Float(40.0))
            .is_err()
    );
    // The base's own settings still work.
    assert_eq!(
        host.param("moza/usb-346e-0002/base", "rotation")
            .unwrap()
            .value,
        Value::Float(900.0)
    );
}

#[test]
fn without_hid_universal_pidff_there_is_no_force_feedback() {
    let env = env("noffb");
    let hid = r9(&env, "limit 450\n");
    env.bind(&hid, "hid-generic");
    let mut host = env.start();
    base_ready(&mut host);
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
    until(&mut host, "components", |h| h.components().len() == 5);

    // The SR-P's axes are known: no mapping, unless by hand.
    let srp = "moza/usb-346e-0003/pedals";
    assert_eq!(host.find(srp).unwrap().description.pedals.len(), 3);
    let procedures = host.procedures(&Target::Component(srp.into())).unwrap();
    assert_eq!(procedures[0].label, "Map the pedals by hand");
    assert!(!procedures[0].setup);
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

/// The 16 joystick buttons of a device of its own (BTN_TRIGGER to BTN_DEAD), the ones in
/// `pressed` held.
fn buttons(pressed: &[u16]) -> String {
    keys(0x120..=0x12f, pressed)
}

/// A base's 128 buttons as hid-universal-pidff maps them: 16 from BTN_JOYSTICK, the rest from
/// KEY_NEXT_FAVORITE.
fn base_buttons(pressed: &[u16]) -> String {
    format!(
        "abs 0 32767 0 65535\n{}",
        keys((0x120..=0x12f).chain(0x270..=0x2df), pressed)
    )
}

fn keys(codes: impl Iterator<Item = u16>, pressed: &[u16]) -> String {
    codes
        .map(|code| format!("key {code} {}\n", u8::from(pressed.contains(&code))))
        .collect()
}

/// Runs a "Map the shifter" procedure: each button of `presses` pressed in turn, then the step
/// that follows waited for.
fn map_shifter(
    host: &mut simrig_host::Host,
    env: &PluginEnv,
    hid: &Path,
    input: fn(&[u16]) -> String,
    target: &Target,
    id: &str,
    presses: &[(u16, &str)],
) {
    let (session, _) = host.start_procedure(target, id).unwrap();
    env.input(hid, &input(&[]));
    host.submit_procedure(&session, "next", BTreeMap::new())
        .unwrap();
    for (code, next) in presses {
        env.input(hid, &input(&[*code]));
        step(host, next);
    }
    env.input(hid, &input(&[]));
    let state = host
        .submit_procedure(&session, "save", BTreeMap::new())
        .unwrap();
    assert!(
        matches!(state, ProcedureState::Finished { .. }),
        "{state:?}"
    );
}

fn gear(host: &mut simrig_host::Host, shifter: &str) -> Option<Value> {
    host.read_inputs(shifter).unwrap().get("gear").cloned()
}

#[test]
fn an_sgp_sequential_shifter() {
    let env = env("sgp");
    let sgp = env.plug(
        "1-6",
        0x346e,
        0x0023,
        "Gudsen SGP Shifter",
        "1-6:1.0/0003:346E:0023.0004",
        Some("hid-generic"),
    );
    // Pushed: its 13th button.
    env.input(&sgp, &buttons(&[0x12c]));
    let mut host = env.start();
    until(&mut host, "shifter", |h| h.components().len() == 1);

    let shifter = "moza/usb-346e-0023/shifter";
    assert_eq!(
        host.find(shifter).unwrap().description.name,
        "MOZA SGP Shifter"
    );
    assert_eq!(
        host.param(shifter, "mode").unwrap().value,
        Value::Choice("sequential".into())
    );
    let inputs = host.read_inputs(shifter).unwrap();
    assert_eq!(
        inputs.get("sequential"),
        Some(&Value::Buttons(vec![true, false]))
    );
    assert!(reason(&mut host, shifter, "gear").contains("no gear positions"));

    // Recognised, mapped by hand only to put it right: here, the direction reversed in the
    // shifter's own settings.
    let target = Target::Component(shifter.into());
    let procedures = host.procedures(&target).unwrap();
    assert_eq!(procedures.len(), 1);
    assert_eq!(procedures[0].label, "Map the shifter by hand");
    assert!(!procedures[0].setup);
    map_shifter(
        &mut host,
        &env,
        &sgp,
        buttons,
        &target,
        "map-sequential-shifter",
        &[(0x12d, "Shift down"), (0x12c, "Check the mapping")],
    );
    let saved = fs::read_to_string(env.config()).unwrap();
    assert!(
        saved.contains("[shifters.\"moza/sgp-shifter\"]\n\"+\" = 301\n- = 300"),
        "{saved}"
    );
    env.input(&sgp, &buttons(&[0x12d]));
    let inputs = host.read_inputs(shifter).unwrap();
    assert_eq!(
        inputs.get("sequential"),
        Some(&Value::Buttons(vec![true, false]))
    );
}

#[test]
fn an_hgp_h_pattern_shifter() {
    let env = env("hgp");
    let hgp = env.plug(
        "1-7",
        0x346e,
        0x0021,
        "HGP Shifter",
        "1-7:1.0/0003:346E:0021.0005",
        Some("hid-generic"),
    );
    // Second gear: its 7th button.
    env.input(&hgp, &buttons(&[0x126]));
    let mut host = env.start();
    until(&mut host, "shifter", |h| h.components().len() == 1);

    let shifter = "moza/usb-346e-0021/shifter";
    assert_eq!(
        host.find(shifter).unwrap().description.name,
        "MOZA HGP Shifter"
    );
    assert_eq!(
        host.param(shifter, "mode").unwrap().value,
        Value::Choice("h_pattern".into())
    );
    assert_eq!(host.param(shifter, "gears").unwrap().value, Value::Int(7));
    assert_eq!(
        gear(&mut host, shifter),
        Some(Value::Gear(Gear::Forward(2)))
    );
    // Reverse on its 5th button, neutral on none.
    env.input(&hgp, &buttons(&[0x124]));
    assert_eq!(gear(&mut host, shifter), Some(Value::Gear(Gear::Reverse)));
    env.input(&hgp, &buttons(&[]));
    assert_eq!(gear(&mut host, shifter), Some(Value::Gear(Gear::Neutral)));
    assert!(reason(&mut host, shifter, "sequential").contains("no sequential mode"));
    let procedures = host.procedures(&Target::Component(shifter.into())).unwrap();
    assert_eq!(procedures[0].id, "map-h-shifter");
    assert_eq!(procedures[0].label, "Map the shifter by hand");
}

#[test]
fn an_hgp_plugged_into_a_base_shows_itself_when_a_gear_is_engaged() {
    let env = env("base-hgp");
    let base = r9(&env, "limit 450\n");
    env.input(&base, &base_buttons(&[]));
    let mut host = env.start();
    base_ready(&mut host);

    // First gear, the base's button 114, is the SGP's shift down too: taken for an SGP.
    env.input(&base, &base_buttons(&[0x2d1]));
    until(&mut host, "the shifter", |h| h.components().len() == 4);
    let shifter = "moza/usb-346e-0002/shifter";
    assert_eq!(
        host.param(shifter, "mode").unwrap().value,
        Value::Choice("sequential".into())
    );
    // Third gear, button 116, only the HGP has.
    env.input(&base, &base_buttons(&[0x2d3]));
    until(&mut host, "an H-pattern shifter", |h| {
        h.param(shifter, "mode")
            .is_ok_and(|p| p.value == Value::Choice("h_pattern".into()))
    });
    assert_eq!(
        gear(&mut host, shifter),
        Some(Value::Gear(Gear::Forward(3)))
    );
    // Back in neutral, it stays.
    env.input(&base, &base_buttons(&[]));
    assert_eq!(gear(&mut host, shifter), Some(Value::Gear(Gear::Neutral)));
    assert_eq!(host.components().len(), 4);

    let device = Target::Device {
        plugin: "moza".into(),
        device: "usb-346e-0002".into(),
    };
    let procedures = host.procedures(&device).unwrap();
    let map = procedures.iter().find(|p| p.id == "map-h-shifter").unwrap();
    assert_eq!(map.label, "Map an H-pattern shifter connected to the base");
    assert!(!map.setup);
}

#[test]
fn an_sgp_plugged_into_a_base_shows_itself_when_shifted() {
    let env = env("base-sgp");
    let base = r9(&env, "limit 450\n");
    env.input(&base, &base_buttons(&[]));
    let mut host = env.start();
    base_ready(&mut host);

    // Shifted up: the base's button 113.
    env.input(&base, &base_buttons(&[0x2d0]));
    until(&mut host, "the shifter", |h| h.components().len() == 4);
    let shifter = "moza/usb-346e-0002/shifter";
    assert_eq!(
        host.find(shifter).unwrap().description.name,
        "Shifter on the MOZA R9 Base"
    );
    let inputs = host.read_inputs(shifter).unwrap();
    assert_eq!(
        inputs.get("sequential"),
        Some(&Value::Buttons(vec![true, false]))
    );
}

#[test]
fn with_paddle_sync_an_sgp_on_a_base_is_mapped_by_hand() {
    let env = env("base-sgp-paddles");
    let base = r9(&env, "limit 450\n");
    env.input(&base, &base_buttons(&[]));
    let mut host = env.start();
    base_ready(&mut host);

    // Its buttons are the paddles' (13 and 14), which do not say it is there: mapped by hand,
    // offered, never required.
    let device = Target::Device {
        plugin: "moza".into(),
        device: "usb-346e-0002".into(),
    };
    let procedures = host.procedures(&device).unwrap();
    let map = procedures
        .iter()
        .find(|p| p.id == "map-sequential-shifter")
        .unwrap();
    assert_eq!(map.label, "Map a sequential shifter connected to the base");
    assert!(!map.setup);

    map_shifter(
        &mut host,
        &env,
        &base,
        base_buttons,
        &device,
        "map-sequential-shifter",
        &[(0x12d, "Shift down"), (0x12c, "Check the mapping")],
    );
    until(&mut host, "the shifter", |h| h.components().len() == 4);
    let shifter = "moza/usb-346e-0002/shifter";
    assert_eq!(
        host.find(shifter).unwrap().description.name,
        "Shifter on the MOZA R9 Base"
    );
    env.input(&base, &base_buttons(&[0x12c]));
    let inputs = host.read_inputs(shifter).unwrap();
    assert_eq!(
        inputs.get("sequential"),
        Some(&Value::Buttons(vec![false, true]))
    );
}
