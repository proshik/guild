use std::path::Path;

use crate::scratch::Scratch;
use guild_session::{
    DEFAULT_NAME, Desk, ProjectState, Refused, Released, Runs, SessionMode, SessionRecord,
    WorkspaceId,
};

#[test]
fn an_old_desk_is_one_workspace_called_default_holding_every_project() {
    let mut desk: Desk = serde_json::from_str(
        r#"{
        "projects": [
          {"id":"00000000-0000-0000-0000-000000000001","path":"/one","name":"one"},
          {"id":"00000000-0000-0000-0000-000000000002","path":"/two","name":"two"}
        ],
        "active_project":"00000000-0000-0000-0000-000000000002"
    }"#,
    )
    .unwrap();
    desk.normalise();
    let workspace = &desk.workspaces()[0];
    assert_eq!(workspace.name, DEFAULT_NAME);
    assert!(workspace.worktree_order.is_empty());
    assert_eq!(workspace.projects.len(), 2);
    assert_eq!(workspace.front, desk.active_project());
}

#[test]
fn opening_the_first_project_creates_its_default_workspace() {
    let mut desk = Desk::default();
    let project = desk.open_in(WorkspaceId::nil(), Path::new("/work/first"));

    assert_eq!(desk.workspaces().len(), 1);
    assert_eq!(desk.workspaces()[0].name, DEFAULT_NAME);
    assert_eq!(desk.workspaces()[0].projects, [project]);
    assert_eq!(desk.workspaces()[0].front, Some(project));
    assert_eq!(desk.active_workspace(), desk.workspaces()[0].id);
}

#[test]
fn one_root_can_have_two_holders_and_is_released_only_after_the_second_leaves() {
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Side", Some("coffee")).unwrap();
    let project = desk.open_in(first, Path::new("/work/guild"));
    assert_eq!(desk.open_in(second, Path::new("/work/guild")), project);
    assert_eq!(desk.projects().count(), 1);
    assert_eq!(desk.leave(first, project), Released::Kept);
    assert_eq!(desk.leave(second, project), Released::Released);
    assert_eq!(desk.projects().count(), 0);
}

#[test]
fn workspace_guards_and_front_are_kept() {
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    assert_eq!(desk.delete_workspace(first), Err(Refused::Last));
    assert_eq!(desk.add_workspace("  ", None), Err(Refused::BlankName));
    assert_eq!(
        desk.add_workspace("Side", Some("unicorn")),
        Err(Refused::UnknownIcon)
    );
    let side = desk.add_workspace("Side", None).unwrap();
    let project = desk.open_in(side, Path::new("/work/side"));
    assert!(desk.select_workspace(side));
    assert_eq!(desk.active_project(), Some(project));
}

#[test]
fn repositories_can_be_reordered_inside_one_workspace() {
    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let a = desk.open_in(workspace, Path::new("/work/a"));
    let b = desk.open_in(workspace, Path::new("/work/b"));
    let c = desk.open_in(workspace, Path::new("/work/c"));

    assert!(desk.reorder_projects(workspace, &[a], c));
    assert_eq!(
        desk.projects_of(workspace)
            .map(|project| project.id)
            .collect::<Vec<_>>(),
        [b, c, a]
    );
    assert!(desk.reorder_projects(workspace, &[a], b));
    assert_eq!(
        desk.projects_of(workspace)
            .map(|project| project.id)
            .collect::<Vec<_>>(),
        [a, b, c]
    );

    let a_worktree = desk.open_in(workspace, Path::new("/work/a-linked"));
    assert!(desk.reorder_projects(workspace, &[a, a_worktree], b));
    assert_eq!(
        desk.projects_of(workspace)
            .map(|project| project.id)
            .collect::<Vec<_>>(),
        [b, a, a_worktree, c],
        "a repository's worktrees move as one ordered group"
    );
}

#[test]
fn moving_a_project_moves_its_seat_and_sessions_to_another_workspace() {
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let side = desk.add_workspace("Side", None).unwrap();
    let project = desk.open_in(first, Path::new("/work/guild"));
    let session = SessionRecord::new(
        "work".to_owned(),
        (first, project),
        Path::new("/work/guild").to_path_buf(),
        Runs::Claude(SessionMode::Terminal),
    );
    let session_id = session.id;
    desk.add_session(session);

    assert!(desk.move_projects(first, side, &[project]));
    assert_eq!(desk.projects_of(first).count(), 0);
    assert_eq!(
        desk.projects_of(side)
            .map(|project| project.id)
            .collect::<Vec<_>>(),
        [project]
    );
    assert_eq!(desk.session(session_id).unwrap().workspace, side);
    assert!(!desk.move_projects(side, first, &[]));
}

#[test]
fn saving_keeps_a_record_owned_by_a_workspace_not_in_memory() {
    let root = Scratch::new("guild-m125-preserve-");
    let global = root.join("workspace.json");
    let project_path = root.join("project");
    std::fs::create_dir_all(&project_path).unwrap();

    let mut desk = Desk::default();
    desk.normalise();
    let visible = desk.active_workspace();
    let project = desk.open_in(visible, &project_path);
    let hidden = WorkspaceId::new();
    let hidden_record = SessionRecord::new(
        "hidden".to_owned(),
        (hidden, project),
        project_path.clone(),
        Runs::Claude(SessionMode::Terminal),
    );
    let hidden_id = hidden_record.id;
    let state = ProjectState {
        sessions: vec![hidden_record],
        ..Default::default()
    };
    guild_session::project_state::save(&project_path, &state).unwrap();
    guild_session::flush();

    guild_session::persist::save(&mut desk, &global).unwrap();
    guild_session::flush();
    guild_session::persist::save(&mut desk, &global).unwrap();
    guild_session::flush();

    let state = guild_session::project_state::load(&project_path);
    assert!(state.sessions.iter().any(|record| record.id == hidden_id));
}

#[test]
fn a_legacy_record_is_migrated_without_being_duplicated() {
    let root = Scratch::new("guild-m125-legacy-");
    let global = root.join("workspace.json");
    let project_path = root.join("project");
    std::fs::create_dir_all(&project_path).unwrap();

    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let project = desk.open_in(workspace, &project_path);
    guild_session::Store::new(global.clone())
        .save(&desk)
        .unwrap();
    let legacy = SessionRecord::new(
        "legacy".to_owned(),
        project,
        project_path.clone(),
        Runs::Claude(SessionMode::Terminal),
    );
    let legacy_id = legacy.id;
    guild_session::project_state::save(
        &project_path,
        &ProjectState {
            sessions: vec![legacy],
            ..Default::default()
        },
    )
    .unwrap();
    guild_session::flush();

    let mut loaded = guild_session::persist::load(&global);
    assert_eq!(loaded.session(legacy_id).unwrap().workspace, workspace);
    guild_session::persist::save(&mut loaded, &global).unwrap();
    guild_session::flush();
    let state = guild_session::project_state::load(&project_path);
    assert_eq!(
        state
            .sessions
            .iter()
            .filter(|record| record.id == legacy_id)
            .count(),
        1
    );
    assert_eq!(state.sessions[0].workspace, workspace);
}

#[test]
fn five_workspaces_refuse_deferred_and_repeated_creation_until_deletion() {
    let mut desk = Desk::default();
    desk.normalise();
    for n in 1..5 {
        desk.add_workspace(&format!("Workspace {n}"), None).unwrap();
    }
    let before = desk.workspaces().to_vec();
    // Requests may have been prepared when capacity still existed. Submission
    // is the boundary, and every caller holds the same mutable desk.
    for name in ["Deferred dialog", "CLI", "Racing request"] {
        assert_eq!(
            desk.add_workspace(name, None),
            Err(Refused::Capacity),
            "accepted {name} at capacity"
        );
        assert_eq!(desk.workspaces(), before);
    }
    assert_eq!(desk.add_workspace(" ", None), Err(Refused::BlankName));
    assert_eq!(
        desk.add_workspace("Invalid", Some("unicorn")),
        Err(Refused::UnknownIcon)
    );
    desk.delete_workspace(before[2].id).unwrap();
    let replacement = desk.add_workspace("Replacement", Some("coffee")).unwrap();
    assert_ne!(replacement, before[2].id);
    assert_eq!(desk.workspaces().len(), 5);
    assert_eq!(desk.add_workspace("Sixth", None), Err(Refused::Capacity));
}

#[test]
fn legacy_over_limit_workspaces_survive_save_edit_and_reopen() {
    let root = Scratch::new("guild-footer-legacy-");
    let global = root.join("workspace.json");
    let mut desk = Desk::default();
    desk.normalise();
    let seed = serde_json::to_value(&desk).unwrap();
    let mut state = seed.clone();
    state["workspaces"] = serde_json::Value::Array(
        (0..7)
            .map(|n| {
                let mut workspace = seed["workspaces"][0].clone();
                workspace["id"] = serde_json::json!(WorkspaceId::new());
                workspace["name"] = serde_json::json!(format!("Legacy {n}"));
                workspace["icon"] = serde_json::json!("coffee");
                workspace
            })
            .collect(),
    );
    desk = serde_json::from_value(state).unwrap();
    desk.normalise();
    let ids: Vec<_> = desk.workspaces().iter().map(|w| w.id).collect();
    assert_eq!(ids.len(), 7);
    assert_eq!(desk.add_workspace("Eighth", None), Err(Refused::Capacity));
    for (n, id) in ids.iter().enumerate() {
        assert!(desk.select_workspace(*id));
        assert!(desk.rename_workspace(*id, &format!("Renamed {n}")));
        assert!(desk.set_icon(*id, Some("sun")).unwrap());
    }
    let project_path = root.join("project");
    std::fs::create_dir_all(&project_path).unwrap();
    let mut state = ProjectState::default();
    state.panels.left.size = 191.0;
    let mut sessions = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let project = desk.open_in(*id, &project_path);
        let record = SessionRecord::new(
            format!("Saved {n}"),
            (*id, project),
            project_path.clone(),
            Runs::Claude(SessionMode::Terminal),
        );
        sessions.push((record.id, *id));
        desk.add_session(record);
        state.seats.insert(
            *id,
            guild_session::project_state::SeatState {
                expanded: vec![format!("folder-{n}").into()],
                ..Default::default()
            },
        );
    }
    guild_session::project_state::save(&project_path, &state).unwrap();
    guild_session::flush();
    guild_session::persist::save(&mut desk, &global).unwrap();
    guild_session::flush();
    let reopened = guild_session::persist::load(&global);
    for (session, workspace) in sessions {
        assert_eq!(reopened.session(session).unwrap().workspace, workspace);
    }
    let saved = guild_session::project_state::load(&project_path);
    assert_eq!(saved.seats, state.seats);
    assert_eq!(saved.panels.left.size, 191.0);
    assert_eq!(reopened.workspaces(), desk.workspaces());
    assert_eq!(reopened.active_workspace(), ids[6]);
}

#[test]
fn saved_worktree_paths_survive_without_opening_roots() {
    let mut desk = Desk::default();
    desk.normalise();
    let mut json = serde_json::to_value(&desk).unwrap();
    json["workspaces"][0]["worktree_order"] =
        serde_json::json!({"/repo/.git": ["/linked-b", "/linked-a"]});
    let restored: Desk = serde_json::from_value(json).unwrap();
    let saved = serde_json::to_value(&restored).unwrap();
    assert_eq!(
        saved["workspaces"][0]["worktree_order"]["/repo/.git"],
        serde_json::json!(["/linked-b", "/linked-a"])
    );
    assert_eq!(restored.projects().count(), 0);
    assert_eq!(restored.active_workspace(), desk.active_workspace());
}

#[test]
fn worktree_order_applies_only_present_paths_and_pins_primary() {
    use guild_session::worktree_order::apply;
    let paths = |xs: &[&str]| xs.iter().map(std::path::PathBuf::from).collect::<Vec<_>>();
    let current = paths(&["/a", "/primary", "/b", "/new"]);
    let saved = paths(&["/missing", "/b", "/primary", "/b", "/a"]);
    assert_eq!(
        apply(&current, &saved, Some(Path::new("/primary"))),
        paths(&["/primary", "/b", "/a", "/new"])
    );
    assert_eq!(apply(&current, &[], None), current);
}

#[test]
fn linked_moves_preserve_other_rows_and_reject_stale_primary_and_noop_drops() {
    use guild_session::worktree_order::{Insertion, moved};
    let paths = |xs: &[&str]| xs.iter().map(std::path::PathBuf::from).collect::<Vec<_>>();
    let current = paths(&["/primary", "/a", "/b", "/c"]);
    let primary = Some(Path::new("/primary"));
    assert_eq!(
        moved(
            &current,
            primary,
            Path::new("/c"),
            &Insertion::Before("/a".into())
        ),
        Some(paths(&["/primary", "/c", "/a", "/b"]))
    );
    assert_eq!(
        moved(
            &current,
            primary,
            Path::new("/a"),
            &Insertion::After("/c".into())
        ),
        Some(paths(&["/primary", "/b", "/c", "/a"]))
    );
    for (source, insertion) in [
        ("/primary", Insertion::After("/c".into())),
        ("/a", Insertion::Before("/primary".into())),
        ("/gone", Insertion::After("/c".into())),
        ("/a", Insertion::After("/gone".into())),
        ("/a", Insertion::Before("/a".into())),
        ("/a", Insertion::Before("/b".into())),
    ] {
        assert_eq!(
            moved(&current, primary, Path::new(source), &insertion),
            None,
            "{source}: {insertion:?}"
        );
    }
}

#[test]
fn worktree_orders_are_workspace_local_and_preserve_front_on_roundtrip() {
    let scratch = Scratch::new("worktree-order");
    let repo = scratch.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let repository = repo.join(".git");
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Other", None).unwrap();
    let root = desk.open_in(first, &repo);
    desk.open_in(second, &repo);
    desk.select_workspace(first);
    let before = (
        desk.active_workspace(),
        desk.active_project(),
        desk.workspace(first).unwrap().projects.clone(),
    );
    let order = vec!["/unopened-b".into(), "/unopened-a".into()];
    assert!(desk.set_worktree_order(first, &repository, order.clone()));
    assert!(!desk.set_worktree_order(first, &repository, order.clone()));
    assert!(!desk.set_worktree_order(WorkspaceId::new(), &repository, order.clone()));
    let global = scratch.join("workspace.json");
    guild_session::persist::save(&mut desk, &global).unwrap();
    let mut restored = guild_session::persist::load(&global);
    restored.normalise();
    assert_eq!(
        restored.workspace(first).unwrap().worktree_order[&repository],
        order
    );
    assert!(
        restored
            .workspace(second)
            .unwrap()
            .worktree_order
            .is_empty()
    );
    assert_eq!(
        (
            restored.active_workspace(),
            restored.active_project(),
            restored.workspace(first).unwrap().projects.clone()
        ),
        before
    );
    assert_eq!(restored.workspace(first).unwrap().front, Some(root));
    assert_eq!(restored.projects().count(), 1);
}

#[test]
fn pinned_worktrees_are_workspace_local_persistent_and_do_not_open_roots() {
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Other", None).unwrap();
    let path = Path::new("/repo-feature");
    let before = desk.projects().count();
    assert!(desk.toggle_worktree_pin(first, path));
    assert_eq!(desk.workspace(first).unwrap().pinned_worktrees, [path]);
    assert!(desk.workspace(second).unwrap().pinned_worktrees.is_empty());
    assert_eq!(desk.projects().count(), before);
    let mut reopened: Desk = serde_json::from_slice(&serde_json::to_vec(&desk).unwrap()).unwrap();
    reopened.normalise();
    assert_eq!(reopened.workspace(first).unwrap().pinned_worktrees, [path]);
    assert!(reopened.toggle_worktree_pin(first, path));
    assert!(
        reopened
            .workspace(first)
            .unwrap()
            .pinned_worktrees
            .is_empty()
    );
    assert!(!reopened.toggle_worktree_pin(WorkspaceId::nil(), path));
}

#[test]
fn a_worktree_s_tab_list_is_folded_until_expanded_per_workspace_and_persists() {
    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Other", None).unwrap();
    let path = Path::new("/repo-feature");
    // Collapsed by default, and a desk nobody touched writes nothing.
    assert!(desk.worktree_tabs_folded(first, path));
    let plain = serde_json::to_value(&desk).unwrap();
    assert!(
        plain["workspaces"][0]
            .get("unfolded_worktree_tabs")
            .is_none()
    );

    assert!(!desk.toggle_worktree_tabs(first, path));
    assert!(!desk.worktree_tabs_folded(first, path));
    assert!(desk.worktree_tabs_folded(second, path));

    let mut reopened: Desk = serde_json::from_slice(&serde_json::to_vec(&desk).unwrap()).unwrap();
    reopened.normalise();
    assert!(!reopened.worktree_tabs_folded(first, path));
    assert!(reopened.toggle_worktree_tabs(first, path));
    assert!(reopened.worktree_tabs_folded(first, path));
    let value = serde_json::to_value(&reopened).unwrap();
    assert!(
        value["workspaces"][0]
            .get("unfolded_worktree_tabs")
            .is_none()
    );
    assert!(!reopened.toggle_worktree_tabs(WorkspaceId::nil(), path));
}

/// State written while lists were expanded by default named only the
/// collapsed ones. Those stay collapsed, and the file still loads.
#[test]
fn a_tab_list_folded_before_the_default_changed_stays_folded() {
    let mut desk = Desk::default();
    desk.normalise();
    let mut value = serde_json::to_value(&desk).unwrap();
    value["workspaces"][0]["folded_worktree_tabs"] = serde_json::json!(["/repo-feature"]);
    let mut reopened: Desk = serde_json::from_value(value).unwrap();
    reopened.normalise();
    let workspace = reopened.active_workspace();
    assert!(reopened.worktree_tabs_folded(workspace, Path::new("/repo-feature")));
    assert!(reopened.worktree_tabs_folded(workspace, Path::new("/repo-other")));
}

/// The session ids a project's own file holds, read from the disk once
/// every queued write has landed — not the writer's pending snapshot,
/// which `project_state::load` would answer.
fn records_on_disk(project: &Path) -> Vec<guild_session::SessionId> {
    guild_session::flush();
    let bytes = std::fs::read(project.join(".guild").join("state.json")).unwrap();
    let state: ProjectState = serde_json::from_slice(&bytes).unwrap();
    state.sessions.iter().map(|record| record.id).collect()
}

fn shell(name: &str, seat: (WorkspaceId, guild_session::ProjectId), path: &Path) -> SessionRecord {
    SessionRecord::new(name.to_owned(), seat, path.to_path_buf(), Runs::Shell)
}

/// GLD-060, the second route: a folder away when the desk was read loads
/// no records, and a save once it is back must not take that emptiness
/// for what the seat holds.
#[test]
fn a_save_keeps_the_records_of_a_folder_that_was_away_when_the_desk_was_read() {
    let root = Scratch::new("guild-gld060-away-");
    let global = root.join("workspace.json");
    let path = root.join("project");
    std::fs::create_dir_all(&path).unwrap();

    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let project = desk.open_in(workspace, &path);
    let kept = desk.add_session(shell("kept", (workspace, project), &path));
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![kept]);

    // Guild starts while the volume is not mounted, and it is back
    // before the next save.
    let away = root.join("project.away");
    std::fs::rename(&path, &away).unwrap();
    let mut restarted = guild_session::persist::load(&global);
    std::fs::rename(&away, &path).unwrap();
    assert!(restarted.session(kept).is_none(), "nothing was read");

    let added = restarted.add_session(shell("added", (workspace, project), &path));
    guild_session::persist::save(&mut restarted, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![kept, added]);
    assert!(
        restarted.session(kept).is_some(),
        "and the record is in memory, for a restore to find"
    );
}

/// GLD-060, the first route: a project closed in a workspace keeps that
/// workspace's records on disk, and a later run that opens it there again
/// without reading them — `guild session new` behind the person's work —
/// must not save them away. Records the run did read still leave the file
/// when they leave memory.
#[test]
fn a_save_keeps_the_records_of_a_seat_opened_again_without_reading_them() {
    let root = Scratch::new("guild-gld060-reopened-");
    let global = root.join("workspace.json");
    let path = root.join("project");
    std::fs::create_dir_all(&path).unwrap();

    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Second", None).unwrap();
    let project = desk.open_in(second, &path);
    let one = desk.add_session(shell("one", (second, project), &path));
    let two = desk.add_session(shell("two", (second, project), &path));
    guild_session::persist::save(&mut desk, &global).unwrap();

    // Closed in the second workspace with `forget_sessions_on_close`
    // off: the root is released, its records stay beside it.
    assert_eq!(desk.leave(second, project), Released::Released);
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![one, two]);

    // A restart in the first workspace, and the folder opened in the
    // second again by a command that reads nothing.
    let mut restarted = guild_session::persist::load(&global);
    assert_eq!(restarted.active_workspace(), first);
    let project = restarted.open_in(second, &path);
    let added = restarted.add_session(shell("added", (second, project), &path));
    guild_session::persist::save(&mut restarted, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![one, two, added]);
    assert!(restarted.session(one).is_some() && restarted.session(two).is_some());

    // Read now: a record removed from memory leaves the file as before.
    restarted.remove_session(one);
    guild_session::persist::save(&mut restarted, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![two, added]);
}

/// GLD-060: the same, for a root another workspace still holds. Its records
/// in memory are the other workspace's, which is what the window's own
/// import (`adopt_project`) took for "already read"; the seat opened again
/// is read on its own.
#[test]
fn a_root_held_elsewhere_and_opened_again_keeps_the_seat_s_records() {
    let root = Scratch::new("guild-gld060-elsewhere-");
    let global = root.join("workspace.json");
    let path = root.join("project");
    std::fs::create_dir_all(&path).unwrap();
    // Which file comes first is not the point: a record kept on disk is
    // written before memory's.
    let sorted = |mut ids: Vec<guild_session::SessionId>| {
        ids.sort_by_key(ToString::to_string);
        ids
    };

    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Second", None).unwrap();
    let project = desk.open_in(first, &path);
    desk.open_in(second, &path);
    let theirs = desk.add_session(shell("theirs", (first, project), &path));
    let mine = desk.add_session(shell("mine", (second, project), &path));
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(desk.leave(second, project), Released::Kept);
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(sorted(records_on_disk(&path)), sorted(vec![theirs, mine]));

    let mut restarted = guild_session::persist::load(&global);
    assert!(restarted.session(mine).is_none());
    restarted.open_in(second, &path);
    let added = restarted.add_session(shell("added", (second, project), &path));
    guild_session::persist::save(&mut restarted, &global).unwrap();
    assert_eq!(
        sorted(records_on_disk(&path)),
        sorted(vec![theirs, mine, added])
    );
    assert!(restarted.session(mine).is_some());
}

/// GLD-060, review r1: a record the desk held from the global file while
/// its folder was away, removed before the folder is back, is not read
/// again from the copy its project's file still has.
#[test]
fn a_record_removed_while_its_folder_was_away_is_not_read_back() {
    let root = Scratch::new("guild-gld060-removed-away-");
    let global = root.join("workspace.json");
    let path = root.join("project");
    let away = root.join("project.away");
    std::fs::create_dir_all(&path).unwrap();

    let mut desk = Desk::default();
    desk.normalise();
    let workspace = desk.active_workspace();
    let project = desk.open_in(workspace, &path);
    let gone = desk.add_session(shell("gone", (workspace, project), &path));
    let kept = desk.add_session(shell("kept", (workspace, project), &path));
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(records_on_disk(&path), vec![gone, kept]);

    // The volume goes: the records go to the global file, and a restart
    // while it is still away reads them from there.
    std::fs::rename(&path, &away).unwrap();
    guild_session::persist::save(&mut desk, &global).unwrap();
    guild_session::flush();
    let mut restarted = guild_session::persist::load(&global);
    assert!(restarted.session(gone).is_some());
    restarted.remove_session(gone);
    guild_session::persist::save(&mut restarted, &global).unwrap();

    std::fs::rename(&away, &path).unwrap();
    guild_session::persist::save(&mut restarted, &global).unwrap();
    assert!(restarted.session(gone).is_none(), "not read back");
    assert_eq!(records_on_disk(&path), vec![kept]);
}

/// GLD-060, review r1: a root moved into a workspace that had closed it
/// lands on records that workspace kept beside it and this run never
/// read; the save after the move keeps them, and the moved ones too.
#[test]
fn a_root_moved_into_a_workspace_that_had_closed_it_keeps_that_workspace_s_records() {
    let root = Scratch::new("guild-gld060-moved-");
    let global = root.join("workspace.json");
    let path = root.join("project");
    std::fs::create_dir_all(&path).unwrap();
    let sorted = |mut ids: Vec<guild_session::SessionId>| {
        ids.sort_by_key(ToString::to_string);
        ids
    };

    let mut desk = Desk::default();
    desk.normalise();
    let first = desk.active_workspace();
    let second = desk.add_workspace("Second", None).unwrap();
    let project = desk.open_in(first, &path);
    desk.open_in(second, &path);
    let moved = desk.add_session(shell("moved", (first, project), &path));
    let retained = desk.add_session(shell("retained", (second, project), &path));
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(desk.leave(second, project), Released::Kept);
    guild_session::persist::save(&mut desk, &global).unwrap();

    assert!(desk.move_projects(first, second, &[project]));
    guild_session::persist::save(&mut desk, &global).unwrap();
    assert_eq!(
        sorted(records_on_disk(&path)),
        sorted(vec![moved, retained])
    );
    assert_eq!(desk.session(moved).unwrap().workspace, second);
    assert_eq!(desk.session(retained).unwrap().workspace, second);
}
