//! One base attached to the plugin: its HID device (inputs and force feedback, through the
//! kernel).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_sdk::prelude::*;

use crate::models::Model;

const UNBOUND: &str = "The base has no HID driver bound: plug it in again.";

pub struct PidffDevice {
    pub hid: HidDevice,
    pub model: Model,
}

pub type Shared = Rc<RefCell<PidffDevice>>;

impl HasHid for PidffDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl PidffDevice {
    pub fn new(id: &str, model: Model, usb: PathBuf, config: PathBuf) -> Self {
        let binding = Binding {
            vendor: model.vid,
            // hid-generic gives the inputs, and force feedback too for the OpenFFBoard and,
            // from Linux 6.15, the others.
            drivers: &["hid-universal-pidff", "hid-generic"],
            rich: &["input"],
            unbound: UNBOUND,
        };
        let key = format!("{:04x}-{:04x}", model.vid, model.pid);
        let hid = HidDevice::new(id, usb, binding, config, &key, Some(0));
        Self { hid, model }
    }

    /// Why force feedback does not work, if it does not.
    pub fn no_force(&self) -> Option<Unsupported> {
        if let Some(u) = self.hid.unbound() {
            return Some(u);
        }
        (!self.hid.inputs.force_feedback()).then(|| {
            if self.model.model.is_some() {
                Unsupported::new(Reason::NeedsDriver).with_detail(
                    "No force feedback: it needs hid-universal-pidff, or Linux 6.15 or newer.",
                )
            } else {
                Unsupported::not_in_driver()
                    .with_detail("No force feedback: it needs Linux 6.15 or newer.")
            }
        })
    }
}
