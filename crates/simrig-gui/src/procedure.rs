//! Guided procedures (calibration, mapping, device setup). The plugin defines every step; the
//! app renders them generically: fields, live values, tables, notices and actions.

use std::collections::BTreeMap;

use slint::{ComponentHandle, ModelRc, VecModel};

use simrig_host::Target;
use simrig_model::{ActionRole, Element, Level, ParamType, ProcedureState, Step, Value};

use crate::app::App;
use crate::backend::Command;
use crate::format;
use crate::ui::{self, Rig, RowKind, Screen, SettingRow, StepElement};

#[derive(Default)]
pub struct ProcedureScreen {
    session: Option<String>,
    target: Option<Target>,
    step: Option<Step>,
    values: BTreeMap<String, Value>,
    /// Where to go when it ends.
    return_to: Option<Screen>,
    /// Plugin that defines it, shown under the step.
    source: String,
    /// The procedure runs inside the add-device flow (step 3).
    pub in_flow: bool,
    finished: bool,
}

impl App {
    pub fn start_procedure(&mut self, target: Target, id: &str) {
        let screen = self.window().get_screen();
        let plugin = match &target {
            Target::Component(address) => address.split('/').next().unwrap_or_default(),
            Target::Device { plugin, .. } => plugin,
        }
        .to_string();
        self.procedure = ProcedureScreen {
            session: None,
            target: Some(target.clone()),
            step: None,
            values: BTreeMap::new(),
            return_to: Some(screen),
            source: self
                .snapshot
                .plugins
                .iter()
                .find(|p| p.id == plugin)
                .map(|p| p.name.clone())
                .unwrap_or(plugin),
            in_flow: screen == Screen::Add,
            finished: false,
        };
        let has_device = matches!(target, Target::Component(_));
        if let Target::Device { .. } = &target {
            // Nothing live to draw but what the steps preview.
            self.current = None;
        }
        if let Target::Component(address) = &target {
            self.current = Some(address.clone());
            let device = self.device(address);
            self.window().global::<Rig>().set_current(device);
        }
        self.window().global::<Rig>().set_procedure(ui::Procedure {
            busy: true,
            has_device,
            in_flow: self.procedure.in_flow,
            source: format!("Step defined by the {} plugin.", self.procedure.source).into(),
            ..Default::default()
        });
        self.send(Command::StartProcedure {
            target,
            procedure: id.to_string(),
        });
        self.go(Screen::Procedure);
    }

    pub fn procedure_update(&mut self, session: String, state: ProcedureState) {
        match &self.procedure.session {
            Some(current) if *current != session => return,
            None if self.procedure.target.is_none() => return,
            _ => {}
        }
        self.procedure.session = Some(session);
        match state {
            ProcedureState::Step(step) => {
                // Keep what the user typed when the plugin only refreshes the step.
                let same_step = self
                    .procedure
                    .step
                    .as_ref()
                    .is_some_and(|s| s.title == step.title);
                if !same_step {
                    self.procedure.values = step.defaults();
                }
                self.procedure.step = Some(step);
                self.show_step(None);
            }
            ProcedureState::Finished { message } => {
                let mut p = self.procedure_base();
                p.finished = true;
                p.title = "Done".into();
                let message = message.unwrap_or_else(|| "Finished.".into());
                p.message = message.clone().into();
                self.flows.done_message = message;
                self.window().global::<Rig>().set_procedure(p);
                self.procedure.step = None;
                self.procedure.finished = true;
                if self.procedure.in_flow {
                    // The add-device flow moves on by itself.
                    self.procedure_closed();
                }
            }
            ProcedureState::Cancelled => {
                self.procedure.step = None;
                self.procedure_closed();
            }
        }
    }

    pub fn procedure_failed(&mut self, error: String) {
        if self.procedure.step.is_some() {
            self.show_step(Some(error));
        } else {
            let mut p = self.procedure_base();
            p.finished = true;
            p.title = "It did not work".into();
            p.error = error.into();
            self.window().global::<Rig>().set_procedure(p);
        }
    }

    fn procedure_base(&self) -> ui::Procedure {
        let window = self.window();
        let old = window.global::<Rig>().get_procedure();
        ui::Procedure {
            session: self.procedure.session.clone().unwrap_or_default().into(),
            source: old.source,
            has_device: old.has_device,
            in_flow: self.procedure.in_flow,
            ..Default::default()
        }
    }

    fn show_step(&mut self, error: Option<String>) {
        let Some(step) = self.procedure.step.clone() else {
            return;
        };
        let mut p = self.procedure_base();
        if let Some(preview) = &step.preview {
            let name = self.procedure.source.clone();
            let device = self.preview_device(preview, &name);
            self.window().global::<Rig>().set_current(device);
            p.has_device = true;
        }
        p.title = step.title.clone().into();
        p.text = step.text.clone().unwrap_or_default().into();
        p.waiting = step.waiting;
        p.error = error.unwrap_or_default().into();
        if let Some([n, total]) = step.progress {
            p.progress_text = format!("{n} of {total}").into();
            p.progress = if total > 0 {
                n as f32 / total as f32
            } else {
                0.0
            };
        }
        let elements: Vec<StepElement> = step
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| self.element(i, e))
            .collect();
        p.elements = ModelRc::new(VecModel::from(elements));
        // Cancelling is always at the bottom left: the step's cancel action is that one. The
        // primary action goes last, at the bottom right.
        let mut ordered: Vec<&simrig_model::StepAction> = step
            .actions
            .iter()
            .filter(|a| a.role != ActionRole::Cancel)
            .collect();
        ordered.sort_by_key(|a| a.role == ActionRole::Primary);
        let actions: Vec<ui::StepAction> = ordered
            .into_iter()
            .map(|a| ui::StepAction {
                id: a.id.clone().into(),
                label: a.label.clone().into(),
                role: match a.role {
                    ActionRole::Primary => "primary",
                    ActionRole::Secondary => "secondary",
                    ActionRole::Cancel => "cancel",
                }
                .into(),
                confirm: a.confirm.clone().unwrap_or_default().into(),
            })
            .collect();
        p.actions = ModelRc::new(VecModel::from(actions));
        self.window().global::<Rig>().set_procedure(p);
    }

    fn element(&self, index: usize, element: &Element) -> StepElement {
        match element {
            Element::Field {
                id,
                label,
                ty,
                value,
                help,
            } => {
                let value = self.procedure.values.get(id).unwrap_or(value);
                StepElement {
                    element: "field".into(),
                    row: field_row(index, label, help.as_deref(), ty, value),
                    ..Default::default()
                }
            }
            Element::Value { label, value, unit } => {
                let (text, own_unit) = format::input(value, None);
                StepElement {
                    element: "value".into(),
                    label: label.clone().into(),
                    value_text: text.into(),
                    unit: unit.clone().unwrap_or(own_unit).into(),
                    ..Default::default()
                }
            }
            Element::Table { columns, rows } => StepElement {
                element: "table".into(),
                columns: ModelRc::new(VecModel::from(
                    columns.iter().map(Into::into).collect::<Vec<_>>(),
                )),
                cells: ModelRc::new(VecModel::from(
                    rows.iter()
                        .flat_map(|r| {
                            (0..columns.len()).map(|c| r.get(c).cloned().unwrap_or_default().into())
                        })
                        .collect::<Vec<_>>(),
                )),
                ..Default::default()
            },
            Element::Notice { level, text } => StepElement {
                element: "notice".into(),
                level: match level {
                    Level::Info => "info",
                    Level::Success => "success",
                    Level::Warning => "warning",
                    Level::Error => "error",
                }
                .into(),
                text: text.clone().into(),
                ..Default::default()
            },
        }
    }

    /// A field of the current step changed.
    fn step_field(&mut self, index: usize, make: impl FnOnce(&ParamType) -> Option<Value>) {
        let Some(step) = &self.procedure.step else {
            return;
        };
        let Some(Element::Field { id, ty, .. }) = step.elements.get(index) else {
            return;
        };
        if let Some(value) = make(ty) {
            self.procedure.values.insert(id.clone(), value);
            self.show_step(None);
        }
    }

    pub fn step_number(&mut self, index: usize, v: f32) {
        self.step_field(index, |ty| match ty {
            ParamType::Int { .. } => Some(Value::Int(v.round() as i64)),
            ParamType::Float { .. } => Some(Value::Float((v as f64 * 10_000.0).round() / 10_000.0)),
            _ => None,
        });
    }

    pub fn step_choice(&mut self, index: usize, choice: usize) {
        self.step_field(index, |ty| match ty {
            ParamType::Choice { options } => {
                options.get(choice).map(|o| Value::Choice(o.id.clone()))
            }
            ParamType::Int { min, step, .. } => {
                Some(Value::Int(min + choice as i64 * step.max(&1)))
            }
            _ => None,
        });
    }

    pub fn step_bool(&mut self, index: usize, on: bool) {
        self.step_field(index, |_| Some(Value::Bool(on)));
    }

    pub fn step_text(&mut self, index: usize, text: &str) {
        self.step_field(index, |ty| ty.parse(text).ok());
    }

    pub fn submit_step(&mut self, action: &str) {
        let Some(session) = self.procedure.session.clone() else {
            return;
        };
        let cancel = self
            .procedure
            .step
            .as_ref()
            .and_then(|s| s.actions.iter().find(|a| a.id == action))
            .is_some_and(|a| a.role == ActionRole::Cancel);
        if cancel {
            self.cancel_procedure();
            return;
        }
        if let Some(step) = &self.procedure.step
            && let Err(e) = step.validate(&self.procedure.values)
        {
            self.show_step(Some(e));
            return;
        }
        let window = self.window();
        let rig = window.global::<Rig>();
        let mut p = rig.get_procedure();
        p.busy = true;
        rig.set_procedure(p);
        self.send(Command::Submit {
            session,
            action: action.to_string(),
            values: self.procedure.values.clone(),
        });
    }

    pub fn cancel_procedure(&mut self) {
        match self.procedure.session.clone() {
            Some(session) => self.send(Command::Cancel { session }),
            None => self.procedure_closed(),
        }
    }

    /// Leaves the procedure screen, back where it was started.
    pub fn procedure_closed(&mut self) {
        let back = self.procedure.return_to.take().unwrap_or(Screen::Home);
        let in_flow = self.procedure.in_flow;
        self.procedure.session = None;
        self.procedure.target = None;
        self.procedure.step = None;
        if in_flow {
            let finished = self.procedure.finished;
            self.flow_procedure_done(finished);
            return;
        }
        match back {
            Screen::Component => {
                if let Some(address) = self.current.clone() {
                    self.open_component(&address);
                }
            }
            other => self.go(other),
        }
    }
}

/// A field of a step, drawn like a setting; `item` carries the element index.
fn field_row(
    index: usize,
    label: &str,
    help: Option<&str>,
    ty: &ParamType,
    value: &Value,
) -> SettingRow {
    let mut row = SettingRow {
        item: index.to_string().into(),
        label: label.into(),
        help: help.unwrap_or_default().into(),
        enabled: true,
        ..SettingRow::default()
    };
    match ty {
        ParamType::Float { min, max, step } => {
            row.kind = RowKind::Slider;
            row.value = value.as_f64().unwrap_or(*min) as f32;
            row.minimum = *min as f32;
            row.maximum = *max as f32;
            row.step = *step as f32;
            row.decimals = if *step >= 1.0 { 0 } else { 2 };
        }
        ParamType::Int { min, max, step } => {
            let count = (max - min) / (*step).max(1) + 1;
            let v = value.as_f64().unwrap_or(*min as f64) as i64;
            if count <= 6 {
                row.kind = RowKind::Segmented;
                row.options = ModelRc::new(VecModel::from(
                    (0..count)
                        .map(|i| (min + i * step).to_string().into())
                        .collect::<Vec<_>>(),
                ));
                row.selected = ((v - min) / (*step).max(1)) as i32;
            } else {
                row.kind = RowKind::Slider;
                row.value = v as f32;
                row.minimum = *min as f32;
                row.maximum = *max as f32;
                row.step = *step as f32;
            }
        }
        ParamType::Bool => {
            row.kind = RowKind::Toggle;
            row.checked = value.as_bool().unwrap_or(false);
        }
        ParamType::Choice { options } => {
            row.kind = RowKind::Segmented;
            let current = value.as_str().unwrap_or_default();
            row.selected = options
                .iter()
                .position(|o| o.id == current)
                .map(|i| i as i32)
                .unwrap_or(-1);
            row.options = ModelRc::new(VecModel::from(
                options
                    .iter()
                    .map(|o| o.label.clone().into())
                    .collect::<Vec<_>>(),
            ));
        }
        ParamType::Curve { .. } | ParamType::Text { .. } => {
            row.kind = RowKind::Text;
            row.text = value.to_text().into();
        }
    }
    row
}
