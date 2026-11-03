//! End to end through the host: a fake VD6 and VD4 (inputs, force feedback, a control interface
//! that answers like the real one) and a V12.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::{ParamType, Value};

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "pxn",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-pxn")),
        name,
        &["universal-pidff"],
    )
}

/// A VD base on hid-generic (force feedback from Linux 6.15), its settings block: 900°, overall
/// force 80, game friction 20, the rest 0.
fn vd(env: &PluginEnv, pid: u16, name: &str) -> PathBuf {
    let hid = env.plug(
        "1-10",
        0x36e6,
        pid,
        name,
        &format!("1-10:1.0/0003:36E6:{pid:04X}.0001"),
        Some("hid-generic"),
    );
    let mut block = [0u8; 31];
    block[..2].copy_from_slice(&900u16.to_le_bytes());
    block[2] = 80;
    block[30] = 20;
    let text: Vec<String> = block.iter().map(|b| format!("{b:02x}")).collect();
    fs::write(env.root.join("usb/1-10/simrig-fake-pxn"), text.join(" ")).unwrap();
    // A quarter turn to the right; force feedback.
    env.input(&hid, "abs 0 49151 0 65535\nkey 288 0\nff\n");
    hid
}

fn block(env: &PluginEnv) -> Vec<u8> {
    fs::read_to_string(env.root.join("usb/1-10/simrig-fake-pxn"))
        .unwrap()
        .split_whitespace()
        .map(|h| u8::from_str_radix(h, 16).unwrap())
        .collect()
}

#[test]
fn a_vd6_and_its_settings() {
    let env = env("vd6");
    vd(&env, 0x400f, "PXN-VD6");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "PXN VD6");

    let inputs = host.read_inputs(&base).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle - 225.0).abs() < 0.1, "{angle}");

    let spec = host.param(&base, "rotation").unwrap();
    assert_eq!(spec.value, Value::Float(900.0));
    assert!(matches!(spec.ty, ParamType::Float { max, .. } if max == 2700.0));
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(80.0)
    );
    host.set(&base, "rotation", Value::Float(1080.0)).unwrap();
    host.set(&base, "natural_damper", Value::Float(15.0))
        .unwrap();
    host.set(&base, "custom.ffb_reverse", Value::Bool(true))
        .unwrap();
    let b = block(&env);
    assert_eq!(u16::from_le_bytes([b[0], b[1]]), 1080);
    assert_eq!(b[10], 15, "mechanical damper");
    assert_eq!(b[13], 1, "reverse");
    // Read, change, write: what SimRig does not touch stays.
    assert_eq!((b[2], b[30]), (80, 20));

    // Force feedback through the kernel.
    assert!(reason(&mut host, &base, "test_effect").is_empty());
}

#[test]
fn a_vd4_has_settings_and_forces_from_linux_6_15() {
    let env = env("vd4");
    let hid = vd(&env, 0x400d, "PXN-VD4");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "PXN VD4");

    let spec = host.param(&base, "rotation").unwrap();
    assert_eq!(spec.value, Value::Float(900.0));
    assert!(spec.caveat.is_none());
    let reverse = host
        .describe(&base)
        .unwrap()
        .custom
        .iter()
        .find(|p| p.id == "ffb_reverse");
    assert!(reverse.unwrap().caveat.is_none());
    host.set(&base, "rotation", Value::Float(1080.0)).unwrap();
    let b = block(&env);
    assert_eq!(u16::from_le_bytes([b[0], b[1]]), 1080);

    // Not in hid-universal-pidff: an older kernel gives no force feedback.
    env.input(&hid, "abs 0 49151 0 65535\n");
    until(&mut host, "no force feedback", |h| {
        !reason(h, &base, "test_effect").is_empty()
    });
    let why = reason(&mut host, &base, "test_effect");
    assert!(
        why.contains("6.15") && !why.contains("hid-universal-pidff"),
        "{why}"
    );
}

#[test]
fn a_base_not_in_the_table_is_taken_when_chosen() {
    let env = env("unknown");
    vd(&env, 0x4011, "PXN-VD10");
    env.plug(
        "1-12",
        0x046d,
        0xc24f,
        "G29",
        "1-12:1.0/0003:046D:C24F.0002",
        Some("hid-generic"),
    );
    let mut host = env.start();
    assert!(host.components().is_empty());

    // PXN's vendor id: the plugin is offered for the model it does not list.
    let makers: Vec<Option<String>> = host
        .usb_devices()
        .iter()
        .map(|(d, _)| host.maker_plugin(d))
        .collect();
    assert_eq!(makers, [Some("pxn".into()), None]);
    let (vd10, _) = &host.usb_devices()[0];
    assert_eq!(host.automatic_plugin(vd10), None);

    host.choose_plugin("usb:36e6:4011", Some("pxn")).unwrap();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "PXN wheel base");
    assert!(host.param(&base, "rotation").unwrap().caveat.is_some());

    host.choose_plugin("usb:046d:c24f", Some("pxn")).unwrap();
    assert_eq!(host.components().len(), 2);
    assert!(
        host.problems()
            .iter()
            .any(|p| p.contains("not a PXN wheel base")),
        "{:?}",
        host.problems()
    );
}

#[test]
fn a_v12_has_inputs_and_forces_but_no_settings() {
    let env = env("v12");
    let hid = env.plug(
        "1-11",
        0x11ff,
        0x1212,
        "PXN V12",
        "1-11:1.0/0003:11FF:1212.0002",
        Some("hid-universal-pidff"),
    );
    env.input(&hid, "abs 0 32767 0 65535\n");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("VD series"));
    // Its pedals are not in a known table: mapping them is part of adding the base.
    let procedures = host.procedures(&simrig_host::Target::Device {
        plugin: "pxn".into(),
        device: "usb-11ff-1212".into(),
    });
    let map = procedures
        .unwrap()
        .into_iter()
        .find(|p| p.id == "map-pedals")
        .unwrap();
    assert!(map.setup);
    // No force feedback on the event node.
    assert!(reason(&mut host, &base, "test_effect").contains("hid-universal-pidff"));
}

#[test]
fn without_settings_the_strength_goes_through_the_input_interface() {
    let env = env("v12-gain");
    let hid = env.plug(
        "1-12",
        0x11ff,
        0x1212,
        "PXN V12",
        "1-12:1.0/0003:11FF:1212.0003",
        Some("hid-universal-pidff"),
    );
    env.input(&hid, "abs 0 32767 0 65535\nff gain\n");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    let spec = host.param(&base, "ffb_strength").unwrap();
    assert!(spec.caveat.is_some());
    host.set(&base, "ffb_strength", Value::Float(25.0)).unwrap();
    assert_eq!(
        fs::read_to_string(hid.join("simrig-fake-ff")).unwrap(),
        "gain 16384\n"
    );
    // The rest still needs the VD series' protocol.
    assert!(reason(&mut host, &base, "rotation").contains("VD series"));
}

/// The manifest claims exactly the plugin's models.
#[test]
fn manifest_matches_the_models() {
    let manifest = simrig_host::Manifest::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/plugin.toml"
    )))
    .unwrap();
    let mut claimed: Vec<(u16, u16)> = manifest
        .matching
        .usb
        .iter()
        .map(|m| (m.vid, m.pid.unwrap()))
        .collect();
    claimed.sort();
    let mut modelled: Vec<(u16, u16)> = simrig_plugin_pxn::models::MODELS
        .iter()
        .map(|m| (m.vid, m.pid))
        .collect();
    modelled.sort();
    assert_eq!(claimed, modelled);
}
