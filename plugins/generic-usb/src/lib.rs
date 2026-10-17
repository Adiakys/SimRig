//! The generic USB plugin: any USB device the user assigns to it, set up from the app as a
//! shifter or a handbrake by moving it, and made into a standard joystick by its driver.
//!
//! - [`config`]: a device's setup, shared by the adapter and the driver's service.
//! - [`decode`]: raw reports to gears and travel; [`detect`]: recognizing positions.
//! - [`hid`]: raw reports from `hidraw`, or, for devices the kernel's HID driver rejects,
//!   straight from USB ([`usbraw`]).
//! - [`adapter`], [`components`], [`setup`]: the plugin's adapter (runs with the app).
//! - [`service`]: the driver's service (runs with the device, app closed).

pub mod adapter;
pub mod components;
pub mod config;
pub mod decode;
pub mod detect;
pub mod hid;
pub mod service;
pub mod setup;
pub mod usbraw;
