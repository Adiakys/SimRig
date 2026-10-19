//! One device handled by a kernel driver: the HID device the driver is bound to, its inputs,
//! and the pedal mapping kept in the user's configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use simrig_plugin_sdk::prelude::*;

use crate::input::{self, FfControl, Inputs};
use crate::pedals::{self, Layout, PedalMap};
use crate::sysfs;
use simrig_plugin_sdk::model::{Control, ControlKind};

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
    /// Where the plugin knows the pedals to be (they need no mapping), if it does.
    pub layout: Option<PedalMap>,
    /// The steering axis of a wheel base (not a pedal); `None` for a pedal set.
    pub steering: Option<u16>,
    /// Force feedback controls set through the input interface, in percent, as last set: they
    /// cannot be read back.
    ff_levels: BTreeMap<FfControl, f64>,
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
            layout: None,
            steering,
            ff_levels: BTreeMap::new(),
        };
        device.refresh();
        device
    }

    /// The pedals are where `layout` says: no mapping needed, unless the user mapped them by
    /// hand (that mapping wins).
    pub fn with_layout(mut self, layout: Layout) -> Self {
        let map = PedalMap::from_layout(layout);
        if self.pedal_map.pedals.is_empty() {
            self.pedal_map = map.clone();
        }
        self.layout = Some(map);
        self
    }

    /// Looks for the driver again (installed or bound later), and opens the inputs if they
    /// could not be opened yet (a driver just bound creates its event node, and udev gives it
    /// to the user, a moment later); true when either changed.
    pub fn refresh(&mut self) -> bool {
        let b = self.binding;
        let hid = sysfs::find_hid(&self.usb, b.vendor, b.drivers, b.rich);
        if hid == self.hid && self.inputs.error().is_none() {
            return false;
        }
        let inputs = Inputs::open(hid.as_deref().and_then(sysfs::event_node));
        let changed = hid != self.hid || inputs.error().is_none();
        self.inputs = inputs;
        self.hid = hid;
        changed
    }

    /// Which of these attributes the driver exposes now: drivers create them while they bind,
    /// so they can show up after the device did. For adapters to notice when they do.
    pub fn exposed(&self, attrs: &[&str]) -> Vec<bool> {
        attrs.iter().map(|a| self.attr(a).is_some()).collect()
    }

    /// The inputs can be read.
    pub fn inputs_ready(&self) -> bool {
        self.hid.is_some() && self.inputs.error().is_none()
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

    /// A force feedback control of the input interface as a percentage, when the driver takes
    /// it: the last value set, else what drivers start with (full strength, no centering
    /// spring). Marked as not fully supported, since the device cannot report it.
    pub fn ff_spec(&self, control: FfControl) -> Option<ParamSpec> {
        if self.hid.is_none() || !self.inputs.takes(control) {
            return None;
        }
        let start = match control {
            FfControl::Gain => 100.0,
            FfControl::Autocenter => 0.0,
        };
        let level = self.ff_levels.get(&control).copied().unwrap_or(start);
        Some(ParamSpec::float(level, 0.0, 100.0, 1.0).with_caveat(
            Caveat::write_only().with_detail(
                "It is set through the kernel's force feedback interface, as games do, and \
                 games can change it too.",
            ),
        ))
    }

    /// Sets a force feedback control of the input interface, `value` in percent.
    pub fn set_ff(&mut self, control: FfControl, value: &Value) -> Result<()> {
        let percent = value
            .as_f64()
            .ok_or_else(|| Error::InvalidValue("expected a number".into()))?
            .clamp(0.0, 100.0);
        let level = (percent / 100.0 * f64::from(u16::MAX)).round() as u16;
        self.inputs
            .set_control(control, level)
            .map_err(|e| Error::failed(e.to_string()))?;
        self.ff_levels.insert(control, percent);
        Ok(())
    }

    /// The hat switches' directions, as a steering wheel's.
    pub fn hats(&self) -> Support<Vec<i64>> {
        let hats = self.inputs.hats();
        if hats.is_empty() {
            return Support::not_in_driver();
        }
        Support::Supported(hats.into_iter().map(|(_, d)| d).collect())
    }

    /// Analog paddles on these axes, each 0..1 over its travel.
    pub fn analog_paddles(&self, codes: &[u16]) -> Support<Vec<f64>> {
        let axes = self.inputs.axes();
        let values: Option<Vec<f64>> = codes
            .iter()
            .map(|c| axes.get(c).map(|a| a.unit()))
            .collect();
        match values {
            Some(v) if !v.is_empty() => Support::Supported(v),
            _ => Support::not_in_driver(),
        }
    }

    /// The device's buttons and hat switches, and the analog paddles on `paddles` (axis code,
    /// name), numbered as games number them; `labels` names buttons by their code.
    pub fn controls(&self, paddles: &[(u16, &str)], labels: &[(u16, &str)]) -> Vec<Control> {
        let mut controls: Vec<Control> = self
            .inputs
            .buttons()
            .into_iter()
            .enumerate()
            .map(|(i, (code, _))| {
                let c = Control::new(ControlKind::Button, i as u32 + 1, "buttons", i)
                    .code(input::key_name(code));
                match labels.iter().find(|(l, _)| *l == code) {
                    Some((_, label)) => c.label(*label),
                    None => c,
                }
            })
            .collect();
        controls.extend(
            self.inputs
                .hats()
                .into_iter()
                .enumerate()
                .map(|(i, (x, _))| {
                    Control::new(ControlKind::Hat, i as u32 + 1, "hats", i).code(format!(
                        "{}/{}",
                        input::axis_name(x),
                        input::axis_name(x + 1)
                    ))
                }),
        );
        // Games number axes in code order, hats apart.
        let axes = self.inputs.axes();
        let numbered: Vec<u16> = axes
            .keys()
            .copied()
            .filter(|c| !input::is_hat(*c))
            .collect();
        for (i, (code, name)) in paddles.iter().enumerate() {
            let (Some(number), Some(axis)) =
                (numbered.iter().position(|c| c == code), axes.get(code))
            else {
                continue;
            };
            let span = (i64::from(axis.max) - i64::from(axis.min)).max(1) as u64 + 1;
            let bits = (64 - (span - 1).leading_zeros()) as u8;
            controls.push(
                Control::new(ControlKind::Axis, number as u32 + 1, "analog_paddles", i)
                    .code(input::axis_name(*code))
                    .label(*name)
                    .bits(bits),
            );
        }
        controls
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
