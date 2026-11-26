//! The plans' limits — Claude Code's since M12.10, decision 46 of the
//! spec; Codex's since M12.12, decision 47, in [`codex`].
//!
//! Claude's are read the person's chosen way:
//! the OAuth token Claude Code keeps in the login Keychain, and the
//! undocumented `https://api.anthropic.com/api/oauth/usage`. **This file is
//! pure**: the two answers read, a window named, the level of a share, a
//! duration said, the agent in front. The Keychain read and the request
//! are `root::usage`, on a thread. **[`codex`] is not**: beside reading
//! Codex's answer it runs `codex app-server` to get one, so that a test can
//! drive the exchange through the crate's public door.
//!
//! **The endpoint is not documented**, so the reading is forgiving: task 0
//! of M12.10 found a `limits` array that names each window plainly, and
//! the older per-window keys beside it; `limits` is read when it is
//! there, the old keys otherwise, and anything unknown is passed over
//! rather than refused.

use std::time::Duration;

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::{Rfc2822, Rfc3339};

pub mod codex;

/// The token inside the Keychain item Claude Code keeps, *Claude
/// Code-credentials*: `{"claudeAiOauth": {"accessToken": …}}`. `None`
/// for anything else. The caller never writes it anywhere.
pub fn token(item: &str) -> Option<String> {
    let item: Value = serde_json::from_str(item.trim()).ok()?;
    item.get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

/// What a window of the plan is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowName {
    /// The five-hour window — *Session*.
    Session,
    /// The seven-day window over everything — *Week*.
    Week,
    /// A seven-day window of one model — *Week · Fable*.
    WeekOf(String),
    /// A window of another length, in minutes — Codex names its windows
    /// only by their length, M12.12.
    Span(u32),
}

/// One window: how much of it is left, and when it starts again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub name: WindowName,
    /// The share left, 0 to 100.
    pub left: u8,
    pub resets_at: Option<OffsetDateTime>,
}

/// The answer was not the form: not JSON, or neither `limits` nor the
/// per-window keys — an error body, most likely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable;

/// The windows an answer of `oauth/usage` names, in its order.
pub fn windows(answer: &str) -> Result<Vec<Window>, Unreadable> {
    let answer: Value = serde_json::from_str(answer).map_err(|_| Unreadable)?;
    if let Some(limits) = answer.get("limits").and_then(Value::as_array) {
        return Ok(limits.iter().filter_map(limit).collect());
    }
    let old: Vec<Window> = [
        ("five_hour", WindowName::Session),
        ("seven_day", WindowName::Week),
    ]
    .into_iter()
    .filter_map(|(key, name)| {
        let window = answer.get(key)?;
        let used = window.get("utilization")?.as_f64()?;
        Some(Window {
            name,
            left: left_of(used),
            resets_at: reset(window),
        })
    })
    .collect();
    if old.is_empty() {
        return Err(Unreadable);
    }
    Ok(old)
}

/// One entry of `limits`: `{kind, percent, resets_at, scope}`, `percent`
/// the share used.
fn limit(entry: &Value) -> Option<Window> {
    let name = match entry.get("kind")?.as_str()? {
        "session" => WindowName::Session,
        "weekly_all" => WindowName::Week,
        "weekly_scoped" => WindowName::WeekOf(
            entry
                .get("scope")?
                .get("model")?
                .get("display_name")?
                .as_str()?
                .to_owned(),
        ),
        _ => return None,
    };
    let used = entry.get("percent")?.as_f64()?;
    Some(Window {
        name,
        left: left_of(used),
        resets_at: reset(entry),
    })
}

/// The windows a status-line payload carries — what Claude Code streams to
/// a `statusLine` command: `rate_limits.five_hour` and `.seven_day`, each
/// `{used_percentage, resets_at}` in unix seconds. The figures of the
/// session's own last answer from the API: nothing per model, which only
/// `oauth/usage` names. `None` while the session has had no answer, and
/// for an API-key session, which never has any.
pub fn statusline(payload: &Value) -> Option<Vec<Window>> {
    let limits = payload.get("rate_limits")?;
    let windows: Vec<Window> = [
        ("five_hour", WindowName::Session),
        ("seven_day", WindowName::Week),
    ]
    .into_iter()
    .filter_map(|(key, name)| {
        let window = limits.get(key)?;
        Some(Window {
            name,
            left: left_of(window.get("used_percentage")?.as_f64()?),
            resets_at: window
                .get("resets_at")
                .and_then(Value::as_i64)
                .and_then(|at| OffsetDateTime::from_unix_timestamp(at).ok()),
        })
    })
    .collect();
    (!windows.is_empty()).then_some(windows)
}

fn left_of(used: f64) -> u8 {
    (100.0 - used).round().clamp(0.0, 100.0) as u8
}

fn reset(window: &Value) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(window.get("resets_at")?.as_str()?, &Rfc3339).ok()
}

/// Whose limits the chip shows — decision 47: the agent in the active
/// tab, else the last one that was in front. The caller starts `last` at
/// Claude Code.
pub fn front(
    active: Option<crate::agents::Agent>,
    last: crate::agents::Agent,
) -> crate::agents::Agent {
    active.unwrap_or(last)
}

/// What the chip shows: the scarcest window's share.
pub fn lowest(windows: &[Window]) -> Option<u8> {
    windows.iter().map(|window| window.left).min()
}

/// How much a share left matters — decision 2: nothing is drawn for
/// plenty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// 50 and up: no disc.
    Plenty,
    /// Under 50: a brass disc.
    Low,
    /// Under 20: a rust disc.
    Scarce,
}

pub fn level(left: u8) -> Level {
    match left {
        50.. => Level::Plenty,
        20.. => Level::Low,
        _ => Level::Scarce,
    }
}

/// *8m*, *2h 10m*, *4d 11h*: the two largest units, a zero second one
/// left out, and never *0m* — under a minute is *1m*.
pub fn resets_in(until: Duration) -> String {
    let minutes = (until.as_secs() / 60).max(1);
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    let pair = |big: u64, big_unit: &str, small: u64, small_unit: &str| {
        if small == 0 {
            format!("{big}{big_unit}")
        } else {
            format!("{big}{big_unit} {small}{small_unit}")
        }
    };
    if days > 0 {
        pair(days, "d", hours, "h")
    } else if hours > 0 {
        pair(hours, "h", minutes, "m")
    } else {
        format!("{minutes}m")
    }
}

/// The longest the pause after a 429 grows to by itself — a
/// `Retry-After` may ask for longer, and is given it.
pub const RATE_LIMIT_CAP: Duration = Duration::from_secs(60 * 60);

/// The `Retry-After` of a 429: whole seconds, or an HTTP date read
/// against `now` — a date already past is no wait. `None` for anything
/// else.
pub fn retry_after(value: &str, now: OffsetDateTime) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    // An HTTP date is RFC 2822's form with `GMT` for the zone, which
    // `time` reads as the obsolete zone it is.
    let at = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    Some(Duration::try_from(at - now).unwrap_or(Duration::ZERO))
}

/// How long to wait after the `strikes`-th 429 in a row: `every` doubled
/// once per strike up to [`RATE_LIMIT_CAP`], or the server's
/// `retry_after` when that is longer — never shorter than either.
pub fn rate_limit_pause(every: Duration, strikes: u32, retry_after: Option<Duration>) -> Duration {
    let doubled = every
        .saturating_mul(1u32.checked_shl(strikes).unwrap_or(u32::MAX))
        .min(RATE_LIMIT_CAP);
    retry_after.map_or(doubled, |after| after.max(doubled))
}
