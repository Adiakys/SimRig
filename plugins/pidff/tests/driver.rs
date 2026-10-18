//! The standard force feedback driver's hooks, run for real in a sandbox with stub system
//! commands. The sources are the bundled universal-pidff.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-pidff.rules";

fn sandbox(name: &str) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "pidff", name, true)
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
    assert!(sb.read(RULES).contains("3416"));
    assert!(sb.status().installed);
}

#[test]
fn on_older_kernels_the_bundled_module_is_built() {
    let sb = sandbox("build");
    let v = version();
    sb.plug(0x3416, 0x0301, Some("hid-generic"));
    sb.plug(0x2433, 0xf301, Some("hid-generic"));
    sb.hid_driver("hid-universal-pidff");
    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("hid-universal-pidff {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    // The bases started again, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-0301/authorized"), "1\n");
    assert_eq!(sb.read("sys/bus/usb/devices/1-f301/authorized"), "1\n");
    assert!(sb.status().installed);
    let (ok, _, err) = sb.hook("uninstall");
    assert!(ok, "{err}");
    assert!(!sb.dkms().contains("installed"), "{}", sb.dkms());
}

#[test]
fn a_module_built_by_another_driver_is_shared() {
    let sb = sandbox("shared");
    // SimRig's Moza or PXN driver built it: for this driver, someone else's.
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

/// The plugin's bases are in the module's table (the OpenFFBoard only behind a build option:
/// the kernel's generic driver drives it), and the ones rebound after a build.
#[test]
fn bases_agree_with_the_module() {
    let root = Path::new(DRIVER).join("universal-pidff");
    let ids = fs::read_to_string(root.join("hid-ids.h")).unwrap();
    let source = fs::read_to_string(root.join("hid-universal-pidff.c")).unwrap();
    let table = source
        .split("universal_pidff_devices[] = {")
        .nth(1)
        .unwrap();
    let table = table.split("#ifdef").next().unwrap();
    let defines: Vec<(String, u16)> = ids
        .lines()
        .filter(|l| l.starts_with("#define"))
        .filter_map(|l| {
            let mut w = l.split_whitespace().skip(1);
            let name = w.next()?.to_string();
            let value = u16::from_str_radix(w.next()?.trim_start_matches("0x"), 16).ok()?;
            Some((name, value))
        })
        .collect();
    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    let bases = common.lines().find(|l| l.starts_with("BASES=")).unwrap();
    let rules = fs::read_to_string(Path::new(DRIVER).join("udev/71-simrig-pidff.rules")).unwrap();
    for m in simrig_plugin_pidff::models::MODELS {
        let pair = format!("{:04x}:{:04x}", m.vid, m.pid);
        assert!(
            rules.contains(&format!("{:04x}", m.pid)),
            "{pair} not in the rules"
        );
        if m.vid == 0x1209 {
            continue;
        }
        let pid = defines
            .iter()
            .find(|(n, v)| n.starts_with("USB_DEVICE_ID") && *v == m.pid)
            .unwrap_or_else(|| panic!("{pair} not in hid-ids.h"));
        assert!(table.contains(&pid.0), "{pair} not driven");
        assert!(bases.contains(&pair), "{pair} not in BASES");
    }
}

#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
