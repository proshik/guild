//! Closing a tab whose agent is at work asks first, once for every tab at
//! stake — M5.12, task 1.

use guild_desktop::closing::{
    Stake, agent_open, at_stake, sheet, strongest, tab_stake, terminal_stake,
};
use guild_desktop::i18n::{Language, Ui};
use guild_session::HarnessKind;
use guild_session::status::{Status, Why};
use guild_session::{Runs, SessionMode};

/// Decision 1: the question is about a turn that would be lost. A working
/// or waiting agent in a live process is at stake; an idle one, a finished
/// one and a process that has already ended are not — there is nothing
/// left to interrupt in any of them.
#[test]
fn only_an_agent_at_work_in_a_live_process_is_at_stake() {
    let table = [
        (Status::Idle, true, None),
        (Status::Working, true, Some(Stake::Working)),
        (Status::Waiting(Why::Permission), true, Some(Stake::Waiting)),
        (Status::Waiting(Why::Question), true, Some(Stake::Waiting)),
        (Status::Done, true, None),
        (Status::Idle, false, None),
        (Status::Working, false, None),
        (Status::Waiting(Why::Permission), false, None),
        (Status::Waiting(Why::Question), false, None),
        (Status::Done, false, None),
    ];
    for (status, alive, stake) in table {
        assert_eq!(
            terminal_stake(status, alive),
            stake,
            "{status:?} with the process alive: {alive}"
        );
    }
}

/// **A tab with Claude Code in it asks before it closes, whatever its agent
/// is doing** — asked for on 2026-09-16, and a setting of its own, on by
/// default. A turn at stake keeps its own word; a Claude tab between turns
/// is `Open`; a shell, or a Claude whose process has ended, asks nothing
/// new; and with the setting off the table is M5.12's.
#[test]
fn a_claude_tab_is_at_stake_while_it_is_open_when_the_setting_says_so() {
    assert_eq!(tab_stake(None, true, true), Some(Stake::Open));
    assert_eq!(tab_stake(None, true, false), None);
    assert_eq!(
        tab_stake(None, false, true),
        None,
        "a shell or a dead claude"
    );
    for live in [Stake::Working, Stake::Waiting] {
        assert_eq!(tab_stake(Some(live), true, true), Some(live));
        assert_eq!(tab_stake(Some(live), true, false), Some(live));
    }
}

/// **Every agent's tab asks, not only Claude Code's** — 2026-09-28. The
/// rule was written for Claude on 2026-09-16, four days before Codex and
/// opencode could run in a tab, and a Codex tab closed on `cmd-w` without
/// a word. An agent is open while its process lives: by the kind of
/// session the tab was started as, or by the program a shell runs in
/// front of it; a shell with nothing in front is not one.
#[test]
fn any_harness_s_tab_is_an_agent_open() {
    let mode = SessionMode::Terminal;
    for runs in [Runs::Claude(mode), Runs::Codex(mode), Runs::OpenCode(mode)] {
        assert!(agent_open(Some(runs), None, false), "{runs:?}");
        assert!(!agent_open(Some(runs), None, true), "{runs:?} that ended");
    }
    for kind in [
        HarnessKind::Claude,
        HarnessKind::Codex,
        HarnessKind::OpenCode,
    ] {
        assert!(
            agent_open(Some(Runs::Shell), Some(kind), false),
            "{kind:?} in a shell"
        );
    }
    assert!(!agent_open(Some(Runs::Shell), None, false), "a bare shell");
    assert!(
        !agent_open(None, None, false),
        "no record, nothing in front"
    );
}

/// **One line, the title, and it says what the agent is doing** —
/// 2026-09-28. The sheet was a title, the name again with the agent's
/// state, and two sentences about the conversation being kept; the person
/// found it long and asked for a detail only where something is lost for
/// good, and a conversation is not. So the state moved into the question:
/// three stakes, three titles, each with the tab's name in it once.
#[test]
fn the_sheet_is_one_question_with_the_agent_s_state_in_it() {
    for language in Language::ALL {
        let ui = Ui(*language);
        assert_eq!(sheet(ui, &[]), None, "no stake is no sheet");

        let titles: Vec<String> = [Stake::Working, Stake::Waiting, Stake::Open]
            .into_iter()
            .map(|stake| {
                let one = [("fix-review".to_owned(), stake)];
                let title = sheet(ui, &one).expect("one tab at stake is a sheet");
                assert_eq!(title, ui.close_tab_title("fix-review", stake));
                assert_eq!(title.matches("fix-review").count(), 1, "{title}");
                title
            })
            .collect();
        assert!(
            titles[0] != titles[1] && titles[1] != titles[2] && titles[0] != titles[2],
            "{titles:?}"
        );

        for n in [2usize, 5, 21] {
            let staked: Vec<(String, Stake)> = (0..n)
                .map(|i| (format!("tab {i}"), Stake::Working))
                .collect();
            let title = sheet(ui, &staked).expect("several tabs are a sheet");
            assert_eq!(title, ui.close_tabs_title(n));
            assert!(title.contains(&n.to_string()), "{title}");
        }
    }

    // The three Russian plural forms, which is what a count in a title
    // gets wrong first.
    let russian = Ui(Language::Russian);
    assert!(russian.close_tabs_title(2).contains("2 вкладки"));
    assert!(russian.close_tabs_title(5).contains("5 вкладок"));
    assert!(russian.close_tabs_title(21).contains("21 вкладку"));
}

/// **The left neighbour comes forward** — 2026-09-23. It was
/// the strip's last tab, so `cmd-w` on the second of eight jumped to the
/// eighth and went on closing from the far end.
#[test]
fn closing_a_tab_brings_its_left_neighbour_forward() {
    use guild_desktop::closing::neighbour;
    assert_eq!(neighbour(&[1, 2, 3, 4], 2), Some(1));
    assert_eq!(
        neighbour(&[1, 2, 3, 4], 1),
        Some(2),
        "none on the left: the right one"
    );
    assert_eq!(neighbour(&[1, 2, 3, 4], 4), Some(3));
    assert_eq!(neighbour(&[7], 7), None, "the last tab leaves nothing");
    assert_eq!(neighbour(&[1, 2], 9), None, "not in the strip");
}

/// **A divided tab is at stake by its strongest pane, not its first** —
/// M12.8: closing the tab ends every pane, so a shell in the first pane
/// and an agent mid-turn in the second is a tab mid-turn, and an idle
/// agent before a waiting one is a tab waiting on the person. The sheet
/// says what is lost, and the most that is lost is the answer.
#[test]
fn a_divided_tab_is_at_stake_by_its_strongest_pane() {
    assert_eq!(strongest([]), None);
    assert_eq!(strongest([Stake::Open]), Some(Stake::Open));
    assert_eq!(
        strongest([Stake::Open, Stake::Working]),
        Some(Stake::Working)
    );
    assert_eq!(
        strongest([Stake::Open, Stake::Waiting]),
        Some(Stake::Waiting)
    );
    assert_eq!(
        strongest([Stake::Waiting, Stake::Working, Stake::Open]),
        Some(Stake::Working)
    );
    assert_eq!(
        strongest([Stake::Working, Stake::Waiting]),
        Some(Stake::Working),
        "a turn to lose is more than a question to answer"
    );
}

/// **The sheet names the pane whose stake it asks about** — M12.8's
/// second window check, 11.6b: the tab menu's *Close* on a divided tab
/// with its shell focused asked *mid-turn* of the agent's turn and named
/// the shell. The pane at stake is the strongest one; among equals the
/// first the caller lists, which is the focused pane.
#[test]
fn the_sheet_names_the_pane_at_stake_not_the_focused_one() {
    // The focused shell first, the agent mid-turn beside it.
    assert_eq!(
        at_stake([("shell", None), ("agent", Some(Stake::Working))]),
        Some(("agent", Stake::Working))
    );
    // An open agent does not outrank a waiting one after it.
    assert_eq!(
        at_stake([
            ("open", Some(Stake::Open)),
            ("waiting", Some(Stake::Waiting))
        ]),
        Some(("waiting", Stake::Waiting))
    );
    // Two at the same stake: the first listed, the focused one.
    assert_eq!(
        at_stake([
            ("focused", Some(Stake::Working)),
            ("other", Some(Stake::Working))
        ]),
        Some(("focused", Stake::Working))
    );
    assert_eq!(at_stake([("shell", None::<Stake>)]), None);
}
