//! The setup of a generic device: what it is, then its inputs mapped by moving it, then saved
//! into the driver. Every step is detected by the plugin while the user moves the device.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use simrig_plugin_sdk::prelude::*;

use crate::adapter::{Shared, joystick_name, save_config};
use crate::config::{DeviceConfig, HandbrakeSetup, Setup, Signature};
use crate::detect::{self, HandbrakeFound, PositionWatch, Rest, RestWatch, Seen};

/// How long the handbrake must stay at its furthest point to be taken as fully pulled.
const PULL_HOLD: Duration = Duration::from_millis(600);

const KINDS: [(&str, &str); 3] = [
    ("h_shifter", "H-pattern shifter"),
    ("sequential", "Sequential shifter"),
    ("handbrake", "Handbrake"),
];

enum Stage {
    /// The device cannot be read yet: the driver grants it, for this device only.
    Access,
    Kind,
    Rest(Vec<RestWatch>),
    /// The n-th position of a shifter.
    Position(usize, Vec<PositionWatch>),
    /// The handbrake, pulled fully and held.
    Pull {
        furthest: Option<(usize, Vec<u8>, u64)>,
        since: Instant,
    },
    Summary,
}

pub struct DeviceSetup {
    state: Shared,
    driver_installed: bool,
    stage: Stage,
    kind: String,
    gears: i64,
    reverse_position: String,
    rests: Vec<Option<Rest>>,
    interface: Option<usize>,
    taken: Vec<Signature>,
    handbrake: Option<HandbrakeFound>,
    notice: Option<(Level, String)>,
}

impl DeviceSetup {
    pub fn new(state: Shared, driver_installed: bool) -> Self {
        let (kind, gears, reverse_position) = match state.borrow().saved.as_ref().map(|c| &c.setup)
        {
            Some(Setup::HShifter {
                gears,
                reverse_position,
                ..
            }) => ("h_shifter", i64::from(*gears), reverse_position.clone()),
            Some(Setup::Sequential { .. }) => ("sequential", 6, "right_down".into()),
            Some(Setup::Handbrake(_)) => ("handbrake", 6, "right_down".into()),
            None => ("h_shifter", 6, "right_down".into()),
        };
        Self {
            state,
            driver_installed,
            stage: Stage::Kind,
            kind: kind.into(),
            gears,
            reverse_position,
            rests: Vec::new(),
            interface: None,
            taken: Vec::new(),
            handbrake: None,
            notice: None,
        }
    }

    fn interfaces(&self) -> usize {
        self.state
            .borrow()
            .reports
            .as_ref()
            .map(|r| r.all().len())
            .unwrap_or(0)
    }

    /// Names of the positions to map, in order.
    fn positions(&self) -> Vec<String> {
        match self.kind.as_str() {
            "h_shifter" => (1..=self.gears)
                .map(ordinal)
                .chain(["reverse".to_string()])
                .collect(),
            "sequential" => vec!["shift up".into(), "shift down".into()],
            _ => Vec::new(),
        }
    }

    fn step(&self) -> Step {
        let mut step = match &self.stage {
            Stage::Access => {
                let mut step = Step::new("Allow reading the device").text(
                    "Mapping the device means reading its raw inputs. The driver allows it \
                         for this device only, once.",
                );
                if !self.driver_installed {
                    step = step.element(Element::notice(
                        Level::Error,
                        "The Generic USB driver is not installed: install it from Plugins first.",
                    ));
                }
                step.action(StepAction::new("allow", "Allow", ActionRole::Primary))
                    .action(StepAction::cancel())
            }
            Stage::Kind => Step::new("What is this device?")
                .text("Choose what it becomes. Games will see it as a standard joystick.")
                .element(Element::field(
                    "kind",
                    "Device type",
                    ParamType::Choice {
                        options: KINDS
                            .iter()
                            .map(|(id, l)| ChoiceOption::new(id, l))
                            .collect(),
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
                    "reverse_position",
                    "Reverse (H-pattern)",
                    ParamType::Choice {
                        options: vec![
                            ChoiceOption::new("right_down", "Right, down"),
                            ChoiceOption::new("left_up", "Left, up"),
                        ],
                    },
                    Value::Choice(self.reverse_position.clone()),
                ))
                .element(Element::notice(
                    Level::Info,
                    "Pedals and wheels are not supported by this plugin yet.",
                ))
                .action(StepAction::next())
                .action(StepAction::cancel()),
            Stage::Rest(_) => Step::new("Leave the device at rest")
                .preview(self.preview(None))
                .text(
                    "Hands off for a moment: the rest position is learned, and anything that \
                     moves by itself is ignored.",
                )
                .waiting()
                .action(StepAction::cancel()),
            Stage::Position(i, _) => {
                let positions = self.positions();
                let name = &positions[*i];
                let title = match self.kind.as_str() {
                    "sequential" => format!("Hold the lever in {name}"),
                    _ if name == "reverse" => "Engage reverse and hold it".into(),
                    _ => format!("Move the lever to {name} gear and hold it"),
                };
                let target = match self.kind.as_str() {
                    "sequential" => if *i == 0 { "+" } else { "-" }.to_string(),
                    _ if name == "reverse" => "R".to_string(),
                    _ => (i + 1).to_string(),
                };
                Step::new(&title)
                    .text("The position is detected as soon as the lever stays still.")
                    .preview(self.preview(Some(target)))
                    .element(self.table())
                    .waiting()
                    .progress(*i as u32 + 1, positions.len() as u32)
                    .action(StepAction::cancel())
            }
            Stage::Pull { .. } => Step::new("Pull the handbrake fully and hold it")
                .preview(self.preview(Some("pulled".into())))
                .text("Its travel is measured from the rest position to the furthest point.")
                .waiting()
                .action(StepAction::cancel()),
            Stage::Summary => {
                let mut step = Step::new("Check the mapping").element(self.table());
                if self.driver_installed {
                    step = step
                        .element(Element::notice(
                            Level::Info,
                            "Saving puts the setup in the driver: games get a standard \
                             joystick, with SimRig closed too.",
                        ))
                        .action(StepAction::new("save", "Save", ActionRole::Primary));
                } else {
                    step = step.element(Element::notice(
                        Level::Warning,
                        "The Generic USB driver is not installed, so the setup cannot be saved. \
                         Install it from Plugins, then set the device up again.",
                    ));
                }
                step.action(StepAction::new(
                    "restart",
                    "Start again",
                    ActionRole::Secondary,
                ))
                .action(StepAction::cancel())
            }
        };
        if let Some((level, text)) = &self.notice {
            step = step.element(Element::notice(*level, text.clone()));
        }
        step
    }

    /// The component being set up, as the app draws it: the gate with the gear to engage, or
    /// the handbrake pulled.
    fn preview(&self, target: Option<String>) -> Preview {
        match self.kind.as_str() {
            "handbrake" => {
                let pulled = target.is_some();
                Preview::new(ComponentKind::Handbrake).value(
                    "position",
                    Value::Axis(Axis {
                        raw: 0,
                        value: if pulled { 1.0 } else { 0.0 },
                    }),
                )
            }
            kind => {
                let mode = if kind == "sequential" {
                    "sequential"
                } else {
                    "h_pattern"
                };
                let mut preview = Preview::new(ComponentKind::Shifter)
                    .value("mode", Value::Choice(mode.into()))
                    .value("gears", Value::Int(self.gears))
                    .value(
                        "reverse_position",
                        Value::Choice(self.reverse_position.clone()),
                    )
                    .value("gear", Value::Gear(Gear::Neutral));
                if let Some(target) = target {
                    preview = preview.target(target);
                }
                preview
            }
        }
    }

    fn table(&self) -> Element {
        let mut rows: Vec<Vec<String>> = Vec::new();
        match self.kind.as_str() {
            "handbrake" => {
                if let Some(found) = &self.handbrake {
                    let input = match &found.input {
                        crate::config::HandbrakeInput::Axis { index, size } => format!(
                            "{}-bit axis at byte {index}, {} to {}",
                            size * 8,
                            found.min,
                            found.max
                        ),
                        crate::config::HandbrakeInput::Button { pressed } => {
                            format!("switch, {}", pressed.describe())
                        }
                    };
                    rows.push(vec!["Handbrake".into(), input]);
                }
            }
            _ => {
                let positions = self.positions();
                for (i, name) in positions.iter().enumerate() {
                    let input = self
                        .taken
                        .get(i)
                        .map(Signature::describe)
                        .unwrap_or_else(|| "waiting".into());
                    rows.push(vec![capitalize(name), input]);
                }
            }
        }
        Element::Table {
            columns: vec!["Position".into(), "Device input".into()],
            rows,
        }
    }

    fn begin_mapping(&mut self) {
        self.rests = vec![None; self.interfaces()];
        self.interface = None;
        self.taken.clear();
        self.handbrake = None;
        self.notice = None;
        self.stage = Stage::Rest(
            (0..self.rests.len())
                .map(|_| RestWatch::default())
                .collect(),
        );
    }

    fn after_rest(&self) -> Stage {
        if self.kind == "handbrake" {
            Stage::Pull {
                furthest: None,
                since: Instant::now(),
            }
        } else {
            let n = self.rests.len();
            Stage::Position(0, (0..n).map(|_| PositionWatch::default()).collect())
        }
    }

    /// The setup the mapping describes.
    fn config(&self) -> Option<DeviceConfig> {
        let s = self.state.borrow();
        let setup = match self.kind.as_str() {
            "h_shifter" => {
                let gears = usize::try_from(self.gears).ok()?;
                if self.taken.len() != gears + 1 {
                    return None;
                }
                Setup::HShifter {
                    gears: gears as u8,
                    reverse_position: self.reverse_position.clone(),
                    forward: self.taken[..gears].to_vec(),
                    reverse: self.taken[gears].clone(),
                }
            }
            "sequential" => Setup::Sequential {
                up: self.taken.first()?.clone(),
                down: self.taken.get(1)?.clone(),
            },
            _ => {
                let found = self.handbrake.clone()?;
                Setup::Handbrake(HandbrakeSetup {
                    input: found.input,
                    calibration_min: found.min,
                    calibration_max: found.max,
                    deadzone: 2.0,
                    curve: vec![[0.0, 0.0], [1.0, 1.0]],
                    mode: "analog".into(),
                    button_threshold: 50.0,
                })
            }
        };
        Some(DeviceConfig {
            name: s.name.clone(),
            vid: s.vid,
            pid: s.pid,
            serial: s.serial.clone(),
            interface: self.interface.unwrap_or(0),
            setup,
        })
    }

    fn reports(&self) -> Vec<Option<Vec<u8>>> {
        self.state
            .borrow()
            .reports
            .as_ref()
            .map(|r| r.all())
            .unwrap_or_default()
    }

    /// Moves on when the device did what the step asks; true when the step changed.
    fn advance(&mut self, now: Instant) -> bool {
        let reports = self.reports();
        // Work on the stage taken out, so the rest of `self` stays usable.
        let stage = std::mem::replace(&mut self.stage, Stage::Summary);
        let (stage, moved) = match stage {
            Stage::Rest(mut watches) => {
                for (i, watch) in watches.iter_mut().enumerate() {
                    if let (Some(Some(report)), Some(None)) = (reports.get(i), self.rests.get(i)) {
                        self.rests[i] = watch.feed(report, now);
                    }
                }
                // Interfaces that never reported are silent: they cannot carry the inputs.
                let ready = self.rests.iter().any(Option::is_some)
                    && self
                        .rests
                        .iter()
                        .zip(&reports)
                        .all(|(rest, report)| rest.is_some() || report.is_none());
                if ready {
                    (self.after_rest(), true)
                } else {
                    (Stage::Rest(watches), false)
                }
            }
            Stage::Position(index, mut watches) => {
                let mut next = None;
                for (i, watch) in watches.iter_mut().enumerate() {
                    if self.interface.is_some_and(|chosen| chosen != i) {
                        continue;
                    }
                    let (Some(Some(report)), Some(Some(rest))) =
                        (reports.get(i), self.rests.get(i))
                    else {
                        continue;
                    };
                    match watch.feed(rest, report, &self.taken, now) {
                        Seen::Waiting => {}
                        Seen::AlreadyTaken(t) => next = Some(Err(t)),
                        Seen::Position(sig) => {
                            next = Some(Ok((i, sig)));
                            break;
                        }
                    }
                }
                match next {
                    None => (Stage::Position(index, watches), false),
                    Some(Err(t)) => {
                        let positions = self.positions();
                        let notice = format!(
                            "That is {} already: move to {}.",
                            named(&positions[t], &self.kind),
                            named(&positions[index], &self.kind)
                        );
                        let changed = self.notice.as_ref().map(|(_, n)| n) != Some(&notice);
                        self.notice = Some((Level::Info, notice));
                        (Stage::Position(index, watches), changed)
                    }
                    Some(Ok((i, sig))) => {
                        self.interface = Some(i);
                        self.taken.push(sig);
                        self.notice = None;
                        let stage = if index + 1 < self.positions().len() {
                            let n = watches.len();
                            Stage::Position(
                                index + 1,
                                (0..n).map(|_| PositionWatch::default()).collect(),
                            )
                        } else {
                            Stage::Summary
                        };
                        (stage, true)
                    }
                }
            }
            Stage::Pull {
                mut furthest,
                mut since,
            } => {
                for (i, report) in reports.iter().enumerate() {
                    let (Some(report), Some(Some(rest))) = (report, self.rests.get(i)) else {
                        continue;
                    };
                    let distance = distance(rest, report);
                    if distance > 0 && furthest.as_ref().is_none_or(|(_, _, d)| distance > *d) {
                        furthest = Some((i, report.clone(), distance));
                        since = now;
                    }
                }
                let held = now.duration_since(since) >= PULL_HOLD;
                match furthest.clone().filter(|_| held) {
                    Some((i, report, _)) => {
                        let rest = self.rests.get(i).cloned().flatten().unwrap_or_default();
                        self.handbrake = detect::handbrake(&rest, &report);
                        self.interface = Some(i);
                        (Stage::Summary, true)
                    }
                    None => (Stage::Pull { furthest, since }, false),
                }
            }
            other => (other, false),
        };
        self.stage = stage;
        moved
    }
}

/// How far a report is from the rest position, ignoring noise.
fn distance(rest: &Rest, report: &[u8]) -> u64 {
    report
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let base = rest.report.get(i).copied().unwrap_or(0);
            let noise = rest.noise.get(i).copied().unwrap_or(0);
            if (v ^ base) & !noise == 0 {
                0
            } else {
                u64::from(v.abs_diff(base))
            }
        })
        .sum()
}

fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, x) if x != 11 => "st",
        (2, x) if x != 12 => "nd",
        (3, x) if x != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn named(position: &str, kind: &str) -> String {
    match kind {
        "h_shifter" if position != "reverse" => format!("{position} gear"),
        _ => position.to_string(),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Procedure for DeviceSetup {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        if self.state.borrow().reports.is_none() {
            self.state.borrow_mut().reopen();
        }
        if self.state.borrow().reports.is_none() {
            self.stage = Stage::Access;
        }
        Ok(Outcome::Step(self.step()))
    }

    fn submit(
        &mut self,
        action: &str,
        values: &BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome> {
        if action == "cancel" {
            return Ok(Outcome::Cancelled);
        }
        match (&self.stage, action) {
            (Stage::Access, "allow") => {
                let (file, vid, pid) = {
                    let s = self.state.borrow();
                    (
                        s.file_stem(),
                        format!("{:04x}", s.vid),
                        format!("{:04x}", s.pid),
                    )
                };
                match host.privileged(
                    "allow-device",
                    &[("file", &file), ("vid", &vid), ("pid", &pid)],
                ) {
                    Ok(_) => {
                        // The permission is applied by udev: give it a moment.
                        for _ in 0..20 {
                            self.state.borrow_mut().reopen();
                            if self.state.borrow().reports.is_some() {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        match self.state.borrow().read_error.clone() {
                            None => {
                                self.notice = None;
                                self.stage = Stage::Kind;
                            }
                            Some(e) => self.notice = Some((Level::Error, e)),
                        }
                    }
                    Err(e) => self.notice = Some((Level::Error, e.to_string())),
                }
            }
            (Stage::Kind, "next") => {
                if let Some(Value::Choice(kind)) = values.get("kind") {
                    self.kind = kind.clone();
                }
                if let Some(gears) = values.get("gears").and_then(Value::as_f64) {
                    self.gears = gears as i64;
                }
                if let Some(Value::Choice(r)) = values.get("reverse_position") {
                    self.reverse_position = r.clone();
                }
                self.begin_mapping();
            }
            (Stage::Summary, "restart") => {
                self.notice = None;
                self.stage = Stage::Kind;
            }
            (Stage::Summary, "save") => {
                let config = self
                    .config()
                    .ok_or_else(|| Error::failed("the mapping is incomplete"))?;
                save_config(&config, host)?;
                let what = match &config.setup {
                    Setup::HShifter { gears, .. } => format!("a {gears}-gear H-pattern shifter"),
                    Setup::Sequential { .. } => "a sequential shifter".into(),
                    Setup::Handbrake(_) => "a handbrake".into(),
                };
                let name = joystick_name(&config);
                let mut s = self.state.borrow_mut();
                s.saved = Some(config.clone());
                s.draft = Some(config);
                return Ok(Outcome::finished(format!(
                    "Set up as {what}. Games see it as “{name}”."
                )));
            }
            (_, other) => {
                return Err(Error::InvalidValue(format!("unexpected action '{other}'")));
            }
        }
        Ok(Outcome::Step(self.step()))
    }

    fn poll(&mut self) -> Option<Outcome> {
        let moved = self.advance(Instant::now());
        moved.then(|| Outcome::Step(self.step()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(7), "7th");
        assert_eq!(capitalize("reverse"), "Reverse");
    }
}
