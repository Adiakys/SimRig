//! The Logitech wheels SimRig knows, by USB product id, and the kernel driver family each one
//! is handled by.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `hid-logitech` (lg4ff): the kernel's, or new-lg4ff, which the driver builds for these
    /// wheels (more effects, gain, autocenter, effect levels). HID driver `logitech`.
    Lg4ff,
    /// `hid-logitech-hidpp`, the kernel's: G920 and G923 for Xbox. HID driver
    /// `logitech-hidpp-device`.
    Hidpp,
    /// The TrueForce driver (`hid-logitech-dd`), which the driver builds for these direct drive
    /// bases. HID driver `logitech-dd`.
    DirectDrive,
    /// The oldest wheels: a fixed rotation, on the kernel's hid-logitech (no force feedback) or
    /// iforce (a USB driver; force feedback, its strength and centering spring through the input
    /// interface).
    Classic,
}

use simrig_plugin_kit::pedals::Layout;
use simrig_plugin_sdk::prelude::PedalRole::{Brake, Clutch, Throttle};

/// Where the pedals plugged into each family of wheels are, at the top of their travel when
/// released (Oversteer's tables).
const Z_RZ_Y: Layout = &[
    (Throttle, 0x02, true),
    (Brake, 0x05, true),
    (Clutch, 0x01, true),
];
const Y_Z_RZ: Layout = &[
    (Throttle, 0x01, true),
    (Brake, 0x02, true),
    (Clutch, 0x05, true),
];
/// Two pedals, no clutch.
const Y_Z: Layout = &[(Throttle, 0x01, true), (Brake, 0x02, true)];
/// The G PRO's, at the bottom of their travel when released.
const RX_RY_RZ: Layout = &[
    (Throttle, 0x03, false),
    (Brake, 0x04, false),
    (Clutch, 0x05, false),
];

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
    pub family: Family,
    /// Widest rotation, in degrees.
    pub max_range: u32,
    /// Where its pedals are, when known.
    pub pedals: Option<Layout>,
}

pub const VENDOR: u16 = 0x046d;

const fn wheel(
    pid: u16,
    name: &'static str,
    model: &'static str,
    family: Family,
    max_range: u32,
    pedals: Option<Layout>,
) -> Model {
    Model {
        pid,
        name,
        model,
        family,
        max_range,
        pedals,
    }
}

use Family::*;

pub const MODELS: &[Model] = &[
    wheel(
        0xc20e,
        "WingMan Formula GP",
        "logitech/wingman-formula-gp",
        Classic,
        180,
        None,
    ),
    wheel(
        0xc291,
        "WingMan Formula Force",
        "logitech/wingman-formula-force",
        Classic,
        180,
        None,
    ),
    wheel(
        0xc293,
        "WingMan Formula Force GP",
        "logitech/wingman-ffgp",
        Lg4ff,
        180,
        None,
    ),
    // Also a G29 or G27 in compatibility mode, before lg4ff switches it to its own mode.
    wheel(
        0xc294,
        "Driving Force",
        "logitech/driving-force",
        Lg4ff,
        270,
        Some(Y_Z),
    ),
    wheel(
        0xc295,
        "MOMO Racing",
        "logitech/momo-racing",
        Lg4ff,
        270,
        Some(Y_Z),
    ),
    wheel(
        0xc298,
        "Driving Force Pro",
        "logitech/driving-force-pro",
        Lg4ff,
        900,
        Some(Y_Z),
    ),
    wheel(
        0xc299,
        "G25 Racing Wheel",
        "logitech/g25",
        Lg4ff,
        900,
        Some(Z_RZ_Y),
    ),
    wheel(
        0xc29a,
        "Driving Force GT",
        "logitech/driving-force-gt",
        Lg4ff,
        900,
        Some(Y_Z),
    ),
    wheel(
        0xc29b,
        "G27 Racing Wheel",
        "logitech/g27",
        Lg4ff,
        900,
        Some(Z_RZ_Y),
    ),
    wheel(
        0xc29c,
        "Speed Force Wireless",
        "logitech/speed-force-wireless",
        Lg4ff,
        270,
        Some(Y_Z),
    ),
    wheel(
        0xca03,
        "MOMO Racing",
        "logitech/momo-racing-2",
        Lg4ff,
        270,
        Some(Y_Z),
    ),
    wheel(
        0xc24f,
        "G29 Driving Force",
        "logitech/g29",
        Lg4ff,
        900,
        Some(Z_RZ_Y),
    ),
    wheel(
        0xc266,
        "G923 Racing Wheel (PlayStation)",
        "logitech/g923-ps",
        Lg4ff,
        900,
        Some(Z_RZ_Y),
    ),
    // The G923 in PlayStation mode: new-lg4ff switches it to c266.
    wheel(
        0xc267,
        "G923 Racing Wheel (PlayStation)",
        "logitech/g923-ps",
        Lg4ff,
        900,
        Some(Z_RZ_Y),
    ),
    wheel(
        0xc262,
        "G920 Driving Force",
        "logitech/g920",
        Hidpp,
        900,
        Some(Y_Z_RZ),
    ),
    wheel(
        0xc26d,
        "G923 Racing Wheel (Xbox)",
        "logitech/g923-xbox",
        Hidpp,
        900,
        Some(Y_Z_RZ),
    ),
    wheel(
        0xc26e,
        "G923 Racing Wheel (Xbox)",
        "logitech/g923-xbox",
        Hidpp,
        900,
        Some(Y_Z_RZ),
    ),
    wheel(
        0xc268,
        "G PRO Racing Wheel (PlayStation)",
        "logitech/g-pro-ps",
        DirectDrive,
        1080,
        Some(RX_RY_RZ),
    ),
    wheel(
        0xc272,
        "G PRO Racing Wheel",
        "logitech/g-pro",
        DirectDrive,
        1080,
        Some(RX_RY_RZ),
    ),
    // Not in Oversteer's tables: its pedals are mapped when it is added.
    wheel(
        0xc276,
        "RS50 Direct Drive Wheel Base",
        "logitech/rs50",
        DirectDrive,
        2700,
        None,
    ),
];

pub fn model(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

/// The kernel's HID driver name for a family.
pub fn hid_driver(family: Family) -> &'static str {
    match family {
        Lg4ff => "logitech",
        Hidpp => "logitech-hidpp-device",
        DirectDrive => "logitech-dd",
        Classic => "logitech",
    }
}
