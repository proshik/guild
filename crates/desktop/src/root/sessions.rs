//! Watching what the sessions are doing, and recording it.

use gpui::Context;

use crate::Body;
use crate::PROBE;
use crate::Root;
use crate::closed::ClosedTab;
use crate::paths;
use guild_session::Conversation;
use guild_session::Resumption;
use guild_session::SessionId;
use guild_session::continuation;
use guild_session::resumption;
use guild_session::transcript;
use std::time::Instant;

impl Root {
    /// Reopens preserve this session's captured policy without changing global settings.
    pub(crate) fn session_launch_settings(&self, id: SessionId) -> guild_session::Settings {
        let mut settings = self.settings.clone();
        if let Some(policy) = self.desk.session(id).and_then(|r| r.launch_permission) {
            settings.permission_mode = policy;
        }
        settings
    }

    /// Take from the live views what must survive a tab closing.
    ///
    /// **On the watch loop's clock, never in `render`.** It was called at
    /// the top of every frame, and it read session views and updated
    /// every terminal — which GPUI records as the window depending on
    /// them, so any tab's output redrew the whole window, the tabs nobody
    /// could see included. A read outside a draw ties nothing. What a
    /// terminal runs in front is asked on the terminal's own clock now,
    /// and the window hears of it through `terminal::facts`.
    pub(crate) fn sync_from_views(&mut self, cx: &mut Context<Root>) {
        let changed = self.probe_terminal_turns();

        // Whether a finished turn has been seen is the board's question
        // since M5.10, and `poll_status` asks it for every kind of tab.

        // A topic, a turn or a cost is drawn — by the strip, the agents
        // list, the status bar — and nothing else redraws for it now that
        // this is off the frame.
        if changed {
            self.save();
            cx.notify();
        }
    }

    /// Notice that a terminal tab has had a turn.
    ///
    /// A turn is visible from the transcript file, because the pty's output is
    /// characters. Checked while work is going on rather than at closing:
    /// closing is sometimes a crash, and then the session would look empty
    /// and be forgotten.
    pub(crate) fn probe_terminal_turns(&mut self) -> bool {
        let projects = paths::claude_projects_dir();
        let mut turns = Vec::new();
        let mut spending = Vec::new();
        let mut names = Vec::new();

        for tab in &mut self.tabs {
            // Every terminal pane of the tab — M12.8: a split tab watches
            // one session per pane.
            let terminals: Vec<SessionId> = tab
                .bodies()
                .filter_map(|body| match body {
                    Body::Terminal { session, .. } => Some(*session),
                    _ => None,
                })
                .collect();
            if terminals.is_empty() {
                continue;
            }
            // The throttle applies to every terminal tab. It used to be
            // tangled with `had_turn`, which stopped the polling for good
            // the moment a turn was seen — right for that question, wrong
            // for spending, which keeps arriving afterwards.
            if tab.last_probe.elapsed() < PROBE {
                continue;
            }
            for session in terminals {
                let Some(record) = self.desk.session(session) else {
                    continue;
                };
                tab.last_probe = Instant::now();
                if record.runs.harness() != Some(guild_session::HarnessKind::Claude) {
                    continue;
                }

                let id = session.to_string();

                if !record.had_turn && transcript::has_conversation(&projects, &id) {
                    turns.push(session);
                }

                // Read only what was appended. A transcript reaches
                // megabytes, and this runs every couple of seconds per
                // open tab.
                let Some(path) = transcript::path(&projects, &id) else {
                    continue;
                };

                // Until the conversation has a name — and only until
                // then: `transcript::name` answers a `/rename` or
                // `claude`'s own `aiTitle` when the file has one, and the
                // person's first words when it does not. This runs every
                // couple of seconds and the `user` record comes before
                // the `ai-title`, so a live tab usually settles on the
                // first words, and a title written after them does not
                // reach it. The start screen's rows read the titles
                // fresh.
                if record.topic.is_none()
                    && let Some(topic) = transcript::name(&path)
                {
                    names.push((session, topic));
                }

                if let Some((usage, offset)) =
                    transcript::usage_since(&path, record.transcript_offset)
                    && offset > record.transcript_offset
                {
                    spending.push((session, usage, offset));
                }
            }
        }

        let changed = !turns.is_empty() || !spending.is_empty() || !names.is_empty();
        for (id, topic) in names {
            self.desk.name_session(id, topic);
        }
        for id in turns {
            self.desk.mark_turn(id);
        }
        for (id, usage, offset) in spending {
            self.desk.record_usage(id, usage, offset);
        }
        changed
    }

    /// Put away the session a closing tab or pane was watching, and
    /// remember it with `area` — where it was closed from — so that
    /// `cmd-shift-t` brings it back there (M12.8).
    pub(crate) fn retire_session(&mut self, id: SessionId, area: Option<guild_session::AreaId>) {
        self.started_as.retain(|_, now| *now != id);
        if self
            .desk
            .session(id)
            .is_some_and(|r| r.runs.harness() == Some(guild_session::HarnessKind::Claude))
            && transcript::has_conversation(&paths::claude_projects_dir(), &id.to_string())
        {
            self.desk.mark_turn(id);
        }

        // What it takes to bring this tab back, before the record is
        // gone: a shell is forgotten below, and after that there is
        // nothing left to say what was closed.
        let Some(record) = self.desk.session(id) else {
            return;
        };
        let seat = guild_session::Seat {
            workspace: record.workspace,
            project: record.project,
        };
        let (workspace, closing) = (record.workspace, ClosedTab::closing(record));
        let forgotten = closing.session().is_none();
        // A tab of a workspace not restored is on disk alone (GLD-058).
        self.forget_behind(seat, id);
        self.closed.push_from(workspace, area, closing);

        // A closed shell is forgotten at once rather than living until
        // the next restart: there is nowhere to return to, and a "no
        // conversation" row in the list would promise otherwise.
        if forgotten {
            self.desk.remove_session(id);
        } else {
            self.desk.close_session(id);
        }
    }

    /// How to bring a record back into a tab — the one question a
    /// restart's restore and every reopen ask (`resumption` answers it):
    /// `Some(conversation)` to run, `Some(None)` for a shell, which comes
    /// back new, and `None` when nothing can revive it and the record is
    /// forgotten.
    ///
    /// For Claude the disk decides, not the `had_turn` flag: that flag is
    /// set while the application is watching, and the window may have
    /// been closed earlier.
    ///
    /// The forgetting cost a dead tab. `claude` creates the transcript
    /// file as soon as a session starts, before the first message; it
    /// considers such an id taken (`Error: Session ID … is already in
    /// use`), yet there is nothing to continue there (`No conversation
    /// found`). Nothing can revive it, so the record is forgotten.
    pub(crate) fn continue_or_forget(&mut self, id: SessionId) -> Option<Option<Conversation>> {
        if let Some(conversation) = self.desk.session(id).and_then(|record| {
            crate::hooks::identity::recover(&crate::hooks::identity::directory(), record)
        }) {
            // A hook cannot transfer another record's conversation or provenance.
            let held = self.desk.sessions().any(|record| {
                record.id != id
                    && record.runs.harness() == Some(guild_session::HarnessKind::Codex)
                    && record.harness_session.as_deref() == Some(conversation.as_str())
            });
            if !held {
                self.desk.record_harness_session(id, conversation);
                // The caller saves after restoring the complete tab layout.
            }
        }
        let record = self.desk.session(id)?;
        let key = id.to_string();
        let claude = || {
            let projects = paths::claude_projects_dir();
            continuation(
                transcript::has_conversation(&projects, &key),
                transcript::is_id_taken(&projects, &key),
            )
        };

        match resumption(record.runs, &key, record.harness_session.as_deref(), claude) {
            Resumption::Continue(conversation) => Some(Some(conversation)),
            Resumption::Fresh | Resumption::Unidentified => Some(None),
            Resumption::Forget => {
                tracing::info!(session = %key, "session cannot be restored, forgetting it");
                self.desk.remove_session(id);
                None
            }
        }
    }

    /// Keep the unidentified record, but let this pane choose from history.
    pub(crate) fn choose_conversation(
        &mut self,
        tab: crate::TabId,
        pane: guild_session::PaneId,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let Some(open) = self.tabs.iter().find(|open| open.id == tab) else {
            return;
        };
        let Some(Body::Broken { session, .. }) = open.panes.body_of(pane) else {
            return;
        };
        let session = *session;
        let area = open.area;
        if !self.desk.session(session).is_some_and(|r| {
            matches!(r.runs, guild_session::Runs::Codex(_))
                && r.harness_session.as_deref().is_none_or(str::is_empty)
        }) {
            return;
        }
        self.focus_area(area, cx);
        self.focus_pane(tab, pane, cx);
        self.tabs
            .iter_mut()
            .find(|open| open.id == tab)
            .unwrap()
            .panes
            .take(pane);
        self.retire_session(session, Some(area));
        self.poll_start(cx);
        self.focusing_front = true;
        self.refocus(window, cx);
        self.save();
        cx.notify();
    }

    /// How many sessions are running right now — every pane's.
    pub fn running_sessions(&self) -> usize {
        self.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter(|body| matches!(body, Body::Terminal { .. }))
            .count()
    }
}
