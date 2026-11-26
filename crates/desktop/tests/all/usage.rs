//! Claude Code's plan limits, as the title bar's chip reads them —
//! M12.10. The two answers are fixtures: the Keychain item's shape with a
//! made-up token, and `oauth/usage`'s shape as task 0 recorded it on
//! 2026-09-25, its figures and nothing of the request.

use std::time::Duration;

use guild_desktop::usage::{self, Level, Window, WindowName};

/// The shape task 0 met, cut to what is read and one key of each kind
/// that is not.
const ANSWER: &str = r#"{
  "five_hour": {"utilization": 11.0, "resets_at": "2026-09-25T12:40:00.224767+00:00"},
  "seven_day": {"utilization": 74.0, "resets_at": "2026-09-29T04:00:00.224790+00:00"},
  "seven_day_opus": null,
  "nimbus_quill": {"utilization": 0.0, "resets_at": null},
  "limits": [
    {"kind": "session", "group": "session", "percent": 11, "severity": "normal",
     "resets_at": "2026-09-25T12:40:00.224767+00:00", "scope": null, "is_active": false},
    {"kind": "weekly_all", "group": "weekly", "percent": 74, "severity": "normal",
     "resets_at": "2026-09-29T04:00:00.224790+00:00", "scope": null, "is_active": true},
    {"kind": "weekly_scoped", "group": "weekly", "percent": 0, "severity": "normal",
     "resets_at": "2026-09-29T04:00:00+00:00",
     "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null},
     "is_active": false},
    {"kind": "something_new", "percent": 5}
  ],
  "spend": {"percent": 0}
}"#;

fn names(windows: &[Window]) -> Vec<WindowName> {
    windows.iter().map(|window| window.name.clone()).collect()
}

/// `limits` says it plainly — task 0: a row per entry, in its order,
/// named by its kind and a scoped one by its model.
#[test]
fn the_limits_array_is_read_a_window_per_entry() {
    let windows = usage::windows(ANSWER).expect("readable");

    assert_eq!(
        names(&windows),
        [
            WindowName::Session,
            WindowName::Week,
            WindowName::WeekOf("Fable".into())
        ]
    );
    let left: Vec<u8> = windows.iter().map(|window| window.left).collect();
    assert_eq!(left, [89, 26, 100], "percent is the share used");
    assert_eq!(
        windows[0].resets_at,
        Some(time::macros::datetime!(2026-09-25 12:40:00.224767 UTC))
    );
}

/// Without `limits`, the per-window keys an older answer had: `five_hour`
/// and `seven_day`, their utilisation the share used.
#[test]
fn without_limits_the_old_keys_are_read() {
    let old = r#"{"five_hour": {"utilization": 25.5, "resets_at": "2026-09-25T12:40:00+00:00"},
                 "seven_day": {"utilization": 120.0, "resets_at": null},
                 "seven_day_opus": null}"#;

    let windows = usage::windows(old).expect("readable");

    assert_eq!(names(&windows), [WindowName::Session, WindowName::Week]);
    let left: Vec<u8> = windows.iter().map(|window| window.left).collect();
    assert_eq!(left, [75, 0], "rounded, and clamped at nothing left");
    assert_eq!(windows[1].resets_at, None);
}

#[test]
fn an_answer_that_is_not_the_form_is_unreadable() {
    assert!(usage::windows("not json").is_err());
    assert!(usage::windows(r#"{"error": {"type": "authentication_error"}}"#).is_err());
}

/// The item Claude Code keeps: the token is inside `claudeAiOauth`.
#[test]
fn the_token_is_read_from_the_keychain_item() {
    let item = r#"{"claudeAiOauth": {"accessToken": "not-a-real-token", "expiresAt": 1}}"#;

    assert_eq!(usage::token(item).as_deref(), Some("not-a-real-token"));
    assert_eq!(usage::token(r#"{"claudeAiOauth": {}}"#), None);
    assert_eq!(usage::token("{}"), None);
    assert_eq!(usage::token("\n"), None);
}

#[test]
fn the_chip_shows_the_lowest_share_left() {
    let windows = usage::windows(ANSWER).unwrap();

    assert_eq!(usage::lowest(&windows), Some(26));
    assert_eq!(usage::lowest(&[]), None);
}

/// The disc's rule — decision 2: none from 50 up, brass under 50, rust
/// under 20.
#[test]
fn the_level_is_plenty_low_or_scarce() {
    assert_eq!(usage::level(100), Level::Plenty);
    assert_eq!(usage::level(50), Level::Plenty);
    assert_eq!(usage::level(49), Level::Low);
    assert_eq!(usage::level(20), Level::Low);
    assert_eq!(usage::level(19), Level::Scarce);
    assert_eq!(usage::level(0), Level::Scarce);
}

/// *resets in 8m*, *2h 10m*, *4d 11h* — the two largest units; under a minute
/// is *1m*, never *0m*.
#[test]
fn a_reset_is_said_in_its_two_largest_units() {
    let at = |seconds: u64| usage::resets_in(Duration::from_secs(seconds));

    assert_eq!(at(8 * 60), "8m");
    assert_eq!(at(2 * 3600 + 10 * 60 + 59), "2h 10m");
    assert_eq!(at(3600), "1h");
    assert_eq!(at(4 * 86400 + 11 * 3600 + 5 * 60), "4d 11h");
    assert_eq!(at(2 * 86400), "2d");
    assert_eq!(at(20), "1m");
}

use guild_desktop::agents::Agent;

/// The chip follows the agent in front; a file or a shell in front
/// keeps the last agent's figure; Claude Code before any — decision 47.
#[test]
fn the_front_agent_is_the_active_one_or_the_last() {
    assert_eq!(
        usage::front(Some(Agent::Codex), Agent::ClaudeCode),
        Agent::Codex
    );
    assert_eq!(
        usage::front(None, Agent::Codex),
        Agent::Codex,
        "a file tab after Codex"
    );
    assert_eq!(
        usage::front(Some(Agent::OpenCode), Agent::Codex),
        Agent::OpenCode
    );
}

/// Each kind wears its harness's mark.
#[test]
fn an_agent_wears_its_harness_mark() {
    use guild_session::DefaultTab;
    assert_eq!(Agent::ClaudeCode.mark(), DefaultTab::Claude);
    assert_eq!(Agent::Codex.mark(), DefaultTab::Codex);
    assert_eq!(Agent::OpenCode.mark(), DefaultTab::OpenCode);
}

/// *All providers* names a window by its length, `5h` and `7d`,
/// in the window's language.
#[test]
fn a_window_has_a_short_name_by_its_length() {
    use guild_desktop::i18n::{Language, Ui};
    let english = Ui(Language::English);
    let russian = Ui(Language::Russian);

    assert_eq!(english.usage_window_short(&WindowName::Session), "5h");
    assert_eq!(english.usage_window_short(&WindowName::Week), "7d");
    assert_eq!(
        english.usage_window_short(&WindowName::WeekOf("Fable".into())),
        "7d · Fable"
    );
    assert_eq!(english.usage_window_short(&WindowName::Span(480)), "8h");
    assert_eq!(russian.usage_window_short(&WindowName::Week), "7д");
    assert_eq!(english.usage_window(&WindowName::Span(2880)), "2d");
}

/// Since 2026-10-01 `oauth/usage` answers 429 to a token asked too often.
/// `Retry-After` is the server's word on when to come back: seconds, or
/// an HTTP date, which is read against the moment given — a date already
/// past is no wait at all.
#[test]
fn retry_after_is_read_as_seconds_or_an_http_date() {
    let now = time::macros::datetime!(2026-10-01 07:24:00 UTC);

    assert_eq!(
        usage::retry_after("120", now),
        Some(Duration::from_secs(120))
    );
    assert_eq!(usage::retry_after(" 7 ", now), Some(Duration::from_secs(7)));
    assert_eq!(
        usage::retry_after("Thu, 01 Oct 2026 07:30:00 GMT", now),
        Some(Duration::from_secs(360))
    );
    assert_eq!(
        usage::retry_after("Thu, 01 Oct 2026 07:00:00 GMT", now),
        Some(Duration::ZERO)
    );
    for nonsense in ["", "soon", "-5", "1.5"] {
        assert_eq!(usage::retry_after(nonsense, now), None, "{nonsense:?}");
    }
}

/// Without a word from the server the pause doubles from the clock's
/// period with every 429 in a row, and stops growing at an hour.
#[test]
fn the_pause_after_a_rate_limit_doubles_and_stops_at_an_hour() {
    let every = Duration::from_secs(5 * 60);
    let minutes = |strikes| usage::rate_limit_pause(every, strikes, None).as_secs() / 60;

    assert_eq!(
        [1, 2, 3, 4, 40].map(minutes),
        [10, 20, 40, 60, 60],
        "the first 429 already waits longer than the clock would"
    );
}

/// A `Retry-After` longer than the pause is honoured whole, the hour or
/// not; a shorter one does not bring the next ask forward.
#[test]
fn a_retry_after_is_honoured_and_never_shortens_the_pause() {
    let every = Duration::from_secs(5 * 60);
    let two_hours = Duration::from_secs(2 * 60 * 60);

    assert_eq!(
        usage::rate_limit_pause(every, 1, Some(two_hours)),
        two_hours
    );
    assert_eq!(
        usage::rate_limit_pause(every, 1, Some(Duration::from_secs(30))),
        Duration::from_secs(10 * 60)
    );
}

/// What Claude Code streams to a status line, cut to what is read — the
/// shape read off 2.1.288: a percentage used and a reset in unix seconds,
/// the five-hour window and the week, nothing per model.
const STATUS_LINE: &str = r#"{
  "session_id": "35e368c2-f1ab-4d87-b178-e1f6504638c6",
  "model": {"display_name": "Opus"},
  "rate_limits": {
    "five_hour": {"used_percentage": 14.4, "resets_at": 1791045000},
    "seven_day": {"used_percentage": 4, "resets_at": 1791608400}
  }
}"#;

#[test]
fn a_status_line_payload_gives_the_session_and_the_week() {
    let payload: serde_json::Value = serde_json::from_str(STATUS_LINE).unwrap();

    let windows = usage::statusline(&payload).expect("it carries limits");

    assert_eq!(names(&windows), [WindowName::Session, WindowName::Week]);
    let left: Vec<u8> = windows.iter().map(|window| window.left).collect();
    assert_eq!(left, [86, 96]);
    assert_eq!(
        windows[0].resets_at,
        Some(time::OffsetDateTime::from_unix_timestamp(1791045000).unwrap())
    );
}

/// A session that has not had an answer from the API yet has no
/// `rate_limits`, and an API-key session never has: nothing to take.
#[test]
fn a_status_line_payload_without_limits_gives_nothing() {
    for payload in [
        r#"{"session_id": "x", "model": {"display_name": "Opus"}}"#,
        r#"{"rate_limits": {}}"#,
        r#"{"rate_limits": {"five_hour": null}}"#,
    ] {
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(usage::statusline(&payload), None, "{payload}");
    }
}
