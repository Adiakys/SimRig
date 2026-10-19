//! End to end through the host: fake Logitech wheels as their drivers expose them in sysfs.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_host::testing::{PluginEnv, reason, until};
use simrig_model::{ParamType, Persistence, Value};

fn env(name: &str) -> PluginEnv {
    PluginEnv::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "logitech",
        Path::new(env!("CARGO_BIN_EXE_simrig-plugin-logitech")),
        name,
        &["new-lg4ff", "logitech-trueforce"],
    )
}

fn attrs(hid: &Path, values: &[(&str, &str)]) {
    for (attr, value) in values {
        fs::write(hid.join(attr), format!("{value}\n")).unwrap();
    }
}

fn read(hid: &Path, attr: &str) -> String {
    fs::read_to_string(hid.join(attr))
        .unwrap()
        .trim()
        .to_string()
}

/// A G29; with `new_lg4ff`, the attributes new-lg4ff adds.
fn g29(env: &PluginEnv, new_lg4ff: bool) -> PathBuf {
    let hid = env.plug(
        "1-6",
        0x046d,
        0xc24f,
        "G29 Driving Force Racing Wheel",
        "1-6:1.0/0003:046D:C24F.0001",
        Some("logitech"),
    );
    attrs(
        &hid,
        &[
            ("range", "900"),
            ("alternate_modes", ""),
            ("combine_pedals", "0"),
        ],
    );
    if new_lg4ff {
        attrs(
            &hid,
            &[
                ("gain", "65535"),
                ("autocenter", "0"),
                ("spring_level", "30"),
                ("damper_level", "30"),
                ("friction_level", "30"),
                ("ffb_leds", "0"),
            ],
        );
    }
    // Steering three quarters to the right; a button pressed.
    env.input(
        &hid,
        "abs 0 49151 0 65535\nabs 1 0 0 255\nkey 288 1\nkey 289 0\n",
    );
    hid
}

#[test]
fn a_g29_on_new_lg4ff() {
    let env = env("g29");
    let hid = g29(&env, true);
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();
    let wheel = host.find("wheel").unwrap().address.clone();
    assert_eq!(
        host.find(&base).unwrap().description.name,
        "G29 Driving Force"
    );

    let inputs = host.read_inputs(&base).unwrap();
    let Some(Value::Float(angle)) = inputs.get("angle") else {
        panic!("{inputs:?}")
    };
    assert!((angle - 225.0).abs() < 0.1, "{angle}");
    let rim = host.read_inputs(&wheel).unwrap();
    assert_eq!(rim.get("buttons"), Some(&Value::Buttons(vec![true, false])));
    // The rim's controls, as games number them.
    let controls = host.find(&wheel).unwrap().description.controls.clone();
    let names: Vec<(u32, Option<&str>)> = controls
        .iter()
        .map(|c| (c.number, c.code.as_deref()))
        .collect();
    assert_eq!(names, [(1, Some("BTN_TRIGGER")), (2, Some("BTN_THUMB"))]);
    // Named from the G29's layout.
    assert_eq!(controls[0].label.as_deref(), Some("Cross"));
    assert_eq!(controls[1].label.as_deref(), Some("Square"));

    let spec = host.param(&base, "rotation").unwrap();
    assert_eq!(spec.value, Value::Float(900.0));
    assert!(matches!(spec.ty, ParamType::Float { min, max, .. } if min == 40.0 && max == 900.0));
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(100.0)
    );
    host.set(&base, "rotation", Value::Float(540.0)).unwrap();
    host.set(&base, "ffb_strength", Value::Float(50.0)).unwrap();
    host.set(&base, "spring_strength", Value::Float(10.0))
        .unwrap();
    host.set(&base, "custom.friction_level", Value::Float(5.0))
        .unwrap();
    host.set(&base, "custom.ffb_leds", Value::Bool(true))
        .unwrap();
    assert_eq!(read(&hid, "range"), "540");
    assert_eq!(read(&hid, "gain"), "32768");
    assert_eq!(read(&hid, "spring_level"), "10");
    assert_eq!(read(&hid, "friction_level"), "5");
    assert_eq!(read(&hid, "ffb_leds"), "1");

    // Saved for the next plug-in (development: in the driver's data).
    host.install_driver("logitech").unwrap();
    host.save(&base, "rotation").unwrap();
    host.save(&base, "ffb_strength").unwrap();
    let saved = fs::read_to_string(env.driver_data().join("settings")).unwrap();
    assert!(saved.contains(r#"ATTR{range}="540""#), "{saved}");
    assert!(saved.contains(r#"ATTR{gain}="32768""#), "{saved}");

    // Logitech wheels center themselves.
    assert!(reason(&mut host, &base, "set_center").contains("by itself"));
}

#[test]
fn a_g29_on_the_kernels_driver() {
    let env = env("g29-kernel");
    g29(&env, false);
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();
    assert_eq!(
        host.param(&base, "rotation").unwrap().value,
        Value::Float(900.0)
    );
    assert!(reason(&mut host, &base, "ffb_strength").contains("new-lg4ff"));
}

#[test]
fn the_kernels_driver_takes_strength_and_centering_through_the_input_interface() {
    let env = env("g29-ff");
    let hid = g29(&env, false);
    env.input(&hid, "abs 0 32768 0 65535\nff gain autocenter\n");
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();

    // Supported, marked: the value cannot be read back.
    let spec = host.param(&base, "ffb_strength").unwrap();
    assert_eq!(spec.value, Value::Float(100.0));
    assert_eq!(spec.persistence, Persistence::Volatile);
    assert!(spec.caveat.is_some());
    host.set(&base, "ffb_strength", Value::Float(50.0)).unwrap();
    host.set(&base, "custom.autocenter", Value::Float(25.0))
        .unwrap();
    let ff = fs::read_to_string(hid.join("simrig-fake-ff")).unwrap();
    assert!(ff.contains("gain 32768"), "{ff}");
    assert!(ff.contains("autocenter 16384"), "{ff}");
    assert_eq!(
        host.param(&base, "ffb_strength").unwrap().value,
        Value::Float(50.0)
    );
    let centering = host
        .find(&base)
        .unwrap()
        .description
        .custom_param("autocenter")
        .cloned()
        .unwrap();
    assert!(centering.availability.is_supported() && centering.caveat.is_some());
    // Nothing to keep for the next plug-in.
    host.install_driver("logitech").unwrap();
    assert!(host.save(&base, "ffb_strength").is_err());
}

#[test]
fn modes_and_combined_pedals() {
    let env = env("g29-modes");
    let hid = g29(&env, true);
    attrs(
        &hid,
        &[
            (
                "alternate_modes",
                "native: G29 Driving Force Racing Wheel *\nDF-EX: Driving Force / Formula EX\n\
                 DFP: Driving Force Pro\nG29: G29 Racing Wheel *",
            ),
            ("combine_pedals", "2"),
        ],
    );
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    let d = host.find(&base).unwrap().description.clone();

    let mode = d.custom_param("mode").unwrap();
    assert_eq!(mode.value, Value::Choice("G29".into()));
    let ParamType::Choice { options } = &mode.ty else {
        panic!("{:?}", mode.ty)
    };
    assert_eq!(options.len(), 3);
    host.set(&base, "custom.mode", Value::Choice("DFP".into()))
        .unwrap();
    assert_eq!(read(&hid, "alternate_modes"), "DFP");

    let combine = d.custom_param("combine_pedals").unwrap();
    assert_eq!(combine.value, Value::Choice("2".into()));
    host.set(&base, "custom.combine_pedals", Value::Choice("1".into()))
        .unwrap();
    assert_eq!(read(&hid, "combine_pedals"), "1");
}

#[test]
fn a_wingman_formula_force_on_iforce() {
    let env = env("wff");
    // iforce is a USB driver: bound to the interface, no HID device.
    let hid = env.plug(
        "1-7",
        0x046d,
        0xc291,
        "WingMan Formula Force",
        "1-7:1.0",
        Some("iforce"),
    );
    env.input(&hid, "abs 0 512 0 1023\nkey 288 0\nff gain autocenter\n");
    let mut host = env.start();
    until(&mut host, "base", |h| h.find("base").is_ok());
    let base = host.find("base").unwrap().address.clone();
    assert!(reason(&mut host, &base, "rotation").contains("180°"));
    let spec = host.param(&base, "ffb_strength").unwrap();
    assert!(spec.caveat.is_some());
    host.set(&base, "ffb_strength", Value::Float(80.0)).unwrap();
    assert!(
        fs::read_to_string(hid.join("simrig-fake-ff"))
            .unwrap()
            .contains("gain 52428")
    );
}

#[test]
fn an_xbox_edition_in_console_mode_has_no_components_yet() {
    let env = env("xbox-console");
    env.plug(
        "1-9",
        0x046d,
        0xc26d,
        "Logitech G923 for Xbox",
        "1-9:1.0",
        Some("xpad"),
    );
    let mut host = env.start();
    until(&mut host, "the device", |h| {
        h.usb_devices()
            .iter()
            .any(|(_, owner)| owner.as_deref() == Some("logitech"))
    });
    std::thread::sleep(std::time::Duration::from_millis(200));
    host.poll();
    assert!(host.components().is_empty());
}

#[test]
fn a_g_pro_on_the_truforce_driver() {
    let env = env("gpro");
    let hid = env.plug(
        "1-7",
        0x046d,
        0xc272,
        "PRO Racing Wheel",
        "1-7:1.0/0003:046D:C272.0002",
        Some("logitech-dd"),
    );
    attrs(
        &hid,
        &[
            ("wheel_range", "1080"),
            ("wheel_strength", "80"),
            ("wheel_damping", "10"),
            ("wheel_trueforce", "50"),
            ("wheel_ffb_filter", "11"),
            ("wheel_calibrate_here", ""),
        ],
    );
    fs::write(
        hid.join("wheel_firmware"),
        "base: U1 65.03.B0038\nmotor: SC 02.01.B0042\n",
    )
    .unwrap();
    env.input(&hid, "abs 0 32767 0 65535\n");
    let mut host = env.start();
    until(&mut host, "base, rim, pedals", |h| {
        h.components().len() == 3
    });
    let base = host.find("base").unwrap().address.clone();

    let inputs = host.read_inputs(&base).unwrap();
    assert_eq!(
        inputs.get("firmware"),
        Some(&Value::Text("U1 65.03.B0038".into()))
    );
    assert_eq!(
        host.param(&base, "rotation").unwrap().value,
        Value::Float(1080.0)
    );
    host.set(&base, "natural_damper", Value::Float(25.0))
        .unwrap();
    host.set(&base, "custom.trueforce", Value::Float(0.0))
        .unwrap();
    host.set(&base, "custom.ffb_filter", Value::Float(15.0))
        .unwrap();
    assert_eq!(read(&hid, "wheel_damping"), "25");
    assert_eq!(read(&hid, "wheel_trueforce"), "0");
    assert_eq!(read(&hid, "wheel_ffb_filter"), "15");
    host.run_action(&base, "set_center", Value::Empty).unwrap();
    assert_eq!(read(&hid, "wheel_calibrate_here"), "1");
    // What only the G29 family has.
    assert!(!reason(&mut host, &base, "spring_strength").is_empty());
}

/// The manifest claims exactly the wheels the plugin models.
#[test]
fn manifest_matches_the_models() {
    let manifest = simrig_host::Manifest::load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/plugin.toml"
    )))
    .unwrap();
    let mut claimed: Vec<u16> = manifest.matching.usb.iter().filter_map(|m| m.pid).collect();
    claimed.sort();
    let mut modelled: Vec<u16> = simrig_plugin_logitech::models::MODELS
        .iter()
        .map(|m| m.pid)
        .collect();
    modelled.sort();
    assert_eq!(claimed, modelled);
}
