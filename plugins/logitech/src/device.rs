//! One Logitech wheel attached to the plugin: the HID device its kernel driver is bound to.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};

use crate::models::Model;

pub const BINDING: Binding = Binding {
    vendor: "046D",
    drivers: &["logitech", "logitech-hidpp-device", "logitech-dd"],
    // The interface with the settings and the inputs, when there are several.
    rich: &["range", "wheel_range", "input"],
    unbound: "No Logitech driver handles the wheel: install the driver, then plug it in again.",
};

pub struct LogitechDevice {
    pub hid: HidDevice,
    pub model: &'static Model,
}

pub type Shared = Rc<RefCell<LogitechDevice>>;

impl HasHid for LogitechDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl LogitechDevice {
    pub fn new(id: &str, model: &'static Model, usb: PathBuf, config: PathBuf) -> Self {
        let mut hid = HidDevice::new(
            id,
            usb,
            BINDING,
            config,
            &format!("{:04x}", model.pid),
            Some(0),
        );
        if let Some(layout) = model.pedals {
            hid = hid.with_layout(layout);
        }
        Self { hid, model }
    }

    /// The rotation the driver applies.
    pub fn range(&self) -> Option<f64> {
        let attr = match self.model.family {
            crate::models::Family::DirectDrive => "wheel_range",
            _ => "range",
        };
        Some(self.hid.read_int(attr)? as f64)
    }
}
