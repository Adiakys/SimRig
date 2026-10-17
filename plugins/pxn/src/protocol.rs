//! The vendor control protocol of PXN VD wheel bases, as reverse engineered and verified on a
//! VD6 by Pitwall (<https://github.com/unionyxx/pitwall>, GPL-3.0, `pitwall/protocol.py`).
//!
//! It runs over the base's vendor HID interface (usage page 0xff00): 63-byte reports, report
//! id 1, values little-endian. Every command is answered with a report that starts like the
//! request; writes are acknowledged with `01 41 e0`. The base also streams its steering
//! position (`01 21 11 …`) about 100 times a second, which is skipped.

/// Bytes of a report, report id included.
pub const REPORT_SIZE: usize = 63;

/// Opens a session; the base echoes it.
pub const HANDSHAKE: [u8; 3] = [0x01, 0xfd, 0x0b];
pub const STREAM: [u8; 3] = [0x01, 0x21, 0x11];
pub const BASE_READ: [u8; 4] = [0x01, 0x81, 0xe0, 0x09];
pub const BASE_WRITE: [u8; 4] = [0x01, 0x41, 0xe0, 0x09];
pub const WRITE_ACK: [u8; 3] = [0x01, 0x41, 0xe0];

/// The base settings block (`StVdBaseParameter`): the steering angle as a little-endian u16,
/// then one byte per field.
pub const BASE_SIZE: usize = 31;

/// Where each base setting is in the block, by Pitwall's name. Written by read-modify-write,
/// so fields SimRig does not show are kept.
pub const FIELDS: &[&str] = &[
    "max_steering_angle", // 2 bytes
    "overall_ffb",
    "spring_force",
    "damper_force",
    "sine_force",
    "inertia_force",
    "max_torque",
    "max_speed",
    "center_force",
    "mechanical_damper",
    "road_feel",
    "inertia",
    "ffb_reverse",
    "stop_force",
    "hands_off_protection",
    "road_feel_eq_low",
    "gain_10hz",
    "gain_15hz",
    "gain_25hz",
    "gain_40hz",
    "gain_60hz",
    "gain_100hz",
    "reserved27",
    "reserved28",
    "reserved29",
    "filter_level",
    "road_feel_mode",
    "mechanical_friction",
    "speed_percent",
    "game_friction",
];

/// A request padded to a whole report.
pub fn report(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(REPORT_SIZE, 0);
    out
}

/// The base settings block of a read's answer.
pub fn base_block(answer: &[u8]) -> Option<[u8; BASE_SIZE]> {
    answer
        .strip_prefix(&BASE_READ[..])?
        .get(..BASE_SIZE)?
        .try_into()
        .ok()
}

/// A field's value in the block.
pub fn get(block: &[u8; BASE_SIZE], field: &str) -> Option<u16> {
    match FIELDS.iter().position(|f| *f == field)? {
        0 => Some(u16::from_le_bytes([block[0], block[1]])),
        i => Some(u16::from(block[i + 1])),
    }
}

/// Sets a field (bytes saturate at 255).
pub fn set(block: &mut [u8; BASE_SIZE], field: &str, value: u16) -> bool {
    match FIELDS.iter().position(|f| *f == field) {
        Some(0) => {
            block[..2].copy_from_slice(&value.to_le_bytes());
            true
        }
        Some(i) => {
            block[i + 1] = value.min(255) as u8;
            true
        }
        None => false,
    }
}

/// The write request for a block.
pub fn base_write(block: &[u8; BASE_SIZE]) -> Vec<u8> {
    let mut out = BASE_WRITE.to_vec();
    out.extend_from_slice(block);
    report(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_31_bytes() {
        // The angle takes two bytes, every other field one.
        assert_eq!(FIELDS.len() + 1, BASE_SIZE);
    }

    #[test]
    fn fields_round_trip() {
        let mut block = [0u8; BASE_SIZE];
        assert!(set(&mut block, "max_steering_angle", 900));
        assert!(set(&mut block, "game_friction", 40));
        assert_eq!(block[..2], [0x84, 0x03]);
        assert_eq!(block[BASE_SIZE - 1], 40);
        assert_eq!(get(&block, "max_steering_angle"), Some(900));
        assert_eq!(get(&block, "overall_ffb"), Some(0));
        assert!(!set(&mut block, "no_such_field", 1));
        let write = base_write(&block);
        assert_eq!(write.len(), REPORT_SIZE);
        assert_eq!(write[..6], [0x01, 0x41, 0xe0, 0x09, 0x84, 0x03]);
        let mut answer = BASE_READ.to_vec();
        answer.extend_from_slice(&block);
        assert_eq!(base_block(&report(&answer)), Some(block));
    }
}
