//! Preferences of the app itself (`gui.toml` next to `rig.toml`). Everything about the devices
//! lives in the drivers; these are only how the app looks and what it remembers between runs.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn index(self) -> i32 {
        match self {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => ThemeMode::Light,
            2 => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GuiSettings {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub reduced_motion: bool,
    /// The first-run flow was completed or skipped.
    #[serde(default)]
    pub setup_done: bool,
    /// The preset applied last, shown in the rig header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

impl GuiSettings {
    fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("gui.toml")
    }

    /// Missing or unreadable settings fall back to the defaults: they are only preferences.
    pub fn load(config_dir: &Path) -> Self {
        fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(config_dir)?;
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(Self::path(config_dir), text)
    }
}
