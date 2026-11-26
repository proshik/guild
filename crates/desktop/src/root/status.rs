//! What the window's agents are doing, and how the window hears it —
//! M5.10.
//!
//! Terminal witnesses move one [`Board`]: the hooks a terminal session posts
//! to the socket, the Esc or Ctrl-C typed into a terminal tab — an
//! interrupt fires no hook — exact transcript lifecycle events, and the tab in
//! front of a focused window. Everything that shows a status reads the
//! board and nothing else.
//!
//! [`Board`]: guild_session::status::Board

use gpui::Context;
use guild_session::SessionId;
use guild_session::status::{Signal, Status};

use crate::hooks::{self, server::Receiver};
use crate::{Body, Root, Tab};

enum LifecycleEvent {
    Hook(serde_json::Value),
    Interrupted(SessionId),
}

/// The hook package on disk, and the socket listening for it.
pub(crate) struct HookState {
    installed: hooks::Installed,
    receiver: Receiver,
}

impl HookState {
    /// What Guild's status line posted since the last look.
    pub(crate) fn usage(&self) -> Vec<serde_json::Value> {
        self.receiver.usage()
    }
}

impl Root {
    /// Write the hook package and listen for it, once.
    ///
    /// From the constructor, **before the tabs are restored**: a flag is
    /// read once, when a process begins, and a restored `claude` launched
    /// without `--settings` would stay silent for the rest of its life.
    pub(crate) fn start_hooks(&mut self) {
        if !self.integrations.external() {
            return;
        }
        #[cfg(test)]
        self.integrations.assert_external_boundary();
        if self.hooks.is_some() {
            return;
        }
        // Sockets left by windows that were killed rather than quit.
        hooks::sweep(&std::env::temp_dir());
        hooks::sweep(std::path::Path::new("/tmp"));

        // An isolated dev window may use the real harnesses without rewriting
        // their global integration files. Per-session hooks below stay local.
        if std::env::var_os("GUILD_NO_GLOBAL_HOOKS").is_none() {
            // opencode's status lives in opencode's own plugins directory (no
            // per-launch plugin flag in V2). The plugin is a no-op outside a
            // session Guild started, so a failure here means only that opencode's
            // dot will not move — never that the window is broken.
            if let Err(err) = hooks::install_opencode() {
                tracing::warn!(%err, "Guild could not install its status plugin for opencode");
            }
            // Codex's status lives in `~/.codex/config.toml` (no per-launch
            // settings file as Claude's `--settings` is; its hooks have one
            // home, the person's own config). Registered only when
            // Codex is actually on the PATH — a person who has never run it
            // should not acquire a `~/.codex/config.toml` from a window that
            // merely read their disk. The script is a no-op without the socket
            // Guild sets on what it launches, so this is the same guard as the
            // opencode plugin.
            let login_path = guild_session::login_path::path_or_inherited();
            let codex_there = guild_session::HarnessKind::available_on(&login_path)
                .contains(&guild_session::HarnessKind::Codex);
            if codex_there && let Err(err) = hooks::install_codex() {
                tracing::warn!(%err, "Guild could not register its status hooks for codex");
            }
        }

        let started = hooks::install(&hooks::directory()).and_then(|installed| {
            let receiver = Receiver::start(&hooks::socket_path(std::process::id()))?;
            Ok(HookState {
                installed,
                receiver,
            })
        });
        match started {
            Ok(state) => self.hooks = Some(state),
            // Not a notice: the window works without them, and a person
            // who has not asked what their agents are doing does not need
            // to hear that a socket would not open.
            Err(err) => tracing::warn!(%err, "Guild could not listen for its sessions' hooks"),
        }
    }

    /// What a terminal session is launched with, while the window listens.
    ///
    /// `None` when it does not, and then the session is started without
    /// `--settings`: a hook posting to a socket nobody holds costs a
    /// process per event for nothing.
    pub(crate) fn hook_launch(&self) -> Option<guild_session::Hooks> {
        self.hooks.as_ref().map(|state| guild_session::Hooks {
            settings: state.installed.settings.clone(),
            statusline_settings: state.installed.statusline_settings.clone(),
            socket: state.receiver.path().to_path_buf(),
            // Found per launch, by the folder it starts in.
            user_statusline: None,
        })
    }

    /// A prompt went into a session, by its harness's hook: the tab's
    /// input bookkeeping, and — when the hook names this tab's current
    /// runtime — a continuation's turn start, finished after the pass.
    fn heard_prompt_submitted(
        &self,
        session: SessionId,
        payload: &serde_json::Value,
        managed_starts: &mut Vec<(SessionId, gpui::EntityId, u64)>,
        cx: &mut Context<Root>,
    ) {
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::Terminal {
                session: held,
                view,
                ..
            } = body
                && *held == session
            {
                view.update(cx, |v, _| v.prompt_submitted());
                let observed = view.read(cx).runtime_identity();
                if observed.is_some() && observed == payload["guild_runtime"].as_str() {
                    managed_starts.push((session, view.entity_id(), view.read(cx).input_epoch()));
                }
            }
        }
    }

    /// Forget what was said about a session's turn: its displayed status
    /// and the managed-input latch both.
    pub(crate) fn forget_status(&mut self, session: SessionId) {
        self.board.forget(session);
        self.latch.forget(session);
        self.unbound_starts.remove(&session);
    }

    /// Which column groups the agents keep active now, and since when —
    /// a project's place in Recent activity's top block. Answers whether
    /// a group became active or stopped being active.
    ///
    /// An agent session is alive while a pane shows it in a terminal whose
    /// program has not exited and the window counts as an agent's — the
    /// same agents the status bar lists. A session no pane shows is gone.
    pub(crate) fn note_active_groups(&mut self) -> bool {
        let sessions: Vec<_> = self
            .tabs
            .iter()
            .flat_map(Tab::bodies)
            .filter(|body| self.body_agent(body).is_some())
            .filter_map(|body| {
                let session = body.session()?;
                let alive = matches!(body, Body::Terminal { facts, .. } if !facts.exited());
                Some((session, alive, self.board.status(session)))
            })
            .collect();
        let running = self.running.update(sessions);
        let groups = running.iter().filter_map(|session| {
            let record = self.desk.session(*session)?;
            let project = self.desk.project(record.project)?;
            Some((
                record.workspace,
                project.path.as_path(),
                project.repository.as_deref(),
            ))
        });
        self.active_groups
            .update(crate::column::active_groups(groups))
    }

    /// What a session's agent is doing.
    pub(crate) fn status_of(&self, session: SessionId) -> Status {
        self.board.status(session)
    }

    /// Subscribe at ownership admission, not from the status clock or render.
    /// The source owns the subscription; GPUI keeps both entity references weak.
    pub(crate) fn hear_terminal_input(
        view: &gpui::Entity<crate::terminal::view::TerminalTab>,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe(
            view,
            |root, source, event: &crate::terminal::view::InputRefused, cx| {
                // Only an actual refusal checks ownership. Cached/satellite
                // handles can outlive removal from the owning collection.
                if root
                    .terminals()
                    .iter()
                    .any(|view| view.entity_id() == source.entity_id())
                {
                    root.warn(event.0.to_owned());
                    cx.notify();
                }
            },
        );
        view.update(cx, |view, _| view.hear_input_refusals(subscription));
    }

    /// Everything the witnesses have said since the last look, on the
    /// watch loop's clock.
    pub(crate) fn poll_status(&mut self, cx: &mut Context<Root>) {
        let mut changed = false;
        let mut records_changed = false;

        // A terminal tab whose program resumed a known conversation —
        // M13.3. Asked of shells only: a harness tab's resume is heard by
        // its hooks, and a tab being handed over is asked once.
        let resumed: Vec<(crate::TabId, SessionId, guild_session::HarnessKind, String)> = self
            .tabs
            .iter()
            .filter(|tab| !self.handing_over.contains(&tab.id))
            .flat_map(|tab| tab.bodies().map(move |body| (tab.id, body)))
            .filter_map(|(tab, body)| match body {
                Body::Terminal { session, facts, .. } => {
                    let (kind, conversation) = facts.resumed()?;
                    let shell = self.desk.session(*session)?.runs == guild_session::Runs::Shell;
                    shell.then_some((tab, *session, kind, conversation))
                }
                _ => None,
            })
            .collect();
        for (tab, current, kind, conversation) in resumed {
            let found = self.found_for(tab, kind, &conversation);
            match crate::binding::binding(current, found) {
                crate::binding::Bind::To(to) => self.hand_over(tab, current, to, cx),
                other => self.tell_binding(tab, other),
            }
        }

        // What the hooks said. A payload naming no session of this
        // window's is dropped: a `claude` the window has forgotten, or one
        // that inherited the socket's variable without being ours.
        // Order locally witnessed input with locally received hooks. A queued
        // Escape predating a prompt must not cancel that new turn; one arriving
        // after the prompt must still cancel it. Do not reorder all interrupts
        // to one side of the hook batch.
        let mut events: Vec<_> = self
            .hooks
            .as_ref()
            .map(|state| state.receiver.poll_observed())
            .unwrap_or_default()
            .into_iter()
            .map(|(at, payload)| (at, LifecycleEvent::Hook(payload)))
            .collect();
        for body in self.tabs.iter().flat_map(crate::Tab::bodies) {
            if let Body::Terminal { session, view, .. } = body
                && let Some(at) = view.update(cx, |view, _| view.take_interrupt())
            {
                events.push((at, LifecycleEvent::Interrupted(*session)));
            }
        }
        events.sort_by_key(|(at, _)| *at);
        let mut managed_starts = Vec::new();
        for (_, event) in events {
            let payload = match event {
                LifecycleEvent::Hook(payload) => payload,
                LifecycleEvent::Interrupted(session) => {
                    // Esc is no status: the program says what it does next.
                    self.latch.apply(session, Signal::Interrupted);
                    continue;
                }
            };
            // Native Codex subagents share launch variables, never pane identity.
            if payload.get("codex_session").is_some()
                && payload
                    .get("agent_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| !id.is_empty())
            {
                continue;
            }
            // Which tab this process runs in, by what it was started as —
            // M13.3. A payload with none is from a process Guild did not
            // start, and is read as before.
            let mut tab = crate::binding::launched_as(
                payload
                    .get("guild_session")
                    .and_then(serde_json::Value::as_str),
            )
            .map(|launched| crate::binding::resolve(launched, &self.started_as))
            .and_then(|current| {
                self.tabs
                    .iter()
                    .find(|tab| tab.sessions().any(|shown| shown == current))
                    .map(|tab| (tab.id, current))
            });
            // A delayed hook from a prior launch cannot change this runtime's
            // identity, Probe cursor or continuation acknowledgement.
            if let (Some((_, session)), Some(runtime)) = (tab, payload["guild_runtime"].as_str()) {
                let current = self
                    .tabs
                    .iter()
                    .flat_map(Tab::bodies)
                    .find_map(|b| match b {
                        Body::Terminal {
                            session: id, view, ..
                        } if *id == session => view.read(cx).runtime_identity().map(str::to_owned),
                        _ => None,
                    });
                if current.as_deref().is_some_and(|owned| owned != runtime) {
                    continue;
                }
            }
            // The conversation the process names now: a `/resume` inside it
            // names another record's, and the tab becomes that session's.
            if let Some((tab_id, current)) = tab
                && let Some(kind) = self
                    .desk
                    .session(current)
                    .and_then(|record| record.runs.harness())
            {
                let field = match kind {
                    guild_session::HarnessKind::Claude => "session_id",
                    guild_session::HarnessKind::OpenCode => "oc_session",
                    guild_session::HarnessKind::Codex => "codex_session",
                };
                if let Some(conversation) = payload
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| !id.is_empty())
                {
                    let found = self.found_for(tab_id, kind, conversation);
                    match crate::binding::binding(current, found) {
                        crate::binding::Bind::To(to) => {
                            self.rebind(tab_id, current, to, cx);
                            tab = Some((tab_id, to));
                            changed = true;
                        }
                        other @ (crate::binding::Bind::Elsewhere(_)
                        | crate::binding::Bind::OtherSeat(_)) => {
                            self.tell_binding(tab_id, other);
                            self.forget_status(current);
                            continue; // A refused binding cannot overwrite another record's identity.
                        }
                        crate::binding::Bind::Nothing => {}
                    }
                }
            }
            // Identity is independent of status: SessionStart has no turn signal.
            // Only the emitting pane may supply its harness's identity.
            if let Some((_, session)) = tab {
                // The two harnesses that generate their own id name it
                // through the hook, so a reopened project can resume the
                // real conversation — M14. OpenCode posts `oc_session`
                // from its plugin; Codex's script lifts `codex_session`
                // out of the event JSON it sends on stdin. Either is
                // recorded only when it names that session's harness, so
                // a stray field cannot be read as another's.
                for (field, kind) in [
                    ("session_id", guild_session::HarnessKind::Claude),
                    ("oc_session", guild_session::HarnessKind::OpenCode),
                    ("codex_session", guild_session::HarnessKind::Codex),
                ] {
                    let Some(id) = payload
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .filter(|id| !id.is_empty())
                    else {
                        continue;
                    };
                    let matches = self
                        .desk
                        .session(session)
                        .is_some_and(|s| s.runs.harness() == Some(kind));
                    let known = self
                        .desk
                        .session(session)
                        .and_then(|s| s.harness_session.clone());
                    if matches && known.as_deref() != Some(id) {
                        self.desk.record_harness_session(session, id.to_owned());
                        self.conversation_admitted(session);
                        records_changed = true;
                        changed = true;
                    }
                }
            }
            let named: Option<SessionId> = payload
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.parse().ok());
            // Who typed the prompt: a hook is the only witness of that, and
            // the project sort's "last user message" reads it. The emitting
            // pane's record, as for identity.
            if let Some(author) = tab.map(|(_, current)| current).or(named)
                && self.desk.session(author).is_some()
            {
                let authored = self
                    .desk
                    .confirm_user_prompt(author, &payload, activity_now());
                records_changed |= authored;
                changed |= authored;
            }
            // Status is OSC 7501's; a hook still moves managed live input's
            // latch, which only a Codex session has.
            // The pane that emitted it decides whose payload it is: a Codex
            // hook names Codex's own conversation as `session_id`.
            let emitter = tab.map(|(_, current)| current).or(named);
            let harness = emitter
                .and_then(|sid| self.desk.session(sid))
                .and_then(|record| record.runs.harness());
            // A prompt typed into a Claude or OpenCode session is still heard
            // here: the tab's input bookkeeping and a continuation's
            // turn-start acknowledgement read it (open decision 1).
            if let (Some(session), Some(kind)) = (emitter, harness)
                && kind != guild_session::HarnessKind::Codex
            {
                let submitted = match kind {
                    guild_session::HarnessKind::OpenCode => payload["opencode_event"] == "prompt",
                    _ => payload["hook_event_name"] == "UserPromptSubmit",
                };
                if submitted {
                    self.heard_prompt_submitted(session, &payload, &mut managed_starts, cx);
                }
                continue;
            }
            let signal = emitter
                .filter(|_| harness == Some(guild_session::HarnessKind::Codex))
                .and_then(|_| guild_session::harness::CodexHarness.status_from_payload(&payload));
            let Some((session, signal)) = signal else {
                continue;
            };
            // A process Guild started signals its tab's record, whatever
            // conversation it names — M13.3: after `/clear` that is one
            // nobody has a record of, and the tab's dot must still move.
            // The emitting pane, not whichever pane happens to have focus.
            let session = tab.map(|(_, current)| current).unwrap_or(session);
            if self.desk.session(session).is_some() {
                let mut signal = Some(signal);
                if payload
                    .get("codex_session")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                {
                    for body in self.tabs.iter().flat_map(Tab::bodies) {
                        if let Body::Terminal {
                            session: shown,
                            view,
                            ..
                        } = body
                            && *shown == session
                        {
                            signal = view.update(cx, |view, _| {
                                view.codex_hook_signal(&payload, signal.unwrap())
                            });
                            break;
                        }
                    }
                }
                let Some(signal) = signal else {
                    continue;
                };
                // The status shown is OSC 7501's alone; a hook moves only the
                // managed-input latch, and the input bookkeeping below.
                self.latch.apply_payload(session, signal, &payload);
                if signal == Signal::PromptSubmitted {
                    self.heard_prompt_submitted(session, &payload, &mut managed_starts, cx);
                }
            }
        }

        // The exact Codex rollout is a second witness when hooks fall silent.
        // Reconcile after the ordered input/hook batch, so disk duplicates cannot
        // erase a newer prompt following Escape. Its cursor belongs to the
        // terminal, so reopening cannot replay an old completion. Never search by cwd or infer completion from silence.
        let mut rollout_signals = Vec::new();
        // The plan's limits ride on the same pass — the Codex row's
        // primary source since 2026-10-03.
        let mut rollout_limits = Vec::new();
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::Terminal { session, view, .. } = body
                && let Some(record) = self.desk.session(*session)
                && record.runs.harness() == Some(guild_session::HarnessKind::Codex)
                && let Some(conversation) = record.harness_session.as_deref()
            {
                let (signals, limits, started) = view.update(cx, |view, _| {
                    (
                        view.codex_signals(
                            conversation,
                            matches!(self.latch.status(*session), Status::Waiting(_)),
                        ),
                        view.take_codex_rate_limits(),
                        view.take_managed_codex_start(),
                    )
                });
                if started {
                    view.update(cx, |v, _| v.prompt_submitted());
                    managed_starts.push((*session, view.entity_id(), view.read(cx).input_epoch()));
                }
                rollout_signals.extend(signals.into_iter().map(|signal| (*session, signal)));
                rollout_limits.extend(limits);
            }
        }
        for limits in rollout_limits {
            changed |= self.heard_codex_limits(&limits);
        }
        for (session, evidence) in rollout_signals {
            let signal = if evidence.signal == Signal::TurnEnded {
                Signal::TurnCompleted
            } else {
                evidence.signal
            };
            self.latch.apply_payload(session, signal, &evidence.payload);
            if signal == Signal::PromptSubmitted {
                for body in self.tabs.iter().flat_map(Tab::bodies) {
                    if let Body::Terminal {
                        session: held,
                        view,
                        ..
                    } = body
                        && *held == session
                    {
                        view.update(cx, |v, _| v.prompt_submitted());
                    }
                }
            }
        }

        for (session, entity, epoch) in managed_starts {
            self.continuation_began(session, entity, epoch);
        }

        // What the programs reported of themselves — OSC 7501: each
        // terminal's root-record changes, in the order they were written,
        // so a turn that began and ended between two polls is two changes.
        let mut reports = Vec::new();
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::Terminal { session, view, .. } = body {
                let (changes, lost) = view.read(cx).take_status_changes();
                if lost > 0 {
                    tracing::warn!(
                        %session,
                        lost,
                        "a terminal's OSC 7501 changes overflowed between two polls; the oldest were dropped"
                    );
                }
                reports.extend(changes.into_iter().map(|change| (*session, change)));
            }
        }
        for (session, change) in reports {
            let record = change.record.as_ref().and_then(|r| r.reported());
            let outcome = self.board.report(session, record);
            changed |= outcome.changed;
            records_changed |=
                self.desk
                    .observe_reported_activity(session, outcome, activity_now());
            // A turn began: what `guild agent send` and a workflow handoff
            // wait for after typing a prompt — M13 task 9.
            if outcome.began {
                for body in self.tabs.iter().flat_map(Tab::bodies) {
                    if let Body::Terminal {
                        session: held,
                        view,
                        ..
                    } = body
                        && *held == session
                    {
                        view.update(cx, |v, _| v.prompt_submitted());
                    }
                }
                self.prompt_began(session);
            }
            // A turn the program reported finishing is the witness `guild
            // agent send` needs after a restart, as a hook's was.
            if outcome.ended && self.desk.session(session).is_some_and(|r| !r.had_turn) {
                self.desk.mark_turn(session);
                records_changed = true;
                changed = true;
            }
        }

        // What the `guild` command asked — M13. Answered here because
        // every command is a thing the window does, and this is the
        // window: the socket's own thread is holding a connection open
        // and waiting for what comes back.
        let asked = self
            .hooks
            .as_ref()
            .map(|state| state.receiver.commands())
            .unwrap_or_default();
        for asked in asked {
            self.take_command(asked, cx);
        }
        // And a held answer whose patience has run out says so.
        self.answer_settled(cx);

        // A session whose tab is gone has no process left — the pty was
        // the tab's — so it is nobody's to wait for.
        let open: std::collections::HashSet<SessionId> =
            self.tabs.iter().flat_map(Tab::sessions).collect();
        changed |= self.board.retain(|session| open.contains(&session));
        self.latch.retain(|session| open.contains(&session));
        self.unbound_starts.retain(|session| open.contains(session));

        // The tab in front of a focused window is seen — **every pane of
        // it**, M12.8's second window check: each is on the screen, and a
        // finished agent beside a focused shell lends the tab its mark
        // (`stage::loudest`), which would otherwise never clear. **And
        // every area's front tab**, the final review's Minor 1: the other
        // area's front is on the screen as much as a pane beside the
        // focused one. Looking at a waiting agent does not answer it; the
        // board knows that.
        if self.window_focused {
            let fronts = self.fronts_on_screen();
            let seen: Vec<SessionId> = self
                .tabs
                .iter()
                .filter(|tab| fronts.contains(&tab.id))
                .flat_map(|tab| tab.sessions())
                .collect();
            for session in seen {
                changed |= self.board.seen(session);
            }
        }

        // Recent activity's active block follows the board once it has
        // taken this pass's reports, closed tabs and seen answers.
        changed |= self.note_active_groups();

        // Only unread first completions survive retain and the Seen pass.
        // Compare with the previous pass too: stream views can signal between polls.
        let done = self.board.done_completions();
        if guild_session::status::should_chime(&self.done_heard, &done) {
            self.sound_player.play("Glass");
        }
        self.done_heard = done;

        // The Dock counts who needs the person — blocked or finished and
        // unread — and is told only when that number moves: a message to
        // AppKit every tenth of a second would be one for nothing.
        let counts = self.board.counts();
        let waiting = counts.blocked + counts.failed + counts.done;
        if waiting != self.dock_badge {
            self.dock_badge = waiting;
            crate::system::set_dock_badge(waiting);
        }

        if records_changed {
            self.save();
        }
        if changed {
            cx.notify();
        }
    }

    /// Bring the session that needs the person to the front: the blocked
    /// first, then the finished, each longest waiting first; from the last
    /// it goes round again, and it crosses projects. M5.10.
    pub(crate) fn go_to_waiting(&mut self, cx: &mut Context<Root>) {
        let open: Vec<SessionId> = self.tabs.iter().flat_map(Tab::sessions).collect();
        let front = self
            .active_tab
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .and_then(Tab::session);
        let Some(session) = self.board.next_waiting(front, &open) else {
            // A keyboard gesture that refuses says why — the lesson of
            // `Commit` on `cmd-k`, which returned in silence and was
            // reported as a stroke that did not work.
            let text = self
                .ui()
                .text(crate::i18n::Text::NoticeNobodyWaiting)
                .to_owned();
            self.inform(text);
            cx.notify();
            return;
        };
        self.go_to_session(session, cx);
    }
}

#[cfg(any(test, feature = "native-preview"))]
impl Root {
    pub(crate) fn attach_test_hooks(&mut self, directory: &std::path::Path) {
        let installed = hooks::install(directory).unwrap();
        let receiver = Receiver::start(&directory.join("h.sock")).unwrap();
        self.hooks = Some(HookState {
            installed,
            receiver,
        });
    }
}

/// Silent by default, including previews and tests. Only main installs AppKit.
#[derive(Default)]
pub(crate) struct SoundPlayer {
    player: Option<fn(&str)>,
    played: usize,
}

impl SoundPlayer {
    fn play(&mut self, name: &str) {
        if let Some(player) = self.player {
            player(name);
        } else {
            self.played += 1;
        }
    }
}

impl Root {
    /// Install the real window's sound backend. Other roots remain silent.
    pub fn set_sound_player(&mut self, player: fn(&str)) {
        self.sound_player.player = Some(player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

    /// **Recent activity's top block follows the window's agent tabs** — a
    /// session runs from its first turn until it fails, its program exits
    /// or its tab closes, and its project keeps the place it took. Real
    /// terminals; the board is moved as OSC 7501 would move it.
    #[gpui::test]
    fn running_agent_tabs_keep_their_projects_on_top_in_order(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::running_agent_tabs_keep_their_projects_on_top_in_order"
        )) {
            return;
        }
        use guild_session::project_sort::Mode;
        use guild_session::status::{ReportOutcome, Reported};
        cx.executor().allow_parking();
        let dirs = [(); 3].map(|_| tempfile::tempdir().unwrap());
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            desk.set_project_sort(workspace, Mode::RecentActivity);
            let seats = dirs.each_ref().map(|dir| Seat {
                workspace,
                project: desk.open_in(workspace, dir.path()),
            });
            let sessions: Vec<SessionId> = seats
                .iter()
                .zip(&dirs)
                .zip([10, 20, 30])
                .map(|((seat, dir), at)| {
                    let id = desk.add_session(SessionRecord::new(
                        "Agent".into(),
                        *seat,
                        dir.path().into(),
                        Runs::Claude(SessionMode::Terminal),
                    ));
                    let began = ReportOutcome {
                        changed: true,
                        began: true,
                        ended: false,
                    };
                    desk.observe_reported_activity(id, began, at);
                    id
                })
                .collect();
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                for ((seat, dir), session) in seats.iter().zip(&dirs).zip(&sessions) {
                    let mut command = portable_pty::CommandBuilder::new("/bin/sh");
                    command.args(["-c", "sleep 30"]);
                    let look = crate::terminal::view::Look::of(&root.settings, root.theme);
                    let view =
                        crate::terminal::view::open_command(command, dir.path(), look, None, cx)
                            .unwrap();
                    let facts = view.read(cx).facts().clone();
                    let body = Body::Terminal {
                        session: *session,
                        view,
                        facts,
                    };
                    root.adopt(*seat, seat.project, body, cx);
                }
                root
            });
            root.update(cx, |root, cx| {
                root.window_focused = false;
                let [a, b, c] = [0, 1, 2].map(|at| sessions[at]);
                let order = |root: &Root| {
                    root.column_groups_for(workspace)
                        .iter()
                        .map(|group| dirs.iter().position(|dir| dir.path() == group.key).unwrap())
                        .map(|at| ["a", "b", "c"][at])
                        .collect::<Vec<_>>()
                };
                let mut step = |root: &mut Root, session, record: Option<Reported>| {
                    if let Some(session) = session {
                        root.board.report(session, record);
                    }
                    root.poll_status(cx);
                    order(root)
                };
                assert_eq!(
                    step(root, None, None),
                    ["c", "b", "a"],
                    "agent tabs opened without a turn: newest turn first"
                );
                assert_eq!(
                    step(root, Some(a), Some(Reported::Working)),
                    ["a", "c", "b"]
                );
                assert_eq!(
                    step(root, Some(b), Some(Reported::Working)),
                    ["a", "b", "c"]
                );
                root.board.report(a, Some(Reported::Done));
                assert!(root.board.seen(a));
                assert_eq!(root.board.status(a), Status::Idle);
                assert_eq!(
                    step(root, None, None),
                    ["a", "b", "c"],
                    "a finished turn seen is rest between turns: a keeps its place"
                );
                assert_eq!(
                    step(root, Some(a), Some(Reported::Working)),
                    ["a", "b", "c"]
                );
                assert_eq!(
                    step(root, Some(b), Some(Reported::Error)),
                    ["a", "c", "b"],
                    "a failure leaves the block"
                );
                assert_eq!(
                    step(root, Some(c), Some(Reported::Working)),
                    ["a", "c", "b"]
                );
                assert_eq!(
                    step(root, Some(b), Some(Reported::Working)),
                    ["a", "c", "b"],
                    "back after a failure: the end of the block"
                );
                let exited = root
                    .tabs
                    .iter()
                    .flat_map(Tab::bodies)
                    .find_map(|body| match body {
                        Body::Terminal { session, facts, .. } if *session == a => {
                            Some(facts.clone())
                        }
                        _ => None,
                    })
                    .unwrap();
                exited.set_exited();
                assert_eq!(
                    step(root, None, None),
                    ["c", "b", "a"],
                    "a's agent exited: a sorts by its latest turn"
                );
                root.tabs.retain(|tab| tab.session() != Some(c));
                assert_eq!(step(root, None, None), ["b", "c", "a"], "c's tab closed");
                root.tabs.clear();
            });
        });
    }

    /// **A turn that began and ended between two polls reaches the board
    /// as two changes, in order** — OSC 7501. The window polls on its own
    /// clock; the terminal queues every change of the root record, and the
    /// board sees `working` before `done`, so the turn is a turn.
    #[gpui::test]
    fn reports_between_two_polls_reach_the_board_in_order(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::reports_between_two_polls_reach_the_board_in_order"
        )) {
            return;
        }
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let session = desk.add_session(SessionRecord::new(
                "fixture".into(),
                seat,
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let mut command = portable_pty::CommandBuilder::new("/bin/sh");
                command.args([
                    "-c",
                    r"printf '\033]7501;state=working\007\033]7501;state=working:id=sub\007\033]7501;state=done\007'; sleep 20",
                ]);
                let look = crate::terminal::view::Look::of(&root.settings, root.theme);
                let view =
                    crate::terminal::view::open_command(command, dir.path(), look, None, cx)
                        .unwrap();
                let facts = view.read(cx).facts().clone();
                root.adopt(seat, project, Body::Terminal { session, view, facts }, cx);
                root
            });
            let view = root.read(cx).tabs.iter().flat_map(Tab::bodies).find_map(|b| match b {
                Body::Terminal { view, .. } => Some(view.clone()),
                _ => None,
            });
            let view = view.unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while view.read(cx).status_root().map(|r| r.state)
                != Some(crate::terminal::program_status::State::Done)
            {
                assert!(std::time::Instant::now() < until, "the program reported");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            root.update(cx, |root, cx| {
                assert_eq!(root.board.reported(session), None, "not polled yet");
                assert!(!root.desk.session(session).unwrap().had_turn);
                root.poll_status(cx);
                assert_eq!(
                    root.board.reported(session),
                    Some(guild_session::status::Reported::Done)
                );
                assert!(
                    root.desk.session(session).unwrap().had_turn,
                    "working reached the board before done: a turn, not a lone done"
                );
                root.tabs.clear();
            });
        });
    }

    /// **Only a change into `done` nobody is looking at chimes** — the
    /// approved answer to open decision 2. Blocked, error, idle, clear and a
    /// process exit are silent; an interrupt the program follows with idle is
    /// silent, one it follows with done chimes; a seen done is rest and a
    /// repeated done does not bring the mark back; an error stays when seen.
    #[gpui::test]
    fn only_an_unseen_change_into_done_chimes(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::only_an_unseen_change_into_done_chimes"
        )) {
            return;
        }
        use guild_session::status::{Kind, Reported};
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let id = desk.add_session(SessionRecord::new(
                "Agent".into(),
                seat,
                dir.path().into(),
                Runs::Codex(SessionMode::Terminal),
            ));
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let body = Body::Broken {
                    session: id,
                    reason: "fixture".into(),
                };
                let tab = root.adopt(seat, project, body, cx);
                root.active_tab = Some(tab);
                root
            });
            root.update(cx, |root, cx| {
                root.window_focused = false;
                fn step(
                    root: &mut Root,
                    id: SessionId,
                    cx: &mut Context<Root>,
                    record: Option<Reported>,
                ) -> usize {
                    root.board.report(id, record);
                    root.poll_status(cx);
                    root.sound_player.played
                }
                for record in [
                    Some(Reported::Working),
                    Some(Reported::Blocked(Some(Kind::Permission))),
                    Some(Reported::Blocked(None)),
                    Some(Reported::Error),
                    Some(Reported::Idle),
                    None,
                ] {
                    assert_eq!(step(root, id, cx, record), 0, "{record:?} is silent");
                }
                // Esc changes nothing shown; what the program reports after it does.
                step(root, id, cx, Some(Reported::Working));
                assert_eq!(
                    step(root, id, cx, Some(Reported::Idle)),
                    0,
                    "interrupted, then idle"
                );
                step(root, id, cx, Some(Reported::Working));
                assert_eq!(
                    step(root, id, cx, Some(Reported::Done)),
                    1,
                    "interrupted, then done"
                );
                assert_eq!(root.board.status(id), Status::Done, "unread");
                assert_eq!(step(root, id, cx, Some(Reported::Done)), 1, "a repeat");
                root.window_focused = true;
                root.poll_status(cx);
                assert_eq!(root.board.status(id), Status::Idle, "seen: rest");
                root.window_focused = false;
                assert_eq!(step(root, id, cx, Some(Reported::Done)), 1);
                assert_eq!(root.board.status(id), Status::Idle, "the mark stays gone");
                step(root, id, cx, Some(Reported::Working));
                step(root, id, cx, Some(Reported::Error));
                root.window_focused = true;
                root.poll_status(cx);
                assert_eq!(
                    root.board.status(id),
                    Status::Failed,
                    "an error stays when seen"
                );
                assert_eq!(root.sound_player.played, 1);
                root.tabs.clear();
            });
        });
    }

    /// **A background program's finished turn chimes once and is unread
    /// until seen — end to end**, from the bytes it writes: `working`, then
    /// `done`, in a tab of an unfocused window.
    #[gpui::test]
    fn a_reported_turn_chimes_once_and_is_unread_until_seen(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::a_reported_turn_chimes_once_and_is_unread_until_seen"
        )) {
            return;
        }
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let session = desk.add_session(SessionRecord::new(
                "fixture".into(),
                seat,
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let mut command = portable_pty::CommandBuilder::new("/bin/sh");
                command.args([
                    "-c",
                    r"printf '\033]7501;state=working\007'; sleep 0.3; printf '\033]7501;state=done\007'; sleep 20",
                ]);
                let look = crate::terminal::view::Look::of(&root.settings, root.theme);
                let view =
                    crate::terminal::view::open_command(command, dir.path(), look, None, cx)
                        .unwrap();
                let facts = view.read(cx).facts().clone();
                let tab = root.adopt(seat, project, Body::Terminal { session, view, facts }, cx);
                root.active_tab = Some(tab);
                root
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let done = root.update(cx, |root, cx| {
                    root.window_focused = false;
                    root.poll_status(cx);
                    root.board.status(session) == Status::Done
                });
                if done {
                    break;
                }
                assert!(std::time::Instant::now() < until, "the turn finished");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            root.update(cx, |root, cx| {
                assert_eq!(root.sound_player.played, 1);
                assert!(root.desk.session(session).unwrap().had_turn);
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 1, "once");
                root.window_focused = true;
                root.poll_status(cx);
                assert_eq!(root.board.status(session), Status::Idle, "seen");
                assert_eq!(root.sound_player.played, 1);
                root.tabs.clear();
            });
        });
    }

    /// **A tab rebound to another session carries the terminal's status
    /// over** — review r1, finding 1. `/resume` inside the program makes the
    /// tab another session's; the program's record stands, and a repeat of
    /// it is no change in the terminal, so the new session must start from
    /// it — as a fact: no sound, no unread mark, no turn.
    #[gpui::test]
    fn a_rebound_session_starts_from_the_terminal_s_record(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::a_rebound_session_starts_from_the_terminal_s_record"
        )) {
            return;
        }
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let record = |desk: &mut Desk| {
                desk.add_session(SessionRecord::new(
                    "fixture".into(),
                    seat,
                    dir.path().into(),
                    Runs::Claude(SessionMode::Terminal),
                ))
            };
            let from = record(&mut desk);
            let to = record(&mut desk);
            let pid_file = dir.path().join("go");
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let mut command = portable_pty::CommandBuilder::new("/bin/sh");
                command.args([
                    "-c",
                    r"printf '\033]7501;state=working\007\033]7501;state=done\007'; while [ ! -e go ]; do sleep 0.05; done; printf '\033]7501;state=done\007'; sleep 20",
                ]);
                let look = crate::terminal::view::Look::of(&root.settings, root.theme);
                let view =
                    crate::terminal::view::open_command(command, dir.path(), look, None, cx)
                        .unwrap();
                let facts = view.read(cx).facts().clone();
                root.adopt(seat, project, Body::Terminal { session: from, view, facts }, cx);
                root
            });
            let tab = root.read(cx).tabs[0].id;
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let done = root.update(cx, |root, cx| {
                    root.poll_status(cx);
                    root.board.reported(from) == Some(guild_session::status::Reported::Done)
                });
                if done {
                    break;
                }
                assert!(std::time::Instant::now() < until, "the program reported");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            root.update(cx, |root, cx| {
                let played = root.sound_player.played;
                let turned = root.desk.session(to).unwrap().had_turn;
                root.rebind(tab, from, to, cx);
                std::fs::write(&pid_file, "").unwrap();
                std::thread::sleep(std::time::Duration::from_millis(300));
                root.poll_status(cx);
                assert_eq!(root.board.observed_status(to), "idle", "a done nobody watched end");
                assert_eq!(root.sound_player.played, played, "no sound for a fact");
                assert_eq!(root.desk.session(to).unwrap().had_turn, turned, "no turn");
                root.tabs.clear();
            });
        });
    }

    #[gpui::test]
    fn chime_poll_observes_focus_coalesces_and_remembers_seen(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::chime_poll_observes_focus_coalesces_and_remembers_seen"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let ids: Vec<_> = (0..2)
                .map(|_| {
                    desk.add_session(SessionRecord::new(
                        "Agent".into(),
                        seat,
                        dir.path().into(),
                        Runs::Claude(SessionMode::Terminal),
                    ))
                })
                .collect();
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                for id in &ids {
                    let body = Body::Broken {
                        session: *id,
                        reason: "fixture".into(),
                    };
                    let tab = root.adopt(seat, project, body, cx);
                    root.active_tab = Some(tab);
                }
                root
            });
            root.update(cx, |root, cx| {
                root.window_focused = false;
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 0, "restoration is silent");
                for id in &ids {
                    root.board
                        .report(*id, Some(guild_session::status::Reported::Done));
                }
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 1, "one sound per pass");
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 1, "unchanged Done is silent");
                root.window_focused = true;
                root.poll_status(cx);
                assert_eq!(root.board.status(ids[1]), Status::Idle);
                root.window_focused = false;
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Done));
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 1, "late completion after Seen");
                root.window_focused = true;
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Working));
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Done));
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 1, "seen before chime decision");
                root.window_focused = false;
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Working));
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Done));
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 2, "front tab of unfocused window");
                root.window_focused = true;
                root.board
                    .report(ids[0], Some(guild_session::status::Reported::Working));
                root.board
                    .report(ids[0], Some(guild_session::status::Reported::Done));
                root.poll_status(cx);
                assert_eq!(
                    root.sound_player.played, 3,
                    "background tab in focused window"
                );
                root.board
                    .report(ids[0], Some(guild_session::status::Reported::Working));
                root.board
                    .report(ids[0], Some(guild_session::status::Reported::Done));
                root.tabs
                    .retain(|tab| !tab.sessions().any(|id| id == ids[0]));
                root.poll_status(cx);
                assert_eq!(root.sound_player.played, 3, "forgotten before decision");
            });
        });
    }
}

#[cfg(test)]
mod navigation_batch_tests {
    use crate::hooks::server::Asked;
    use gpui::{AppContext, TestAppContext};
    use guild_session::command::{Answer, Command};
    use std::sync::mpsc;

    #[test]
    fn isolated_workspace_command_batch() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::status::navigation_batch_tests::poll_status_records_each_workspace_command",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "run through isolated_workspace_command_batch"]
    fn poll_status_records_each_workspace_command(cx: &mut TestAppContext) {
        let hooks = tempfile::tempdir().unwrap();
        let mut entity = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let first = desk.active_workspace();
            let second = desk.add_workspace("Second", None).unwrap();
            let third = desk.add_workspace("Third", None).unwrap();
            let root = cx.new(|cx| {
                let mut r = crate::Root::from_state(Default::default(), desk, cx);
                r.isolated_render = true;
                r.restored_workspaces.extend([first, second, third]);
                r.attach_test_hooks(hooks.path());
                r
            });
            root.update(cx, |r, cx| {
                // Inject at the real receiver boundary; poll_status drains both
                // commands in one update, without a render or watch sample.
                let mut batch = Vec::new();
                let mut replies = Vec::new();
                for workspace in [second, third] {
                    let (answer, reply) = mpsc::channel();
                    replies.push(reply);
                    batch.push(Asked {
                        command: Command::WorkspaceSelect {
                            workspace: workspace.to_string(),
                        },
                        answer,
                    });
                }
                r.hooks.as_mut().unwrap().receiver =
                    crate::hooks::server::command_batch_fixture(hooks.path(), batch);
                r.poll_status(cx);
                for reply in replies {
                    assert!(matches!(
                        reply.recv().unwrap(),
                        Answer::WorkspaceSelected { .. }
                    ));
                }
                assert_eq!(r.active_workspace, third);
                r.go_back(window, cx);
                assert_eq!(r.active_workspace, second);
                r.go_back(window, cx);
                assert_eq!(r.active_workspace, first);
                r.go_forward(window, cx);
                assert_eq!(r.active_workspace, second);
            });
            entity = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        visual.run_until_parked();
        visual.update(|_, cx| entity.unwrap().update(cx, |r, cx| r.stop_every_process(cx)));
    }
}

/// Local receipt time, never a Git timestamp or a transcript file's mtime.
fn activity_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
