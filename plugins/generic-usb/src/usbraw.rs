//! Raw reports of a USB device's HID interfaces, read straight from their interrupt IN
//! endpoints: for devices the kernel's HID driver rejects (a broken report descriptor, so no
//! `hidraw` node), which still send ordinary reports. Reading an interface claims it, for one
//! reader at a time: while the app sets the device up, the app; once it is set up, the driver's
//! service, which relays the reports to the app (see [`relay_path`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Direction, In, Interrupt, TransferError};

/// HID interfaces have class 3.
const HID_CLASS: u8 = 3;
/// How long a read waits for a report before trying again (devices report on change only).
const WAIT: Duration = Duration::from_millis(500);

/// One HID interface, claimed, and its interrupt IN endpoint.
pub struct RawInterface {
    endpoint: nusb::Endpoint<Interrupt, In>,
    packet: usize,
    // Released when dropped.
    _interface: nusb::Interface,
}

fn sysfs_number(usb_sysfs: &Path, name: &str) -> Option<u8> {
    fs::read_to_string(usb_sysfs.join(name))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn to_io(e: nusb::Error) -> io::Error {
    io::Error::from(e)
}

/// Claims the HID interfaces of the USB device at `usb_sysfs` that have an interrupt IN
/// endpoint, in interface order. Fails when there is none, or no permission.
pub fn open(usb_sysfs: &Path) -> io::Result<Vec<RawInterface>> {
    let (Some(bus), Some(address)) = (
        sysfs_number(usb_sysfs, "busnum"),
        sysfs_number(usb_sysfs, "devnum"),
    ) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "not a USB device"));
    };
    let info = nusb::list_devices()
        .wait()
        .map_err(to_io)?
        .find(|d| d.busnum() == bus && d.device_address() == address)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the device is gone"))?;
    let device = info.open().wait().map_err(to_io)?;
    let config = device
        .active_configuration()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut found = Vec::new();
    for interface in config.interfaces() {
        let alt = interface.first_alt_setting();
        if alt.class() != HID_CLASS {
            continue;
        }
        let Some(ep) = alt.endpoints().find(|ep| {
            ep.transfer_type() == TransferType::Interrupt && ep.direction() == Direction::In
        }) else {
            continue;
        };
        found.push((
            interface.interface_number(),
            ep.address(),
            ep.max_packet_size(),
        ));
    }
    if found.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the device has no HID interface",
        ));
    }
    found.sort();
    let mut interfaces = Vec::new();
    for (number, address, packet) in found {
        let interface = device
            .detach_and_claim_interface(number)
            .wait()
            .map_err(to_io)?;
        let endpoint = interface
            .endpoint::<Interrupt, In>(address)
            .map_err(to_io)?;
        interfaces.push(RawInterface {
            endpoint,
            packet: packet.max(1),
            _interface: interface,
        });
    }
    Ok(interfaces)
}

impl RawInterface {
    /// The next report, waiting for it like a read of `hidraw`; 0 when the device went away.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let completion = self
                .endpoint
                .transfer_blocking(Buffer::new(self.packet), WAIT);
            match completion.status {
                Ok(()) => {
                    let n = completion.actual_len.min(buf.len());
                    buf[..n].copy_from_slice(&completion.buffer[..n]);
                    return Ok(n);
                }
                // Nothing reported for a while.
                Err(TransferError::Cancelled) => continue,
                Err(TransferError::Disconnected) => return Ok(0),
                Err(e) => return Err(io::Error::other(e.to_string())),
            }
        }
    }
}

/// Where the driver's service relays the reports of a set-up device it reads directly, for
/// the app to show them live: a file of hex reports, one line per interface, like the fake
/// device of tests. `SIMRIG_GENERIC_USB_RELAY` moves it, for tests.
pub fn relay_path(file_stem: &str) -> PathBuf {
    let dir = std::env::var_os("SIMRIG_GENERIC_USB_RELAY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/simrig-generic-usb"));
    dir.join(format!("{file_stem}.report"))
}

/// Writes the latest report of `interface` to the relay file (atomically).
pub fn write_relay(path: &Path, interface: usize, report: &[u8]) -> io::Result<()> {
    let mut text = "\n".repeat(interface);
    let hex: Vec<String> = report.iter().map(|b| format!("{b:02x}")).collect();
    text.push_str(&hex.join(" "));
    text.push('\n');
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_lines_are_per_interface() {
        let dir = std::env::temp_dir().join(format!("simrig-relay-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.report");
        write_relay(&path, 1, &[0x01, 0xfe]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "\n01 fe\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
