//! One device handled by a kernel driver: the HID device the driver is bound to, its inputs,
//! and the pedal mapping kept in the user's configuration.

use std::path::PathBuf;

use simrig_plugin_sdk::prelude::*;

use crate::input::Inputs;
use crate::pedals::{self, PedalMap};
use crate::sysfs;

/// Which HID devices below the USB device belong to the driver.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    /// USB vendor in hex, e.g. `"0EB7"`.
    pub vendor: &'static str,
    /// Names of the kernel drivers (as in `/sys/bus/hid/drivers`) that count.
    pub drivers: &'static [&'static str],
    /// Entries that mark the HID device to talk to, when there are several.
    pub rich: &'static [&'static str],
    /// Why nothing works when no HID device is bound.
    pub unbound: &'static str,
}

pub struct HidDevice {
    pub id: String,
    pub usb: PathBuf,
    pub binding: Binding,
    /// The HID device directory, while the driver is bound.
    pub hid: Option<PathBuf>,
    pub inputs: Inputs,
    /// The user's configuration file, and this model's key in it.
    pub config: PathBuf,
    pub map_key: String,
    pub pedal_map: PedalMap,
    /// The steering axis of a wheel base (not a pedal); `None` for a pedal set.
    pub steering: Option<u16>,
}

impl HidDevice {
    pub fn new(
        id: &str,
        usb: PathBuf,
        binding: Binding,
        config: PathBuf,
        map_key: &str,
        steering: Option<u16>,
    ) -> Self {
        let pedal_map = pedals::load_map(&config, map_key);
        let mut device = Self {
            id: id.into(),
            usb,
            binding,
            hid: None,
            inputs: Inputs::None(String::new()),
            config,
            map_key: map_key.into(),
            pedal_map,
            steering,
        };
        device.refresh();
        device
    }

    /// Looks for the driver again (installed or bound later); true when that changed.
    pub fn refresh(&mut self) -> bool {
        let b = self.binding;
        let hid = sysfs::find_hid(&self.usb, b.vendor, b.drivers, b.rich);
        if hid == self.hid {
            return false;
        }
        self.inputs = Inputs::open(hid.as_deref().and_then(sysfs::event_node));
        self.hid = hid;
        true
    }

    /// Why nothing works, when the driver does not handle the device.
    pub fn unbound(&self) -> Option<Unsupported> {
        self.hid
            .is_none()
            .then(|| Unsupported::new(Reason::NeedsDriver).with_detail(self.binding.unbound))
    }

    /// The kernel driver bound to the HID device.
    pub fn driver(&self) -> Option<String> {
        let link = std::fs::read_link(self.hid.as_ref()?.join("driver")).ok()?;
        Some(link.file_name()?.to_string_lossy().into_owned())
    }

    /// A sysfs attribute of the HID device, when it exists.
    pub fn attr(&self, name: &str) -> Option<PathBuf> {
        let path = self.hid.as_ref()?.join(name);
        path.exists().then_some(path)
    }

    pub fn read_int(&self, name: &str) -> Option<i64> {
        sysfs::read_int(&self.attr(name)?)
    }

    /// Writes an attribute, with errors as plugin errors.
    pub fn write(&self, name: &str, value: &str) -> Result<()> {
        let path = self.attr(name).ok_or_else(|| {
            Error::failed(format!(
                "'{name}' is not exposed by the installed driver for this device."
            ))
        })?;
        sysfs::write(&path, value).map_err(|e| Error::failed(e.to_string()))
    }

    /// The steering angle in degrees for a rotation of `range` (centred at 0).
    pub fn angle(&self, range: f64) -> Support<Degrees> {
        if let Some(u) = self.unbound() {
            return Support::Unsupported(u);
        }
        let axis = self
            .steering
            .and_then(|code| self.inputs.axes().get(&code).copied());
        let Some(axis) = axis else {
            return Support::Unsupported(
                Unsupported::new(Reason::Unavailable)
                    .with_detail(self.inputs.error().unwrap_or("No steering axis.")),
            );
        };
        Support::Supported((axis.unit() - 0.5) * range)
    }

    /// The force feedback test and stop, as base actions.
    pub fn force_action(&self, action: WheelBaseAction) -> Option<Support<ActionSpec>> {
        match action {
            WheelBaseAction::TestEffect => Some(Support::Supported(ActionSpec::confirm(
                "The wheel turns left and right by itself for two seconds. Hold it firmly.",
            ))),
            WheelBaseAction::StopForce => Some(Support::Supported(ActionSpec::default())),
            _ => None,
        }
    }

    pub fn run_force_action(&mut self, action: WheelBaseAction) -> Option<Result<ActionOutcome>> {
        let result = match action {
            WheelBaseAction::TestEffect => self
                .inputs
                .test_effect()
                .map(|_| ActionOutcome::message("Force feedback test running.")),
            WheelBaseAction::StopForce => self
                .inputs
                .stop_force()
                .map(|_| ActionOutcome::message("Forces stopped.")),
            _ => return None,
        };
        Some(result.map_err(|e| Error::failed(e.to_string())))
    }

    /// The buttons, pressed or not, as a steering wheel's.
    pub fn buttons(&self) -> Support<Vec<bool>> {
        let buttons = self.inputs.buttons();
        if buttons.is_empty() {
            return Support::not_in_driver();
        }
        Support::Supported(buttons.into_iter().map(|(_, pressed)| pressed).collect())
    }

    pub fn save_map(&mut self, map: PedalMap) -> std::io::Result<()> {
        pedals::save_map(&self.config, &self.map_key, &map)?;
        self.pedal_map = map;
        Ok(())
    }
}

/// A plugin's device type: whatever it keeps, around a [`HidDevice`].
pub trait HasHid: 'static {
    fn hid(&self) -> &HidDevice;
    fn hid_mut(&mut self) -> &mut HidDevice;
}

impl HasHid for HidDevice {
    fn hid(&self) -> &HidDevice {
        self
    }
    fn hid_mut(&mut self) -> &mut HidDevice {
        self
    }
}
