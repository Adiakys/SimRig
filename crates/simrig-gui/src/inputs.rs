//! A steering wheel's controls, live: what was pressed last, what is held, and the Inputs view
//! of its screen (buttons, hat switches, encoders, analog paddles, as games number them).

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, VecModel};

use simrig_model::{Control, ControlKind, Value};

use crate::ui::{InputAxis, InputButton, InputEncoder, InputHat, InputsView};

/// How long a press shows on the home.
pub const SHOWN: Duration = Duration::from_millis(1500);
/// A button held longer than this is reported as probably stuck.
pub const STUCK: Duration = Duration::from_secs(30);

/// What a steering wheel's controls did, followed across live updates.
#[derive(Default)]
pub struct Presses {
    buttons: Vec<bool>,
    hats: Vec<i64>,
    encoders: Vec<i64>,
    /// Since when each button is held.
    held: Vec<Option<Instant>>,
    /// The control used last (its index in the component's controls), and when.
    pub last: Option<(usize, Instant)>,
}

impl Presses {
    /// Takes the latest live values of the component's inputs.
    pub fn update(&mut self, controls: &[Control], live: &BTreeMap<String, Value>, now: Instant) {
        if let Some(Value::Buttons(buttons)) = live.get("buttons") {
            self.held.resize(buttons.len(), None);
            for (i, &on) in buttons.iter().enumerate() {
                let was = self.buttons.get(i).copied().unwrap_or(false);
                if on && !was {
                    self.held[i] = Some(now);
                    self.used(controls, "buttons", i, now);
                } else if !on {
                    self.held[i] = None;
                }
            }
            self.buttons = buttons.clone();
        }
        if let Some(Value::Ints(hats)) = live.get("hats") {
            for (i, &d) in hats.iter().enumerate() {
                if d >= 0 && self.hats.get(i).copied().unwrap_or(-1) != d {
                    self.used(controls, "hats", i, now);
                }
            }
            self.hats = hats.clone();
        }
        if let Some(Value::Ints(encoders)) = live.get("encoders") {
            for (i, &steps) in encoders.iter().enumerate() {
                if self.encoders.get(i).is_some_and(|&was| was != steps) {
                    self.used(controls, "encoders", i, now);
                }
            }
            self.encoders = encoders.clone();
        }
    }

    fn used(&mut self, controls: &[Control], item: &str, index: usize, now: Instant) {
        if let Some(c) = controls
            .iter()
            .position(|c| c.item == item && c.index == index)
        {
            self.last = Some((c, now));
        }
    }

    /// The control used in the last moment, for the home.
    pub fn recent<'a>(&self, controls: &'a [Control], now: Instant) -> Option<&'a Control> {
        let (c, at) = self.last?;
        (now.duration_since(at) < SHOWN)
            .then(|| controls.get(c))
            .flatten()
    }

    /// Buttons held longer than [`STUCK`]: their index and for how long.
    fn stuck(&self, now: Instant) -> Vec<(usize, Duration)> {
        self.held
            .iter()
            .enumerate()
            .filter_map(|(i, since)| {
                let held = now.duration_since((*since)?);
                (held >= STUCK).then_some((i, held))
            })
            .collect()
    }
}

/// The face-button symbol drawn for a control (`cross`…), or empty.
pub fn symbol(c: &Control) -> &'static str {
    c.symbol.map(|s| s.key()).unwrap_or_default()
}

/// "Button 7", "Hat 1", "Axis 4", "Encoder 2".
pub fn title(c: &Control) -> String {
    let kind = match c.kind {
        ControlKind::Button => "Button",
        ControlKind::Axis => "Axis",
        ControlKind::Hat => "Hat",
        ControlKind::Encoder => "Encoder",
    };
    format!("{kind} {}", c.number)
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// The Inputs view of a steering wheel.
pub fn view(
    controls: &[Control],
    live: &BTreeMap<String, Value>,
    presses: &Presses,
    now: Instant,
) -> InputsView {
    let last = presses.last.map(|(c, _)| c);
    let stuck = presses.stuck(now);
    let of = |kind: ControlKind| {
        controls
            .iter()
            .enumerate()
            .filter(move |(_, c)| c.kind == kind)
    };

    let buttons: Vec<InputButton> = of(ControlKind::Button)
        .map(|(i, c)| {
            let pressed = matches!(live.get(&c.item), Some(Value::Buttons(b)) if b.get(c.index) == Some(&true));
            let held = stuck
                .iter()
                .find(|(b, _)| *b == c.index)
                .map(|(_, d)| format!("held {} s", d.as_secs()))
                .unwrap_or_default();
            InputButton {
                number: c.number as i32,
                label: c.label.clone().unwrap_or_default().into(),
                symbol: symbol(c).into(),
                pressed,
                last: last == Some(i) && !pressed,
                held: held.into(),
            }
        })
        .collect();
    let ints = |item: &str, index: usize| match live.get(item) {
        Some(Value::Ints(v)) => v.get(index).copied(),
        _ => None,
    };
    let hats: Vec<InputHat> = of(ControlKind::Hat)
        .map(|(_, c)| InputHat {
            number: c.number as i32,
            direction: ints(&c.item, c.index).unwrap_or(-1) as i32,
        })
        .collect();
    let encoders: Vec<InputEncoder> = of(ControlKind::Encoder)
        .map(|(_, c)| InputEncoder {
            label: c.label.clone().unwrap_or_else(|| title(c)).into(),
            steps: ints(&c.item, c.index).unwrap_or(0) as i32,
            buttons: match c.buttons.as_slice() {
                [a, b] => format!("buttons {a} and {b}"),
                _ => String::new(),
            }
            .into(),
        })
        .collect();
    let axes: Vec<InputAxis> = of(ControlKind::Axis)
        .map(|(_, c)| {
            // 0..1 over the travel, as the standard input carries it.
            let value = match live.get(&c.item) {
                Some(Value::Floats(v)) => v.get(c.index).copied().unwrap_or(0.0),
                _ => 0.0,
            };
            let code = match (&c.code, c.bits) {
                (Some(code), Some(bits)) => format!("{code} · {bits} bit"),
                (Some(code), None) => code.clone(),
                (None, _) => String::new(),
            };
            InputAxis {
                number: c.number as i32,
                label: c.label.clone().unwrap_or_else(|| title(c)).into(),
                code: code.into(),
                value: value.clamp(0.0, 1.0) as f32,
            }
        })
        .collect();

    let used = last.and_then(|c| controls.get(c));
    let stuck_text = stuck
        .first()
        .and_then(|(b, d)| {
            let c = controls
                .iter()
                .find(|c| c.kind == ControlKind::Button && c.index == *b)?;
            Some(format!(
                "Button {} has been pressed for {} s. A button held this long is often stuck, \
                 or a switch left on: games read it as pressed.",
                c.number,
                d.as_secs()
            ))
        })
        .unwrap_or_default();
    InputsView {
        buttons: model(buttons),
        hats: model(hats),
        encoders: model(encoders),
        axes: model(axes),
        last_what: used.map(title).unwrap_or_default().into(),
        last_label: used
            .and_then(|c| c.label.clone().filter(|l| *l != title(c)))
            .unwrap_or_default()
            .into(),
        last_code: used.and_then(|c| c.code.clone()).unwrap_or_default().into(),
        last_symbol: used.map(symbol).unwrap_or_default().into(),
        stuck: stuck_text.into(),
    }
}

/// Followed presses of every steering wheel, by component address.
pub type AllPresses = HashMap<String, Presses>;

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> Vec<Control> {
        vec![
            Control::new(ControlKind::Button, 1, "buttons", 0).label("Shift down"),
            Control::new(ControlKind::Button, 2, "buttons", 1),
            Control::new(ControlKind::Hat, 1, "hats", 0),
        ]
    }

    fn live(buttons: &[bool], hat: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("buttons".into(), Value::Buttons(buttons.to_vec())),
            ("hats".into(), Value::Ints(vec![hat])),
        ])
    }

    #[test]
    fn the_last_press_and_held_buttons() {
        let c = controls();
        let mut p = Presses::default();
        let t0 = Instant::now();
        p.update(&c, &live(&[false, false], -1), t0);
        assert!(p.last.is_none());
        p.update(&c, &live(&[true, false], -1), t0);
        assert_eq!(p.recent(&c, t0).map(title), Some("Button 1".into()));
        // Shown for a moment only.
        assert!(p.recent(&c, t0 + SHOWN).is_none());
        p.update(&c, &live(&[true, false], 2), t0 + Duration::from_secs(1));
        assert_eq!(
            p.recent(&c, t0 + Duration::from_secs(1)).map(title),
            Some("Hat 1".into())
        );

        // Button 1 is still held after half a minute: probably stuck.
        let later = t0 + STUCK + Duration::from_secs(1);
        let v = view(&c, &live(&[true, false], -1), &p, later);
        assert!(
            v.stuck.contains("Button 1 has been pressed for 31 s"),
            "{}",
            v.stuck
        );
        use slint::Model;
        let first = v.buttons.row_data(0).unwrap();
        assert!(first.pressed);
        assert_eq!(first.held, "held 31 s");
        assert_eq!(first.label, "Shift down");
        assert_eq!(v.last_what, "Hat 1");
    }
}

/// What the app keeps for this feature.
#[derive(Default)]
pub struct InputsState {
    pub presses: AllPresses,
    /// The open steering wheel shows its Inputs view.
    pub tab: bool,
    /// Hides presses on the home and counts held buttons when nothing else moves.
    ticker: Option<slint::Timer>,
}

impl crate::app::App {
    /// The component at `address`, with its controls, when it lists any (a steering wheel, a
    /// shifter with buttons of its own).
    fn controls_of(&self, address: &str) -> Option<Vec<Control>> {
        let d = &self.view(address)?.component.description;
        (!d.controls.is_empty()).then(|| d.controls.clone())
    }

    /// Follows the controls of the steering wheels that just sent live values.
    pub fn track_presses(&mut self, addresses: &[String]) {
        let now = Instant::now();
        for address in addresses {
            let (Some(controls), Some(live)) = (self.controls_of(address), self.live.get(address))
            else {
                continue;
            };
            self.inputs
                .presses
                .entry(address.clone())
                .or_default()
                .update(&controls, live, now);
        }
    }

    /// The control a steering wheel used a moment ago, for its card: title, name, symbol.
    pub fn recent_press(&self, address: &str) -> Option<(String, String, &'static str)> {
        let controls = self.controls_of(address)?;
        let c = self
            .inputs
            .presses
            .get(address)?
            .recent(&controls, Instant::now())?;
        let label = c.label.clone().filter(|l| *l != title(c));
        Some((title(c), label.unwrap_or_default(), symbol(c)))
    }

    /// Settings or Inputs on the open component's screen.
    pub fn show_inputs(&mut self, on: bool) {
        self.inputs.tab = on;
        self.window().global::<crate::ui::Rig>().set_inputs_tab(on);
        self.refresh_inputs();
    }

    /// The Inputs view of the open component, when it has one.
    pub fn refresh_inputs(&mut self) {
        let window = self.window();
        let rig = window.global::<crate::ui::Rig>();
        let address = self.current.clone().unwrap_or_default();
        let Some(controls) = self.controls_of(&address) else {
            rig.set_has_inputs(false);
            return;
        };
        rig.set_has_inputs(true);
        if !self.inputs.tab {
            return;
        }
        let empty = BTreeMap::new();
        let live = self.live.get(&address).unwrap_or(&empty);
        let presses = self.inputs.presses.entry(address.clone()).or_default();
        rig.set_inputs(view(&controls, live, presses, Instant::now()));
    }

    /// Every quarter of a second: presses on the home fade out, held buttons count up.
    pub fn start_inputs_ticker(&mut self) {
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(250),
            || {
                crate::app::with_app(|a| {
                    let now = Instant::now();
                    let fading: Vec<String> = a
                        .inputs
                        .presses
                        .iter()
                        .filter(|(_, p)| {
                            p.last.is_some_and(|(_, at)| {
                                now.duration_since(at) < SHOWN + Duration::from_millis(500)
                            })
                        })
                        .map(|(address, _)| address.clone())
                        .collect();
                    if !fading.is_empty() {
                        a.update_cards(|d| fading.iter().any(|f| f == d));
                    }
                    if a.inputs.tab {
                        a.refresh_inputs();
                    }
                })
            },
        );
        self.inputs.ticker = Some(timer);
    }
}
