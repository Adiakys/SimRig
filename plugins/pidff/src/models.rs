//! The bases SimRig knows: the wheels in hid-universal-pidff's table (`hid-ids.h`) that no brand
//! plugin of SimRig handles, and the OpenFFBoard, which the kernel's generic driver drives.

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
    /// The rotation the steering angle is shown over: the base's own setting cannot be read,
    /// so this is its usual one.
    pub range: u32,
}

const fn base(vid: u16, pid: u16, name: &'static str, model: &'static str, range: u32) -> Model {
    Model {
        vid,
        pid,
        name,
        model,
        range,
    }
}

pub const MODELS: &[Model] = &[
    base(0x3416, 0x0301, "Cammus C5", "cammus/c5", 1080),
    base(0x3416, 0x0302, "Cammus C12", "cammus/c12", 1080),
    base(0x2433, 0xf300, "Asetek Invicta", "asetek/invicta", 900),
    base(0x2433, 0xf301, "Asetek Forte", "asetek/forte", 900),
    base(0x2433, 0xf303, "Asetek La Prima", "asetek/la-prima", 900),
    base(
        0x2433,
        0xf306,
        "Asetek Tony Kanaan",
        "asetek/tony-kanaan",
        900,
    ),
    base(
        0x0483,
        0xa355,
        "VRS DirectForce Pro",
        "vrs/directforce-pro",
        900,
    ),
    base(0x045b, 0x59d7, "FFBeast Wheel", "ffbeast/wheel", 900),
    base(
        0x10f5,
        0x7077,
        "Turtle Beach VelocityOne Race",
        "turtle-beach/velocityone-race",
        1080,
    ),
    base(
        0x1209,
        0xffb0,
        "OpenFFBoard",
        "openffboard/openffboard",
        900,
    ),
];

pub fn model(vid: u16, pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.vid == vid && m.pid == pid)
}
