//! The control interface of a PXN VD base (its vendor HID interface, a hidraw node), and the
//! fake one of tests.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol::{self, BASE_SIZE, REPORT_SIZE};

/// The fake interface of tests: a file in a USB device directory outside `/sys`, holding the
/// base settings block in hex.
pub const FAKE_CONTROL: &str = "simrig-fake-pxn";

const TIMEOUT: Duration = Duration::from_millis(250);
const RETRIES: usize = 3;

pub trait Control {
    fn read_base(&mut self) -> io::Result<[u8; BASE_SIZE]>;
    fn write_base(&mut self, block: &[u8; BASE_SIZE]) -> io::Result<()>;
}

/// The hidraw node of the vendor interface below a USB device (its report descriptor starts
/// with usage page 0xff00), or the fake one of tests.
pub fn find(usb: &Path) -> Option<PathBuf> {
    let fake = usb.join(FAKE_CONTROL);
    if !usb.starts_with("/sys") && fake.is_file() {
        return Some(fake);
    }
    let mut nodes = Vec::new();
    for interface in fs::read_dir(usb).ok()?.flatten() {
        let Ok(hids) = fs::read_dir(interface.path()) else {
            continue;
        };
        for hid in hids.flatten() {
            let descriptor = fs::read(hid.path().join("report_descriptor")).unwrap_or_default();
            if !descriptor.starts_with(&[0x06, 0x00, 0xff]) {
                continue;
            }
            if let Ok(raws) = fs::read_dir(hid.path().join("hidraw")) {
                nodes.extend(
                    raws.flatten()
                        .map(|r| r.file_name().to_string_lossy().into_owned()),
                );
            }
        }
    }
    nodes.sort();
    nodes.first().map(|n| PathBuf::from("/dev").join(n))
}

pub fn open(path: &Path) -> io::Result<Box<dyn Control>> {
    if path.file_name().is_some_and(|n| n == FAKE_CONTROL) {
        return Ok(Box::new(Fake(path.to_path_buf())));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)?;
    let mut hidraw = Hidraw {
        file,
        session: false,
    };
    hidraw.handshake()?;
    Ok(Box::new(hidraw))
}

struct Hidraw {
    file: File,
    session: bool,
}

impl Hidraw {
    /// Drops what is waiting (mostly the steering stream).
    fn drain(&mut self) {
        let mut buf = [0u8; REPORT_SIZE];
        while matches!(self.file.read(&mut buf), Ok(n) if n > 0) {}
    }

    fn wait_for(&mut self, prefix: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + TIMEOUT;
        let mut buf = [0u8; REPORT_SIZE];
        while Instant::now() < deadline {
            match self.file.read(&mut buf) {
                Ok(n) if buf[..n].starts_with(prefix) => return Ok(Some(buf[..n].to_vec())),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    fn transact(&mut self, request: &[u8], prefix: &[u8]) -> io::Result<Vec<u8>> {
        for _ in 0..RETRIES {
            self.drain();
            self.file.write_all(&protocol::report(request))?;
            if let Some(answer) = self.wait_for(prefix)? {
                return Ok(answer);
            }
        }
        self.session = false;
        Err(io::Error::new(
            ErrorKind::TimedOut,
            "the base did not answer",
        ))
    }

    fn handshake(&mut self) -> io::Result<()> {
        self.transact(&protocol::HANDSHAKE, &protocol::HANDSHAKE)?;
        self.session = true;
        Ok(())
    }

    fn ready(&mut self) -> io::Result<()> {
        if !self.session {
            self.handshake()?;
        }
        Ok(())
    }
}

impl Control for Hidraw {
    fn read_base(&mut self) -> io::Result<[u8; BASE_SIZE]> {
        self.ready()?;
        let answer = self.transact(&protocol::BASE_READ, &protocol::BASE_READ)?;
        protocol::base_block(&answer)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "short base settings answer"))
    }

    fn write_base(&mut self, block: &[u8; BASE_SIZE]) -> io::Result<()> {
        self.ready()?;
        self.transact(&protocol::base_write(block), &protocol::WRITE_ACK)?;
        Ok(())
    }
}

/// Answers like a base, from and to the file; writes go through the same encoding.
struct Fake(PathBuf);

impl Control for Fake {
    fn read_base(&mut self) -> io::Result<[u8; BASE_SIZE]> {
        let text = fs::read_to_string(&self.0)?;
        let bytes: Vec<u8> = text
            .split_whitespace()
            .filter_map(|h| u8::from_str_radix(h, 16).ok())
            .collect();
        let mut answer = protocol::BASE_READ.to_vec();
        answer.extend(bytes);
        protocol::base_block(&protocol::report(&answer))
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "short fake block"))
    }

    fn write_base(&mut self, block: &[u8; BASE_SIZE]) -> io::Result<()> {
        let request = protocol::base_write(block);
        if !request.starts_with(&protocol::BASE_WRITE) || request.len() != REPORT_SIZE {
            return Err(io::Error::other("malformed write"));
        }
        let payload = &request[protocol::BASE_WRITE.len()..][..BASE_SIZE];
        let text: Vec<String> = payload.iter().map(|b| format!("{b:02x}")).collect();
        fs::write(&self.0, text.join(" ") + "\n")
    }
}
