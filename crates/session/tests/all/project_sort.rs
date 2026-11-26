use guild_session::{Desk, project_sort::Mode};
use std::path::Path;

#[test]
fn workspace_sort_roundtrips_without_replacing_manual_roots() {
    let mut desk = Desk::default();
    let a = desk.open_in(guild_session::WorkspaceId::nil(), Path::new("/a"));
    let b = desk.open_in(desk.active_workspace(), Path::new("/b"));
    let workspace = desk.active_workspace();
    let order = desk.workspace(workspace).unwrap().projects.clone();
    assert!(desk.set_project_sort(workspace, Mode::RecentActivity));
    let mut restored: Desk = serde_json::from_str(&serde_json::to_string(&desk).unwrap()).unwrap();
    assert_eq!(
        restored.workspace(workspace).unwrap().project_sort,
        Mode::RecentActivity
    );
    assert_eq!(restored.workspace(workspace).unwrap().projects, order);
    assert!(restored.set_project_sort(workspace, Mode::Manual));
    assert_eq!(restored.workspace(workspace).unwrap().projects, order);
    assert!(order.contains(&a) && order.contains(&b));
}

#[test]
fn old_workspace_defaults_to_manual_and_mode_is_workspace_local() {
    let mut desk = Desk::default();
    desk.open_in(guild_session::WorkspaceId::nil(), Path::new("/a"));
    let first = desk.active_workspace();
    let second = desk.add_workspace("Other", None).unwrap();
    desk.set_project_sort(first, Mode::MostActivity);
    assert_eq!(desk.workspace(second).unwrap().project_sort, Mode::Manual);
    let mut json = serde_json::to_value(&desk).unwrap();
    for workspace in json["workspaces"].as_array_mut().unwrap() {
        workspace.as_object_mut().unwrap().remove("project_sort");
    }
    let old: Desk = serde_json::from_value(json).unwrap();
    assert_eq!(old.workspace(first).unwrap().project_sort, Mode::Manual);
}

#[test]
fn a_valid_project_drag_selects_manual_but_a_stale_drop_does_not() {
    let mut desk = Desk::default();
    let a = desk.open_in(guild_session::WorkspaceId::nil(), Path::new("/a"));
    let workspace = desk.active_workspace();
    let b = desk.open_in(workspace, Path::new("/b"));
    desk.set_project_sort(workspace, Mode::RecentActivity);
    assert!(!desk.reorder_projects(workspace, &[a], a));
    assert_eq!(
        desk.workspace(workspace).unwrap().project_sort,
        Mode::RecentActivity
    );
    assert!(desk.reorder_projects(workspace, &[a], b));
    assert_eq!(
        desk.workspace(workspace).unwrap().project_sort,
        Mode::Manual
    );
    assert_eq!(desk.workspace(workspace).unwrap().projects, [b, a]);
}

fn observed_desk() -> (Desk, guild_session::SessionId, guild_session::WorkspaceId) {
    let mut desk = Desk::default();
    let project = desk.open_in(guild_session::WorkspaceId::nil(), Path::new("/a"));
    let workspace = desk.active_workspace();
    let record = guild_session::SessionRecord::new(
        "Agent".into(),
        (workspace, project),
        Path::new("/a").into(),
        guild_session::Runs::Claude(guild_session::SessionMode::Terminal),
    );
    let id = desk.add_session(record);
    (desk, id, workspace)
}

/// What the program reported, as `Board::report` answered it.
fn outcome(changed: bool, began: bool, ended: bool) -> guild_session::status::ReportOutcome {
    guild_session::status::ReportOutcome {
        changed,
        began,
        ended,
    }
}

/// **Activity and turns come from what the program reports** (OSC 7501):
/// a turn is counted when it begins, any change of the record is activity,
/// a repeat is nothing, and the sums survive a restart and a forgotten
/// session.
#[test]
fn reported_turns_count_once_and_every_change_is_activity() {
    let (mut desk, id, _) = observed_desk();
    assert!(desk.observe_reported_activity(id, outcome(true, true, false), 10));
    assert!(!desk.observe_reported_activity(id, outcome(false, false, false), 11));
    assert!(desk.observe_reported_activity(id, outcome(true, false, false), 12));
    assert!(desk.observe_reported_activity(id, outcome(true, false, true), 20));
    let activity = desk.project_activity(Path::new("/a"));
    assert_eq!((activity.turns, activity.last_activity), (1, Some(20)));
    assert_eq!(activity.last_user, None, "a report is not authorship");
    let mut desk: Desk = serde_json::from_str(&serde_json::to_string(&desk).unwrap()).unwrap();
    desk.observe_reported_activity(id, outcome(true, true, false), 30);
    assert_eq!(desk.project_activity(Path::new("/a")).turns, 2);
    desk.remove_session(id);
    assert_eq!(
        desk.project_activity(Path::new("/a")).last_activity,
        Some(30),
        "closed/forgotten sessions do not erase worktree history"
    );
}

/// **Only a prompt the person submitted dates "last user message"** — the
/// harness's own `UserPromptSubmit`, once per turn it names; never an
/// OpenCode execution start, never another hook.
#[test]
fn only_a_submitted_prompt_dates_the_last_user_message() {
    use guild_session::{Runs, SessionMode, SessionRecord};
    use serde_json::json;
    let (mut desk, id, workspace) = observed_desk();
    let prompt = json!({"hook_event_name":"UserPromptSubmit", "prompt_id":"one"});
    assert!(desk.confirm_user_prompt(id, &prompt, 10));
    assert!(
        !desk.confirm_user_prompt(id, &prompt, 11),
        "the same prompt again"
    );
    assert!(!desk.confirm_user_prompt(id, &json!({"hook_event_name":"Stop"}), 12));
    assert_eq!(desk.project_activity(Path::new("/a")).last_user, Some(10));
    let bare = json!({"hook_event_name":"UserPromptSubmit"});
    assert!(desk.confirm_user_prompt(id, &bare, 13));
    assert!(
        desk.confirm_user_prompt(id, &bare, 14),
        "no identity: each is a prompt"
    );
    assert_eq!(desk.project_activity(Path::new("/a")).last_user, Some(14));
    assert_eq!(
        desk.project_activity(Path::new("/a")).turns,
        0,
        "authorship counts no turn"
    );

    let project = desk.workspace(workspace).unwrap().projects[0];
    let open_code = desk.add_session(SessionRecord::new(
        "OpenCode".into(),
        (workspace, project),
        "/a".into(),
        Runs::OpenCode(SessionMode::Terminal),
    ));
    let event = json!({"opencode_event":"prompt", "request_id":"execution-1", "hook_event_name":"UserPromptSubmit"});
    assert!(!desk.confirm_user_prompt(open_code, &event, 20));
    let codex = desk.add_session(SessionRecord::new(
        "Codex".into(),
        (workspace, project),
        "/a".into(),
        Runs::Codex(SessionMode::Terminal),
    ));
    desk.record_harness_session(codex, "conversation".into());
    let turn = json!({"turn_id":"first", "hook_event_name":"UserPromptSubmit", "codex_session":"conversation"});
    assert!(desk.confirm_user_prompt(codex, &turn, 30));
    let mut desk: Desk = serde_json::from_str(&serde_json::to_string(&desk).unwrap()).unwrap();
    assert!(
        !desk.confirm_user_prompt(codex, &turn, 31),
        "a replayed turn after restart"
    );
    assert_eq!(desk.project_activity(Path::new("/a")).last_user, Some(30));
}

#[test]
fn sorted_drop_moves_only_one_group_in_saved_manual_order() {
    let mut desk = Desk::default();
    let a = desk.open_in(guild_session::WorkspaceId::nil(), Path::new("/a"));
    let workspace = desk.active_workspace();
    let b = desk.open_in(workspace, Path::new("/b"));
    let c = desk.open_in(workspace, Path::new("/c"));
    let linked = desk.open_in(workspace, Path::new("/a-linked"));
    desk.set_project_sort(workspace, Mode::MostActivity);
    // Display can be C, A, B. A dropped on C goes before C, even though
    // A originally precedes C in the saved manual list.
    assert!(desk.reorder_projects_at(workspace, &[a, linked], c, false));
    assert_eq!(
        desk.workspace(workspace).unwrap().projects,
        [b, a, linked, c]
    );
    assert_eq!(
        desk.workspace(workspace).unwrap().project_sort,
        Mode::Manual
    );
    desk.set_project_sort(workspace, Mode::LastUserMessage);
    desk.set_project_sort(workspace, Mode::Manual);
    assert_eq!(
        desk.workspace(workspace).unwrap().projects,
        [b, a, linked, c]
    );
}

/// **Recent activity sorts by the last turn that began** — what the
/// program reported (OSC 7501), whoever began it. Any other change of its
/// record, and the person's prompt by its hook, leave the key alone, so a
/// working agent does not reorder the column.
#[test]
fn last_turn_moves_only_when_a_reported_turn_begins() {
    use serde_json::json;
    let (mut desk, id, _) = observed_desk();
    let last_turn = |desk: &Desk| desk.project_activity(Path::new("/a")).last_turn;
    assert!(desk.observe_reported_activity(id, outcome(true, true, false), 10));
    assert_eq!(last_turn(&desk), Some(10));
    let prompt = json!({"turn_id":"one", "hook_event_name":"UserPromptSubmit"});
    assert!(desk.confirm_user_prompt(id, &prompt, 11));
    assert_eq!(last_turn(&desk), Some(10), "authorship is not a turn start");
    for (outcome, at) in [
        (outcome(true, false, false), 12),
        (outcome(false, false, false), 13),
        (outcome(true, false, true), 16),
    ] {
        desk.observe_reported_activity(id, outcome, at);
        assert_eq!(
            last_turn(&desk),
            Some(10),
            "{outcome:?} is not a turn start"
        );
    }
    assert_eq!(
        desk.project_activity(Path::new("/a")).last_activity,
        Some(16)
    );
    assert!(desk.observe_reported_activity(id, outcome(true, true, false), 20));
    assert_eq!(last_turn(&desk), Some(20));
    let restored: Desk = serde_json::from_str(&serde_json::to_string(&desk).unwrap()).unwrap();
    assert_eq!(last_turn(&restored), Some(20));
}

#[test]
fn activity_saved_before_turn_starts_were_kept_is_seeded_once() {
    let (mut desk, id, _) = observed_desk();
    desk.observe_reported_activity(id, outcome(true, true, false), 10);
    desk.observe_reported_activity(id, outcome(true, false, false), 15);
    let mut old = serde_json::to_value(&desk).unwrap();
    for activity in old["project_activity"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        activity.as_object_mut().unwrap().remove("last_turn");
    }
    let mut desk: Desk = serde_json::from_value(old).unwrap();
    let activity = desk.project_activity(Path::new("/a"));
    assert_eq!(
        activity.last_turn,
        Some(15),
        "older state dates its turns by its last activity"
    );
    assert_eq!(activity.last_activity, Some(15));
    desk.observe_reported_activity(id, outcome(true, false, true), 30);
    assert_eq!(
        desk.project_activity(Path::new("/a")).last_turn,
        Some(15),
        "the seed is not a running fallback"
    );

    // A project whose program never reported a turn beginning has none, and
    // a reload of new state must not re-seed it from its other activity.
    let (mut quiet, id, _) = observed_desk();
    quiet.observe_reported_activity(id, outcome(true, false, false), 40);
    let quiet: Desk = serde_json::from_str(&serde_json::to_string(&quiet).unwrap()).unwrap();
    assert_eq!(quiet.project_activity(Path::new("/a")).last_turn, None);
    assert_eq!(
        quiet.project_activity(Path::new("/a")).last_activity,
        Some(40)
    );
}

/// **Recent activity's active block keeps the order projects became
/// active in.** A project is active while one of its agent sessions runs;
/// what those sessions do meanwhile does not move it, and only becoming
/// active again after it stopped puts it at the end of the block.
#[test]
fn active_projects_keep_the_order_they_became_active_in() {
    use guild_session::project_sort::Active;
    let mut active = Active::default();
    let order = |active: &Active<&'static str>, keys: &[&'static str]| {
        let mut keys: Vec<_> = keys
            .iter()
            .filter_map(|key| active.since(key).map(|since| (since, *key)))
            .collect();
        keys.sort();
        keys.into_iter().map(|(_, key)| key).collect::<Vec<_>>()
    };
    let all = ["guild", "deploy", "base13", "other"];
    assert!(active.update(["guild"]));
    assert!(active.update(["guild", "deploy"]));
    assert!(active.update(["base13", "deploy", "guild"]));
    assert_eq!(order(&active, &all), ["guild", "deploy", "base13"]);
    assert!(
        !active.update(["deploy", "base13", "guild"]),
        "the same set in another order changes nothing"
    );

    // `deploy` waits for an answer, finishes a turn, rests, starts
    // another: its session runs throughout, so nothing it does moves it.
    for _ in 0..3 {
        active.update(["guild", "deploy", "base13"]);
        assert_eq!(order(&active, &all), ["guild", "deploy", "base13"]);
    }

    assert!(active.update(["guild", "base13"]), "deploy's agent exited");
    assert_eq!(active.since(&"deploy"), None, "its place is forgotten");
    assert!(active.update(["guild", "base13", "other"]));
    assert_eq!(order(&active, &all), ["guild", "base13", "other"]);
    assert!(active.update(["guild", "base13", "other", "deploy"]));
    assert_eq!(
        order(&active, &all),
        ["guild", "base13", "other", "deploy"],
        "active again after it stopped is the end of the block"
    );
}

/// Projects that become active in one pass share a moment, so their tie
/// is broken by the column's own stable order and not by a set's.
#[test]
fn projects_active_in_one_pass_share_their_moment() {
    use guild_session::project_sort::Active;
    let mut active = Active::default();
    active.update(["a"]);
    active.update(["a", "b", "c"]);
    assert!(active.since(&"a") < active.since(&"b"));
    assert_eq!(active.since(&"b"), active.since(&"c"));
    assert!(active.update(std::iter::empty::<&str>()));
    assert_eq!(active.since(&"a"), None);
}

/// **A running agent session is one alive and used since it was opened.**
/// Opening a tab is not a turn; a turn — work, a question, an answer —
/// makes it running, and rest between turns, the answer seen included,
/// keeps it so. A failure, its program exiting or its tab closing ends it,
/// and only its next turn makes it running again.
#[test]
fn an_agent_session_runs_from_its_first_turn_until_it_fails_or_goes() {
    use guild_session::project_sort::Running;
    use guild_session::status::{Status, Why};
    let mut running = Running::default();
    let mut pass = |sessions: &[(&'static str, bool, Status)]| {
        let mut found: Vec<_> = running
            .update(sessions.iter().copied())
            .into_iter()
            .collect();
        found.sort();
        found
    };
    assert!(
        pass(&[("a", true, Status::Idle)]).is_empty(),
        "a freshly opened tab is not running"
    );
    assert_eq!(pass(&[("a", true, Status::Working)]), ["a"]);
    for status in [
        Status::Done,
        Status::Idle,
        Status::Waiting(Why::Question),
        Status::Idle,
        Status::Working,
    ] {
        assert_eq!(
            pass(&[("a", true, status)]),
            ["a"],
            "{status:?} between turns keeps it running"
        );
    }
    assert_eq!(
        pass(&[
            ("a", true, Status::Idle),
            ("b", true, Status::Waiting(Why::Permission))
        ]),
        ["a", "b"]
    );

    assert_eq!(
        pass(&[("a", true, Status::Failed), ("b", true, Status::Idle)]),
        ["b"]
    );
    assert_eq!(
        pass(&[("a", true, Status::Idle), ("b", true, Status::Idle)]),
        ["b"],
        "after a failure, rest is not running"
    );
    assert_eq!(
        pass(&[("a", true, Status::Working), ("b", true, Status::Idle)]),
        ["a", "b"]
    );

    assert_eq!(
        pass(&[("a", false, Status::Idle), ("b", true, Status::Idle)]),
        ["b"],
        "its program exited"
    );
    assert_eq!(
        pass(&[("a", true, Status::Idle), ("b", true, Status::Idle)]),
        ["b"],
        "the same session alive again has had no turn"
    );
    assert!(
        pass(&[("a", true, Status::Idle)]).is_empty(),
        "b's tab closed: it is not in the pass at all"
    );
    assert!(
        pass(&[("a", true, Status::Idle), ("b", true, Status::Idle)]).is_empty(),
        "and a tab opened again for it starts from no turn"
    );
}

/// The column's Sessions mode ages a row by its last activity, and the
/// board keeps nothing across a restart: the record carries it.
#[test]
fn a_session_keeps_its_last_activity_across_a_save() {
    let (mut desk, id, _) = observed_desk();
    assert_eq!(desk.session(id).unwrap().last_activity, None);
    assert!(desk.observe_reported_activity(id, outcome(true, true, false), 10));
    assert!(desk.observe_reported_activity(id, outcome(true, false, true), 25));
    // An unchanged report is not activity, and time does not run back.
    assert!(!desk.observe_reported_activity(id, outcome(false, false, false), 40));
    assert!(desk.observe_reported_activity(id, outcome(true, false, false), 20));
    assert_eq!(desk.session(id).unwrap().last_activity, Some(25));
    let restored: Desk = serde_json::from_str(&serde_json::to_string(&desk).unwrap()).unwrap();
    assert_eq!(restored.session(id).unwrap().last_activity, Some(25));
    let mut json = serde_json::to_value(&desk).unwrap();
    for record in json["sessions"].as_array_mut().unwrap() {
        record.as_object_mut().unwrap().remove("last_activity");
    }
    let old: Desk = serde_json::from_value(json).unwrap();
    assert_eq!(old.session(id).unwrap().last_activity, None);
}
