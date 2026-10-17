//! The Moza serial protocol, as reverse engineered by Boxflat
//! (<https://github.com/Lawstorant/boxflat>, GPL-3.0, `moza-protocol.md` and `data/serial.yml`).
//!
//! A frame is `0x7e, length, group, device, command id…, value…, checksum`: `length` counts
//! the command id and the value, values are big-endian, and the checksum is the sum of every
//! byte before it plus 13, modulo 256. A device answers with the same frame, the group with
//! its top bit set and the device id's halves swapped; to a read, with the value stored.

pub const START: u8 = 0x7e;
const MAGIC: u32 = 13;

/// Who a command is for, on the serial port of the device it is sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The base's main controller (game effect gains).
    Main = 18,
    Base = 19,
    Pedals = 25,
    Handbrake = 27,
}

/// One setting: how to read it and how to write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    pub target: Target,
    /// Group and command id of a read; `None` when it cannot be read.
    pub read: Option<(u8, &'static [u8])>,
    /// Group and command id of a write; `None` when it cannot be written.
    pub write: Option<(u8, &'static [u8])>,
    /// Size of the value in bytes (an unsigned big-endian integer).
    pub bytes: usize,
}

const fn rw(name: &'static str, target: Target, read: u8, write: u8, id: &'static [u8]) -> Command {
    Command {
        name,
        target,
        read: Some((read, id)),
        write: Some((write, id)),
        bytes: 2,
    }
}

/// Settings stored by the base itself (read group 40, write group 41).
const fn base(name: &'static str, id: &'static [u8]) -> Command {
    rw(name, Target::Base, 40, 41, id)
}

/// Gains the base applies to the games' effects: separate read and write ids.
const fn gain(name: &'static str, read: &'static [u8], write: &'static [u8]) -> Command {
    Command {
        name,
        target: Target::Main,
        read: Some((31, read)),
        write: Some((31, write)),
        bytes: 1,
    }
}

pub const COMMANDS: &[Command] = &[
    // Half the rotation in degrees: the limit applied, and the widest the base allows games.
    base("limit", &[1]),
    base("max-angle", &[23]),
    // Tenths of a percent.
    base("ffb-strength", &[2]),
    base("inertia", &[4]),
    base("damper", &[7]),
    base("friction", &[8]),
    base("spring", &[9]),
    // Tenths of a percent.
    base("speed", &[10]),
    base("road-sensitivity", &[12]),
    base("protection", &[13]),
    base("torque", &[18]),
    base("natural-inertia", &[19]),
    base("speed-damping", &[25]),
    base("speed-damping-point", &[26]),
    base("soft-limit-strength", &[27]),
    base("soft-limit-stiffness", &[31]),
    Command {
        name: "calibration",
        target: Target::Base,
        read: None,
        write: Some((42, &[1])),
        bytes: 2,
    },
    gain("spring-gain", &[79, 8], &[78, 8]),
    gain("damper-gain", &[79, 9], &[78, 9]),
    gain("inertia-gain", &[79, 10], &[78, 10]),
    gain("friction-gain", &[79, 11], &[78, 11]),
];

pub fn command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

fn checksum(bytes: &[u8]) -> u8 {
    ((bytes.iter().map(|b| u32::from(*b)).sum::<u32>() + MAGIC) % 256) as u8
}

/// A request frame. Reads carry the value 1.
pub fn frame(group: u8, target: Target, id: &[u8], value: u64, bytes: usize) -> Vec<u8> {
    let mut out = vec![START, (id.len() + bytes) as u8, group, target as u8];
    out.extend_from_slice(id);
    out.extend_from_slice(&value.to_be_bytes()[8 - bytes..]);
    out.push(checksum(&out));
    out
}

pub fn read_frame(cmd: &Command) -> Option<Vec<u8>> {
    let (group, id) = cmd.read?;
    Some(frame(group, cmd.target, id, 1, cmd.bytes))
}

pub fn write_frame(cmd: &Command, value: u64) -> Option<Vec<u8>> {
    let (group, id) = cmd.write?;
    Some(frame(group, cmd.target, id, value, cmd.bytes))
}

/// A frame received: group, device and payload (command id and value), checksum checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub group: u8,
    pub device: u8,
    pub payload: Vec<u8>,
}

/// Takes the first whole answer out of `buf` (dropping anything before it); `None` until one
/// is complete. The checksum of answers is not checked (Boxflat does not either: how devices
/// compute it is not known for sure); frames that cannot be answers are skipped.
pub fn take_frame(buf: &mut Vec<u8>) -> Option<Frame> {
    loop {
        let start = buf.iter().position(|b| *b == START)?;
        buf.drain(..start);
        let len = usize::from(*buf.get(1)?);
        let plausible = (2..=11).contains(&len) && buf.get(2).is_none_or(|g| g & 0x80 != 0);
        if !plausible {
            // A 0x7e inside a value, not a start.
            buf.remove(0);
            continue;
        }
        let total = len + 5;
        if buf.len() < total {
            return None;
        }
        let frame: Vec<u8> = buf.drain(..total).collect();
        return Some(Frame {
            group: frame[2],
            device: frame[3],
            payload: frame[4..total - 1].to_vec(),
        });
    }
}

/// Parses a request frame (the fake device of tests), checksum checked.
pub fn parse_request(bytes: &[u8]) -> Option<Frame> {
    let len = usize::from(*bytes.get(1)?);
    let total = len + 5;
    if bytes.first() != Some(&START) || bytes.len() != total {
        return None;
    }
    (checksum(&bytes[..total - 1]) == bytes[total - 1]).then(|| Frame {
        group: bytes[2],
        device: bytes[3],
        payload: bytes[4..total - 1].to_vec(),
    })
}

fn swap_nibbles(b: u8) -> u8 {
    b.rotate_left(4)
}

/// The value a response carries for a request of (group, id) to `target`, if it is one.
pub fn answer(frame: &Frame, group: u8, target: Target, id: &[u8]) -> Option<u64> {
    if frame.group != group | 0x80 || frame.device != swap_nibbles(target as u8) {
        return None;
    }
    let value = frame.payload.strip_prefix(id)?;
    Some(value.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)))
}

/// The answer a device gives to a request frame, for the fake device of tests: the request
/// mirrored, with `value` when it is a read.
pub fn respond(request: &Frame, id_len: usize, value: Option<u64>) -> Vec<u8> {
    let mut payload = request.payload.clone();
    if let Some(value) = value {
        let bytes = payload.len() - id_len;
        payload.truncate(id_len);
        payload.extend_from_slice(&value.to_be_bytes()[8 - bytes..]);
    }
    let mut out = vec![
        START,
        payload.len() as u8,
        request.group | 0x80,
        swap_nibbles(request.device),
    ];
    out.extend(payload);
    out.push(checksum(&out));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_like_boxflat() {
        // Rotation 900°: limit 450 to the base.
        let f = write_frame(command("limit").unwrap(), 450).unwrap();
        assert_eq!(f[..7], [0x7e, 3, 41, 19, 1, 0x01, 0xc2]);
        let sum: u32 = f[..7].iter().map(|b| u32::from(*b)).sum::<u32>() + 13;
        assert_eq!(f[7], (sum % 256) as u8);
        // A game gain: one byte, a two-byte command id.
        let f = read_frame(command("damper-gain").unwrap()).unwrap();
        assert_eq!(f[..7], [0x7e, 3, 31, 18, 79, 9, 1]);
    }

    #[test]
    fn answers_are_matched_and_decoded() {
        let request = write_frame(command("limit").unwrap(), 1).unwrap();
        let req = parse_request(&request).unwrap();
        // Noise first, with a stray start byte.
        let mut stream = vec![0x00, 0x7e, 0x01];
        stream.extend(respond(&req, 1, Some(540)));
        let frame = take_frame(&mut stream).unwrap();
        assert_eq!(frame.group, 41 | 0x80);
        assert_eq!(frame.device, 0x31);
        assert_eq!(answer(&frame, 41, Target::Base, &[1]), Some(540));
        assert_eq!(answer(&frame, 40, Target::Base, &[1]), None);
        assert_eq!(answer(&frame, 41, Target::Base, &[2]), None);
        assert!(stream.is_empty());
    }
}
