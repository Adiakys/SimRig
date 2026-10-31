//! Shared pieces for plugins of wheels and pedals that talk to a kernel driver like a game:
//!
//! - [`device`]: one device a kernel driver handles, with its inputs and pedal mapping;
//! - [`input`]: the event node, read on request (axes, buttons), and the force feedback test;
//! - [`sysfs`]: the HID device a driver is bound to, and its attributes;
//! - [`settings`]: settings that are sysfs attributes, as parameters;
//! - [`pedals`]: which axis is which pedal, learned by a guided procedure, and pedals as a
//!   standard component;
//! - [`shifter`]: which button is which position of a sequential or H-pattern shifter, known
//!   or learned by a guided procedure, and the shifter as a standard component.

pub mod device;
pub mod input;
pub mod pedals;
pub mod settings;
pub mod shifter;
pub mod sysfs;

use simrig_plugin_sdk::prelude::PedalRole;

pub fn role_label(role: PedalRole) -> &'static str {
    match role {
        PedalRole::Throttle => "Throttle",
        PedalRole::Brake => "Brake",
        PedalRole::Clutch => "Clutch",
    }
}

/// `--config <file>` from the adapter's arguments, else `$XDG_CONFIG_HOME/simrig/<name>.toml`.
pub fn config_path(name: &str) -> std::path::PathBuf {
    use std::path::PathBuf;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("simrig/{name}.toml"))
}
