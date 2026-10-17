//! A sandbox to run one plugin through the host in tests: the plugin's manifest and driver
//! (without bundled sources) copied into a temporary directory, its configuration there too,
//! and a fake USB tree (`usb/`) that devices are "plugged" into.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use simrig_model::{ProcedureState, Support};

use crate::{Host, HostEvent, HostOptions};

pub struct PluginEnv {
    pub root: PathBuf,
    plugin: String,
    exec: PathBuf,
}

impl PluginEnv {
    /// `source`: the plugin's directory (with `plugin.toml` and `driver/`); `exec`: its built
    /// adapter; `skip`: directories of the driver not copied (bundled sources). The adapter
    /// gets `--config <root>/<plugin>.toml`.
    pub fn new(source: &Path, plugin: &str, exec: &Path, name: &str, skip: &[&str]) -> Self {
        let root =
            std::env::temp_dir().join(format!("simrig-{plugin}-e2e-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usb")).unwrap();
        let dir = root.join("plugins").join(plugin);
        fs::create_dir_all(&dir).unwrap();
        let config = root.join(format!("{plugin}.toml"));
        let manifest = fs::read_to_string(source.join("plugin.toml"))
            .unwrap()
            .replace(
                "driver = \"driver/driver.toml\"",
                &format!(
                    "driver = \"driver/driver.toml\"\nargs = [\"--config\", \"{}\"]",
                    config.display()
                ),
            );
        fs::write(dir.join("plugin.toml"), manifest).unwrap();
        // A symbolic link would be refused: drivers must live inside their plugin.
        if source.join("driver").is_dir() {
            copy_dir(&source.join("driver"), &dir.join("driver"), skip);
        }
        PluginEnv {
            root,
            plugin: plugin.into(),
            exec: exec.to_path_buf(),
        }
    }

    /// A USB device plugged in at `usb/<port>`, with its HID device at `usb/<port>/<hid>`
    /// bound to `driver` (when given). Returns the HID directory.
    pub fn plug(
        &self,
        port: &str,
        vid: u16,
        pid: u16,
        product: &str,
        hid: &str,
        driver: Option<&str>,
    ) -> PathBuf {
        let usb = self.root.join("usb").join(port);
        fs::create_dir_all(&usb).unwrap();
        fs::write(usb.join("idVendor"), format!("{vid:04x}\n")).unwrap();
        fs::write(usb.join("idProduct"), format!("{pid:04x}\n")).unwrap();
        fs::write(usb.join("product"), format!("{product}\n")).unwrap();
        let hid = usb.join(hid);
        fs::create_dir_all(&hid).unwrap();
        if let Some(driver) = driver {
            self.bind(&hid, driver);
        }
        hid
    }

    /// Binds a HID device directory to a (fake) kernel driver.
    pub fn bind(&self, hid: &Path, driver: &str) {
        let dir = self.root.join("drivers").join(driver);
        fs::create_dir_all(&dir).unwrap();
        let _ = fs::remove_file(hid.join("driver"));
        std::os::unix::fs::symlink(&dir, hid.join("driver")).unwrap();
    }

    /// The fake inputs of a HID device: `abs <code> <value> <min> <max>` and
    /// `key <code> <0|1>` lines.
    pub fn input(&self, hid: &Path, text: &str) {
        fs::write(hid.join("simrig-fake-input"), text).unwrap();
    }

    /// The plugin's configuration file.
    pub fn config(&self) -> PathBuf {
        self.root.join(format!("{}.toml", self.plugin))
    }

    /// The driver's data directory (development install).
    pub fn driver_data(&self) -> PathBuf {
        self.root
            .join("state/drivers")
            .join(&self.plugin)
            .join("data")
    }

    pub fn start(&self) -> Host {
        let bin_dir = self.exec.parent().unwrap().to_path_buf();
        Host::start(&HostOptions {
            plugin_dirs: vec![self.root.join("plugins").join(&self.plugin)],
            default_dirs: false,
            exec_dirs: vec![bin_dir.clone()],
            usb_root: self.root.join("usb"),
            config_dir: self.root.join("config"),
            data_dir: self.root.join("data"),
            dev_state_dir: self.root.join("state"),
            helper: Some(helper(&bin_dir)),
            ..HostOptions::default()
        })
    }
}

impl Drop for PluginEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path, skip: &[&str]) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let name = entry.file_name();
        if skip.iter().any(|s| name == *s) {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to.join(name), skip);
        } else {
            fs::copy(entry.path(), to.join(name)).unwrap();
        }
    }
}

/// The privileged helper, built next to the adapter if needed: with the same profile, so it
/// lands in the same directory (`target/debug` or `target/release`).
fn helper(bin_dir: &Path) -> PathBuf {
    let path = bin_dir.join("simrig-helper");
    if !path.is_file() {
        let mut build = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        build.args(["build", "-q", "-p", "simrig-helper"]);
        if bin_dir.file_name().is_some_and(|n| n == "release") {
            build.arg("--release");
        }
        assert!(build.status().unwrap().success());
        assert!(path.is_file(), "{} was not built", path.display());
    }
    path
}

/// Polls the host until `done`, failing the test after five seconds.
pub fn until(host: &mut Host, what: &str, done: impl Fn(&mut Host) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        host.poll();
        if done(host) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out: {what}");
}

/// Polls the host until a procedure shows a step whose title contains `title`.
pub fn step(host: &mut Host, title: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for event in host.poll() {
            if let HostEvent::ProcedureUpdated {
                state: ProcedureState::Step(s),
                ..
            } = event
            {
                if s.title.contains(title) {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no step {title}");
}

/// Why an item of a component is unsupported (empty when it is supported).
pub fn reason(host: &mut Host, address: &str, item: &str) -> String {
    match &host.describe(address).unwrap().item(item).unwrap().support {
        Support::Unsupported(u) => u.explain(),
        Support::Supported(_) => String::new(),
    }
}
