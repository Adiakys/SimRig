//! The HID device a kernel driver is bound to, below a USB device, and its attributes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// HID devices of the USB `vendor` bound to one of `drivers`, below a USB device.
/// A device often has several; the first one with any of the `rich` entries (attributes or
/// the `input` directory) is the one to talk to. A USB interface bound to one of `drivers`
/// counts too: some old wheels have a USB driver of their own, not a HID one.
pub fn find_hid(usb: &Path, vendor: u16, drivers: &[&str], rich: &[&str]) -> Option<PathBuf> {
    let prefix = format!("0003:{vendor:04X}:");
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

/// The usage page of force feedback in HID (Physical Interface Device).
const PID_PAGE: u32 = 0x0f;

/// Whether a HID device below a USB device declares force feedback in its report descriptor,
/// whatever driver handles it and whether the kernel drives it yet: wheel bases do, pedals and
/// handbrakes do not.
pub fn declares_force_feedback(usb: &Path) -> bool {
    let mut descriptors = Vec::new();
    descriptors_below(usb, 0, &mut descriptors);
    descriptors
        .iter()
        .any(|d| usage_pages(d).contains(&PID_PAGE))
}

fn descriptors_below(dir: &Path, depth: usize, found: &mut Vec<Vec<u8>>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() == "report_descriptor" {
            found.extend(fs::read(entry.path()));
        } else if entry.file_type().is_ok_and(|t| t.is_dir()) {
            descriptors_below(&entry.path(), depth + 1, found);
        }
    }
}

/// The usage pages a HID report descriptor sets (its Usage Page items).
pub fn usage_pages(descriptor: &[u8]) -> Vec<u32> {
    let mut pages = Vec::new();
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        // A long item: its data size is in the next byte.
        if prefix == 0xfe {
            i += 3 + descriptor.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let data = descriptor.get(i + 1..i + 1 + size).unwrap_or_default();
        if prefix & 0xfc == 0x04 {
            let page = data
                .iter()
                .rev()
                .fold(0u32, |page, b| (page << 8) | u32::from(*b));
            pages.push(page);
        }
        i += 1 + size;
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_pages_of_a_descriptor() {
        // Generic desktop (a joystick), then PID with its two-byte form, then a vendor page.
        // The 0x05 inside the Usage Minimum's data is not an item.
        let descriptor = [
            0x05, 0x01, 0x09, 0x04, 0xa1, 0x01, 0x19, 0x05, 0x06, 0x0f, 0x00, 0xc0, 0x06, 0x00,
            0xff,
        ];
        assert_eq!(usage_pages(&descriptor), [0x01, 0x0f, 0xff00]);
        // Pedals: generic desktop and simulation controls only.
        assert_eq!(
            usage_pages(&[0x05, 0x01, 0x05, 0x02, 0x09, 0xc5]),
            [0x01, 0x02]
        );
    }
}
