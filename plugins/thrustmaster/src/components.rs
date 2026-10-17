//! Thrustmaster wheels as standard components, over hid-tmff2's sysfs attributes and event
//! node.

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::model::{Appearance, RimShape, StdItem};
use simrig_plugin_sdk::prelude::*;

use crate::device::{SETTINGS, Shared, TmDevice};

pub struct Base {
    pub device: Shared,
}

impl Base {
    fn setting(&self, item: &str) -> Option<Setting> {
        match item.strip_prefix("custom.") {
            Some(id) => SETTINGS
                .iter()
                .find(|s| s.custom_id() == id && matches!(s.place, Place::Custom { .. }))
                .copied(),
            None => WheelBaseParam::from_key(item).and_then(|p| self.device.borrow().setting(p)),
        }
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
        SETTINGS
            .iter()
            .enumerate()
            .filter_map(|(order, s)| s.custom_param(&d.hid, d.max_range(), order as i32))
            .collect()
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = self
            .setting(&format!("custom.{id}"))
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        setting.write(&self.device.borrow().hid, value)
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        let setting = self
            .setting(item)
            .ok_or_else(|| Error::NotFound(format!("no setting '{item}'")))?;
        let raw = setting.raw(value)?;
        // An option of the hid-tmff2 module: applied to every wheel it drives.
        host.privileged(
            "save-setting",
            &[("attr", setting.attr), ("value", &raw.to_string())],
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
        let range = d
            .hid
            .read_int("range")
            .map(|r| r as f64)
            .unwrap_or(d.max_range());
        d.hid.angle(range)
    }

    fn angular_velocity(&mut self) -> Support<f64> {
        Support::not_in_driver()
    }

    fn status(&mut self) -> Support<String> {
        let d = self.device.borrow();
        Support::Supported(if d.hid.hid.is_some() {
            "ok".into()
        } else {
            "the driver is not bound".into()
        })
    }

    fn firmware(&mut self) -> Support<String> {
        Support::not_in_driver()
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        let d = self.device.borrow();
        match d.setting(param) {
            Some(setting) => setting.spec(&d.hid, d.max_range()),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let d = self.device.borrow();
        let setting = d.setting(param).ok_or_else(Error::unsupported)?;
        setting.write(&d.hid, value)
    }

    fn action(&mut self, action: WheelBaseAction) -> Support<ActionSpec> {
        let d = self.device.borrow();
        if let Some(u) = d.hid.unbound() {
            return Support::Unsupported(u);
        }
        if let Some(spec) = d.hid.force_action(action) {
            return spec;
        }
        match action {
            WheelBaseAction::SetCenter => Support::Unsupported(
                Unsupported::not_in_driver()
                    .with_detail("The wheel finds its center by itself when it is plugged in."),
            ),
            _ => Support::not_in_driver(),
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
                .with_detail("hid-tmff2 does not say which rim is mounted."),
        )
    }
}

/// The pedals plugged into a Thrustmaster base.
pub fn pedals(device: &Shared) -> PedalSet<TmDevice> {
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", device.borrow().model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
