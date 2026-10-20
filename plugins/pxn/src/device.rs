//! One PXN base attached to the plugin: its HID device (inputs and force feedback, through the
//! kernel) and, on a VD base, its control interface (settings).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use simrig_plugin_kit::device::{Binding, HasHid, HidDevice};
use simrig_plugin_sdk::prelude::*;

use crate::control::{self, Control};
use crate::models::Model;
use crate::protocol::{self, BASE_SIZE};

const UNBOUND: &str = "The base has no HID driver bound: plug it in again.";

fn binding(model: &Model) -> Binding {
    Binding {
        vendor: model.vid,
        // hid-generic gives the inputs, and force feedback too from Linux 6.15.
        drivers: &["hid-universal-pidff", "hid-generic"],
        rich: &["input"],
        unbound: UNBOUND,
    }
}

pub struct PxnDevice {
    pub hid: HidDevice,
    pub model: Model,
    control: Option<Box<dyn Control>>,
    /// Why the control interface cannot be used, while it cannot.
    control_error: Option<String>,
    /// The base settings block, as read or last written.
    block: Option<[u8; BASE_SIZE]>,
}

pub type Shared = Rc<RefCell<PxnDevice>>;

impl HasHid for PxnDevice {
    fn hid(&self) -> &HidDevice {
        &self.hid
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        &mut self.hid
    }
}

impl PxnDevice {
    pub fn new(id: &str, model: Model, usb: PathBuf, config: PathBuf) -> Self {
        let key = format!("{:04x}-{:04x}", model.vid, model.pid);
        let hid = HidDevice::new(id, usb, binding(&model), config, &key, Some(0));
        let mut device = Self {
            hid,
            model,
            control: None,
            control_error: None,
            block: None,
        };
        device.refresh();
        device
    }

    /// Looks for the drivers and the control interface again; true when the HID binding
    /// changed.
    pub fn refresh(&mut self) -> bool {
        if self.model.vd && self.control.is_none() {
            match control::find(&self.hid.usb) {
                None => self.control_error = Some("The base has no control interface.".into()),
                Some(path) => match control::open(&path) {
                    Ok(c) => {
                        self.control = Some(c);
                        self.control_error = None;
                    }
                    Err(e) => {
                        self.control_error = Some(format!("Cannot open {}: {e}.", path.display()))
                    }
                },
            }
        }
        self.hid.refresh()
    }

    /// Why settings are unavailable, if they are.
    pub fn no_settings(&self) -> Option<Unsupported> {
        if !self.model.vd {
            return Some(Unsupported::not_in_driver().with_detail(
                "PXN's settings protocol is known for the VD series only: set it on the base.",
            ));
        }
        let error = self.control_error.as_ref()?;
        let reason = if error.contains("ermission denied") {
            // SimRig's udev rules give the user the control interface.
            Reason::NeedsDriver
        } else {
            Reason::Unavailable
        };
        Some(Unsupported::new(reason).with_detail(error))
    }

    fn block(&mut self) -> Result<[u8; BASE_SIZE]> {
        if let Some(block) = self.block {
            return Ok(block);
        }
        let control = self
            .control
            .as_mut()
            .ok_or_else(|| Error::failed("No control interface."))?;
        match control.read_base() {
            Ok(block) => {
                self.block = Some(block);
                Ok(block)
            }
            Err(e) => {
                // Opened again at the next refresh.
                self.control = None;
                Err(Error::failed(e.to_string()))
            }
        }
    }

    pub fn get(&mut self, field: &str) -> Option<u16> {
        protocol::get(&self.block().ok()?, field)
    }

    /// Read, change, write: fields SimRig does not show are kept.
    pub fn set(&mut self, field: &str, value: u16) -> Result<()> {
        let mut block = self.block()?;
        if !protocol::set(&mut block, field, value) {
            return Err(Error::NotFound(format!("no field '{field}'")));
        }
        let control = self
            .control
            .as_mut()
            .ok_or_else(|| Error::failed("No control interface."))?;
        if let Err(e) = control.write_base(&block) {
            self.control = None;
            self.block = None;
            return Err(Error::failed(e.to_string()));
        }
        self.block = Some(block);
        Ok(())
    }

    /// The rotation the base applies.
    pub fn range(&mut self) -> f64 {
        self.get("max_steering_angle")
            .map(f64::from)
            .unwrap_or(900.0)
    }

    /// Why force feedback does not work, if it does not.
    pub fn no_force(&self) -> Option<Unsupported> {
        if let Some(u) = self.hid.unbound() {
            return Some(u);
        }
        (!self.hid.inputs.force_feedback()).then(|| {
            if self.model.module {
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
