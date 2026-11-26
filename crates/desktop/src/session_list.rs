//! The column's Sessions mode, the status bar's counters and the sessions
//! popup ask one question — who needs the person first — and this is the
//! one place it is answered. Pure: what a session is doing and when it last
//! did something go in; sections, Settled, an order and counts come out.
//!
//! **Urgency, not the board's attention order.** The sections are what a
//! person reads top to bottom: what waits on them (a question, a permission,
//! a failure), what is working, what finished unread, what rests. Within a
//! section the newest activity comes first; a session nobody has timed goes
//! last and is never called old.

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use guild_session::status::Status;
use guild_session::{HarnessKind, SessionId};

use crate::agents::Agent;

/// A section of the Sessions mode, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Urgency {
    /// Waiting for the person, or failed: something only they can move.
    Waiting,
    /// In the middle of a turn.
    Working,
    /// A finished turn nobody has read.
    Done,
    /// Nothing going on.
    Idle,
}

impl Urgency {
    /// Every section, in drawing order.
    pub const ALL: [Self; 4] = [Self::Waiting, Self::Working, Self::Done, Self::Idle];
}

impl From<Urgency> for guild_session::settings::SessionSection {
    fn from(urgency: Urgency) -> Self {
        match urgency {
            Urgency::Waiting => Self::Waiting,
            Urgency::Working => Self::Working,
            Urgency::Done => Self::Done,
            Urgency::Idle => Self::Idle,
        }
    }
}

impl From<guild_session::settings::SessionSection> for Urgency {
    fn from(section: guild_session::settings::SessionSection) -> Self {
        use guild_session::settings::SessionSection;
        match section {
            SessionSection::Waiting => Self::Waiting,
            SessionSection::Working => Self::Working,
            SessionSection::Done => Self::Done,
            SessionSection::Idle => Self::Idle,
        }
    }
}

/// The section a status belongs to.
pub fn urgency(status: Status) -> Urgency {
    match status {
        Status::Waiting(_) | Status::Failed => Urgency::Waiting,
        Status::Working => Urgency::Working,
        Status::Done => Urgency::Done,
        Status::Idle => Urgency::Idle,
    }
}

/// What a row stands for: an open tab's session, or a conversation a
/// harness remembers that no tab holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A session open in a tab.
    Open(SessionId),
    /// A closed conversation, by its harness's own id.
    Closed {
        /// Whose history it is in.
        harness: HarnessKind,
        /// What the harness resumes it by.
        id: String,
    },
}

/// One session as the model needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Which one.
    pub key: Key,
    /// What runs it.
    pub agent: Agent,
    /// What it is doing; a closed one is idle.
    pub status: Status,
    /// Its project's name, which the filter goes by.
    pub project: String,
    /// When it last did something, if anything says.
    pub last: Option<SystemTime>,
}

/// What the Sessions mode draws.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The non-empty sections, in order, each newest first.
    pub sections: Vec<(Urgency, Vec<Key>)>,
    /// Open tabs at rest for longer than the threshold and conversations
    /// closed within it, newest first.
    pub settled: Vec<Key>,
}

/// The Sessions mode's list: `open` by urgency, with idle tabs older than
/// `settle_after` moved to Settled beside the `closed` conversations that
/// are newer than it. Closed ones older than the threshold are left out.
pub fn listing(
    open: Vec<Item>,
    closed: Vec<Item>,
    now: SystemTime,
    settle_after: Duration,
    filter: &Filter,
) -> Listing {
    let age = |item: &Item| item.last.and_then(|at| now.duration_since(at).ok());
    let mut sections: Vec<(Urgency, Vec<Item>)> = Urgency::ALL
        .into_iter()
        .map(|urgency| (urgency, Vec::new()))
        .collect();
    let mut settled = Vec::new();
    for item in open.into_iter().filter(|item| filter.admits(item)) {
        let urgency = urgency(item.status);
        if urgency == Urgency::Idle && age(&item).is_some_and(|age| age > settle_after) {
            settled.push(item);
        } else if let Some((_, rows)) = sections.iter_mut().find(|(u, _)| *u == urgency) {
            rows.push(item);
        }
    }
    settled.extend(
        closed
            .into_iter()
            .filter(|item| filter.admits(item))
            .filter(|item| age(item).is_some_and(|age| age <= settle_after)),
    );
    for (_, rows) in &mut sections {
        newest_first(rows);
    }
    newest_first(&mut settled);
    Listing {
        sections: sections
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(urgency, rows)| (urgency, keys(rows)))
            .collect(),
        settled: keys(settled),
    }
}

/// One line of the Sessions page's scrolled list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// An expanded section's heading, with how many rows it holds.
    Section(Urgency, usize),
    /// A session.
    Row(Key),
}

/// Settled, at the foot of the page under its hairline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledZone {
    /// How many rows it holds, open or not.
    pub count: usize,
    /// Whether it shows them.
    pub open: bool,
    /// Its rows while open, newest first; none while closed.
    pub rows: Vec<Key>,
}

/// What a Sessions page draws, by where it draws it: the expanded sections
/// in a list that scrolls, and at the column's foot — not scrolling — the
/// collapsed sections' headings stacked above Settled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    /// The expanded sections, each under its heading.
    pub list: Vec<Line>,
    /// The collapsed sections, in urgency order, with their counts.
    pub collapsed: Vec<(Urgency, usize)>,
    /// Settled, when it holds anything.
    pub settled: Option<SettledZone>,
}

impl Page {
    /// The rows a person can choose, in the order the arrows walk them:
    /// the list's, then an open Settled's.
    pub fn rows(&self) -> Vec<Key> {
        self.list
            .iter()
            .filter_map(|line| match line {
                Line::Row(key) => Some(key.clone()),
                Line::Section(..) => None,
            })
            .chain(
                self.settled
                    .iter()
                    .flat_map(|zone| zone.rows.iter().cloned()),
            )
            .collect()
    }
}

/// Lay `listing` out as `folds` says: a collapsed section leaves the list
/// for the stack, and Settled shows its rows only while it is open.
pub fn page(listing: Listing, folds: &guild_session::settings::SessionFolds) -> Page {
    let mut page = Page::default();
    for (urgency, rows) in listing.sections {
        if folds.collapsed.contains(&urgency.into()) {
            page.collapsed.push((urgency, rows.len()));
        } else {
            page.list.push(Line::Section(urgency, rows.len()));
            page.list.extend(rows.into_iter().map(Line::Row));
        }
    }
    if !listing.settled.is_empty() {
        page.settled = Some(SettledZone {
            count: listing.settled.len(),
            open: folds.settled_open,
            rows: if folds.settled_open {
                listing.settled
            } else {
                Vec::new()
            },
        });
    }
    page
}

/// The popup's one column: every open session by urgency, newest first
/// within each, nothing settled away.
pub fn popup_order(mut open: Vec<Item>) -> Vec<Key> {
    newest_first(&mut open);
    open.sort_by_key(|item| urgency(item.status));
    keys(open)
}

/// Newest first, the untimed last — stable, so equals keep their order.
fn newest_first(rows: &mut [Item]) {
    rows.sort_by_key(|item| std::cmp::Reverse(item.last));
}

fn keys(rows: Vec<Item>) -> Vec<Key> {
    rows.into_iter().map(|item| item.key).collect()
}

/// How many sessions are in each state the status bar counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Failed — the only red mark there is.
    pub failed: usize,
    /// Working.
    pub working: usize,
    /// Waiting for the person.
    pub waiting: usize,
    /// Finished unread.
    pub done: usize,
    /// Open and doing nothing — counted last and quietly, so a window
    /// whose agents have not reported yet still says how many it has.
    pub idle: usize,
}

impl Counts {
    /// No agent at all: the control says its name instead.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    fn add(&mut self, status: Status) {
        match status {
            Status::Failed => self.failed += 1,
            Status::Working => self.working += 1,
            Status::Waiting(_) => self.waiting += 1,
            Status::Done => self.done += 1,
            Status::Idle => self.idle += 1,
        }
    }
}

/// The counts of `statuses`.
pub fn counts(statuses: impl IntoIterator<Item = Status>) -> Counts {
    let mut counts = Counts::default();
    for status in statuses {
        counts.add(status);
    }
    counts
}

/// The counts per agent, in the agents' declared order, leaving out an
/// agent with nothing to count — the counter's hover text.
pub fn breakdown(sessions: impl IntoIterator<Item = (Agent, Status)>) -> Vec<(Agent, Counts)> {
    let mut by: Vec<(Agent, Counts)> = Vec::new();
    for (agent, status) in sessions {
        match by.iter_mut().find(|(a, _)| *a == agent) {
            Some((_, counts)) => counts.add(status),
            None => by.push((agent, counts([status]))),
        }
    }
    by.retain(|(_, counts)| !counts.is_empty());
    by.sort_by_key(|(agent, _)| *agent);
    by
}

/// What the heading's filter menu has unticked. Empty shows everything.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Filter {
    agents: BTreeSet<Agent>,
    states: BTreeSet<Urgency>,
    projects: BTreeSet<String>,
}

impl Filter {
    /// Whether nothing is hidden.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.states.is_empty() && self.projects.is_empty()
    }

    /// Whether `item` is shown.
    pub fn admits(&self, item: &Item) -> bool {
        !self.agents.contains(&item.agent)
            && !self.states.contains(&urgency(item.status))
            && !self.projects.contains(&item.project)
    }

    /// Whether `agent`'s sessions are hidden.
    pub fn excludes_agent(&self, agent: Agent) -> bool {
        self.agents.contains(&agent)
    }

    /// Whether a section is hidden.
    pub fn excludes_state(&self, state: Urgency) -> bool {
        self.states.contains(&state)
    }

    /// Whether a project's sessions are hidden.
    pub fn excludes_project(&self, project: &str) -> bool {
        self.projects.contains(project)
    }

    /// Hide `agent`'s sessions, or show them again.
    pub fn toggle_agent(&mut self, agent: Agent) {
        toggle(&mut self.agents, agent);
    }

    /// Hide a section, or show it again.
    pub fn toggle_state(&mut self, state: Urgency) {
        toggle(&mut self.states, state);
    }

    /// Hide a project's sessions, or show them again.
    pub fn toggle_project(&mut self, project: &str) {
        toggle(&mut self.projects, project.to_owned());
    }

    /// Show everything — the menu's Show All.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn toggle<T: Ord>(set: &mut BTreeSet<T>, value: T) {
    if !set.remove(&value) {
        set.insert(value);
    }
}
