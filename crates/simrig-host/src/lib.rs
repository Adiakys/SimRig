//! The simrig host: discovers plugins, starts their adapters, builds the rig and exposes it to
//! the GUI and the CLI. It runs only while the app is open and never needs privileges: those
//! are asked through `simrig-helper` only for driver operations.

pub mod catalog;
pub mod conformance;
pub mod device_checks;
mod drivers;
mod error;
mod host;
pub mod manifest;
pub mod paths;
pub mod presets;
mod process;
pub mod rig;
mod steam;
pub mod system;
#[cfg(feature = "testing")]
pub mod testing;
pub mod usb;

pub use catalog::{Catalog, CatalogEntry, Image, Model};
pub use device_checks::{DeviceFindings, Finding, Fix, Problem, SystemRoot};
pub use drivers::DriverUpdate;
pub use error::HostError;
pub use host::{Host, HostComponent, HostEvent, HostOptions, Plugin, PluginState, Target};
pub use manifest::{Manifest, PluginLocation};
pub use presets::{ApplyEntry, ApplyOutcome, Preset, PresetStore};
pub use rig::RigConfig;
pub use simrig_driver::{
    DriverContract, DriverStatus, InstallState, InstalledDriver, version_text,
};
pub use system::SystemInfo;
