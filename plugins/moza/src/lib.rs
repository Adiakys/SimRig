//! The Moza plugin: Moza wheel bases and their rims, pedals, handbrakes and shifters. Force
//! feedback and inputs go through the kernel (hid-universal-pidff, in Linux from 6.15; the driver
//! builds it for older kernels); settings through the devices' serial port, with the protocol
//! Boxflat (<https://github.com/Lawstorant/boxflat>) worked out.

pub mod adapter;
pub mod components;
pub mod device;
pub mod models;
pub mod protocol;
pub mod serial;
pub mod settings;
