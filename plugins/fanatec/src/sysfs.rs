//! Where hid-fanatecff exposes a device: its HID device directory (bound to the `fanatec`
//! driver), the tuning menu and the mounted rim below it, and its event node.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// `…/0003:0EB7:<PID>.<N>`: `range`, `wheel_id`, `fw_version`, `display`…
    pub hid: PathBuf,
}

impl Paths {
    /// The HID device of a USB device that the fanatec driver is bound to.
    pub fn find(usb: &Path) -> Option<Self> {
        find_hid(usb).map(|hid| Self { hid })
    }

    /// The tuning menu (`ftec_tuning/<name>/`), when the base has one.
    pub fn tuning(&self) -> Option<PathBuf> {
        first_dir(&self.hid.join("ftec_tuning"))
    }

    /// The mounted rim (`ftec_wheel/<name>/`), when the driver knows it.
    pub fn wheel(&self) -> Option<PathBuf> {
        first_dir(&self.hid.join("ftec_wheel"))
    }

    /// The event device of the HID device (`/dev/input/eventN`), or the fake input of tests.
    pub fn event(&self) -> Option<PathBuf> {
        let fake = self.hid.join(crate::input::FAKE_INPUT);
        if !self.hid.starts_with("/sys") && fake.is_file() {
            return Some(fake);
        }
        let input = first_dir(&self.hid.join("input"))?;
        let event = fs::read_dir(input)
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|n| n.starts_with("event"))?;
        Some(PathBuf::from("/dev/input").join(event))
    }

    pub fn attr(&self, name: &str) -> PathBuf {
        self.hid.join(name)
    }
}

fn find_hid(usb: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    collect(usb, 0, &mut found);
    found.sort();
    // A base has several HID devices bound to the driver; the one with its settings and its
    // inputs is the one to talk to (the others only carry raw HID).
    let rich = |p: &PathBuf| p.join("range").exists() || p.join("input").is_dir();
    found.iter().find(|p| rich(p)).or(found.first()).cloned()
}

/// HID devices of the Fanatec vendor bound to the `fanatec` driver, below a USB device.
fn collect(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        let path = entry.path();
        if name.starts_with("0003:0EB7:") {
            let driver = fs::read_link(path.join("driver")).ok();
            if driver.is_some_and(|d| d.file_name().is_some_and(|n| n == "fanatec")) {
                found.push(path);
            }
        } else {
            collect(&path, depth + 1, found);
        }
    }
}

fn first_dir(dir: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

pub fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Integers as the driver prints them: decimal, or hexadecimal with `0x`.
pub fn read_int(path: &Path) -> Option<i64> {
    let text = read(path)?;
    match text.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

pub fn write(path: &Path, value: &str) -> io::Result<()> {
    fs::write(path, format!("{value}\n"))
}
