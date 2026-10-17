//! The PXN wheel bases hid-universal-pidff drives (`hid-ids.h`), and what SimRig knows about
//! them. The V10 and V12 families use Lite Star's vendor id; the VD series PXN's own.

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
    /// A VD base: settings over its control interface.
    pub vd: bool,
    /// Widest rotation, in degrees.
    pub max_range: u32,
}

pub const LITE_STAR: u16 = 0x11ff;
pub const PXN: u16 = 0x36e6;

const fn classic(pid: u16, name: &'static str, model: &'static str) -> Model {
    Model {
        vid: LITE_STAR,
        pid,
        name,
        model,
        vd: false,
        max_range: 900,
    }
}

pub const MODELS: &[Model] = &[
    classic(0x3245, "PXN V10", "pxn/v10"),
    classic(0x1212, "PXN V12", "pxn/v12"),
    classic(0x1112, "PXN V12 Lite", "pxn/v12-lite"),
    classic(0x1211, "PXN V12 Lite", "pxn/v12-lite"),
    classic(0x2141, "Lite Star GT987", "pxn/gt987"),
    Model {
        vid: PXN,
        pid: 0x400f,
        name: "PXN VD6",
        model: "pxn/vd6",
        vd: true,
        max_range: 2700,
    },
];

pub fn model(vid: u16, pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.vid == vid && m.pid == pid)
}
