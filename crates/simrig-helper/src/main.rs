//! `simrig-helper`: the only privileged part of simrig.
//!
//! The app runs it through `pkexec`, once per operation; it never stays running. It accepts a
//! closed set of actions on drivers declared by plugins **installed in system directories**
//! (root-owned), with arguments validated against each driver's `driver.toml`. It never takes
//! paths or commands from its caller.
//!
//! Run as a normal user (development), it needs explicit `--plugin-root` and `--state-dir` and
//! gains no privileges: hooks run as that user.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use simrig_driver::{Registry, StateLayout, effective_uid};
use simrig_host::manifest::{self, PluginLocation};

/// Exit code when the request itself is refused (as opposed to a failing hook).
const REFUSED: u8 = 2;

#[derive(Parser)]
#[command(name = "simrig-helper", version, about = "simrig privileged helper")]
struct Cli {
    /// Development only (not allowed as root): where to look for plugins.
    #[arg(long = "plugin-root")]
    plugin_roots: Vec<PathBuf>,
    /// Development only (not allowed as root): where to record installed drivers.
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Install the driver of a plugin.
    Install { plugin: String },
    /// Remove the driver of a plugin (works even if the plugin is gone).
    Uninstall { plugin: String },
    /// Run a privileged operation declared by an installed driver.
    Privileged {
        plugin: String,
        operation: String,
        /// Arguments as NAME=VALUE.
        args: Vec<String>,
    },
    /// List installed drivers.
    List,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("simrig-helper: {e:#}");
            ExitCode::from(REFUSED)
        }
    }
}

fn run(cli: Cli) -> Result<String> {
    let root = effective_uid() == Some(0);
    let (roots, layout) = if root {
        if !cli.plugin_roots.is_empty() || cli.state_dir.is_some() {
            bail!("--plugin-root and --state-dir are not allowed when running as root");
        }
        let roots = manifest::default_dirs()
            .into_iter()
            .filter(|(_, l)| *l == PluginLocation::System)
            .collect();
        (roots, StateLayout::system())
    } else {
        let Some(state) = cli.state_dir else {
            bail!("--state-dir is required when not running as root")
        };
        let roots: Vec<_> = cli
            .plugin_roots
            .into_iter()
            .map(|d| (d, PluginLocation::Explicit))
            .collect();
        (roots, StateLayout::new(state))
    };

    Ok(match cli.action {
        Action::Install { plugin } => {
            let (found, _) = manifest::discover(&roots);
            let f = found
                .iter()
                .find(|f| f.manifest.id == plugin)
                .with_context(|| {
                    format!("plugin '{plugin}' is not installed in a system plugin directory")
                })?;
            let driver = f
                .manifest
                .driver
                .as_deref()
                .with_context(|| format!("plugin '{plugin}' has no driver"))?;
            let contract = f.dir.join(driver).canonicalize()?;
            if !contract.starts_with(f.dir.canonicalize()?) {
                bail!("the driver of '{plugin}' is outside its plugin directory");
            }
            simrig_driver::install(&layout, &plugin, &contract)?
        }
        Action::Uninstall { plugin } => simrig_driver::uninstall(&layout, &plugin)?,
        Action::Privileged {
            plugin,
            operation,
            args,
        } => {
            let mut map = BTreeMap::new();
            for arg in args {
                let (name, value) = arg
                    .split_once('=')
                    .with_context(|| format!("'{arg}' is not NAME=VALUE"))?;
                map.insert(name.to_string(), value.to_string());
            }
            simrig_driver::privileged(&layout, &plugin, &operation, map)?
        }
        Action::List => {
            let registry = Registry::load(&layout)?;
            registry
                .drivers
                .iter()
                .map(|d| format!("{} {} {} {:?}\n", d.plugin, d.driver, d.version, d.state))
                .collect()
        }
    })
}
