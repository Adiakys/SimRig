//! Guided procedures of the simulated rig: a pedal calibration, and the setup of a generic
//! device (what a plugin for unbranded USB devices would offer, e.g. mapping a shifter).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use simrig_plugin_sdk::prelude::*;

use crate::pedals;
use crate::sim::{RAW_MAX, Setup, Shared, SimDriver};

/// Simulated time the user takes to move a lever to the asked position.
const DETECTION_DELAY: Duration = Duration::from_millis(80);

fn step_actions(step: Step) -> Step {
    step.action(StepAction::next()).action(StepAction::cancel())
}

// ------------------------------------------------------------------ pedal calibration

enum Stage {
    Release,
    Press,
    Summary,
}

/// Release, press, check: captures the pedal's travel and applies it as its calibration.
pub struct PedalCalibration {
    shared: Shared,
    pedal: PedalRole,
    stage: Stage,
    min: i64,
    max: i64,
    shown: Option<i64>,
}

impl PedalCalibration {
    pub fn new(shared: Shared, pedal: PedalRole) -> Self {
        Self {
            shared,
            pedal,
            stage: Stage::Release,
            min: 0,
            max: RAW_MAX,
            shown: None,
        }
    }

    /// The simulated pedal follows the instruction of the current step.
    fn follow(&self) {
        let position = match self.stage {
            Stage::Release => Some(0.02),
            Stage::Press => Some(0.97),
            Stage::Summary => None,
        };
        self.shared.get().pedal_override = position.map(|p| (pedals::index(self.pedal), p));
    }

    fn step(&mut self) -> Step {
        let name = self.pedal.key();
        let raw = pedals::raw(&self.shared, self.pedal);
        self.shown = Some(raw);
        let live = Element::Value {
            label: "Raw position".into(),
            value: Value::Int(raw),
            unit: None,
        };
        match self.stage {
            Stage::Release => step_actions(
                Step::new(&format!("Release the {name} completely"))
                    .text("Take your foot off the pedal, then continue.")
                    .element(live)
                    .progress(1, 3),
            ),
            Stage::Press => step_actions(
                Step::new(&format!("Press the {name} all the way"))
                    .text("Hold it at the end of its travel, then continue.")
                    .element(live)
                    .progress(2, 3),
            ),
            Stage::Summary => {
                let table = Element::Table {
                    columns: vec!["".into(), "Raw value".into()],
                    rows: vec![
                        vec!["Released".into(), self.min.to_string()],
                        vec!["Pressed".into(), self.max.to_string()],
                    ],
                };
                let mut step = Step::new("Check the result").element(table).progress(3, 3);
                if self.min < self.max {
                    step = step.action(StepAction::new("finish", "Apply", ActionRole::Primary));
                } else {
                    step = step.element(Element::notice(
                        Level::Error,
                        "The pressed position is not beyond the released one.",
                    ));
                }
                step.action(StepAction::new(
                    "restart",
                    "Start again",
                    ActionRole::Secondary,
                ))
                .action(StepAction::cancel())
            }
        }
    }
}

impl Procedure for PedalCalibration {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        self.follow();
        Ok(Outcome::Step(self.step()))
    }

    fn submit(
        &mut self,
        action: &str,
        _values: &BTreeMap<String, Value>,
        _host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        let raw = pedals::raw(&self.shared, self.pedal);
        match (&self.stage, action) {
            (_, "cancel") => {
                self.cancel();
                return Ok(Outcome::Cancelled);
            }
            (Stage::Release, "next") => {
                self.min = raw;
                self.stage = Stage::Press;
            }
            (Stage::Press, "next") => {
                self.max = raw;
                self.stage = Stage::Summary;
            }
            (Stage::Summary, "restart") => self.stage = Stage::Release,
            (Stage::Summary, "finish") => {
                let key = |item: &str| format!("{}.{item}", self.pedal.key());
                return Ok(Outcome::Finished {
                    message: Some(format!("{} calibrated.", self.pedal.key())),
                    apply: vec![
                        (key("calibration_min"), Value::Int(self.min)),
                        (key("calibration_max"), Value::Int(self.max)),
                    ],
                });
            }
            (_, other) => return Err(Error::InvalidValue(format!("unexpected action '{other}'"))),
        }
        self.follow();
        Ok(Outcome::Step(self.step()))
    }

    /// Live raw position while the user moves the pedal.
    fn poll(&mut self) -> Option<Outcome> {
        if matches!(self.stage, Stage::Summary) {
            return None;
        }
        let raw = pedals::raw(&self.shared, self.pedal);
        (Some(raw) != self.shown).then(|| Outcome::Step(self.step()))
    }

    fn cancel(&mut self) {
        self.shared.get().pedal_override = None;
    }
}

// ------------------------------------------------------------------ generic device setup

enum SetupStage {
    Kind,
    Detect(usize),
    Summary,
}

/// Turns an unconfigured device into a shifter or a handbrake: choose the type, move the
/// lever to each position (detected by the simulated device), check, save.
pub struct DeviceSetup {
    shared: Shared,
    device: String,
    stage: SetupStage,
    kind: String,
    gears: i64,
    pedals: i64,
    detected: Vec<(String, String)>,
    since: Instant,
}

impl DeviceSetup {
    pub fn new(shared: Shared, device: &str) -> Self {
        Self {
            shared,
            device: device.into(),
            stage: SetupStage::Kind,
            kind: "shifter".into(),
            gears: 6,
            pedals: 3,
            detected: Vec::new(),
            since: Instant::now(),
        }
    }

    /// What the user moves, one after the other, so the device's inputs can be detected.
    fn positions(&self) -> Vec<String> {
        match self.kind.as_str() {
            "shifter" => (1..=self.gears)
                .map(|g| format!("Gear {g}"))
                .chain(["Reverse".to_string()])
                .collect(),
            "sequential" => vec!["Shift up".into(), "Shift down".into()],
            "pedals" => Setup::pedal_roles(self.pedals)
                .into_iter()
                .map(|r| {
                    let name = r.key();
                    let mut chars = name.chars();
                    let first = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or(' ');
                    format!("{first}{}", chars.as_str())
                })
                .collect(),
            _ => vec!["Pulled".into(), "Released".into()],
        }
    }

    fn setup(&self) -> Setup {
        match self.kind.as_str() {
            "shifter" => Setup::Shifter { gears: self.gears },
            "sequential" => Setup::SequentialShifter,
            "pedals" => Setup::Pedals { count: self.pedals },
            _ => Setup::Handbrake,
        }
    }

    fn driver_installed(&self) -> bool {
        self.shared
            .get()
            .driver
            .as_ref()
            .is_some_and(SimDriver::installed)
    }

    fn step(&self) -> Step {
        match self.stage {
            SetupStage::Kind => step_actions(
                Step::new("What is this device?")
                    .text("Choose what to configure it as.")
                    .element(Element::field(
                        "kind",
                        "Device type",
                        ParamType::Choice {
                            options: vec![
                                ChoiceOption::new("shifter", "H-pattern shifter"),
                                ChoiceOption::new("sequential", "Sequential shifter"),
                                ChoiceOption::new("handbrake", "Handbrake"),
                                ChoiceOption::new("pedals", "Pedals"),
                            ],
                        },
                        Value::Choice(self.kind.clone()),
                    ))
                    .element(Element::field(
                        "gears",
                        "Forward gears (H-pattern)",
                        ParamType::Int {
                            min: 4,
                            max: 8,
                            step: 1,
                        },
                        Value::Int(self.gears),
                    ))
                    .element(Element::field(
                        "pedals",
                        "Pedals (pedal sets)",
                        ParamType::Int {
                            min: 1,
                            max: 3,
                            step: 1,
                        },
                        Value::Int(self.pedals),
                    )),
            ),
            SetupStage::Detect(i) => {
                let positions = self.positions();
                let title = match (self.kind.as_str(), positions[i].as_str()) {
                    ("shifter", p) => format!("Move the lever to {}", p.to_lowercase()),
                    ("sequential", p) => format!("{p} once"),
                    ("pedals", p) => format!("Press the {} all the way", p.to_lowercase()),
                    (_, "Pulled") => "Pull the handbrake fully".into(),
                    _ => "Release the handbrake".into(),
                };
                Step::new(&title)
                    .text("Waiting for the device…")
                    .element(Element::value(
                        "Detected so far",
                        Value::Int(self.detected.len() as i64),
                    ))
                    .waiting()
                    .progress(i as u32 + 1, positions.len() as u32)
                    .action(StepAction::cancel())
            }
            SetupStage::Summary => {
                let rows = self
                    .detected
                    .iter()
                    .map(|(p, input)| vec![p.clone(), input.clone()])
                    .collect();
                let notice = if self.driver_installed() {
                    Element::notice(
                        Level::Info,
                        "The driver will restore this setup when the device is plugged in.",
                    )
                } else {
                    Element::notice(
                        Level::Warning,
                        "The driver is not installed: this setup lasts until the app is closed.",
                    )
                };
                Step::new("Check the mapping")
                    .element(Element::Table {
                        columns: vec!["Position".into(), "Device input".into()],
                        rows,
                    })
                    .element(notice)
                    .action(StepAction::new("finish", "Save", ActionRole::Primary))
                    .action(StepAction::new(
                        "restart",
                        "Start again",
                        ActionRole::Secondary,
                    ))
                    .action(StepAction::cancel())
            }
        }
    }
}

impl Procedure for DeviceSetup {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        Ok(Outcome::Step(self.step()))
    }

    fn submit(
        &mut self,
        action: &str,
        values: &BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        match (&self.stage, action) {
            (_, "cancel") => return Ok(Outcome::Cancelled),
            (SetupStage::Kind, "next") => {
                self.kind = values
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("shifter")
                    .into();
                self.gears = values.get("gears").and_then(Value::as_f64).unwrap_or(6.0) as i64;
                self.pedals = values.get("pedals").and_then(Value::as_f64).unwrap_or(3.0) as i64;
                self.detected.clear();
                self.stage = SetupStage::Detect(0);
                self.since = Instant::now();
            }
            (SetupStage::Summary, "restart") => self.stage = SetupStage::Kind,
            (SetupStage::Summary, "finish") => {
                let setup = self.setup();
                let saved = self.driver_installed();
                if saved {
                    let key = format!("{}.setup", self.device);
                    host.privileged(
                        "save-setting",
                        &[("key", &key), ("value", &setup.to_text())],
                    )?;
                }
                let what = match &setup {
                    Setup::Shifter { gears } => format!("an H-pattern shifter with {gears} gears"),
                    Setup::SequentialShifter => "a sequential shifter".into(),
                    Setup::Handbrake => "a handbrake".into(),
                    Setup::Pedals { count: 1 } => "a single pedal".into(),
                    Setup::Pedals { count } => format!("a set of {count} pedals"),
                };
                let note = if saved {
                    ""
                } else {
                    " (not saved: install the driver to keep it)"
                };
                self.shared.get().setups.insert(self.device.clone(), setup);
                return Ok(Outcome::finished(format!("Set up as {what}{note}.")));
            }
            (_, other) => return Err(Error::InvalidValue(format!("unexpected action '{other}'"))),
        }
        Ok(Outcome::Step(self.step()))
    }

    /// The simulated device "detects" each position shortly after it is asked for.
    fn poll(&mut self) -> Option<Outcome> {
        let SetupStage::Detect(i) = self.stage else {
            return None;
        };
        if self.since.elapsed() < DETECTION_DELAY {
            return None;
        }
        let positions = self.positions();
        self.detected.push((
            positions[i].clone(),
            format!("byte 2 = 0x{:02x}", 1u32 << i.min(7)),
        ));
        self.stage = if i + 1 < positions.len() {
            SetupStage::Detect(i + 1)
        } else {
            SetupStage::Summary
        };
        self.since = Instant::now();
        Some(Outcome::Step(self.step()))
    }
}
