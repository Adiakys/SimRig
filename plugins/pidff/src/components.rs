//! The bases as standard components: inputs and force feedback through the kernel; the strength
//! and centering spring through its input interface (the bases' own settings stay in their
//! software).

use simrig_plugin_kit::input::FfControl;
use simrig_plugin_kit::pedals::{self, PedalNotes, PedalSet};
use simrig_plugin_kit::settings::{Place, Setting};
use simrig_plugin_sdk::model::{Appearance, RimShape};
use simrig_plugin_sdk::prelude::*;

use crate::device::{PidffDevice, Shared};

const OWN_SOFTWARE: &str = "Set it with the base's own software or on the base: SimRig cannot \
                            change it.";

/// Through the input interface only: these bases have no driver attributes.
const SETTINGS: &[Setting] = &[
    Setting {
        ff: Some(FfControl::Gain),
        missing: "The driver does not take it.",
        ..Setting::percent(
            "gain",
            "Force feedback strength",
            Place::Standard(WheelBaseParam::FfbStrength),
        )
    },
    Setting {
        ff: Some(FfControl::Autocenter),
        missing: "The driver does not take it.",
        help: "Pulls the wheel back to the center; games usually set it themselves.",
        ..Setting::percent(
            "autocenter",
            "Centering spring",
            Place::Custom {
                group: "Force feedback",
                advanced: false,
            },
        )
    },
];

pub struct Base {
    pub device: Shared,
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
        let range = f64::from(d.model.range);
        SETTINGS
            .iter()
            .enumerate()
            .filter_map(|(order, s)| s.custom_param(&d.hid, range, order as i32))
            .collect()
    }

    fn set_custom(&mut self, id: &str, value: &Value) -> Result<()> {
        let setting = SETTINGS
            .iter()
            .find(|s| s.custom_id() == id && matches!(s.place, Place::Custom { .. }))
            .ok_or_else(|| Error::NotFound(format!("no setting '{id}'")))?;
        setting.write(&mut self.device.borrow_mut().hid, value)
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
        d.hid.angle(f64::from(d.model.range))
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
        Support::not_in_driver()
    }

    fn param(&mut self, param: WheelBaseParam) -> Support<ParamSpec> {
        let d = self.device.borrow();
        match SETTINGS.iter().find(|s| s.place == Place::Standard(param)) {
            Some(setting) => setting.spec(&d.hid, f64::from(d.model.range)),
            None => Support::Unsupported(Unsupported::not_in_driver().with_detail(OWN_SOFTWARE)),
        }
    }

    fn set_param(&mut self, param: WheelBaseParam, value: &Value) -> Result<()> {
        let setting = SETTINGS
            .iter()
            .find(|s| s.place == Place::Standard(param))
            .ok_or_else(Error::unsupported)?;
        setting.write(&mut self.device.borrow_mut().hid, value)
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
            WheelBaseAction::SetCenter => {
                Support::Unsupported(Unsupported::not_in_driver().with_detail(OWN_SOFTWARE))
            }
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

/// The rim: its buttons come through the base.
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

/// The pedals plugged into the base, once mapped.
pub fn pedals(device: &Shared) -> PedalSet<PidffDevice> {
    PedalSet {
        device: device.clone(),
        name: format!("Pedals on the {}", device.borrow().model.name),
        model: None,
        notes: PedalNotes::default(),
    }
}
