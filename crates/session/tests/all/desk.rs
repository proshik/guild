//! Projects and the sessions inside them.
//!
//! Sessions used to lie in one flat list, with the directory taken from
//! wherever the application was started. Work is not arranged that way:
//! there are several projects, each with its own sessions — as in the
//! environments people already know.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_session::{Desk, Runs, SessionMode, SessionRecord, Store, Usage};

fn temp_dir(name: &str) -> Scratch {
    Scratch::absent(&format!("guild-ws-{name}-"))
}

#[test]
fn a_project_is_named_after_its_directory() {
    let mut workspace = Desk::default();

    let id = workspace.add_project(Path::new("/work/guild"));

    assert_eq!(workspace.project(id).unwrap().name, "guild");
    assert_eq!(
        workspace.project(id).unwrap().path,
        Path::new("/work/guild")
    );
}

/// The same directory means the same project. Otherwise adding it again
/// would breed twins, and a person would not know which one holds their
/// sessions.
#[test]
fn adding_the_same_directory_twice_returns_the_same_project() {
    let mut workspace = Desk::default();

    let first = workspace.add_project(Path::new("/work/guild"));
    let second = workspace.add_project(Path::new("/work/guild"));

    assert_eq!(first, second);
    assert_eq!(workspace.projects().count(), 1);
}

/// The model's key property: sessions do not leak between projects.
#[test]
fn sessions_belong_to_their_project() {
    let mut workspace = Desk::default();
    let first = workspace.add_project(Path::new("/work/first"));
    let second = workspace.add_project(Path::new("/work/second"));

    workspace.add_session(SessionRecord::new(
        "fixing the config".to_owned(),
        first,
        PathBuf::from("/work/first"),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.add_session(SessionRecord::new(
        "writing a test".to_owned(),
        second,
        PathBuf::from("/work/second"),
        Runs::Claude(SessionMode::Terminal),
    ));

    let titles: Vec<&str> = workspace
        .sessions_of(first)
        .map(|record| record.title.as_str())
        .collect();

    assert_eq!(titles, vec!["fixing the config"]);
    assert_eq!(workspace.sessions_of(second).count(), 1);
}

/// A session runs in the directory of **its own** project. While the
/// directory came from `current_dir()`, every session ran in the same
/// place regardless of what it was working on.
#[test]
fn a_session_runs_in_the_directory_of_its_project() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));

    let cwd = workspace.project(project).unwrap().path.clone();
    let id = workspace.add_session(SessionRecord::new(
        "a task".to_owned(),
        project,
        cwd,
        Runs::Claude(SessionMode::Terminal),
    ));

    assert_eq!(workspace.session(id).unwrap().cwd, Path::new("/work/guild"));
}

/// Forgetting a project means forgetting its sessions too: there is no
/// point keeping sessions without a project, they are unreachable.
#[test]
fn forgetting_a_project_forgets_its_sessions() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let session = workspace.add_session(SessionRecord::new(
        "a task".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));

    workspace.remove_project(project);

    assert!(workspace.project(project).is_none());
    assert!(
        workspace.session(session).is_none(),
        "a session without a project is unreachable, no point keeping it"
    );
}

#[test]
fn state_survives_a_restart() {
    let dir = temp_dir("restart");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let session = workspace.add_session(SessionRecord::new(
        "fixing the config".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.mark_turn(session);
    workspace.record_usage(
        session,
        Usage {
            cost_usd: Some(0.25),
            ..Usage::default()
        },
        0,
    );
    store.save(&workspace).unwrap();

    let restored = store.load().unwrap();

    assert_eq!(restored.projects().count(), 1);
    let record = restored
        .session(session)
        .expect("the session survived the restart");
    assert_eq!(record.title, "fixing the config");
    assert!(record.had_turn);
    assert!((record.usage.cost_usd.unwrap() - 0.25).abs() < 1e-9);
}

/// M12: the repository a root belongs to is remembered, so the column is
/// drawn grouped from the first frame rather than a second later.
#[test]
fn a_project_s_repository_survives_a_restart() {
    let dir = temp_dir("repository");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild-lazy-lsp"));
    assert!(workspace.set_repository(project, Some(PathBuf::from("/work/guild/.git"))));
    assert!(
        !workspace.set_repository(project, Some(PathBuf::from("/work/guild/.git"))),
        "the same answer again is not a change"
    );
    store.save(&workspace).unwrap();

    let restored = store.load().unwrap();
    assert_eq!(
        restored.project(project).unwrap().repository.as_deref(),
        Some(Path::new("/work/guild/.git"))
    );
}

/// A project saved before M12 has no `repository`, and reads as one git
/// has not been asked about yet. The shape is a real file's, from disk:
/// `id`, `name` and `path`, nothing else.
#[test]
fn a_project_saved_before_repositories_existed_has_none() {
    let dir = temp_dir("pre-m12");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("workspace.json");
    std::fs::write(
        &file,
        r#"{
          "projects": [
            {"id": "4f6a1c2e-8b3d-4e5f-9a7b-1c2d3e4f5a6b", "name": "guild", "path": "/work/guild"}
          ],
          "sessions": [],
          "recent": ["/work/guild"],
          "active_project": null,
          "active_session": null
        }"#,
    )
    .unwrap();

    let restored = Store::<Desk>::new(file).load().unwrap();

    let project = restored
        .projects()
        .next()
        .expect("the project is read, not dropped");
    assert_eq!(project.name, "guild");
    assert_eq!(project.repository, None);
}

/// **Migrating old state.** Before projects the file kept a flat list of
/// sessions, each with its own directory. Reading it and silently
/// discarding it would mean losing a person's sessions — the very thing
/// `--resume` applies to.
///
/// So a session without a project gets one from its own directory.
#[test]
fn sessions_saved_before_projects_existed_are_not_lost() {
    let dir = temp_dir("migration");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");

    // A file in the old format: the list is called `records`, with no
    // mention of projects. The shape was taken from a real
    // ~/.config/guild/sessions.json rather than invented.
    std::fs::write(
        &path,
        r#"{
          "records": [
            {
              "id": "11111111-1111-4111-8111-111111111111",
              "title": "an old session",
              "cwd": "/work/guild",
              "mode": "terminal",
              "cost_usd": 0.5,
              "had_turn": true,
              "open": false
            }
          ]
        }"#,
    )
    .unwrap();

    let workspace = Store::<Desk>::new(path).load().unwrap();

    assert_eq!(
        workspace.projects().count(),
        1,
        "a project must appear from the old session's directory"
    );
    let project = workspace.projects().next().unwrap();
    assert_eq!(project.name, "guild");
    assert_eq!(
        workspace.sessions_of(project.id).count(),
        1,
        "the session must end up in that project rather than be lost"
    );
}

/// Several old sessions from one directory give one project, not a
/// project per session.
#[test]
fn migrated_sessions_from_one_directory_share_a_project() {
    let dir = temp_dir("migration-shared");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");

    let session = |id: &str| {
        format!(
            r#"{{"id":"{id}","title":"s","cwd":"/work/one","mode":"stream",
                 "cost_usd":0.0,"had_turn":true,"open":false}}"#
        )
    };
    std::fs::write(
        &path,
        format!(
            r#"{{"records":[{},{}]}}"#,
            session("11111111-1111-4111-8111-111111111111"),
            session("22222222-2222-4222-8222-222222222222")
        ),
    )
    .unwrap();

    let workspace = Store::<Desk>::new(path).load().unwrap();

    assert_eq!(workspace.projects().count(), 1);
    assert_eq!(workspace.projects().next().unwrap().name, "one");
}

/// The cost counter is what makes closing a tab safe: a closed session
/// must not spend money unnoticed.
#[test]
fn total_cost_sums_across_projects() {
    let mut workspace = Desk::default();
    let first = workspace.add_project(Path::new("/work/first"));
    let second = workspace.add_project(Path::new("/work/second"));

    let a = workspace.add_session(SessionRecord::new(
        "a".to_owned(),
        first,
        PathBuf::from("/work/first"),
        Runs::Claude(SessionMode::Terminal),
    ));
    let b = workspace.add_session(SessionRecord::new(
        "b".to_owned(),
        second,
        PathBuf::from("/work/second"),
        Runs::Claude(SessionMode::Terminal),
    ));

    workspace.record_usage(
        a,
        Usage {
            cost_usd: Some(0.012),
            ..Usage::default()
        },
        0,
    );
    workspace.record_usage(
        b,
        Usage {
            cost_usd: Some(0.030),
            ..Usage::default()
        },
        0,
    );
    workspace.record_usage(
        a,
        Usage {
            cost_usd: Some(0.020),
            ..Usage::default()
        },
        0,
    );

    assert!((workspace.total_usage().cost_usd.unwrap() - 0.05).abs() < 1e-9);
    assert!((workspace.session(a).unwrap().usage.cost_usd.unwrap() - 0.02).abs() < 1e-9);
}

/// Closing a tab and forgetting a session are different things, and
/// confusing them is expensive: a session that vanished with its tab is
/// lost for good.
#[test]
fn closing_a_tab_keeps_the_session() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let session = workspace.add_session(SessionRecord::new(
        "a task".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.mark_turn(session);

    workspace.close_session(session);

    assert!(
        workspace.session(session).is_some(),
        "the record is still there"
    );
    assert!(
        !workspace.session(session).unwrap().open,
        "but the tab is closed"
    );
    assert_eq!(workspace.open_sessions_of(project).count(), 0);

    workspace.reopen_session(session);
    assert_eq!(workspace.open_sessions_of(project).count(), 1);
}

/// Every launch creates a tab; without a turn there is no returning to
/// it, and such records would pile up and hide the real sessions.
#[test]
fn forgets_closed_sessions_with_nothing_to_return_to() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");

    let empty = workspace.add_session(SessionRecord::new(
        "empty".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let talked = workspace.add_session(SessionRecord::new(
        "talked".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let mut spent = SessionRecord::new(
        "spent".to_owned(),
        project,
        cwd,
        Runs::Claude(SessionMode::Terminal),
    );
    spent.cost_usd = 0.01;
    let spent = workspace.add_session(spent);
    workspace.mark_turn(talked);
    workspace.close_everything();

    workspace.forget_unresumable();

    assert!(
        workspace.session(empty).is_none(),
        "there is no returning to it"
    );
    assert!(workspace.session(talked).is_some());
    assert!(
        workspace.session(spent).is_some(),
        "money spent must not be forgotten even without a conversation"
    );
}

/// The mode is part of the record: it cannot be switched on an existing
/// session, because these are different ways to start the process, not to
/// show it.
#[test]
fn mode_round_trips_through_the_file() {
    let dir = temp_dir("mode");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");
    workspace.add_session(SessionRecord::new(
        "t".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.add_session(SessionRecord::new(
        "s".to_owned(),
        project,
        cwd,
        Runs::Claude(SessionMode::Terminal),
    ));
    store.save(&workspace).unwrap();

    let modes: Vec<Runs> = store.load().unwrap().sessions().map(|s| s.runs).collect();

    assert_eq!(
        modes,
        vec![
            Runs::Claude(SessionMode::Terminal),
            Runs::Claude(SessionMode::Terminal)
        ]
    );
}

/// Tabs survive a restart. Without that, "saving" was only half done:
/// sessions stayed in the list but the layout had to be assembled by hand
/// again.
#[test]
fn open_tabs_are_what_gets_restored() {
    let dir = temp_dir("restore");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");

    let visible = workspace.add_session(SessionRecord::new(
        "open one".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let hidden = workspace.add_session(SessionRecord::new(
        "closed one".to_owned(),
        project,
        cwd,
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.mark_turn(visible);
    workspace.mark_turn(hidden);
    workspace.close_session(hidden);

    store.save(&workspace).unwrap();
    let restored = store.load().unwrap();

    let to_open: Vec<_> = restored.open_sessions_of(project).map(|s| s.id).collect();

    assert_eq!(to_open, vec![visible], "only the open one is restored");
    assert!(
        restored.session(hidden).is_some(),
        "the closed one stays in the list, just not as a tab"
    );
}

/// Where the person was looking survives a restart too: coming back and
/// hunting for the right tab again is not the same as coming back to
/// work.
#[test]
fn the_last_looked_at_tab_survives_a_restart() {
    let dir = temp_dir("focus");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let first = workspace.add_project(Path::new("/work/first"));
    let second = workspace.add_project(Path::new("/work/second"));
    let session = workspace.add_session(SessionRecord::new(
        "second".to_owned(),
        second,
        PathBuf::from("/work/second"),
        Runs::Claude(SessionMode::Terminal),
    ));

    workspace.look_at(Some(second), Some(session));
    store.save(&workspace).unwrap();

    let restored = store.load().unwrap();

    assert_eq!(restored.active_project(), Some(second));
    assert_eq!(restored.active_session(), Some(session));
    assert_ne!(restored.active_project(), Some(first));
}

/// A forgotten session must not stay selected: otherwise the window opens
/// pointing at something that is gone.
#[test]
fn forgetting_the_selected_session_clears_the_selection() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let session = workspace.add_session(SessionRecord::new(
        "empty".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));

    workspace.look_at(Some(project), Some(session));
    workspace.close_session(session);
    workspace.forget_unresumable();

    assert!(workspace.session(session).is_none());
    assert_eq!(
        workspace.active_session(),
        None,
        "a session that no longer exists stayed selected"
    );
}

/// A forgotten project must not stay selected either.
#[test]
fn forgetting_the_selected_project_clears_the_selection() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    workspace.look_at(Some(project), None);

    workspace.remove_project(project);

    assert_eq!(workspace.active_project(), None);
}

/// A shell is not a Claude session. A session has a conversation,
/// `--resume` and a cost; a shell has none of that, and pretending
/// otherwise means fields that are meaningless for half the tabs.
#[test]
fn a_shell_tab_is_not_a_claude_session() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");

    let session = workspace.add_session(SessionRecord::new(
        "session".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let shell = workspace.add_session(SessionRecord::new(
        "shell".to_owned(),
        project,
        cwd,
        Runs::Shell,
    ));

    assert!(workspace.session(session).unwrap().runs.is_claude());
    assert!(!workspace.session(shell).unwrap().runs.is_claude());
}

/// A closed shell is forgotten: there is nowhere to return to, while a
/// row in the list would promise otherwise.
#[test]
fn a_closed_shell_is_forgotten_but_a_closed_session_is_kept() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");

    let session = workspace.add_session(SessionRecord::new(
        "session".to_owned(),
        project,
        cwd.clone(),
        Runs::Claude(SessionMode::Terminal),
    ));
    let shell = workspace.add_session(SessionRecord::new(
        "shell".to_owned(),
        project,
        cwd,
        Runs::Shell,
    ));
    workspace.mark_turn(session);

    workspace.close_session(session);
    workspace.close_session(shell);
    workspace.forget_unresumable();

    assert!(
        workspace.session(session).is_some(),
        "the conversation can be returned to"
    );
    assert!(
        workspace.session(shell).is_none(),
        "there is nowhere to return to in a closed shell"
    );
}

/// An open shell survives a restart: it was on screen, so it is expected
/// back. There is nothing to continue in it — a new one opens in the same
/// directory, and that is honest: the process did not survive the
/// restart.
#[test]
fn an_open_shell_is_restored() {
    let dir = temp_dir("shell");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let shell = workspace.add_session(SessionRecord::new(
        "shell".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Shell,
    ));
    store.save(&workspace).unwrap();

    let restored = store.load().unwrap();
    let to_open: Vec<_> = restored.open_sessions_of(project).map(|s| s.id).collect();

    assert_eq!(to_open, vec![shell]);
    assert_eq!(restored.session(shell).unwrap().runs, Runs::Shell);
}

/// **Migration.** Files in the previous format had no tab kind — they had
/// a `mode` field. Every such record is a Claude session: there were no
/// shells back then.
#[test]
fn records_saved_before_shells_existed_are_claude_sessions() {
    let dir = temp_dir("migration-kind");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");

    std::fs::write(
        &path,
        r#"{"sessions":[{
             "id":"11111111-1111-4111-8111-111111111111",
             "title":"old","cwd":"/work/guild","mode":"stream",
             "cost_usd":0.0,"had_turn":true,"open":false
           }]}"#,
    )
    .unwrap();

    let workspace = Store::<Desk>::new(path).load().unwrap();
    let record = workspace.sessions().next().expect("the record is not lost");

    assert_eq!(
        record.runs,
        Runs::Claude(SessionMode::Terminal),
        "a record in the previous format is a terminal Claude session"
    );
}

/// A shell closed by hand is forgotten at once, without living until the
/// next restart. Otherwise it hangs in the list as a "no conversation" row
/// and promises a return that will not happen.
#[test]
fn a_shell_leaves_no_trace_when_removed() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let shell = workspace.add_session(SessionRecord::new(
        "shell".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Shell,
    ));

    workspace.remove_session(shell);

    assert!(workspace.session(shell).is_none());
    assert_eq!(workspace.sessions_of(project).count(), 0);
}

/// "Open Recent" outlives the project list: a directory removed from the
/// workspace is exactly the one a person then wants back, and hunting for
/// its path a second time is the annoyance the menu exists to remove.
#[test]
fn a_removed_project_stays_in_the_recent_list() {
    let mut workspace = Desk::default();
    let id = workspace.add_project(Path::new("/work/guild"));

    workspace.remove_project(id);

    assert_eq!(workspace.projects().count(), 0);
    assert_eq!(
        workspace.recent().collect::<Vec<_>>(),
        vec![Path::new("/work/guild")],
        "the path is still offered"
    );
}

/// Reopening a directory moves it up rather than adding a twin: a list
/// with the same entry three times is a list nobody reads.
#[test]
fn reopening_a_directory_moves_it_to_the_top() {
    let mut workspace = Desk::default();
    workspace.add_project(Path::new("/a"));
    workspace.add_project(Path::new("/b"));
    workspace.add_project(Path::new("/a"));

    assert_eq!(
        workspace.recent().collect::<Vec<_>>(),
        vec![Path::new("/a"), Path::new("/b")],
        "newest first, and no repeats"
    );
}

/// A menu one has to read through is no faster than the file dialogue it
/// replaces.
#[test]
fn the_recent_list_is_bounded() {
    let mut workspace = Desk::default();
    for index in 0..25 {
        workspace.add_project(Path::new(&format!("/project-{index}")));
    }

    assert_eq!(workspace.recent().count(), 10);
    assert_eq!(
        workspace.recent().next(),
        Some(Path::new("/project-24")),
        "the newest is at the top"
    );
}

#[test]
fn the_recent_list_can_be_forgotten() {
    let mut workspace = Desk::default();
    workspace.add_project(Path::new("/a"));

    workspace.clear_recent();

    assert_eq!(workspace.recent().count(), 0);
}

/// A file written before the list existed must still read: without
/// `serde(default)` every project and session in it would be discarded.
#[test]
fn a_file_without_a_recent_list_still_reads() {
    let workspace: Desk =
        serde_json::from_str(r#"{"projects":[],"sessions":[]}"#).expect("an older file reads");

    assert_eq!(workspace.recent().count(), 0);
}

/// A terminal session's spending arrives from the transcript in pieces,
/// and the pieces add up.
#[test]
fn usage_accumulates_across_readings() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let id = workspace.add_session(SessionRecord::new(
        "guild".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));

    workspace.record_usage(
        id,
        Usage {
            output_tokens: 100,
            ..Usage::default()
        },
        512,
    );
    workspace.record_usage(
        id,
        Usage {
            output_tokens: 50,
            ..Usage::default()
        },
        900,
    );

    let record = workspace.session(id).expect("the session");
    assert_eq!(record.usage.output_tokens, 150, "the pieces add up");
    assert_eq!(
        record.transcript_offset, 900,
        "and we remember where we stopped"
    );
}

/// The offset survives a restart, so coming back does not re-read a
/// transcript that can be megabytes long.
#[test]
fn the_read_offset_survives_a_save_and_load() {
    let dir = temp_dir("usage-offset");
    let store: Store<Desk> = Store::new(dir.join("workspace.json"));

    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let id = workspace.add_session(SessionRecord::new(
        "guild".to_owned(),
        project,
        PathBuf::from("/work/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.record_usage(
        id,
        Usage {
            output_tokens: 7,
            cost_usd: Some(0.25),
            ..Usage::default()
        },
        4096,
    );
    store.save(&workspace).expect("saved");

    let back = store.load().expect("loaded");
    let record = back.session(id).expect("the session");

    assert_eq!(record.transcript_offset, 4096);
    assert_eq!(record.usage.output_tokens, 7);
    assert_eq!(record.usage.cost_usd, Some(0.25));
}

/// A file written before this field existed must still read. The shape
/// below was taken off disk rather than remembered: the last time an old
/// format was guessed at, the migration test checked a key the real file
/// never had, and it passed on invented data.
#[test]
fn a_file_written_before_usage_existed_still_loads() {
    let dir = temp_dir("usage-absent");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    std::fs::write(
        &path,
        r#"{
  "projects": [
    {
      "id": "6ff7dbde-e6c3-48d5-92b8-3feff220f6d5",
      "path": "/work/guild",
      "name": "guild"
    }
  ],
  "sessions": [
    {
      "id": "789c3866-1948-4f6b-a282-11b631474497",
      "project": "6ff7dbde-e6c3-48d5-92b8-3feff220f6d5",
      "title": "guild",
      "cwd": "/work/guild",
      "runs": { "claude": "terminal" },
      "cost_usd": 0.0,
      "had_turn": true,
      "open": true
    }
  ]
}"#,
    )
    .unwrap();

    let workspace = Store::<Desk>::new(path).load().expect("loads");

    let session = workspace.sessions().next().expect("the session survives");
    assert_eq!(session.usage, Usage::default(), "nothing read from it yet");
    assert_eq!(
        session.transcript_offset, 0,
        "and we start at the beginning"
    );
    assert!(session.had_turn, "what the file did say is kept");
}

/// **A session started with a task stays one across a restart** — M13.
/// The rule that such a session may not start another with a task is
/// only a rule if it survives the window being closed and opened again,
/// and every record written before M13 must read as what it was: started
/// by a person.
#[test]
fn a_task_is_remembered_and_an_old_record_is_a_person_s() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/w/guild"));
    let by_hand = workspace.add_session(SessionRecord::new(
        "guild".to_owned(),
        project,
        PathBuf::from("/w/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    let by_command = workspace.add_session(SessionRecord::new(
        "guild".to_owned(),
        project,
        PathBuf::from("/w/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    workspace.mark_task(by_command);

    let written = serde_json::to_string(&workspace).unwrap();
    let read: Desk = serde_json::from_str(&written).unwrap();
    assert!(!read.session(by_hand).unwrap().task);
    assert!(read.session(by_command).unwrap().task);

    // A record from before the field existed has no `task` key at all.
    let mut old: serde_json::Value = serde_json::from_str(&written).unwrap();
    for session in old["sessions"].as_array_mut().unwrap() {
        session.as_object_mut().unwrap().remove("task");
    }
    let from_before: Desk = serde_json::from_value(old).unwrap();
    assert!(from_before.sessions().all(|session| !session.task));
}

/// **A name a person gives a tab is kept, and an empty one gives the tab
/// back its own** — M12.1 task 1. Kept on the record, so a restart shows
/// it; trimmed, because a stray space is not part of a name; and a record
/// written before the field existed simply has none.
#[test]
fn a_name_a_person_gives_is_kept_and_an_empty_one_takes_it_away() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/w/guild"));
    let id = workspace.add_session(SessionRecord::new(
        "guild".to_owned(),
        project,
        PathBuf::from("/w/guild"),
        Runs::Claude(SessionMode::Terminal),
    ));
    assert_eq!(workspace.session(id).unwrap().name, None);

    workspace.rename_session(id, "  login bug ");
    let written = serde_json::to_string(&workspace).unwrap();
    let read: Desk = serde_json::from_str(&written).unwrap();
    assert_eq!(read.session(id).unwrap().name.as_deref(), Some("login bug"));

    workspace.rename_session(id, "   ");
    assert_eq!(workspace.session(id).unwrap().name, None);

    let mut old: serde_json::Value = serde_json::from_str(&written).unwrap();
    for session in old["sessions"].as_array_mut().unwrap() {
        session.as_object_mut().unwrap().remove("name");
    }
    let from_before: Desk = serde_json::from_value(old).unwrap();
    assert_eq!(from_before.session(id).unwrap().name, None);
}

/// **A project is called what the person named it** — M12.2 task 9, *Rename
/// Project*. Kept by the repository rather than by any one of its roots, so
/// every worktree's group carries it; trimmed, and a blank name gives the
/// project its folder's name back. A file from before has no names at all.
#[test]
fn a_project_is_called_what_the_person_named_it() {
    let key = Path::new("/w/guild/.git");
    let mut workspace = Desk::default();
    assert_eq!(workspace.repository_name(key), None);

    workspace.rename_repository(key, "  Guild desktop ");
    let written = serde_json::to_string(&workspace).unwrap();
    let read: Desk = serde_json::from_str(&written).unwrap();
    assert_eq!(read.repository_name(key), Some("Guild desktop"));

    workspace.rename_repository(key, "  ");
    assert_eq!(workspace.repository_name(key), None);

    let mut old: serde_json::Value = serde_json::from_str(&written).unwrap();
    old.as_object_mut().unwrap().remove("names");
    let from_before: Desk = serde_json::from_value(old).unwrap();
    assert_eq!(from_before.repository_name(key), None);
}

/// **A project folds by its repository** — M12.5 decision 18. One answer
/// for every worktree of it, kept by the key its name is kept by; a desk
/// with nothing folded writes no key, so a file stays as it was.
#[test]
fn a_project_folds_and_unfolds_by_its_key() {
    let key = Path::new("/w/guild/.git");
    let other = Path::new("/w/notes");
    let mut workspace = Desk::default();
    assert!(!workspace.is_folded(key));

    assert!(workspace.toggle_fold(key));
    assert!(workspace.is_folded(key));
    assert!(!workspace.is_folded(other));

    assert!(!workspace.toggle_fold(key));
    assert!(!workspace.is_folded(key));

    workspace.toggle_fold(key);
    workspace.unfold(key);
    assert!(!workspace.is_folded(key));
    // Unfolding what is open is nothing.
    workspace.unfold(other);
    assert!(!workspace.is_folded(other));
}

#[test]
fn folding_survives_a_write_and_a_read() {
    let key = Path::new("/w/guild/.git");
    let mut workspace = Desk::default();
    let open = serde_json::to_value(&workspace).unwrap();
    assert!(open.get("folded").is_none(), "{open}");

    workspace.toggle_fold(key);
    let written = serde_json::to_string(&workspace).unwrap();
    let read: Desk = serde_json::from_str(&written).unwrap();
    assert!(read.is_folded(key));
}

/// The keys of a desk written before folding, as the person's own
/// `workspace.json` had them on 2026-09-15.
#[test]
fn a_desk_written_before_folding_reads_with_nothing_folded() {
    let old = r#"{
        "projects": [],
        "sessions": [],
        "active_project": null,
        "active_session": null,
        "recent": ["/w/guild"]
    }"#;
    let read: Desk = serde_json::from_str(old).unwrap();
    assert!(!read.is_folded(Path::new("/w/guild/.git")));
}

/// A closed Codex or OpenCode record is kept across a restart once it has
/// a conversation — the id its harness reported — as a Claude one is once
/// it has had a turn. Only an agent that never said which conversation it
/// was is forgotten, like Claude's untouched tab: there is nothing to
/// return to.
#[test]
fn a_closed_codex_or_opencode_record_with_a_conversation_is_kept() {
    let mut workspace = Desk::default();
    let project = workspace.add_project(Path::new("/work/guild"));
    let cwd = PathBuf::from("/work/guild");
    let mut add = |runs| {
        workspace.add_session(SessionRecord::new(
            "t".to_owned(),
            project,
            cwd.clone(),
            runs,
        ))
    };
    let codex = add(Runs::Codex(SessionMode::Terminal));
    let opencode = add(Runs::OpenCode(SessionMode::Terminal));
    let silent = add(Runs::Codex(SessionMode::Terminal));
    workspace.record_harness_session(codex, "019a-codex".to_owned());
    workspace.record_harness_session(opencode, "ses_a".to_owned());

    for id in [codex, opencode, silent] {
        workspace.close_session(id);
    }
    assert!(workspace.session(codex).unwrap().has_conversation());
    assert!(!workspace.session(silent).unwrap().has_conversation());
    workspace.forget_unresumable();

    assert!(workspace.session(codex).is_some(), "Codex's conversation");
    assert!(
        workspace.session(opencode).is_some(),
        "OpenCode's conversation"
    );
    assert!(
        workspace.session(silent).is_none(),
        "no id reported, no turn: nothing to return to"
    );
}
