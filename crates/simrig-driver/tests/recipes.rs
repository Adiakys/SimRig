//! The shell recipes, run for real with stub system commands (dkms, modprobe, mokutil, udevadm,
//! uname) that log their calls and keep a little state: no root, nothing touched outside a
//! temporary directory.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const RECIPES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/recipes");
const KERNEL: &str = "7.0.0-test";

/// The stub `dkms`: state lines are `<name> <version> <kernel or -> <state>`.
const DKMS: &str = r#"#!/bin/sh
state="$SIMRIG_TEST_ROOT/dkms-state"; touch "$state"
echo "dkms $*" >> "$SIMRIG_TEST_ROOT/log"
cmd=$1; shift; m=; v=; k=
while [ $# -gt 0 ]; do
    case $1 in -m) m=$2; shift ;; -v) v=$2; shift ;; -k) k=$2; shift ;; esac
    shift
done
case $cmd in
status)
    while read -r n vv kk st; do
        [ "$n" = "$m" ] || continue
        [ -z "$v" ] || [ "$vv" = "$v" ] || continue
        [ -z "$k" ] || [ "$kk" = "$k" ] || continue
        echo "$n/$vv, $kk, x86_64: $st"
    done < "$state" ;;
add) echo "$m $v - added" >> "$state" ;;
install)
    if [ -f "$SIMRIG_TEST_ROOT/fail-build" ]; then
        mkdir -p "$SIMRIG_TEST_ROOT/var/lib/dkms/$m/$v/build"
        echo "error: implicit declaration" > "$SIMRIG_TEST_ROOT/var/lib/dkms/$m/$v/build/make.log"
        exit 10
    fi
    echo "$m $v $k installed" >> "$state" ;;
remove) grep -v "^$m $v " "$state" > "$state.t" || true; mv "$state.t" "$state" ;;
esac
"#;

const LOGGER: &str = r#"#!/bin/sh
echo "$(basename "$0") $*" >> "$SIMRIG_TEST_ROOT/log"
"#;

const MODPROBE: &str = r#"#!/bin/sh
echo "modprobe $*" >> "$SIMRIG_TEST_ROOT/log"
if [ -f "$SIMRIG_TEST_ROOT/sb-reject" ]; then
    echo "modprobe: ERROR: could not insert '$1': Key was rejected by service" >&2
    exit 1
fi
"#;

const MOKUTIL: &str = r#"#!/bin/sh
echo "mokutil $*" >> "$SIMRIG_TEST_ROOT/log"
case $1 in
--sb-state) echo "SecureBoot enabled" ;;
--test-key) echo "$2 is not enrolled" ;;
--generate-hash=*) echo '$6$hash' ;;
esac
"#;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-recipes-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let uname = format!("#!/bin/sh\necho {KERNEL}\n");
        for (cmd, body) in [
            ("dkms", DKMS),
            ("modprobe", MODPROBE),
            ("mokutil", MOKUTIL),
            ("udevadm", LOGGER),
            ("uname", uname.as_str()),
        ] {
            let path = bin.join(cmd);
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir_all(root.join("lib/modules").join(KERNEL).join("build")).unwrap();
        Sandbox { root }
    }

    /// Runs `script` with the recipes sourced; returns (success, stdout, stderr).
    fn run(&self, script: &str) -> (bool, String, String) {
        let sources: String = ["prereq", "status", "dkms", "secureboot", "udev"]
            .iter()
            .map(|r| format!(". {RECIPES}/{r}.sh\n"))
            .collect();
        let out = Command::new("sh")
            .arg("-c")
            .arg(format!("set -eu\n{sources}{script}"))
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("SIMRIG_TEST_ROOT", &self.root)
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn log(&self) -> String {
        fs::read_to_string(self.root.join("log")).unwrap_or_default()
    }

    fn state(&self) -> String {
        fs::read_to_string(self.root.join("dkms-state")).unwrap_or_default()
    }

    /// A module's sources, with a VCS directory that must not be copied.
    fn source(&self) -> PathBuf {
        let src = self.root.join("source");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git/HEAD"), "ref").unwrap();
        fs::write(src.join("dkms.conf"), "PACKAGE_VERSION=\"#VERSION#\"\n").unwrap();
        fs::write(src.join("mod.c"), "MODULE_VERSION(\"#VERSION#\");\n").unwrap();
        src
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
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
