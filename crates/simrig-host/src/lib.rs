//! The simrig host: discovers plugins, starts their adapters, builds the rig and exposes it to
//! the GUI and the CLI. It runs only while the app is open; it never starts background
//! processes of its own.

pub mod conformance;
mod error;
mod host;
pub mod manifest;
mod process;
pub mod usb;

pub use error::HostError;
pub use host::{Host, HostComponent, HostOptions, Plugin, PluginState};
pub use manifest::{Manifest, PluginLocation};
