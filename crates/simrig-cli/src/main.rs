//! `simrig-cli`: the simrig rig from the terminal. Same core as the GUI; it runs only while a
//! command runs and leaves nothing behind.

mod commands;
mod format;

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use simrig_host::{Host, HostOptions, PluginState};
use simrig_model::{CUSTOM_PREFIX, ItemSpec, ParamType, Support, Value};

#[derive(Parser)]
#[command(name = "simrig-cli", version, about = "Configure your sim-racing rig")]
struct Cli {
    /// Extra plugin directory (a plugin, or a directory of plugins). Repeatable.
    #[arg(long = "plugin-dir", global = true)]
    plugin_dirs: Vec<PathBuf>,

    /// Do not search the default user and system plugin directories.
    #[arg(long, global = true)]
    no_default_dirs: bool,

    /// Rig composition and presets (default: ~/.config/simrig).
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List installed plugins and their state.
    Plugins,
    /// List the components of the rig.
    List,
    /// Show the full interface of a component, unsupported items included.
    Show {
        /// Component address (`sim/rig0/base`) or a unique suffix (`base`).
        component: String,
    },
    /// Read a parameter.
    Get { component: String, item: String },
    /// Set a parameter (`custom.<id>` for custom parameters).
    Set {
        component: String,
        item: String,
        value: String,
        /// Also save it into the driver, so it is restored at plug-in.
        #[arg(long)]
        save: bool,
    },
    /// Run an action.
    Run {
        component: String,
        action: String,
        /// Confirm actions that require it (e.g. effects that move the wheel).
        #[arg(long)]
        yes: bool,
    },
    /// Write an output (LEDs, display).
    Output {
        component: String,
        item: String,
        value: String,
    },
    /// Show live inputs.
    Watch {
        component: String,
        /// Updates per second.
        #[arg(long, default_value_t = 10.0)]
        rate: f64,
        /// Stop after this many seconds (default: until Ctrl+C).
        #[arg(long)]
        seconds: Option<f64>,
    },
    /// Save the current value of a parameter into the driver, so it is restored at plug-in.
    Save { component: String, item: String },
    /// Exclude a component from the rig (e.g. pedal inputs of a base when other pedals are used).
    Exclude { component: String },
    /// Include an excluded component back.
    Include { component: String },
    /// Connected USB devices and the plugin handling each.
    Devices,
    /// Choose the plugin of a device (key from `devices`); without plugin, clear the choice.
    Choose {
        device: String,
        plugin: Option<String>,
    },
    /// Device models known to the installed plugins.
    Catalog,
    /// Add a catalog model to the rig before plugging it in (`--remove` to undo).
    Plan {
        model: String,
        #[arg(long)]
        remove: bool,
    },
    /// Where the picture of a component comes from.
    Image { component: String },
    /// Guided procedures a plugin offers on a component or a device (`<plugin>/<device>`).
    Procedures { target: String },
    /// Run a guided procedure interactively (calibration, device setup…).
    Configure { target: String, procedure: String },
    /// Drivers of plugins.
    #[command(subcommand)]
    Driver(commands::DriverCommand),
    /// Presets.
    #[command(subcommand)]
    Preset(commands::PresetCommand),
    /// What the setup wizard checks: distribution, kernel, Secure Boot, session.
    System,
    /// Print rig changes (devices plugged and unplugged, rim swaps…) as they happen.
    Monitor {
        /// Stop after this many seconds (default: until Ctrl+C).
        #[arg(long)]
        seconds: Option<f64>,
    },
    /// Report plugin problems (manifests, startup, conformance).
    Check,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        // `simrig-cli watch … | head`: a closed pipe is a normal way to stop.
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let mut options = HostOptions {
        plugin_dirs: cli.plugin_dirs,
        default_dirs: !cli.no_default_dirs,
        ..HostOptions::default()
    };
    if let Some(dir) = cli.config_dir {
        options.config_dir = dir;
    }
    let mut host = Host::start(&options);
    let problems = host.problems().len();
    if problems > 0 && !matches!(cli.command, Command::Check) {
        eprintln!("warning: {problems} plugin problem(s), see `simrig-cli check`");
    }

    match cli.command {
        Command::Plugins => print_plugins(&host),
        Command::List => print_list(&host),
        Command::Show { component } => {
            let address = host.find(&component)?.address.clone();
            host.describe(&address)?;
            let inputs = host.read_inputs(&address).unwrap_or_default();
            print!("{}", format::component(host.find(&address)?, &inputs));
        }
        Command::Get { component, item } => {
            let address = host.find(&component)?.address.clone();
            let c = host.describe(&address)?;
            let text = match item.strip_prefix(CUSTOM_PREFIX) {
                Some(id) => {
                    let p = c
                        .custom_param(id)
                        .with_context(|| format!("no custom parameter '{id}'"))?;
                    format!(
                        "{}{}",
                        p.value,
                        p.unit
                            .as_deref()
                            .map(|u| format!(" {u}"))
                            .unwrap_or_default()
                    )
                }
                None => {
                    let spec = host.param(&address, &item)?;
                    format::value_with_unit(&spec.value, format::unit(host.find(&address)?, &item))
                }
            };
            println!("{text}");
        }
        Command::Set {
            component,
            item,
            value,
            save,
        } => {
            let address = host.find(&component)?.address.clone();
            let ty = param_type(&mut host, &address, &item)?;
            let value = ty.parse(&value).map_err(|e| anyhow::anyhow!("{e}"))?;
            host.set(&address, &item, value)?;
            if save {
                host.save(&address, &item)?;
                println!("{item} set and saved");
            } else {
                println!("{item} set");
            }
        }
        Command::Run {
            component,
            action,
            yes,
        } => {
            let address = host.find(&component)?.address.clone();
            let c = host.describe(&address)?;
            let item = c
                .item(&action)
                .with_context(|| format!("no action '{action}'"))?;
            match &item.support {
                Support::Supported(ItemSpec::Action(spec)) => {
                    if let Some(text) = &spec.confirm {
                        if !yes && !confirm(text)? {
                            bail!("cancelled");
                        }
                    }
                }
                Support::Supported(_) => bail!("'{action}' is not an action"),
                Support::Unsupported(u) => bail!("'{action}' is unavailable: {}", u.explain()),
            }
            let outcome = host.run_action(&address, &action, Value::Empty)?;
            println!("{}", outcome.message.unwrap_or_else(|| "done".into()));
        }
        Command::Output {
            component,
            item,
            value,
        } => {
            let address = host.find(&component)?.address.clone();
            let c = host.describe(&address)?;
            let ty = match c.item(&item).map(|i| &i.support) {
                Some(Support::Supported(ItemSpec::Output { ty })) => ty.clone(),
                Some(Support::Unsupported(u)) => bail!("'{item}' is unavailable: {}", u.explain()),
                _ => bail!("no output '{item}'"),
            };
            let value = ty.parse(&value).map_err(|e| anyhow::anyhow!("{e}"))?;
            host.write_output(&address, &item, value)?;
            println!("{item} written");
        }
        Command::Watch {
            component,
            rate,
            seconds,
        } => watch(&mut host, &component, rate, seconds)?,
        Command::Save { component, item } => {
            let address = host.find(&component)?.address.clone();
            host.save(&address, &item)?;
            println!("{item} saved: restored when the device is plugged in");
        }
        Command::Exclude { component } => {
            host.set_excluded(&component, true)?;
            println!("{component} excluded from the rig");
        }
        Command::Include { component } => {
            host.set_excluded(&component, false)?;
            println!("{component} included in the rig");
        }
        Command::Devices => commands::devices(&host),
        Command::Choose { device, plugin } => {
            host.choose_plugin(&device, plugin.as_deref())?;
            println!("saved: takes effect when the device is plugged in again");
        }
        Command::Catalog => commands::catalog(&host),
        Command::Plan { model, remove } => {
            host.set_planned(&model, !remove)?;
            println!(
                "{model} {}",
                if remove {
                    "removed from the rig"
                } else {
                    "added to the rig"
                }
            );
        }
        Command::Image { component } => commands::image(&host, &component)?,
        Command::Procedures { target } => commands::procedures(&mut host, &target)?,
        Command::Configure { target, procedure } => {
            commands::configure(&mut host, &target, &procedure)?
        }
        Command::Driver(command) => return commands::driver(&mut host, command),
        Command::Preset(command) => commands::preset(&mut host, command)?,
        Command::System => commands::system(&host),
        Command::Monitor { seconds } => commands::monitor(&mut host, seconds)?,
        Command::Check => {
            if host.problems().is_empty() {
                println!("no problems");
            } else {
                for p in host.problems() {
                    println!("{p}");
                }
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn print_plugins(host: &Host) {
    if host.plugins().is_empty() {
        println!("no plugins found");
    }
    for p in host.plugins() {
        let state = match &p.state {
            PluginState::Running => "running".to_string(),
            PluginState::Idle => "idle (no device)".to_string(),
            PluginState::Failed(e) => format!("failed: {e}"),
        };
        println!(
            "{:<12} {:<24} {:<8} {:<10?} {state}",
            p.manifest.id, p.manifest.name, p.manifest.version, p.location
        );
        println!("{:<12} {}", "", p.dir.display());
    }
}

fn print_list(host: &Host) {
    if host.components().is_empty() {
        println!("no components: connect a device or check `simrig-cli plugins`");
        return;
    }
    let mut device = None;
    for c in host.components() {
        if device != Some((&c.plugin, &c.device.id)) {
            device = Some((&c.plugin, &c.device.id));
            println!("{} ({}/{})", c.device.name, c.plugin, c.device.id);
        }
        let d = &c.description;
        let supported = d.items.iter().filter(|i| i.support.is_supported()).count();
        println!(
            "  {:<24} {:<15} {:<34} {supported}/{} standard, {} custom{}",
            c.address,
            d.kind.label(),
            d.name,
            d.items.len(),
            d.custom.len(),
            if c.excluded { "  (excluded)" } else { "" }
        );
    }
}

fn param_type(host: &mut Host, address: &str, item: &str) -> Result<ParamType> {
    let c = host.describe(address)?;
    if let Some(id) = item.strip_prefix(CUSTOM_PREFIX) {
        let p = c
            .custom_param(id)
            .with_context(|| format!("no custom parameter '{id}'"))?;
        if let Support::Unsupported(u) = &p.availability {
            bail!("'{item}' is unavailable: {}", u.explain());
        }
        return Ok(p.ty.clone());
    }
    match c.item(item).map(|i| &i.support) {
        Some(Support::Supported(ItemSpec::Param(spec))) => Ok(spec.ty.clone()),
        Some(Support::Unsupported(u)) => bail!("'{item}' is unavailable: {}", u.explain()),
        Some(_) => bail!("'{item}' is not a parameter"),
        None => bail!("no parameter '{item}'"),
    }
}

fn confirm(text: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("{text} (pass --yes to confirm)");
    }
    print!("{text} Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("yes"))
}

fn watch(host: &mut Host, component: &str, rate: f64, seconds: Option<f64>) -> Result<()> {
    let address = host.find(component)?.address.clone();
    host.subscribe(&address, rate)?;
    let end = seconds.map(|s| Instant::now() + Duration::from_secs_f64(s));
    let tty = io::stdout().is_terminal();
    let mut out = io::stdout().lock();
    while end.is_none_or(|end| Instant::now() < end) {
        let Some(values) = host.next_input_state(&address, Duration::from_secs(2))? else {
            bail!("no input state received");
        };
        if tty {
            write!(out, "\x1b[H\x1b[2J")?;
        }
        let c = host.find(&address)?;
        writeln!(out, "{} — {}", c.address, c.description.name)?;
        for (id, value) in &values {
            writeln!(
                out,
                "  {id:<24} {}",
                format::value_with_unit(value, format::unit(c, id))
            )?;
        }
        if !tty {
            writeln!(out)?;
        }
        out.flush()?;
    }
    host.unsubscribe(&address)?;
    Ok(())
}
