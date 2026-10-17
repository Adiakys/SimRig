use simrig_plugin_sdk::prelude::*;

use crate::sim::{AxisShape, RAW_MAX, Shared, as_f64, raw_from_unit};

const ONLY_BRAKE: &str = "Only the brake pedal has a load cell.";

/// Simulated 3-pedal set. The brake has a load cell; vibration is not implemented.
pub struct SimPedals {
    shared: Shared,
    id: String,
    name: String,
    model: String,
    shapes: [AxisShape; 3],
    brake_max_force: Kilograms,
}

/// Raw position of a simulated pedal: a wave, or what a guided calibration asks for.
pub fn raw(shared: &Shared, pedal: PedalRole) -> i64 {
    let mut s = shared.get();
    let unit = match s.pedal_override {
        Some((i, position)) if i == index(pedal) => position,
        _ => s.unit_wave(index(pedal) as f64 / 3.0),
    } + s.jitter() * 0.05;
    raw_from_unit(unit)
}

pub fn index(pedal: PedalRole) -> usize {
    match pedal {
        PedalRole::Throttle => 0,
        PedalRole::Brake => 1,
        PedalRole::Clutch => 2,
    }
}

impl SimPedals {
    pub fn new(shared: Shared, id: &str, name: &str, model: &str) -> Self {
        Self {
            shared,
            id: id.into(),
            name: name.into(),
            model: model.into(),
            shapes: Default::default(),
            brake_max_force: 80.0,
        }
    }

    fn raw(&self, pedal: PedalRole) -> i64 {
        raw(&self.shared, pedal)
    }
}

impl Component for SimPedals {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn procedures(&mut self) -> Vec<ProcedureDescriptor> {
        PedalRole::ALL
            .iter()
            .map(|p| {
                ProcedureDescriptor::new(&format!("calibrate-{}", p.key()), &format!("Calibrate the {} (guided)", p.key()))
                    .help("Release and press the pedal when asked; the result is applied as its calibration.")
            })
            .collect()
    }

    fn start_procedure(&mut self, id: &str) -> Result<Box<dyn Procedure>> {
        let pedal = id
            .strip_prefix("calibrate-")
            .and_then(PedalRole::from_key)
            .ok_or_else(|| Error::NotFound(format!("unknown procedure '{id}'")))?;
        Ok(Box::new(crate::procedures::PedalCalibration::new(
            self.shared.clone(),
            pedal,
        )))
    }

    fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
        self.shared.save(&self.id(), item, value, host)
    }
}

impl Pedals for SimPedals {
    fn pedals(&self) -> Vec<PedalRole> {
        PedalRole::ALL.to_vec()
    }

    fn position(&mut self, pedal: PedalRole) -> Support<Axis> {
        let raw = self.raw(pedal);
        Support::Supported(self.shapes[index(pedal)].apply(raw))
    }

    fn force(&mut self, pedal: PedalRole) -> Support<Kilograms> {
        if pedal != PedalRole::Brake {
            return Support::not_in_hardware(ONLY_BRAKE);
        }
        let raw = self.raw(pedal);
        Support::Supported(self.shapes[1].apply(raw).value * self.brake_max_force)
    }

    fn param(&mut self, pedal: PedalRole, param: PedalParam) -> Support<ParamSpec> {
        let shape = &self.shapes[index(pedal)];
        Support::Supported(match param {
            PedalParam::CalibrationMin => ParamSpec::int(shape.cal_min, 0, RAW_MAX, 1).persistent(),
            PedalParam::CalibrationMax => ParamSpec::int(shape.cal_max, 0, RAW_MAX, 1).persistent(),
            PedalParam::DeadzoneLow => {
                ParamSpec::float(shape.deadzone_low, 0.0, 50.0, 0.5).persistent()
            }
            PedalParam::DeadzoneHigh => {
                ParamSpec::float(shape.deadzone_high, 0.0, 50.0, 0.5).persistent()
            }
            PedalParam::Curve => ParamSpec::curve(shape.curve.clone(), 8).persistent(),
            PedalParam::Inverted => ParamSpec::bool(shape.inverted).persistent(),
            PedalParam::MaxForce if pedal == PedalRole::Brake => {
                ParamSpec::float(self.brake_max_force, 10.0, 140.0, 1.0).persistent()
            }
            PedalParam::MaxForce => return Support::not_in_hardware(ONLY_BRAKE),
            PedalParam::Vibration => return Support::not_in_driver(),
        })
    }

    fn set_param(&mut self, pedal: PedalRole, param: PedalParam, value: &Value) -> Result<()> {
        let shape = &mut self.shapes[index(pedal)];
        match (param, value) {
            (PedalParam::CalibrationMin, Value::Int(v)) => {
                shape.check_calibration(*v, shape.cal_max)?;
                shape.cal_min = *v;
            }
            (PedalParam::CalibrationMax, Value::Int(v)) => {
                shape.check_calibration(shape.cal_min, *v)?;
                shape.cal_max = *v;
            }
            (PedalParam::DeadzoneLow, v) => shape.deadzone_low = as_f64(v),
            (PedalParam::DeadzoneHigh, v) => shape.deadzone_high = as_f64(v),
            (PedalParam::Curve, Value::Curve(points)) => shape.curve = points.clone(),
            (PedalParam::Inverted, Value::Bool(v)) => shape.inverted = *v,
            (PedalParam::MaxForce, v) if pedal == PedalRole::Brake => {
                self.brake_max_force = as_f64(v)
            }
            _ => return Err(Error::unsupported()),
        }
        Ok(())
    }

    fn action(&mut self, _pedal: PedalRole, action: PedalAction) -> Support<ActionSpec> {
        match action {
            PedalAction::Calibrate => Support::Supported(ActionSpec::default()),
            PedalAction::VibrationTest => Support::not_in_driver(),
        }
    }

    fn run_action(
        &mut self,
        pedal: PedalRole,
        action: PedalAction,
        _args: &Value,
    ) -> Result<ActionOutcome> {
        match action {
            PedalAction::Calibrate => {
                let shape = &mut self.shapes[index(pedal)];
                shape.cal_min = 0;
                shape.cal_max = RAW_MAX;
                Ok(ActionOutcome::message(format!(
                    "{} calibrated to the full travel.",
                    pedal.key()
                )))
            }
            PedalAction::VibrationTest => Err(Error::unsupported()),
        }
    }
}
