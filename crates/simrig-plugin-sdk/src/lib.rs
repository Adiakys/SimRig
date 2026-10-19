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
mod procedures;
mod server;

pub use error::{Error, Result};
pub use interfaces::{
    AnyComponent, Component, Handbrake, Pedals, Shifter, SteeringWheel, WheelBase,
};
pub use procedures::{Outcome, Procedure};
pub use server::{HostLink, Server, run};
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

    /// Changes the adapter noticed on its own (e.g. a rim swap). Called after every request and
    /// periodically; the SDK applies them and notifies the host. Changes of virtual devices are
    /// detected by calling [`Adapter::discover`] again.
    fn take_changes(&mut self) -> Vec<Change> {
        Vec::new()
    }

    /// Guided procedures offered on an attached device, e.g. the setup of a device that has no
    /// components yet.
    fn device_procedures(&mut self, _device_id: &str) -> Vec<model::ProcedureDescriptor> {
        Vec::new()
    }

    /// Starts one of [`Adapter::device_procedures`]; called only for available ones.
    fn start_device_procedure(&mut self, _device_id: &str, id: &str) -> Result<Box<dyn Procedure>> {
        Err(Error::NotFound(format!("unknown procedure '{id}'")))
    }
}

/// A change of an attached device's components.
pub enum Change {
    /// A new component, e.g. a rim that was mounted.
    ComponentAdded {
        device: String,
        component: AnyComponent,
    },
    /// A component that went away, by id.
    ComponentRemoved { component: String },
}

/// Everything a plugin author usually needs.
pub mod prelude {
    pub use crate::model::{
        ActionOutcome, ActionRole, ActionSpec, Axis, Caveat, ChoiceOption, ComponentKind, Control,
        ControlKind, CurvePoint, CustomParam, Degrees, DeviceInfo, Element, Gear, HandbrakeAction,
        HandbrakeParam, Kilograms, Level, ParamSpec, ParamType, PedalAction, PedalParam, PedalRole,
        Percent, Persistence, Preview, ProcedureDescriptor, Reason, ShifterAction, ShifterParam,
        SteeringWheelAction, SteeringWheelOutput, SteeringWheelParam, Step, StepAction, Support,
        Unsupported, Value, WheelBaseAction, WheelBaseParam,
    };
    pub use crate::{
        Adapter, AnyComponent, Change, Component, Error, Handbrake, HostLink, Outcome, Pedals,
        PluginInfo, Procedure, Result, Shifter, SteeringWheel, WheelBase,
    };
}
