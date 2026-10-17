//! The PXN plugin: PXN wheel bases on hid-universal-pidff (in Linux from 6.15; the driver builds
//! it for older kernels). The VD series' settings go over its vendor control interface, with the
//! protocol Pitwall (<https://github.com/unionyxx/pitwall>) worked out.

pub mod adapter;
pub mod components;
pub mod control;
pub mod device;
pub mod models;
pub mod protocol;
pub mod settings;
