//! The PXN wheel bases SimRig knows: the ones hid-universal-pidff drives (`hid-ids.h`) and VD
//! bases only the kernel's generic PID force feedback drives. The V10 and V12 families use Lite
//! Star's vendor id; the VD series PXN's own. Other bases of these vendors, given to the plugin
//! by the user, get a generic model.

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: Option<&'static str>,
    /// A VD base: settings over its control interface.
    pub vd: bool,
    /// The settings protocol is confirmed on this model.
    pub verified: bool,
    /// In hid-universal-pidff; otherwise force feedback needs Linux 6.15 or newer.
    pub module: bool,
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
        model: Some(model),
        vd: false,
        verified: false,
        module: true,
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
        model: Some("pxn/vd6"),
        vd: true,
        verified: true,
        module: true,
        max_range: 2700,
    },
    Model {
        vid: PXN,
        pid: 0x400d,
        name: "PXN VD4",
        model: Some("pxn/vd4"),
        vd: true,
        verified: true,
        module: false,
        max_range: 2700,
    },
];

/// The model of a base, or a generic one for another base of PXN's vendors: a VD base for
/// PXN's own vendor id, with its settings unconfirmed.
pub fn model(vid: u16, pid: u16) -> Option<Model> {
    if let Some(m) = MODELS.iter().find(|m| m.vid == vid && m.pid == pid) {
        return Some(*m);
    }
    let vd = match vid {
        PXN => true,
        LITE_STAR => false,
        _ => return None,
    };
    Some(Model {
        vid,
        pid,
        name: "PXN wheel base",
        model: None,
        vd,
        verified: false,
        module: false,
        max_range: if vd { 2700 } else { 900 },
    })
}
