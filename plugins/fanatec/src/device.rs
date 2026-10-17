//! One Fanatec device attached to the plugin: where the driver exposes it and how it is read.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use simrig_plugin_sdk::prelude::*;

use crate::input::Inputs;
use crate::models::Model;
use crate::sysfs::{self, Paths};

/// Which axis is which pedal, learned by the "Map the pedals" procedure.
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

/// The mappings of every model, in the user's configuration (`fanatec.toml`).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    models: BTreeMap<String, PedalMap>,
}

fn model_key(pid: u16) -> String {
    format!("{pid:04x}")
}

pub fn load_map(config: &Path, pid: u16) -> PedalMap {
    fs::read_to_string(config)
        .ok()
        .and_then(|t| toml::from_str::<Config>(&t).ok())
        .and_then(|c| c.models.get(&model_key(pid)).cloned())
        .unwrap_or_default()
}

pub fn save_map(config: &Path, pid: u16, map: &PedalMap) -> std::io::Result<()> {
    let mut all: Config = fs::read_to_string(config)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    all.models.insert(model_key(pid), map.clone());
    if let Some(dir) = config.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(
        config,
        toml::to_string_pretty(&all).map_err(std::io::Error::other)?,
    )
}

pub struct FanatecDevice {
    pub id: String,
    pub model: &'static Model,
    pub usb: PathBuf,
    pub paths: Option<Paths>,
    pub inputs: Inputs,
    pub config: PathBuf,
    pub pedal_map: PedalMap,
}

pub type Shared = Rc<RefCell<FanatecDevice>>;

impl FanatecDevice {
    pub fn new(id: &str, model: &'static Model, usb: PathBuf, config: PathBuf) -> Self {
        let pedal_map = load_map(&config, model.pid);
        let mut device = Self {
            id: id.into(),
            model,
            usb,
            paths: None,
            inputs: Inputs::None(String::new()),
            config,
            pedal_map,
        };
        device.refresh();
        device
    }

    /// Looks for the driver again (installed or bound later); true when that changed.
    pub fn refresh(&mut self) -> bool {
        let paths = Paths::find(&self.usb);
        if paths == self.paths {
            return false;
        }
        self.inputs = Inputs::open(paths.as_ref().and_then(Paths::event));
        self.paths = paths;
        true
    }

    /// Why nothing works, when the driver does not handle the device.
    pub fn unbound(&self) -> Option<Unsupported> {
        self.paths.is_none().then(|| {
            Unsupported::new(Reason::NeedsDriver)
                .with_detail("The hid-fanatec driver is not installed, or not bound to the device.")
        })
    }

    pub fn attr(&self, name: &str) -> Option<PathBuf> {
        let path = self.paths.as_ref()?.attr(name);
        path.exists().then_some(path)
    }

    pub fn tuning(&self, name: &str) -> Option<PathBuf> {
        let path = self.paths.as_ref()?.tuning()?.join(name);
        path.exists().then_some(path)
    }

    /// The mounted rim's id; `None` with no rim (or no driver).
    pub fn wheel_id(&self) -> Option<u16> {
        let id = sysfs::read_int(&self.attr("wheel_id")?)?;
        (id != 0).then_some(id as u16)
    }

    /// The rotation the driver applies; its "automatic" value is the base's maximum.
    pub fn range(&self) -> Option<f64> {
        let range = sysfs::read_int(&self.attr("range")?)? as f64;
        let max = self.max_range();
        Some(range.min(max))
    }

    pub fn max_range(&self) -> f64 {
        match self.model.kind {
            crate::models::Kind::Base { max_range } => f64::from(max_range),
            crate::models::Kind::Pedals => 0.0,
        }
    }
}
