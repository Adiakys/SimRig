//! The Simagic plugin: Simagic wheel bases on simagic-ff, which its driver (`driver/`) bundles
//! and builds with DKMS. Bases with a firmware from before v159 speak standard HID force
//! feedback and work with the kernel's own driver, without settings.

pub mod adapter;
pub mod components;
pub mod device;
pub mod models;
