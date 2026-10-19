//! Thrustmaster wheels as standard components, over hid-tmff2's sysfs attributes and event
//! node.

use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::settings::Setting;
use simrig_plugin_sdk::model::{Appearance, RimShape, StdItem};
use simrig_plugin_sdk::prelude::*;

use crate::device::{Shared, TmDevice};
use crate::models::{self, Driver};

pub struct Base {
    pub device: Shared,
}

impl Base {
    fn setting(&self, item: &str) -> Option<Setting> {
        let d = self.device.borrow();
        match item.strip_prefix("custom.") {
            Some(id) => d.custom(id),
            None => WheelBaseParam::from_key(item).and_then(|p| d.setting(p)),
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
        d.settings()
            .iter()
            .enumerate()
            .filter_map(|(order, s)| s.custom_param(&d.hid, d.max_range(), order as i32))
            .collect()
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = self
            .setting(&format!("custom.{id}"))
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        setting.write(&mut self.device.borrow_mut().hid, value)
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        let setting = self
            .setting(item)
            .ok_or_else(|| Error::NotFound(format!("no setting '{item}'")))?;
        if !setting.saved_by_attr(&self.device.borrow().hid) {
            return Err(Error::failed(
                "The driver has no setting to keep: it lasts until the wheel is unplugged.",
            ));
        }
        let raw = setting.raw(value)?;
        let pid = format!("{:04x}", self.device.borrow().model.pid);
        // An option of the hid-tmff2 module, applied to every wheel it drives; for hid-t150, a
        // udev rule for this model.
        host.privileged(
            "save-setting",
            &[
                ("pid", &pid),
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
        if let (WheelBaseParam::Rotation, Driver::Generic { why }) = (param, d.model.driver) {
            let range = d.max_range();
            // A wheel without force feedback has no motor to limit the rotation with.
            return Support::Unsupported(if why == models::NO_FFB {
                Unsupported::new(Reason::NotInHardware)
                    .with_detail(format!("The rotation is fixed at {range}°."))
            } else {
                Unsupported::not_in_driver().with_detail(format!(
                    "The wheel keeps its own rotation, {range}°: no Linux driver sets it."
                ))
            });
        }
        match d.setting(param) {
            Some(setting) => setting.spec(&d.hid, d.max_range()),
            None => Support::not_in_driver(),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let mut d = self.device.borrow_mut();
        let setting = d.setting(param).ok_or_else(Error::unsupported)?;
        setting.write(&mut d.hid, value)
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
    fn controls(&mut self) -> Vec<Control> {
        self.device.borrow().hid.controls(&[], &[])
    }

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
    fn hats(&mut self) -> Support<Vec<i64>> {
        self.device.borrow().hid.hats()
    }

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

/// The pedals plugged into a Thrustmaster base.
pub fn pedals(device: &Shared) -> PedalSet<TmDevice> {
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", device.borrow().model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
