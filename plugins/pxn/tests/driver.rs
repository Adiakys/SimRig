//! The PXN driver's hooks, run for real in a sandbox with stub system commands. The sources are
//! the bundled universal-pidff.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-pxn.rules";

fn sandbox(name: &str) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "pxn", name, true)
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("universal-pidff.version"))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn with_the_kernels_module_only_rules_are_added() {
    let sb = sandbox("kernel");
    sb.module("hid-universal-pidff", "", true);
    sb.install();
    assert!(!sb.log().contains("dkms add"), "{}", sb.log());
    assert!(sb.read(RULES).contains("hidraw"));
    assert!(sb.status().installed);
}

#[test]
fn on_older_kernels_the_bundled_module_is_built() {
    let sb = sandbox("build");
    let v = version();
    sb.plug(0x11ff, 0x1212, Some("hid-generic"));
    sb.plug(0x36e6, 0x400f, Some("hid-generic"));
    sb.hid_driver("hid-universal-pidff");
    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("hid-universal-pidff {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    // Each vendor's bases started again, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-1212/authorized"), "1\n");
    assert_eq!(sb.read("sys/bus/usb/devices/1-400f/authorized"), "1\n");
    assert!(sb.status().installed);
}

#[test]
fn a_module_built_by_the_moza_driver_is_shared() {
    let sb = sandbox("shared");
    // SimRig's Moza driver built it: for the PXN driver, someone else's.
    fs::write(
        sb.file("dkms-state"),
        format!("hid-universal-pidff {} {KERNEL} installed\n", version()),
    )
    .unwrap();
    let out = sb.install();
    assert!(out.contains("not by SimRig"), "{out}");
    assert!(!sb.log().contains("dkms add"));
    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(sb.dkms().contains("hid-universal-pidff"), "{}", sb.dkms());
}

/// The bases rebound after a build, and the plugin's models marked as in the module, are the ones
/// the module drives.
#[test]
fn bases_agree_with_the_module() {
    let ids = fs::read_to_string(Path::new(DRIVER).join("universal-pidff/hid-ids.h")).unwrap();
    let value = |name: &str| -> u16 {
        let line = ids
            .lines()
            .find(|l| l.split_whitespace().nth(1) == Some(name))
            .unwrap_or_else(|| panic!("{name}"));
        u16::from_str_radix(&line.split_whitespace().nth(2).unwrap()[2..], 16).unwrap()
    };
    let mut from_module: Vec<(u16, u16)> = ids
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter(|n| n.starts_with("USB_DEVICE_ID_PXN") || n.starts_with("USB_DEVICE_ID_LITE_STAR"))
        .map(|n| {
            let vendor = if n.contains("_VD") {
                "USB_VENDOR_ID_PXN"
            } else {
                "USB_VENDOR_ID_LITE_STAR"
            };
            (value(vendor), value(n))
        })
        .collect();
    from_module.sort();
    let mut modelled: Vec<(u16, u16)> = simrig_plugin_pxn::models::MODELS
        .iter()
        .filter(|m| m.module)
        .map(|m| (m.vid, m.pid))
        .collect();
    modelled.sort();
    assert_eq!(modelled, from_module);

    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    for (vid, pid) in from_module {
        let var = if vid == 0x36e6 {
            "PXN_BASES"
        } else {
            "LITE_STAR_BASES"
        };
        let line = common.lines().find(|l| l.starts_with(var)).unwrap();
        assert!(
            line.contains(&format!("{pid:04x}")),
            "{pid:04x} not in {var}"
        );
    }
}

#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
