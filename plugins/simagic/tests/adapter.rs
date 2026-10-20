//! End to end through the host: fake Simagic bases on simagic-ff, and on the kernel's generic
//! driver with an older and a newer firmware.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::{ParamType, Value};

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "simagic",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-simagic")),
        name,
        &["simagic-ff"],
    )
}

fn read(hid: &Path, attr: &str) -> String {
    fs::read_to_string(hid.join(attr))
        .unwrap()
        .trim()
        .to_string()
}

/// An EVO Pro on simagic-ff, the wheel `steer` (0..65535) along its axis.
fn evo(env: &PluginEnv, steer: u32) -> PathBuf {
    let hid = env.plug(
        "1-2",
        0x3670,
        0x0502,
        "Simagic Alpha EVO Pro",
        "1-2:1.0/0003:3670:0502.0001",
        Some("simagic-ff"),
    );
    for (attr, value) in [
        ("max_angle", "900"),
        ("ff_strength", "80"),
        ("mechanical_damper", "10"),
        ("mechanical_friction", "5"),
        ("mechanical_inertia", "0"),
        ("game_centering", "100"),
        ("game_damper", "150"),
        ("game_friction", "100"),
        ("game_inertia", "100"),
        ("ring_light_enabled", "1"),
        ("fw_version", "172"),
    ] {
        fs::write(hid.join(attr), format!("{value}\n")).unwrap();
    }
    env.input(
        &hid,
        &format!("abs 0 {steer} 0 65535\nkey 288 0\nff gain autocenter\n"),
    );
    hid
}

#[test]
fn an_evo_pro_on_simagic_ff() {
    let env = env("evo");
    let hid = evo(&env, 32768);
    let mut host = env.start();
    until(&mut host, "base and rim", |h| h.components().len() == 2);
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(
        host.find(&base).unwrap().description.name,
        "Simagic Alpha EVO Pro"
    );
    assert_eq!(
        host.read_inputs(&base).unwrap().get("firmware"),
        Some(&Value::Text("v172".into()))
    );

    let spec = host.param(&base, "rotation").unwrap();
    assert!(matches!(spec.ty, ParamType::Float { min, max, .. } if min == 90.0 && max == 2520.0));
    // Game effects go up to 200%.
    let spec = host.param(&base, "damper_strength").unwrap();
    assert_eq!(spec.value, Value::Float(150.0));
    assert!(matches!(spec.ty, ParamType::Float { max, .. } if max == 200.0));

    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    host.set(&base, "ffb_strength", Value::Float(60.0)).unwrap();
    host.set(&base, "custom.ring_light_enabled", Value::Bool(false))
        .unwrap();
    assert_eq!(read(&hid, "max_angle"), "540");
    assert_eq!(read(&hid, "ff_strength"), "60");
    assert_eq!(read(&hid, "ring_light_enabled"), "0");

    // Saved for the next plug-in (development: in the driver's data), the rotation apart.
    host.install_driver("simagic").unwrap();
    host.save(&base, "ffb_strength").unwrap();
    let saved = fs::read_to_string(env.driver_data().join("settings")).unwrap();
    assert!(
        saved.contains(r#"KERNEL=="0003:3670:0502.*", ATTR{ff_strength}="60""#),
        "{saved}"
    );
    assert!(host.save(&base, "rotation").is_err());
}

#[test]
fn the_rotation_changes_only_with_the_wheel_inside_it() {
    let env = env("evo-angle");
    // 3/4 of the axis over 900°: 225° to the right.
    let hid = evo(&env, 49151);
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    let err = host
        .set(&base, "rotation", Value::Float(360.0))
        .unwrap_err()
        .to_string();
    assert!(err.contains("180°"), "{err}");
    assert_eq!(read(&hid, "max_angle"), "900");
    // Within it, fine.
    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    assert_eq!(read(&hid, "max_angle"), "540");
}

fn alpha(env: &PluginEnv, descriptor: &[u8]) -> PathBuf {
    let hid = env.plug(
        "1-3",
        0x0483,
        0x0522,
        "SIMAGIC Alpha",
        "1-3:1.0/0003:0483:0522.0002",
        Some("hid-generic"),
    );
    fs::write(hid.join("report_descriptor"), descriptor).unwrap();
    env.input(&hid, "abs 0 32768 0 65535\nff gain\n");
    hid
}

#[test]
fn an_older_firmware_on_the_kernels_driver() {
    let env = env("alpha-old");
    alpha(
        &env,
        &[
            0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x05, 0x0f, 0x09, 0x92, 0xc0,
        ],
    );
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("older than v159"));
    // Force feedback through the kernel.
    assert!(reason(&mut host, &base, "test_effect").is_empty());
}

#[test]
fn a_newer_firmware_needs_simagic_ff() {
    let env = env("alpha-new");
    alpha(
        &env,
        &[0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x06, 0x00, 0xff, 0xc0],
    );
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("simagic-ff"));
    assert!(reason(&mut host, &base, "test_effect").contains("simagic-ff"));
}

#[test]
fn a_base_not_in_the_table_is_taken_when_chosen() {
    let env = env("unknown");
    let hid = env.plug(
        "1-4",
        0x3670,
        0x0510,
        "Simagic Alpha EVO X",
        "1-4:1.0/0003:3670:0510.0001",
        Some("hid-generic"),
    );
    fs::write(
        hid.join("report_descriptor"),
        [0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x06, 0x00, 0xff, 0xc0],
    )
    .unwrap();
    env.input(&hid, "abs 0 32768 0 65535\n");
    env.plug(
        "1-5",
        0x0483,
        0x5750,
        "STM device",
        "1-5:1.0/0003:0483:5750.0002",
        Some("hid-generic"),
    );
    let mut host = env.start();
    assert!(host.components().is_empty());

    host.choose_plugin("usb:3670:0510", Some("simagic"))
        .unwrap();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(
        host.find(&base).unwrap().description.name,
        "Simagic wheel base"
    );
    // Installing simagic-ff would not help: it does not know the base.
    let why = reason(&mut host, &base, "test_effect");
    assert!(
        why.contains("does not know") && !why.contains("install"),
        "{why}"
    );

    host.choose_plugin("usb:0483:5750", Some("simagic"))
        .unwrap();
    assert!(
        host.problems()
            .iter()
            .any(|p| p.contains("not a Simagic wheel base")),
        "{:?}",
        host.problems()
    );
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
    let mut modelled: Vec<(u16, u16)> = simrig_plugin_simagic::models::MODELS
        .iter()
        .map(|m| (m.vid, m.pid))
        .collect();
    modelled.sort();
    assert_eq!(claimed, modelled);
}
