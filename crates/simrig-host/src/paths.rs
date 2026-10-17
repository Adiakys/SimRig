//! XDG base directories.

use std::path::PathBuf;

fn xdg(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)))
        .map(|d| d.join("simrig"))
}

/// `~/.config/simrig`: rig composition and presets.
pub fn config_dir() -> Option<PathBuf> {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `~/.local/share/simrig`: user plugins and images.
pub fn data_dir() -> Option<PathBuf> {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// `~/.local/state/simrig`: drivers of development plugins.
pub fn state_dir() -> Option<PathBuf> {
    xdg("XDG_STATE_HOME", ".local/state")
}
