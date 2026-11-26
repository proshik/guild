//! Serial acknowledged preference IO. The host executes one Work off-thread and
//! returns its Answer; selection invalidation itself never waits for filesystem IO.
use super::control::Gate;
use guild_session::update::{PreferenceState, PreferenceStore, Preferences};
use uuid::Uuid;

/// Persistence status, separate from download/install state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Initial load; no defaults are activated before its acknowledgement.
    Loading,
    /// The desired choice is waiting for durable acknowledgement.
    Saving,
    /// A recovery decision is being durably resolved; ordinary edits stay blocked.
    Resolving,
    /// Current selection is durably acknowledged.
    Ready,
    /// IO failed; the current source remains unavailable until explicit retry.
    Failed,
    /// A write may have reached disk; only an explicit identity-pinned decision resolves it.
    Recovery(PreferenceState),
}
#[derive(Debug, Clone)]
enum Action {
    Load,
    Save(Preferences),
    Resolve {
        operation_id: Uuid,
        keep_requested: bool,
    },
}
/// Owned IO request; execute exactly once on the host's background executor.
pub struct Work {
    id: Uuid,
    selection: Uuid,
    action: Action,
}
/// Opaque reply, including read-back after a failed save; stale replies cannot reactivate a source.
pub struct Answer {
    id: Uuid,
    selection: Uuid,
    action: Action,
    result: Result<PreferenceState, Option<PreferenceState>>,
}
impl Work {
    /// Perform blocking persistence away from the window thread. Failed save is
    /// followed by read-back to distinguish an unsettled intent from an old value.
    pub fn execute(self, store: &PreferenceStore) -> Answer {
        let result = match &self.action {
            Action::Load => store.load(),
            Action::Save(value) => store.save(value),
            Action::Resolve {
                operation_id,
                keep_requested,
            } => store.resolve(*operation_id, *keep_requested),
        }
        .map_err(|_| store.load().ok());
        Answer {
            id: self.id,
            selection: self.selection,
            action: self.action,
            result,
        }
    }
}
/// One in-flight writer and one coalesced desired choice. Own this with Gate in
/// the runtime; keep all work handles alive through their completion and quit.
pub struct Selection {
    gate: Gate,
    status: Status,
    next: Option<Action>,
    flight: Option<Uuid>,
    retry: Option<Action>,
}
impl Selection {
    /// Initial load is always required, including when no preferences file exists.
    pub fn new(repository: String) -> Self {
        Self {
            gate: Gate::pending(Preferences::default(), repository),
            status: Status::Loading,
            next: Some(Action::Load),
            flight: None,
            retry: None,
        }
    }
    /// Selection authority used by discovery, download and confirmation callbacks.
    pub fn gate(&self) -> &Gate {
        &self.gate
    }
    /// Runtime operations use the same authority as persistence acknowledgements.
    pub fn gate_mut(&mut self) -> &mut Gate {
        &mut self.gate
    }
    /// Localized UI derives save/recovery controls from this status.
    pub fn status(&self) -> &Status {
        &self.status
    }
    /// Invalidate old work synchronously, then queue the latest desired choice.
    pub fn change(&mut self, value: Preferences) -> bool {
        if !matches!(self.status, Status::Ready | Status::Saving)
            || &value == self.gate.preferences()
        {
            return false;
        }
        self.gate.change(value.clone());
        self.next = Some(Action::Save(value));
        self.status = Status::Saving;
        true
    }
    /// Take one request; another cannot start until its matching Answer arrives.
    pub fn take_work(&mut self) -> Option<Work> {
        if self.flight.is_some() {
            return None;
        }
        let action = self.next.take()?;
        let id = Uuid::new_v4();
        self.flight = Some(id);
        Some(Work {
            id,
            selection: self.gate.selection(),
            action,
        })
    }
    /// Apply an IO completion without ever acknowledging a superseded choice.
    pub fn complete(&mut self, answer: Answer) -> bool {
        if self.flight != Some(answer.id) {
            return false;
        }
        self.flight = None;
        match answer.result {
            Ok(state @ PreferenceState::Recovery { .. })
            | Err(Some(state @ PreferenceState::Recovery { .. })) => {
                // A stale reply can still describe a real durable intent. Stop
                // all saves, discard optimistic queued choices, and expose it.
                self.next = None;
                self.retry = None;
                self.gate.change(state.displayed().clone());
                self.status = Status::Recovery(state);
            }
            Err(_) => {
                self.retry = Some(match answer.action {
                    Action::Load | Action::Resolve { .. } => Action::Load,
                    Action::Save(_) => self
                        .next
                        .take()
                        .unwrap_or_else(|| Action::Save(self.gate.preferences().clone())),
                });
                self.next = None;
                self.gate.change(self.gate.preferences().clone());
                self.status = Status::Failed;
            }
            Ok(PreferenceState::Ready(value)) => {
                self.retry = None;
                if self.next.is_some() {
                    // An older successful write grants no authority while the
                    // latest desired choice still waits for persistence.
                    self.status = Status::Saving;
                } else {
                    match answer.action {
                        Action::Load | Action::Resolve { .. } => {
                            self.gate.change(value.clone());
                        }
                        Action::Save(_) => {
                            if answer.selection != self.gate.selection()
                                || &value != self.gate.preferences()
                            {
                                self.retry = Some(Action::Load);
                                self.gate.change(self.gate.preferences().clone());
                                self.status = Status::Failed;
                                return true;
                            }
                        }
                    }
                    self.gate.acknowledge(self.gate.selection(), &value);
                    self.status = Status::Ready;
                }
            }
        }
        true
    }
    /// Retry only after an observed failure; no automatic save retry.
    pub fn retry(&mut self) -> bool {
        if self.status != Status::Failed || self.flight.is_some() {
            return false;
        }
        let Some(action) = self.retry.take() else {
            return false;
        };
        self.gate.change(self.gate.preferences().clone());
        self.status = if matches!(action, Action::Load) {
            Status::Loading
        } else {
            Status::Saving
        };
        self.next = Some(action);
        true
    }
    /// Resolve exactly the displayed interrupted operation.
    pub fn resolve(&mut self, operation_id: Uuid, keep_requested: bool) -> bool {
        let Status::Recovery(PreferenceState::Recovery {
            operation_id: expected,
            ..
        }) = &self.status
        else {
            return false;
        };
        if operation_id != *expected || self.flight.is_some() {
            return false;
        }
        self.gate.change(self.gate.preferences().clone());
        self.next = Some(Action::Resolve {
            operation_id,
            keep_requested,
        });
        self.status = Status::Resolving;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guild_session::update::{Channel, Source};
    fn initialized() -> (tempfile::TempDir, PreferenceStore, Selection) {
        let root = tempfile::tempdir().unwrap();
        let store = PreferenceStore::new(root.path().to_path_buf());
        let mut selection = Selection::new("owner/guild".into());
        assert!(selection.gate_mut().begin().is_none());
        let answer = selection.take_work().unwrap().execute(&store);
        assert!(selection.complete(answer));
        assert_eq!(selection.status(), &Status::Ready);
        (root, store, selection)
    }
    #[test]
    fn serial_save_coalesces_latest_choice_without_acknowledging_an_older_write() {
        let (_root, store, mut selection) = initialized();
        let old_ticket = selection.gate_mut().begin().unwrap();
        let local = Preferences {
            source: Source::Local,
            local_directory: Some("/tmp/готовые сборки".into()),
            ..Preferences::default()
        };
        assert!(selection.change(local.clone()));
        assert!(!selection.gate().owns(&old_ticket));
        let first = selection.take_work().unwrap();
        let stable = Preferences {
            source: Source::Remote,
            channel: Channel::Stable,
            ..local
        };
        selection.change(Preferences::default());
        selection.change(stable.clone());
        assert!(selection.take_work().is_none());
        assert!(selection.complete(first.execute(&store)));
        assert!(selection.gate_mut().begin().is_none());
        assert_eq!(selection.gate().preferences(), &stable);
        let last = selection.take_work().unwrap();
        assert!(selection.complete(last.execute(&store)));
        assert!(selection.take_work().is_none());
        assert!(selection.gate_mut().begin().is_some());
        assert_eq!(store.load().unwrap(), PreferenceState::Ready(stable));
    }
    #[test]
    fn unreadable_future_settings_never_activate_defaults_or_get_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("update-preferences.json");
        let bytes = br#"{"schema":99,"operation_id":"00000000-0000-0000-0000-000000000000","source":"future","channel":"stable","local_directory":null}"#;
        std::fs::write(&path, bytes).unwrap();
        let store = PreferenceStore::new(root.path().to_path_buf());
        let mut selection = Selection::new("owner/guild".into());
        let answer = selection.take_work().unwrap().execute(&store);
        selection.complete(answer);
        assert_eq!(selection.status(), &Status::Failed);
        assert!(selection.gate_mut().begin().is_none());
        assert!(!selection.change(Preferences::default()));
        assert!(selection.retry());
        let answer = selection.take_work().unwrap().execute(&store);
        selection.complete(answer);
        assert_eq!(selection.status(), &Status::Failed);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn recovery_blocks_queued_choices_and_requires_the_exact_explicit_operation() {
        let (_root, _store, mut selection) = initialized();
        let local = Preferences {
            source: Source::Local,
            ..Preferences::default()
        };
        selection.change(local.clone());
        let work = selection.take_work().unwrap();
        selection.change(Preferences {
            channel: Channel::Stable,
            ..Preferences::default()
        });
        let operation_id = Uuid::new_v4();
        let recovery = PreferenceState::Recovery {
            operation_id,
            old: Preferences::default(),
            requested: local.clone(),
            published: Some(local.clone()),
        };
        let answer = Answer {
            id: work.id,
            selection: work.selection,
            action: work.action,
            result: Err(Some(recovery.clone())),
        };
        assert!(selection.complete(answer));
        assert_eq!(selection.status(), &Status::Recovery(recovery));
        assert_eq!(selection.gate().preferences(), &local);
        assert!(selection.gate_mut().begin().is_none());
        assert!(selection.take_work().is_none());
        assert!(!selection.retry());
        assert!(!selection.resolve(Uuid::new_v4(), true));
        assert!(!selection.change(Preferences::default()));
        assert!(selection.resolve(operation_id, false));
        assert!(
            !selection.change(Preferences {
                channel: Channel::Stable,
                ..Preferences::default()
            }),
            "recovery resolution cannot be overwritten by an ordinary queued save"
        );
        let resolving = selection.take_work().unwrap();
        assert!(
            matches!(resolving.action, Action::Resolve { operation_id: id, keep_requested: false } if id == operation_id)
        );
        let answer = Answer {
            id: resolving.id,
            selection: resolving.selection,
            action: resolving.action,
            result: Ok(PreferenceState::Ready(Preferences::default())),
        };
        assert!(selection.complete(answer));
        assert_eq!(selection.status(), &Status::Ready);
        assert!(selection.gate_mut().begin().is_some());
    }
}
