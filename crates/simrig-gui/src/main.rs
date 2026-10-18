//! `simrig`: the desktop app. It is a client of the drivers, like a game: it starts without
//! privileges, talks to the plugins while it is open and leaves nothing running when closed.

mod app;
mod backend;
mod component;
mod devices;
mod flows;
mod format;
mod procedure;
mod raster;
mod settings;
mod shot;
mod update;

#[allow(unsafe_code, clippy::all)]
mod ui {
    slint::include_modules!();
}

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use simrig_host::HostOptions;
use slint::ComponentHandle;

use crate::app::App;
use crate::backend::Backend;
use crate::ui::{AppWindow, Screen};

#[derive(Parser)]
#[command(name = "simrig", version, about = "Configure your sim-racing rig")]
struct Cli {
    /// Extra plugin directory (a plugin, or a directory of plugins). Repeatable.
    #[arg(long = "plugin-dir")]
    plugin_dirs: Vec<PathBuf>,

    /// Do not search the default user and system plugin directories.
    #[arg(long)]
    no_default_dirs: bool,

    /// Rig composition, presets and app preferences (default: ~/.config/simrig).
    #[arg(long)]
    config_dir: Option<PathBuf>,

    /// USB devices in sysfs (development: a fake tree).
    #[arg(long, hide = true)]
    usb_root: Option<PathBuf>,

    /// Installed drivers of development plugins.
    #[arg(long, hide = true)]
    state_dir: Option<PathBuf>,

    /// Render the window into this PNG and exit, without a display.
    #[arg(long, hide = true)]
    screenshot: Option<PathBuf>,

    /// With --screenshot: the screen to show, e.g. `home`, `component:base`, `plugins`.
    #[arg(long, hide = true, default_value = "home")]
    screen: String,

    /// With --screenshot: `light` or `dark`.
    #[arg(long, hide = true)]
    theme: Option<String>,

    /// With --screenshot: window size.
    #[arg(long, hide = true, default_value = "1440x900")]
    size: String,

    /// With --screenshot: milliseconds to wait before opening the screen.
    #[arg(long, hide = true, default_value_t = 1500)]
    after: u64,

    /// With --screenshot: milliseconds between opening the screen and capturing it.
    #[arg(long, hide = true, default_value_t = 400)]
    hold: u64,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let mut options = HostOptions {
        plugin_dirs: cli.plugin_dirs.clone(),
        default_dirs: !cli.no_default_dirs,
        ..HostOptions::default()
    };
    if let Some(dir) = &cli.config_dir {
        options.config_dir = dir.clone();
    }
    if let Some(dir) = &cli.usb_root {
        options.usb_root = dir.clone();
    }
    if let Some(dir) = &cli.state_dir {
        options.dev_state_dir = dir.clone();
    }

    let offscreen = match &cli.screenshot {
        Some(_) => {
            let (w, h) = cli
                .size
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                .context("--size must look like 1440x900")?;
            Some(shot::install(w, h)?)
        }
        None => None,
    };

    // Where to start again after an update: once replaced, the running binary has no path.
    let exe = std::env::current_exe().ok();
    let window = AppWindow::new().context("cannot create the window")?;
    // Matches simrig.desktop, so docks and task bars show the app's icon and group its window
    // (Wayland app id, X11 window class). Nothing to match in screenshots.
    let _ = slint::set_xdg_app_id("simrig");
    let config_dir = options.config_dir.clone();
    let backend = Backend::start(options, app::deliver);
    // Slint's software renderer (no OpenGL, or screenshots) cannot rotate images.
    let raster =
        offscreen.is_some() || std::env::var("SLINT_BACKEND").is_ok_and(|b| b.contains("software"));
    let app = Rc::new(RefCell::new(App::new(&window, backend, config_dir, raster)));
    app::install(app.clone());
    callbacks::wire(&window, &app);

    if let Some(offscreen) = offscreen {
        if let Some(theme) = &cli.theme {
            window
                .global::<ui::Theme>()
                .set_mode(if theme == "dark" { 2 } else { 1 });
        }
        let path = cli.screenshot.clone().unwrap_or_default();
        let screen = cli.screen.clone();
        let target = app.clone();
        let hold = Duration::from_millis(cli.hold);
        let _ = &target;
        // Let the plugins start and the first inputs arrive, then open the screen and capture.
        // `--screen` may chain actions with `;`, one every `--hold` ms, e.g.
        // `add;generic:usb:1234:0001;submit:next`: the capture follows the last one.
        let steps: Vec<String> = screen.split(';').map(str::to_string).collect();
        let delays = (0..steps.len()).map(|i| Duration::from_millis(cli.after) + hold * i as u32);
        for (step, delay) in steps.iter().cloned().zip(delays) {
            let target = target.clone();
            slint::Timer::single_shot(delay, move || callbacks::open_screen(&target, &step));
        }
        let end = Duration::from_millis(cli.after) + hold * steps.len() as u32;
        let captured: Rc<RefCell<Option<Result<()>>>> = Rc::default();
        let result = captured.clone();
        slint::Timer::single_shot(end, move || {
            *result.borrow_mut() = Some(shot::capture(&offscreen, &path));
            let _ = slint::quit_event_loop();
        });
        window.show()?;
        slint::run_event_loop()?;
        captured
            .take()
            .unwrap_or_else(|| Err(anyhow::anyhow!("the screen was not captured")))?;
        window.hide()?;
    } else {
        app.borrow_mut().start_update_checks();
        window.run()?;
    }

    // Stop the host thread and every adapter before leaving.
    drop(app);
    if let Some(app) = app::uninstall() {
        drop(app);
    }
    update::restart(exe.as_deref());
    Ok(())
}

mod callbacks {
    use super::*;
    use crate::app::Shared;
    use crate::ui::Api;

    pub fn wire(window: &AppWindow, app: &Shared) {
        let api = window.global::<Api>();
        let a = app.clone();
        window.on_navigate(move |screen| a.borrow_mut().go(screen));
        let a = app.clone();
        api.on_go_home(move || a.borrow_mut().go(Screen::Home));
        let a = app.clone();
        api.on_apply_preset(move |name| a.borrow_mut().apply_preset(name.to_string()));
        let a = app.clone();
        api.on_save_preset(move |name| {
            a.borrow()
                .send(backend::Command::SavePreset(name.trim().to_string()))
        });
        let a = app.clone();
        api.on_dismiss_toast(move |i| a.borrow_mut().dismiss_toast(i as usize));
        let a = app.clone();
        api.on_open_component(move |address| a.borrow_mut().open_component(&address));
        let a = app.clone();
        api.on_set_number(move |item, v| a.borrow_mut().set_number(&item, v));
        let a = app.clone();
        api.on_set_choice(move |item, i| a.borrow_mut().set_choice(&item, i));
        let a = app.clone();
        api.on_set_bool(move |item, on| a.borrow_mut().set_bool(&item, on));
        let a = app.clone();
        api.on_set_text(move |item, text| a.borrow_mut().set_text(&item, &text));
        let a = app.clone();
        api.on_set_curve_point(move |item, i, y| {
            a.borrow_mut().set_curve_point(&item, i.max(0) as usize, y)
        });
        let a = app.clone();
        api.on_toggle_advanced(move |_| a.borrow_mut().toggle_advanced());
        let a = app.clone();
        api.on_run_action(move |item| a.borrow_mut().run_component_action(&item));
        let a = app.clone();
        api.on_save_all(move || a.borrow_mut().save_component());
        let a = app.clone();
        api.on_reset_values(move || a.borrow_mut().reset_component());
        let a = app.clone();
        api.on_start_procedure(move |id| {
            let mut a = a.borrow_mut();
            if let Some(address) = a.current.clone() {
                a.start_procedure(simrig_host::Target::Component(address), &id);
            }
        });
        let a = app.clone();
        api.on_submit_step(move |action| a.borrow_mut().submit_step(&action));
        let a = app.clone();
        api.on_step_number(move |i, v| a.borrow_mut().step_number(i.max(0) as usize, v));
        let a = app.clone();
        api.on_step_choice(move |i, c| {
            a.borrow_mut()
                .step_choice(i.max(0) as usize, c.max(0) as usize)
        });
        let a = app.clone();
        api.on_step_bool(move |i, on| a.borrow_mut().step_bool(i.max(0) as usize, on));
        let a = app.clone();
        api.on_step_text(move |i, t| a.borrow_mut().step_text(i.max(0) as usize, &t));
        let a = app.clone();
        api.on_cancel_procedure(move || a.borrow_mut().cancel_procedure());
        let a = app.clone();
        api.on_close_procedure(move || a.borrow_mut().procedure_closed());
        let a = app.clone();
        api.on_start_add(move || a.borrow_mut().start_add());
        let a = app.clone();
        api.on_add_choose(move |key| a.borrow_mut().add_choose(&key));
        let a = app.clone();
        api.on_add_generic(move |key| a.borrow_mut().add_generic(&key));
        let a = app.clone();
        api.on_add_show_all(move |on| a.borrow_mut().add_show_all(on));
        let a = app.clone();
        api.on_add_install_driver(move || a.borrow_mut().add_install_driver());
        let a = app.clone();
        api.on_add_finish(move || a.borrow_mut().add_finish(false));
        let a = app.clone();
        api.on_add_open_settings(move || a.borrow_mut().add_finish(true));
        let a = app.clone();
        api.on_install_driver(move |p| a.borrow_mut().install_driver(&p));
        let a = app.clone();
        api.on_remove_driver(move |p| a.borrow_mut().remove_driver(&p));
        let a = app.clone();
        api.on_restart_plugin(move |p| {
            a.borrow()
                .send(backend::Command::RestartPlugin(p.to_string()))
        });
        let a = app.clone();
        api.on_set_excluded(move |address, excluded| {
            a.borrow_mut().set_excluded(&address, excluded)
        });
        let a = app.clone();
        api.on_delete_preset(move |name| a.borrow_mut().delete_preset(&name));
        let a = app.clone();
        api.on_set_theme(move |i| a.borrow_mut().set_theme_index(i));
        let a = app.clone();
        api.on_set_reduced_motion(move |on| {
            let mut a = a.borrow_mut();
            a.set_reduced_motion(on);
            a.refresh_flows();
        });
        let a = app.clone();
        api.on_choose_plugin(move |key, i| a.borrow_mut().choose_plugin(&key, i));
        let a = app.clone();
        api.on_start_device_procedure(move |plugin, device, id| {
            a.borrow_mut().start_device_procedure(&plugin, &device, &id)
        });
        let a = app.clone();
        api.on_first_next(move || a.borrow_mut().first_next());
        let a = app.clone();
        api.on_first_back(move || a.borrow_mut().first_back());
        let a = app.clone();
        api.on_first_install_drivers(move || a.borrow_mut().first_install_drivers());
        let a = app.clone();
        api.on_first_run_done(move || a.borrow_mut().first_run_done());
        let a = app.clone();
        api.on_check_updates(move || a.borrow_mut().check_updates());
        let a = app.clone();
        api.on_install_update(move || a.borrow_mut().install_update());
        let a = app.clone();
        api.on_set_prereleases(move |on| a.borrow_mut().set_prereleases(on));
        api.on_restart_app(update::request_restart);
        api.on_open_url(|url| update::open_url(&url));
    }

    /// Opens a screen by name, for screenshots: `home`, `component:<address or suffix>`…
    pub fn open_screen(app: &Shared, name: &str) {
        let (screen, arg) = name.split_once(':').unwrap_or((name, ""));
        let mut a = app.borrow_mut();
        if screen == "component" && arg.contains("+advanced") {
            a.open_component(arg.trim_end_matches("+advanced"));
            a.toggle_advanced();
            return;
        }
        match screen {
            "plugins" => a.go(Screen::Plugins),
            "settings" => a.go(Screen::Settings),
            "add" => a.start_add(),
            "first" => {
                for _ in 0..arg.parse::<u8>().unwrap_or(0) {
                    a.first_next();
                }
                a.go(Screen::FirstRun)
            }
            "component" => a.open_component(arg),
            // procedure:<component>/<id>, e.g. procedure:pedals/calibrate-brake
            "procedure" => {
                if let Some((component, id)) = arg.rsplit_once('/') {
                    a.open_component(component);
                    if let Some(address) = a.current.clone() {
                        a.start_procedure(simrig_host::Target::Component(address), id);
                    }
                }
            }
            // set:<item>=<number>, on the open component
            "set" => {
                if let Some((item, v)) = arg.split_once('=') {
                    a.set_number(item, v.parse().unwrap_or(0.0));
                }
            }
            "save" => a.save_component(),
            // update:<version>[/<stage>], the update window on a made-up release
            "update" => a.preview_update(arg),
            "driver" => a.install_driver(arg),
            "generic" => a.add_generic(arg),
            "choose" => a.add_choose(arg),
            "install" => a.add_install_driver(),
            "submit" => a.submit_step(arg),
            // choice:<element index>:<option index>, in a procedure step
            "choice" => {
                if let Some((i, c)) = arg.split_once(':') {
                    a.step_choice(i.parse().unwrap_or(0), c.parse().unwrap_or(0));
                }
            }
            // setup:<plugin>/<device>/<id>, e.g. setup:sim/generic0/setup
            "setup" => {
                let parts: Vec<&str> = arg.splitn(3, '/').collect();
                if let [plugin, device, id] = parts[..] {
                    let target = simrig_host::Target::Device {
                        plugin: plugin.into(),
                        device: device.into(),
                    };
                    a.start_procedure(target, id);
                }
            }
            _ => a.go(Screen::Home),
        }
    }
}
