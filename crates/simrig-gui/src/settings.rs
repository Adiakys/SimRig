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

/// How the app draws: how often devices are read and drawn, and whether anything moves on
/// its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsMode {
    /// No transitions, and the steering wheel drawing stays straight.
    ReducedMotion,
    #[default]
    Hz30,
    Hz60,
}

impl GraphicsMode {
    pub fn index(self) -> i32 {
        match self {
            GraphicsMode::ReducedMotion => 0,
            GraphicsMode::Hz30 => 1,
            GraphicsMode::Hz60 => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            0 => GraphicsMode::ReducedMotion,
            2 => GraphicsMode::Hz60,
            _ => GraphicsMode::Hz30,
        }
    }

    /// Live inputs read and drawn per second.
    pub fn live_rate(self) -> f64 {
        match self {
            GraphicsMode::Hz60 => 60.0,
            _ => 30.0,
        }
    }

    pub fn reduced_motion(self) -> bool {
        self == GraphicsMode::ReducedMotion
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GuiSettings {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub graphics: GraphicsMode,
    /// The switch that came before [`GraphicsMode`]: read once into it, never written.
    #[serde(default, skip_serializing)]
    reduced_motion: bool,
    /// The first-run flow was completed or skipped.
    #[serde(default)]
    pub setup_done: bool,
    /// The preset applied last, shown in the rig header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Offer pre-releases as updates. Unset: as the running version is (a release candidate
    /// is followed by the next one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereleases: Option<bool>,
}

impl GuiSettings {
    fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("gui.toml")
    }

    /// Missing or unreadable settings fall back to the defaults: they are only preferences.
    pub fn load(config_dir: &Path) -> Self {
        let mut settings: Self = fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        if std::mem::take(&mut settings.reduced_motion) {
            settings.graphics = GraphicsMode::ReducedMotion;
        }
        settings
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(config_dir)?;
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(Self::path(config_dir), text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_motion_becomes_the_graphics_mode() {
        let dir = std::env::temp_dir().join(format!("simrig-gui-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("gui.toml"), "reduced_motion = true\n").unwrap();
        let settings = GuiSettings::load(&dir);
        assert_eq!(settings.graphics, GraphicsMode::ReducedMotion);
        settings.save(&dir).unwrap();
        let text = fs::read_to_string(dir.join("gui.toml")).unwrap();
        assert!(text.contains("graphics = \"reduced_motion\""), "{text}");
        assert!(!text.contains("reduced_motion = "), "{text}");
        assert_eq!(
            GuiSettings::load(&dir).graphics,
            GraphicsMode::ReducedMotion
        );
        fs::remove_dir_all(dir).unwrap();

        assert_eq!(GuiSettings::default().graphics.live_rate(), 30.0);
        assert_eq!(GraphicsMode::Hz60.live_rate(), 60.0);
        for mode in [
            GraphicsMode::ReducedMotion,
            GraphicsMode::Hz30,
            GraphicsMode::Hz60,
        ] {
            assert_eq!(GraphicsMode::from_index(mode.index()), mode);
        }
    }
}
