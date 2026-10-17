//! The Thrustmaster wheels hid-tmff2 drives (`src/hid-tmff2.h`), and the ids wheels have while
//! they start up, before hid-tminit switches them to their own.

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
    }
}

const fn starting(pid: u16) -> Model {
    Model {
        pid,
        name: "Thrustmaster wheel (starting up)",
        model: "thrustmaster/wheel",
        kind: Kind::StartingUp,
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
    wheel(0xb696, "T248", "thrustmaster/t248", 140, 900),
    wheel(0xb669, "TX Racing Wheel", "thrustmaster/tx", 140, 900),
    wheel(0xb689, "TS-PC Racer", "thrustmaster/ts-pc", 140, 1080),
    wheel(0xb692, "TS-XW Racer", "thrustmaster/ts-xw", 140, 1080),
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
