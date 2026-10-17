//! End to end through the host: a fake T300 RS as hid-tmff2 exposes it in sysfs, and a wheel
//! starting up.

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
        &["hid-tmff2"],
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
    until(&mut host, "base and rim", |h| h.components().len() == 2);
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
fn without_the_driver_everything_says_so() {
    let env = env("unbound");
    t300(&env, false);
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
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
