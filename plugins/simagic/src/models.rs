//! The Simagic bases simagic-ff drives (`hid-simagic.h`). The Alpha family shares one id, the
//! one of the older M10 too.

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub vid: u16,
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
}

pub const ALPHA_VENDOR: u16 = 0x0483;
pub const VENDOR: u16 = 0x3670;

/// The widest rotation simagic-ff sets.
pub const MAX_RANGE: f64 = 2520.0;

pub const MODELS: &[Model] = &[
    Model {
        vid: ALPHA_VENDOR,
        pid: 0x0522,
        name: "Simagic Alpha",
        model: "simagic/alpha",
    },
    Model {
        vid: VENDOR,
        pid: 0x0500,
        name: "Simagic Alpha EVO Sport",
        model: "simagic/evo-sport",
    },
    Model {
        vid: VENDOR,
        pid: 0x0501,
        name: "Simagic Alpha EVO",
        model: "simagic/evo",
    },
    Model {
        vid: VENDOR,
        pid: 0x0502,
        name: "Simagic Alpha EVO Pro",
        model: "simagic/evo-pro",
    },
];

pub fn model(vid: u16, pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.vid == vid && m.pid == pid)
}
