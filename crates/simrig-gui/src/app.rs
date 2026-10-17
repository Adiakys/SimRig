//! The interface controller: turns the host's snapshots and live inputs into the models the
//! Slint screens show, and the screens' requests into backend commands. Runs on the UI thread.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

use simrig_host::Image;
use simrig_model::{ComponentKind, ItemSpec, PedalRole, Support, Value};

use crate::backend::{Backend, Command, ComponentView, Snapshot, Update};
use crate::devices::{GateLayout, Geometry};
use crate::settings::{GuiSettings, ThemeMode};
use crate::ui::{self, AppWindow, Device, GateLabel, Rig, Screen};

pub struct App {
    pub window: Weak<AppWindow>,
    pub backend: Backend,
    pub config_dir: PathBuf,
    pub settings: GuiSettings,
    pub geometry: Geometry,
    pub snapshot: Snapshot,
    /// Latest live inputs per component address.
    pub live: HashMap<String, BTreeMap<String, Value>>,
    pub models: Models,
    /// Gate pictures and label models per shifter configuration.
    gates: HashMap<String, (slint::Image, ModelRc<GateLabel>)>,
    /// Pictures shipped by plugins, by path.
    pictures: HashMap<PathBuf, slint::Image>,
    /// The component open in the settings screen.
    pub current: Option<String>,
    pub component: crate::component::ComponentScreen,
    pub procedure: crate::procedure::ProcedureScreen,
    pub flows: crate::flows::Flows,
    /// Rotated layers drawn on the CPU, when the renderer cannot rotate.
    raster: Option<crate::raster::Raster>,
    snapshot_count: usize,
}

pub struct Models {
    pub top: Rc<VecModel<Device>>,
    pub bottom: Rc<VecModel<Device>>,
    pub side: Rc<VecModel<Device>>,
    pub toasts: Rc<VecModel<ui::Toast>>,
}

pub type Shared = Rc<RefCell<App>>;

thread_local! {
    static APP: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

/// Live inputs waiting for the UI thread. They are merged here instead of queued, so a busy
/// interface skips frames rather than falling behind the hardware.
static PENDING_LIVE: Mutex<Option<BTreeMap<String, BTreeMap<String, Value>>>> = Mutex::new(None);

/// Delivers a backend update to the app, on the UI thread.
pub fn deliver(update: Update) {
    if let Update::Live(batch) = update {
        let Ok(mut pending) = PENDING_LIVE.lock() else {
            return;
        };
        let scheduled = pending.is_some();
        let merged = pending.get_or_insert_with(BTreeMap::new);
        for (address, values) in batch {
            merged.entry(address).or_default().extend(values);
        }
        if scheduled {
            return;
        }
        drop(pending);
        let _ = slint::invoke_from_event_loop(|| {
            let batch = PENDING_LIVE.lock().ok().and_then(|mut p| p.take());
            if let (Some(app), Some(batch)) = (APP.with(|a| a.borrow().clone()), batch) {
                App::handle(&app, Update::Live(batch.into_iter().collect()));
            }
        });
        return;
    }
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = APP.with(|a| a.borrow().clone()) {
            App::handle(&app, update);
        }
    });
}

pub fn install(app: Shared) {
    APP.with(|a| *a.borrow_mut() = Some(app));
}

pub fn uninstall() -> Option<Shared> {
    APP.with(|a| a.borrow_mut().take())
}

impl Models {
    pub fn new() -> Self {
        Self {
            top: Rc::new(VecModel::default()),
            bottom: Rc::new(VecModel::default()),
            side: Rc::new(VecModel::default()),
            toasts: Rc::new(VecModel::default()),
        }
    }
}

impl App {
    pub fn new(window: &AppWindow, backend: Backend, config_dir: PathBuf, raster: bool) -> Self {
        let settings = GuiSettings::load(&config_dir);
        let models = Models::new();
        let rig = window.global::<Rig>();
        rig.set_top(models.top.clone().into());
        rig.set_bottom(models.bottom.clone().into());
        rig.set_side(models.side.clone().into());
        rig.set_toasts(models.toasts.clone().into());
        rig.set_app_version(env!("CARGO_PKG_VERSION").into());
        if let Some(preset) = &settings.preset {
            rig.set_preset(preset.into());
        }
        let theme = window.global::<ui::Theme>();
        theme.set_mode(settings.theme.index());
        theme.set_reduced_motion(settings.reduced_motion);

        let geometry = Geometry::load();
        set_geometry(window, &geometry);
        window.global::<ui::Geometry>().set_raster(raster);
        Self {
            window: window.as_weak(),
            backend,
            config_dir,
            settings,
            geometry,
            snapshot: Snapshot::default(),
            live: HashMap::new(),
            models,
            gates: HashMap::new(),
            pictures: HashMap::new(),
            current: None,
            component: Default::default(),
            procedure: Default::default(),
            flows: Default::default(),
            raster: raster.then(Default::default),
            snapshot_count: 0,
        }
    }

    pub fn window(&self) -> AppWindow {
        self.window.upgrade().expect("the window outlives the app")
    }

    pub fn send(&self, command: Command) {
        self.backend.send(command);
    }

    pub fn handle(app: &Shared, update: Update) {
        let mut this = app.borrow_mut();
        match update {
            Update::Snapshot(snapshot) => {
                this.snapshot = *snapshot;
                let addresses: Vec<String> = this
                    .snapshot
                    .components
                    .iter()
                    .map(|c| c.component.address.clone())
                    .collect();
                this.live.retain(|a, _| addresses.contains(a));
                let first = this.snapshot_count == 0;
                this.snapshot_count += 1;
                this.rebuild_home();
                this.refresh_component();
                this.refresh_flows();
                if first && this.needs_first_run() && this.window().get_screen() == Screen::Home {
                    this.go(Screen::FirstRun);
                }
            }
            Update::Component(view) => {
                let address = view.component.address.clone();
                match this
                    .snapshot
                    .components
                    .iter_mut()
                    .find(|c| c.component.address == address)
                {
                    Some(slot) => *slot = *view,
                    None => this.snapshot.components.push(*view),
                }
                this.update_cards(|_| true);
                if this.current.as_deref() == Some(&address) {
                    this.refresh_component();
                }
            }
            Update::Live(batch) => {
                let mut moved = Vec::new();
                for (address, values) in batch {
                    this.live.entry(address.clone()).or_default().extend(values);
                    moved.push(address);
                }
                this.update_cards(|d| moved.iter().any(|m| same_device(m, d)));
                this.refresh_live();
            }
            Update::Procedure { session, state } => this.procedure_update(session, state),
            Update::ProcedureFailed(error) => this.procedure_failed(error),
            Update::Done { ok, text } => {
                this.toast(&text, if ok { "success" } else { "error" });
            }
            Update::PresetApplied {
                name,
                saved,
                applied,
                ignored,
            } => this.preset_applied(name, saved, applied, ignored),
            Update::DriverDone { plugin, ok, text } => this.driver_done(plugin, ok, text),
            Update::Procedures { target, procedures } => this.flow_procedures(target, procedures),
        }
    }

    // ------------------------------------------------------------------ home

    fn rebuild_home(&mut self) {
        let mut top = Vec::new();
        let mut bottom = Vec::new();
        let mut side = Vec::new();
        let mut views: Vec<&ComponentView> = self
            .snapshot
            .components
            .iter()
            .filter(|c| !c.component.excluded)
            .collect();
        views.sort_by_key(|c| (c.component.description.kind, c.component.address.clone()));
        let devices: Vec<(ComponentKind, Device)> = views
            .iter()
            .map(|v| (v.component.description.kind, v.component.address.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(kind, address)| (kind, self.device(&address)))
            .collect();
        for (kind, device) in devices {
            match kind {
                ComponentKind::WheelBase | ComponentKind::SteeringWheel => top.push(device),
                ComponentKind::Pedals => bottom.push(device),
                ComponentKind::Shifter | ComponentKind::Handbrake => side.push(device),
            }
        }
        // The rig is a column of cards when there is nothing on the side, and vice versa.
        if top.is_empty() && bottom.is_empty() {
            top = std::mem::take(&mut side);
        }
        let count = top.len() + bottom.len() + side.len();
        self.models.top.set_vec(top);
        self.models.bottom.set_vec(bottom);
        self.models.side.set_vec(side);

        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_device_count(count as i32);
        rig.set_starting(false);
        let (text, tone) = self.rig_status(count);
        rig.set_status_text(text.into());
        rig.set_status_tone(tone.into());
        let presets: Vec<ui::PresetItem> = self
            .snapshot
            .presets
            .iter()
            .map(|name| ui::PresetItem {
                name: name.into(),
                current: self.settings.preset.as_deref() == Some(name),
            })
            .collect();
        rig.set_presets(ModelRc::new(VecModel::from(presets)));
    }

    fn rig_status(&self, count: usize) -> (String, &'static str) {
        let failed = self
            .snapshot
            .plugins
            .iter()
            .filter(|p| matches!(p.state, simrig_host::PluginState::Failed(_)))
            .count();
        if failed > 0 {
            return (
                if failed == 1 {
                    "1 plugin not responding".into()
                } else {
                    format!("{failed} plugins not responding")
                },
                "error",
            );
        }
        let to_set_up = self.flows.devices_to_set_up(&self.snapshot);
        if to_set_up > 0 {
            return (
                if to_set_up == 1 {
                    "1 device to set up".into()
                } else {
                    format!("{to_set_up} devices to set up")
                },
                "warning",
            );
        }
        match count {
            0 => ("No devices".into(), "neutral"),
            1 => ("1 device ready".into(), "success"),
            n => (format!("{n} devices ready"), "success"),
        }
    }

    /// Updates the live state of the cards for which `which` holds.
    pub fn update_cards(&mut self, which: impl Fn(&str) -> bool) {
        let models = [
            self.models.top.clone(),
            self.models.bottom.clone(),
            self.models.side.clone(),
        ];
        for model in models {
            for i in 0..model.row_count() {
                let Some(old) = model.row_data(i) else {
                    continue;
                };
                if !which(&old.address) {
                    continue;
                }
                let new = self.device(&old.address);
                if new != old {
                    model.set_row_data(i, new);
                }
            }
        }
    }

    pub fn view(&self, address: &str) -> Option<&ComponentView> {
        self.snapshot
            .components
            .iter()
            .find(|c| c.component.address == address)
    }

    fn input(&self, address: &str, item: &str) -> Option<&Value> {
        self.live.get(address)?.get(item)
    }

    fn param(&self, address: &str, item: &str) -> Option<Value> {
        let view = self.view(address)?;
        match &view.component.description.item(item)?.support {
            Support::Supported(ItemSpec::Param(spec)) => Some(spec.value.clone()),
            _ => None,
        }
    }

    /// The wheel base on the same device as `address` (a steering wheel turns with its base).
    fn base_of(&self, address: &str) -> Option<String> {
        let view = self.view(address)?;
        self.snapshot
            .components
            .iter()
            .find(|c| {
                c.component.description.kind == ComponentKind::WheelBase
                    && c.component.plugin == view.component.plugin
                    && c.component.device.id == view.component.device.id
            })
            .map(|c| c.component.address.clone())
    }

    /// Everything a card or the settings screen shows about a component, live.
    pub fn device(&mut self, address: &str) -> Device {
        let Some(view) = self.view(address).cloned() else {
            return Device::default();
        };
        let c = &view.component;
        let d = &c.description;
        let mut device = Device {
            address: c.address.clone().into(),
            kind: d.kind.key().into(),
            kind_label: d.kind.label().into(),
            name: d.name.clone().into(),
            brand: view.plugin_name.clone().into(),
            state: "ready".into(),
            state_text: "Ready".into(),
            rim: d.appearance.rim.map(|r| r.key()).unwrap_or("round").into(),
            ..Device::default()
        };
        match &view.image {
            Image::User(path) | Image::Model(path) => {
                if let Some(picture) = self.picture(path) {
                    device.picture = picture;
                }
            }
            Image::Generic { rim, .. } => {
                if let Some(rim) = rim {
                    device.rim = rim.key().into();
                }
            }
        }

        match d.kind {
            ComponentKind::WheelBase | ComponentKind::SteeringWheel => {
                let base = if d.kind == ComponentKind::WheelBase {
                    Some(c.address.clone())
                } else {
                    self.base_of(&c.address)
                };
                if let Some(base) = base {
                    if let Some(angle) = self.input(&base, "angle").and_then(Value::as_f64) {
                        device.has_angle = true;
                        device.angle = angle as f32;
                    }
                    if let Some(rotation) = self.param(&base, "rotation").and_then(|v| v.as_f64()) {
                        device.rotation = rotation as f32;
                    }
                    if let Some(ffb) = self.param(&base, "ffb_strength").and_then(|v| v.as_f64()) {
                        device.has_ffb = true;
                        device.ffb = ffb as f32;
                    }
                }
                if d.kind == ComponentKind::SteeringWheel {
                    let pivot = self.geometry.steering_wheel.pivot;
                    if let Some(raster) = &mut self.raster {
                        device.frame =
                            raster.wheel(&device.rim, device.angle, (pivot[0], pivot[1]));
                    }
                }
            }
            ComponentKind::Pedals => {
                let roles: Vec<&str> = d.pedals.iter().map(|r| r.key()).collect();
                let hinges = self.geometry.pedal_hinges(&roles);
                for role in &d.pedals {
                    let value = self
                        .input(&c.address, &format!("{}.position", role.key()))
                        .and_then(axis)
                        .unwrap_or(0.0);
                    let [x, y] = hinges.get(role.key()).copied().unwrap_or_default();
                    match role {
                        PedalRole::Clutch => {
                            device.has_clutch = true;
                            device.clutch = value;
                            device.clutch_x = x;
                            device.clutch_y = y;
                        }
                        PedalRole::Brake => {
                            device.has_brake = true;
                            device.brake = value;
                            device.brake_x = x;
                            device.brake_y = y;
                        }
                        PedalRole::Throttle => {
                            device.has_throttle = true;
                            device.throttle = value;
                            device.throttle_x = x;
                            device.throttle_y = y;
                        }
                    }
                }
            }
            ComponentKind::Shifter => {
                let mode = self
                    .param(&c.address, "mode")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "h_pattern".into());
                let gears = self
                    .param(&c.address, "gears")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(6.0) as u8;
                let reverse = self
                    .param(&c.address, "reverse_position")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "right_down".into());
                let sequential = mode == "sequential";
                let gear = if sequential {
                    match self.input(&c.address, "sequential") {
                        Some(Value::Buttons(b)) if b.first() == Some(&true) => "+".to_string(),
                        Some(Value::Buttons(b)) if b.get(1) == Some(&true) => "-".to_string(),
                        _ => "N".to_string(),
                    }
                } else {
                    match self.input(&c.address, "gear") {
                        Some(Value::Gear(g)) => g.to_string(),
                        _ => "N".to_string(),
                    }
                };
                self.shifter_drawing(&mut device, &mode, gears, &reverse, gear, None);
            }
            ComponentKind::Handbrake => {
                let pull = self
                    .input(&c.address, "position")
                    .and_then(axis)
                    .or_else(|| match self.input(&c.address, "pressed") {
                        Some(Value::Bool(true)) => Some(1.0),
                        Some(Value::Bool(false)) => Some(0.0),
                        _ => None,
                    });
                device.has_pull = pull.is_some();
                self.handbrake_drawing(&mut device, pull.unwrap_or(0.0));
            }
        }
        device
    }

    /// The gate of a shifter's configuration, the knob on `gear`, and `target` marked.
    fn shifter_drawing(
        &mut self,
        device: &mut Device,
        mode: &str,
        gears: u8,
        reverse: &str,
        gear: String,
        target: Option<&str>,
    ) {
        let sequential = mode == "sequential";
        let (layout, key) = if sequential {
            (GateLayout::sequential(), "sequential".to_string())
        } else {
            (
                GateLayout::h_pattern(gears, reverse),
                format!("h{gears}{reverse}"),
            )
        };
        device.layout_text = if sequential {
            "Sequential".into()
        } else {
            format!("H {gears}+R").into()
        };
        let (gate, labels) = self.gate(&key, &layout);
        device.gate = gate;
        device.gate_labels = labels;
        let (x, y) = self.geometry.knob(&layout, &gear);
        device.knob_x = x;
        device.knob_y = y;
        device.gear = gear.into();
        if let Some(target) = target {
            let (x, y) = self.geometry.knob(&layout, target);
            device.target = target.into();
            device.target_x = x;
            device.target_y = y;
        }
    }

    /// The handbrake lever pulled by `pull` (0..1).
    fn handbrake_drawing(&mut self, device: &mut Device, pull: f32) {
        device.pull = pull;
        let lever = &self.geometry.handbrake.lever;
        if let Some(raster) = &mut self.raster {
            device.frame = raster.lever(
                pull.clamp(0.0, 1.0) * lever.pulled_angle,
                (lever.pivot[0], lever.pivot[1]),
            );
        }
        device.pull_text = if pull < 0.02 {
            "Released".into()
        } else if pull > 0.98 {
            "Fully pulled".into()
        } else {
            "Pulled".into()
        };
    }

    /// The drawing a procedure step asks for: a component as its values describe it.
    pub fn preview_device(&mut self, preview: &simrig_model::Preview, name: &str) -> Device {
        let mut device = Device {
            kind: preview.kind.key().into(),
            kind_label: preview.kind.label().into(),
            name: name.into(),
            state: "ready".into(),
            rim: "round".into(),
            ..Device::default()
        };
        let text = |item: &str| {
            preview
                .values
                .get(item)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        match preview.kind {
            ComponentKind::Shifter => {
                let mode = text("mode").unwrap_or_else(|| "h_pattern".into());
                let gears = preview
                    .values
                    .get("gears")
                    .and_then(Value::as_f64)
                    .unwrap_or(6.0) as u8;
                let reverse = text("reverse_position").unwrap_or_else(|| "right_down".into());
                let gear = match preview.values.get("gear") {
                    Some(Value::Gear(g)) => g.to_string(),
                    _ => "N".into(),
                };
                let target = preview.target.clone();
                self.shifter_drawing(&mut device, &mode, gears, &reverse, gear, target.as_deref());
            }
            ComponentKind::Handbrake => {
                let pull = preview.values.get("position").and_then(axis).unwrap_or(0.0);
                device.has_pull = true;
                self.handbrake_drawing(&mut device, pull);
            }
            _ => {}
        }
        device
    }

    fn gate(&mut self, key: &str, layout: &GateLayout) -> (slint::Image, ModelRc<GateLabel>) {
        if let Some(cached) = self.gates.get(key) {
            return cached.clone();
        }
        let svg = self.geometry.gate_svg(layout);
        let image = slint::Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default();
        let labels: Vec<GateLabel> = self
            .geometry
            .labels(layout)
            .into_iter()
            .map(|(text, x, y)| GateLabel {
                text: text.into(),
                x,
                y,
            })
            .collect();
        let entry = (image, ModelRc::new(VecModel::from(labels)));
        self.gates.insert(key.to_string(), entry.clone());
        entry
    }

    fn picture(&mut self, path: &PathBuf) -> Option<slint::Image> {
        if let Some(image) = self.pictures.get(path) {
            return Some(image.clone());
        }
        // A folder of layers is not animated yet: only single pictures are drawn.
        if !path.is_file() {
            return None;
        }
        let image = slint::Image::load_from_path(path).ok()?;
        self.pictures.insert(path.clone(), image.clone());
        Some(image)
    }

    // ------------------------------------------------------------------ navigation

    pub fn go(&mut self, screen: Screen) {
        let window = self.window();
        if screen == Screen::Home {
            self.current = None;
        }
        window.set_screen(screen);
    }

    // ------------------------------------------------------------------ presets

    pub fn apply_preset(&mut self, name: String) {
        self.send(Command::ApplyPreset(name));
    }

    fn preset_applied(&mut self, name: String, saved: usize, applied: usize, ignored: Vec<String>) {
        self.settings.preset = Some(name.clone());
        self.save_settings();
        let window = self.window();
        let rig = window.global::<Rig>();
        rig.set_preset(name.clone().into());
        let note = if applied == 0 && ignored.is_empty() {
            "Applied and saved in the drivers".to_string()
        } else if saved == 0 {
            "Applied until the devices are unplugged".to_string()
        } else {
            "Applied; some values last until unplugged".to_string()
        };
        rig.set_preset_note(note.into());
        if !ignored.is_empty() {
            self.toast(
                &format!(
                    "Preset \"{name}\": {} value(s) not applied. {}",
                    ignored.len(),
                    ignored.first().cloned().unwrap_or_default()
                ),
                "warning",
            );
        }
        self.rebuild_home();
    }

    pub fn save_settings(&self) {
        if let Err(e) = self.settings.save(&self.config_dir) {
            eprintln!("cannot save the app settings: {e}");
        }
    }

    pub fn set_theme(&mut self, mode: ThemeMode) {
        self.settings.theme = mode;
        self.save_settings();
        self.window().global::<ui::Theme>().set_mode(mode.index());
    }

    pub fn set_reduced_motion(&mut self, on: bool) {
        self.settings.reduced_motion = on;
        self.save_settings();
        self.window().global::<ui::Theme>().set_reduced_motion(on);
    }

    // ------------------------------------------------------------------ toasts

    pub fn toast(&mut self, text: &str, level: &str) {
        let toasts = self.models.toasts.clone();
        // Keep the latest few: they are confirmations, not a log.
        while toasts.row_count() >= 3 {
            toasts.remove(0);
        }
        let toast = ui::Toast {
            text: text.into(),
            level: level.into(),
        };
        toasts.push(toast.clone());
        // Errors stay until dismissed; the rest go away by themselves.
        if level != "error" {
            let weak = Rc::downgrade(&toasts);
            slint::Timer::single_shot(std::time::Duration::from_secs(5), move || {
                if let Some(toasts) = weak.upgrade()
                    && let Some(i) = toasts.iter().position(|t| t == toast)
                {
                    toasts.remove(i);
                }
            });
        }
    }

    pub fn dismiss_toast(&mut self, index: usize) {
        if index < self.models.toasts.row_count() {
            self.models.toasts.remove(index);
        }
    }
}

/// True when `moved` and `address` are on the same device (a wheel follows its base).
fn same_device(moved: &str, address: &str) -> bool {
    let device = |a: &str| a.rsplit_once('/').map(|(d, _)| d.to_string());
    moved == address || device(moved) == device(address)
}

fn axis(value: &Value) -> Option<f32> {
    match value {
        Value::Axis(a) => Some(a.value as f32),
        Value::Float(v) => Some(*v as f32),
        _ => None,
    }
}

fn set_geometry(window: &AppWindow, g: &Geometry) {
    let geo = window.global::<ui::Geometry>();
    geo.set_base_w(g.wheel_base.view_box[0]);
    geo.set_base_h(g.wheel_base.view_box[1]);
    geo.set_wheel_w(g.steering_wheel.view_box[0]);
    geo.set_wheel_h(g.steering_wheel.view_box[1]);
    geo.set_wheel_pivot_x(g.steering_wheel.pivot[0]);
    geo.set_wheel_pivot_y(g.steering_wheel.pivot[1]);
    geo.set_pedals_w(g.pedals.view_box[0]);
    geo.set_pedals_h(g.pedals.view_box[1]);
    geo.set_pedal_w(g.pedals.pedal.size[0]);
    geo.set_pedal_h(g.pedals.pedal.size[1]);
    geo.set_pedal_hinge_x(g.pedals.pedal.hinge[0]);
    geo.set_pedal_hinge_y(g.pedals.pedal.hinge[1]);
    geo.set_shifter_w(g.shifter.view_box[0]);
    geo.set_shifter_h(g.shifter.view_box[1]);
    geo.set_knob_size(g.shifter.knob.size[0]);
    geo.set_handbrake_w(g.handbrake.view_box[0]);
    geo.set_handbrake_h(g.handbrake.view_box[1]);
    geo.set_lever_pivot_x(g.handbrake.lever.pivot[0]);
    geo.set_lever_pivot_y(g.handbrake.lever.pivot[1]);
    geo.set_lever_angle(g.handbrake.lever.pulled_angle);
}
