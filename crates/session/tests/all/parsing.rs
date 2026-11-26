//! Parsing the `claude --output-format stream-json` event stream.
//!
//! The sample in `fixtures/stream.jsonl` was **captured from a live run**,
//! not invented. An invented one would check that the parser matches our
//! idea of the format rather than the format itself. The fields that
//! describe the machine it ran on — the folder, the MCP servers, commands,
//! skills, agents and plugins, the memory and socket paths, a hook's
//! output — were neutralised afterwards; the events and their shape are as
//! captured.

use guild_session::{SessionEvent, parse_line};

fn recorded() -> Vec<SessionEvent> {
    include_str!("../fixtures/stream.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| parse_line(line).unwrap_or_else(|err| panic!("line not parsed: {err}\n{line}")))
        .collect()
}

#[test]
fn init_event_carries_the_session_id_and_the_tools() {
    let init = recorded()
        .into_iter()
        .find_map(|event| match event {
            SessionEvent::Init(init) => Some(init),
            _ => None,
        })
        .expect("the recorded stream contains system/init");

    assert!(
        !init.session_id.is_empty(),
        "--resume is done with it later"
    );
    assert!(!init.cwd.is_empty());
    assert!(!init.model.is_empty());
    assert!(
        !init.tools.is_empty(),
        "the tool list is empty, yet it shows what the session has at hand"
    );
}

#[test]
fn assistant_event_carries_text() {
    let texts: Vec<String> = recorded()
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::Assistant { text, .. } => Some(text),
            _ => None,
        })
        .collect();

    assert!(!texts.is_empty(), "the stream contains model replies");
    assert!(
        texts.iter().any(|text| !text.trim().is_empty()),
        "at least one reply is non-empty"
    );
}

/// `parent_tool_use_id` is what the subtask tree is built from without
/// parsing text. The field is optional: top-level events do not have it.
#[test]
fn parent_tool_use_id_is_preserved_when_present() {
    let line = r#"{"type":"assistant","session_id":"s","uuid":"u","parent_tool_use_id":"toolu_42","message":{"content":[{"type":"text","text":"hello"}]}}"#;

    match parse_line(line).unwrap() {
        SessionEvent::Assistant {
            parent_tool_use_id, ..
        } => assert_eq!(parent_tool_use_id.as_deref(), Some("toolu_42")),
        other => panic!("expected assistant, got {other:?}"),
    }
}

#[test]
fn result_event_carries_cost_and_turns() {
    let result = recorded()
        .into_iter()
        .find_map(|event| match event {
            SessionEvent::Result(result) => Some(result),
            _ => None,
        })
        .expect("the recorded stream contains result");

    assert!(
        result.cost_usd > 0.0,
        "the counter needs the cost: a closed tab must not mean a forgotten session"
    );
    assert!(result.duration_ms > 0);
    assert!(result.turns >= 1);
    assert!(!result.is_error);
}

/// An unknown event type is **not an error**.
///
/// `claude` is updated more often than Guild. A new type must not kill the
/// session: it lands in `Unknown` and is shown raw. Otherwise a CLI update
/// breaks the desktop, and the desktop is the one at fault.
#[test]
fn unknown_event_type_does_not_fail_the_session() {
    let line = r#"{"type":"quantum_teleportation","session_id":"s","payload":{"where":"there"}}"#;

    match parse_line(line).unwrap() {
        SessionEvent::Unknown { raw } => {
            assert_eq!(raw["type"], "quantum_teleportation");
        }
        other => panic!("a new type must land in Unknown, got {other:?}"),
    }
}

/// An event without a `type` field is no reason to fail either: it is
/// still JSON, and showing it raw is more honest than aborting the
/// session.
#[test]
fn event_without_type_becomes_unknown_too() {
    match parse_line(r#"{"session_id":"s"}"#).unwrap() {
        SessionEvent::Unknown { .. } => {}
        other => panic!("expected Unknown, got {other:?}"),
    }
}

/// Non-JSON, however, is an error: it means the output is not an event
/// stream, and pretending everything is fine is not an option. But with a
/// message, not a panic.
#[test]
fn malformed_line_is_an_error_with_a_message_not_a_panic() {
    let err = parse_line("this is not json").unwrap_err();
    let text = err.to_string();

    assert!(!text.is_empty());
    assert!(
        text.contains("this is not json") || text.contains("parse"),
        "the message must give something to go on: {text}"
    );
}

#[test]
fn empty_line_is_an_error_not_silently_skipped() {
    // Skipping empty lines is the caller's business: it knows the stream
    // is line-delimited. The parser knows nothing about the line protocol
    // and must not stay silent about empty input.
    assert!(parse_line("").is_err());
}

/// A permission refusal is an event that must not be lost in `Unknown`.
///
/// Structured provider output must retain the refused tool and reason.
///
/// The sample was captured from a live `--permission-mode manual` run.
#[test]
fn permission_denied_is_recognised_not_swallowed() {
    let line = r#"{"type":"system","subtype":"permission_denied","tool_name":"Write","tool_use_id":"toolu_01DW","message":"Claude requested permissions to write to /tmp/probe.txt, but you haven't granted it yet.","session_id":"s","uuid":"u"}"#;

    match parse_line(line).unwrap() {
        SessionEvent::PermissionDenied {
            tool_name, message, ..
        } => {
            assert_eq!(tool_name, "Write", "it is clear what exactly was refused");
            assert!(
                message.contains("granted"),
                "the text explains the refusal: {message}"
            );
        }
        other => panic!("expected a permission refusal, got {other:?}"),
    }
}
