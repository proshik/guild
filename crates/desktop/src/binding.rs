//! Which session a tab holds when its program continues another's
//! conversation — M13.3, decision 52. Pure.
//!
//! **Provenance belongs to the conversation, not to the process.** A tab
//! whose program resumes a conversation Guild has a record of becomes that
//! record's tab, keeping what the record is — a task stays a task. Two
//! witnesses ask this table: a terminal's command line and a harness
//! tab's hooks; `root/tabs.rs` does what it answers.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use guild_session::{HarnessKind, SessionId};

/// A record as the table reads it.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub id: SessionId,
    pub harness: Option<HarnessKind>,
    pub harness_session: Option<&'a str>,
}

/// The record whose conversation this is — `start::resuming`'s rule: a
/// Claude conversation is the record whose id it is, the others' the
/// record whose `harness_session` it is, each by its own harness.
pub fn record_of(
    harness: HarnessKind,
    conversation: &str,
    records: &[Record<'_>],
) -> Option<SessionId> {
    records
        .iter()
        .filter(|record| record.harness == Some(harness))
        .find(|record| match harness {
            HarnessKind::Claude => record.id.to_string() == conversation,
            HarnessKind::OpenCode | HarnessKind::Codex => {
                record.harness_session == Some(conversation)
            }
        })
        .map(|record| record.id)
}

/// A record a tab's program was found continuing, and where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub record: SessionId,
    /// The record's workspace and root are the tab's.
    pub same_seat: bool,
    /// Another tab holds it, or it is open in a workspace not restored.
    pub running_elsewhere: bool,
}

/// What to do with the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    /// It becomes this record's tab.
    To(SessionId),
    /// Tell the person this conversation is open elsewhere; bind nothing.
    Elsewhere(SessionId),
    /// Tell the person it belongs to another root; bind nothing.
    OtherSeat(SessionId),
    /// Nothing to do.
    Nothing,
}

/// The table — decision 52. Two processes on one conversation is the
/// worst outcome the window knows, so *held elsewhere* is said first.
pub fn binding(current: SessionId, found: Option<Found>) -> Bind {
    match found {
        None => Bind::Nothing,
        Some(found) if found.record == current => Bind::Nothing,
        Some(found) if found.running_elsewhere => Bind::Elsewhere(found.record),
        Some(found) if !found.same_seat => Bind::OtherSeat(found.record),
        Some(found) => Bind::To(found.record),
    }
}

/// The record a process now means: what it was started as, through the
/// rebinds its tab has had. Its `GUILD_SESSION` never changes, so this is
/// how a task resumed in a person's tab is still a task to decision 35.
pub fn resolve(id: SessionId, started_as: &HashMap<SessionId, SessionId>) -> SessionId {
    started_as.get(&id).copied().unwrap_or(id)
}

/// The session a hook's `guild_session` names, when it names one.
pub fn launched_as(guild_session: Option<&str>) -> Option<SessionId> {
    guild_session?.parse().ok()
}

/// Which tabs have been told about which conversation, so a probe that
/// finds the same one every two seconds says so once.
#[derive(Debug)]
pub struct Told<K>(HashSet<(K, SessionId)>);

impl<K> Default for Told<K> {
    fn default() -> Self {
        Self(HashSet::new())
    }
}

impl<K: Eq + Hash> Told<K> {
    /// Whether this is the first time — and remember it.
    pub fn first(&mut self, key: K, id: SessionId) -> bool {
        self.0.insert((key, id))
    }

    /// Forget a tab that has closed.
    pub fn forget(&mut self, key: &K) {
        self.0.retain(|(held, _)| held != key);
    }
}
