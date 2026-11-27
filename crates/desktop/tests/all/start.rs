//! The start screen's model: what the rows are, where the one highlight
//! is, and what a key does — M14.5.

use std::time::{Duration, SystemTime};

use guild_desktop::i18n::{Language, Ui};
use guild_desktop::start::{
    AGAIN, Deed, Filter, Fits, Lit, Resuming, Start, fits, hears, read_due, resuming, running,
};
use guild_session::history::Past;
use guild_session::{DefaultTab, HarnessKind, SessionId};

fn past(harness: HarnessKind, id: &str, age: u64) -> Past {
    Past {
        harness,
        id: id.into(),
        title: id.into(),
        touched: SystemTime::UNIX_EPOCH + Duration::from_secs(1000 - age),
    }
}

fn start(rows: Vec<Past>) -> Start {
    let mut start = Start::default();
    start.set_rows(rows);
    start
}

const ALL: [HarnessKind; 3] = HarnessKind::ALL;

#[test]
fn the_highlight_starts_on_the_card_and_enter_starts_the_default() {
    let mut s = start(vec![past(HarnessKind::Claude, "a", 1)]);
    assert_eq!(s.lit, Lit::Card);
    assert_eq!(s.key("enter", &ALL), Some(Deed::Default));
}

#[test]
fn the_arrows_walk_card_then_rows_and_stop_at_the_ends() {
    let mut s = start(vec![
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Codex, "b", 2),
    ]);
    s.key("up", &ALL);
    assert_eq!(s.lit, Lit::Card);
    s.key("down", &ALL);
    s.key("down", &ALL);
    s.key("down", &ALL);
    assert_eq!(s.lit, Lit::Row(1));
    assert_eq!(
        s.key("enter", &ALL),
        Some(Deed::Resume {
            past: past(HarnessKind::Codex, "b", 2),
        })
    );
}

#[test]
fn a_tile_s_key_starts_it_unless_its_harness_is_missing() {
    let mut s = start(vec![]);
    assert_eq!(
        s.key("3", &ALL),
        Some(Deed::Tile {
            tab: DefaultTab::Codex,
        })
    );
    assert_eq!(
        s.key("t", &[]),
        Some(Deed::Tile {
            tab: DefaultTab::Terminal,
        })
    );
    assert_eq!(s.key("2", &[HarnessKind::Claude]), None);
}

#[test]
fn chips_appear_only_when_two_harnesses_have_sessions() {
    assert!(
        start(vec![past(HarnessKind::Claude, "a", 1)])
            .chips()
            .is_empty()
    );
    let s = start(vec![
        past(HarnessKind::Codex, "b", 2),
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Claude, "c", 3),
    ]);
    assert_eq!(
        s.chips(),
        [
            (Filter::All, 3),
            (Filter::Only(HarnessKind::Claude), 2),
            (Filter::Only(HarnessKind::Codex), 1),
        ]
    );
}

#[test]
fn a_chip_filters_and_lights_the_first_row_and_the_side_arrows_step_chips() {
    let mut s = start(vec![
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Codex, "b", 2),
    ]);
    s.choose(Filter::Only(HarnessKind::Codex));
    assert_eq!(
        s.shown().iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["b"]
    );
    assert_eq!(s.lit, Lit::Row(0));
    s.key("left", &ALL);
    assert_eq!(s.filter, Filter::Only(HarnessKind::Claude));
    s.key("left", &ALL);
    assert_eq!(s.filter, Filter::All);
    s.key("left", &ALL);
    assert_eq!(s.filter, Filter::All, "stops at the first");
}

/// A re-read that shortens the list never leaves the highlight on a row
/// that is not there (§6.32 rule 1).
#[test]
fn new_rows_keep_the_highlight_on_something() {
    let mut s = start(vec![
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Claude, "b", 2),
    ]);
    s.key("down", &ALL);
    s.key("down", &ALL);
    s.set_rows(vec![past(HarnessKind::Claude, "a", 1)]);
    assert_eq!(s.lit, Lit::Row(0));
    s.set_rows(vec![]);
    assert_eq!(s.lit, Lit::Card);
}

/// **Final review.** The rows are read again every ten seconds, and a
/// session written elsewhere moves up: the highlight stays on the
/// session the person walked to, not on whichever now sits at its
/// index — `enter` resumes what was picked. Identity is the harness and
/// the id together: the same id under another harness is another row.
#[test]
fn a_re_read_keeps_the_highlight_on_the_same_session() {
    let mut s = start(vec![
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Codex, "b", 2),
        past(HarnessKind::Claude, "c", 3),
    ]);
    s.key("down", &ALL);
    s.key("down", &ALL);
    assert_eq!(s.lit, Lit::Row(1));

    // A session of another harness with b's id, and a new one, above it.
    s.set_rows(vec![
        past(HarnessKind::OpenCode, "new", 0),
        past(HarnessKind::Claude, "b", 0),
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Codex, "b", 2),
        past(HarnessKind::Claude, "c", 3),
    ]);
    assert_eq!(s.lit, Lit::Row(3));
    assert_eq!(
        s.key("enter", &ALL),
        Some(Deed::Resume {
            past: past(HarnessKind::Codex, "b", 2),
        })
    );

    // Gone: the index is clamped, as before.
    s.set_rows(vec![past(HarnessKind::Claude, "a", 1)]);
    assert_eq!(s.lit, Lit::Row(0));
}

/// **Review focus 2.** A tab anywhere holding the session is brought
/// forward — two processes appending to one transcript is the outcome
/// worse than no button.
#[test]
fn an_open_session_is_brought_forward() {
    let id: SessionId = "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    let p = past(HarnessKind::Claude, &id.to_string(), 1);
    assert_eq!(resuming(&p, &[(id, None, true)]), Resuming::Front(id));
    let codex = past(HarnessKind::Codex, "u1", 1);
    let rec: SessionId = "1b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    assert_eq!(
        resuming(&codex, &[(rec, Some("u1".into()), true)]),
        Resuming::Front(rec)
    );
}

/// **Final review.** Only the active workspace is restored at a start, so
/// a record left open in another has no tab yet — and it is running as
/// far as a resume is concerned: its workspace's restore will start it,
/// and a reopen now would start it twice. Once that workspace is
/// restored, the record's word no longer stands for a tab to come: a
/// record open with no tab there is one the restore could not bring
/// back, and nothing runs it. Every harness by the same rule.
#[test]
fn a_record_open_in_a_workspace_not_restored_yet_is_brought_forward() {
    // (in a tab, the record says open, its workspace restored)
    assert!(running(true, false, true));
    assert!(running(true, true, true));
    assert!(running(false, true, false));
    assert!(!running(false, true, true));
    assert!(!running(false, false, false));

    let id: SessionId = "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    let pending = running(false, true, false);
    for (row, theirs) in [
        (past(HarnessKind::Claude, &id.to_string(), 1), None),
        (past(HarnessKind::Codex, "u1", 1), Some("u1".to_owned())),
        (
            past(HarnessKind::OpenCode, "ses_a", 1),
            Some("ses_a".to_owned()),
        ),
    ] {
        assert_eq!(
            resuming(&row, &[(id, theirs, pending)]),
            Resuming::Front(id),
            "{:?}",
            row.harness
        );
    }
}

/// **Review focus 3.** A closed record with Claude's id is reopened, never
/// doubled; a closed Codex record is reopened resuming its own id.
#[test]
fn a_closed_record_is_reopened() {
    let id: SessionId = "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    let p = past(HarnessKind::Claude, &id.to_string(), 1);
    assert_eq!(
        resuming(&p, &[(id, None, false)]),
        Resuming::Reopen {
            record: id,
            conversation: None
        }
    );
    let rec: SessionId = "1b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    let codex = past(HarnessKind::Codex, "u1", 1);
    assert_eq!(
        resuming(&codex, &[(rec, Some("u1".into()), false)]),
        Resuming::Reopen {
            record: rec,
            conversation: Some("u1".into())
        }
    );
}

/// A record of another harness whose own id happens to match is not this
/// row's: Claude matches by the record's id, the others by
/// `harness_session`, and never across.
#[test]
fn a_record_is_matched_by_its_own_harness_rule() {
    let id: SessionId = "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    let claude = past(HarnessKind::Claude, &id.to_string(), 1);
    let other: SessionId = "1b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    assert_eq!(
        resuming(&claude, &[(other, Some(id.to_string()), true)]),
        Resuming::New {
            id: Some(id),
            harness_session: None
        }
    );
    let codex = past(HarnessKind::Codex, &id.to_string(), 1);
    assert_eq!(
        resuming(&codex, &[(id, None, true)]),
        Resuming::New {
            id: None,
            harness_session: Some(id.to_string())
        }
    );
}

#[test]
fn a_session_nobody_has_is_a_new_record() {
    let claude = past(
        HarnessKind::Claude,
        "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d",
        1,
    );
    assert_eq!(
        resuming(&claude, &[]),
        Resuming::New {
            id: Some(claude.id.parse().unwrap()),
            harness_session: None
        }
    );
    let opencode = past(HarnessKind::OpenCode, "ses_a", 1);
    assert_eq!(
        resuming(&opencode, &[]),
        Resuming::New {
            id: None,
            harness_session: Some("ses_a".into())
        }
    );
}

/// A Claude row whose id is not a UUID cannot be a record's id, so it
/// cannot be `--resume`d by one: nothing to make (`start_deed` skips it).
#[test]
fn a_claude_row_that_is_not_a_uuid_makes_nothing() {
    let odd = past(HarnessKind::Claude, "not-a-uuid", 1);
    let id: SessionId = "0b7c5a3e-1d2f-4a5b-8c9d-0e1f2a3b4c5d".parse().unwrap();
    assert_eq!(
        resuming(&odd, &[(id, Some("not-a-uuid".into()), true)]),
        Resuming::New {
            id: None,
            harness_session: None
        }
    );
}

/// The screen's keys are bare, and `cmd` only with `enter`: `cmd-1` and
/// `cmd-t` are the window's own strokes and must reach it — the design's
/// "What a press does". Any other modifier is somebody else's stroke too.
#[test]
fn the_screen_hears_its_bare_keys_and_cmd_enter_and_nothing_else() {
    for key in ["1", "2", "3", "t", "up", "down", "left", "right", "enter"] {
        assert!(hears(key, false, false), "{key}");
    }
    assert!(
        hears("enter", true, false),
        "cmd-enter opens the same session"
    );
    for key in ["1", "2", "3", "t", "up", "down"] {
        assert!(!hears(key, true, false), "cmd-{key} is the window's");
    }
    assert!(!hears("t", false, true), "shift-t, ctrl-t, alt-t");
    assert!(!hears("enter", false, true));
    assert!(!hears("a", false, false));
    assert!(!hears("4", false, false));
    assert!(!hears("escape", false, false));
}

/// A read's answer repaints only when the picture changed — trap 184: a
/// clock that notifies redraws the whole window. The first answer for a
/// folder always does, even an empty one, because it ends the reading
/// state; the same rows again do not.
#[test]
fn an_answer_changes_the_picture_only_when_it_differs() {
    let rows = vec![
        past(HarnessKind::Claude, "a", 1),
        past(HarnessKind::Codex, "b", 2),
    ];
    let mut s = Start::default();
    assert!(s.answered(vec![]), "the first answer ends the reading");
    assert!(s.read);
    assert!(!s.answered(vec![]), "nothing again is nothing new");
    assert!(s.answered(rows.clone()));
    assert_eq!(s.rows, rows);
    s.key("down", &ALL);
    assert!(!s.answered(rows.clone()), "the same rows again");
    assert_eq!(s.lit, Lit::Row(0), "and the highlight stays where it was");
    let mut touched = rows.clone();
    touched[0] = past(HarnessKind::Claude, "a", 0);
    assert!(s.answered(touched), "a session touched since is a new age");
}

/// The first read for a folder is asked at once; then every `AGAIN` while
/// the window is in front, never two at a time — trap 155: the round is a
/// process and a stat per file, priced beside `AGAIN`.
#[test]
fn a_read_is_due_at_once_then_every_again_while_in_front() {
    assert!(read_due(None, false, true));
    assert!(read_due(None, false, false), "the first read does not wait");
    assert!(!read_due(None, true, true), "one read at a time");
    assert!(!read_due(Some(AGAIN / 2), false, true));
    assert!(read_due(Some(AGAIN), false, true));
    assert!(!read_due(Some(AGAIN), true, true));
    assert!(
        !read_due(Some(AGAIN * 6), false, false),
        "a window behind another app does not read again"
    );
}

/// **An idle screen's ages still move** — R6's "the picture changed"
/// includes the rows' age words: rows that come back the same leave
/// `answered` quiet, so `ages` is what says a minute has turned over.
/// One word per shown row, in `shown()`'s order, filter included.
#[test]
fn the_ages_are_the_shown_rows_words_at_a_moment() {
    let ago = |seconds: i64| Ui(Language::English).ago(seconds);
    let mut start = start(vec![
        past(HarnessKind::Claude, "a", 30),
        past(HarnessKind::Codex, "b", 600),
    ]);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    assert_eq!(start.ages(now, ago), ["just now", "10m ago"]);
    // Within the same minute the words do not move, and nothing is drawn.
    assert_eq!(
        start.ages(now + Duration::from_secs(20), ago),
        start.ages(now, ago)
    );
    // Past it, the first row's word does.
    assert_eq!(
        start.ages(now + Duration::from_secs(40), ago),
        ["1m ago", "10m ago"]
    );
    // A row touched after `now` — another machine's clock — is just now.
    assert_eq!(
        start.ages(now - Duration::from_secs(60), ago)[0],
        "just now"
    );
    start.choose(Filter::Only(HarnessKind::Codex));
    assert_eq!(start.ages(now, ago), ["10m ago"]);
}

/// Decision 20: a short place — a pane under an agent — shows the heading,
/// the default card and the tiles; Recent and the foot come with room.
#[test]
fn a_short_place_shows_a_compact_start_screen() {
    let all = |recent, foot| Fits {
        heading: true,
        tiles: true,
        two_rows: false,
        recent,
        foot,
    };
    assert_eq!(fits(600.0, 330.0), all(false, false));
    assert_eq!(fits(600.0, 440.0), all(true, false));
    assert_eq!(fits(600.0, 520.0), all(true, true));
    assert_eq!(Fits::ALL, all(true, true), "before the first measurement");
}

/// **Down to the floor, a part goes rather than being cut** — M12.8's
/// second window check, O2: a pane 213 high and an area 290 high drew the
/// heading and the card and cut the tiles in half, because decision 20's
/// three parts need about 320 and a place may be 160. The heading goes
/// first — the title bar names the project and the branch too — then the
/// tiles, and the default card, which `enter` opens, stays at every
/// height. A narrow place wraps its tiles to a second row, which is more
/// height, so the width is asked as well.
#[test]
fn a_place_under_the_screen_s_height_drops_a_part_rather_than_cutting_it() {
    let parts = |heading, tiles| Fits {
        heading,
        tiles,
        two_rows: false,
        recent: false,
        foot: false,
    };
    let narrow = |heading, tiles| Fits {
        two_rows: true,
        ..parts(heading, tiles)
    };
    // Wide: the tiles stand in one row.
    assert_eq!(fits(600.0, 290.0), parts(false, true), "O2's area");
    assert_eq!(fits(600.0, 213.0), parts(true, false), "O2's pane");
    assert_eq!(fits(600.0, 160.0), parts(false, false), "the floor");
    // Narrow: two rows of tiles need more height for the same parts.
    assert_eq!(fits(320.0, 330.0), narrow(true, false));
    assert_eq!(fits(320.0, 350.0), narrow(false, true));
    assert_eq!(fits(320.0, 420.0), narrow(true, true));
    assert_eq!(fits(320.0, 160.0), narrow(false, false), "the floor");
    // Recent and the foot never come without the parts above them.
    assert_eq!(
        fits(320.0, 440.0),
        Fits {
            heading: true,
            tiles: true,
            two_rows: true,
            recent: true,
            foot: false
        }
    );
}

/// Window check 3: at the 318-point floor the four tiles stood in one
/// row, their names touching the tiles' edges, and in Russian Codex's
/// mark ran under its key. **The rows the height is counted for are the
/// rows drawn**: two rows of two wherever `fits` counted two, from the
/// same width — a wrap that fell where the layout pleased could draw
/// three and one, or one row where two were counted.
#[test]
fn a_narrow_place_draws_its_tiles_in_the_two_rows_it_counted() {
    assert!(fits(318.0, 600.0).two_rows, "the floor");
    assert!(fits(407.0, 600.0).two_rows);
    assert!(!fits(408.0, 600.0).two_rows, "four squares and their gaps");
}

/// Command-Enter is admitted by the keyboard gate but carries no mode into
/// either launch or resume; clicks use the same `take` path.
#[test]
fn command_enter_keeps_the_same_launch_and_resume_deeds() {
    let mut screen = start(vec![past(HarnessKind::Claude, "conversation", 0)]);
    for command in [false, true] {
        assert!(hears("enter", command, false));
        screen.lit = Lit::Card;
        assert_eq!(screen.key("enter", &ALL), Some(Deed::Default));
        screen.lit = Lit::Row(0);
        assert_eq!(
            screen.key("enter", &ALL),
            Some(Deed::Resume {
                past: screen.rows[0].clone()
            })
        );
        assert_eq!(screen.take(Lit::Row(0)), screen.key("enter", &ALL));
    }
}

/// GLD-081: a missing agent's notice says it is not installed and where
/// its own install page is, in both languages.
#[test]
fn a_missing_agent_s_notice_says_where_to_install_it() {
    for (kind, name, page) in [
        (
            HarnessKind::Claude,
            "Claude Code",
            "https://code.claude.com/docs/en/setup",
        ),
        (
            HarnessKind::Codex,
            "Codex",
            "https://github.com/openai/codex",
        ),
        (
            HarnessKind::OpenCode,
            "OpenCode",
            "https://opencode.ai/docs",
        ),
    ] {
        assert_eq!(kind.install_page(), page);
        let english = Ui(Language::English).not_installed(kind);
        assert_eq!(
            english,
            format!("{name} is not installed: install it from {page}")
        );
        let russian = Ui(Language::Russian).not_installed(kind);
        assert_eq!(
            russian,
            format!("{name} не установлен: установите по инструкции {page}")
        );
    }
}
