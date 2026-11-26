//! What a session spent, read from the transcript `claude` keeps.
//!
//! The fixtures are real records with the conversation stripped out: the
//! shape and the numbers are `claude`'s, the words were the author's and
//! have no business in a repository.
//!
//! The sums below were read out of the fixture by hand. A test that
//! computes its own expectation from the code it is checking proves only
//! that the code agrees with itself.
//!
//! `usage-turns.jsonl`, three assistant turns:
//!
//! | field | sum |
//! |---|---|
//! | `input_tokens` | 6 |
//! | `output_tokens` | 2914 |
//! | `cache_read_input_tokens` | 83292 |
//! | `cache_creation_input_tokens` | 36712 |
//! | total | 122924 |

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_session::transcript;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Tokens are on every assistant turn, in every transcript. This is the
/// number the interface can always show.
#[test]
fn tokens_are_summed_over_the_turns() {
    let path = fixture("usage-turns.jsonl");
    let (usage, offset) = transcript::usage_since(&path, 0).expect("the fixture is readable");

    assert_eq!(usage.input_tokens, 6);
    assert_eq!(usage.output_tokens, 2914);
    assert_eq!(usage.cache_read_tokens, 83292);
    assert_eq!(usage.cache_creation_tokens, 36712);
    assert_eq!(usage.total_tokens(), 122_924);

    assert_eq!(
        offset,
        std::fs::metadata(&path).unwrap().len(),
        "the whole file was consumed"
    );
}

/// The trap: `usage` carries an `iterations` array repeating the same
/// numbers. Summing both doubles every turn, and the file looks like it
/// wants to be walked recursively.
///
/// Written out here rather than taken from a fixture so the numbers are
/// visible next to the assertion: one record, a hundred output tokens, and
/// an `iterations` array saying the same hundred.
#[test]
fn the_iterations_array_is_not_counted_twice() {
    let dir = Scratch::new("guild-iterations-");
    let path = dir.join("one-turn.jsonl");

    // The shape is copied from a real transcript; only the numbers are
    // chosen, so that what the test asserts can be read off its input.
    std::fs::write(
        &path,
        concat!(
            r#"{"type":"assistant","message":{"model":"claude-opus-5","usage":{"#,
            r#""input_tokens":2,"output_tokens":100,"#,
            r#""cache_read_input_tokens":30,"cache_creation_input_tokens":40,"#,
            r#""iterations":[{"input_tokens":2,"output_tokens":100,"#,
            r#""cache_read_input_tokens":30,"cache_creation_input_tokens":40}]}}}"#,
            "\n"
        ),
    )
    .unwrap();

    let (usage, _) = transcript::usage_since(&path, 0).expect("readable");

    assert_eq!(usage.output_tokens, 100, "not 200");
    assert_eq!(usage.cache_read_tokens, 30, "not 60");
    assert_eq!(usage.total_tokens(), 172, "2 + 100 + 30 + 40, counted once");
}

/// Money, when the transcript happens to carry it. It does in about one
/// transcript in thirty-five, so it is a bonus rather than a foundation.
#[test]
fn cost_is_taken_when_the_transcript_has_it() {
    let (usage, _) =
        transcript::usage_since(&fixture("usage-with-cost.jsonl"), 0).expect("readable");

    assert_eq!(usage.cost_usd, Some(3.6493195), "the value in the fixture");
    assert_eq!(
        usage.output_tokens, 2914,
        "and the tokens are still counted alongside it"
    );
}

/// Most transcripts have no cost record, and that must read as "unknown"
/// rather than as zero. Zero says the session was free — which is exactly
/// the lie the counter used to tell.
#[test]
fn a_transcript_without_a_cost_record_reports_no_cost() {
    let (usage, _) = transcript::usage_since(&fixture("usage-turns.jsonl"), 0).expect("readable");

    assert_eq!(usage.cost_usd, None);
}

/// The transcript only ever grows and reaches megabytes, so reading it
/// whole on every poll is work nobody asked for. What the window actually
/// does is read the growth: this is that loop, and it must land on the same
/// totals as one reading of the finished file.
#[test]
fn reading_the_growth_matches_reading_it_whole() {
    let dir = Scratch::new("guild-growth-");
    let path = dir.join("growing.jsonl");

    let whole = std::fs::read_to_string(fixture("usage-turns.jsonl")).unwrap();
    let mut lines = whole.lines();
    let first = format!("{}\n", lines.next().unwrap());
    let rest: String = lines.map(|line| format!("{line}\n")).collect();

    // The session has had one turn.
    std::fs::write(&path, &first).unwrap();
    let (mut running, offset) = transcript::usage_since(&path, 0).expect("readable");

    // It has two more, and we read only what arrived.
    std::fs::write(&path, format!("{first}{rest}")).unwrap();
    let (grown, end) = transcript::usage_since(&path, offset).expect("readable");
    running.absorb(grown);

    let (at_once, _) =
        transcript::usage_since(fixture("usage-turns.jsonl").as_path(), 0).expect("readable");

    assert_eq!(running.total_tokens(), at_once.total_tokens());
    assert_eq!(
        running.output_tokens, 2914,
        "the hand-read sum, reached in two goes"
    );
    assert_eq!(end, std::fs::metadata(&path).unwrap().len());
}

/// A running total is replaced, not added. `totalCostUSD` is what the
/// session has cost so far, so absorbing it twice the way tokens are
/// absorbed would bill the person once per reading.
#[test]
fn cost_is_replaced_rather_than_added_up() {
    let (mut running, _) =
        transcript::usage_since(&fixture("usage-with-cost.jsonl"), 0).expect("readable");
    let (again, _) =
        transcript::usage_since(&fixture("usage-with-cost.jsonl"), 0).expect("readable");

    running.absorb(again);

    assert_eq!(
        running.cost_usd,
        Some(3.6493195),
        "still the total, not twice it"
    );
    assert_eq!(
        running.output_tokens, 5828,
        "while tokens, which are an increment, did add up"
    );
}

/// `claude` is writing while we read. A half-written last line must be
/// left for next time rather than parsed or skipped.
#[test]
fn a_half_written_line_is_left_for_next_time() {
    let dir = Scratch::new("guild-partial-");
    let path = dir.join("partial.jsonl");

    let whole = std::fs::read_to_string(fixture("usage-turns.jsonl")).unwrap();
    let first_line_end = whole.find('\n').unwrap() + 1;
    std::fs::write(
        &path,
        format!("{}{}", &whole[..first_line_end], r#"{"type":"assis"#),
    )
    .unwrap();

    let (_, offset) = transcript::usage_since(&path, 0).expect("readable");

    assert_eq!(
        offset, first_line_end as u64,
        "the offset stops at the last complete line, so the half line is read next time"
    );
}

/// A session whose transcript has not appeared yet is not an error: the
/// file arrives when `claude` starts writing.
#[test]
fn a_missing_transcript_is_not_an_error() {
    assert!(transcript::usage_since(Path::new("/no/such/transcript.jsonl"), 0).is_none());
}

/// What a conversation is called, from its own transcript.
///
/// The bug this answers: `claude` writes an OSC title that is **not this
/// session's**. A tab whose transcript had no title at all, and whose first
/// words were "что за проект", was shown as "Rust uninstall workflow" —
/// another conversation entirely. Measured on a running window before this
/// was written.
#[test]
fn a_conversation_is_named_by_its_own_transcript() {
    let dir = Scratch::new("guild-name-");

    let titled = dir.join("titled.jsonl");
    std::fs::write(
        &titled,
        concat!(
            r#"{"type":"ai-title","aiTitle":"Rust uninstall workflow"}"#,
            "\n",
            r#"{"type":"user","message":{"content":"something else entirely"}}"#,
            "\n"
        ),
    )
    .unwrap();
    assert_eq!(
        transcript::name(&titled).as_deref(),
        Some("Rust uninstall workflow"),
        "what claude decided the conversation is about wins"
    );

    let untitled = dir.join("untitled.jsonl");
    std::fs::write(
        &untitled,
        concat!(
            r#"{"type":"assistant","message":{"usage":{"output_tokens":1}}}"#,
            "\n",
            r#"{"type":"user","message":{"content":"что за проект"}}"#,
            "\n"
        ),
    )
    .unwrap();
    assert_eq!(
        transcript::name(&untitled).as_deref(),
        Some("что за проект"),
        "and without one, the first thing the person said"
    );
}

/// Content arrives as a list when there are attachments or tool results.
/// Only the text is a name; a tool result says nothing about the
/// conversation.
#[test]
fn only_the_text_of_a_message_becomes_a_name() {
    let dir = Scratch::new("guild-name-parts-");
    let path = dir.join("parts.jsonl");

    std::fs::write(
        &path,
        concat!(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"},"#,
            r#"{"type":"text","text":"first line\nsecond line"}]}}"#,
            "\n"
        ),
    )
    .unwrap();

    assert_eq!(
        transcript::name(&path).as_deref(),
        Some("first line"),
        "the first line only: a pasted stack trace is not a name"
    );
}

/// A conversation with nothing said in it has no name yet, and the caller
/// falls back to counting.
#[test]
fn a_conversation_with_nothing_in_it_has_no_name() {
    assert_eq!(transcript::name(Path::new("/no/such/file.jsonl")), None);
}

/// **A slash command is named as Claude Code shows it**, not by its
/// markup. The first thing said in a conversation that opens with
/// `/fix-review` is recorded as
/// `<command-message>fix-review</command-message>\n<command-name>/fix-review</command-name>`
/// — measured in a transcript on 2026-09-28 — and the tab was called
/// `command-message>fix-review<`.
#[test]
fn a_slash_command_is_named_as_it_was_typed() {
    let dir = Scratch::new("guild-name-command-");
    let bare = dir.join("bare.jsonl");
    std::fs::write(
        &bare,
        concat!(
            r#"{"type":"user","message":{"content":"<command-message>fix-review</command-message>\n<command-name>/fix-review</command-name>"}}"#,
            "\n"
        ),
    )
    .unwrap();
    assert_eq!(transcript::name(&bare).as_deref(), Some("/fix-review"));

    // Its arguments are what it was asked, so they are part of the name.
    let asked = dir.join("asked.jsonl");
    std::fs::write(
        &asked,
        concat!(
            r#"{"type":"user","message":{"content":"<command-message>resume</command-message>\n<command-name>/resume</command-name>\n<command-args>the last session</command-args>"}}"#,
            "\n"
        ),
    )
    .unwrap();
    assert_eq!(
        transcript::name(&asked).as_deref(),
        Some("/resume the last session")
    );
}

/// **A name already kept is read the same way.** A session is named once,
/// and the ones named before this rule keep the first line of the markup
/// — the message tag alone, without the command's own.
#[test]
fn a_kept_name_of_a_slash_command_is_read_as_the_command() {
    assert_eq!(
        transcript::slash_command("<command-message>fix-review</command-message>").as_deref(),
        Some("/fix-review")
    );
    assert_eq!(transcript::slash_command("fix the review"), None);
}
