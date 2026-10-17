//! The Fanatec driver's hooks, run for real against stub system commands (dkms, modprobe,
//! modinfo, mokutil, udevadm, uname, id) and a fake /sys: no root, nothing outside a temporary
//! directory. The sources are the bundled hid-fanatecff.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use simrig_driver::DriverStatus;

const DRIVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/driver");
const KERNEL: &str = "7.0.0-test";

/// Stub `dkms`: state lines are `<name> <version> <kernel or -> <state>`.
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
install) echo "$m $v $k installed" >> "$state" ;;
remove) grep -v "^$m $v " "$state" > "$state.t" || true; mv "$state.t" "$state" ;;
esac
"#;

/// Stub `modprobe`: loading creates /sys/module/hid_fanatec, unless Secure Boot refuses it.
const MODPROBE: &str = r#"#!/bin/sh
echo "modprobe $*" >> "$SIMRIG_TEST_ROOT/log"
if [ "$1" = "-r" ]; then rm -rf "$SIMRIG_TEST_ROOT/sys/module/$2"; exit 0; fi
if [ -f "$SIMRIG_TEST_ROOT/sb-reject" ]; then
    echo "modprobe: ERROR: could not insert '$1': Key was rejected by service" >&2
    exit 1
fi
mkdir -p "$SIMRIG_TEST_ROOT/sys/module/$1"
"#;

/// Stub `modinfo`: a module installed by other means is described by the `modinfo` file.
const MODINFO: &str = r#"#!/bin/sh
f="$SIMRIG_TEST_ROOT/modinfo"
[ -f "$f" ] || { echo "modinfo: ERROR: Module $2 not found." >&2; exit 1; }
case $1 in -n) echo /lib/modules/x/extra/hid-fanatec.ko ;; -F) cat "$f" ;; esac
"#;

const MOKUTIL: &str = r#"#!/bin/sh
echo "mokutil $*" >> "$SIMRIG_TEST_ROOT/log"
case $1 in
--sb-state) echo "SecureBoot enabled" ;;
--test-key) echo "$2 is not enrolled" ;;
--generate-hash=*) echo '$6$hash' ;;
esac
"#;

const LOGGER: &str = "#!/bin/sh\necho \"$(basename \"$0\") $*\" >> \"$SIMRIG_TEST_ROOT/log\"\n";

struct Sandbox {
    root: PathBuf,
    data: PathBuf,
}

impl Sandbox {
    fn new(name: &str, root_user: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-fanatec-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let uname = format!("#!/bin/sh\necho {KERNEL}\n");
        let id = format!("#!/bin/sh\necho {}\n", if root_user { 0 } else { 1000 });
        for (cmd, body) in [
            ("dkms", DKMS),
            ("modprobe", MODPROBE),
            ("modinfo", MODINFO),
            ("mokutil", MOKUTIL),
            ("udevadm", LOGGER),
            ("uname", uname.as_str()),
            ("id", id.as_str()),
            // Present, so the prerequisites are met.
            ("make", LOGGER),
            ("gcc", LOGGER),
        ] {
            let path = bin.join(cmd);
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir_all(root.join("lib/modules").join(KERNEL).join("build")).unwrap();
        let data = root.join("data");
        Sandbox { root, data }
    }

    fn hook(&self, name: &str) -> (bool, String, String) {
        let out = Command::new(Path::new(DRIVER).join("hooks").join(name))
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("SIMRIG_TEST_ROOT", &self.root)
            .env("SIMRIG_PLUGIN", "fanatec")
            .env("SIMRIG_DRIVER_DIR", DRIVER)
            .env("SIMRIG_DRIVER_DATA", &self.data)
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn status(&self) -> DriverStatus {
        let (ok, out, err) = self.hook("status");
        assert!(ok, "{err}");
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
    }

    fn file(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    fn log(&self) -> String {
        fs::read_to_string(self.file("log")).unwrap_or_default()
    }

    fn dkms(&self) -> String {
        fs::read_to_string(self.file("dkms-state")).unwrap_or_default()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn version() -> String {
    fs::read_to_string(Path::new(DRIVER).join("hid-fanatecff.version"))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn clean_install_and_uninstall() {
    let sb = Sandbox::new("clean", true);
    let v = version();
    let status = sb.status();
    assert!(!status.installed);

    // A base plugged in before the driver, bound to hid-generic.
    let dev = sb.file("sys/bus/hid/devices/0003:0EB7:0020.0005");
    fs::create_dir_all(&dev).unwrap();
    fs::create_dir_all(sb.file("sys/bus/hid/drivers/hid-generic")).unwrap();
    fs::create_dir_all(sb.file("sys/bus/hid/drivers/fanatec")).unwrap();
    std::os::unix::fs::symlink(
        sb.file("sys/bus/hid/drivers/hid-generic"),
        dev.join("driver"),
    )
    .unwrap();

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
    // The base went to the fanatec driver without replugging.
    assert_eq!(
        fs::read_to_string(sb.file("sys/bus/hid/drivers/hid-generic/unbind")).unwrap(),
        "0003:0EB7:0020.0005\n"
    );
    assert_eq!(
        fs::read_to_string(sb.file("sys/bus/hid/drivers/fanatec/bind")).unwrap(),
        "0003:0EB7:0020.0005\n"
    );

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
    let sb = Sandbox::new("foreign", true);
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
    let sb = Sandbox::new("package", true);
    fs::write(sb.file("modinfo"), "0.2.1\n").unwrap();
    assert_eq!(sb.status().version.as_deref(), Some("0.2.1"));
}

#[test]
fn secure_boot_asks_to_enroll_the_signing_key() {
    let sb = Sandbox::new("sb", true);
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
    let sb = Sandbox::new("prereq", true);
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
    let sb = Sandbox::new("upgrade", true);
    let v = version();
    fs::create_dir_all(&sb.data).unwrap();
    fs::write(sb.data.join("installed"), "0.2.2\n").unwrap();
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
    let sb = Sandbox::new("dev", false);
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
