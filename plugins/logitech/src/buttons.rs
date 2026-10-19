//! What Logitech's buttons are, per model: the rims are part of the wheels, so a model has one
//! layout. From the tables of the TrueForce driver project (`evtest.rs`,
//! `docs/BUTTON_MAPPING.md` in `driver/logitech-trueforce`), captured on the wheels by
//! pressing every button in turn: the RS50 (and the G PRO, which shares its layout), the G923
//! for PlayStation and the G923 for Xbox. The G29 has the G923 PlayStation's layout, and the
//! G920 the G923 Xbox's first eleven buttons and shifter.
//!
//! Codes are the kernel's default sequential mapping of the HID buttons, which hid-logitech,
//! new-lg4ff, hid-logitech-hidpp and the TrueForce driver all keep.

/// RS50 and G PRO.
const DIRECT_DRIVE: &[(u16, &str)] = &[
    (0x120, "A"),
    (0x121, "X"),
    (0x122, "B"),
    (0x123, "Y"),
    (0x124, "Right paddle"),
    (0x125, "Left paddle"),
    (0x126, "RT"),
    (0x127, "LT"),
    (0x128, "View"),
    (0x129, "Menu"),
    (0x12a, "RSB"),
    (0x12b, "LSB"),
    (0x2c5, "Right dial right"),
    (0x2c6, "Right dial left"),
    (0x2c7, "Right dial push"),
    (0x2c8, "Left dial right"),
    (0x2c9, "Left dial left"),
    (0x2ca, "Left dial push"),
    (0x2cb, "G1"),
    (0x2cc, "GL"),
    (0x2cd, "GR"),
];

/// G923 for PlayStation, and the G29.
const PLAYSTATION: &[(u16, &str)] = &[
    (0x120, "Cross"),
    (0x121, "Square"),
    (0x122, "Circle"),
    (0x123, "Triangle"),
    (0x124, "Right paddle"),
    (0x125, "Left paddle"),
    (0x126, "R2"),
    (0x127, "L2"),
    (0x128, "Share"),
    (0x129, "Options"),
    (0x12a, "R3"),
    (0x12b, "L3"),
    (0x2c3, "Plus"),
    (0x2c4, "Minus"),
    (0x2c5, "Dial right"),
    (0x2c6, "Dial left"),
    (0x2c7, "Dial push"),
    (0x2c8, "PS"),
];

/// G923 for Xbox, and the G920 (without the plus, minus and dial).
const XBOX: &[(u16, &str)] = &[
    (0x120, "A"),
    (0x121, "B"),
    (0x122, "X"),
    (0x123, "Y"),
    (0x124, "Right paddle"),
    (0x125, "Left paddle"),
    (0x126, "Menu"),
    (0x127, "View"),
    (0x128, "RSB"),
    (0x129, "LSB"),
    (0x12a, "Xbox"),
    // The H-shifter, on the wheel's own button page.
    (0x12b, "Shifter R"),
    (0x12c, "Shifter 1"),
    (0x12d, "Shifter 2"),
    (0x12e, "Shifter 3"),
    (0x12f, "Shifter 4"),
    (0x2c0, "Shifter 5"),
    (0x2c1, "Shifter 6"),
    (0x2c2, "Plus"),
    (0x2c3, "Minus"),
    (0x2c4, "Dial right"),
    (0x2c5, "Dial left"),
    (0x2c6, "Dial push"),
];

/// The names of a model's buttons, by code; none for the models no table covers.
pub fn labels(pid: u16) -> &'static [(u16, &'static str)] {
    match pid {
        // G PRO (PlayStation, Xbox/PC), RS50.
        0xc268 | 0xc272 | 0xc276 => DIRECT_DRIVE,
        // G29, G923 for PlayStation.
        0xc24f | 0xc266 | 0xc267 => PLAYSTATION,
        // G920, G923 for Xbox.
        0xc262 | 0xc26e => XBOX,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MODELS;

    /// Every table names a model SimRig knows.
    #[test]
    fn tables_name_known_models() {
        for pid in [
            0xc268, 0xc272, 0xc276, 0xc24f, 0xc266, 0xc267, 0xc262, 0xc26e,
        ] {
            assert!(MODELS.iter().any(|m| m.pid == pid), "{pid:04x}");
            assert!(!labels(pid).is_empty());
        }
        assert!(labels(0xc29b).is_empty());
    }
}
