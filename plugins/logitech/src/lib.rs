//! The Logitech plugin: Logitech wheels on their kernel drivers. Most work with the kernel's
//! own (`hid-logitech` for the G29 family, `hid-logitech-hidpp` for the G920 family); the
//! driver adds new-lg4ff for the G29 family (more effects and settings) and the TrueForce
//! driver for the direct drive G PRO and RS50, which the kernel does not drive. The oldest
//! WingMan wheels work with the kernel's `hid-logitech` and `iforce`.

pub mod adapter;
pub mod components;
pub mod device;
pub mod models;
pub mod modes;
pub mod settings;
