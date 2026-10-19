//! What Fanatec's buttons are. The firmware numbers the controls of every rim, and of the
//! shifters plugged into the base, in one index space; hid-fanatecff's optional key map
//! (`ftec_keymap` in `hid-ftec.c`) documents it. With the driver's default mapping, HID button
//! `n` becomes the code `BTN_JOYSTICK + n - 1` up to `BTN_DEAD`, then `KEY_MACRO1 + n - 16`.
//!
//! The names say the function, not what is printed on a given rim, and are not confirmed on
//! every rim: the indices the driver marks unknown are left without a name.

/// HID button index → name.
const NAMES: &[(u16, &str)] = &[
    (0x01, "Square"),
    (0x02, "Cross"),
    (0x03, "Circle"),
    (0x04, "Triangle"),
    (0x05, "Gear up"),
    (0x06, "Gear down"),
    (0x07, "R2"),
    (0x08, "L2"),
    (0x09, "Share"),
    (0x0a, "Options"),
    (0x0b, "R3"),
    (0x0c, "L3"),
    (0x0d, "Shifter R"),
    (0x0e, "Shifter 1"),
    (0x0f, "Shifter 2"),
    (0x10, "Shifter 3"),
    (0x11, "Shifter 4"),
    (0x12, "Shifter 5"),
    (0x13, "Shifter 6"),
    (0x14, "Shifter 7"),
    (0x16, "PS / Xbox"),
    (0x17, "Funky twist left"),
    (0x18, "Funky twist right"),
    (0x19, "Funky push"),
    (0x1a, "Stick push"),
    (0x1b, "Left toggle up"),
    (0x1d, "Sequential down"),
    (0x1e, "Sequential up"),
    (0x1f, "Right toggle down"),
    (0x20, "Left toggle down"),
    (0x21, "Right toggle up (normal)"),
    (0x22, "Left toggle up (normal)"),
    (0x25, "Left knob right"),
    (0x26, "Left knob left"),
    (0x27, "Left knob 3"),
    (0x28, "Left knob 4"),
    (0x29, "Left knob 5"),
    (0x2a, "Left knob 6"),
    (0x2b, "Left knob 7"),
    (0x2c, "Left knob 8"),
    (0x2d, "Left knob 9"),
    (0x2e, "Left knob 10"),
    (0x2f, "Left knob 11"),
    (0x30, "Left knob 0"),
    (0x31, "Right knob right"),
    (0x32, "Right knob left"),
    (0x33, "Right knob 3"),
    (0x34, "Right knob 4"),
    (0x35, "Right knob 5"),
    (0x36, "Right knob 6"),
    (0x37, "Right knob 7"),
    (0x38, "Right knob 8"),
    (0x39, "Right knob 9"),
    (0x3a, "Right knob 10"),
    (0x3b, "Right knob 11"),
    (0x3c, "Right knob 0"),
    (0x3d, "Left paddle"),
    (0x3e, "Right paddle"),
];

const BTN_JOYSTICK: u16 = 0x120;
const BTN_DEAD: u16 = 0x12f;
const KEY_MACRO1: u16 = 0x290;

/// The code the driver's default mapping gives HID button `n` (from 1).
pub fn code(n: u16) -> u16 {
    let code = BTN_JOYSTICK + n - 1;
    if code > BTN_DEAD {
        code - BTN_DEAD + KEY_MACRO1
    } else {
        code
    }
}

/// The names by code, when the device's buttons follow the driver's default mapping; none
/// otherwise (`custom_button_mapping`, or a driver that numbers them another way).
pub fn labels(codes: &[u16]) -> Vec<(u16, &'static str)> {
    let default = codes
        .iter()
        .all(|&c| (BTN_JOYSTICK..=BTN_DEAD).contains(&c) || c > KEY_MACRO1);
    if !default {
        return Vec::new();
    }
    NAMES.iter().map(|&(n, name)| (code(n), name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drivers_default_mapping() {
        assert_eq!(code(1), 0x120);
        assert_eq!(code(16), 0x12f);
        // KEY_MACRO1 is skipped: button 17 is KEY_MACRO2.
        assert_eq!(code(17), 0x291);
        assert_eq!(code(0x16), 0x296);
    }

    #[test]
    fn names_only_with_the_default_mapping() {
        let labels = labels(&[0x120, 0x121, 0x296]);
        assert!(labels.contains(&(0x120, "Square")));
        assert!(labels.contains(&(0x296, "PS / Xbox")));
        // The optional key map gives other codes (BTN_WEST, KEY_F1…).
        assert!(super::labels(&[0x134, 0x3b]).is_empty());
    }
}
