//! A sandbox to run a driver's hooks for real in tests: stub system commands (dkms, modprobe,
//! modinfo, mokutil, udevadm, uname, id, make, gcc) that log their calls and keep a little
//! state, and a fake root (`/sys`, `/etc`, `/usr/src`…) through `SIMRIG_TEST_ROOT`. No root,
//! nothing touched outside a temporary directory.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::DriverStatus;

/// The running kernel, as `uname -r` says.
pub const KERNEL: &str = "7.0.0-test";

/// `dkms`: state lines are `<name> <version> <kernel or -> <state>`. Fails to build when a
/// `fail-build` file exists.
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

/// `modprobe`: loading creates /sys/module/<module>, unless Secure Boot refuses it
/// (`sb-reject`); `-r` removes it.
const MODPROBE: &str = r#"#!/bin/sh
echo "modprobe $*" >> "$SIMRIG_TEST_ROOT/log"
if [ "$1" = "-r" ]; then rm -rf "$SIMRIG_TEST_ROOT/sys/module/$(echo "$2" | tr - _)"; exit 0; fi
if [ -f "$SIMRIG_TEST_ROOT/sb-reject" ]; then
    echo "modprobe: ERROR: could not insert '$1': Key was rejected by service" >&2
    exit 1
fi
mkdir -p "$SIMRIG_TEST_ROOT/sys/module/$(echo "$1" | tr - _)"
"#;

/// `modinfo`: modules are described by `modinfo/<module>` files of `field=value` lines
/// (`filename`, `version`, `intree`).
const MODINFO: &str = r#"#!/bin/sh
case $1 in -n) field=filename; m=$2 ;; -F) field=$2; m=$3 ;; *) field=; m=$1 ;; esac
f="$SIMRIG_TEST_ROOT/modinfo/$(echo "$m" | tr - _)"
[ -f "$f" ] || { echo "modinfo: ERROR: Module $m not found." >&2; exit 1; }
sed -n "s/^$field=//p" "$f"
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

pub struct Sandbox {
    pub root: PathBuf,
    /// The driver's data directory (`SIMRIG_DRIVER_DATA`).
    pub data: PathBuf,
    driver: PathBuf,
    plugin: String,
}

impl Sandbox {
    /// A sandbox for the driver in `driver` (its directory with `driver.toml`), running hooks
    /// as root or as a user.
    pub fn new(driver: &Path, plugin: &str, name: &str, root_user: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-{plugin}-{name}-{}", std::process::id()));
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
        Sandbox {
            root,
            data,
            driver: driver.to_path_buf(),
            plugin: plugin.into(),
        }
    }

    /// Runs a hook with `SIMRIG_ARG_*` arguments; returns (success, stdout, stderr).
    pub fn hook_with(&self, name: &str, args: &[(&str, &str)]) -> (bool, String, String) {
        self.run(Command::new(self.driver.join("hooks").join(name)), args)
    }

    /// Runs a shell script (`set -eu`) in the hooks' environment.
    pub fn shell(&self, script: &str) -> (bool, String, String) {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(format!("set -eu\n{script}"));
        self.run(cmd, &[])
    }

    fn run(&self, mut cmd: Command, args: &[(&str, &str)]) -> (bool, String, String) {
        cmd.env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("SIMRIG_TEST_ROOT", &self.root)
            .env("SIMRIG_PLUGIN", &self.plugin)
            .env("SIMRIG_DRIVER_DIR", &self.driver)
            .env("SIMRIG_DRIVER_DATA", &self.data);
        for (key, value) in args {
            cmd.env(format!("SIMRIG_ARG_{}", key.to_ascii_uppercase()), value);
        }
        // A stub or hook just written may be busy for an instant (see `hooks::spawn`).
        let mut attempt = 0;
        let out = loop {
            match cmd.output() {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 20 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                result => break result.unwrap(),
            }
        };
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    pub fn hook(&self, name: &str) -> (bool, String, String) {
        self.hook_with(name, &[])
    }

    /// Runs `install` and fails the test if it fails.
    pub fn install(&self) -> String {
        let (ok, out, err) = self.hook("install");
        assert!(ok, "install failed:\n{out}{err}");
        out
    }

    pub fn status(&self) -> DriverStatus {
        let (ok, out, err) = self.hook("status");
        assert!(ok, "{err}");
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
    }

    /// A path in the fake root.
    pub fn file(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// Writes a file in the fake root, creating its directory.
    pub fn write(&self, path: &str, content: &str) {
        let path = self.file(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.file(path)).unwrap_or_default()
    }

    /// The commands the stubs ran.
    pub fn log(&self) -> String {
        self.read("log")
    }

    /// The stub DKMS's state: `<name> <version> <kernel> <state>` lines.
    pub fn dkms(&self) -> String {
        self.read("dkms-state")
    }

    /// A module modinfo knows: from the kernel (`in_tree`) or from elsewhere.
    pub fn module(&self, name: &str, version: &str, in_tree: bool) {
        let dir = if in_tree {
            "kernel/drivers/hid"
        } else {
            "extra"
        };
        self.write(
            &format!("modinfo/{}", name.replace('-', "_")),
            &format!(
                "filename=/lib/modules/{KERNEL}/{dir}/{name}.ko\nversion={version}\nintree={}\n",
                if in_tree { "Y" } else { "" }
            ),
        );
    }

    /// A USB device plugged in, with a HID device bound to `driver` (when given).
    pub fn plug(&self, vid: u16, pid: u16, driver: Option<&str>) -> String {
        let usb = format!("sys/bus/usb/devices/1-{pid:04x}");
        self.write(&format!("{usb}/idVendor"), &format!("{vid:04x}\n"));
        self.write(&format!("{usb}/idProduct"), &format!("{pid:04x}\n"));
        let id = format!("0003:{vid:04X}:{pid:04X}.0001");
        let dev = self.file(&format!("sys/bus/hid/devices/{id}"));
        fs::create_dir_all(&dev).unwrap();
        if let Some(driver) = driver {
            let dir = self.file(&format!("sys/bus/hid/drivers/{driver}"));
            fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink(dir, dev.join("driver")).unwrap();
        }
        id
    }

    /// A HID driver registered with the kernel (its `bind`/`unbind` files are written to).
    pub fn hid_driver(&self, name: &str) {
        fs::create_dir_all(self.file(&format!("sys/bus/hid/drivers/{name}"))).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
