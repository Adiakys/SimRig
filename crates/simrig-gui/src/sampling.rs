//! How often the plugins read each component's live inputs. A component with buttons, hats or a
//! gear is read at the live rate all the time: a quick press would fall between two slower
//! readings and be lost. A component with axes only (a base's angle, pedals, a handbrake) is read
//! slowly while it stays still, and at the live rate as soon as it moves.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use simrig_model::Value;

/// Readings per second of a component with axes only, while it stays still.
pub const STILL_HZ: f64 = 5.0;
/// How long a component with axes only stays at the live rate after it last moved.
const STILL_AFTER: Duration = Duration::from_secs(1);

struct Component {
    /// Read at the live rate (else at [`STILL_HZ`]).
    fast: bool,
    /// It has inputs a slower reading would miss; known from its first state.
    switches: Option<bool>,
    values: BTreeMap<String, Value>,
    moved: Instant,
}

pub struct Sampling {
    /// Readings per second of the components that need them: the app's live rate.
    live: f64,
    components: HashMap<String, Component>,
}

impl Sampling {
    pub fn new(live: f64) -> Self {
        Self {
            live,
            components: HashMap::new(),
        }
    }

    pub fn live(&self) -> f64 {
        self.live
    }

    /// Forgets the components that went away and returns the ones not followed yet, to subscribe
    /// at the live rate.
    pub fn missing(&mut self, addresses: &[String]) -> Vec<String> {
        self.components.retain(|a, _| addresses.contains(a));
        addresses
            .iter()
            .filter(|a| !self.components.contains_key(*a))
            .cloned()
            .collect()
    }

    /// A component subscribed at the live rate.
    pub fn add(&mut self, address: &str, now: Instant) {
        self.components.insert(
            address.to_string(),
            Component {
                fast: true,
                switches: None,
                values: BTreeMap::new(),
                moved: now,
            },
        );
    }

    /// A component sent its inputs: the rate to subscribe it at, when it must change.
    pub fn inputs(
        &mut self,
        address: &str,
        values: &BTreeMap<String, Value>,
        now: Instant,
    ) -> Option<f64> {
        let c = self.components.get_mut(address)?;
        let first = c.switches.is_none();
        if first {
            c.switches = Some(values.values().any(is_switch));
        }
        if c.values == *values {
            return None;
        }
        c.values = values.clone();
        if first {
            return None;
        }
        c.moved = now;
        if c.fast {
            return None;
        }
        c.fast = true;
        Some(self.live)
    }

    /// The components with axes only that stayed still long enough, to subscribe at
    /// [`STILL_HZ`].
    pub fn still(&mut self, now: Instant) -> Vec<String> {
        self.components
            .iter_mut()
            .filter(|(_, c)| {
                c.fast && c.switches == Some(false) && now.duration_since(c.moved) >= STILL_AFTER
            })
            .map(|(address, c)| {
                c.fast = false;
                address.clone()
            })
            .collect()
    }

    /// The live rate changed: the components read at it, to subscribe at the new one.
    pub fn set_live(&mut self, live: f64) -> Vec<String> {
        self.live = live;
        self.components
            .iter()
            .filter(|(_, c)| c.fast)
            .map(|(address, _)| address.clone())
            .collect()
    }
}

/// An input that changes in a moment and back: a button, a hat, a gear, an encoder's steps.
fn is_switch(value: &Value) -> bool {
    matches!(
        value,
        Value::Bool(_) | Value::Int(_) | Value::Gear(_) | Value::Buttons(_) | Value::Ints(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(items: &[(&str, Value)]) -> BTreeMap<String, Value> {
        items
            .iter()
            .map(|(id, v)| (id.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn axes_slow_down_when_still_and_speed_up_when_moved() {
        let t0 = Instant::now();
        let mut s = Sampling::new(30.0);
        let rig = ["pedals".to_string()];
        assert_eq!(s.missing(&rig), rig);
        s.add("pedals", t0);
        assert!(s.missing(&rig).is_empty());

        let rest = state(&[("brake.position", Value::Float(0.0))]);
        assert_eq!(s.inputs("pedals", &rest, t0), None);
        assert!(s.still(t0 + Duration::from_millis(500)).is_empty());
        assert_eq!(s.still(t0 + STILL_AFTER), rig);
        // Already slow, and the same state again is no move.
        assert!(s.still(t0 + STILL_AFTER * 2).is_empty());
        assert_eq!(s.inputs("pedals", &rest, t0 + STILL_AFTER * 2), None);

        let pressed = state(&[("brake.position", Value::Float(0.4))]);
        let t1 = t0 + STILL_AFTER * 3;
        assert_eq!(s.inputs("pedals", &pressed, t1), Some(30.0));
        assert_eq!(s.inputs("pedals", &rest, t1), None, "already fast");
        assert!(s.still(t1 + Duration::from_millis(500)).is_empty());
        assert_eq!(s.still(t1 + STILL_AFTER), rig);
    }

    #[test]
    fn buttons_stay_at_the_live_rate() {
        let t0 = Instant::now();
        let mut s = Sampling::new(30.0);
        s.add("wheel", t0);
        let buttons = state(&[("buttons", Value::Buttons(vec![false; 4]))]);
        assert_eq!(s.inputs("wheel", &buttons, t0), None);
        assert!(s.still(t0 + STILL_AFTER * 5).is_empty());
    }

    #[test]
    fn the_live_rate_follows_the_setting() {
        let t0 = Instant::now();
        let mut s = Sampling::new(30.0);
        s.add("wheel", t0);
        s.add("pedals", t0);
        s.inputs("pedals", &state(&[("x", Value::Float(0.0))]), t0);
        s.still(t0 + STILL_AFTER);
        assert_eq!(s.set_live(60.0), ["wheel"]);
        assert_eq!(s.live(), 60.0);
        let moved = state(&[("x", Value::Float(1.0))]);
        assert_eq!(s.inputs("pedals", &moved, t0 + STILL_AFTER), Some(60.0));
        // Gone from the rig: forgotten.
        assert_eq!(s.missing(&["wheel".to_string()]), Vec::<String>::new());
        assert_eq!(s.set_live(30.0), ["wheel"]);
    }
}
