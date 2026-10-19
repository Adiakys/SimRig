//! The settings screen of one component, rendered from its description: Essentials first,
//! Advanced collapsed, then the plugin's own settings and its guided procedures. Unsupported
//! items keep their row, disabled, with the reason.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use simrig_model::{
    CUSTOM_PREFIX, ComponentDescription, ComponentKind, CurvePoint, CustomParam, ItemClass,
    ItemSpec, ParamType, PedalRole, Persistence, Support, Value,
};

use crate::app::App;
use crate::backend::{Command, ComponentView};
use crate::format;
use crate::ui::{ChipData, Rig, RowKind, Screen, SettingRow};

/// Prefix of the rows that start a plugin procedure.
pub const PROCEDURE_PREFIX: &str = "procedure:";

pub struct ComponentScreen {
    pub rows: Rc<VecModel<SettingRow>>,
    pub inputs: Rc<VecModel<SettingRow>>,
    advanced: bool,
    /// Values when the screen opened: "Reset values" goes back to them.
    opened: BTreeMap<String, Value>,
    /// Persistent items changed and not saved into the driver yet, by component.
    pub unsaved: BTreeSet<(String, String)>,
    /// Inner models of the rows, kept while their content is the same, so a refresh does not
    /// rebuild what the user is dragging.
    floats: HashMap<String, (Vec<f32>, ModelRc<f32>)>,
    strings: HashMap<String, (Vec<String>, ModelRc<SharedString>)>,
}

impl Default for ComponentScreen {
    fn default() -> Self {
        Self {
            rows: Rc::new(VecModel::default()),
            inputs: Rc::new(VecModel::default()),
            advanced: false,
            opened: BTreeMap::new(),
            unsaved: BTreeSet::new(),
            floats: HashMap::new(),
            strings: HashMap::new(),
        }
    }
}

impl ComponentScreen {
    fn floats(&mut self, key: String, values: Vec<f32>) -> ModelRc<f32> {
        if let Some((old, model)) = self.floats.get(&key) {
            if *old == values {
                return model.clone();
            }
            // Same length: update in place, the elements stay (a dragged point keeps its drag).
            if old.len() == values.len() {
                let model = model.clone();
                for (i, v) in values.iter().enumerate() {
                    if model.row_data(i) != Some(*v) {
                        model.set_row_data(i, *v);
                    }
                }
                self.floats.insert(key, (values, model.clone()));
                return model;
            }
        }
        let model: ModelRc<f32> = ModelRc::new(VecModel::from(values.clone()));
        self.floats.insert(key, (values, model.clone()));
        model
    }

    fn strings(&mut self, key: String, values: Vec<String>) -> ModelRc<SharedString> {
        if let Some((old, model)) = self.strings.get(&key)
            && *old == values
        {
            return model.clone();
        }
        let model: ModelRc<SharedString> = ModelRc::new(VecModel::from(
            values.iter().map(SharedString::from).collect::<Vec<_>>(),
        ));
        self.strings.insert(key, (values, model.clone()));
        model
    }
}

/// The standard items shown first, per component type (pedal items per pedal).
fn essentials(kind: ComponentKind) -> &'static [&'static str] {
    match kind {
        ComponentKind::WheelBase => &["rotation", "ffb_strength", "natural_damper"],
        ComponentKind::SteeringWheel => &["led_brightness", "analog_paddle_mode", "vibration"],
        ComponentKind::Pedals => &["curve", "deadzone_low", "deadzone_high", "max_force"],
        ComponentKind::Shifter => &["mode", "gears", "reverse_position"],
        ComponentKind::Handbrake => &["mode", "deadzone", "curve"],
    }
}

/// Common rotations offered as one-click values.
const QUICK_ROTATIONS: [f64; 6] = [360.0, 540.0, 720.0, 900.0, 1080.0, 2520.0];

/// What a row needs to know about one parameter, standard or custom.
struct ParamInfo<'a> {
    item: String,
    label: String,
    help: String,
    ty: &'a ParamType,
    value: &'a Value,
    unit: String,
    persistence: Persistence,
}

impl App {
    pub fn open_component(&mut self, query: &str) {
        let Some(address) = self
            .snapshot
            .components
            .iter()
            .map(|c| c.component.address.clone())
            .find(|a| a == query || a.ends_with(&format!("/{query}")))
        else {
            return;
        };
        self.current = Some(address.clone());
        self.component.advanced = false;
        self.inputs.tab = false;
        self.window().global::<Rig>().set_inputs_tab(false);
        self.component.opened = self
            .view(&address)
            .map(|v| current_values(&v.component.description))
            .unwrap_or_default();
        self.refresh_component();
        self.go(Screen::Component);
    }

    /// Rebuilds the settings rows of the open component.
    pub fn refresh_component(&mut self) {
        let Some(address) = self.current.clone() else {
            return;
        };
        let window = self.window();
        let rig = window.global::<Rig>();
        let Some(view) = self.view(&address).cloned() else {
            // Unplugged while open: the screen stays, faded, until it comes back.
            rig.set_current_gone(true);
            return;
        };
        rig.set_current_gone(false);
        rig.set_current(self.device(&address));
        rig.set_current_title(view.component.description.kind.label().into());
        rig.set_current_chips(ModelRc::new(VecModel::from(self.chips(&view))));
        rig.set_current_untested(view.untested);
        self.refresh_inputs();

        let rows = self.rows(&view);
        let model = self.component.rows.clone();
        let same_shape = model.row_count() == rows.len()
            && rows.iter().enumerate().all(|(i, r)| {
                model
                    .row_data(i)
                    .is_some_and(|o| o.item == r.item && o.kind == r.kind)
            });
        if same_shape {
            for (i, row) in rows.into_iter().enumerate() {
                if model.row_data(i).as_ref() != Some(&row) {
                    model.set_row_data(i, row);
                }
            }
        } else {
            model.set_vec(rows);
        }
        rig.set_rows(model.into());

        let unsaved = self.component.unsaved.iter().any(|(a, _)| *a == address);
        rig.set_unsaved(unsaved);
        rig.set_save_privileged(view.system_plugin);
        rig.set_save_note(
            if view.system_plugin {
                "Asks for your password once, to save it in the system."
            } else {
                "Saved in the driver: restored every time the device is plugged in."
            }
            .into(),
        );
        self.refresh_live();
    }

    /// Live values of the open component: the drawing and the list of inputs.
    pub fn refresh_live(&mut self) {
        let Some(address) = self.current.clone() else {
            return;
        };
        let Some(view) = self.view(&address).cloned() else {
            return;
        };
        self.refresh_inputs();
        let window = self.window();
        let rig = window.global::<Rig>();
        let device = self.device(&address);
        if rig.get_current() != device {
            rig.set_current(device);
        }

        let d = &view.component.description;
        let live = self.live.get(&address).cloned().unwrap_or_default();
        let mut rows = Vec::new();
        for item in d.items.iter().filter(|i| i.class == ItemClass::Input) {
            let def = d.kind.item_def(&item.id);
            let label = input_label(d.kind, &item.id);
            let mut row = SettingRow {
                kind: RowKind::Info,
                item: item.id.clone().into(),
                label: label.into(),
                ..SettingRow::default()
            };
            match &item.support {
                Support::Supported(_) => {
                    row.enabled = true;
                    let (text, unit) = live
                        .get(&item.id)
                        .map(|v| match (item.id.as_str(), v) {
                            ("hats", Value::Ints(hats)) => (format::hats(hats), String::new()),
                            _ => format::input(v, def.and_then(|d| d.unit)),
                        })
                        .unwrap_or_else(|| ("–".into(), String::new()));
                    row.value_text = text.into();
                    row.unit = unit.into();
                }
                Support::Unsupported(u) => row.reason = u.explain().into(),
            }
            rows.push(row);
        }
        let model = self.component.inputs.clone();
        if model.row_count() == rows.len() {
            for (i, row) in rows.into_iter().enumerate() {
                if model.row_data(i).as_ref() != Some(&row) {
                    model.set_row_data(i, row);
                }
            }
        } else {
            model.set_vec(rows);
            rig.set_live_values(model.into());
        }
    }

    /// What needs attention on the device: its driver, when something is to be done about it.
    /// The plugin's name is already under the title, the firmware among the live values, and a
    /// driver working needs no chip.
    fn chips(&self, view: &ComponentView) -> Vec<ChipData> {
        let Some(plugin) = self
            .snapshot
            .plugins
            .iter()
            .find(|p| p.id == view.component.plugin)
            .filter(|p| p.has_driver)
        else {
            return Vec::new();
        };
        let text = match &plugin.driver {
            Some(s) if s.provided_by_system => return Vec::new(),
            Some(s) if s.needs_reboot => "Driver needs a restart",
            Some(s) if s.installed && plugin.driver_update.is_some() => "Driver update available",
            Some(s) if s.installed => return Vec::new(),
            _ => "Driver not installed",
        };
        vec![ChipData {
            text: text.into(),
            tone: "warning".into(),
        }]
    }

    fn rows(&mut self, view: &ComponentView) -> Vec<SettingRow> {
        let c = &view.component;
        let d = &c.description;
        let address = c.address.clone();
        let mut essential = Vec::new();
        let mut advanced = Vec::new();

        // Standard parameters (and outputs), Essentials first; pedals per pedal.
        let groups: Vec<(Option<PedalRole>, String)> = if d.kind == ComponentKind::Pedals {
            // Pedals listed like on the floor: clutch, brake, throttle.
            [PedalRole::Clutch, PedalRole::Brake, PedalRole::Throttle]
                .into_iter()
                .filter(|r| d.pedals.contains(r))
                .map(|r| (Some(r), format!("{}.", r.key())))
                .collect()
        } else {
            vec![(None, String::new())]
        };
        for (role, prefix) in &groups {
            let mut first = Vec::new();
            let mut rest = Vec::new();
            for item in &d.items {
                let Some(key) = item.id.strip_prefix(prefix.as_str()) else {
                    continue;
                };
                if !matches!(item.class, ItemClass::Param | ItemClass::Output) {
                    continue;
                }
                let row = self.standard_row(&address, d, &item.id, key, &item.support);
                if item.class == ItemClass::Param && essentials(d.kind).contains(&key) {
                    first.push((essentials(d.kind).iter().position(|k| *k == key), row));
                } else {
                    rest.push(row);
                }
            }
            first.sort_by_key(|(i, _)| *i);
            if let Some(role) = role {
                let title = format::role_label(*role);
                if !first.is_empty() {
                    essential.push(heading(title, 1));
                }
                if !rest.is_empty() {
                    advanced.push(heading(title, 1));
                }
            }
            essential.extend(first.into_iter().map(|(_, r)| r));
            advanced.extend(rest);
        }

        // The plugin's own settings: grouped, advanced ones with the standard ones.
        let mut custom: Vec<&CustomParam> = d.custom.iter().collect();
        custom.sort_by_key(|p| (p.group.clone(), p.order));
        let mut brand = Vec::new();
        let mut group: Option<String> = None;
        for p in custom {
            let row = self.custom_row(&address, p);
            if p.advanced {
                advanced.push(row);
                continue;
            }
            if p.group != group {
                group = p.group.clone();
                if let Some(g) = &group {
                    brand.push(heading(g, 1));
                }
            }
            brand.push(row);
        }

        let mut rows = vec![heading("Essentials", 0)];
        rows.extend(essential);
        let settings = advanced
            .iter()
            .filter(|r| r.kind != RowKind::Heading)
            .count();
        if settings > 0 {
            rows.push(SettingRow {
                kind: RowKind::Disclosure,
                item: "advanced".into(),
                label: "Advanced".into(),
                expanded: self.component.advanced,
                count: settings as i32,
                enabled: true,
                ..SettingRow::default()
            });
            if self.component.advanced {
                rows.extend(advanced);
            }
        }
        if !brand.is_empty() {
            rows.push(heading(&format!("{} settings", view.plugin_name), 0));
            rows.extend(brand);
        }

        // Guided procedures of the plugin, then the standard actions.
        let mut actions = Vec::new();
        for p in &d.procedures {
            let mut row = SettingRow {
                kind: RowKind::Action,
                item: format!("{PROCEDURE_PREFIX}{}", p.id).into(),
                label: p.label.clone().into(),
                help: p.help.clone().unwrap_or_default().into(),
                enabled: p.availability.is_supported(),
                ..SettingRow::default()
            };
            if let Support::Unsupported(u) = &p.availability {
                row.reason = u.explain().into();
            }
            actions.push(row);
        }
        for item in d.items.iter().filter(|i| i.class == ItemClass::Action) {
            let label = match item.id.split_once('.') {
                Some((role, key)) if d.kind == ComponentKind::Pedals => format!(
                    "{} {}",
                    PedalRole::from_key(role)
                        .map(format::role_label)
                        .unwrap_or(role),
                    format::item_label(d.kind, &format!("{role}.{key}")).to_lowercase()
                ),
                _ => format::item_label(d.kind, &item.id),
            };
            let mut row = SettingRow {
                kind: RowKind::Action,
                item: item.id.clone().into(),
                label: label.into(),
                ..SettingRow::default()
            };
            match &item.support {
                Support::Supported(ItemSpec::Action(spec)) => {
                    row.enabled = true;
                    row.confirm = spec.confirm.clone().unwrap_or_default().into();
                }
                Support::Supported(_) => row.enabled = true,
                Support::Unsupported(u) => row.reason = u.explain().into(),
            }
            actions.push(row);
        }
        if !actions.is_empty() {
            rows.push(heading("Guided procedures", 0));
            rows.extend(actions);
        }
        rows
    }

    fn standard_row(
        &mut self,
        address: &str,
        d: &ComponentDescription,
        id: &str,
        key: &str,
        support: &Support<ItemSpec>,
    ) -> SettingRow {
        let def = d.kind.item_def(id);
        let label = def.map(|d| d.label).unwrap_or(key).to_string();
        let help = format::item_help(d.kind, key).to_string();
        let unit = format::unit(def.and_then(|d| d.unit));
        match support {
            Support::Supported(ItemSpec::Param(spec)) => {
                let mut row = self.param_row(
                    address,
                    ParamInfo {
                        item: id.to_string(),
                        label,
                        help,
                        ty: &spec.ty,
                        value: &spec.value,
                        unit,
                        persistence: spec.persistence,
                    },
                );
                if let Some(c) = &spec.caveat {
                    row.caveat = c.explain().into();
                }
                row
            }
            Support::Supported(ItemSpec::Output { .. }) => SettingRow {
                kind: RowKind::Info,
                item: id.into(),
                label: label.into(),
                help: "Driven by games while you race.".into(),
                enabled: true,
                value_text: "Used by games".into(),
                ..SettingRow::default()
            },
            Support::Supported(_) => SettingRow::default(),
            Support::Unsupported(u) => {
                let kind = match def.map(|d| d.kind) {
                    Some(simrig_model::ValueKind::Bool) => RowKind::Toggle,
                    Some(simrig_model::ValueKind::Choice) => RowKind::Info,
                    Some(simrig_model::ValueKind::Curve) => RowKind::Info,
                    Some(simrig_model::ValueKind::Text) => RowKind::Info,
                    Some(_) if def.is_some_and(|d| d.class == ItemClass::Output) => RowKind::Info,
                    _ => RowKind::Slider,
                };
                SettingRow {
                    kind,
                    item: id.into(),
                    label: label.into(),
                    help: help.into(),
                    enabled: false,
                    reason: u.explain().into(),
                    value_text: "–".into(),
                    maximum: 1.0,
                    step: 1.0,
                    ..SettingRow::default()
                }
            }
        }
    }

    fn custom_row(&mut self, address: &str, p: &CustomParam) -> SettingRow {
        let mut row = self.param_row(
            address,
            ParamInfo {
                item: format!("{CUSTOM_PREFIX}{}", p.id),
                label: p.label.clone(),
                help: p.help.clone().unwrap_or_default(),
                ty: &p.ty,
                value: &p.value,
                unit: p.unit.clone().unwrap_or_default(),
                persistence: p.persistence,
            },
        );
        if let Some(c) = &p.caveat {
            row.caveat = c.explain().into();
        }
        if let Support::Unsupported(u) = &p.availability {
            row.enabled = false;
            row.reason = u.explain().into();
        }
        row
    }

    fn param_row(&mut self, address: &str, p: ParamInfo<'_>) -> SettingRow {
        let unsaved = self
            .component
            .unsaved
            .contains(&(address.to_string(), p.item.clone()));
        let persistence = match p.persistence {
            Persistence::Persistent if unsaved => "Changed: save it to keep it",
            Persistence::Persistent => "Kept with the app closed",
            Persistence::Volatile => "Until unplugged",
        };
        let mut row = SettingRow {
            item: p.item.clone().into(),
            label: p.label.into(),
            help: p.help.into(),
            enabled: true,
            unit: p.unit.into(),
            persistence: persistence.into(),
            unsaved,
            ..SettingRow::default()
        };
        match p.ty {
            ParamType::Float { min, max, step } => {
                let v = p.value.as_f64().unwrap_or(*min);
                self.number_row(&mut row, v, *min, *max, *step);
            }
            ParamType::Int { min, max, step } => {
                let v = p.value.as_f64().unwrap_or(*min as f64);
                let count = (max - min) / (*step).max(1) + 1;
                // A handful of whole values reads better as choices (4+R … 8+R).
                if count <= 6 && p.item == "gears" {
                    row.kind = RowKind::Segmented;
                    let options: Vec<String> = (0..count)
                        .map(|i| format!("{}+R", min + i * step))
                        .collect();
                    row.selected = ((v as i64 - min) / (*step).max(1)) as i32;
                    row.options = self.component.strings(p.item.clone(), options);
                } else {
                    self.number_row(&mut row, v, *min as f64, *max as f64, *step as f64);
                }
            }
            ParamType::Bool => {
                row.kind = RowKind::Toggle;
                row.checked = p.value.as_bool().unwrap_or(false);
            }
            ParamType::Choice { options } => {
                row.kind = RowKind::Segmented;
                let current = p.value.as_str().unwrap_or_default();
                row.selected = options
                    .iter()
                    .position(|o| o.id == current)
                    .map(|i| i as i32)
                    .unwrap_or(-1);
                let labels = options.iter().map(|o| o.label.clone()).collect();
                row.options = self.component.strings(p.item.clone(), labels);
            }
            ParamType::Curve { .. } => {
                row.kind = RowKind::Curve;
                let points = match p.value {
                    Value::Curve(points) => points.clone(),
                    _ => vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)],
                };
                let xs: Vec<f32> = points.iter().map(|p| p.x as f32).collect();
                let ys: Vec<f32> = points.iter().map(|p| p.y as f32).collect();
                row.curve_path = curve_path(&points).into();
                row.curve_x = self.component.floats(format!("{}#x", p.item), xs);
                row.curve_y = self.component.floats(format!("{}#y", p.item), ys);
                row.value_text = curve_name(&points).into();
            }
            ParamType::Text { .. } => {
                row.kind = RowKind::Text;
                row.text = p.value.as_str().unwrap_or_default().into();
            }
        }
        row
    }

    fn number_row(&mut self, row: &mut SettingRow, v: f64, min: f64, max: f64, step: f64) {
        row.kind = RowKind::Slider;
        row.value = v as f32;
        row.minimum = min as f32;
        row.maximum = max as f32;
        row.step = step as f32;
        row.decimals = decimals(step);
        row.value_text = format::number(v, step).into();
        if row.item.as_str() == "rotation" {
            let quick: Vec<f32> = QUICK_ROTATIONS
                .into_iter()
                .filter(|q| *q >= min && *q <= max)
                .map(|q| q as f32)
                .collect();
            row.quick = self.component.floats(format!("{}#quick", row.item), quick);
        }
    }

    // ------------------------------------------------------------------ changes

    /// The type of an item of the open component.
    fn item_type(&self, item: &str) -> Option<ParamType> {
        let view = self.view(self.current.as_deref()?)?;
        let d = &view.component.description;
        match item.strip_prefix(CUSTOM_PREFIX) {
            Some(id) => d.custom_param(id).map(|p| p.ty.clone()),
            None => match &d.item(item)?.support {
                Support::Supported(ItemSpec::Param(spec)) => Some(spec.ty.clone()),
                _ => None,
            },
        }
    }

    fn persistence(&self, item: &str) -> Persistence {
        let Some(view) = self.current.as_deref().and_then(|a| self.view(a)) else {
            return Persistence::Volatile;
        };
        let d = &view.component.description;
        match item.strip_prefix(CUSTOM_PREFIX) {
            Some(id) => d
                .custom_param(id)
                .map(|p| p.persistence)
                .unwrap_or(Persistence::Volatile),
            None => match d.item(item).map(|i| &i.support) {
                Some(Support::Supported(ItemSpec::Param(spec))) => spec.persistence,
                _ => Persistence::Volatile,
            },
        }
    }

    /// Applies a value live (no privileges), and shows it at once.
    pub fn set_value(&mut self, item: &str, value: Value) {
        let Some(address) = self.current.clone() else {
            return;
        };
        // Show it at once; the plugin's answer refreshes it.
        if let Some(view) = self
            .snapshot
            .components
            .iter_mut()
            .find(|c| c.component.address == address)
        {
            set_local(&mut view.component.description, item, value.clone());
        }
        if self.persistence(item) == Persistence::Persistent {
            self.component
                .unsaved
                .insert((address.clone(), item.to_string()));
        }
        self.send(Command::Set {
            address,
            item: item.to_string(),
            value,
        });
        self.refresh_component();
        // Rotation, force and the gate show on the home cards too.
        self.update_cards(|_| true);
    }

    pub fn set_number(&mut self, item: &str, v: f32) {
        let value = match self.item_type(item) {
            Some(ParamType::Int { .. }) => Value::Int(v.round() as i64),
            Some(ParamType::Float { .. }) => Value::Float(round_to(v as f64)),
            _ => return,
        };
        self.set_value(item, value);
    }

    pub fn set_choice(&mut self, item: &str, index: i32) {
        let value = match self.item_type(item) {
            Some(ParamType::Choice { options }) => match options.get(index as usize) {
                Some(o) => Value::Choice(o.id.clone()),
                None => return,
            },
            Some(ParamType::Int { min, step, .. }) => Value::Int(min + index as i64 * step.max(1)),
            _ => return,
        };
        self.set_value(item, value);
    }

    pub fn set_bool(&mut self, item: &str, on: bool) {
        self.set_value(item, Value::Bool(on));
    }

    pub fn set_text(&mut self, item: &str, text: &str) {
        if let Some(ty) = self.item_type(item) {
            match ty.parse(text) {
                Ok(value) => self.set_value(item, value),
                Err(e) => self.toast(&e.to_string(), "error"),
            }
        }
    }

    /// Moves one point of a curve vertically (the ends stay at their x).
    pub fn set_curve_point(&mut self, item: &str, index: usize, y: f32) {
        let Some(view) = self.current.as_deref().and_then(|a| self.view(a)) else {
            return;
        };
        let d = &view.component.description;
        let current = match item.strip_prefix(CUSTOM_PREFIX) {
            Some(id) => d.custom_param(id).map(|p| p.value.clone()),
            None => match d.item(item).map(|i| &i.support) {
                Some(Support::Supported(ItemSpec::Param(spec))) => Some(spec.value.clone()),
                _ => None,
            },
        };
        let Some(Value::Curve(mut points)) = current else {
            return;
        };
        let Some(point) = points.get_mut(index) else {
            return;
        };
        point.y = (y as f64).clamp(0.0, 1.0);
        point.y = (point.y * 100.0).round() / 100.0;
        self.set_value(item, Value::Curve(points));
    }

    pub fn toggle_advanced(&mut self) {
        self.component.advanced = !self.component.advanced;
        self.refresh_component();
    }

    pub fn run_component_action(&mut self, item: &str) {
        let Some(address) = self.current.clone() else {
            return;
        };
        if let Some(procedure) = item.strip_prefix(PROCEDURE_PREFIX) {
            self.start_procedure(simrig_host::Target::Component(address), procedure);
            return;
        }
        self.send(Command::RunAction {
            address,
            item: item.to_string(),
        });
    }

    /// Saves the changed persistent values into the driver.
    pub fn save_component(&mut self) {
        let Some(address) = self.current.clone() else {
            return;
        };
        let items: Vec<String> = self
            .component
            .unsaved
            .iter()
            .filter(|(a, _)| *a == address)
            .map(|(_, i)| i.clone())
            .collect();
        if items.is_empty() {
            return;
        }
        self.component.unsaved.retain(|(a, _)| *a != address);
        self.send(Command::Save { address, items });
        self.refresh_component();
    }

    /// Puts back the values the component had when the screen opened.
    pub fn reset_component(&mut self) {
        let Some(view) = self.current.as_deref().and_then(|a| self.view(a)) else {
            return;
        };
        let now = current_values(&view.component.description);
        let changed: Vec<(String, Value)> = self
            .component
            .opened
            .iter()
            .filter(|(item, value)| now.get(*item) != Some(value))
            .map(|(i, v)| (i.clone(), v.clone()))
            .collect();
        for (item, value) in changed {
            self.set_value(&item, value);
        }
        let address = self.current.clone().unwrap_or_default();
        self.component.unsaved.retain(|(a, _)| *a != address);
        self.refresh_component();
    }
}

fn heading(text: &str, level: i32) -> SettingRow {
    SettingRow {
        kind: RowKind::Heading,
        item: format!("heading:{text}").into(),
        label: text.into(),
        count: level,
        enabled: true,
        ..SettingRow::default()
    }
}

fn input_label(kind: ComponentKind, id: &str) -> String {
    match id.split_once('.') {
        Some((role, key)) if kind == ComponentKind::Pedals => format!(
            "{} {}",
            PedalRole::from_key(role)
                .map(format::role_label)
                .unwrap_or(role),
            format::item_label(kind, &format!("{role}.{key}")).to_lowercase()
        ),
        _ => format::item_label(kind, id),
    }
}

fn decimals(step: f64) -> i32 {
    if step <= 0.0 || step >= 1.0 {
        0
    } else {
        (-(step.log10()).floor()).clamp(1.0, 3.0) as i32
    }
}

/// Floats from a slider carry float noise (12.500000953): keep 4 decimals.
fn round_to(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Every parameter value, to go back to it.
fn current_values(d: &ComponentDescription) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    for item in &d.items {
        if let Support::Supported(ItemSpec::Param(spec)) = &item.support {
            values.insert(item.id.clone(), spec.value.clone());
        }
    }
    for p in &d.custom {
        if p.availability.is_supported() {
            values.insert(format!("{CUSTOM_PREFIX}{}", p.id), p.value.clone());
        }
    }
    values
}

fn set_local(d: &mut ComponentDescription, item: &str, value: Value) {
    match item.strip_prefix(CUSTOM_PREFIX) {
        Some(id) => {
            if let Some(p) = d.custom.iter_mut().find(|p| p.id == id) {
                p.value = value;
            }
        }
        None => {
            if let Some(i) = d.items.iter_mut().find(|i| i.id == item)
                && let Support::Supported(ItemSpec::Param(spec)) = &mut i.support
            {
                spec.value = value;
            }
        }
    }
}

/// The curve as a polyline in a 100×100 box, y up.
fn curve_path(points: &[CurvePoint]) -> String {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "{}{:.1} {:.1}",
                if i == 0 { "M" } else { "L" },
                p.x * 100.0,
                100.0 - p.y * 100.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Linear", "Progressive" or "Aggressive", from where the middle of the travel lands.
fn curve_name(points: &[CurvePoint]) -> &'static str {
    let at = |x: f64| {
        points
            .windows(2)
            .find(|w| w[0].x <= x && x <= w[1].x)
            .map(|w| {
                let t = (x - w[0].x) / (w[1].x - w[0].x).max(f64::EPSILON);
                w[0].y + t * (w[1].y - w[0].y)
            })
            .unwrap_or(x)
    };
    let mid = at(0.5);
    if (mid - 0.5).abs() < 0.04 {
        "Linear"
    } else if mid < 0.5 {
        "Progressive"
    } else {
        "Aggressive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_names() {
        let linear = [CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)];
        assert_eq!(curve_name(&linear), "Linear");
        let soft = [
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.3),
            CurvePoint::new(1.0, 1.0),
        ];
        assert_eq!(curve_name(&soft), "Progressive");
        assert_eq!(curve_path(&linear), "M0.0 100.0 L100.0 0.0");
    }

    #[test]
    fn decimals_of_steps() {
        assert_eq!(decimals(10.0), 0);
        assert_eq!(decimals(0.5), 1);
        assert_eq!(decimals(0.05), 2);
    }
}
