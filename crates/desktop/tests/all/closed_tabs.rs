//! Bringing back a tab that was closed.
//!
//! Out here rather than beside the code because every one of these goes
//! in through the public door — which makes them a check on that door as
//! well as on the behaviour behind it.

use std::path::PathBuf;

use guild_desktop::closed::*;
use guild_session::{AreaId, ProjectId, Runs, Seat, SessionId, SessionMode, WorkspaceId};

fn project() -> ProjectId {
    ProjectId::new()
}

fn workspace() -> WorkspaceId {
    WorkspaceId::new()
}

fn seat(workspace: WorkspaceId, project: ProjectId) -> Seat {
    Seat { workspace, project }
}

fn claude(project: ProjectId) -> ClosedTab {
    ClosedTab::Session {
        project,
        runs: Runs::Claude(SessionMode::Terminal),
        cwd: PathBuf::from("/work/guild"),
        session: Some(SessionId::new()),
    }
}

fn shell(project: ProjectId) -> ClosedTab {
    ClosedTab::Session {
        project,
        runs: Runs::Shell,
        cwd: PathBuf::from("/work/guild"),
        session: None,
    }
}

fn file(project: ProjectId, path: &str) -> ClosedTab {
    ClosedTab::File {
        project,
        path: PathBuf::from(path),
    }
}

/// The gesture is an undo: the last thing closed is the first thing
/// back.
#[test]
fn the_newest_closing_comes_back_first() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();

    let first = claude(project);
    let second = claude(project);
    closed.push(workspace, first.clone());
    closed.push(workspace, second.clone());

    assert_eq!(closed.take_last(seat(workspace, project)), Some(second));
    assert_eq!(closed.take_last(seat(workspace, project)), Some(first));
    assert_eq!(closed.take_last(seat(workspace, project)), None);
}

/// A shell has nothing to resume, and says so with `None`.
#[test]
fn a_shell_comes_back_as_a_shell() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    closed.push(workspace, shell(project));

    let back = closed
        .take_last(seat(workspace, project))
        .expect("remembered");
    assert!(matches!(
        back,
        ClosedTab::Session {
            runs: Runs::Shell,
            session: None,
            ..
        }
    ));
}

/// Each project gets its own closings back, not another project's.
#[test]
fn closings_are_per_project() {
    let (one, other) = (project(), project());
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    closed.push(workspace, claude(one));
    closed.push(workspace, claude(other));

    let back = closed
        .take_last(seat(workspace, one))
        .expect("this project's");
    assert_eq!(back.project(), one);
    assert_eq!(
        closed.take_last(seat(workspace, one)),
        None,
        "the other's stays"
    );
    assert!(closed.take_last(seat(workspace, other)).is_some());
}

/// Closed, reopened, closed again is one closing, not two.
#[test]
fn the_same_session_closed_twice_is_one_entry() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    let tab = claude(project);
    closed.push(workspace, tab.clone());
    closed.push(workspace, tab.clone());

    assert_eq!(closed.take_last(seat(workspace, project)), Some(tab));
    assert_eq!(closed.take_last(seat(workspace, project)), None);
}

/// A session that turns out to be unresumable is dropped from the list,
/// so the gesture does not offer a tab that cannot open.
#[test]
fn forgetting_a_session_removes_its_closing() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    let tab = claude(project);
    let id = tab.session().expect("a claude tab has one");
    closed.push(workspace, tab);

    closed.forget(id);
    assert_eq!(closed.take_last(seat(workspace, project)), None);
}

/// A closed file comes back as the same path; closed twice, it is one
/// entry, the way a session is.
#[test]
fn a_closed_file_comes_back_as_the_same_path_and_twice_is_once() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    closed.push(workspace, file(project, "crates/desktop/src/lib.rs"));
    closed.push(workspace, file(project, "crates/desktop/src/lib.rs"));

    let back = closed
        .take_last(seat(workspace, project))
        .expect("remembered");
    assert!(
        matches!(back, ClosedTab::File { path, .. } if path == std::path::Path::new("crates/desktop/src/lib.rs"))
    );
    assert_eq!(
        closed.take_last(seat(workspace, project)),
        None,
        "twice is once"
    );
}

/// The list does not grow without end: past the depth, the oldest goes.
#[test]
fn the_list_keeps_only_the_last_few() {
    let project = project();
    let workspace = workspace();
    let mut closed = ClosedTabs::default();
    for _ in 0..(DEPTH + 3) {
        closed.push(workspace, claude(project));
    }

    let mut count = 0;
    while closed.take_last(seat(workspace, project)).is_some() {
        count += 1;
    }
    assert_eq!(count, DEPTH);
}

/// Closing any agent's tab leaves its session to come back to; only a
/// shell leaves none. A Codex or OpenCode tab used to be taken for a
/// shell — "not Claude" — and ⌘⇧T, the Sessions panel and the start
/// screen could never bring its conversation back.
#[test]
fn closing_an_agent_leaves_its_session_and_a_shell_none() {
    use guild_session::SessionRecord;

    for runs in [
        Runs::Claude(SessionMode::Terminal),
        Runs::Codex(SessionMode::Terminal),
        Runs::OpenCode(SessionMode::Terminal),
    ] {
        let record = SessionRecord::new("t".into(), project(), "/work/guild".into(), runs);
        assert_eq!(
            ClosedTab::closing(&record).session(),
            Some(record.id),
            "{runs:?}"
        );
    }
    let shell = SessionRecord::new("t".into(), project(), "/work/guild".into(), Runs::Shell);
    assert_eq!(ClosedTab::closing(&shell).session(), None);
}

/// M12.8: a closing remembers the area it was closed from, so
/// `cmd-shift-t` puts the tab back where it was.
#[test]
fn a_closing_remembers_the_area_it_was_closed_from() {
    let (workspace, project, area) = (workspace(), project(), AreaId::new());
    let mut closed = ClosedTabs::default();
    closed.push_from(workspace, Some(area), claude(project));
    let (tab, from) = closed
        .take_last_from(seat(workspace, project))
        .expect("one closing");
    assert!(matches!(tab, ClosedTab::Session { .. }));
    assert_eq!(from, Some(area));
}

/// …and one pushed the old way comes back with none, so every caller that
/// has not learnt about areas still works.
#[test]
fn a_closing_pushed_without_an_area_comes_back_without_one() {
    let (workspace, project) = (workspace(), project());
    let mut closed = ClosedTabs::default();
    closed.push(workspace, shell(project));
    assert_eq!(
        closed
            .take_last_from(seat(workspace, project))
            .map(|(_, from)| from),
        Some(None)
    );
}
