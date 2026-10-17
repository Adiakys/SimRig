//! Pedals read through the event node: which axis is which pedal (learned by the "Map the
//! pedals" procedure and kept in the user's configuration), and pedals as a standard component.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use simrig_plugin_sdk::prelude::*;

use crate::device::HasHid;
use crate::role_label;

/// Which axis is which pedal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedalMap {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pedals: BTreeMap<String, MappedAxis>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedAxis {
    /// evdev axis code.
    pub code: u16,
    /// Pressed goes towards the axis minimum.
    #[serde(default)]
    pub inverted: bool,
}

impl PedalMap {
    /// The pedals mapped, as on the floor: clutch, brake, throttle.
    pub fn roles(&self) -> Vec<PedalRole> {
        [PedalRole::Clutch, PedalRole::Brake, PedalRole::Throttle]
            .into_iter()
            .filter(|r| self.pedals.contains_key(r.key()))
            .collect()
    }
}

/// The mappings of every model, in a plugin's configuration file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    models: BTreeMap<String, PedalMap>,
}

pub fn load_map(config: &Path, key: &str) -> PedalMap {
    fs::read_to_string(config)
        .ok()
        .and_then(|t| toml::from_str::<Config>(&t).ok())
        .and_then(|c| c.models.get(key).cloned())
        .unwrap_or_default()
}

pub fn save_map(config: &Path, key: &str, map: &PedalMap) -> std::io::Result<()> {
    let mut all: Config = fs::read_to_string(config)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    all.models.insert(key.into(), map.clone());
    if let Some(dir) = config.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(
        config,
        toml::to_string_pretty(&all).map_err(std::io::Error::other)?,
    )
}

/// What a brand's pedals cannot do through its driver, said on the parameters.
#[derive(Clone, Copy, Debug)]
pub struct PedalNotes {
    /// Calibration (minimum, maximum, the calibrate action).
    pub calibration: &'static str,
    /// Dead zones and curves.
    pub shaping: &'static str,
    /// The brake's maximum force.
    pub max_force: &'static str,
}

impl Default for PedalNotes {
    fn default() -> Self {
        Self {
            calibration: "The driver has no calibration: use the device's own tool or the game.",
            shaping: "The driver has no dead zones or curves: set them in the game.",
            max_force: "Not adjustable through the driver.",
        }
    }
}

/// Pedals as a standard component, once mapped.
pub struct PedalSet<D: HasHid> {
    pub device: Rc<RefCell<D>>,
    pub name: String,
    pub model: Option<String>,
    pub notes: PedalNotes,
}

impl<D: HasHid> Component for PedalSet<D> {
    fn id(&self) -> String {
        format!("{}/pedals", self.device.borrow().hid().id)
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn model(&self) -> Option<String> {
        self.model.clone()
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        vec![descriptor(&self.device)]
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        start(&self.device, id)
    }
}

impl<D: HasHid> Pedals for PedalSet<D> {
    fn pedals(&self) -> Vec<PedalRole> {
        self.device.borrow().hid().pedal_map.roles()
    }

    fn position(&mut self, pedal: PedalRole) -> Support<Axis> {
        let d = self.device.borrow();
        let d = d.hid();
        if let Some(u) = d.unbound() {
            return Support::Unsupported(u);
        }
        let Some(mapped) = d.pedal_map.pedals.get(pedal.key()).copied() else {
            return Support::Unsupported(
                Unsupported::not_in_driver().with_detail("Map the pedals first."),
            );
        };
        let Some(axis) = d.inputs.axes().get(&mapped.code).copied() else {
            return Support::Unsupported(Unsupported::new(Reason::Unavailable));
        };
        let unit = axis.unit();
        Support::Supported(Axis {
            raw: i64::from(axis.value),
            value: if mapped.inverted { 1.0 - unit } else { unit },
        })
    }

    fn force(&mut self, _pedal: PedalRole) -> Support<Kilograms> {
        Support::not_in_driver()
    }

    fn param(&mut self, _pedal: PedalRole, param: PedalParam) -> Support<ParamSpec> {
        let detail = match param {
            PedalParam::CalibrationMin | PedalParam::CalibrationMax => self.notes.calibration,
            PedalParam::DeadzoneLow | PedalParam::DeadzoneHigh | PedalParam::Curve => {
                self.notes.shaping
            }
            PedalParam::MaxForce => self.notes.max_force,
            PedalParam::Inverted | PedalParam::Vibration => return Support::not_in_driver(),
        };
        Support::Unsupported(Unsupported::not_in_driver().with_detail(detail))
    }

    fn action(&mut self, _pedal: PedalRole, action: PedalAction) -> Support<ActionSpec> {
        match action {
            PedalAction::Calibrate => Support::Unsupported(
                Unsupported::not_in_driver().with_detail(self.notes.calibration),
            ),
            PedalAction::VibrationTest => Support::not_in_driver(),
        }
    }
}

/// The id of the mapping procedure, on the device and on its components.
pub const MAP_PEDALS: &str = "map-pedals";

pub fn descriptor<D: HasHid>(device: &Rc<RefCell<D>>) -> ProcedureDescriptor {
    let d = device.borrow();
    let d = d.hid();
    let label = match (d.pedal_map.pedals.is_empty(), d.steering.is_some()) {
        (false, _) => "Map the pedals again",
        (true, true) => "Map the pedals connected to the base",
        (true, false) => "Map the pedals",
    };
    let mut descriptor = ProcedureDescriptor::new(MAP_PEDALS, label)
        .help("Press each pedal when asked, so SimRig knows which input is which.");
    if let Some(u) = d.unbound() {
        descriptor = descriptor.availability(Support::Unsupported(u));
    }
    descriptor
}

pub fn start<D: HasHid>(device: &Rc<RefCell<D>>, id: &str) -> Result<Box<dyn Procedure>> {
    if id != MAP_PEDALS {
        return Err(Error::NotFound(format!("unknown procedure '{id}'")));
    }
    Ok(Box::new(MapPedals::new(device.clone())))
}

/// The guided mapping: at rest, then each pedal pressed in turn; the axis that moves most is
/// that pedal. Saved in the user's configuration (it only tells SimRig how to read them).
pub struct MapPedals<D: HasHid> {
    device: Rc<RefCell<D>>,
    rest: BTreeMap<u16, f64>,
    /// The pedal being pressed, by index in `ORDER`.
    step: usize,
    map: PedalMap,
}

const ORDER: [PedalRole; 3] = [PedalRole::Throttle, PedalRole::Brake, PedalRole::Clutch];
/// How far an axis must move to count as the pedal pressed.
const PRESSED: f64 = 0.5;

impl<D: HasHid> MapPedals<D> {
    pub fn new(device: Rc<RefCell<D>>) -> Self {
        Self {
            device,
            rest: BTreeMap::new(),
            step: 0,
            map: PedalMap::default(),
        }
    }

    fn axes(&self) -> BTreeMap<u16, f64> {
        let d = self.device.borrow();
        let d = d.hid();
        d.inputs
            .axes()
            .into_iter()
            .filter(|(code, _)| Some(*code) != d.steering)
            .map(|(code, a)| (code, a.unit()))
            .collect()
    }

    fn table(&self) -> Element {
        Element::Table {
            columns: vec!["Pedal".into(), "Input".into()],
            rows: ORDER
                .iter()
                .map(|role| {
                    let input = match self.map.pedals.get(role.key()) {
                        Some(m) => format!(
                            "axis {}{}",
                            m.code,
                            if m.inverted { ", inverted" } else { "" }
                        ),
                        None => "not mapped".into(),
                    };
                    vec![role_label(*role).into(), input]
                })
                .collect(),
        }
    }

    fn press_step(&self) -> Step {
        let role = ORDER[self.step];
        let name = role_label(role).to_lowercase();
        Step::new(&format!("Press the {name} all the way"))
            .text("Hold it until it is detected, then release it.")
            .element(self.table())
            .preview(Preview::new(ComponentKind::Pedals).target(role.key()))
            .waiting()
            .progress(self.step as u32 + 1, ORDER.len() as u32)
            .action(StepAction::new(
                "skip",
                &format!("No {name}"),
                ActionRole::Secondary,
            ))
            .action(StepAction::cancel())
    }

    fn summary(&self) -> Step {
        Step::new("Check the pedals")
            .element(self.table())
            .action(StepAction::new("save", "Save", ActionRole::Primary))
            .action(StepAction::new(
                "restart",
                "Start again",
                ActionRole::Secondary,
            ))
            .action(StepAction::cancel())
    }

    fn next(&mut self) -> Outcome {
        self.step += 1;
        if self.step < ORDER.len() {
            Outcome::Step(self.press_step())
        } else {
            Outcome::Step(self.summary())
        }
    }
}

impl<D: HasHid> Procedure for MapPedals<D> {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        if let Some(u) = self.device.borrow().hid().unbound() {
            return Err(Error::failed(u.explain()));
        }
        Ok(Outcome::Step(
            Step::new("Release every pedal")
                .text("Feet off the pedals, then continue.")
                .action(StepAction::next())
                .action(StepAction::cancel()),
        ))
    }

    fn submit(
        &mut self,
        action: &str,
        _values: &BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        match action {
            "cancel" => Ok(Outcome::Cancelled),
            "next" => {
                self.rest = self.axes();
                self.step = 0;
                self.map = PedalMap::default();
                Ok(Outcome::Step(self.press_step()))
            }
            "skip" => Ok(self.next()),
            "restart" => self.start(host),
            "save" => {
                self.device
                    .borrow_mut()
                    .hid_mut()
                    .save_map(self.map.clone())
                    .map_err(|e| Error::failed(e.to_string()))?;
                let n = self.map.pedals.len();
                Ok(Outcome::finished(format!(
                    "{n} pedal{} mapped.",
                    if n == 1 { "" } else { "s" }
                )))
            }
            other => Err(Error::InvalidValue(format!("unexpected action '{other}'"))),
        }
    }

    fn poll(&mut self) -> Option<Outcome> {
        if self.step >= ORDER.len() || self.rest.is_empty() {
            return None;
        }
        let taken: Vec<u16> = self.map.pedals.values().map(|m| m.code).collect();
        let moved = self
            .axes()
            .into_iter()
            .filter(|(code, _)| !taken.contains(code))
            .filter_map(|(code, now)| {
                let rest = *self.rest.get(&code)?;
                let delta = now - rest;
                (delta.abs() >= PRESSED).then_some((code, delta))
            })
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))?;
        self.map.pedals.insert(
            ORDER[self.step].key().into(),
            MappedAxis {
                code: moved.0,
                inverted: moved.1 < 0.0,
            },
        );
        Some(self.next())
    }
}
