//! The shell recipes, run for real in the test sandbox (stub system commands, a fake root).

use std::fs;
use std::path::{Path, PathBuf};

use simrig_driver::testing::KERNEL;

const RECIPES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/recipes");

struct Sandbox(simrig_driver::testing::Sandbox);

impl Sandbox {
    fn new(name: &str) -> Self {
        Sandbox(simrig_driver::testing::Sandbox::new(
            Path::new(RECIPES),
            "recipes",
            name,
            true,
        ))
    }

    /// Runs `script` with the recipes sourced; returns (success, stdout, stderr).
    fn run(&self, script: &str) -> (bool, String, String) {
        let sources: String = ["prereq", "status", "dkms", "secureboot", "udev", "module"]
            .iter()
            .map(|r| format!(". {RECIPES}/{r}.sh\n"))
            .collect();
        self.0.shell(&format!("{sources}{script}"))
    }

    fn log(&self) -> String {
        self.0.log()
    }

    fn state(&self) -> String {
        self.0.dkms()
    }

    /// A module's sources, with a VCS directory that must not be copied.
    fn source(&self) -> PathBuf {
        let src = self.0.root.join("source");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git/HEAD"), "ref").unwrap();
        fs::write(src.join("dkms.conf"), "PACKAGE_VERSION=\"#VERSION#\"\n").unwrap();
        fs::write(src.join("mod.c"), "MODULE_VERSION(\"#VERSION#\");\n").unwrap();
        src
    }
}

impl std::ops::Deref for Sandbox {
    type Target = simrig_driver::testing::Sandbox;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn dkms_installs_for_every_kernel_with_headers() {
    let sb = Sandbox::new("dkms");
    fs::create_dir_all(sb.root.join("lib/modules/6.9-old/build")).unwrap();
    fs::create_dir_all(sb.root.join("lib/modules/6.8-noheaders")).unwrap();
    let src = sb.source();
    let (ok, _, err) = sb.run(&format!("dkms_install hid-x 1.2 {}", src.display()));
    assert!(ok, "{err}");

    let dest = sb.root.join("usr/src/hid-x-1.2");
    assert!(!dest.join(".git").exists());
    assert_eq!(read(&dest.join("dkms.conf")), "PACKAGE_VERSION=\"1.2\"\n");
    assert!(read(&dest.join("mod.c")).contains("\"1.2\""));
    let state = sb.state();
    assert!(
        state.contains(&format!("hid-x 1.2 {KERNEL} installed")),
        "{state}"
    );
    assert!(state.contains("hid-x 1.2 6.9-old installed"), "{state}");
    assert!(!state.contains("6.8-noheaders"), "{state}");

    // Again: nothing added or built twice.
    let (ok, _, err) = sb.run(&format!("dkms_install hid-x 1.2 {}", src.display()));
    assert!(ok, "{err}");
    assert_eq!(sb.log().matches("dkms add").count(), 1, "{}", sb.log());
    assert_eq!(sb.log().matches("dkms install").count(), 2, "{}", sb.log());

    let (ok, out, _) = sb.run("dkms_versions hid-x; dkms_installed hid-x 1.2 && echo yes");
    assert!(ok);
    assert_eq!(out, "1.2\nyes\n");

    let (ok, _, _) = sb.run("dkms_remove hid-x 1.2");
    assert!(ok);
    assert!(!dest.exists());
    assert!(!sb.state().contains("hid-x 1.2"), "{}", sb.state());
}

#[test]
fn dkms_build_failure_shows_the_log() {
    let sb = Sandbox::new("fail");
    let src = sb.source();
    fs::write(sb.root.join("fail-build"), "").unwrap();
    let (ok, _, err) = sb.run(&format!("dkms_install hid-x 1.2 {}", src.display()));
    assert!(!ok);
    assert!(err.contains("implicit declaration"), "{err}");
}

#[test]
fn module_load_reports_rejected_signatures() {
    let sb = Sandbox::new("load");
    let (ok, out, _) = sb.run("module_load hid_x && echo loaded");
    assert!(ok);
    assert_eq!(out, "loaded\n");
    fs::write(sb.root.join("sb-reject"), "").unwrap();
    let (_, out, _) = sb.run("set +e; module_load hid_x; echo \"code $?\"");
    assert_eq!(out, "code 2\n");
}

#[test]
fn secure_boot_enrollment_gives_a_one_time_password() {
    let sb = Sandbox::new("sb");
    fs::create_dir_all(sb.root.join("var/lib/dkms")).unwrap();
    fs::write(sb.root.join("var/lib/dkms/mok.pub"), "key").unwrap();
    let (ok, out, err) = sb.run(
        "sb_enabled && echo enabled; key=$(sb_key); echo \"$key\"; \
         sb_key_enrolled \"$key\" || echo not-enrolled; sb_request_enrollment \"$key\"",
    );
    assert!(ok, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "enabled");
    assert!(lines[1].ends_with("var/lib/dkms/mok.pub"));
    assert_eq!(lines[2], "not-enrolled");
    assert!(
        lines[3].len() == 8 && lines[3].chars().all(|c| c.is_ascii_digit()),
        "{out}"
    );
    assert!(sb.log().contains("mokutil --import"), "{}", sb.log());
}

#[test]
fn udev_rules_are_installed_and_applied() {
    let sb = Sandbox::new("udev");
    let (ok, _, err) =
        sb.run("printf 'SUBSYSTEM==\"hid\"\\n' | udev_install 70-x.rules; udev_apply hid input");
    assert!(ok, "{err}");
    let rules = sb.root.join("etc/udev/rules.d/70-x.rules");
    assert_eq!(read(&rules), "SUBSYSTEM==\"hid\"\n");
    let log = sb.log();
    assert!(log.contains("udevadm control --reload"), "{log}");
    assert!(log.contains("--subsystem-match=input"), "{log}");
    let (ok, _, _) = sb.run("udev_remove 70-x.rules");
    assert!(ok);
    assert!(!rules.exists());
}

#[test]
fn prerequisites_and_status_line() {
    let sb = Sandbox::new("status");
    let (ok, out, _) = sb.run(
        "missing_commands sh no-such-tool; kernel_headers_present && echo headers; \
         status_json true false '' false true true 'Needs \"a\" restart'",
    );
    assert!(ok);
    let mut lines = out.lines();
    assert_eq!(lines.next(), Some("no-such-tool"));
    assert_eq!(lines.next(), Some("headers"));
    let status: simrig_driver::DriverStatus = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert!(status.installed && status.provided_by_system);
    assert_eq!(status.version, None);
    assert_eq!(status.message.as_deref(), Some("Needs \"a\" restart"));
}

/// Drivers keep copies of the recipes they use (they must work after the app is gone): the
/// copies must stay identical to the originals.
#[test]
fn driver_copies_match_the_recipes() {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    for plugin in fs::read_dir(plugins).unwrap().flatten() {
        let copies = plugin.path().join("driver/recipes");
        let Ok(entries) = fs::read_dir(&copies) else {
            continue;
        };
        for copy in entries.flatten() {
            let name = copy.file_name();
            let original = Path::new(RECIPES).join(&name);
            assert_eq!(
                read(&copy.path()),
                read(&original),
                "{} differs from recipes/{}: copy it again",
                copy.path().display(),
                name.to_string_lossy()
            );
        }
    }
}

#[test]
fn dkms_takes_upstream_packages_as_they_are() {
    let sb = Sandbox::new("upstream");
    let src = sb.root.join("upstream");
    fs::create_dir_all(src.join("dkms")).unwrap();
    // A fixed name and version, and the configuration in a subdirectory.
    fs::write(
        src.join("dkms/dkms.conf"),
        "PACKAGE_NAME=\"hid-up\"\nPACKAGE_VERSION=\"0.82\"\nAUTOINSTALL=\"yes\"\n",
    )
    .unwrap();
    let (ok, _, err) = sb.run(&format!(
        "dkms_install hid-x 0.83-4-gabc {} dkms/dkms.conf",
        src.display()
    ));
    assert!(ok, "{err}");
    let conf = read(&sb.file("usr/src/hid-x-0.83-4-gabc/dkms.conf"));
    assert!(conf.contains("PACKAGE_NAME=\"hid-x\""), "{conf}");
    assert!(conf.contains("PACKAGE_VERSION=\"0.83-4-gabc\""), "{conf}");
    assert!(conf.contains("AUTOINSTALL"), "{conf}");
}

#[test]
fn module_build_load_and_states() {
    let sb = Sandbox::new("module");
    let src = sb.source();
    let script = |s: &str| {
        let (ok, out, err) = sb.run(s);
        assert!(ok, "{s}: {err}");
        out
    };
    assert_eq!(script("mod_state hid-x hid_x 1.2"), "none\n");
    sb.module("hid_x", "", true);
    assert_eq!(script("mod_state hid-x hid_x 1.2"), "kernel\n");

    let out = script(&format!(
        "rc=0; mod_build hid-x 1.2 {} dkms.conf hid_x || rc=$?; echo $rc",
        src.display()
    ));
    assert_eq!(out, "0\n");
    assert!(sb.file("sys/module/hid_x").is_dir());
    // The kernel's module was unloaded first, so the new one runs.
    assert!(sb.log().contains("modprobe -r hid_x"), "{}", sb.log());
    assert_eq!(script("mod_state hid-x hid_x 1.2"), "ours 1.2\n");
    assert_eq!(script("mod_state hid-x hid_x 1.3"), "outdated 1.2\n");

    // An upgrade replaces SimRig's version.
    script(&format!(
        "mod_build hid-x 1.3 {} dkms.conf hid_x",
        src.display()
    ));
    assert!(!sb.state().contains("1.2"), "{}", sb.state());
    assert_eq!(script("mod_state hid-x hid_x 1.3"), "ours 1.3\n");

    script("mod_remove hid-x hid_x");
    assert!(!sb.state().contains("hid-x"), "{}", sb.state());
    assert_eq!(script("mod_state hid-x hid_x 1.3"), "kernel\n");
}

#[test]
fn module_of_someone_else_is_foreign() {
    let sb = Sandbox::new("foreign");
    sb.module("hid_x", "0.9", false);
    let (_, out, _) = sb.run("mod_state hid-x hid_x 1.2");
    assert_eq!(out, "foreign 0.9\n");
    fs::write(
        sb.file("dkms-state"),
        format!("hid-x 0.8 {KERNEL} installed\n"),
    )
    .unwrap();
    let (_, out, _) = sb.run("mod_state hid-x hid_x 1.2");
    assert_eq!(out, "foreign 0.8\n");
}

#[test]
fn module_refused_by_secure_boot_waits_for_the_key() {
    let sb = Sandbox::new("modsb");
    let src = sb.source();
    sb.write("sb-reject", "");
    sb.write("var/lib/dkms/mok.pub", "key");
    let (ok, out, err) = sb.run(&format!(
        "rc=0; mod_build hid-x 1.2 {} dkms.conf hid_x || rc=$?; echo $rc; mod_state hid-x hid_x 1.2; mod_key_message",
        src.display()
    ));
    assert!(ok, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "3");
    assert_eq!(lines[1], "pending 1.2");
    let password = read(&sb.data.join("mok-pending"));
    assert!(lines[2].contains(password.trim()), "{out}");
}

#[test]
fn hid_devices_are_rebound_and_usb_devices_found() {
    let sb = Sandbox::new("rebind");
    let generic = sb.plug(0x046d, 0xc24f, Some("hid-generic"));
    sb.plug(0x046d, 0xc262, Some("hid-generic"));
    sb.hid_driver("logitech");
    let (ok, out, err) = sb.run(
        "hid_rebind 046d logitech c24f; usb_present 046D C24F && echo yes; \
         usb_present 046d c999 || echo no; usb_present 046d && echo any",
    );
    assert!(ok, "{err}");
    assert_eq!(out, "yes\nno\nany\n");
    assert_eq!(
        sb.read("sys/bus/hid/drivers/logitech/bind"),
        format!("{generic}\n")
    );
    assert_eq!(
        sb.read("sys/bus/hid/drivers/hid-generic/unbind"),
        format!("{generic}\n")
    );
}
