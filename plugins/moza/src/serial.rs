//! The serial port of a Moza device (a `ttyACM` of its USB device), where its settings are
//! read and written, and the fake one of tests.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol::{self, Target};

/// How long a device has to answer: the base answers in milliseconds.
pub const ANSWER: Duration = Duration::from_millis(250);

/// The fake port of tests: a file of `<command> <value>` lines in a USB device directory outside
/// `/sys`, standing for the settings the device stores.
pub const FAKE_SERIAL: &str = "simrig-fake-serial";

pub trait Link: Send {
    /// Sends a request and waits, for up to `wait`, for the answer to `of` (group, device id,
    /// id): the value it carries. `None` when the device does not answer.
    fn exchange(
        &mut self,
        request: &[u8],
        of: (u8, u8, &[u8]),
        wait: Duration,
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
        (group, device, id): (u8, u8, &[u8]),
        wait: Duration,
    ) -> io::Result<Option<u64>> {
        // What came after the last answer was waited for is dropped: a late answer to another
        // device id must not pass for this one's.
        self.buf.clear();
        let _ = rustix::termios::tcflush(&self.file, rustix::termios::QueueSelector::IFlush);
        self.file.write_all(request)?;
        let deadline = Instant::now() + wait;
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
/// same decoding as real ones. A `wheel-id` line sets the rim's device id (23 when absent), a
/// `wheel-answers-as` line the id its answers carry.
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
        (group, device, id): (u8, u8, &[u8]),
        _wait: Duration,
    ) -> io::Result<Option<u64>> {
        let mut frame = protocol::parse_request(request)
            .ok_or_else(|| io::Error::other(format!("malformed request {request:02x?}")))?;
        let mut values = self.values();
        let setting = |name: &str| {
            values
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| *v as u8)
        };
        let wheel_id = setting("wheel-id").unwrap_or(23);
        let answers_as = setting("wheel-answers-as");
        let found = protocol::COMMANDS.iter().find_map(|c| {
            let at = match c.target {
                Target::Wheel => frame.device == wheel_id,
                other => other.ids().contains(&frame.device),
            };
            if !at {
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
        if command.target == Target::Wheel
            && let Some(as_id) = answers_as
        {
            frame.device = as_id;
        }
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
