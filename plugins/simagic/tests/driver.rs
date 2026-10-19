//! The Simagic driver's hooks, run for real in a sandbox with stub system commands. The sources
//! are the bundled simagic-ff.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const RULES: &str = "etc/udev/rules.d/71-simrig-simagic.rules";

/// A report descriptor declaring standard force feedback (older firmware), and one that does
/// not (newer firmware).
const OLD_FIRMWARE: &[u8] = &[
    0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x05, 0x0f, 0x09, 0x92, 0xc0,
];
const NEW_FIRMWARE: &[u8] = &[0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x06, 0x00, 0xff, 0xc0];

fn sandbox(name: &str) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "simagic", name, true)
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("simagic-ff.version"))
        .unwrap()
        .trim()
        .to_string()
}

fn alpha(sb: &Sandbox, descriptor: &[u8]) {
    let id = sb.plug(0x0483, 0x0522, Some("hid-generic"));
    fs::write(
        sb.file(&format!("sys/bus/hid/devices/{id}/report_descriptor")),
        descriptor,
    )
    .unwrap();
}

#[test]
fn an_evo_gets_simagic_ff() {
    let sb = sandbox("evo");
    let v = version();
    sb.plug(0x3670, 0x0502, Some("hid-generic"));
    let status = sb.status();
    assert!(!status.installed);
    assert!(status.message.unwrap().contains("Builds"));
    sb.hid_driver("simagic-ff");
    sb.install();
    assert!(
        sb.dkms()
            .contains(&format!("simagic-ff {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    assert!(sb.file("sys/module/hid_simagic_ff").is_dir());
    // The base started again, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-0502/authorized"), "1\n");
    assert!(sb.read(RULES).contains("simagic-ff"));
    assert!(sb.status().installed);

    let (ok, _, err) = sb.hook("uninstall");
    assert!(ok, "{err}");
    assert!(!sb.dkms().contains("installed"), "{}", sb.dkms());
    assert!(!sb.file(RULES).exists());
}

#[test]
fn an_alpha_with_an_older_firmware_keeps_the_kernels_driver() {
    let sb = sandbox("old");
    alpha(&sb, OLD_FIRMWARE);
    sb.install();
    assert!(!sb.log().contains("dkms add"), "{}", sb.log());
    let status = sb.status();
    assert!(status.installed, "{status:?}");
    assert!(status.message.unwrap().contains("older firmware"));
}

#[test]
fn an_alpha_with_a_newer_firmware_gets_simagic_ff() {
    let sb = sandbox("new");
    alpha(&sb, NEW_FIRMWARE);
    sb.hid_driver("simagic-ff");
    sb.install();
    assert!(sb.dkms().contains("simagic-ff"), "{}", sb.dkms());
}

#[test]
fn settings_are_saved_as_udev_rules() {
    let sb = sandbox("save");
    sb.install();
    let save = |attr: &str, value: &str| {
        let (ok, _, err) = sb.hook_with(
            "save-setting",
            &[("model", "3670:0502"), ("attr", attr), ("value", value)],
        );
        assert!(ok, "{err}");
    };
    save("ff_strength", "80");
    save("game_damper", "120");
    save("ff_strength", "60");
    let rules = sb.read("etc/udev/rules.d/72-simrig-simagic-settings.rules");
    assert!(
        rules.contains(
            r#"DRIVER=="simagic-ff", KERNEL=="0003:3670:0502.*", ATTR{ff_strength}="60""#
        ),
        "{rules}"
    );
    assert!(!rules.contains(r#"ATTR{ff_strength}="80""#), "{rules}");
    assert!(rules.contains(r#"ATTR{game_damper}="120""#), "{rules}");
}

/// The plugin's bases are the ones simagic-ff drives, rebound after a build and given to the
/// user by the rules.
#[test]
fn bases_agree_with_the_driver() {
    let header = fs::read_to_string(Path::new(DRIVER).join("simagic-ff/hid-simagic.h")).unwrap();
    let source = fs::read_to_string(Path::new(DRIVER).join("simagic-ff/hid-simagic.c")).unwrap();
    let value = |name: &str| -> u16 {
        let line = header
            .lines()
            .find(|l| l.split_whitespace().nth(1) == Some(name))
            .unwrap_or_else(|| panic!("{name}"));
        u16::from_str_radix(&line.split_whitespace().nth(2).unwrap()[2..], 16).unwrap()
    };
    let mut driven: Vec<(u16, u16)> = source
        .lines()
        .filter_map(|l| l.trim().strip_prefix("{ HID_USB_DEVICE("))
        .map(|l| {
            let mut names = l.split([',', ')']).map(str::trim);
            (value(names.next().unwrap()), value(names.next().unwrap()))
        })
        .collect();
    driven.sort();
    let mut modelled: Vec<(u16, u16)> = simrig_plugin_simagic::models::MODELS
        .iter()
        .map(|m| (m.vid, m.pid))
        .collect();
    modelled.sort();
    assert_eq!(modelled, driven);

    let common = fs::read_to_string(Path::new(DRIVER).join("hooks/common.sh")).unwrap();
    let evo = common
        .lines()
        .find(|l| l.starts_with("EVO_BASES="))
        .unwrap();
    let rules = fs::read_to_string(Path::new(DRIVER).join("udev/71-simrig-simagic.rules")).unwrap();
    for (vid, pid) in driven {
        if vid == 0x3670 {
            assert!(evo.contains(&format!("{pid:04x}")), "{pid:04x}");
        }
        assert!(rules.contains(&format!("{pid:04x}")), "{pid:04x}");
    }
}

#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
