//! The user's rig composition (`rig.toml`): choices the hardware cannot tell us.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::HostError;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigConfig {
    /// Components excluded from the rig, by address (e.g. the pedal inputs of a base when
    /// other pedals are used).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded: BTreeSet<String>,
    /// Device key (`usb:vvvv:pppp[:serial]`) → plugin chosen by the user for it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub device_plugins: BTreeMap<String, String>,
    /// Catalog models added to the rig before being plugged in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planned: Vec<String>,
    /// Problems games may have with a device that the user chose to ignore, as
    /// `<device id>/<finding key>` (see [`crate::device_checks::Finding::key`]).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub ignored: BTreeSet<String>,
}

impl RigConfig {
    /// Rewrites addresses of a device from the id it had before ids were stable (its port,
    /// `usb-1-4`) to its id now; true when anything changed.
    pub fn migrate_device(&mut self, legacy: &str, id: &str) -> bool {
        if legacy == id {
            return false;
        }
        let moved: Vec<String> = self
            .excluded
            .iter()
            .filter(|a| a.split('/').nth(1) == Some(legacy))
            .cloned()
            .collect();
        for address in &moved {
            self.excluded.remove(address);
            let mut parts: Vec<&str> = address.split('/').collect();
            parts[1] = id;
            self.excluded.insert(parts.join("/"));
        }
        !moved.is_empty()
    }

    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("rig.toml")
    }

    pub fn load(config_dir: &Path) -> Result<Self, HostError> {
        let path = Self::path(config_dir);
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| HostError::Invalid {
                path,
                message: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<(), HostError> {
        fs::create_dir_all(config_dir)?;
        let path = Self::path(config_dir);
        let text = toml::to_string_pretty(self).map_err(|e| HostError::Invalid {
            path: path.clone(),
            message: e.to_string(),
        })?;
        fs::write(path, text)?;
        Ok(())
    }
}
