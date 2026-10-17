//! Rig composition, drivers, presets, catalog, system and monitoring commands.

use std::collections::BTreeMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Subcommand;
use simrig_host::{ApplyOutcome, Host, HostEvent, Image, PluginLocation};
use simrig_model::{Element, Level, ParamType, ProcedureState, Step, Value};

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
    println!("Attached devices");
    for (plugin, device) in host.devices() {
        let components = host
            .components()
            .iter()
            .filter(|c| c.plugin == plugin && c.device.id == device.id)
            .count();
        let note = if components == 0 {
            "  (no components: see `procedures`)"
        } else {
            ""
        };
        println!(
            "  {:<28} {}{note}",
            format!("{plugin}/{}", device.id),
            device.name
        );
    }
    println!("USB devices");
    let devices = host.usb_devices();
    if devices.is_empty() {
        println!("  none");
    }
    for (device, plugin) in devices {
        let keys = device.keys();
        println!(
            "  {:<28} {:<40} {}",
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
        Image::Generic { kind, rim } => {
            let shape = rim
                .map(|r| format!(", {} rim", r.key()))
                .unwrap_or_default();
            println!("core drawing of a {}{shape}", kind.label().to_lowercase())
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
                HostEvent::Inputs { .. } | HostEvent::UsbChanged => {}
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
                HostEvent::ProcedureUpdated { session, .. } => {
                    println!("~ procedure {session} moved on")
                }
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

pub fn procedures(host: &mut Host, target: &str) -> Result<()> {
    let target = host.target(target)?;
    let procedures = host.procedures(&target)?;
    if procedures.is_empty() {
        println!("no procedures");
    }
    for p in procedures {
        let availability = match &p.availability {
            simrig_model::Support::Supported(()) => String::new(),
            simrig_model::Support::Unsupported(u) => format!("  ✗ unavailable — {}", u.explain()),
        };
        println!("{:<22} {}{availability}", p.id, p.label);
        if let Some(help) = p.help {
            println!("{:<22} {help}", "");
        }
    }
    Ok(())
}

fn render(step: &Step) {
    let progress = step
        .progress
        .map(|[n, total]| format!(" ({n}/{total})"))
        .unwrap_or_default();
    println!("\n── {}{progress}", step.title);
    if let Some(text) = &step.text {
        println!("{text}");
    }
    for element in &step.elements {
        match element {
            Element::Field { .. } => {}
            Element::Value { label, value, unit } => {
                let unit = unit.as_deref().map(|u| format!(" {u}")).unwrap_or_default();
                println!("  {label}: {value}{unit}");
            }
            Element::Table { columns, rows } => {
                let widths: Vec<usize> = (0..columns.len())
                    .map(|i| {
                        rows.iter()
                            .map(|r| r.get(i).map_or(0, |c| c.chars().count()))
                            .chain([columns[i].chars().count()])
                            .max()
                            .unwrap_or(0)
                    })
                    .collect();
                let line = |cells: &[String]| {
                    let cells: Vec<String> = cells
                        .iter()
                        .zip(&widths)
                        .map(|(c, w)| format!("{c:<w$}"))
                        .collect();
                    println!("  {}", cells.join("  "));
                };
                line(columns);
                for row in rows {
                    line(row);
                }
            }
            Element::Notice { level, text } => {
                let mark = match level {
                    Level::Info => "ℹ",
                    Level::Success => "✓",
                    Level::Warning => "⚠",
                    Level::Error => "✗",
                };
                println!("  {mark} {text}");
            }
        }
    }
}

fn prompt(question: &str) -> Result<String> {
    print!("{question}");
    io::stdout().flush()?;
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer)? == 0 {
        anyhow::bail!("input closed");
    }
    Ok(answer.trim().to_string())
}

/// Asks the step's fields, then which action to take.
fn answer(step: &Step) -> Result<(String, BTreeMap<String, Value>)> {
    let mut values = step.defaults();
    for element in &step.elements {
        let Element::Field {
            id,
            label,
            ty,
            value,
            help,
        } = element
        else {
            continue;
        };
        if let Some(help) = help {
            println!("  {help}");
        }
        let choices = match ty {
            ParamType::Choice { options } => {
                format!(
                    " ({})",
                    options
                        .iter()
                        .map(|o| format!("{} = {}", o.id, o.label))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            _ => String::new(),
        };
        loop {
            let text = prompt(&format!("  {label}{choices} [{}]: ", value.to_text()))?;
            if text.is_empty() {
                break;
            }
            match ty.parse(&text) {
                Ok(v) => {
                    values.insert(id.clone(), v);
                    break;
                }
                Err(e) => println!("  {e}"),
            }
        }
    }
    for (i, action) in step.actions.iter().enumerate() {
        println!("  {}) {}", i + 1, action.label);
    }
    loop {
        let text = prompt("  choose [1]: ")?;
        let index = if text.is_empty() {
            Some(0)
        } else {
            text.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
        };
        if let Some(action) = index.and_then(|i| step.actions.get(i)) {
            if let Some(confirm) = &action.confirm {
                if !prompt(&format!("  {confirm} Type 'yes' to continue: "))?
                    .eq_ignore_ascii_case("yes")
                {
                    continue;
                }
            }
            return Ok((action.id.clone(), values));
        }
    }
}

/// Latest update of a session, waiting for the plugin to move on.
fn wait_update(host: &mut Host, session: &str) -> Result<ProcedureState> {
    loop {
        let mut latest = host.poll().into_iter().filter_map(|e| match e {
            HostEvent::ProcedureUpdated { session: s, state } if s == session => Some(state),
            _ => None,
        });
        if let Some(state) = latest.next_back() {
            return Ok(state);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Runs a plugin's procedure interactively, rendering its steps generically.
pub fn configure(host: &mut Host, target: &str, procedure: &str) -> Result<()> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("procedures are interactive: run this command in a terminal");
    }
    let target = host.target(target)?;
    let (session, mut state) = host.start_procedure(&target, procedure)?;
    loop {
        let step = match state {
            ProcedureState::Finished { message } => {
                println!("\n✓ {}", message.unwrap_or_else(|| "done".into()));
                return Ok(());
            }
            ProcedureState::Cancelled => {
                println!("\ncancelled");
                return Ok(());
            }
            ProcedureState::Step(step) => step,
        };
        render(&step);
        if step.waiting {
            println!("  (waiting, Ctrl+C to abort)");
            state = wait_update(host, &session)?;
            continue;
        }
        let (action, values) = answer(&step)?;
        state = match host.submit_procedure(&session, &action, values) {
            Ok(next) => {
                // Drop live updates of the step we just left.
                host.poll();
                next
            }
            Err(e) => {
                println!("  {e}");
                ProcedureState::Step(step)
            }
        };
    }
}
