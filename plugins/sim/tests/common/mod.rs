//! Helpers shared by the end-to-end tests.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use simrig_host::{Host, HostEvent, HostOptions};
use simrig_model::{Reason, Support};

pub const USB_PLUGIN: &str = "sim-usb";
pub const GENERIC_PLUGIN: &str = "sim-generic";

/// Temporary directories of one test, removed on drop.
pub struct Env {
    pub root: PathBuf,
}

impl Env {
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("simrig-e2e-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usb")).unwrap();
        Env { root }
    }

    pub fn options(&self) -> HostOptions {
        let exec = PathBuf::from(env!("CARGO_BIN_EXE_simrig-plugin-sim"));
        let bin_dir = exec.parent().unwrap_or(Path::new(".")).to_path_buf();
        HostOptions {
            plugin_dirs: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                self.root.join("plugins"),
            ],
            default_dirs: false,
            exec_dirs: vec![bin_dir.clone()],
            usb_root: self.root.join("usb"),
            config_dir: self.root.join("config"),
            data_dir: self.root.join("data"),
            dev_state_dir: self.root.join("state"),
            helper: Some(helper(&bin_dir)),
            ..HostOptions::default()
        }
    }

    pub fn start(&self) -> Host {
        Host::start(&self.options())
    }

    /// A second simulated "brand": every USB device with vendor 0xfffe becomes a pedal set.
    pub fn add_usb_plugin(&self) {
        let dir = self.root.join("plugins").join(USB_PLUGIN);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.toml"),
            format!(
                "id = '{USB_PLUGIN}'\nname = 'Simulated USB pedals'\nversion = '0.1.0'\napi = 1\n\
                 exec = 'simrig-plugin-sim'\nargs = ['--id', '{USB_PLUGIN}', '--devices', '', '--usb-pedals']\n\
                 [match]\nusb = [{{ vid = 0xfffe }}]\n"
            ),
        )
        .unwrap();
    }

    /// A generic plugin: it gets only the USB devices the user assigns to it.
    pub fn add_generic_plugin(&self) {
        let dir = self.root.join("plugins").join(GENERIC_PLUGIN);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.toml"),
            format!(
                "id = '{GENERIC_PLUGIN}'\nname = 'Simulated generic USB'\nversion = '0.1.0'\napi = 1\n\
                 exec = 'simrig-plugin-sim'\nargs = ['--id', '{GENERIC_PLUGIN}', '--devices', '', '--usb-generic']\n\
                 [match]\non_request = true\n"
            ),
        )
        .unwrap();
    }

    /// A USB device no plugin knows.
    pub fn plug_unknown_usb(&self, port: &str) {
        let dev = self.root.join("usb").join(port);
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), "1234\n").unwrap();
        fs::write(dev.join("idProduct"), "0001\n").unwrap();
        fs::write(dev.join("product"), "Unknown shifter\n").unwrap();
    }

    pub fn plug_usb(&self, port: &str) {
        let dev = self.root.join("usb").join(port);
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), "fffe\n").unwrap();
        fs::write(dev.join("idProduct"), "0001\n").unwrap();
        fs::write(dev.join("product"), "Fake pedals\n").unwrap();
    }

    pub fn unplug_usb(&self, port: &str) {
        fs::remove_dir_all(self.root.join("usb").join(port)).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// `simrig-helper` from the same build; built on demand when only this package is tested.
pub fn helper(bin_dir: &Path) -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    let path = bin_dir.join("simrig-helper");
    BUILT.get_or_init(|| {
        if !path.is_file() {
            let status = Command::new(env!("CARGO"))
                .args(["build", "-q", "-p", "simrig-helper"])
                .status()
                .unwrap();
            assert!(status.success(), "cannot build simrig-helper");
        }
    });
    path
}

pub fn reason(host: &Host, address: &str, item: &str) -> Option<Reason> {
    match &host
        .find(address)
        .unwrap()
        .description
        .item(item)
        .unwrap()
        .support
    {
        Support::Supported(_) => None,
        Support::Unsupported(u) => Some(u.reason.clone()),
    }
}

/// Polls until `done` holds for an event, or fails after a while.
pub fn poll_until(
    host: &mut Host,
    what: &str,
    done: impl Fn(&HostEvent) -> bool,
) -> Vec<HostEvent> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        let events = host.poll();
        let found = events.iter().any(&done);
        seen.extend(events);
        if found {
            return seen;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no event: {what}; seen {seen:?}");
}
