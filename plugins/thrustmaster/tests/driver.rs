//! The Thrustmaster driver's hooks, run for real in a sandbox with stub system commands. The
//! sources are the bundled hid-tmff2.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-thrustmaster.rules";
const CONF: &str = "etc/modprobe.d/simrig-thrustmaster.conf";

fn sandbox(name: &str, root_user: bool) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "thrustmaster", name, root_user)
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("hid-tmff2.version"))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn clean_install_and_uninstall() {
    let sb = sandbox("clean", true);
    let v = version();
    assert!(!sb.status().installed);
    // A T300 RS already plugged in, on hid-generic.
    sb.plug(0x044f, 0xb66e, Some("hid-generic"));
    sb.hid_driver("tmff2");

    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("hid-tmff2 {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    // Upstream keeps dkms.conf in a subdirectory and a fixed version.
    let conf = sb.read(&format!("usr/src/hid-tmff2-{v}/dkms.conf"));
    assert!(conf.contains(&format!("PACKAGE_VERSION=\"{v}\"")), "{conf}");
    assert!(
        sb.file(&format!(
            "usr/src/hid-tmff2-{v}/deps/hid-tminit/hid-tminit.c"
        ))
        .is_file()
    );
    // hid-tminit starts the wheels in place of the kernel's module.
    assert!(sb.read(CONF).contains("blacklist hid_thrustmaster"));
    let log = sb.log();
    assert!(log.contains("modprobe -r hid_thrustmaster"), "{log}");
    for module in ["hid_tminit_new", "usb_tminit_new", "hid_tmff_new"] {
        assert!(
            sb.file(&format!("sys/module/{module}")).is_dir(),
            "{module}"
        );
    }
    // The wheel started again, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-b66e/authorized"), "1\n");
    assert!(sb.read(RULES).contains("tmff2"));
    let status = sb.status();
    assert!(status.installed && status.active, "{status:?}");

    let (ok, _, err) = sb.hook("uninstall");
    assert!(ok, "{err}");
    assert!(!sb.dkms().contains("hid-tmff2"), "{}", sb.dkms());
    assert!(!sb.file(CONF).exists() && !sb.file(RULES).exists());
    assert!(sb.log().contains("modprobe hid_thrustmaster"));
    assert!(!sb.status().installed);
}

#[test]
fn a_driver_installed_by_the_user_is_kept() {
    let sb = sandbox("foreign", true);
    fs::write(
        sb.file("dkms-state"),
        format!("hid-tmff2 0.82 {KERNEL} installed\n"),
    )
    .unwrap();
    assert_eq!(sb.status().version.as_deref(), Some("0.82"));
    let out = sb.install();
    assert!(out.contains("not by SimRig"), "{out}");
    assert!(!sb.log().contains("dkms add"));
    // The user's configuration of the kernel's modules is theirs.
    assert!(!sb.file(CONF).exists());
    assert!(sb.status().installed);
    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(sb.dkms().contains("hid-tmff2 0.82"));
}

#[test]
fn settings_are_module_options() {
    let sb = sandbox("save", true);
    sb.install();
    let save = |attr: &str, value: &str| {
        let (ok, _, err) = sb.hook_with("save-setting", &[("attr", attr), ("value", value)]);
        assert!(ok, "{err}");
    };
    save("range", "900");
    save("gain", "40000");
    save("range", "540");
    let conf = sb.read(CONF);
    assert!(conf.contains("blacklist hid_thrustmaster"), "{conf}");
    assert!(
        conf.lines()
            .any(|l| l == "options hid_tmff_new gain=40000 range=540"),
        "{conf}"
    );
}

#[test]
fn secure_boot_asks_to_enroll_the_signing_key() {
    let sb = sandbox("sb", true);
    sb.write("sb-reject", "");
    sb.write("var/lib/dkms/mok.pub", "key");
    let out = sb.install();
    let password = fs::read_to_string(sb.data.join("mok-pending")).unwrap();
    assert!(out.contains(password.trim()), "{out}");
    let status = sb.status();
    assert!(status.needs_reboot && !status.active, "{status:?}");
}

#[test]
fn without_root_nothing_is_built() {
    let sb = sandbox("dev", false);
    let out = sb.install();
    assert!(out.contains("development install"), "{out}");
    assert!(sb.dkms().is_empty());
    assert!(sb.status().installed);
}

/// The wheels rebound after a build, and the plugin's models, are the ones hid-tmff2 drives;
/// the ones starting up are hid-tminit's.
#[test]
fn wheels_agree_with_the_driver() {
    use simrig_plugin_thrustmaster::models::{Kind, MODELS};
    let header = fs::read_to_string(Path::new(DRIVER).join("hid-tmff2/src/hid-tmff2.h")).unwrap();
    let source = fs::read_to_string(Path::new(DRIVER).join("hid-tmff2/src/hid-tmff2.c")).unwrap();
    let table = source.split("tmff2_devices[] = {").nth(1).unwrap();
    let table = table.split("};").next().unwrap();
    let mut driven: Vec<u16> = header
        .lines()
        .filter(|l| l.starts_with("#define"))
        .filter_map(|l| {
            let mut words = l.split_whitespace().skip(1);
            let (name, value) = (words.next()?, words.next()?);
            (table.contains(name) && !name.contains("VENDOR")).then_some(value)
        })
        .filter_map(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok())
        .collect();
    driven.sort();

    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    let line = common.lines().find(|l| l.starts_with("WHEELS=")).unwrap();
    let mut rebound: Vec<u16> = line
        .split('"')
        .nth(1)
        .unwrap()
        .split_whitespace()
        .map(|p| u16::from_str_radix(p, 16).unwrap())
        .collect();
    rebound.sort();
    assert_eq!(rebound, driven);

    let mut wheels: Vec<u16> = MODELS
        .iter()
        .filter(|m| matches!(m.kind, Kind::Wheel { .. }))
        .map(|m| m.pid)
        .collect();
    wheels.sort();
    assert_eq!(wheels, driven);

    let init = fs::read_to_string(Path::new(DRIVER).join("hid-tmff2/deps/hid-tminit/hid-tminit.c"))
        .unwrap()
        + &fs::read_to_string(Path::new(DRIVER).join("hid-tmff2/deps/hid-tminit/usb-tminit.c"))
            .unwrap();
    for m in MODELS.iter().filter(|m| m.kind == Kind::StartingUp) {
        assert!(init.contains(&format!("0x{:04x}", m.pid)), "{:04x}", m.pid);
    }
}

#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
