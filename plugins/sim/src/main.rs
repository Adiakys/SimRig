//! A complete simulated rig: wheel base, steering wheel, pedals, shifter and handbrake.
//!
//! Inputs move by themselves (waves with optional noise), parameters are kept in memory, and
//! a few items are deliberately unsupported with each kind of reason, so the app and the
//! wizard can be developed and tested without hardware.

mod base;
mod handbrake;
mod pedals;
mod shifter;
mod sim;
mod wheel;

use std::process::ExitCode;

use simrig_plugin_sdk::model::Bus;
use simrig_plugin_sdk::prelude::*;

use crate::sim::Shared;

pub const DEVICE_ID: &str = "rig0";

#[derive(Default)]
struct SimAdapter {
    shared: Shared,
}

impl Adapter for SimAdapter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "sim".into(),
            name: "Simulated rig".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    fn discover(&mut self) -> Vec<DeviceInfo> {
        vec![DeviceInfo {
            id: DEVICE_ID.into(),
            name: "Simulated rig".into(),
            bus: Bus::Virtual,
        }]
    }

    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>> {
        if device.id != DEVICE_ID {
            return Err(Error::NotFound(format!("unknown device '{}'", device.id)));
        }
        let s = &self.shared;
        Ok(vec![
            AnyComponent::WheelBase(Box::new(base::SimBase::new(s.clone()))),
            AnyComponent::SteeringWheel(Box::new(wheel::SimWheel::new(s.clone()))),
            AnyComponent::Pedals(Box::new(pedals::SimPedals::new(s.clone()))),
            AnyComponent::Shifter(Box::new(shifter::SimShifter::new(s.clone()))),
            AnyComponent::Handbrake(Box::new(handbrake::SimHandbrake::new(s.clone()))),
        ])
    }
}

fn main() -> ExitCode {
    simrig_plugin_sdk::run(SimAdapter::default())
}
