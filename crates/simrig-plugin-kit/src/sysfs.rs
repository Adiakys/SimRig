//! The HID device a kernel driver is bound to, below a USB device, and its attributes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// HID devices of `vendor` (hex, e.g. `0EB7`) bound to one of `drivers`, below a USB device.
/// A device often has several; the first one with any of the `rich` entries (attributes or
/// the `input` directory) is the one to talk to. A USB interface bound to one of `drivers`
/// counts too: some old wheels have a USB driver of their own, not a HID one.
pub fn find_hid(usb: &Path, vendor: &str, drivers: &[&str], rich: &[&str]) -> Option<PathBuf> {
    let prefix = format!("0003:{}:", vendor.to_ascii_uppercase());
    let mut found = Vec::new();
    collect(usb, 0, &prefix, drivers, &mut found);
    found.sort();
    found
        .iter()
        .find(|p| rich.iter().any(|r| p.join(r).exists()))
        .or(found.first())
        .cloned()
}

fn collect(dir: &Path, depth: usize, prefix: &str, drivers: &[&str], found: &mut Vec<PathBuf>) {
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
        let bound = || {
            fs::read_link(path.join("driver"))
                .ok()
                .and_then(|d| d.file_name().map(|n| n.to_owned()))
                .is_some_and(|n| drivers.iter().any(|d| n == *d))
        };
        if name.starts_with(prefix) {
            if bound() {
                found.push(path);
            }
        } else if name.contains(':') && !name.starts_with("0003:") && bound() {
            // A USB interface (`1-2:1.0`) a USB driver of the wheel's own is bound to.
            found.push(path);
        } else {
            collect(&path, depth + 1, prefix, drivers, found);
        }
    }
}

/// The first subdirectory, e.g. the one class device below `<hid>/ftec_tuning`.
pub fn first_dir(dir: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

/// The event node of a HID device (`/dev/input/eventN`), or the fake input of tests (a
/// `simrig-fake-input` file in a HID directory outside `/sys`).
pub fn event_node(hid: &Path) -> Option<PathBuf> {
    let fake = hid.join(crate::input::FAKE_INPUT);
    if !hid.starts_with("/sys") && fake.is_file() {
        return Some(fake);
    }
    let input = first_dir(&hid.join("input"))?;
    let event = fs::read_dir(input)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("event"))?;
    Some(PathBuf::from("/dev/input").join(event))
}

pub fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Integers as drivers print them: decimal, or hexadecimal with `0x`.
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
