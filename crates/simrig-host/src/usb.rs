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
    /// It offers a keyboard or a mouse (a HID interface with the boot protocol): a desktop
    /// device, never offered to the plugin of its maker by itself.
    pub boot_input: bool,
    /// The device's id for plugins, in component addresses: the same on any port (see
    /// [`stable_id`]).
    pub id: String,
}

impl UsbDevice {
    /// Stable keys of the device for user choices, most specific first:
    /// `usb:vvvv:pppp:serial` (when it has a serial) and `usb:vvvv:pppp`.
    pub fn keys(&self) -> Vec<String> {
        let base = format!("usb:{:04x}:{:04x}", self.vid, self.pid);
        match &self.serial {
            Some(serial) => vec![format!("{base}:{serial}"), base],
            None => vec![base],
        }
    }

    /// The USB port, as sysfs names it (`1-2`, `3-1.4`).
    pub fn port(&self) -> String {
        self.sysfs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The id component addresses had before it was stable: the port.
    pub fn legacy_id(&self) -> String {
        format!("usb-{}", self.port())
    }

    pub fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            id: self.id.clone(),
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

pub const DEFAULT_ROOT: &str = SYSFS_USB;
const HUB_CLASS: &str = "09";

/// Whether an interface of the device is a HID keyboard or mouse with the boot protocol
/// (class 03, subclass 01, protocol 01 or 02).
fn has_boot_input(device: &Path) -> bool {
    let Ok(entries) = fs::read_dir(device) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).any(|e| {
        let read = |file: &str| {
            fs::read_to_string(e.path().join(file))
                .ok()
                .map(|s| s.trim().to_string())
        };
        read("bInterfaceClass").as_deref() == Some("03")
            && read("bInterfaceSubClass").as_deref() == Some("01")
            && matches!(read("bInterfaceProtocol").as_deref(), Some("01" | "02"))
    })
}

/// Connected USB devices (not interfaces, not hubs' ports).
pub fn scan() -> Vec<UsbDevice> {
    scan_dir(Path::new(SYSFS_USB))
}

/// Same as [`scan`] on another sysfs tree (tests use a fake one).
pub fn scan_dir(root: &Path) -> Vec<UsbDevice> {
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
            // Hubs are infrastructure, never a rig device.
            if read("bDeviceClass").as_deref() == Some(HUB_CLASS) {
                return None;
            }
            let vid = u16::from_str_radix(&read("idVendor")?, 16).ok()?;
            let pid = u16::from_str_radix(&read("idProduct")?, 16).ok()?;
            let name = match (read("manufacturer"), read("product")) {
                (Some(m), Some(p)) => format!("{m} {p}"),
                (None, Some(p)) => p,
                (Some(m), None) => m,
                (None, None) => format!("USB {vid:04x}:{pid:04x}"),
            };
            let serial = read("serial");
            let boot_input = has_boot_input(&path);
            Some(UsbDevice {
                vid,
                pid,
                sysfs: path,
                name,
                serial,
                boot_input,
                id: String::new(),
            })
        })
        .collect();
    devices.sort_by(|a, b| a.sysfs.cmp(&b.sysfs));
    let ids: Vec<String> = devices.iter().map(stable_id).collect();
    for (i, device) in devices.iter_mut().enumerate() {
        // Two devices alike (same model, no serial or the same one): the port tells them apart.
        let alike = ids.iter().filter(|id| **id == ids[i]).count() > 1;
        device.id = if alike {
            format!("{}-at-{}", ids[i], device.port().replace('.', "-"))
        } else {
            ids[i].clone()
        };
    }
    devices
}

/// A device's id from what it is, not where it is plugged in: `usb-vvvv-pppp-<serial>`, or
/// `usb-vvvv-pppp` without a serial. The serial is kept to lower-case letters and digits, as
/// ids are parts of addresses.
pub fn stable_id(device: &UsbDevice) -> String {
    let base = format!("usb-{:04x}-{:04x}", device.vid, device.pid);
    let serial: String = device
        .serial
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if serial.is_empty() {
        base
    } else {
        format!("{base}-{serial}")
    }
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
        let hub = root.join("usb1");
        fs::create_dir_all(&hub).unwrap();
        fs::write(hub.join("idVendor"), "1d6b\n").unwrap();
        fs::write(hub.join("idProduct"), "0002\n").unwrap();
        fs::write(hub.join("bDeviceClass"), "09\n").unwrap();
        let devices = scan_dir(&root);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!((devices[0].vid, devices[0].pid), (0x0eb7, 0x0020));
        assert_eq!(devices[0].device_info().id, "usb-0eb7-0020");
        assert_eq!(devices[0].legacy_id(), "usb-1-2");
        assert!(!devices[0].boot_input);
    }

    #[test]
    fn keyboards_and_mice_are_told_apart() {
        let root =
            std::env::temp_dir().join(format!("simrig-usb-boot-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        plug(&root, "1-2", "c52b", None);
        let receiver = root.join("1-2/1-2:1.1");
        fs::create_dir_all(&receiver).unwrap();
        fs::write(receiver.join("bInterfaceClass"), "03\n").unwrap();
        fs::write(receiver.join("bInterfaceSubClass"), "01\n").unwrap();
        fs::write(receiver.join("bInterfaceProtocol"), "02\n").unwrap();
        plug(&root, "1-3", "c24f", None);
        let wheel = root.join("1-3/1-3:1.0");
        fs::create_dir_all(&wheel).unwrap();
        fs::write(wheel.join("bInterfaceClass"), "03\n").unwrap();
        fs::write(wheel.join("bInterfaceSubClass"), "00\n").unwrap();
        fs::write(wheel.join("bInterfaceProtocol"), "00\n").unwrap();
        let boot: Vec<bool> = scan_dir(&root).iter().map(|d| d.boot_input).collect();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(boot, [true, false]);
    }

    fn plug(root: &Path, port: &str, pid: &str, serial: Option<&str>) {
        let dev = root.join(port);
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join("idVendor"), "046d\n").unwrap();
        fs::write(dev.join("idProduct"), format!("{pid}\n")).unwrap();
        if let Some(serial) = serial {
            fs::write(dev.join("serial"), format!("{serial}\n")).unwrap();
        }
    }

    #[test]
    fn ids_follow_the_device_not_the_port() {
        let root = std::env::temp_dir().join(format!("simrig-usb-ids-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        plug(&root, "1-2", "c24f", Some("AB:12-cd"));
        plug(&root, "1-3", "c262", None);
        plug(&root, "3-1.4", "c262", None);
        plug(&root, "1-5", "c29b", Some("0000"));
        plug(&root, "1-6", "c29b", Some("0000"));
        let ids: Vec<String> = scan_dir(&root).into_iter().map(|d| d.id).collect();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            ids,
            [
                "usb-046d-c24f-ab12cd",
                "usb-046d-c262-at-1-3",
                "usb-046d-c29b-0000-at-1-5",
                "usb-046d-c29b-0000-at-1-6",
                "usb-046d-c262-at-3-1-4",
            ]
        );
    }
}
