use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::DriverError;
use crate::layout::StateLayout;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub drivers: Vec<InstalledDriver>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledDriver {
    pub plugin: String,
    pub driver: String,
    pub version: String,
    pub state: InstallState,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    /// Path of `driver.toml` inside the driver copy.
    pub contract: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    /// The install hook is running, or was interrupted.
    Installing,
    Installed,
    /// The install hook failed: `uninstall` must still clean up.
    Failed,
}

impl Registry {
    /// Loads the registry, dropping entries whose files were removed by hand or by the
    /// standalone `remove-drivers` script.
    pub fn load(layout: &StateLayout) -> Result<Self, DriverError> {
        let path = layout.registry();
        let mut registry: Registry = match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| DriverError::Invalid {
                path: path.clone(),
                message: e.to_string(),
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Registry::default(),
            Err(e) => return Err(e.into()),
        };
        registry
            .drivers
            .retain(|d| layout.plugin_dir(&d.plugin).is_dir());
        Ok(registry)
    }

    pub fn save(&self, layout: &StateLayout) -> Result<(), DriverError> {
        let path = layout.registry();
        if self.drivers.is_empty() {
            return remove_if_exists(&path);
        }
        fs::create_dir_all(&layout.root)?;
        let text = toml::to_string_pretty(self).map_err(|e| DriverError::Invalid {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn find(&self, plugin: &str) -> Option<&InstalledDriver> {
        self.drivers.iter().find(|d| d.plugin == plugin)
    }

    pub fn upsert(&mut self, entry: InstalledDriver) {
        self.drivers.retain(|d| d.plugin != entry.plugin);
        self.drivers.push(entry);
        self.drivers.sort_by(|a, b| a.plugin.cmp(&b.plugin));
    }

    pub fn remove(&mut self, plugin: &str) {
        self.drivers.retain(|d| d.plugin != plugin);
    }
}

pub(crate) fn remove_if_exists(path: &Path) -> Result<(), DriverError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
