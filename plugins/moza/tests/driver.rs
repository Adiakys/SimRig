//! The Moza driver's hooks, run for real in a sandbox with stub system commands. The sources
//! are the bundled universal-pidff.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-moza.rules";

fn sandbox(name: &str, root_user: bool) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "moza", name, root_user)
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("universal-pidff.version"))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn with_the_kernels_module_only_rules_are_added() {
    let sb = sandbox("kernel", true);
    sb.module("hid-universal-pidff", "", true);
    let status = sb.status();
    assert!(!status.installed, "{status:?}");
    assert!(status.message.unwrap().contains("through the kernel"));

    let out = sb.install();
    assert!(out.contains("kernel's"), "{out}");
    assert!(!sb.log().contains("dkms add"), "{}", sb.log());
    let rules = sb.read(RULES);
    assert!(
        rules.contains("ttyACM") && rules.contains("uaccess"),
        "{rules}"
    );
    assert!(sb.status().installed);

    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(!sb.file(RULES).exists());
    assert!(!sb.status().installed);
}

#[test]
fn on_older_kernels_the_bundled_module_is_built() {
    let sb = sandbox("build", true);
    let v = version();
    // A base already plugged in, on hid-generic without force feedback.
    sb.plug(0x346e, 0x0002, Some("hid-generic"));
    // A pedal set: not given to hid-universal-pidff.
    sb.plug(0x346e, 0x0003, Some("hid-generic"));
    sb.hid_driver("hid-universal-pidff");

    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("hid-universal-pidff {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    let conf = sb.read(&format!("usr/src/hid-universal-pidff-{v}/dkms.conf"));
    assert!(conf.contains(&format!("PACKAGE_VERSION=\"{v}\"")), "{conf}");
    // The base started again, as if replugged; the pedals were left alone.
    assert_eq!(sb.read("sys/bus/usb/devices/1-0002/authorized"), "1\n");
    assert_eq!(sb.read("sys/bus/usb/devices/1-0003/authorized"), "");
    let status = sb.status();
    assert!(status.installed && status.active, "{status:?}");
    assert_eq!(status.version.as_deref(), Some(v.as_str()));

    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(!sb.dkms().contains("hid-universal-pidff"), "{}", sb.dkms());
}

#[test]
fn a_module_installed_by_the_user_is_kept() {
    let sb = sandbox("foreign", true);
    sb.module("hid-universal-pidff", "0.1.0", false);
    let status = sb.status();
    assert!(!status.installed);
    assert_eq!(status.version.as_deref(), Some("0.1.0"));
    let out = sb.install();
    assert!(out.contains("not by SimRig"), "{out}");
    assert!(!sb.log().contains("dkms add"));
    assert!(sb.status().installed);
}

#[test]
fn without_root_nothing_is_installed() {
    let sb = sandbox("dev", false);
    let out = sb.install();
    assert!(out.contains("development install"), "{out}");
    assert!(sb.dkms().is_empty() && !sb.file(RULES).exists());
    assert!(sb.status().installed);
}

/// The bases rebound after a build are the ones the bundled module drives.
#[test]
fn rebound_bases_are_the_modules() {
    let ids = fs::read_to_string(Path::new(DRIVER).join("universal-pidff/hid-ids.h")).unwrap();
    let mut from_driver: Vec<String> = ids
        .lines()
        .filter(|l| l.contains("USB_DEVICE_ID_MOZA"))
        .filter_map(|l| l.split_whitespace().nth(2))
        .map(|v| format!("{:04X}", u16::from_str_radix(&v[2..], 16).unwrap()))
        .collect();
    from_driver.sort();
    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    let line = common.lines().find(|l| l.starts_with("BASES=")).unwrap();
    let mut rebound: Vec<String> = line
        .trim_start_matches("BASES=")
        .trim_matches('"')
        .split_whitespace()
        .map(str::to_string)
        .collect();
    rebound.sort();
    assert_eq!(rebound, from_driver);
}

#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
