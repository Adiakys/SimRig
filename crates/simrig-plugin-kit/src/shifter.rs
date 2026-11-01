//! Shifters whose positions are buttons, read through the event node: a sequential lever or an
//! H-pattern gate. Which button is which position is known to the plugin (a layout) or learned
//! by the "Map the shifter" procedure and kept in the user's configuration.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use simrig_plugin_sdk::prelude::*;

use crate::device::HasHid;
use crate::input;

/// The evdev key code of each position: `+` and `-`, or the gears (`1`…) and `R`.
pub type ShifterMap = BTreeMap<String, u16>;

/// A kind of shifter a device can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    Sequential,
    HPattern {
        gears: u8,
        /// `left_up` or `right_down`, as the standard parameter.
        reverse_position: &'static str,
    },
}

impl Gate {
    /// The procedure that maps a shifter of this kind.
    pub fn procedure(self) -> &'static str {
        match self {
            Gate::Sequential => "map-sequential-shifter",
            Gate::HPattern { .. } => "map-h-shifter",
        }
    }

    /// The positions, in the order they are mapped.
    fn positions(self) -> Vec<String> {
        match self {
            Gate::Sequential => vec!["+".into(), "-".into()],
            Gate::HPattern { gears, .. } => (1..=gears)
                .map(|g| g.to_string())
                .chain(["R".into()])
                .collect(),
        }
    }

    /// A shifter of this kind on these buttons, in the order of its positions.
    pub fn layout(self, buttons: &[u16]) -> ShifterMap {
        self.positions().into_iter().zip(buttons.to_vec()).collect()
    }
}

fn is_sequential(map: &ShifterMap) -> bool {
    map.contains_key("+")
}

fn pressed<D: HasHid>(device: &D) -> BTreeSet<u16> {
    device
        .hid()
        .inputs
        .buttons()
        .into_iter()
        .filter_map(|(code, pressed)| pressed.then_some(code))
        .collect()
}

const NO_GEARS: &str = "A sequential shifter has no gear positions.";

/// A position's name, for the button it is on.
fn position_name(position: &str) -> String {
    match position {
        "+" => "Shift up".into(),
        "-" => "Shift down".into(),
        "R" => "Reverse".into(),
        gear => format!("Gear {gear}"),
    }
}

/// A shifter as a standard component, once its buttons are known.
pub struct ButtonShifter<D: HasHid> {
    pub device: Rc<RefCell<D>>,
    pub name: String,
    pub model: Option<String>,
    /// The kinds of shifter the device can have.
    pub gates: &'static [Gate],
}

impl<D: HasHid> ButtonShifter<D> {
    /// The map and the buttons pressed, or why they cannot be read.
    fn read(&self) -> std::result::Result<(ShifterMap, BTreeSet<u16>), Unsupported> {
        let d = self.device.borrow();
        if let Some(u) = d.hid().unbound() {
            return Err(u);
        }
        if let Some(e) = d.hid().inputs.error() {
            return Err(Unsupported::new(Reason::Unavailable).with_detail(e));
        }
        let map = d.hid().shifter().ok_or_else(Unsupported::not_in_driver)?;
        Ok((map, pressed(&*d)))
    }
}

impl<D: HasHid> Component for ButtonShifter<D> {
    fn id(&self) -> String {
        format!("{}/shifter", self.device.borrow().hid().id)
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn model(&self) -> Option<String> {
        self.model.clone()
    }

    /// The device's buttons, the positions named. Not on a base: its buttons are the base's.
    fn controls(&mut self) -> Vec<Control> {
        let d = self.device.borrow();
        let hid = d.hid();
        if hid.steering.is_some() {
            return Vec::new();
        }
        let names: Vec<(u16, String)> = hid
            .shifter()
            .unwrap_or_default()
            .into_iter()
            .map(|(position, code)| (code, position_name(&position)))
            .collect();
        let labels: Vec<(u16, &str)> = names.iter().map(|(c, n)| (*c, n.as_str())).collect();
        hid.controls(&[], &labels)
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        descriptors(&self.device, self.gates)
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        start(&self.device, id, self.gates)
    }
}

impl<D: HasHid> Shifter for ButtonShifter<D> {
    fn gear(&mut self) -> Support<Gear> {
        let (map, pressed) = match self.read() {
            Ok(read) => read,
            Err(u) => return Support::Unsupported(u),
        };
        if is_sequential(&map) {
            return Support::not_in_hardware(NO_GEARS);
        }
        let engaged = map.iter().find(|(_, code)| pressed.contains(code));
        Support::Supported(match engaged.map(|(p, _)| p.as_str()) {
            None => Gear::Neutral,
            Some("R") => Gear::Reverse,
            Some(g) => Gear::Forward(g.parse().unwrap_or(0)),
        })
    }

    fn position_xy(&mut self) -> Support<[f64; 2]> {
        Support::not_in_hardware("The positions are buttons, not a lever position.")
    }

    fn sequential(&mut self) -> Support<[bool; 2]> {
        let (map, pressed) = match self.read() {
            Ok(read) => read,
            Err(u) => return Support::Unsupported(u),
        };
        if !is_sequential(&map) {
            return Support::not_in_hardware("An H-pattern shifter has no sequential mode.");
        }
        let held = |p: &str| map.get(p).is_some_and(|code| pressed.contains(code));
        Support::Supported([held("+"), held("-")])
    }

    fn buttons(&mut self) -> Support<Vec<bool>> {
        let d = self.device.borrow();
        let hid = d.hid();
        if let Some(u) = hid.unbound() {
            return Support::Unsupported(u);
        }
        if hid.steering.is_some() {
            return Support::Unsupported(Unsupported::not_in_driver().with_detail(
                "Plugged into a base, its buttons are the base's: they show with the base's rim.",
            ));
        }
        hid.buttons()
    }

    fn param(&mut self, param: ShifterParam) -> Support<ParamSpec> {
        let Some(map) = self.device.borrow().hid().shifter() else {
            return Support::not_in_driver();
        };
        // Set by the shifter itself: one value each, shown so the gate is drawn right.
        if is_sequential(&map) {
            return match param {
                ShifterParam::Mode => Support::Supported(
                    ParamSpec::choice("sequential", &[("sequential", "Sequential")]).persistent(),
                ),
                _ => Support::not_in_hardware(NO_GEARS),
            };
        }
        match param {
            ShifterParam::Mode => Support::Supported(
                ParamSpec::choice("h_pattern", &[("h_pattern", "H-pattern")]).persistent(),
            ),
            ShifterParam::Gears => {
                // The gears and reverse.
                let gears = map.len() as i64 - 1;
                Support::Supported(ParamSpec::int(gears, gears, gears, 1).persistent())
            }
            ShifterParam::ReversePosition => {
                let reverse = self
                    .gates
                    .iter()
                    .find_map(|g| match g {
                        Gate::HPattern {
                            reverse_position, ..
                        } => Some(*reverse_position),
                        Gate::Sequential => None,
                    })
                    .unwrap_or("right_down");
                Support::Supported(
                    ParamSpec::choice(
                        reverse,
                        &[("left_up", "Left, up"), ("right_down", "Right, down")],
                    )
                    .persistent(),
                )
            }
            ShifterParam::EngageThreshold => {
                Support::not_in_hardware("The gears are buttons: there is no threshold.")
            }
        }
    }
}

/// The mapping procedures of a device that can have these kinds of shifter: by hand, never
/// required (the plugin knows the shifter, or it is an accessory of a base).
pub fn descriptors<D: HasHid>(device: &Rc<RefCell<D>>, gates: &[Gate]) -> Vec<ProcedureDescriptor> {
    let d = device.borrow();
    let d = d.hid();
    gates
        .iter()
        .map(|gate| {
            let label = match (d.steering.is_some(), gate) {
                (true, Gate::Sequential) => "Map a sequential shifter connected to the base",
                (true, Gate::HPattern { .. }) => "Map an H-pattern shifter connected to the base",
                (false, _) => "Map the shifter by hand",
            };
            let descriptor = ProcedureDescriptor::new(gate.procedure(), label)
                .help("Move the shifter when asked, so SimRig knows which buttons it uses.");
            match d.unbound() {
                Some(u) => descriptor.availability(Support::Unsupported(u)),
                None => descriptor,
            }
        })
        .collect()
}

pub fn start<D: HasHid>(
    device: &Rc<RefCell<D>>,
    id: &str,
    gates: &[Gate],
) -> Result<Box<dyn Procedure>> {
    let gate = gates
        .iter()
        .find(|g| g.procedure() == id)
        .ok_or_else(|| Error::NotFound(format!("unknown procedure '{id}'")))?;
    Ok(Box::new(MapShifter {
        device: device.clone(),
        gate: *gate,
        positions: gate.positions(),
        rest: None,
        mapped: Vec::new(),
    }))
}

/// The guided mapping: the shifter at rest, then each position in turn; the button pressed
/// each time is that position.
struct MapShifter<D: HasHid> {
    device: Rc<RefCell<D>>,
    gate: Gate,
    positions: Vec<String>,
    /// Buttons already pressed at rest, ignored; `None` until the shifter is at rest.
    rest: Option<BTreeSet<u16>>,
    mapped: Vec<u16>,
}

impl<D: HasHid> MapShifter<D> {
    fn name(position: &str) -> String {
        match position {
            "+" => "Shift up".into(),
            "-" => "Shift down".into(),
            "R" => "Reverse".into(),
            g => format!("Gear {g}"),
        }
    }

    fn table(&self) -> Element {
        let buttons = self.device.borrow().hid().inputs.buttons();
        Element::Table {
            columns: vec!["Position".into(), "Input".into()],
            rows: self
                .positions
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    // As games number the buttons, else as the kernel names them.
                    let input = match self.mapped.get(i) {
                        None => "not mapped".into(),
                        Some(code) => match buttons.iter().position(|(c, _)| c == code) {
                            Some(n) => format!("button {}", n + 1),
                            None => input::key_name(*code),
                        },
                    };
                    vec![Self::name(p), input]
                })
                .collect(),
        }
    }

    fn preview(&self, target: Option<&str>) -> Preview {
        let preview = match self.gate {
            Gate::Sequential => Preview::new(ComponentKind::Shifter)
                .value("mode", Value::Choice("sequential".into())),
            Gate::HPattern {
                gears,
                reverse_position,
            } => Preview::new(ComponentKind::Shifter)
                .value("mode", Value::Choice("h_pattern".into()))
                .value("gears", Value::Int(gears.into()))
                .value("reverse_position", Value::Choice(reverse_position.into())),
        }
        .value("gear", Value::Gear(Gear::Neutral));
        match target {
            Some(t) => preview.target(t),
            None => preview,
        }
    }

    /// The position to move to next, or the mapping to check once all are mapped.
    fn step(&self) -> Step {
        let i = self.mapped.len();
        let Some(position) = self.positions.get(i) else {
            return Step::new("Check the mapping")
                .element(self.table())
                .action(StepAction::new("save", "Save", ActionRole::Primary))
                .action(StepAction::new(
                    "restart",
                    "Start again",
                    ActionRole::Secondary,
                ))
                .action(StepAction::cancel());
        };
        let title = match position.as_str() {
            "+" | "-" => format!("{} and hold the lever", Self::name(position)),
            _ => format!("Engage {} and hold it", Self::name(position).to_lowercase()),
        };
        Step::new(&title)
            .text("Hold it until it is detected, then release it.")
            .element(self.table())
            .preview(self.preview(Some(position)))
            .waiting()
            .progress(i as u32 + 1, self.positions.len() as u32)
            .action(StepAction::cancel())
    }
}

impl<D: HasHid> Procedure for MapShifter<D> {
    fn start(&mut self, _host: &mut HostLink<'_>) -> Result<Outcome> {
        if let Some(u) = self.device.borrow().hid().unbound() {
            return Err(Error::failed(u.explain()));
        }
        self.rest = None;
        self.mapped.clear();
        Ok(Outcome::Step(
            Step::new("Leave the lever at rest")
                .text("Hands off the lever, then continue.")
                .preview(self.preview(None))
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
                self.rest = Some(pressed(&*self.device.borrow()));
                Ok(Outcome::Step(self.step()))
            }
            "restart" => self.start(host),
            "save" => {
                self.device
                    .borrow_mut()
                    .hid_mut()
                    .save_shifter(self.gate.layout(&self.mapped))
                    .map_err(|e| Error::failed(e.to_string()))?;
                Ok(Outcome::finished("Shifter mapped."))
            }
            other => Err(Error::InvalidValue(format!("unexpected action '{other}'"))),
        }
    }

    fn poll(&mut self) -> Option<Outcome> {
        let rest = self.rest.as_ref()?;
        if self.mapped.len() == self.positions.len() {
            return None;
        }
        let code = pressed(&*self.device.borrow())
            .into_iter()
            .find(|code| !rest.contains(code) && !self.mapped.contains(code))?;
        self.mapped.push(code);
        Some(Outcome::Step(self.step()))
    }
}
