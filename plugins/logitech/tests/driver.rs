//! The Logitech driver's hooks, run for real in a sandbox with stub system commands. The sources
//! are the bundled new-lg4ff and TrueForce driver.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-logitech.rules";
const SETTINGS: &str = "etc/udev/rules.d/72-simrig-logitech-settings.rules";

fn sandbox(name: &str, root_user: bool) -> Sandbox {
    let sb = Sandbox::new(Path::new(DRIVER), "logitech", name, root_user);
    // The kernel's own hid-logitech, as on every distribution.
    sb.module("hid-logitech", "", true);
    sb
}

fn version(name: &str) -> String {
    fs::read_to_string(Path::new(DRIVER).join(format!("{name}.version")))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_g920_needs_only_the_rules() {
    let sb = sandbox("g920", true);
    sb.plug(0x046d, 0xc262, Some("logitech-hidpp-device"));
    let status = sb.status();
    assert!(!status.installed);
    assert!(!status.message.unwrap().contains("builds"));
    sb.install();
    assert!(!sb.log().contains("dkms add"), "{}", sb.log());
    assert!(sb.read(RULES).contains("logitech-hidpp-device"));
    let status = sb.status();
    assert!(status.installed && status.version.is_none(), "{status:?}");
}

#[test]
fn a_g29_gets_new_lg4ff() {
    let sb = sandbox("g29", true);
    let v = version("new-lg4ff");
    sb.plug(0x046d, 0xc24f, Some("hid-generic"));
    sb.hid_driver("logitech");
    assert!(sb.status().message.unwrap().contains("new-lg4ff"));

    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("new-lg4ff {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    // The kernel's hid-logitech made way for new-lg4ff's.
    let log = sb.log();
    assert!(log.contains("modprobe -r hid_logitech"), "{log}");
    assert!(!sb.dkms().contains("trueforce"));
    // The wheel started again with it, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-c24f/authorized"), "1\n");
    let status = sb.status();
    assert!(status.installed, "{status:?}");
    assert_eq!(status.version, Some(format!("new-lg4ff {v}")));

    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(!sb.dkms().contains("new-lg4ff"), "{}", sb.dkms());
    assert!(!sb.file(RULES).exists());
}

#[test]
fn a_direct_drive_plugged_in_later_asks_for_the_truforce_driver() {
    let sb = sandbox("gpro", true);
    let v = version("logitech-trueforce");
    sb.install();
    assert!(sb.status().installed);

    sb.plug(0x046d, 0xc272, None);
    sb.hid_driver("logitech-dd");
    let status = sb.status();
    assert!(!status.installed, "{status:?}");
    assert!(status.message.unwrap().contains("TrueForce"));

    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("logitech-trueforce {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    // Upstream's fixed version became the bundled one.
    let conf = sb.read(&format!("usr/src/logitech-trueforce-{v}/dkms.conf"));
    assert!(conf.contains(&format!("PACKAGE_VERSION=\"{v}\"")), "{conf}");
    assert!(sb.status().installed);
}

#[test]
fn new_lg4ff_installed_by_the_user_is_kept() {
    let sb = sandbox("foreign", true);
    sb.plug(0x046d, 0xc24f, Some("logitech"));
    fs::write(
        sb.file("dkms-state"),
        format!("new-lg4ff 0.4.1 {KERNEL} installed\n"),
    )
    .unwrap();
    let out = sb.install();
    assert!(out.contains("not by SimRig"), "{out}");
    assert!(!sb.log().contains("dkms add"));
    assert!(sb.status().installed);
}

#[test]
fn settings_are_saved_as_udev_rules() {
    let sb = sandbox("save", true);
    let save = |attr: &str, value: &str| {
        let (ok, _, err) = sb.hook_with(
            "save-setting",
            &[("pid", "c24f"), ("attr", attr), ("value", value)],
        );
        assert!(ok, "{err}");
    };
    save("range", "900");
    save("gain", "32767");
    save("range", "540");
    let rules = sb.read(SETTINGS);
    let lines: Vec<&str> = rules.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(lines.len(), 2, "{rules}");
    assert!(
        rules.contains(r#"KERNEL=="0003:046D:C24F.*", ATTR{range}="540""#),
        "{rules}"
    );
    assert!(rules.contains(r#"ATTR{gain}="32767""#), "{rules}");

    // Development: kept in the driver's data.
    let dev = sandbox("save-dev", false);
    let (ok, _, _) = dev.hook_with(
        "save-setting",
        &[("pid", "c262"), ("attr", "range"), ("value", "720")],
    );
    assert!(ok);
    assert!(
        fs::read_to_string(dev.data.join("settings"))
            .unwrap()
            .contains("C262")
    );
}

/// The wheels the driver builds modules for are the plugin's, in the right family, and those
/// the bundled modules drive.
#[test]
fn wheels_agree_with_the_models_and_the_modules() {
    use simrig_plugin_logitech::models::{Family, MODELS};
    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    let list = |var: &str| -> Vec<u16> {
        let line = common
            .lines()
            .find(|l| l.starts_with(&format!("{var}=")))
            .unwrap();
        line.split('"')
            .nth(1)
            .unwrap()
            .split_whitespace()
            .map(|p| u16::from_str_radix(p, 16).unwrap())
            .collect()
    };
    let family = |f: Family| -> Vec<u16> {
        MODELS
            .iter()
            .filter(|m| m.family == f)
            .map(|m| m.pid)
            .collect()
    };
    let mut lg4ff = list("LG4FF_WHEELS");
    lg4ff.sort();
    let mut models = family(Family::Lg4ff);
    models.sort();
    assert_eq!(lg4ff, models);
    let mut tf = list("TF_WHEELS");
    tf.sort();
    let mut models = family(Family::DirectDrive);
    models.sort();
    assert_eq!(tf, models);

    // new-lg4ff drives each of them (hid-lg4ff.c), or switches it to a mode it drives.
    let ids = fs::read_to_string(Path::new(DRIVER).join("new-lg4ff/hid-ids.h")).unwrap();
    let source = fs::read_to_string(Path::new(DRIVER).join("new-lg4ff/hid-lg4ff.c")).unwrap();
    let section = |start: &str| {
        let after = source.split(start).nth(1).unwrap();
        after.split("};").next().unwrap().to_string()
    };
    let table = section("lg4ff_devices[] = {") + &section("lg4ff_multimode_wheels[] = {");
    for pid in lg4ff {
        let define = ids
            .lines()
            .find(|l| l.to_lowercase().ends_with(&format!("0x{pid:04x}")) && l.contains("LOGITECH"))
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_else(|| panic!("{pid:04x} not in hid-ids.h"));
        assert!(table.contains(define), "{define} not handled by new-lg4ff");
    }
    // The TrueForce driver's ids.
    let ids = fs::read_to_string(Path::new(DRIVER).join("logitech-trueforce/mainline/hid-ids.h"))
        .unwrap();
    for pid in tf {
        assert!(
            ids.to_lowercase().contains(&format!("0x{pid:04x}")),
            "{pid:04x}"
        );
    }
}

#[test]
fn contract_version_is_the_bundled_ones() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(
        contract.version,
        format!(
            "lg4ff-{}+tf-{}",
            version("new-lg4ff"),
            version("logitech-trueforce")
        )
    );
}
