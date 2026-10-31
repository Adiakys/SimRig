//! The serial port of a Moza device (a `ttyACM` of its USB device), where its settings are
//! read and written, and the fake one of tests.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol;

/// The fake port of tests: a file of `<command> <value>` lines in a USB device directory outside
/// `/sys`, standing for the settings the device stores.
pub const FAKE_SERIAL: &str = "simrig-fake-serial";

/// How long a device has to answer.
const TIMEOUT: Duration = Duration::from_millis(250);

pub trait Link {
    /// Sends a request and waits for the answer to (group, device id, id): the value it
    /// carries. `None` when the device does not answer.
    fn exchange(
        &mut self,
        request: &[u8],
        group: u8,
        device: u8,
        id: &[u8],
    ) -> io::Result<Option<u64>>;
}

/// The serial port of a USB device: its `ttyACM` node, or the fake one of tests.
pub fn find(usb: &Path) -> Option<PathBuf> {
    let fake = usb.join(FAKE_SERIAL);
    if !usb.starts_with("/sys") && fake.is_file() {
        return Some(fake);
    }
    let mut ttys: Vec<String> = fs::read_dir(usb)
        .ok()?
        .flatten()
        .filter_map(|interface| fs::read_dir(interface.path().join("tty")).ok())
        .flat_map(|dir| dir.flatten())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("ttyACM"))
        .collect();
    ttys.sort();
    ttys.first().map(|n| PathBuf::from("/dev").join(n))
}

pub fn open(path: &Path) -> io::Result<Box<dyn Link>> {
    if path.file_name().is_some_and(|n| n == FAKE_SERIAL) {
        return Ok(Box::new(Fake(path.to_path_buf())));
    }
    Tty::open(path).map(|t| Box::new(t) as Box<dyn Link>)
}

struct Tty {
    file: File,
    buf: Vec<u8>,
}

impl Tty {
    fn open(path: &Path) -> io::Result<Self> {
        use rustix::termios::{OptionalActions, SpecialCodeIndex, tcgetattr, tcsetattr};
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(rustix::fs::OFlags::NOCTTY.bits() as i32)
            .open(path)?;
        // Raw bytes; reads return after a tenth of a second without data.
        let mut t = tcgetattr(&file)?;
        t.make_raw();
        t.special_codes[SpecialCodeIndex::VMIN] = 0;
        t.special_codes[SpecialCodeIndex::VTIME] = 1;
        let _ = t.set_speed(115_200);
        tcsetattr(&file, OptionalActions::Now, &t)?;
        Ok(Self {
            file,
            buf: Vec::new(),
        })
    }
}

impl Link for Tty {
    fn exchange(
        &mut self,
        request: &[u8],
        group: u8,
        device: u8,
        id: &[u8],
    ) -> io::Result<Option<u64>> {
        self.file.write_all(request)?;
        let deadline = Instant::now() + TIMEOUT;
        let mut chunk = [0u8; 64];
        while Instant::now() < deadline {
            while let Some(frame) = protocol::take_frame(&mut self.buf) {
                if let Some(value) = protocol::answer(&frame, group, device, id) {
                    return Ok(Some(value));
                }
            }
            let n = self.file.read(&mut chunk)?;
            self.buf.extend_from_slice(&chunk[..n]);
        }
        Ok(None)
    }
}

/// Answers like a device, from and to the file: requests are checked, answers go through the
/// same decoding as real ones.
struct Fake(PathBuf);

impl Fake {
    fn values(&self) -> Vec<(String, u64)> {
        fs::read_to_string(&self.0)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (name, value) = l.split_once(' ')?;
                Some((name.to_string(), value.trim().parse().ok()?))
            })
            .collect()
    }
}

impl Link for Fake {
    fn exchange(
        &mut self,
        request: &[u8],
        group: u8,
        device: u8,
        id: &[u8],
    ) -> io::Result<Option<u64>> {
        let frame = protocol::parse_request(request)
            .ok_or_else(|| io::Error::other(format!("malformed request {request:02x?}")))?;
        let found = protocol::COMMANDS.iter().find_map(|c| {
            if !c.target.ids().contains(&frame.device) {
                return None;
            }
            [(c.read, false), (c.write, true)]
                .into_iter()
                .find_map(|(rw, write)| match rw {
                    Some((g, i)) if g == frame.group && frame.payload.starts_with(i) => {
                        Some((c, i.len(), write))
                    }
                    _ => None,
                })
        });
        let Some((command, id_len, write)) = found else {
            return Ok(None);
        };
        let mut values = self.values();
        let value = if write {
            let value = frame.payload[id_len..]
                .iter()
                .fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
            values.retain(|(n, _)| n != command.name);
            values.push((command.name.into(), value));
            let text: String = values.iter().map(|(n, v)| format!("{n} {v}\n")).collect();
            fs::write(&self.0, text)?;
            None
        } else {
            match values.iter().find(|(n, _)| n == command.name) {
                Some((_, v)) => Some(*v),
                // A setting this device does not have: no answer.
                None => return Ok(None),
            }
        };
        let mut answer = protocol::respond(&frame, id_len, value);
        let frame = protocol::take_frame(&mut answer).expect("an answer frame");
        Ok(protocol::answer(&frame, group, device, id))
    }
}
