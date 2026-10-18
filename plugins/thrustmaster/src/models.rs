//! The Thrustmaster wheels SimRig knows: the ones hid-tmff2 drives (`src/hid-tmff2.h`), the T150
//! and TMX hid-t150 drives, the ones without a force feedback driver on Linux, and the ids
//! wheels have while they start up, before hid-tminit switches them to their own.

use simrig_plugin_kit::pedals::Layout;
use simrig_plugin_sdk::prelude::PedalRole::{Brake, Clutch, Throttle};

/// Where the pedals plugged into a base are, at the top of their travel when released
/// (Oversteer's tables).
const Z_RZ_Y: Layout = &[
    (Throttle, 0x02, true),
    (Brake, 0x05, true),
    (Clutch, 0x01, true),
];
/// The T248's: throttle ABS_RZ, brake ABS_Y, clutch ABS_THROTTLE.
const T248: Layout = &[
    (Throttle, 0x05, true),
    (Brake, 0x01, true),
    (Clutch, 0x06, true),
];

/// The kernel driver that drives a wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    /// hid-tmff2 (SimRig's, or one installed by other means).
    Tmff2,
    /// hid-t150, for the T150 and TMX.
    T150,
    /// None on Linux: hid-generic gives the inputs only. `why` says what is missing.
    Generic { why: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A wheel hid-tmff2 drives; rotation from `min_range` to `max_range` degrees.
    Wheel { min_range: u32, max_range: u32 },
    /// A wheel starting up: hid-tminit switches it to its own id.
    StartingUp,
}

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
    pub kind: Kind,
    /// Where its pedals are, when known.
    pub pedals: Option<Layout>,
    pub driver: Driver,
}

pub const VENDOR: u16 = 0x044f;

const fn wheel(
    pid: u16,
    name: &'static str,
    model: &'static str,
    min_range: u32,
    max_range: u32,
) -> Model {
    Model {
        pid,
        name,
        model,
        kind: Kind::Wheel {
            min_range,
            max_range,
        },
        pedals: Some(Z_RZ_Y),
        driver: Driver::Tmff2,
    }
}

/// A wheel with a fixed rotation and no force feedback driver on Linux; its pedals are mapped
/// when it is added.
const fn generic(
    pid: u16,
    name: &'static str,
    model: &'static str,
    range: u32,
    why: &'static str,
) -> Model {
    Model {
        pedals: None,
        driver: Driver::Generic { why },
        ..wheel(pid, name, model, range, range)
    }
}

const NO_FFB_DRIVER: &str = "No Linux driver gives this wheel force feedback yet: hid-tmff2 does \
                             not drive it.";
pub const NO_FFB: &str = "The wheel has no force feedback.";

const fn starting(pid: u16) -> Model {
    Model {
        pid,
        name: "Thrustmaster wheel (starting up)",
        model: "thrustmaster/wheel",
        kind: Kind::StartingUp,
        pedals: None,
        driver: Driver::Tmff2,
    }
}

pub const MODELS: &[Model] = &[
    wheel(0xb66e, "T300 RS", "thrustmaster/t300rs", 40, 1080),
    // With the F1 rim, in its advanced mode.
    wheel(
        0xb66f,
        "T300 RS (advanced mode)",
        "thrustmaster/t300rs",
        40,
        1080,
    ),
    // Also the T-GT II, which reuses the id.
    wheel(
        0xb66d,
        "T300 RS (PS4 mode)",
        "thrustmaster/t300rs",
        40,
        1080,
    ),
    // Also the T128.
    Model {
        pedals: Some(T248),
        ..wheel(0xb696, "T248", "thrustmaster/t248", 140, 900)
    },
    wheel(0xb669, "TX Racing Wheel", "thrustmaster/tx", 140, 900),
    wheel(0xb689, "TS-PC Racer", "thrustmaster/ts-pc", 140, 1080),
    wheel(0xb692, "TS-XW Racer", "thrustmaster/ts-xw", 140, 1080),
    // hid-t150's.
    Model {
        pedals: Some(T248),
        driver: Driver::T150,
        ..wheel(0xb677, "T150", "thrustmaster/t150", 270, 1080)
    },
    // hid-tminit does not start it up: it must already be in its PC mode.
    Model {
        pedals: Some(T248),
        driver: Driver::T150,
        ..wheel(0xb67f, "TMX", "thrustmaster/tmx", 270, 900)
    },
    // Inputs only.
    generic(
        0xb65e,
        "T500 RS",
        "thrustmaster/t500rs",
        1080,
        NO_FFB_DRIVER,
    ),
    generic(0xb668, "T80 Racing Wheel", "thrustmaster/t80", 270, NO_FFB),
    // Oversteer's T80H.
    generic(
        0xb66a,
        "T80 Racing Wheel (H)",
        "thrustmaster/t80h",
        270,
        NO_FFB,
    ),
    // hid-tminit and usb-tminit (deps/hid-tminit).
    starting(0xb65d),
    starting(0xb664),
    starting(0xb69c),
    starting(0xb691),
    starting(0xb6a5),
];

pub fn model(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}
