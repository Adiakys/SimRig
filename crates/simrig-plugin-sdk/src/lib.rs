//! SDK to write simrig plugin adapters in Rust.
//!
//! An adapter implements [`Adapter`] and, for every component its devices provide, one of the
//! standard component interfaces ([`WheelBase`], [`SteeringWheel`], [`Pedals`], [`Shifter`],
//! [`Handbrake`]). Every interface method defaults to "unsupported, not in driver": implement
//! only what the hardware can do, and report what it physically lacks as `NotInHardware`.
//!
//! ```no_run
//! use simrig_plugin_sdk::prelude::*;
//!
//! struct MyAdapter;
//!
//! impl Adapter for MyAdapter {
//!     fn info(&self) -> PluginInfo {
//!         PluginInfo { id: "my".into(), name: "My plugin".into(), version: "0.1.0".into() }
//!     }
//!     fn attach(&mut self, _device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
//!         Ok(Vec::new())
//!     }
//! }
//!
//! fn main() -> std::process::ExitCode {
//!     simrig_plugin_sdk::run(MyAdapter)
//! }
//! ```
//!
//! The protocol runs on stdin/stdout: adapters must log to stderr only.

mod dispatch;
mod error;
mod interfaces;
mod server;

pub use error::{Error, Result};
pub use interfaces::{
    AnyComponent, Component, Handbrake, Pedals, Shifter, SteeringWheel, WheelBase,
};
pub use server::{Server, run};
pub use simrig_model as model;
pub use simrig_protocol::PluginInfo;

/// The plugin entry point.
pub trait Adapter {
    fn info(&self) -> PluginInfo;

    /// Virtual devices this plugin provides by itself (plugins with `match.virtual = true`).
    fn discover(&mut self) -> Vec<model::DeviceInfo> {
        Vec::new()
    }

    /// A device is available: return the components it provides. Component ids must be unique
    /// within the plugin.
    fn attach(&mut self, device: &model::DeviceInfo) -> Result<Vec<AnyComponent>>;

    /// A device went away. Its components are dropped by the SDK.
    fn detach(&mut self, _device_id: &str) {}
}

/// Everything a plugin author usually needs.
pub mod prelude {
    pub use crate::model::{
        ActionOutcome, ActionSpec, Axis, ChoiceOption, CurvePoint, CustomParam, Degrees,
        DeviceInfo, Gear, HandbrakeAction, HandbrakeParam, Kilograms, ParamSpec, ParamType,
        PedalAction, PedalParam, PedalRole, Percent, Persistence, Reason, ShifterAction,
        ShifterParam, SteeringWheelAction, SteeringWheelOutput, SteeringWheelParam, Support,
        Unsupported, Value, WheelBaseAction, WheelBaseParam,
    };
    pub use crate::{
        Adapter, AnyComponent, Component, Error, Handbrake, Pedals, PluginInfo, Result, Shifter,
        SteeringWheel, WheelBase,
    };
}
