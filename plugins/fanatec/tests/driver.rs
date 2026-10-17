//! The Fanatec driver's hooks, run for real in a sandbox with stub system commands. The sources
//! are the bundled hid-fanatecff.

use std::fs;
use std::path::Path;

use simrig_driver::testing::{KERNEL, Sandbox};

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");

fn sandbox(name: &str, root_user: bool) -> Sandbox {
    Sandbox::new(Path::new(DRIVER), "fanatec", name, root_user)
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("hid-fanatecff.version"))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn clean_install_and_uninstall() {
    let sb = sandbox("clean", true);
    let v = version();
    let status = sb.status();
    assert!(!status.installed);

    // A base plugged in before the driver, on hid-generic.
    sb.plug(0x0eb7, 0x0020, Some("hid-generic"));

    let (ok, out, err) = sb.hook("install");
    assert!(ok, "{out}{err}");
    assert!(
        sb.dkms()
            .contains(&format!("hid-fanatec {v} {KERNEL} installed")),
        "{}",
        sb.dkms()
    );
    let src = sb.file(&format!("usr/src/hid-fanatec-{v}"));
    let conf = fs::read_to_string(src.join("dkms.conf")).unwrap();
    assert!(conf.contains(&format!("PACKAGE_VERSION=\"{v}\"")), "{conf}");
    assert!(!src.join(".git").exists());
    assert!(
        sb.file("etc/udev/rules.d/71-simrig-fanatec.rules")
            .is_file()
    );
    // The base started again with the driver, as if replugged.
    assert_eq!(sb.read("sys/bus/usb/devices/1-0020/authorized"), "1\n");

    let status = sb.status();
    assert!(
        status.installed && status.active && !status.provided_by_system,
        "{status:?}"
    );
    assert_eq!(status.version.as_deref(), Some(v.as_str()));

    let (ok, _, err) = sb.hook("uninstall");
    assert!(ok, "{err}");
    assert!(!sb.dkms().contains("hid-fanatec"), "{}", sb.dkms());
    assert!(!src.exists());
    assert!(!sb.file("etc/udev/rules.d/71-simrig-fanatec.rules").exists());
    assert!(!sb.status().installed);
}

#[test]
fn a_driver_installed_by_the_user_is_kept_with_simrigs_rules() {
    let sb = sandbox("foreign", true);
    // Like a hid-fanatec 0.2.3 installed by hand with DKMS.
    fs::write(
        sb.file("dkms-state"),
        format!("hid-fanatec 0.2.3 {KERNEL} installed\n"),
    )
    .unwrap();
    let status = sb.status();
    assert!(
        !status.installed && !status.provided_by_system,
        "{status:?}"
    );
    assert_eq!(status.version.as_deref(), Some("0.2.3"));
    assert!(status.message.unwrap_or_default().contains("keeps it"));

    // Installing adds only SimRig's rules: the module is not touched.
    let (ok, out, err) = sb.hook("install");
    assert!(ok, "{out}{err}");
    assert!(out.contains("not by SimRig"), "{out}");
    assert!(!sb.log().contains("dkms add"), "{}", sb.log());
    assert!(
        sb.file("etc/udev/rules.d/71-simrig-fanatec.rules")
            .is_file()
    );
    let status = sb.status();
    assert!(status.installed && !status.provided_by_system, "{status:?}");
    assert_eq!(status.version.as_deref(), Some("0.2.3"));

    // Removing SimRig's driver keeps the user's module.
    let (ok, _, _) = sb.hook("uninstall");
    assert!(ok);
    assert!(
        sb.dkms().contains("hid-fanatec 0.2.3"),
        "removed: {}",
        sb.dkms()
    );
    assert!(!sb.file("etc/udev/rules.d/71-simrig-fanatec.rules").exists());
    assert!(!sb.status().installed);

    // A module from a distribution package, outside DKMS.
    let sb = sandbox("package", true);
    sb.module("hid-fanatec", "0.2.1", false);
    assert_eq!(sb.status().version.as_deref(), Some("0.2.1"));
}

#[test]
fn secure_boot_asks_to_enroll_the_signing_key() {
    let sb = sandbox("sb", true);
    fs::write(sb.file("sb-reject"), "").unwrap();
    fs::create_dir_all(sb.file("var/lib/dkms")).unwrap();
    fs::write(sb.file("var/lib/dkms/mok.pub"), "key").unwrap();
    let (ok, out, err) = sb.hook("install");
    assert!(ok, "{out}{err}");
    let password = fs::read_to_string(sb.data.join("mok-pending")).unwrap();
    let password = password.trim();
    assert_eq!(password.len(), 8);
    assert!(out.contains(password), "{out}");
    let status = sb.status();
    assert!(
        status.installed && status.needs_reboot && !status.active,
        "{status:?}"
    );
    assert!(status.message.unwrap_or_default().contains(password));

    // After the restart the key is enrolled and the module loads.
    fs::remove_file(sb.file("sb-reject")).unwrap();
    fs::create_dir_all(sb.file("sys/module/hid_fanatec")).unwrap();
    let status = sb.status();
    assert!(!status.needs_reboot && status.active, "{status:?}");
}

#[test]
fn missing_prerequisites_are_named() {
    let sb = sandbox("prereq", true);
    fs::remove_dir_all(sb.file(&format!("lib/modules/{KERNEL}/build"))).unwrap();
    let (ok, _, err) = sb.hook("install");
    assert!(!ok);
    assert!(
        err.contains(&format!("kernel headers for {KERNEL}")),
        "{err}"
    );
    assert!(err.contains("linux-headers"), "{err}");
    assert!(!sb.log().contains("dkms add"));
}

#[test]
fn upgrades_the_version_simrig_installed() {
    let sb = sandbox("upgrade", true);
    let v = version();
    fs::create_dir_all(&sb.data).unwrap();
    fs::write(sb.data.join("hid-fanatec.installed"), "0.2.2\n").unwrap();
    fs::write(
        sb.file("dkms-state"),
        format!("hid-fanatec 0.2.2 {KERNEL} installed\n"),
    )
    .unwrap();
    let status = sb.status();
    assert!(!status.provided_by_system);
    assert!(status.message.unwrap_or_default().contains(&v));

    let (ok, out, err) = sb.hook("install");
    assert!(ok, "{out}{err}");
    let dkms = sb.dkms();
    assert!(!dkms.contains("0.2.2"), "{dkms}");
    assert!(
        dkms.contains(&format!("hid-fanatec {v} {KERNEL} installed")),
        "{dkms}"
    );
    assert_eq!(sb.status().version.as_deref(), Some(v.as_str()));
}

#[test]
fn without_root_nothing_is_built() {
    let sb = sandbox("dev", false);
    let (ok, out, _) = sb.hook("install");
    assert!(ok);
    assert!(out.contains("development install"), "{out}");
    assert!(sb.dkms().is_empty());
    let status = sb.status();
    assert!(status.installed && !status.active);
}

/// Every setting the bundled driver exposes is made writable by the udev rules: a new one
/// upstream shows up here when the submodule is updated.
#[test]
fn udev_rules_cover_every_setting() {
    let rules = fs::read_to_string(Path::new(DRIVER).join("udev/71-simrig-fanatec.rules")).unwrap();
    let header = fs::read_to_string(Path::new(DRIVER).join("hid-fanatecff/hid-ftec.h")).unwrap();
    let tuning: Vec<&str> = header
        .split("FTEC_TUNING_ATTR(")
        .skip(1)
        .filter_map(|s| s.split(',').next())
        .map(str::trim)
        .filter(|name| *name != "id")
        .collect();
    assert!(tuning.len() > 10, "{tuning:?}");
    let line = rules.lines().find(|l| l.contains("ftec_tuning")).unwrap();
    for name in tuning {
        assert!(
            line.contains(&format!(" {name} ")),
            "{name} is not writable"
        );
    }
}

/// The contract and the bundled sources agree on the version.
#[test]
fn contract_version_is_the_bundled_one() {
    let contract =
        simrig_driver::DriverContract::load(&Path::new(DRIVER).join("driver.toml")).unwrap();
    assert_eq!(contract.version, version());
}
