//! End to end through the host: fake bases on the standard force feedback protocol.

use std::fs;
use std::path::Path;

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::{Persistence, Value};

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "pidff",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-pidff")),
        name,
        &["universal-pidff"],
    )
}

#[test]
fn a_cammus_c5_with_force_feedback() {
    let env = env("c5");
    let hid = env.plug(
        "1-2",
        0x3416,
        0x0301,
        "CAMMUS C5",
        "1-2:1.0/0003:3416:0301.0001",
        Some("hid-universal-pidff"),
    );
    env.input(&hid, "abs 0 65535 0 65535\nkey 288 1\nff gain autocenter\n");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(host.find(&base).unwrap().description.name, "Cammus C5");

    // Shown over its usual rotation: the base's own cannot be read.
    let inputs = host.read_inputs(&base).unwrap();
    assert_eq!(inputs.get("angle"), Some(&Value::Float(540.0)));
    assert!(reason(&mut host, &base, "rotation").contains("own software"));

    // Strength and centering through the input interface: supported, marked.
    let spec = host.param(&base, "ffb_strength").unwrap();
    assert_eq!(spec.persistence, Persistence::Volatile);
    assert!(spec.caveat.is_some());
    host.set(&base, "ffb_strength", Value::Float(75.0)).unwrap();
    host.set(&base, "custom.autocenter", Value::Float(10.0))
        .unwrap();
    let ff = fs::read_to_string(hid.join("simrig-fake-ff")).unwrap();
    assert!(ff.contains("gain 49151"), "{ff}");
    assert!(ff.contains("autocenter 6554"), "{ff}");
    assert!(reason(&mut host, &base, "test_effect").is_empty());
}

#[test]
fn any_base_is_taken_when_chosen() {
    let env = env("unknown");
    let hid = env.plug(
        "1-4",
        0x1d50,
        0x6100,
        "DIY FFB wheel",
        "1-4:1.0/0003:1D50:6100.0001",
        Some("hid-generic"),
    );
    env.input(&hid, "abs 0 32768 0 65535\n");
    let mut host = env.start();
    assert!(host.components().is_empty());

    host.choose_plugin("usb:1d50:6100", Some("pidff")).unwrap();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(
        host.find(&base).unwrap().description.name,
        "Force feedback wheel base"
    );
    // Not in hid-universal-pidff: only the kernel's generic driver can give it forces.
    let why = reason(&mut host, &base, "test_effect");
    assert!(
        why.contains("6.15") && !why.contains("hid-universal-pidff"),
        "{why}"
    );
}

#[test]
fn without_the_module_there_is_no_force_feedback() {
    let env = env("noffb");
    let hid = env.plug(
        "1-3",
        0x2433,
        0xf301,
        "Asetek Forte",
        "1-3:1.0/0003:2433:F301.0002",
        Some("hid-generic"),
    );
    env.input(&hid, "abs 0 32767 0 65535\n");
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "test_effect").contains("hid-universal-pidff"));
    assert!(reason(&mut host, &base, "ffb_strength").contains("does not take it"));
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
    let mut modelled: Vec<(u16, u16)> = simrig_plugin_pidff::models::MODELS
        .iter()
        .map(|m| (m.vid, m.pid))
        .collect();
    modelled.sort();
    assert_eq!(claimed, modelled);
}
