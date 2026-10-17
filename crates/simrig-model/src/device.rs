use serde::{Deserialize, Serialize};

/// A physical (or simulated) device handled by a plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique within the plugin.
    pub id: String,
    pub name: String,
    pub bus: Bus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Bus {
    Usb {
        vid: u16,
        pid: u16,
        /// sysfs path of the USB device.
        sysfs: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        serial: Option<String>,
    },
    /// Provided by the plugin itself (e.g. the simulated rig).
    Virtual,
}
