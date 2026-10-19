//! Simagic bases as standard components, over simagic-ff's sysfs attributes and the event node.

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::settings::Setting;
use simrig_plugin_sdk::model::{Appearance, RimShape, StdItem};
use simrig_plugin_sdk::prelude::*;

use crate::device::{SETTINGS, Shared, SimagicDevice};
use crate::models;

pub struct Base {
    pub device: Shared,
}

impl Base {
    fn setting(&self, item: &str) -> Option<Setting> {
        match item.strip_prefix("custom.") {
            Some(id) => SimagicDevice::custom(id),
            None => WheelBaseParam::from_key(item).and_then(SimagicDevice::setting),
        }
    }

    fn spec(&self, setting: &Setting) -> Support<ParamSpec> {
        let d = self.device.borrow();
        match d.no_settings() {
            Some(u) => Support::Unsupported(u),
            None => setting.spec(&d.hid, models::MAX_RANGE),
        }
    }

    fn write(&self, setting: &Setting, value: &Value) -> Result<()> {
        let mut d = self.device.borrow_mut();
        if let Some(u) = d.no_settings() {
            return Err(Error::Unsupported(u));
        }
        if setting.attr == "max_angle" {
            // The firmware loses force feedback (until replugged) when the rotation changes with
            // the wheel outside the new one, as Simagic's software warns.
            let new = value.as_f64().unwrap_or(0.0);
            if let Support::Supported(angle) = d.hid.angle(d.range())
                && angle.abs() > new / 2.0
            {
                return Err(Error::failed(format!(
                    "Turn the wheel to within {:.0}° of the center first: changed with the wheel \
                     outside the new rotation, the base loses its force feedback until it is \
                     plugged in again.",
                    new / 2.0
                )));
            }
        }
        setting.write(&mut d.hid, value)
    }
}

impl Component for Base {
    fn id(&self) -> String {
        format!("{}/base", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        self.device.borrow().model.name.into()
    }

    fn model(&self) -> Option<String> {
        Some(self.device.borrow().model.model.into())
    }

    fn custom_params(&mut self) -> Vec<CustomParam> {
        let d = self.device.borrow();
        let unavailable = d.no_settings();
        SETTINGS
            .iter()
            .enumerate()
            .filter_map(|(order, s)| {
                let p = s.custom_param(&d.hid, models::MAX_RANGE, order as i32)?;
                Some(match &unavailable {
                    Some(u) => p.availability(Support::Unsupported(u.clone())),
                    None => p,
                })
            })
            .collect()
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = SimagicDevice::custom(id)
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        self.write(&setting, value)
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        let setting = self
            .setting(item)
            .ok_or_else(|| Error::NotFound(format!("no setting '{item}'")))?;
        if setting.attr == "max_angle" {
            // Applied at plug-in, it could catch the wheel outside it (see `write`).
            return Err(Error::failed(
                "The rotation is not saved for the next plug-in: the wheel could be outside it \
                 then, and the base would lose its force feedback. Games set it as they start.",
            ));
        }
        let raw = setting.raw(value)?;
        let d = self.device.borrow();
        let id = format!("{:04X}:{:04X}", d.model.vid, d.model.pid);
        // A udev rule writes it again whenever simagic-ff binds to the base.
        host.privileged(
            "save-setting",
            &[
                ("model", &id),
                ("attr", setting.attr),
                ("value", &raw.to_string()),
            ],
        )?;
        Ok(())
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        vec![pedals::descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        pedals::start(&self.device, id)
    }
}

impl WheelBase for Base {
    fn angle(&mut self) -> Support<Degrees> {
        let d = self.device.borrow();
        d.hid.angle(d.range())
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(match d.hid.driver() {
            Some(driver) => format!("ok ({driver})"),
            None => "no driver bound".into(),
        })
    }

    fn firmware(&mut self) -> Support<String> {
        match self.device.borrow().hid.read_int("fw_version") {
            Some(v) => Support::Supported(format!("v{v}")),
            None => Support::not_in_driver(),
        }
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        match SimagicDevice::setting(param) {
            Some(setting) => self.spec(&setting),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let setting = SimagicDevice::setting(param).ok_or_else(Error::unsupported)?;
        self.write(&setting, value)
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        let d = self.device.borrow();
        match action {
            WheelBaseAction::TestEffect | WheelBaseAction::StopForce => match d.no_force() {
                Some(u) => Support::Unsupported(u),
                None => d
                    .hid
                    .force_action(action)
                    .unwrap_or_else(Support::not_in_driver),
            },
            WheelBaseAction::SetCenter => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("Not available through SimRig yet: set it in Simagic's software."),
            ),
            WheelBaseAction::Reset => Support::not_in_driver(),
        }
    }

    fn run_action(&mut self, action: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> {
        self.device
            .borrow_mut()
            .hid
            .run_force_action(action)
            .unwrap_or_else(|| Err(Error::unsupported()))
    }
}

/// The rim mounted on the base: its buttons come through the base.
pub struct Wheel {
    pub device: Shared,
}

impl Component for Wheel {
    fn id(&self) -> String {
        format!("{}/wheel", self.device.borrow().hid.id)
    }

    fn name(&self) -> String {
        format!("{} rim", self.device.borrow().model.name)
    }

    fn appearance(&self) -> Appearance {
        Appearance::rim(RimShape::Round)
    }
}

impl SteeringWheel for Wheel {
    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        d.hid.buttons()
    }

    fn rim_id(&mut self) -> Support<String> {
        Support::Unsupported(
            Unsupported::not_in_driver()
                .with_detail("The driver does not say which rim is mounted."),
        )
    }
}

/// The pedals plugged into the base (over its bus port), once mapped.
pub fn pedals(device: &Shared) -> PedalSet<SimagicDevice> {
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", device.borrow().model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
