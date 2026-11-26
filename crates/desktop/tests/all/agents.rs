//! What the status bar's counter counts, and how its list is grouped —
//! M12.2 task 5.

use guild_desktop::agents::{Agent, Open, groups, is_claude_command, kind_of, total, unnamed};
use guild_desktop::i18n::Text;
use guild_desktop::view::menu::{self, Choice, Deed, Entry, Label, Mark};
use guild_session::{Runs, SessionId, SessionMode};

fn open(kind: Agent, name: &str) -> Open {
    Open {
        session: SessionId::new(),
        kind,
        name: name.to_owned(),
        project: "guild".to_owned(),
        workspace: None,
    }
}

#[test]
fn a_row_can_name_the_workspace_when_the_window_has_several() {
    let mut row = open(Agent::ClaudeCode, "review");
    assert_eq!(row.workspace, None);
    row.workspace = Some("Side".into());
    assert_eq!(row.workspace.as_deref(), Some("Side"));
}

/// **The counter counts agents, not tabs.** A Claude Code session is an agent;
/// a shell is a terminal, and counting it would be counting
/// something nobody asked the number to say.
#[test]
fn a_claude_session_is_an_agent_and_a_shell_is_not() {
    assert_eq!(
        kind_of(Runs::Claude(SessionMode::Terminal)),
        Some(Agent::ClaudeCode)
    );
    assert_eq!(kind_of(Runs::Shell), None);
}

/// **One total, a group per kind of agent, the tabs' own order inside a
/// group.** There is one kind today; the person said a second — Open Code —
/// will come and be listed apart under the same total, so the shape is a
/// list of groups from the start.
#[test]
fn the_list_is_grouped_by_kind_under_one_total() {
    let found = groups(vec![
        open(Agent::ClaudeCode, "login bug"),
        open(Agent::ClaudeCode, "review"),
    ]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, Agent::ClaudeCode);
    let names: Vec<&str> = found[0].rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["login bug", "review"], "the tabs' order");
    assert_eq!(total(&found), 2);
    assert!(groups(Vec::new()).is_empty());
    assert_eq!(total(&[]), 0);
}

/// **`claude` typed into a shell is Claude Code too** — M12.3 task 4. What a
/// shell tab runs in front is read as a command line, and every way Claude
/// Code is found on this machine counts: the name on the `PATH`, the
/// versioned binary it links to, the app bundle's own, the npm install's
/// script, and a wrapper script that runs it. A file called `claude.md`
/// being edited does not.
#[test]
fn claude_is_recognised_however_it_was_started() {
    for claude in [
        "claude",
        "claude --resume 0b6b0a52",
        "/Users/person/.local/bin/claude",
        "/Users/person/.local/share/claude/versions/2.1.270 --permission-mode auto",
        "/Users/person/.local/share/claude/ClaudeCode.app/Contents/MacOS/claude",
        "node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
        "/bin/bash /Users/person/.wrapper/bin/claude --resume 4e73c42f",
    ] {
        assert!(is_claude_command(claude), "{claude}");
    }
    for other in [
        "zsh",
        "-zsh",
        "vim claude.md",
        "cat /tmp/claude",
        "",
        "node server.js",
        "claudette",
    ] {
        assert!(!is_claude_command(other), "{other}");
    }
}

/// **The counter's list is a menu of the engine now** — M12.7 task 4: a
/// heading per kind, by its name in the translation table, then a choice
/// per agent with the kind's mark and a check on the one whose session is
/// the front tab's — so exactly one row carries `Some(true)` when the
/// front is one of the agents listed, and none when it is not.
///
/// **The label is the agent's name alone, and the place is its `detail`**
/// — fix round 1: it rode in the label after an en dash until then, and
/// two agents can share a name.
#[test]
fn the_list_is_a_heading_per_kind_and_a_choice_per_agent_with_a_check_on_the_front() {
    let mut claude_front = open(Agent::ClaudeCode, "review");
    claude_front.workspace = Some("Side".into());
    let claude_other = open(Agent::ClaudeCode, "login bug");
    let code = open(Agent::OpenCode, "docs");
    let front = claude_front.session;
    let found = groups(vec![
        claude_front.clone(),
        claude_other.clone(),
        code.clone(),
    ]);

    let list = menu::agents::agents(&found, Some(front));
    assert!(
        matches!(&list[0], Entry::Heading(Label::Text(Text::AgentClaudeCode))),
        "a heading names the kind: {:?}",
        list[0]
    );
    let choices: Vec<&Choice> = list
        .iter()
        .filter_map(|entry| match entry {
            Entry::Choice(choice) => Some(choice),
            _ => None,
        })
        .collect();
    assert_eq!(choices.len(), 3, "one choice per agent, every group's");
    for choice in &choices {
        assert!(choice.enabled);
    }
    let lit: Vec<&Choice> = choices
        .iter()
        .copied()
        .filter(|choice| choice.check == Some(true))
        .collect();
    assert_eq!(lit.len(), 1, "exactly one row carries the front's check");
    assert_eq!(lit[0].mark, Mark::Status(front));
    assert_eq!(lit[0].deed, Deed::ShowSession(front));
    assert_eq!(
        lit[0].label,
        Label::Named("review".into()),
        "the name alone, not fused with its place"
    );
    assert_eq!(
        lit[0].detail,
        Some(Label::Named("Side · guild".into())),
        "the workspace and the project, as the label used to carry them"
    );
    // No workspace: the detail is the project alone.
    let other = choices
        .iter()
        .find(|choice| choice.deed == Deed::ShowSession(claude_other.session))
        .unwrap();
    assert_eq!(other.detail, Some(Label::Named("guild".into())));

    // With no front among them — or no front at all — every check is
    // `Some(false)`, and no row is `Some(true)`.
    for absent in [None, Some(SessionId::new())] {
        let list = menu::agents::agents(&found, absent);
        assert!(
            list.iter().all(|entry| !matches!(
                entry,
                Entry::Choice(choice) if choice.check == Some(true)
            )),
            "nothing is lit when the front is not one of the agents listed"
        );
    }
}

/// **A window with nothing open is a single note**, the empty list's own
/// words — not a heading with nothing under it.
#[test]
fn an_empty_window_is_a_single_note() {
    assert_eq!(
        menu::agents::agents(&[], None),
        vec![Entry::Note(Label::Text(Text::AgentsNone))]
    );
}

/// Window check 3: two agents in one divided tab were both *Session 8*,
/// and only the dot told them apart. An agent with no name of its own is
/// called by its tab's label, and — in a tab showing more than one agent
/// — by its pane's place in the tab after it, counted from one in draw
/// order, empty panes and shells included: the place the person sees.
#[test]
fn two_agents_of_one_tab_are_told_apart_by_their_pane() {
    assert_eq!(
        unnamed("Session 8", 1, 1),
        "Session 8",
        "one agent: the tab's name"
    );
    assert_eq!(
        unnamed("Session 8", 3, 1),
        "Session 8",
        "beside a shell, too"
    );
    assert_eq!(unnamed("Session 8", 1, 2), "Session 8 · 1");
    assert_eq!(unnamed("Session 8", 2, 2), "Session 8 · 2");
    assert_eq!(unnamed("Сессия 8", 3, 2), "Сессия 8 · 3");
}

/// `cmd-shift-u` walks the board's attention order: the blocked first,
/// then the failed and the unread, each the longest waiting first — and
/// looking at a turn is not activity.
#[test]
fn go_to_waiting_follows_the_board_s_attention_order() {
    use guild_session::status::Board;
    let rows: Vec<_> = [
        "idle",
        "working",
        "done",
        "failed",
        "waiting",
        "waiting later",
    ]
    .into_iter()
    .map(|name| open(Agent::Codex, name))
    .collect();
    let mut board = Board::default();
    use guild_session::status::{Kind, Reported};
    for (i, record) in [
        (1, Reported::Working),
        (2, Reported::Done),
        (3, Reported::Error),
        (4, Reported::Blocked(Some(Kind::Question))),
        (5, Reported::Blocked(Some(Kind::Permission))),
    ] {
        board.report(rows[i].session, Some(record));
    }
    let open: Vec<_> = rows.iter().map(|row| row.session).collect();
    let queue = board.attention_order(&open);
    assert_eq!(
        queue,
        vec![
            rows[4].session,
            rows[5].session,
            rows[3].session,
            rows[2].session
        ]
    );
    let mut current = None;
    for expected in &queue {
        current = board.next_waiting(current, &open);
        assert_eq!(current, Some(*expected));
    }
    let before = board.transitioned_at(rows[2].session);
    board.seen(rows[2].session);
    assert_eq!(
        board.transitioned_at(rows[2].session),
        before,
        "viewing is not execution activity"
    );
    assert!(!board.attention_order(&open).contains(&rows[2].session));
    board.seen(rows[4].session);
    assert!(board.attention_order(&open).contains(&rows[4].session));
}

#[test]
fn activity_age_uses_real_time_and_unknown_stays_unknown() {
    use guild_desktop::agents::relative_time;
    use guild_session::status::Board;
    use std::time::{Duration, SystemTime};
    let ui = guild_desktop::i18n::Ui(guild_session::Language::English);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
    assert_eq!(relative_time(None, now, ui), "—");
    assert_eq!(
        relative_time(Some(now + Duration::from_secs(1)), now, ui),
        "—"
    );
    for (seconds, label) in [
        (0, "just now"),
        (59, "just now"),
        (60, "1m ago"),
        (3599, "59m ago"),
        (3600, "1h ago"),
        (86400, "1d ago"),
    ] {
        assert_eq!(
            relative_time(Some(now - Duration::from_secs(seconds)), now, ui),
            label
        );
    }
    assert_eq!(
        relative_time(
            Some(now - Duration::from_secs(120)),
            now,
            guild_desktop::i18n::Ui(guild_session::Language::Russian)
        ),
        "2 мин назад"
    );
    let id = SessionId::new();
    let mut board = Board::default();
    assert_eq!(board.transitioned_at(id), None);
    let before = SystemTime::now();
    board.report(id, Some(guild_session::status::Reported::Working));
    let at = board.transitioned_at(id).unwrap();
    assert!(at >= before && at <= SystemTime::now());
    board.report(id, Some(guild_session::status::Reported::Working));
    assert_eq!(
        board.transitioned_at(id),
        Some(at),
        "same state is not a transition"
    );
    board.forget(id);
    assert_eq!(board.transitioned_at(id), None);
    assert_eq!(
        Board::default().transitioned_at(id),
        None,
        "no fabricated restore age"
    );
}
