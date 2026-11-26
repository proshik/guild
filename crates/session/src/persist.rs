//! Where the desk is written, and why it is written in two places.
//!
//! In memory a desk is one thing: projects and every session in them,
//! which is what the window addresses. On disk it is not, and the reason is
//! ownership rather than tidiness.
//!
//! A session belongs to a project, and a project is a directory. While every
//! project's sessions lived in one global file, that file was the place all
//! work accumulated — and closing a project had to throw its sessions away,
//! because there was nowhere else to put them. That is exactly the bug this
//! answers: a project reopened came back empty.
//!
//! So the global file keeps what is global — which directories are open,
//! which were recent, which one was last looked at — and each project keeps
//! its own sessions beside itself, in `<project>/.guild/`.

use std::path::Path;

use crate::desk::Desk;
use crate::project_state;
use crate::store::{Store, StoreError};

/// Read a desk from the global file and every project beside it.
///
/// **Migration is a side effect of reading.** A global file in the old
/// format still carries sessions; they are read from it and land in their
/// own projects on the next save, which happens on the first change. There
/// is no separate migration step to forget to run.
pub fn load(global: &Path) -> Desk {
    let mut desk: Desk = Store::new(global.to_path_buf()).load().unwrap_or_default();

    // Whatever an old file still holds. These are somebody's conversations,
    // so they are kept and re-homed rather than dropped.
    let carried = desk.take_sessions();
    if !carried.is_empty() {
        tracing::info!(
            sessions = carried.len(),
            "the global file still carries sessions; they move to their projects on the next save"
        );
    }

    let directories: Vec<_> = desk
        .projects()
        .map(|project| (project.id, project.path.clone()))
        .collect();

    for (id, path) in directories {
        let state = project_state::load(&path);
        let holders = desk.holders(id);
        let first = holders.first().copied();
        // A record whose project has been forgotten would be invisible and
        // unreachable, so the project it was found beside is the one it
        // belongs to.
        desk.absorb_sessions(state.sessions.into_iter().filter_map(|mut session| {
            session.project = id;
            if session.workspace.is_nil() {
                session.workspace = first?;
            }
            (holders.contains(&session.workspace)
                && !carried.iter().any(|kept| kept.id == session.id))
            .then_some(session)
        }));
    }

    desk.absorb_sessions(carried);
    desk
}

/// Write the global file and each project's own.
///
/// A project whose directory has gone keeps its sessions **in the global
/// file**. Losing somebody's conversations because a disk was not mounted
/// is not a migration, and the directory may well be back tomorrow.
pub fn save(desk: &Desk, global: &Path) -> Result<(), StoreError> {
    let mut global_state = desk.clone();
    let sessions = global_state.take_sessions();

    let mut homeless = Vec::new();
    for project in desk.projects() {
        let holders = desk.holders(project.id);
        if !project.path.is_dir() {
            homeless.extend(
                sessions
                    .iter()
                    .filter(|session| session.project == project.id)
                    .cloned(),
            );
            continue;
        }
        let mine: Vec<_> = sessions
            .iter()
            .filter(|session| session.project == project.id && holders.contains(&session.workspace))
            .cloned()
            .collect();

        let mut state = project_state::load(&project.path);
        // Only replace records owned by seats present in memory. Records of
        // a workspace which stopped holding this root stay on disk: adding
        // the seat again makes those conversations reachable again.
        state.sessions.retain(|session| {
            !holders.contains(&session.workspace)
                && !mine.iter().any(|replacement| replacement.id == session.id)
        });
        state.sessions.extend(mine);
        if let Err(err) = project_state::save(&project.path, &state) {
            // A file this run could not read was said once when it was
            // read; its sessions still go to the global file below.
            if matches!(err, StoreError::NotRead { .. }) {
                tracing::debug!(project = %project.path.display(), %err, "a project's state was not saved");
            } else {
                tracing::warn!(project = %project.path.display(), %err, "a project's state was not saved");
            }
            homeless.extend(
                sessions
                    .iter()
                    .filter(|session| session.project == project.id)
                    .cloned(),
            );
        }
    }

    // Anything belonging to no open project at all: the same rule again.
    let open: Vec<_> = desk.projects().map(|project| project.id).collect();
    homeless.extend(
        sessions
            .into_iter()
            .filter(|session| !open.contains(&session.project)),
    );

    global_state.absorb_sessions(homeless);
    // Queued, like each project's file: this runs on every change in the
    // window, and a synced write is not something to wait for there.
    Store::new(global.to_path_buf()).save_soon(&global_state)
}
