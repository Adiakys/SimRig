//! Protocol between the simrig app (host) and plugin adapters.
//!
//! Transport: JSON-RPC 2.0 over the adapter's stdio, one message per line. The host sends
//! requests on the adapter's stdin; the adapter answers on stdout and may send notifications.
//! Adapters log to stderr.

pub mod messages;
pub mod rpc;

pub use messages::*;
pub use rpc::{Message, RpcError, read_message, write_message};

/// Version of the adapter protocol. The host refuses adapters with a different version.
pub const API_VERSION: u32 = 1;

/// Method names.
pub mod method {
    /// host → plugin: handshake. Params [`InitializeParams`](crate::InitializeParams).
    pub const INITIALIZE: &str = "initialize";
    /// host → plugin: virtual devices the plugin provides by itself.
    pub const DISCOVER: &str = "devices/discover";
    /// host → plugin: a device is available; returns its components.
    pub const ATTACH: &str = "device/attach";
    /// host → plugin: a device went away.
    pub const DETACH: &str = "device/detach";
    /// host → plugin: full description of a component.
    pub const DESCRIBE: &str = "component/describe";
    /// host → plugin: read a standard parameter.
    pub const STD_GET: &str = "std/get";
    /// host → plugin: write a standard parameter.
    pub const STD_SET: &str = "std/set";
    /// host → plugin: write a custom parameter.
    pub const CUSTOM_SET: &str = "custom/set";
    /// host → plugin: run a standard action.
    pub const ACTION_RUN: &str = "action/run";
    /// host → plugin: write a standard output (LEDs, display…).
    pub const OUTPUT_WRITE: &str = "output/write";
    /// host → plugin: read all supported inputs of a component once.
    pub const INPUT_READ: &str = "input/read";
    /// host → plugin: start sending `input/state` notifications at a capped rate.
    pub const INPUT_SUBSCRIBE: &str = "input/subscribe";
    /// host → plugin: stop sending `input/state` notifications.
    pub const INPUT_UNSUBSCRIBE: &str = "input/unsubscribe";
    /// host → plugin: the app is closing; the plugin must exit.
    pub const SHUTDOWN: &str = "shutdown";

    /// plugin → host notification: live input state.
    pub const INPUT_STATE: &str = "input/state";
    /// plugin → host notification: the components of a device changed (e.g. rim swap).
    pub const COMPONENTS_CHANGED: &str = "components/changed";
    /// plugin → host notification: custom parameters or ranges of a component changed.
    pub const PARAMS_CHANGED: &str = "params/changed";
}

/// JSON-RPC error codes.
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL: i64 = -32603;
    /// The item is unsupported; `data` is a [`simrig_model::Unsupported`].
    pub const UNSUPPORTED: i64 = -32001;
    /// The value does not fit the item's type or range.
    pub const INVALID_VALUE: i64 = -32002;
    /// Unknown device, component or item.
    pub const NOT_FOUND: i64 = -32003;
    /// Incompatible API version.
    pub const INCOMPATIBLE: i64 = -32004;
    /// The device or driver failed to perform the operation.
    pub const FAILED: i64 = -32005;
}
