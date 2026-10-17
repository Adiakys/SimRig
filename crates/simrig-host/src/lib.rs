//! The simrig host: discovers plugins, starts their adapters, builds the rig and exposes it to
//! the GUI and the CLI. It runs only while the app is open and never needs privileges: those
//! are asked through `simrig-helper` only for driver operations.

pub mod catalog;
pub mod conformance;
mod drivers;
mod error;
mod host;
pub mod manifest;
pub mod paths;
pub mod presets;
mod process;
pub mod rig;
pub mod system;
pub mod usb;

pub use catalog::{Catalog, CatalogEntry, Image, Model};
pub use error::HostError;
pub use host::{Host, HostComponent, HostEvent, HostOptions, Plugin, PluginState};
pub use manifest::{Manifest, PluginLocation};
pub use presets::{ApplyEntry, ApplyOutcome, Preset, PresetStore};
pub use rig::RigConfig;
pub use simrig_driver::{DriverStatus, InstallState, InstalledDriver};
pub use system::SystemInfo;
