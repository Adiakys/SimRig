//! End to end through the host: a fake T300 RS as hid-tmff2 exposes it in sysfs, a T150 on
//! hid-t150, wheels without a force feedback driver, and a wheel starting up.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::{ParamType, Value};

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "thrustmaster",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-thrustmaster")),
        name,
        &["hid-tmff2", "hid-t150"],
    )
}

fn read(hid: &Path, attr: &str) -> String {
    fs::read_to_string(hid.join(attr))
        .unwrap()
        .trim()
        .to_string()
}

fn t300(env: &PluginEnv, bound: bool) -> PathBuf {
    let hid = env.plug(
        "1-8",
        0x044f,
        0xb66e,
        "Thrustmaster T300RS Racing wheel",
        "1-8:1.0/0003:044F:B66E.0001",
        bound.then_some("tmff2"),
    );
    if bound {
        for (attr, value) in [
            ("range", "900"),
            ("gain", "40000"),
            ("spring_level", "30"),
            ("damper_level", "30"),
            ("friction_level", "30"),
            ("alternate_modes", ""),
        ] {
            fs::write(hid.join(attr), format!("{value}\n")).unwrap();
        }
    }
    env.input(&hid, "abs 0 0 0 65535\nkey 288 0\nkey 289 1\n");
    hid
}

#[test]
fn a_t300rs_on_hid_tmff2() {
    let env = env("t300");
    let hid = t300(&env, true);
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "T300 RS");

    // Fully to the left over 900°.
    let inputs = host.read_inputs(&base).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle + 450.0).abs() < 0.1, "{angle}");

    let spec = host.param(&base, "rotation").unwrap();
    assert!(matches!(spec.ty, ParamType::Float { min, max, .. } if min == 40.0 && max == 1080.0));
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(61.0)
    );
    host.set(&base, "rotation", Value::Float(1080.0)).unwrap();
    host.set(&base, "ffb_strength", Value::Float(100.0))
        .unwrap();
    host.set(&base, "custom.friction_level", Value::Float(0.0))
        .unwrap();
    assert_eq!(read(&hid, "range"), "1080");
    assert_eq!(read(&hid, "gain"), "65535");
    assert_eq!(read(&hid, "friction_level"), "0");

    // Saved as options of the module (development: in the driver's data).
    host.install_driver("thrustmaster").unwrap();
    host.save(&base, "rotation").unwrap();
    host.save(&base, "custom.friction_level").unwrap();
    let saved = fs::read_to_string(env.driver_data().join("settings")).unwrap();
    assert_eq!(saved, "range=1080\nfriction_level=0\n");
}

#[test]
fn the_centering_spring_through_the_input_interface() {
    let env = env("t300-centering");
    let hid = t300(&env, true);
    env.input(&hid, "abs 0 0 0 65535\nff gain autocenter\n");
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    // hid-tmff2 has an attribute for the gain: nothing to mark.
    assert!(host.param(&base, "ffb_strength").unwrap().caveat.is_none());
    let d = host.find(&base).unwrap().description.clone();
    let centering = d.custom_param("autocenter").unwrap();
    assert!(centering.availability.is_supported());
    assert!(centering.caveat.is_some());
    host.set(&base, "custom.autocenter", Value::Float(50.0))
        .unwrap();
    assert!(
        fs::read_to_string(hid.join("simrig-fake-ff"))
            .unwrap()
            .contains("autocenter 32768")
    );
}

#[test]
fn a_t150_on_hid_t150() {
    let env = env("t150");
    let hid = env.plug(
        "1-5",
        0x044f,
        0xb677,
        "Thrustmaster T150 Racing Wheel",
        "1-5:1.0/0003:044F:B677.0001",
        Some("hid-t150"),
    );
    for (attr, value) in [("range", "900"), ("gain", "32768"), ("autocenter", "0")] {
        fs::write(hid.join(attr), format!("{value}\n")).unwrap();
    }
    env.input(&hid, "abs 0 32768 0 65535\nabs 5 0 0 1023\nff gain\n");
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "T150");
    let spec = host.param(&base, "rotation").unwrap();
    assert!(matches!(spec.ty, ParamType::Float { min, max, .. } if min == 270.0 && max == 1080.0));
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(50.0)
    );
    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    host.set(&base, "custom.autocenter", Value::Float(100.0))
        .unwrap();
    assert_eq!(read(&hid, "range"), "540");
    assert_eq!(read(&hid, "autocenter"), "65535");

    // Saved as a udev rule for the model (development: in the driver's data).
    host.install_driver("thrustmaster").unwrap();
    host.save(&base, "rotation").unwrap();
    let saved = fs::read_to_string(env.driver_data().join("t150-settings")).unwrap();
    assert!(
        saved.contains(r#"KERNEL=="0003:044F:B677.*", ATTR{range}="540""#),
        "{saved}"
    );
}

#[test]
fn wheels_without_a_force_feedback_driver_give_their_inputs() {
    let env = env("t500");
    let t500 = env.plug(
        "1-3",
        0x044f,
        0xb65e,
        "Thrustmaster T500 RS Racing Wheel",
        "1-3:1.0/0003:044F:B65E.0001",
        Some("hid-generic"),
    );
    env.input(&t500, "abs 0 65535 0 65535\nkey 288 1\n");
    let t80 = env.plug(
        "1-4",
        0x044f,
        0xb668,
        "Thrustmaster T80",
        "1-4:1.0/0003:044F:B668.0001",
        Some("hid-generic"),
    );
    env.input(&t80, "abs 0 0 0 1023\n");
    let mut host = env.start();
    until(&mut host, "two bases", |h| {
        h.components()
            .iter()
            .filter(|c| c.address.contains("base"))
            .count()
            == 2
    });
    let bases: Vec<(String, String)> = host
        .components()
        .iter()
        .filter(|c| c.address.contains("base"))
        .map(|c| (c.address.clone(), c.description.name.clone()))
        .collect();
    let address = |name: &str| bases.iter().find(|(_, n)| n == name).unwrap().0.clone();
    let t500 = address("T500 RS");
    let t80 = address("T80 Racing Wheel");

    let inputs = host.read_inputs(&t500).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle - 540.0).abs() < 0.1, "{angle}");
    assert!(reason(&mut host, &t500, "rotation").contains("no Linux driver"));
    assert!(reason(&mut host, &t500, "ffb_strength").contains("hid-tmff2 does not drive it"));
    assert!(reason(&mut host, &t80, "rotation").contains("fixed at 270°"));
    assert!(reason(&mut host, &t80, "ffb_strength").contains("no force feedback"));
}

#[test]
fn without_the_driver_everything_says_so() {
    let env = env("unbound");
    t300(&env, false);
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("driver to be installed"));
    assert!(reason(&mut host, &base, "angle").contains("hid-tmff2"));
}

#[test]
fn a_wheel_starting_up_has_no_components_yet() {
    let env = env("starting");
    env.plug(
        "1-9",
        0x044f,
        0xb65d,
        "Thrustmaster FFB Wheel",
        "1-9:1.0/0003:044F:B65D.0001",
        None,
    );
    let mut host = env.start();
    until(&mut host, "the device", |h| !h.usb_devices().is_empty());
    std::thread::sleep(std::time::Duration::from_millis(200));
    host.poll();
    assert!(host.components().is_empty());
}

/// The manifest claims exactly the plugin's models.
#[test]
fn manifest_matches_the_models() {
    let manifest = simrig_host::Manifest::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/plugin.toml"
    )))
    .unwrap();
    let mut claimed: Vec<u16> = manifest.matching.usb.iter().filter_map(|m| m.pid).collect();
    claimed.sort();
    let mut modelled: Vec<u16> = simrig_plugin_thrustmaster::models::MODELS
        .iter()
        .map(|m| m.pid)
        .collect();
    modelled.sort();
    assert_eq!(claimed, modelled);
}
