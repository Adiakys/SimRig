use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use simrig_model::{ComponentDescription, DeviceInfo, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub api_version: u32,
    pub host_name: String,
    pub host_version: String,
    /// BCP 47 language tag for the plugin's labels, e.g. `en` or `it`.
    pub locale: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    pub api_version: u32,
    pub plugin: PluginInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscoverResult {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttachParams {
    pub device: DeviceInfo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttachResult {
    pub components: Vec<ComponentDescription>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetachParams {
    pub device: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentParams {
    pub component: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemParams {
    pub component: String,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetParams {
    pub component: String,
    pub item: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionParams {
    pub component: String,
    pub item: String,
    #[serde(default = "empty")]
    pub args: Value,
}

fn empty() -> Value {
    Value::Empty
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubscribeParams {
    pub component: String,
    /// Maximum notification rate; the plugin may send fewer.
    pub rate_hz: f64,
}

/// Result of `input/read` and payload of `input/state`. Unsupported inputs are omitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputState {
    pub component: String,
    pub values: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentsChanged {
    pub device: String,
    pub components: Vec<ComponentDescription>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamsChanged {
    pub component: ComponentDescription,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DevicesChanged {
    /// The full list of the plugin's virtual devices.
    pub devices: Vec<DeviceInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrivilegedRequest {
    /// Operation name declared in the driver's `driver.toml`.
    pub operation: String,
    /// Arguments, validated by the helper against the patterns in `driver.toml`.
    pub args: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrivilegedResult {
    /// What the operation printed on stdout.
    pub output: String,
}
