//! The standard force feedback plugin: wheel bases that speak the USB standard for force
//! feedback (HID PID), on hid-universal-pidff (in Linux from 6.15; the driver builds it for older
//! kernels) or, for the OpenFFBoard, the kernel's generic one. Their settings (rotation
//! included) live in each brand's own software or on the base: SimRig gives the inputs, the
//! force feedback test, and the strength and centering spring through the kernel's input
//! interface.

pub mod adapter;
pub mod components;
pub mod device;
pub mod models;
