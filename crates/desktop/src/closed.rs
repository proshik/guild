//! Tabs that were closed, newest first.
//!
//! Closing a tab and wanting it back a second later is ordinary: the wrong
//! one was closed, or the right one was closed too early. Every editor and
//! every browser answers that with one gesture, and without it the only way
//! back was the session list — which does not list shells at all, because a
//! closed shell is forgotten at once.
//!
//! The stack lives for the session and no longer. Reopening a tab from
//! yesterday is what the session list is for; this is for the tab that was
//! closed a moment ago.

use std::path::PathBuf;

use guild_session::{AreaId, ProjectId, Runs, Seat, SessionId, SessionRecord, WorkspaceId};

/// How many closings are remembered.
///
/// Enough to undo a run of mistaken closes, short enough that the stack
/// never becomes a second, worse session list.
pub const DEPTH: usize = 10;

/// A tab that was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedTab {
    /// A session tab, or a shell.
    Session {
        /// The project it belonged to.
        project: ProjectId,
        /// What it ran — an agent of some harness, or a shell.
        runs: Runs,
        /// The directory it ran in.
        cwd: PathBuf,
        /// The session to come back to, when there is one.
        ///
        /// `None` for a shell: it has no conversation and no identifier
        /// worth keeping, so reopening one means starting a new shell in
        /// the same directory. Pretending otherwise would promise a
        /// scrollback that is gone.
        session: Option<SessionId>,
    },
    /// A file tab, by its path relative to the project. It comes back as
    /// the same file, read again — an edit that was not saved is not
    /// here, because it was answered about when the tab closed.
    File {
        /// The project it belonged to.
        project: ProjectId,
        /// The file, relative to the project.
        path: PathBuf,
    },
}

impl ClosedTab {
    /// What closing the tab showing `record` leaves to come back to.
    ///
    /// **A shell is `Runs::Shell`, not "not Claude".** Every harness has a
    /// conversation to return to; reading the kind as Claude-or-shell,
    /// as this did before M14 brought OpenCode and Codex, forgot theirs
    /// on close.
    pub fn closing(record: &SessionRecord) -> Self {
        Self::Session {
            project: record.project,
            runs: record.runs,
            cwd: record.cwd.clone(),
            session: record.runs.is_agent().then_some(record.id),
        }
    }

    /// The project the tab belonged to.
    pub fn project(&self) -> ProjectId {
        match self {
            Self::Session { project, .. } | Self::File { project, .. } => *project,
        }
    }

    /// The session it would come back to, when it is one.
    pub fn session(&self) -> Option<SessionId> {
        match self {
            Self::Session { session, .. } => *session,
            Self::File { .. } => None,
        }
    }
}

/// The closings of this run, newest first, each with the area it was
/// closed from — M12.8: `cmd-shift-t` puts a tab back where it was.
#[derive(Debug, Default)]
pub struct ClosedTabs {
    tabs: Vec<(WorkspaceId, Option<AreaId>, ClosedTab)>,
}

impl ClosedTabs {
    /// Remember a closing, from no area in particular — the focused one
    /// takes it back.
    pub fn push(&mut self, workspace: WorkspaceId, tab: ClosedTab) {
        self.push_from(workspace, None, tab);
    }

    /// Remember a closing from `area`, the one it goes back to while its
    /// seat still has it (`Stage::reopening`).
    pub fn push_from(&mut self, workspace: WorkspaceId, area: Option<AreaId>, tab: ClosedTab) {
        // The same thing closed twice — closed, reopened, closed again —
        // is one entry, not two: the second would bring back a tab that
        // is already open. A session is the same by its id, a file by its
        // path in its project.
        match &tab {
            ClosedTab::Session {
                session: Some(id), ..
            } => {
                let id = *id;
                self.tabs
                    .retain(|(_, _, known)| known.session() != Some(id));
            }
            ClosedTab::File { project, path } => {
                let (project, path) = (*project, path.clone());
                self.tabs.retain(|(known_workspace, _, known)| {
                    *known_workspace != workspace || !matches!(known, ClosedTab::File { project: p, path: q } if *p == project && *q == path)
                });
            }
            ClosedTab::Session { session: None, .. } => {}
        }

        self.tabs.insert(0, (workspace, area, tab));
        self.tabs.truncate(DEPTH);
    }

    /// Take the most recent closing in this project.
    ///
    /// Scoped to the project because tabs belong to one: reopening a tab
    /// would otherwise sometimes move the person to another project, which
    /// is not what the gesture promises.
    pub fn take_last(&mut self, seat: Seat) -> Option<ClosedTab> {
        self.take_last_from(seat).map(|(tab, _)| tab)
    }

    /// [`ClosedTabs::take_last`], with the area it was closed from.
    pub fn take_last_from(&mut self, seat: Seat) -> Option<(ClosedTab, Option<AreaId>)> {
        let index = self.tabs.iter().position(|(workspace, _, tab)| {
            *workspace == seat.workspace && tab.project() == seat.project
        })?;
        let (_, area, tab) = self.tabs.remove(index);
        Some((tab, area))
    }

    /// Forget a session — it is open again, or gone for good.
    pub fn forget(&mut self, session: SessionId) {
        self.tabs
            .retain(|(_, _, tab)| tab.session() != Some(session));
    }

    /// Forget closings belonging to a seat which has left the workspace.
    pub fn forget_seat(&mut self, seat: Seat) {
        self.tabs.retain(|(workspace, _, tab)| {
            *workspace != seat.workspace || tab.project() != seat.project
        });
    }

    /// Carry the recent closings with a project moved to another workspace.
    pub fn move_seat(&mut self, from: Seat, to: Seat) {
        for (workspace, _, tab) in &mut self.tabs {
            if *workspace == from.workspace && tab.project() == from.project {
                *workspace = to.workspace;
            }
        }
    }
}

impl ClosedTabs {
    pub(crate) fn relocate(
        &mut self,
        project: ProjectId,
        from: &std::path::Path,
        to: &std::path::Path,
    ) {
        for (_, _, closed) in &mut self.tabs {
            if let ClosedTab::File {
                project: owner,
                path,
            } = closed
                && *owner == project
                && let Some(next) = crate::file_operations::remap(path, from, to)
            {
                *path = next;
            }
        }
    }
}
