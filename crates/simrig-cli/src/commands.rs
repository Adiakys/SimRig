//! Rig composition, drivers, presets, catalog, system and monitoring commands.

use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Subcommand;
use simrig_host::{ApplyOutcome, Host, HostEvent, Image, PluginLocation};

#[derive(Subcommand)]
pub enum DriverCommand {
    /// Driver status of every plugin, or of one.
    Status { plugin: Option<String> },
    /// Install a plugin's driver (asks for authorization for system plugins).
    Install { plugin: String },
    /// Remove a plugin's driver (asks for authorization for system plugins).
    Uninstall { plugin: String },
    /// Drivers installed by simrig.
    List,
}

#[derive(Subcommand)]
pub enum PresetCommand {
    List,
    Show {
        name: String,
    },
    /// Save the current saveable values of the rig as a preset.
    Capture {
        name: String,
    },
    /// Apply a preset and save its values into the drivers.
    Apply {
        name: String,
    },
    Delete {
        name: String,
    },
    Export {
        name: String,
        file: PathBuf,
    },
    Import {
        file: PathBuf,
        /// Replace a preset with the same name.
        #[arg(long)]
        replace: bool,
    },
}

pub fn devices(host: &Host) {
    let devices = host.usb_devices();
    if devices.is_empty() {
        println!("no USB devices");
    }
    for (device, plugin) in devices {
        let keys = device.keys();
        println!(
            "{:<22} {:<40} {}",
            keys.first().map(String::as_str).unwrap_or_default(),
            device.name,
            plugin
                .map(|p| format!("plugin {p}"))
                .unwrap_or_else(|| "-".into())
        );
    }
}

pub fn catalog(host: &Host) {
    let planned = &host.rig_config().planned;
    for m in host.catalog().models() {
        let mark = if planned.contains(&m.entry.id) {
            " (planned)"
        } else {
            ""
        };
        println!(
            "{:<24} {:<15} {} {}{mark}  [{}]",
            m.entry.id,
            m.entry.kind.label(),
            m.entry.vendor,
            m.entry.name,
            m.plugin
        );
    }
}

pub fn image(host: &Host, component: &str) -> Result<()> {
    match host.image(component)? {
        Image::User(path) => println!("user picture {}", path.display()),
        Image::Model(path) => println!("model image {}", path.display()),
        Image::Generic(kind) => {
            println!("generic illustration of a {}", kind.label().to_lowercase())
        }
    }
    Ok(())
}

pub fn driver(host: &mut Host, command: DriverCommand) -> Result<ExitCode> {
    match command {
        DriverCommand::Status { plugin } => {
            let ids: Vec<String> = match plugin {
                Some(p) => vec![p],
                None => host
                    .plugins()
                    .iter()
                    .map(|p| p.manifest.id.clone())
                    .collect(),
            };
            for id in ids {
                match host.driver_status(&id) {
                    Ok(None) => println!("{id:<12} no driver needed"),
                    Ok(Some(s)) => {
                        let state = if s.provided_by_system {
                            "provided by the system"
                        } else if s.installed {
                            "installed"
                        } else {
                            "not installed"
                        };
                        let access = match s.user_access {
                            Some(true) => ", user access ok",
                            Some(false) => ", NO user access",
                            None => "",
                        };
                        let reboot = if s.needs_reboot {
                            ", reboot needed"
                        } else {
                            ""
                        };
                        let message = s.message.map(|m| format!(" — {m}")).unwrap_or_default();
                        println!("{id:<12} {state}{access}{reboot}{message}");
                    }
                    Err(e) => println!("{id:<12} error: {e}"),
                }
            }
        }
        DriverCommand::Install { plugin } => print!("{}", host.install_driver(&plugin)?),
        DriverCommand::Uninstall { plugin } => print!("{}", host.uninstall_driver(&plugin)?),
        DriverCommand::List => {
            let installed = host.installed_drivers();
            if installed.is_empty() {
                println!("no drivers installed by simrig");
            }
            for (location, d) in installed {
                let place = if location == PluginLocation::System {
                    "system"
                } else {
                    "development"
                };
                println!(
                    "{:<12} {} {} {:?} ({place})",
                    d.plugin, d.driver, d.version, d.state
                );
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn preset(host: &mut Host, command: PresetCommand) -> Result<()> {
    match command {
        PresetCommand::List => {
            let presets = host.presets().list()?;
            if presets.is_empty() {
                println!("no presets");
            }
            for p in presets {
                println!("{}", p.name);
            }
        }
        PresetCommand::Show { name } => print!("{}", host.presets().get(&name)?.to_toml()),
        PresetCommand::Capture { name } => {
            let preset = host.capture_preset(&name)?;
            let path = host.presets().save(&preset)?;
            println!("saved {}", path.display());
        }
        PresetCommand::Apply { name } => {
            let preset = host.presets().get(&name)?;
            for e in host.apply_preset(&preset) {
                let outcome = match e.outcome {
                    ApplyOutcome::Saved => "saved".to_string(),
                    ApplyOutcome::Applied { note: Some(note) } => format!("applied ({note})"),
                    ApplyOutcome::Applied { note: None } => "applied".to_string(),
                    ApplyOutcome::Ignored(why) => format!("ignored: {why}"),
                };
                println!("{:<26} {:<24} {outcome}", e.target, e.item);
            }
        }
        PresetCommand::Delete { name } => {
            host.presets().delete(&name)?;
            println!("deleted {name}");
        }
        PresetCommand::Export { name, file } => {
            host.presets().export(&name, &file)?;
            println!("exported to {}", file.display());
        }
        PresetCommand::Import { file, replace } => {
            let preset = host.presets().import(&file, replace)?;
            println!("imported {}", preset.name);
        }
    }
    Ok(())
}

pub fn system(host: &Host) {
    let s = host.system();
    let or_unknown = |v: Option<String>| v.unwrap_or_else(|| "unknown".into());
    println!(
        "distribution  {} ({})",
        or_unknown(s.distro),
        s.distro_ids.join(" ")
    );
    println!("kernel        {}", or_unknown(s.kernel));
    let secure_boot = match s.secure_boot {
        Some(true) => {
            "enabled: drivers must be signed (DKMS does it, the key must be enrolled once)"
        }
        Some(false) => "disabled",
        None => "unknown (no EFI or not readable)",
    };
    println!("secure boot   {secure_boot}");
    println!(
        "session       {} on {}",
        or_unknown(s.session),
        or_unknown(s.desktop)
    );
    println!("systemd       {}", if s.systemd { "yes" } else { "no" });
    println!(
        "pkexec        {}",
        if s.pkexec {
            "yes"
        } else {
            "no: drivers cannot be installed from the app"
        }
    );
}

pub fn monitor(host: &mut Host, seconds: Option<f64>) -> Result<()> {
    let end = seconds.map(|s| Instant::now() + Duration::from_secs_f64(s));
    println!("watching the rig, Ctrl+C to stop");
    while end.is_none_or(|end| Instant::now() < end) {
        for event in host.poll() {
            match event {
                HostEvent::Inputs { .. } => {}
                HostEvent::PluginStarted { plugin } => println!("plugin {plugin} started"),
                HostEvent::PluginStopped { plugin } => {
                    println!("plugin {plugin} stopped (no devices)")
                }
                HostEvent::PluginFailed { plugin, error } => {
                    println!("plugin {plugin} failed: {error}")
                }
                HostEvent::DeviceAdded { plugin, device } => {
                    println!("+ {} ({plugin}/{})", device.name, device.id)
                }
                HostEvent::DeviceRemoved { plugin, device } => println!("- {plugin}/{device}"),
                HostEvent::ComponentsChanged { plugin, device } => {
                    println!("~ components of {plugin}/{device} changed")
                }
                HostEvent::DescriptionChanged { address } => println!("~ {address} updated"),
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
