//! USB device enumeration through sysfs.

use std::fs;
use std::path::{Path, PathBuf};

use simrig_model::{Bus, DeviceInfo};

const SYSFS_USB: &str = "/sys/bus/usb/devices";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDevice {
    pub vid: u16,
    pub pid: u16,
    pub sysfs: PathBuf,
    pub name: String,
    pub serial: Option<String>,
}

impl UsbDevice {
    pub fn device_info(&self) -> DeviceInfo {
        let port = self
            .sysfs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        DeviceInfo {
            id: format!("usb-{port}"),
            name: self.name.clone(),
            bus: Bus::Usb {
                vid: self.vid,
                pid: self.pid,
                sysfs: self.sysfs.display().to_string(),
                serial: self.serial.clone(),
            },
        }
    }
}

/// Connected USB devices (not interfaces, not hubs' ports).
pub fn scan() -> Vec<UsbDevice> {
    scan_dir(Path::new(SYSFS_USB))
}

fn scan_dir(root: &Path) -> Vec<UsbDevice> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut devices: Vec<UsbDevice> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = fs::canonicalize(e.path()).unwrap_or_else(|_| e.path());
            let read = |file: &str| {
                fs::read_to_string(path.join(file))
                    .ok()
                    .map(|s| s.trim().to_string())
            };
            let vid = u16::from_str_radix(&read("idVendor")?, 16).ok()?;
            let pid = u16::from_str_radix(&read("idProduct")?, 16).ok()?;
            let name = match (read("manufacturer"), read("product")) {
                (Some(m), Some(p)) => format!("{m} {p}"),
                (None, Some(p)) => p,
                (Some(m), None) => m,
                (None, None) => format!("USB {vid:04x}:{pid:04x}"),
            };
            let serial = read("serial");
            Some(UsbDevice {
                vid,
                pid,
                sysfs: path,
                name,
                serial,
            })
        })
        .collect();
    devices.sort_by(|a, b| a.sysfs.cmp(&b.sysfs));
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_fake_sysfs() {
        let root = std::env::temp_dir().join(format!("simrig-usb-test-{}", std::process::id()));
        let dev = root.join("1-2");
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), "0eb7\n").unwrap();
        fs::write(dev.join("idProduct"), "0020\n").unwrap();
        fs::write(dev.join("product"), "FANATEC CSL DD\n").unwrap();
        fs::create_dir_all(root.join("1-2:1.0")).unwrap(); // an interface: no idVendor
        let devices = scan_dir(&root);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!((devices[0].vid, devices[0].pid), (0x0eb7, 0x0020));
        assert_eq!(devices[0].device_info().id, "usb-1-2");
    }
}
