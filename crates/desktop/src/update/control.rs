//! Selection and operation ownership shared by every asynchronous updater callback.
pub use guild_session::update::{Artifact, Identity, Origin};
use guild_session::update::{Preferences, Source};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
/// Revocable permission for short off-thread cache publication. Installation
/// still requires the runtime's exact ticket and candidate confirmation check.
pub struct Permit {
    alive: Arc<AtomicBool>,
    identity: Identity,
}
impl Permit {
    /// Whether this exact artifact still owns the publishing operation.
    pub fn pins(&self, identity: &Identity) -> bool {
        self.alive.load(Ordering::Acquire) && &self.identity == identity
    }
}
/// Work ownership is opaque to callers: clone the ticket into that work's callbacks.
#[derive(Debug, Clone)]
pub struct Ticket {
    selection: Uuid,
    operation: Uuid,
    preferences: Preferences,
}
impl Ticket {
    /// Whether two operations belong to the same acknowledged selection epoch.
    pub fn same_selection(&self, other: &Self) -> bool {
        self.selection == other.selection && self.preferences == other.preferences
    }
    /// The exact acknowledged settings this worker is allowed to use.
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
}
/// The window's single authority for accepting late results and confirmation continuations.
pub struct Gate {
    preferences: Preferences,
    repository: String,
    selection: Uuid,
    acknowledged: bool,
    operation: Option<Uuid>,
    selected: Option<Identity>,
    publication: Arc<AtomicBool>,
}
impl Gate {
    /// A displayed choice is unavailable for work until persistence/loading acknowledges it.
    pub fn pending(preferences: Preferences, repository: String) -> Self {
        Self {
            preferences,
            repository,
            selection: Uuid::new_v4(),
            acknowledged: false,
            operation: None,
            selected: None,
            publication: Arc::new(AtomicBool::new(true)),
        }
    }
    /// Current desired values, including those not durably saved yet.
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    /// Snapshot token for the serial preference writer's acknowledgement.
    pub fn selection(&self) -> Uuid {
        self.selection
    }
    /// Invalidate all operations and confirmations immediately, before asynchronous saving.
    pub fn change(&mut self, preferences: Preferences) -> Uuid {
        self.revoke_publication();
        self.preferences = preferences;
        self.selection = Uuid::new_v4();
        self.acknowledged = false;
        self.operation = None;
        self.selected = None;
        self.selection
    }
    /// Only the latest matching persistence result can activate a source.
    pub fn acknowledge(&mut self, selection: Uuid, preferences: &Preferences) -> bool {
        if selection != self.selection || preferences != &self.preferences {
            return false;
        }
        self.acknowledged = true;
        true
    }
    /// Begin a new operation; a retry also invalidates callbacks from the old operation.
    pub fn begin(&mut self) -> Option<Ticket> {
        if !self.acknowledged {
            return None;
        }
        self.revoke_publication();
        let operation = Uuid::new_v4();
        self.operation = Some(operation);
        Some(Ticket {
            selection: self.selection,
            operation,
            preferences: self.preferences.clone(),
        })
    }
    /// Cancel work immediately; owned workers receive cancellation separately and reap off-thread.
    pub fn cancel(&mut self) {
        self.revoke_publication();
        self.operation = None;
    }
    /// Whether work still belongs to the selected and durably acknowledged source.
    pub fn owns(&self, ticket: &Ticket) -> bool {
        self.acknowledged
            && ticket.selection == self.selection
            && Some(ticket.operation) == self.operation
            && ticket.preferences == self.preferences
    }
    /// Publish only a matching selected-source artifact.
    pub fn accept(&mut self, ticket: &Ticket, identity: Identity) -> bool {
        if !self.owns(ticket) {
            return false;
        }
        let matches = match &identity.origin {
            Origin::Local { directory, seal } => {
                let artifact_valid = match &identity.artifact {
                    Artifact::Build(id) => !id.is_nil(),
                    Artifact::LegacySeal(expected) => expected == seal,
                };
                self.preferences.source == Source::Local
                    && directory == &self.preferences.local_directory
                    && valid_sha(seal)
                    && artifact_valid
            }
            Origin::Remote {
                repository,
                channel,
                release_id,
                sha256,
            } => {
                self.preferences.source == Source::Remote
                    && repository == &self.repository
                    && channel == &self.preferences.channel
                    && *release_id > 0
                    && valid_sha(sha256)
                    && matches!(&identity.artifact, Artifact::Build(id) if !id.is_nil())
            }
        };
        if !matches {
            return false;
        }
        if self.selected.as_ref() != Some(&identity) {
            self.revoke_publication();
        }
        self.selected = Some(identity);
        true
    }
    fn revoke_publication(&mut self) {
        self.publication.store(false, Ordering::Release);
        self.publication = Arc::new(AtomicBool::new(true));
    }
    /// Export revocable authority without moving Gate or blocking the window on IO.
    pub fn permit(&self, ticket: &Ticket, identity: &Identity) -> Option<Permit> {
        self.pins(ticket, identity).then(|| Permit {
            alive: self.publication.clone(),
            identity: identity.clone(),
        })
    }
    /// Recheck the exact artifact after a confirmation or preparation callback.
    pub fn pins(&self, ticket: &Ticket, identity: &Identity) -> bool {
        self.owns(ticket) && self.selected.as_ref() == Some(identity)
    }
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.publication.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guild_session::update::Channel;
    use std::path::PathBuf;
    fn remote(channel: Channel) -> Identity {
        Identity {
            artifact: Artifact::Build(Uuid::new_v4()),
            origin: Origin::Remote {
                repository: "owner/guild".into(),
                channel,
                release_id: 1,
                sha256: "1".repeat(64),
            },
        }
    }
    #[test]
    fn source_changes_block_work_until_latest_save_and_reject_every_late_callback() {
        let original = Preferences::default();
        let mut gate = Gate::pending(original.clone(), "owner/guild".into());
        assert!(gate.begin().is_none());
        let initial = gate.selection();
        assert!(gate.acknowledge(initial, &original));
        let old = gate.begin().unwrap();
        let candidate = remote(Channel::Nightly);
        assert!(gate.accept(&old, candidate.clone()));
        assert!(gate.pins(&old, &candidate));
        let permit = gate.permit(&old, &candidate).unwrap();
        let local = Preferences {
            source: Source::Local,
            local_directory: Some(PathBuf::from("/tmp/готовые сборки")),
            ..original.clone()
        };
        let selection = gate.change(local.clone());
        assert!(!gate.owns(&old));
        assert!(!permit.pins(&candidate));
        assert!(!gate.pins(&old, &candidate));
        assert!(!gate.accept(&old, candidate.clone()));
        assert!(gate.begin().is_none());
        assert!(!gate.acknowledge(initial, &original));
        assert!(gate.begin().is_none());
        assert!(gate.acknowledge(selection, &local));
        let current = gate.begin().unwrap();
        assert!(!gate.accept(&current, candidate));
        let local_candidate = Identity {
            artifact: Artifact::Build(Uuid::new_v4()),
            origin: Origin::Local {
                directory: local.local_directory.clone(),
                seal: "2".repeat(64),
            },
        };
        assert!(gate.accept(&current, local_candidate.clone()));
        assert!(gate.pins(&current, &local_candidate));
    }
    #[test]
    fn channel_retry_cancel_and_confirmation_are_bound_to_exact_artifact() {
        let nightly = Preferences::default();
        let mut gate = Gate::pending(nightly.clone(), "owner/guild".into());
        gate.acknowledge(gate.selection(), &nightly);
        let old = gate.begin().unwrap();
        let candidate = remote(Channel::Nightly);
        assert!(gate.accept(&old, candidate.clone()));
        let retry = gate.begin().unwrap();
        assert!(!gate.owns(&old));
        assert!(!gate.pins(&old, &candidate));
        assert!(gate.owns(&retry));
        gate.cancel();
        assert!(!gate.owns(&retry));
        let stable = Preferences {
            channel: Channel::Stable,
            ..nightly
        };
        let selected = gate.change(stable.clone());
        gate.acknowledge(selected, &stable);
        let work = gate.begin().unwrap();
        assert!(!gate.accept(&work, candidate));
        let stable_candidate = remote(Channel::Stable);
        assert!(gate.accept(&work, stable_candidate.clone()));
        let confirmed = stable_candidate.clone();
        let replacement = remote(Channel::Stable);
        assert!(gate.accept(&work, replacement));
        assert!(!gate.pins(&work, &confirmed));
    }
}
