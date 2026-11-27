//! The window's half of the plans' limits — M12.10, decision 46; Codex
//! since M12.12, decision 47.
//!
//! [`crate::usage`] reads the answers; here is the asking: Claude Code's
//! token from the Keychain through the system's `security` and a request
//! through `ureq`; Codex's through `codex app-server`, which makes its own
//! request with its own token. Each on a thread of its own, and one clock
//! for both.
//!
//! **Claude's token is never written anywhere.** It is held in memory
//! between requests so the Keychain is not asked every five minutes — a
//! person who answered macOS's prompt with *Allow* rather than *Always
//! Allow* would otherwise be asked again and again — and read afresh only
//! when there is none, or the server refused the one held or answered it
//! with 429. It is not
//! logged, and it is not in any message the thread sends back but the one
//! that hands it over. **Codex's is never read at all.**

use std::ffi::OsStr;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use gpui::Context;

use crate::Root;
use crate::agents::Agent;
use crate::usage::{self, Window, codex};

/// Where Claude Code's limits are asked.
const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
/// The Keychain item Claude Code keeps its OAuth token in.
const ITEM: &str = "Claude Code-credentials";
/// How long a request may take before it is a failure — Claude's and
/// Codex's alike.
const TIMEOUT: Duration = Duration::from_secs(10);
/// How often the figures are asked for while the window is in front.
pub(crate) const EVERY: Duration = Duration::from_secs(5 * 60);
/// How old the figures may be when the card opens before it asks again.
const FRESH: Duration = Duration::from_secs(60);
/// How young the figures a session carried must be to keep the network
/// from being asked — on the clock and on opening; Refresh asks anyway.
/// The clock's own interval, the person's choice of 2026-10-03: a session
/// that talks posts every 15 seconds, and one that has gone quiet for
/// longer than a tick of the clock is no fresher than the network.
pub(crate) const SESSION_FRESH: Duration = EVERY;
/// How old figures of either source are before the card calls them
/// outdated — the person's choice of 2026-10-03.
pub(crate) const OUTDATED: Duration = Duration::from_secs(30 * 60);

/// Where the figures shown came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Source {
    /// Asked: `oauth/usage`, or `codex app-server`.
    #[default]
    Network,
    /// Carried by a session running in a tab: Claude's status line, or a
    /// Codex rollout.
    Session,
}

/// Why the figures could not be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Claude: no Keychain item, no token in it, or the person denied the
    /// read.
    NoCredentials,
    /// Codex: no `codex` on the login shell's `PATH`.
    NotInstalled,
    /// Claude: the server refused the token — Claude Code refreshes its
    /// own, and the next ask reads the Keychain again.
    Refused,
    /// Claude: the server answered 429, with its `Retry-After` when it
    /// gave one.
    RateLimited(Option<Duration>),
    /// The request failed — the network, a timeout, a status, Codex's
    /// `error`.
    Request(String),
    /// The answer was not the form.
    Unreadable,
}

impl Failure {
    /// Whether asking again on the clock is pointless — until Refresh.
    pub(crate) fn stops(&self) -> bool {
        matches!(self, Self::NoCredentials | Self::NotInstalled)
    }

    /// Whether the held token is let go, so the next ask reads the
    /// Keychain. A 429 is one: since 2026-10-01 the server has answered a
    /// token held for days with 429 at the minutes it answered Claude
    /// Code's fresh one, so the Keychain is read again after a 429 as
    /// after a refusal — and only then, never on a tick.
    pub(crate) fn lets_token_go(&self) -> bool {
        matches!(self, Self::Refused | Self::RateLimited(_))
    }
}

impl From<codex::Failure> for Failure {
    fn from(failure: codex::Failure) -> Self {
        match failure {
            codex::Failure::NotInstalled => Self::NotInstalled,
            codex::Failure::Request(said) => Self::Request(said),
            codex::Failure::Unreadable => Self::Unreadable,
        }
    }
}

/// What a thread sends back: the windows or why not, and — Claude's
/// only — the token it used when it read a new one.
struct Fetched {
    outcome: Result<Vec<Window>, Failure>,
    token: Option<String>,
}

/// What the window knows of one provider's limits.
#[derive(Default)]
pub(crate) struct Provider {
    windows: Vec<Window>,
    /// When the figures shown were fetched, or written by the session
    /// that carried them.
    fetched_at: Option<Instant>,
    /// Where they came from.
    source: Source,
    /// When the last try failed, if it was the last thing to happen.
    failed_at: Option<Instant>,
    /// The last failure, while it stands.
    failure: Option<Failure>,
    /// When the last request began.
    asked_at: Option<Instant>,
    answer: Option<Receiver<Fetched>>,
    /// 429s in a row since the last answer.
    strikes: u32,
    /// Not asked on the clock or on opening before this.
    paused_until: Option<Instant>,
}

impl Provider {
    /// The windows, while there are any to show.
    pub(crate) fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// How long ago the figures shown were fetched.
    pub(crate) fn fetched_ago(&self) -> Option<Duration> {
        self.fetched_at.map(|at| at.elapsed())
    }

    /// Where the figures shown came from.
    pub(crate) fn source(&self) -> Source {
        self.source
    }

    /// Whether the last try failed, and it is still the last word — a
    /// session's figures newer than the failure are.
    pub(crate) fn failed(&self) -> bool {
        self.failed_at
            .is_some_and(|failed| self.fetched_at.is_none_or(|fetched| failed > fetched))
    }

    /// Whether the figures shown are old: [`OUTDATED`], or a window whose
    /// reset `now` has passed — its share is not what it says any more.
    pub(crate) fn outdated(&self, now: time::OffsetDateTime) -> bool {
        self.fetched_ago().is_some_and(|ago| ago >= OUTDATED)
            || self
                .windows
                .iter()
                .any(|window| window.resets_at.is_some_and(|at| at <= now))
    }

    /// Whether a session's figures are young enough to stand in for the
    /// network — [`SESSION_FRESH`].
    fn session_fresh(&self) -> bool {
        self.source == Source::Session && self.fetched_ago().is_some_and(|ago| ago < SESSION_FRESH)
    }

    /// Whether a request is under way.
    pub(crate) fn asking(&self) -> bool {
        self.answer.is_some()
    }

    /// The last failure, while it is the last word.
    pub(crate) fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref().filter(|_| self.failed())
    }

    /// Whether it has ever answered — with windows or with none.
    pub(crate) fn answered(&self) -> bool {
        self.fetched_at.is_some()
    }

    /// Whether a 429's pause is not out yet.
    pub(crate) fn paused(&self) -> bool {
        self.paused_until
            .is_some_and(|until| Instant::now() < until)
    }

    /// Due on the clock: never asked, or [`EVERY`] since while the window
    /// is in front — and never after a failure that [`Failure::stops`],
    /// nor inside a 429's pause.
    fn due(&self, window_focused: bool) -> bool {
        if self.failure.as_ref().is_some_and(Failure::stops)
            || self.paused()
            || self.session_fresh()
        {
            return false;
        }
        match self.asked_at {
            None => true,
            Some(asked) => window_focused && asked.elapsed() >= EVERY,
        }
    }

    /// Due when the card opens: its figures older than [`FRESH`] or none,
    /// and neither stopped nor paused — those wait for Refresh, as on the
    /// clock. A card is opened often, and every opening that asked was
    /// one more request to a server that had said to wait.
    fn due_on_opening(&self) -> bool {
        let stopped = self.failure.as_ref().is_some_and(Failure::stops);
        !stopped
            && !self.paused()
            && !self.session_fresh()
            && self.fetched_at.is_none_or(|at| at.elapsed() >= FRESH)
    }

    /// Start `fetch` on a thread, unless a request is under way.
    fn ask(&mut self, fetch: impl FnOnce() -> Fetched + Send + 'static) {
        if self.answer.is_some() {
            return;
        }
        self.asked_at = Some(Instant::now());
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(fetch());
        });
        self.answer = Some(receive);
    }

    /// The thread's answer, if it has come.
    fn receive(&mut self) -> Option<Fetched> {
        let answer = self.answer.as_ref()?;
        match answer.try_recv() {
            Ok(fetched) => {
                self.answer = None;
                Some(fetched)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.answer = None;
                None
            }
        }
    }

    /// Take an outcome in; `who` names the provider in the log line.
    fn take(&mut self, outcome: Result<Vec<Window>, Failure>, who: &str) {
        match outcome {
            Ok(windows) => {
                tracing::info!(
                    "usage: {who}, {} windows, the lowest {}% left",
                    windows.len(),
                    usage::lowest(&windows).unwrap_or(100)
                );
                self.windows = windows;
                self.fetched_at = Some(Instant::now());
                self.source = Source::Network;
                self.failed_at = None;
                self.failure = None;
                self.strikes = 0;
                self.paused_until = None;
            }
            Err(failure) => {
                tracing::info!("usage: {who}, not had — {failure:?}");
                // Figures a session carried are not the network's to clear.
                if failure.stops() && self.source == Source::Network {
                    self.windows.clear();
                }
                // The figures stay: a 429 says nothing about them.
                if let Failure::RateLimited(after) = failure {
                    self.strikes = self.strikes.saturating_add(1);
                    let pause = usage::rate_limit_pause(EVERY, self.strikes, after);
                    tracing::info!("usage: {who}, asking again in {}s", pause.as_secs());
                    self.paused_until = Some(Instant::now() + pause);
                }
                self.failed_at = Some(Instant::now());
                self.failure = Some(failure);
            }
        }
    }
}

impl Provider {
    /// Take figures a session carried, written `at`, when they are newer
    /// than the ones shown. A failure of the network stays — its pause
    /// with it — but stops being the last word.
    fn heard(&mut self, windows: Vec<Window>, at: Instant, who: &str) {
        if self.fetched_at.is_some_and(|shown| shown >= at) {
            return;
        }
        if self.source == Source::Network || self.windows != windows {
            tracing::info!(
                "usage: {who}, {} windows from a session, the lowest {}% left",
                windows.len(),
                usage::lowest(&windows).unwrap_or(100)
            );
        }
        self.windows = windows;
        self.fetched_at = Some(at);
        self.source = Source::Session;
    }
}

/// What the window knows of the plans' limits.
#[derive(Default)]
pub(crate) struct Usage {
    claude: Provider,
    codex: Provider,
    /// Claude's token, held between requests.
    token: Option<String>,
}

impl Usage {
    /// A kind of agent's limits; `None` for OpenCode, which has none.
    pub(crate) fn of(&self, agent: Agent) -> Option<&Provider> {
        match agent {
            Agent::ClaudeCode => Some(&self.claude),
            Agent::Codex => Some(&self.codex),
            Agent::OpenCode => None,
        }
    }

    /// The provider the Refresh row speaks of: the one in front, or
    /// Claude's when that is OpenCode.
    pub(crate) fn shown(&self, agent: Agent) -> &Provider {
        self.of(agent).unwrap_or(&self.claude)
    }

    /// Take Claude's answer in, and the token with it.
    fn take_claude(&mut self, fetched: Fetched) {
        if fetched.token.is_some() {
            self.token = fetched.token;
        }
        if fetched
            .outcome
            .as_ref()
            .err()
            .is_some_and(Failure::lets_token_go)
        {
            // Claude Code refreshes its token; read it afresh.
            self.token = None;
        }
        self.claude.take(fetched.outcome, "claude");
    }
}

impl Root {
    /// The usage card, open or put away — the chip's press. Opening it
    /// asks each provider whose figures are older than a minute, unless it
    /// is stopped.
    pub(crate) fn toggle_usage(&mut self, cx: &mut Context<Self>) {
        let opening = !self.showing_usage;
        self.put_fields_away();
        self.showing_usage = opening;
        if opening && self.usage.claude.due_on_opening() {
            self.ask_claude();
        }
        if opening && self.usage.codex.due_on_opening() {
            self.ask_codex();
        }
        cx.notify();
    }

    /// The agent whose limits the chip shows — decision 47 — and the one
    /// remembered for when a file or a shell is in front.
    pub(crate) fn usage_front(&self) -> Agent {
        let active = self
            .active_tab
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .and_then(|tab| self.tab_agent(tab));
        let front = usage::front(active, self.usage_last.get());
        self.usage_last.set(front);
        front
    }

    /// On the watch loop's clock: take what the sessions carried and the
    /// answers, and ask each provider when it is due — which it is not
    /// while a session's figures are fresh. `window_focused` is the
    /// activation observer's, never a paint's (docs/traps.md).
    pub(crate) fn poll_usage(&mut self, cx: &mut Context<Self>) {
        if !self.integrations.external() {
            return;
        }
        #[cfg(test)]
        self.integrations.assert_external_boundary();
        if self.heard_status_lines() {
            cx.notify();
        }
        self.settle_usage(cx);
        // The last agent in front, recorded here too: the chip and the card
        // record it only while they are drawn, and a tab switched before any
        // figures came would otherwise be missed — decision 47.
        self.usage_front();
        if self.usage.claude.due(self.window_focused) {
            self.ask_claude();
        }
        if self.usage.codex.due(self.window_focused) {
            self.ask_codex();
        }
    }

    /// Ask both now, each unless a request of its own is under way —
    /// Refresh, which asks even after a failure that stopped the clock and
    /// inside a 429's pause: it is the person's own click, rare beside the
    /// clock, and a person who presses it wants to know now. A 429 it
    /// earns only lengthens the pause.
    pub(crate) fn ask_usage(&mut self) {
        self.ask_claude();
        self.ask_codex();
    }

    /// What Guild's status line posted from the Claude sessions in the
    /// tabs: the latest figures each carried, taken as of now — a status
    /// line runs on the session's own answers, and is posted at most
    /// fifteen seconds after one. Whether anything was taken.
    fn heard_status_lines(&mut self) -> bool {
        let posted = self
            .hooks
            .as_ref()
            .map(super::status::HookState::usage)
            .unwrap_or_default();
        let mut taken = false;
        for post in posted {
            if post.get("provider").and_then(serde_json::Value::as_str) != Some("claude") {
                continue;
            }
            let Some(payload) = post.get("payload") else {
                continue;
            };
            if let Some(windows) = usage::statusline(payload) {
                self.usage.claude.heard(windows, Instant::now(), "claude");
                taken = true;
            }
        }
        taken
    }

    /// The limits a Codex tab's rollout recorded, as of the line's own
    /// time. Whether they were taken.
    pub(crate) fn heard_codex_limits(
        &mut self,
        limits: &guild_session::codex_transcript::RateLimits,
    ) -> bool {
        let Some(windows) = codex::rollout(&limits.limits) else {
            return false;
        };
        let Some(at) = written_at(&limits.at, time::OffsetDateTime::now_utc(), Instant::now())
        else {
            return false;
        };
        self.usage.codex.heard(windows, at, "codex");
        true
    }

    fn ask_claude(&mut self) {
        if !self.integrations.external() {
            return;
        }
        let held = self.usage.token.clone();
        self.usage.claude.ask(move || fetch_claude(held));
    }

    fn ask_codex(&mut self) {
        if !self.integrations.external() {
            return;
        }
        // GLD-078: `codex app-server` is not run for a Codex never run
        // here — it is what creates `~/.codex` and its databases. The card
        // shows the agent in front, and a Codex tab in front has a home by
        // then: the next tick asks.
        if !guild_session::harness::known(&guild_session::harness::CodexHarness::home()) {
            return;
        }
        self.usage.codex.ask(|| {
            let path = guild_session::login_path::path();
            Fetched {
                outcome: codex::ask(OsStr::new("codex"), path.as_deref(), TIMEOUT)
                    .map_err(Failure::from),
                token: None,
            }
        });
    }

    fn settle_usage(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(fetched) = self.usage.claude.receive() {
            self.usage.take_claude(fetched);
            changed = true;
        }
        if let Some(fetched) = self.usage.codex.receive() {
            self.usage.codex.take(fetched.outcome, "codex");
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
}

/// The instant an RFC 3339 time was, read against `now` and `clock` — a
/// time ahead of `now` is now. `None` for what is not a time.
fn written_at(at: &str, now: time::OffsetDateTime, clock: Instant) -> Option<Instant> {
    let at =
        time::OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339).ok()?;
    let ago = Duration::try_from(now - at).unwrap_or(Duration::ZERO);
    Some(clock.checked_sub(ago).unwrap_or(clock))
}

/// On the thread: the token held, or the Keychain's; then the request.
/// A refused held token is tried once more with the Keychain's, since
/// Claude Code may have refreshed it since it was read. A 429 is not
/// tried again here — a second request straight after the server said
/// to wait would be the thing it limits; the token is let go, and the
/// next ask after the pause reads the Keychain.
fn fetch_claude(held: Option<String>) -> Fetched {
    let fresh = held.is_none();
    let token = match held.map(Ok).unwrap_or_else(keychain_token) {
        Ok(token) => token,
        Err(failure) => {
            return Fetched {
                outcome: Err(failure),
                token: None,
            };
        }
    };
    match request(&token) {
        Err(Failure::Refused) if !fresh => match keychain_token() {
            Ok(again) if again != token => {
                let outcome = request(&again);
                Fetched {
                    outcome,
                    token: Some(again),
                }
            }
            _ => Fetched {
                outcome: Err(Failure::Refused),
                token: None,
            },
        },
        outcome => Fetched {
            outcome,
            token: fresh.then_some(token),
        },
    }
}

/// The token in Claude Code's Keychain item, through the system's
/// `security` — its stdout is read here and goes nowhere else. macOS asks
/// the person the first time; a *Deny* stops automatic reads. A failure
/// to launch the command is transient and must not disable polling.
fn keychain_token() -> Result<String, Failure> {
    keychain_result(
        Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", ITEM, "-w"])
            .stdin(Stdio::null())
            .output(),
    )
}

fn keychain_result(output: std::io::Result<std::process::Output>) -> Result<String, Failure> {
    let out = output.map_err(|err| Failure::Request(format!("Keychain command: {err}")))?;
    if !out.status.success() {
        return Err(Failure::NoCredentials);
    }
    usage::token(&String::from_utf8_lossy(&out.stdout)).ok_or(Failure::NoCredentials)
}

/// The one request: `GET` with the token as a bearer and the beta header
/// task 0 found the endpoint answering to, under an honest agent.
fn request(token: &str) -> Result<Vec<Window>, Failure> {
    // A status is checked here rather than made an error by ureq, which
    // would keep the status and lose the headers — `Retry-After` with them.
    let network = crate::network::snapshot();
    let config = ureq::Agent::config_builder()
        .proxy(
            crate::network::http_policy(&network)
                .map_err(|_| Failure::Request("Invalid proxy configuration".into()))?,
        )
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build();
    let agent = crate::network::http_agent(config, &network)
        .map_err(|_| Failure::Request("Invalid proxy configuration".into()))?;
    let mut answer = agent
        .get(ENDPOINT)
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", concat!("guild/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json")
        .call()
        .map_err(|err| Failure::Request(err.to_string()))?;
    let retry_after = answer
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| usage::retry_after(value, time::OffsetDateTime::now_utc()));
    if let Some(failure) = status_failure(answer.status().as_u16(), retry_after) {
        return Err(failure);
    }
    let body = answer
        .body_mut()
        .read_to_string()
        .map_err(|err| Failure::Request(err.to_string()))?;
    usage::windows(&body).map_err(|_| Failure::Unreadable)
}

/// What a status says went wrong, or `None` for an answer. The words of
/// any other status are the ones ureq used to give it.
fn status_failure(status: u16, retry_after: Option<Duration>) -> Option<Failure> {
    match status {
        200..=299 => None,
        401 | 403 => Some(Failure::Refused),
        429 => Some(Failure::RateLimited(retry_after)),
        other => Some(Failure::Request(format!("http status: {other}"))),
    }
}

/// The card opening asks a provider on its own terms — the first round of
/// revmux found it asking both whenever either was stale, which asked a
/// stopped one too and put the Keychain prompt up on every click after
/// a *Deny*. `Provider` is this module's, so its tests are here.
#[cfg(test)]
mod tests {
    use super::*;

    /// A startup resource failure must recover on the clock without Refresh;
    /// an actual refusal must not repeatedly prompt for Keychain access.
    #[test]
    fn keychain_launch_failure_retries_but_denied_access_stops() {
        use std::os::unix::process::ExitStatusExt;
        let transient = keychain_result(Err(std::io::Error::from_raw_os_error(24)));
        let mut provider = Provider {
            asked_at: Some(Instant::now() - 2 * EVERY),
            ..Default::default()
        };
        provider.take(transient.map(|_| Vec::new()), "claude");
        assert!(provider.due(true));
        assert!(provider.due_on_opening());

        let refused = keychain_result(Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }));
        provider.take(refused.map(|_| Vec::new()), "claude");
        assert!(!provider.due(true));
        assert!(!provider.due_on_opening());
    }

    #[test]
    fn a_provider_never_answered_is_asked_when_the_card_opens() {
        assert!(Provider::default().due_on_opening());
    }

    #[test]
    fn figures_under_a_minute_old_are_not_asked_again() {
        let provider = Provider {
            fetched_at: Some(Instant::now()),
            ..Default::default()
        };
        assert!(!provider.due_on_opening());
    }

    #[test]
    fn a_stopped_provider_waits_for_refresh() {
        for failure in [Failure::NoCredentials, Failure::NotInstalled] {
            let provider = Provider {
                failure: Some(failure.clone()),
                failed_at: Some(Instant::now()),
                ..Default::default()
            };
            assert!(!provider.due_on_opening(), "{failure:?}");
        }
        let failed = Provider {
            failure: Some(Failure::Request("no answer".into())),
            failed_at: Some(Instant::now()),
            ..Default::default()
        };
        assert!(
            failed.due_on_opening(),
            "a failure that does not stop is tried again"
        );
    }

    /// What a 429 leaves behind: since 2026-10-01 the server has answered
    /// a long-held token with 429 while the Keychain's fresh one was
    /// answered, so a rate limit lets the token go as a refusal does.
    #[test]
    fn a_rate_limit_or_a_refusal_lets_the_held_token_go() {
        for failure in [Failure::RateLimited(None), Failure::Refused] {
            let mut usage = Usage {
                token: Some("held".into()),
                ..Default::default()
            };
            usage.take_claude(Fetched {
                outcome: Err(failure.clone()),
                token: None,
            });
            assert_eq!(usage.token, None, "{failure:?}");
        }
        for failure in [Failure::Request("no answer".into()), Failure::Unreadable] {
            let mut usage = Usage {
                token: Some("held".into()),
                ..Default::default()
            };
            usage.take_claude(Fetched {
                outcome: Err(failure.clone()),
                token: None,
            });
            assert!(usage.token.is_some(), "{failure:?} keeps the token");
        }
    }

    #[test]
    fn a_status_is_read_into_a_failure() {
        let minute = Some(Duration::from_secs(60));
        assert_eq!(status_failure(200, None), None);
        assert_eq!(
            status_failure(429, minute),
            Some(Failure::RateLimited(minute))
        );
        assert_eq!(status_failure(401, None), Some(Failure::Refused));
        assert_eq!(status_failure(403, None), Some(Failure::Refused));
        assert_eq!(
            status_failure(500, None),
            Some(Failure::Request("http status: 500".into()))
        );
    }

    /// Neither the clock nor the card opening asks a rate-limited
    /// provider before its pause is out — the installed Guild asked three
    /// times in six seconds on openings while the server said 429.
    #[test]
    fn a_rate_limited_provider_waits_out_its_pause() {
        let mut provider = Provider {
            asked_at: Some(Instant::now() - 2 * EVERY),
            ..Default::default()
        };
        provider.take(Err(Failure::RateLimited(None)), "claude");
        assert!(!provider.due(true));
        assert!(!provider.due_on_opening());

        provider.paused_until = Some(Instant::now());
        assert!(provider.due(true), "a pause that is out is out");
        assert!(provider.due_on_opening());
    }

    fn session_window(left: u8) -> Window {
        Window {
            name: usage::WindowName::Session,
            left,
            resets_at: None,
        }
    }

    /// Figures a session carried are the primary source: while they are
    /// younger than the clock's interval, neither the clock nor the card
    /// opening asks the network — the person's 5-minute threshold.
    #[test]
    fn fresh_session_figures_keep_the_network_from_being_asked() {
        let mut provider = Provider::default();
        provider.heard(vec![session_window(60)], Instant::now(), "claude");
        assert_eq!(provider.windows(), [session_window(60)]);
        assert_eq!(provider.source(), Source::Session);
        assert!(!provider.due(true), "never asked, but fed");
        assert!(!provider.due_on_opening());

        provider.heard(
            vec![session_window(50)],
            Instant::now() - SESSION_FRESH,
            "claude",
        );
        assert_eq!(
            provider.windows(),
            [session_window(60)],
            "older than what is shown, so not taken"
        );

        let mut stale = Provider::default();
        stale.heard(
            vec![session_window(60)],
            Instant::now() - SESSION_FRESH,
            "claude",
        );
        assert!(stale.due(true), "the fallback, once they are old");
        assert!(stale.due_on_opening());
    }

    /// The network's answer is newer than anything a session said before
    /// it, and takes the figures back.
    #[test]
    fn a_network_answer_after_session_figures_replaces_them() {
        let mut provider = Provider::default();
        provider.heard(vec![session_window(60)], Instant::now(), "claude");
        provider.take(Ok(vec![session_window(40)]), "claude");
        assert_eq!(provider.source(), Source::Network);
        assert_eq!(provider.windows(), [session_window(40)]);
    }

    /// A failure of the network is the last word only until a session
    /// says something newer — then the card speaks of the figures, and
    /// the 429's pause still holds.
    #[test]
    fn session_figures_after_a_failure_supersede_it_but_not_its_pause() {
        let mut provider = Provider::default();
        provider.take(Err(Failure::RateLimited(None)), "claude");
        assert!(provider.failed());
        provider.heard(vec![session_window(60)], Instant::now(), "claude");
        assert!(!provider.failed());
        assert_eq!(provider.failure(), None);
        assert!(provider.paused());
    }

    /// Old figures are shown as old: thirty minutes, or a window whose
    /// reset has already passed, whichever comes first.
    #[test]
    fn figures_are_outdated_after_thirty_minutes_or_a_passed_reset() {
        let now = time::OffsetDateTime::now_utc();
        let mut provider = Provider::default();
        provider.heard(vec![session_window(60)], Instant::now(), "claude");
        assert!(!provider.outdated(now));

        let mut old = Provider::default();
        old.heard(
            vec![session_window(60)],
            Instant::now() - OUTDATED,
            "claude",
        );
        assert!(old.outdated(now));

        let mut reset = Provider::default();
        reset.heard(
            vec![Window {
                resets_at: Some(now - time::Duration::minutes(1)),
                ..session_window(60)
            }],
            Instant::now(),
            "claude",
        );
        assert!(reset.outdated(now));
    }

    /// A rollout line's time becomes an instant as long ago as it was
    /// written; one from the future is now.
    #[test]
    fn a_rollout_time_is_read_as_that_long_ago() {
        let now = time::macros::datetime!(2026-10-03 11:40:00 UTC);
        let clock = Instant::now();
        assert_eq!(
            written_at("2026-10-03T11:38:40.000Z", now, clock),
            Some(clock - Duration::from_secs(80))
        );
        assert_eq!(written_at("2026-10-03T12:00:00Z", now, clock), Some(clock));
        assert_eq!(written_at("yesterday", now, clock), None);
    }

    /// The pause grows with each 429 in a row and ends with an answer.
    #[test]
    fn rate_limits_in_a_row_lengthen_the_pause_and_an_answer_ends_it() {
        let mut provider = Provider::default();
        provider.take(Err(Failure::RateLimited(None)), "claude");
        let first = provider.paused_until.expect("paused");
        provider.take(Err(Failure::RateLimited(None)), "claude");
        let second = provider.paused_until.expect("paused");
        assert!(second - first > EVERY, "ten minutes, then twenty");

        provider.take(Ok(Vec::new()), "claude");
        assert_eq!(provider.paused_until, None);
        assert_eq!(provider.strikes, 0);
    }

    /// A rate limit is not a reason to forget what was read: the card
    /// keeps the last figures and says why they are not newer.
    #[test]
    fn a_rate_limit_keeps_the_figures() {
        let window = Window {
            name: usage::WindowName::Session,
            left: 40,
            resets_at: None,
        };
        let mut provider = Provider::default();
        provider.take(Ok(vec![window.clone()]), "claude");
        provider.take(Err(Failure::RateLimited(None)), "claude");
        assert_eq!(provider.windows(), [window]);
        assert_eq!(provider.failure(), Some(&Failure::RateLimited(None)));
    }
}

#[cfg(test)]
#[path = "usage_layer_regression.rs"]
mod usage_layer_regression;
