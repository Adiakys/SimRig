//! Geometry of the core's generic drawings, from `assets/devices/devices.toml`: where the pedals
//! go, the gate of a shifter for its configuration and where the knob sits.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

const DEVICES_TOML: &str = include_str!("../../../assets/devices/devices.toml");

#[derive(Debug, Deserialize)]
pub struct Geometry {
    pub wheel_base: WheelBase,
    pub steering_wheel: SteeringWheel,
    pub pedals: Pedals,
    pub shifter: Shifter,
    pub handbrake: Handbrake,
}

#[derive(Debug, Deserialize)]
pub struct WheelBase {
    pub view_box: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct SteeringWheel {
    pub view_box: [f32; 2],
    pub pivot: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct Pedals {
    pub view_box: [f32; 2],
    pub pedal: Pedal,
    /// Hinge points by number of pedals, then by role.
    pub hinges: BTreeMap<String, BTreeMap<String, [f32; 2]>>,
}

#[derive(Debug, Deserialize)]
pub struct Pedal {
    pub size: [f32; 2],
    pub hinge: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct Shifter {
    pub view_box: [f32; 2],
    pub gate: Gate,
    pub knob: Knob,
}

#[derive(Debug, Deserialize)]
pub struct Gate {
    pub origin: [f32; 2],
    pub u: [f32; 2],
    pub v: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct Knob {
    pub size: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct Handbrake {
    pub view_box: [f32; 2],
    pub lever: Lever,
}

#[derive(Debug, Deserialize)]
pub struct Lever {
    pub pivot: [f32; 2],
    pub pulled_angle: f32,
}

impl Geometry {
    pub fn load() -> Self {
        toml::from_str(DEVICES_TOML).expect("assets/devices/devices.toml is valid")
    }

    /// Hinge point of each declared pedal (`clutch`, `brake`, `throttle`), for their count.
    pub fn pedal_hinges(&self, roles: &[&str]) -> BTreeMap<String, [f32; 2]> {
        let Some(points) = self.pedals.hinges.get(&roles.len().to_string()) else {
            return BTreeMap::new();
        };
        // Left to right as on the floor; a role without its own point takes the free slot.
        let ordered: Vec<&str> = ["clutch", "brake", "throttle"]
            .into_iter()
            .filter(|r| roles.contains(r))
            .collect();
        let mut slots: Vec<[f32; 2]> = points.values().copied().collect();
        slots.sort_by(|a, b| a[0].total_cmp(&b[0]));
        ordered
            .into_iter()
            .enumerate()
            .filter_map(|(i, role)| {
                let p = points.get(role).copied().or(slots.get(i).copied())?;
                Some((role.to_string(), p))
            })
            .collect()
    }

    fn map(&self, a: f32, b: f32) -> (f32, f32) {
        let g = &self.shifter.gate;
        (
            g.origin[0] + a * g.u[0] + b * g.v[0],
            g.origin[1] + a * g.u[1] + b * g.v[1],
        )
    }

    /// Where the knob sits for the engaged gear (the centre of the gate in neutral).
    pub fn knob(&self, layout: &GateLayout, engaged: &str) -> (f32, f32) {
        match layout.slots.iter().find(|s| s.label == engaged) {
            Some(slot) => self.map(slot.a, slot.b),
            None => self.map(0.5, 0.5),
        }
    }

    /// Where each gear label goes: above the top slots and below the bottom ones.
    pub fn labels(&self, layout: &GateLayout) -> Vec<(String, f32, f32)> {
        layout
            .slots
            .iter()
            .map(|s| {
                let (x, y) = self.map(s.a, if s.b > 0.5 { 1.065 } else { -0.105 });
                (s.label.clone(), x, y)
            })
            .collect()
    }

    /// SVG of the gate over the shifter plate (the labels are drawn by the interface, in its
    /// fonts).
    pub fn gate_svg(&self, layout: &GateLayout) -> String {
        let [w, h] = self.shifter.view_box;
        let p = |a: f32, b: f32| {
            let (x, y) = self.map(a, b);
            format!("{x:.1} {y:.1}")
        };
        let mut path = String::new();
        if layout.columns > 0 {
            let _ = write!(
                path,
                "M{}L{}",
                p(layout.column_a(0), 0.5),
                p(layout.column_a(layout.columns - 1), 0.5)
            );
        }
        for c in 0..layout.columns {
            let in_column = layout.slots.iter().filter(|s| s.column == c);
            let (mut top, mut bottom) = (0.5f32, 0.5f32);
            for s in in_column {
                if s.b > 0.5 {
                    top = s.b;
                } else {
                    bottom = s.b;
                }
            }
            let a = layout.column_a(c);
            let _ = write!(path, "M{}L{}", p(a, top), p(a, bottom));
        }
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}"><path d="{path}" fill="none" stroke="#141516" stroke-width="14" stroke-linecap="round" stroke-linejoin="round"/><path d="{path}" fill="none" stroke="#5d6268" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.8"/></svg>"##
        )
    }
}

/// A slot of the gate: its label and gate coordinates (`b` = 1 is up, away from the driver).
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub label: String,
    pub column: usize,
    pub a: f32,
    pub b: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GateLayout {
    pub columns: usize,
    pub slots: Vec<Slot>,
}

const UP: f32 = 0.92;
const DOWN: f32 = 0.08;

impl GateLayout {
    fn column_a(&self, column: usize) -> f32 {
        (column as f32 + 0.5) / self.columns as f32
    }

    /// The H pattern for `gears` forward gears and the reverse position (`left_up` or
    /// `right_down`), following the rules in devices.toml.
    pub fn h_pattern(gears: u8, reverse: &str) -> Self {
        let gears = gears.clamp(1, 12) as usize;
        let pairs = gears.div_ceil(2);
        let (columns, offset, reverse_slot) = if reverse == "left_up" {
            (pairs + 1, 1, (0, UP))
        } else if gears % 2 == 1 {
            (pairs, 0, (pairs - 1, DOWN))
        } else {
            (pairs + 1, 0, (pairs, DOWN))
        };
        let mut layout = Self {
            columns,
            slots: Vec::new(),
        };
        for g in 1..=gears {
            let column = (g - 1) / 2 + offset;
            layout.slots.push(Slot {
                label: g.to_string(),
                column,
                a: layout.column_a(column),
                b: if g % 2 == 1 { UP } else { DOWN },
            });
        }
        layout.slots.push(Slot {
            label: "R".into(),
            column: reverse_slot.0,
            a: layout.column_a(reverse_slot.0),
            b: reverse_slot.1,
        });
        layout
    }

    /// One column: + at the top, - at the bottom.
    pub fn sequential() -> Self {
        let slot = |label: &str, b| Slot {
            label: label.into(),
            column: 0,
            a: 0.5,
            b,
        };
        Self {
            columns: 1,
            slots: vec![slot("+", UP), slot("-", DOWN)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(layout: &GateLayout, column: usize) -> Vec<String> {
        layout
            .slots
            .iter()
            .filter(|s| s.column == column)
            .map(|s| s.label.clone())
            .collect()
    }

    #[test]
    fn six_plus_reverse_right() {
        let g = GateLayout::h_pattern(6, "right_down");
        assert_eq!(g.columns, 4);
        assert_eq!(labels(&g, 3), ["R"]);
    }

    #[test]
    fn seven_plus_reverse_takes_the_free_slot() {
        let g = GateLayout::h_pattern(7, "right_down");
        assert_eq!(g.columns, 4);
        assert_eq!(labels(&g, 3), ["7", "R"]);
    }

    #[test]
    fn reverse_left_up() {
        let g = GateLayout::h_pattern(6, "left_up");
        assert_eq!(g.columns, 4);
        assert_eq!(labels(&g, 0), ["R"]);
        assert_eq!(labels(&g, 1), ["1", "2"]);
    }

    #[test]
    fn geometry_parses_and_places_pedals() {
        let geo = Geometry::load();
        let three = geo.pedal_hinges(&["clutch", "brake", "throttle"]);
        assert!(three["clutch"][0] < three["brake"][0]);
        assert!(three["brake"][0] < three["throttle"][0]);
        let two = geo.pedal_hinges(&["brake", "throttle"]);
        assert_eq!(two.len(), 2);
        let svg = geo.gate_svg(&GateLayout::h_pattern(6, "right_down"));
        assert!(svg.starts_with("<svg"));
        let labels = geo.labels(&GateLayout::h_pattern(6, "right_down"));
        assert_eq!(labels.len(), 7);
        let knob = geo.knob(&GateLayout::h_pattern(6, "right_down"), "N");
        assert_eq!(knob, (56.0 + 104.0, 264.0 - 104.0));
    }
}
