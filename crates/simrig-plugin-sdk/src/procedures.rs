//! Guided procedures defined by the plugin. The SDK keeps the running sessions, validates the
//! values the user sends against the current step, and applies the values a finished procedure
//! produces to its component.

use std::collections::{BTreeMap, HashMap};

use simrig_model::{ProcedureState, ProcedureTarget, Step, Value};

use crate::error::Result;
use crate::server::HostLink;

/// A running procedure, written by the plugin. It decides the steps; the app only renders them.
pub trait Procedure {
    /// The first step (or an immediate outcome).
    fn start(&mut self, host: &mut HostLink<'_>) -> Result<Outcome>;

    /// The user chose `action` on the current step; `values` are the step's fields, already
    /// validated (except for `Cancel` actions, which send the values as they are).
    fn submit(
        &mut self,
        action: &str,
        values: &BTreeMap<String, Value>,
        host: &mut HostLink<'_>,
    ) -> Result<Outcome>;

    /// Called periodically: return a new outcome when the procedure moved on by itself (a live
    /// value changed, an input was detected…).
    fn poll(&mut self) -> Option<Outcome> {
        None
    }

    /// The app abandoned the procedure.
    fn cancel(&mut self) {}
}

/// Where a procedure goes next.
pub enum Outcome {
    Step(Step),
    /// Done. `apply` holds values (standard item ids or `custom.<id>`) the SDK sets on the
    /// procedure's component, validated like any other change (e.g. a calibration result).
    Finished {
        message: Option<String>,
        apply: Vec<(String, Value)>,
    },
    Cancelled,
}

impl Outcome {
    pub fn finished(message: impl Into<String>) -> Self {
        Outcome::Finished {
            message: Some(message.into()),
            apply: Vec::new(),
        }
    }
}

pub(crate) struct Session {
    pub target: ProcedureTarget,
    pub procedure: Box<dyn Procedure>,
    /// The step currently shown, to validate what the user sends.
    pub step: Option<Step>,
}

#[derive(Default)]
pub(crate) struct Sessions {
    sessions: HashMap<String, Session>,
    next: u64,
}

impl Sessions {
    pub fn insert(&mut self, session: Session) -> String {
        self.next += 1;
        let id = format!("p{}", self.next);
        self.sessions.insert(id.clone(), session);
        id
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Session> {
        self.sessions.remove(id)
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.sessions.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Cancels the sessions whose target is gone.
    pub fn retain_targets(&mut self, alive: impl Fn(&ProcedureTarget) -> bool) {
        self.sessions.retain(|_, s| {
            let keep = alive(&s.target);
            if !keep {
                s.procedure.cancel();
            }
            keep
        });
    }
}

/// The protocol state of an outcome, and the values to apply.
pub(crate) fn split(outcome: Outcome) -> (ProcedureState, Vec<(String, Value)>) {
    match outcome {
        Outcome::Step(step) => (ProcedureState::Step(step), Vec::new()),
        Outcome::Finished { message, apply } => (ProcedureState::Finished { message }, apply),
        Outcome::Cancelled => (ProcedureState::Cancelled, Vec::new()),
    }
}
