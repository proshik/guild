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
use crate::workspace::Seat;

/// Read a desk from the global file and every project beside it.
///
/// **Migration is a side effect of reading.** A global file in the old
/// format still carries sessions; they are read from it and land in their
/// own projects on the next save, which happens on the first change. There
/// is no separate migration step to forget to run.
pub fn load(global: &Path) -> Desk {
    let mut desk: Desk = Store::new(global.to_path_buf()).load().unwrap_or_default();
    // Seats the global file carries whole: read, wherever their folder is.
    let read_away = std::mem::take(&mut desk.read_away);

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
        // A folder away now answers no state, which is not the seat's
        // records: its seats stay unread until `read_unread` finds it —
        // all but those the global file carries whole (`read_away`).
        // And a file that is there and would not read is not a read either:
        // its records are not in memory, though no save will write over
        // them this run (`StoreError::NotRead`).
        let read_file = project_state::load_for_change(&path);
        let read_it = read_file.is_some();
        let state = read_file.unwrap_or_default();
        let holders = desk.holders(id);
        let whole: Vec<_> = holders
            .iter()
            .copied()
            .filter(|&workspace| read_away.contains(&(workspace, id)))
            .collect();
        let read = if read_it {
            holders.clone()
        } else {
            whole.clone()
        };
        desk.loaded.extend(read.into_iter().map(|workspace| Seat {
            workspace,
            project: id,
        }));
        let first = holders.first().copied();
        // A record whose project has been forgotten would be invisible and
        // unreachable, so the project it was found beside is the one it
        // belongs to.
        desk.absorb_sessions(state.sessions.into_iter().filter_map(|mut session| {
            session.project = id;
            if session.workspace.is_nil() {
                session.workspace = first?;
            }
            // A seat the global file carries whole has nothing to add
            // from here: a record here and not there was removed.
            (holders.contains(&session.workspace)
                && !whole.contains(&session.workspace)
                && !carried.iter().any(|kept| kept.id == session.id))
            .then_some(session)
        }));
    }

    desk.absorb_sessions(carried);
    desk
}

/// Read the records of every held seat this run has not read yet — GLD-060.
///
/// Two seats are held without having been read: one added since the desk
/// was read — a root opened again in a workspace that had closed it, whose
/// records stayed beside the root — and one whose folder was away when it
/// was read. Their records come into memory, where a restore looks for
/// them, as `load` would have brought them; a record memory already holds
/// is not read a second time, nor one this run removed. A folder still
/// away, or a file that will not read, leaves its seats unread: [`save`]
/// keeps their records on disk.
pub fn read_unread(desk: &mut Desk) {
    let waiting: Vec<_> = desk
        .projects()
        .filter_map(|project| {
            let seats: Vec<_> = desk
                .holders(project.id)
                .into_iter()
                .filter(|&workspace| {
                    !desk.loaded.contains(&Seat {
                        workspace,
                        project: project.id,
                    })
                })
                .collect();
            (!seats.is_empty()).then(|| (project.id, project.path.clone(), seats))
        })
        .collect();
    for (id, path, seats) in waiting {
        let Some(state) = project_state::load_for_change(&path) else {
            continue;
        };
        let first = desk.holders(id).first().copied();
        let read: Vec<_> = state
            .sessions
            .into_iter()
            .filter_map(|mut session| {
                session.project = id;
                if session.workspace.is_nil() {
                    session.workspace = first?;
                }
                (seats.contains(&session.workspace)
                    && desk.session(session.id).is_none()
                    && !desk.removed.contains(&session.id))
                .then_some(session)
            })
            .collect();
        if !read.is_empty() {
            tracing::info!(
                project = %path.display(),
                sessions = read.len(),
                "a seat's records were read after the desk was"
            );
        }
        desk.absorb_earlier(id, read);
        desk.loaded.extend(seats.into_iter().map(|workspace| Seat {
            workspace,
            project: id,
        }));
    }
}

/// Write the global file and each project's own.
///
/// A project whose directory has gone keeps its sessions **in the global
/// file**. Losing somebody's conversations because a disk was not mounted
/// is not a migration, and the directory may well be back tomorrow.
///
/// **A seat's records on disk are replaced only once this run has read
/// them** — GLD-060. Unread seats are read first ([`read_unread`]), which
/// is why the desk is taken mutably; one still unread keeps what its file
/// holds, and memory's records of it are added by id. Without that, a seat
/// memory never read was written as empty: the conversations a workspace
/// kept in a root it had closed, or in a folder away at startup, left the
/// file at the first save.
pub fn save(desk: &mut Desk, global: &Path) -> Result<(), StoreError> {
    read_unread(desk);
    let desk = &*desk;
    let mut global_state = desk.clone();
    let sessions = global_state.take_sessions();

    let mut homeless = Vec::new();
    global_state.read_away.clear();
    for project in desk.projects() {
        let holders = desk.holders(project.id);
        let read: Vec<_> = holders
            .iter()
            .copied()
            .filter(|&workspace| {
                desk.loaded.contains(&Seat {
                    workspace,
                    project: project.id,
                })
            })
            .collect();
        // Whenever a root's records go to the global file instead of its
        // own — its folder away, or its file refusing the write — what
        // this run read of it goes there whole, and is said to be: the
        // next run takes those seats from there, not from the older file.
        let whole = read.iter().map(|&workspace| (workspace, project.id));
        if !project.path.is_dir() {
            global_state.read_away.extend(whole);
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
        // Only replace records owned by seats this run has read. Records of
        // a workspace which stopped holding this root stay on disk: adding
        // the seat again makes those conversations reachable again. So do
        // those of a seat held but never read — except a record this run
        // removed, whose copy here is older than the removal.
        state.sessions.retain(|session| {
            !read.contains(&session.workspace)
                && !desk.removed.contains(&session.id)
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
            global_state.read_away.extend(whole);
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
