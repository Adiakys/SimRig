//! The Fanatec devices hid-fanatecff drives (`hid-ftec.h`), and what SimRig knows about them.

use simrig_plugin_sdk::model::RimShape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A wheel base; `max_range` is its widest rotation in degrees.
    Base { max_range: u32 },
    /// A pedal set connected by USB.
    Pedals,
}

#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    /// Catalog model id.
    pub model: &'static str,
    pub kind: Kind,
}

pub const VENDOR: u16 = 0x0eb7;

pub const MODELS: &[Model] = &[
    base(
        0x0001,
        "ClubSport Wheel Base V2",
        "fanatec/clubsport-v2",
        900,
    ),
    base(
        0x0004,
        "ClubSport Wheel Base V2.5",
        "fanatec/clubsport-v25",
        900,
    ),
    base(
        0x0005,
        "CSL Elite Wheel Base (PS4)",
        "fanatec/csl-elite-ps4",
        1080,
    ),
    base(0x0e03, "CSL Elite Wheel Base", "fanatec/csl-elite", 1080),
    base(0x0006, "Podium Wheel Base DD1", "fanatec/podium-dd1", 2520),
    base(0x0007, "Podium Wheel Base DD2", "fanatec/podium-dd2", 2520),
    base(0x0020, "CSL DD", "fanatec/csl-dd", 2520),
    base(0x0011, "CSR Elite Wheel Base", "fanatec/csr-elite", 900),
    base(
        0x0197,
        "Porsche 911 Turbo S Wheel",
        "fanatec/porsche-911",
        900,
    ),
    pedals(0x6204, "CSL Elite Pedals", "fanatec/csl-elite-pedals"),
    pedals(0x6205, "CSL Pedals (load cell)", "fanatec/csl-pedals-lc"),
    pedals(
        0x6206,
        "CSL Pedals (load cell) V2",
        "fanatec/csl-pedals-lc-v2",
    ),
    pedals(0x183b, "ClubSport Pedals V3", "fanatec/clubsport-pedals-v3"),
];

const fn base(pid: u16, name: &'static str, model: &'static str, max_range: u32) -> Model {
    Model {
        pid,
        name,
        model,
        kind: Kind::Base { max_range },
    }
}

const fn pedals(pid: u16, name: &'static str, model: &'static str) -> Model {
    Model {
        pid,
        name,
        model,
        kind: Kind::Pedals,
    }
}

pub fn model(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

/// Steering wheels (rims) by the id the base reports in `wheel_id`.
pub fn rim(id: u16) -> Option<(&'static str, RimShape)> {
    Some(match id {
        0x0008 => ("CSL Steering Wheel P1 V2", RimShape::Round),
        0x0412 => ("CSL Elite Steering Wheel WRC", RimShape::Round),
        0x310b => ("CSL Elite Steering Wheel McLaren GT3", RimShape::Gt),
        0x2102 => ("ClubSport Steering Wheel F1 IS", RimShape::Formula),
        0x000a => ("ClubSport Steering Wheel Formula V2", RimShape::Formula),
        0x0c12 => ("Podium Steering Wheel Porsche 911 GT3 R", RimShape::Gt),
        _ => return None,
    })
}
