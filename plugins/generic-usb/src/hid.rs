//! Raw reports of a USB device's HID interfaces, through `hidraw`. Each interface is read on its
//! own thread; the latest report of each is kept.
//!
//! Tests and development use a fake device: a USB directory outside `/sys` with a
//! `simrig-fake-report` file holding the current report in hex, one interface per line.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const FAKE_REPORT: &str = "simrig-fake-report";
const MAX_REPORT: usize = 64;

/// The hidraw nodes (`/dev/hidrawN`) of a USB device, in interface order.
pub fn hidraw_nodes(usb_sysfs: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(usb_sysfs, 0, &mut found);
    // Interfaces are named `<port>:<config>.<interface>`: their order is the path order.
    found.sort();
    found
        .into_iter()
        .map(|(_, name)| PathBuf::from("/dev").join(name))
        .collect()
}

fn collect(dir: &Path, depth: usize, found: &mut Vec<(PathBuf, String)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // Only real subdirectories: sysfs links back to parents and drivers.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if name == "hidraw" {
            if let Ok(nodes) = fs::read_dir(&path) {
                for node in nodes.flatten() {
                    let node = node.file_name().to_string_lossy().into_owned();
                    if node.starts_with("hidraw") {
                        found.push((path.clone(), node));
                    }
                }
            }
        } else {
            collect(&path, depth + 1, found);
        }
    }
}

/// The sysfs directory of the USB device a hidraw node belongs to, with its ids and serial.
pub fn usb_device_of(hidraw: &str) -> Option<(PathBuf, u16, u16, Option<String>)> {
    let class = Path::new("/sys/class/hidraw").join(hidraw).join("device");
    let mut dir = fs::canonicalize(class).ok()?;
    loop {
        if dir.join("idVendor").is_file() {
            let read = |f: &str| {
                fs::read_to_string(dir.join(f))
                    .ok()
                    .map(|s| s.trim().to_string())
            };
            let vid = u16::from_str_radix(&read("idVendor")?, 16).ok()?;
            let pid = u16::from_str_radix(&read("idProduct")?, 16).ok()?;
            let serial = read("serial").filter(|s| !s.is_empty());
            return Some((dir, vid, pid, serial));
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Latest reports of every HID interface of a device.
pub struct Reports {
    latest: Arc<Mutex<Vec<Option<Vec<u8>>>>>,
    stop: Arc<AtomicBool>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl Reports {
    /// Starts reading every interface of the device at `usb_sysfs`. Fails when no interface can
    /// be read at all (no HID interface, or no permission yet).
    pub fn open(usb_sysfs: &Path) -> io::Result<Self> {
        let fake = usb_sysfs.join(FAKE_REPORT);
        if !usb_sysfs.starts_with("/sys") && fake.is_file() {
            return Ok(Self::fake(fake));
        }
        let nodes = hidraw_nodes(usb_sysfs);
        if nodes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the device has no HID interface",
            ));
        }
        let mut files = Vec::new();
        let mut denied = None;
        for node in &nodes {
            match File::open(node) {
                Ok(file) => files.push(Some(file)),
                Err(e) => {
                    denied.get_or_insert(e);
                    files.push(None);
                }
            }
        }
        if files.iter().all(Option::is_none) {
            return Err(denied.unwrap_or_else(|| io::Error::other("no readable interface")));
        }
        let reports = Self::empty(nodes.len());
        for (i, file) in files.into_iter().enumerate() {
            if let Some(file) = file {
                reports.spawn_reader(i, file);
            }
        }
        Ok(reports)
    }

    fn empty(interfaces: usize) -> Self {
        Self {
            latest: Arc::new(Mutex::new(vec![None; interfaces])),
            stop: Arc::new(AtomicBool::new(false)),
            errors: Arc::default(),
        }
    }

    fn spawn_reader(&self, interface: usize, mut file: File) {
        let latest = self.latest.clone();
        let stop = self.stop.clone();
        let errors = self.errors.clone();
        thread::spawn(move || {
            let mut buf = [0u8; MAX_REPORT];
            while !stop.load(Ordering::Relaxed) {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut l) = latest.lock() {
                            l[interface] = Some(buf[..n].to_vec());
                        }
                    }
                    Err(e) => {
                        if let Ok(mut errors) = errors.lock() {
                            errors.push(e.to_string());
                        }
                        break;
                    }
                }
            }
        });
    }

    fn fake(path: PathBuf) -> Self {
        let lines = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .count()
            .max(1);
        let reports = Self::empty(lines);
        let latest = reports.latest.clone();
        let stop = reports.stop.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(mut l) = latest.lock() {
                        for (i, line) in text.lines().enumerate() {
                            if let (Some(slot), Some(report)) = (l.get_mut(i), parse_hex(line)) {
                                *slot = Some(report);
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        reports
    }

    /// The latest report of an interface.
    pub fn latest(&self, interface: usize) -> Option<Vec<u8>> {
        self.latest.lock().ok()?.get(interface).cloned().flatten()
    }

    /// The latest report of every interface.
    pub fn all(&self) -> Vec<Option<Vec<u8>>> {
        self.latest.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Errors of the readers, e.g. the device went away.
    pub fn errors(&self) -> Vec<String> {
        self.errors.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl Drop for Reports {
    fn drop(&mut self) {
        // Readers blocked on a quiet device notice at their next report or when it goes away.
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// `01 0a ff` or `010aff` to bytes.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.is_empty() || digits.len() % 2 != 0 {
        return None;
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex() {
        assert_eq!(parse_hex("01 0a ff"), Some(vec![1, 10, 255]));
        assert_eq!(parse_hex("0"), None);
        assert_eq!(parse_hex("zz"), None);
    }

    #[test]
    fn finds_hidraw_nodes_in_interface_order() {
        let root = std::env::temp_dir().join(format!("simrig-hid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (iface, node) in [("1-3:1.1", "hidraw7"), ("1-3:1.0", "hidraw4")] {
            fs::create_dir_all(
                root.join(iface)
                    .join("0003:1234:0001.0001/hidraw")
                    .join(node),
            )
            .unwrap();
        }
        assert_eq!(
            hidraw_nodes(&root),
            vec![PathBuf::from("/dev/hidraw4"), PathBuf::from("/dev/hidraw7")]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fake_device() {
        let root = std::env::temp_dir().join(format!("simrig-fake-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(FAKE_REPORT), "00 04\n").unwrap();
        let reports = Reports::open(&root).unwrap();
        let mut seen = None;
        for _ in 0..100 {
            seen = reports.latest(0);
            if seen.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(seen, Some(vec![0, 4]));
        let _ = fs::remove_dir_all(&root);
    }
}
