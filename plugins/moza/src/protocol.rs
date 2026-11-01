//! The Moza serial protocol, as reverse engineered by Boxflat
//! (<https://github.com/Lawstorant/boxflat>, GPL-3.0, `moza-protocol.md` and `data/serial.yml`).
//! Boxflat is pinned in `plugins/moza/boxflat`: the tests check every command here against its
//! table.
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
    Main,
    Base,
    /// The rim mounted on the base.
    Wheel,
    /// A shifter plugged into the base (the SGP's and the HGP's share an id).
    Shifter,
    /// A handbrake plugged into the base.
    Handbrake,
}

impl Target {
    /// The device ids it answers at, the usual one first: most rims answer at 23, others at 21
    /// or 19 (Boxflat tries them in turn).
    pub fn ids(self) -> &'static [u8] {
        match self {
            Target::Main => &[18],
            Target::Base => &[19],
            Target::Wheel => &[23, 21, 19],
            Target::Shifter => &[26],
            Target::Handbrake => &[27],
        }
    }

    /// What answers, for messages.
    pub fn noun(self) -> &'static str {
        match self {
            Target::Main | Target::Base => "base",
            Target::Wheel => "steering wheel",
            Target::Shifter => "shifter",
            Target::Handbrake => "handbrake",
        }
    }
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

/// Settings stored by the rim (read group 64, write group 63), one byte.
const fn wheel(name: &'static str, id: &'static [u8]) -> Command {
    Command {
        bytes: 1,
        ..rw(name, Target::Wheel, 64, 63, id)
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
    // Answered by every rim: Boxflat reads it to know one is mounted.
    wheel("wheel-telemetry-mode", &[28, 0]),
    wheel("wheel-paddles-mode", &[3]),
    // Percent of the combined axis given to the left paddle.
    wheel("wheel-clutch-point", &[9]),
    wheel("wheel-stick-mode", &[5]),
    // Each rotary encoder's.
    wheel("wheel-knob0-mode", &[42, 0]),
    wheel("wheel-knob1-mode", &[42, 1]),
    wheel("wheel-knob2-mode", &[42, 2]),
    wheel("wheel-knob3-mode", &[42, 3]),
    wheel("wheel-knob4-mode", &[42, 4]),
    wheel("wheel-rpm-brightness", &[27, 0, 255]),
    wheel("wheel-buttons-brightness", &[27, 1, 255]),
    // Parts plugged into the base: answering says they are there (Boxflat).
    Command {
        name: "sequential-output-y",
        target: Target::Shifter,
        read: Some((83, &[2])),
        write: None,
        bytes: 2,
    },
    rw("sequential-direction", Target::Shifter, 81, 82, &[5]),
    rw("sequential-paddle-sync", Target::Shifter, 81, 82, &[6]),
    rw("handbrake-direction", Target::Handbrake, 91, 92, &[1]),
];

pub fn command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

fn checksum(bytes: &[u8]) -> u8 {
    ((bytes.iter().map(|b| u32::from(*b)).sum::<u32>() + MAGIC) % 256) as u8
}

/// A request frame to the device id `device`. Reads carry the value 1.
pub fn frame(group: u8, device: u8, id: &[u8], value: u64, bytes: usize) -> Vec<u8> {
    let mut out = vec![START, (id.len() + bytes) as u8, group, device];
    out.extend_from_slice(id);
    out.extend_from_slice(&value.to_be_bytes()[8 - bytes..]);
    out.push(checksum(&out));
    out
}

pub fn read_frame(cmd: &Command, device: u8) -> Option<Vec<u8>> {
    let (group, id) = cmd.read?;
    Some(frame(group, device, id, 1, cmd.bytes))
}

pub fn write_frame(cmd: &Command, device: u8, value: u64) -> Option<Vec<u8>> {
    let (group, id) = cmd.write?;
    Some(frame(group, device, id, value, cmd.bytes))
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

/// The value a response carries for a request of (group, id) to `device`, if it is one. A rim's
/// answers (groups 63 to 66) count whatever device id they carry: some rims answer with the
/// base's or the main controller's, and Boxflat takes them by their group.
pub fn answer(frame: &Frame, group: u8, device: u8, id: &[u8]) -> Option<u64> {
    let rim = (63..=66).contains(&group);
    if frame.group != group | 0x80 || (!rim && frame.device != swap_nibbles(device)) {
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
        let f = write_frame(command("limit").unwrap(), 19, 450).unwrap();
        assert_eq!(f[..7], [0x7e, 3, 41, 19, 1, 0x01, 0xc2]);
        let sum: u32 = f[..7].iter().map(|b| u32::from(*b)).sum::<u32>() + 13;
        assert_eq!(f[7], (sum % 256) as u8);
        // A game gain: one byte, a two-byte command id.
        let f = read_frame(command("damper-gain").unwrap(), 18).unwrap();
        assert_eq!(f[..7], [0x7e, 3, 31, 18, 79, 9, 1]);
        // A rim setting, to the rim's device id.
        let f = read_frame(command("wheel-paddles-mode").unwrap(), 23).unwrap();
        assert_eq!(f[..6], [0x7e, 2, 64, 23, 3, 1]);
    }

    #[test]
    fn answers_are_matched_and_decoded() {
        let request = write_frame(command("limit").unwrap(), 19, 1).unwrap();
        let req = parse_request(&request).unwrap();
        // Noise first, with a stray start byte.
        let mut stream = vec![0x00, 0x7e, 0x01];
        stream.extend(respond(&req, 1, Some(540)));
        let frame = take_frame(&mut stream).unwrap();
        assert_eq!(frame.group, 41 | 0x80);
        assert_eq!(frame.device, 0x31);
        assert_eq!(answer(&frame, 41, 19, &[1]), Some(540));
        assert_eq!(answer(&frame, 40, 19, &[1]), None);
        assert_eq!(answer(&frame, 41, 19, &[2]), None);
        assert_eq!(answer(&frame, 41, 23, &[1]), None);
        assert!(stream.is_empty());
    }

    #[test]
    fn a_rims_answer_counts_whatever_id_it_carries() {
        let request = read_frame(command("wheel-paddles-mode").unwrap(), 23).unwrap();
        let mut req = parse_request(&request).unwrap();
        // Answered as the base.
        req.device = 19;
        let mut stream = respond(&req, 1, Some(2));
        let frame = take_frame(&mut stream).unwrap();
        assert_eq!(answer(&frame, 64, 23, &[3]), Some(2));
        assert_eq!(answer(&frame, 64, 23, &[9]), None);
    }

    /// A command of Boxflat's table: read group, write group (-1 when it cannot), id, bytes.
    type Entry = (i64, i64, Vec<u8>, usize);

    /// Boxflat's command table (`data/serial.yml` of the pinned submodule), by device and name.
    fn boxflat() -> std::collections::BTreeMap<(String, String), Entry> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/boxflat/data/serial.yml");
        let text = std::fs::read_to_string(path).expect("the Boxflat submodule");
        let mut table = std::collections::BTreeMap::new();
        let (mut device, mut name) = (String::new(), String::new());
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim_end();
            let indent = line.len() - line.trim_start().len();
            let Some((key, value)) = line.trim().split_once(':') else {
                continue;
            };
            let value = value.trim();
            match indent {
                2 if value.is_empty() => device = key.to_string(),
                4 if value.is_empty() => {
                    name = key.to_string();
                    table.insert((device.clone(), name.clone()), (-1, -1, Vec::new(), 0));
                }
                6 => {
                    let Some(entry) = table.get_mut(&(device.clone(), name.clone())) else {
                        continue;
                    };
                    match key {
                        "read" => entry.0 = value.parse().unwrap(),
                        "write" => entry.1 = value.parse().unwrap(),
                        "bytes" => entry.3 = value.parse().unwrap(),
                        "id" => {
                            entry.2 = value
                                .trim_matches(['[', ']'])
                                .split(',')
                                .map(|b| b.trim().parse().unwrap())
                                .collect()
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        table
    }

    #[test]
    fn every_command_is_boxflats() {
        let table = boxflat();
        for c in COMMANDS {
            // Boxflat's name for the target: commands of the parts are named after it.
            let device = match c.target {
                Target::Main => "main",
                Target::Base => "base",
                Target::Wheel => "wheel",
                Target::Shifter => "sequential",
                Target::Handbrake => "handbrake",
            };
            let name = c.name.strip_prefix(&format!("{device}-")).unwrap_or(c.name);
            let entry = |name: String| {
                table
                    .get(&(device.to_string(), name.clone()))
                    .unwrap_or_else(|| panic!("{device}-{name} is not in Boxflat's table"))
            };
            // Commands Boxflat splits in two: `get-` reads, `set-` writes.
            let (read, write) = if table.contains_key(&(device.to_string(), name.to_string())) {
                (entry(name.into()), entry(name.into()))
            } else {
                (entry(format!("get-{name}")), entry(format!("set-{name}")))
            };
            let as_ours =
                |group: i64, id: &Vec<u8>| (group >= 0).then(|| (group as u8, id.clone()));
            let ours = |rw: Option<(u8, &[u8])>| rw.map(|(g, id)| (g, id.to_vec()));
            assert_eq!(ours(c.read), as_ours(read.0, &read.2), "{} read", c.name);
            assert_eq!(
                ours(c.write),
                as_ours(write.1, &write.2),
                "{} write",
                c.name
            );
            assert_eq!(c.bytes, read.3, "{} bytes", c.name);
        }
    }
}
