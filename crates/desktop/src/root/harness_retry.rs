//! Explicit replacement of a failed runtime; never reopen/forget a task record.
use crate::i18n::Text;
use crate::{Body, Root, TabId};
use gpui::{Context, Window};
use guild_session::{Conversation, PaneId, Resumption, SessionRecord};

#[derive(Debug, PartialEq)]
enum Choice {
    Launch(Conversation),
    ConfirmFresh,
    Refuse(Text),
}
fn choice(resumption: Resumption, task: bool) -> Choice {
    match resumption {
        Resumption::Continue(conversation) => Choice::Launch(conversation),
        Resumption::Unidentified if !task => Choice::ConfirmFresh,
        Resumption::Forget => Choice::Refuse(Text::HarnessRetryOccupied),
        _ => Choice::Refuse(Text::HarnessRetryManaged),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn occupied_service_only_claude_transcript_is_retained_and_exact_conversation_resumes() {
        let fixture = tempfile::tempdir().unwrap();
        let projects = fixture.path().join("project");
        std::fs::create_dir(&projects).unwrap();
        let mut record = SessionRecord::new(
            "task".into(),
            guild_session::ProjectId::new(),
            fixture.path().into(),
            guild_session::Runs::Claude(guild_session::SessionMode::Terminal),
        );
        record.task = true;
        let key = record.id.to_string();
        let unavailable = fixture.path().join("not-a-directory");
        std::fs::write(&unavailable, "fixture").unwrap();
        assert_eq!(
            prepare_in(&record, &unavailable),
            Choice::Refuse(Text::HarnessRetryIdentity)
        );
        assert_eq!(
            prepare_in(&record, fixture.path()),
            Choice::Launch(Conversation::New(key.clone()))
        );
        let transcript = projects.join(format!("{key}.jsonl"));
        std::fs::write(
            &transcript,
            "{\"type\":\"ai-title\",\"aiTitle\":\"fixture\"}\n",
        )
        .unwrap();
        assert_eq!(
            prepare_in(&record, fixture.path()),
            Choice::Refuse(Text::HarnessRetryOccupied)
        );
        assert!(transcript.exists());
        assert!(record.task);
        std::fs::write(
            &transcript,
            "{\"type\":\"user\",\"message\":{\"content\":\"fixture\"}}\n",
        )
        .unwrap();
        assert_eq!(
            prepare_in(&record, fixture.path()),
            Choice::Launch(Conversation::Resume(key))
        );
    }

    #[test]
    fn retry_distinguishes_unused_exact_unknown_and_occupied_identities() {
        for task in [false, true] {
            for conversation in [
                Conversation::New("unused".into()),
                Conversation::Resume("exact".into()),
            ] {
                assert_eq!(
                    choice(Resumption::Continue(conversation.clone()), task),
                    Choice::Launch(conversation)
                );
            }
            assert_eq!(
                choice(Resumption::Forget, task),
                Choice::Refuse(Text::HarnessRetryOccupied)
            );
        }
        assert_eq!(
            choice(Resumption::Unidentified, false),
            Choice::ConfirmFresh
        );
        assert_eq!(
            choice(Resumption::Unidentified, true),
            Choice::Refuse(Text::HarnessRetryManaged)
        );
    }
}

fn prepare(record: &SessionRecord) -> Choice {
    prepare_in(record, &crate::paths::claude_projects_dir())
}
fn prepare_in(record: &SessionRecord, projects: &std::path::Path) -> Choice {
    let key = record.id.to_string();
    if record.runs.harness() == Some(guild_session::HarnessKind::Claude)
        && !claude_namespace_known(projects, &key)
    {
        return Choice::Refuse(Text::HarnessRetryIdentity);
    }
    let recovered = crate::hooks::identity::recover(&crate::hooks::identity::directory(), record);
    choice(
        guild_session::resumption(
            record.runs,
            &key,
            record.harness_session.as_deref().or(recovered.as_deref()),
            || {
                guild_session::continuation(
                    guild_session::transcript::has_conversation(projects, &key),
                    guild_session::transcript::is_id_taken(projects, &key),
                )
            },
        ),
        record.task,
    )
}
// The legacy transcript predicates hide read errors. Retry may call a new ID
// unused only when the namespace was actually inspectable (or is absent).
fn claude_namespace_known(projects: &std::path::Path, key: &str) -> bool {
    let entries = match std::fs::read_dir(projects) {
        Ok(entries) => entries,
        Err(error) => return error.kind() == std::io::ErrorKind::NotFound,
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        if !kind.is_dir() && !kind.is_symlink() {
            continue;
        }
        let candidate = entry.path().join(format!("{key}.jsonl"));
        match candidate.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                if candidate.symlink_metadata().is_ok() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

fn admit(record: &SessionRecord, conversation: &Conversation) -> Result<(), ()> {
    std::fs::read_dir(&record.cwd).map_err(|_| ())?;
    if let Conversation::Resume(id) = conversation {
        if record.runs.harness() == Some(guild_session::HarnessKind::Codex) {
            let rollout = guild_session::codex_transcript::path(
                &guild_session::harness::CodexHarness::home(),
                id,
            )
            .ok_or(())?;
            super::resume_handoff::inspect(id, &rollout).map_err(|_| ())?;
        } else if !crate::terminal::ownership::await_holders(id)
            .map_err(|_| ())?
            .is_empty()
        {
            return Err(());
        }
    } else if !crate::terminal::ownership::await_holders(conversation.id())
        .map_err(|_| ())?
        .is_empty()
    {
        return Err(());
    }
    Ok(())
}

impl Root {
    pub(crate) fn retry_harness(
        &mut self,
        tab: TabId,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(Body::Terminal {
            session,
            view,
            facts,
        }) = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .and_then(|open| open.panes.body_of(pane))
        else {
            return;
        };
        if !facts.exited() || facts.failure().is_none() || facts.retrying() {
            return;
        }
        let (session, view_id, facts) = (*session, view.entity_id(), facts.clone());
        let Some(record) = self.desk.session(session).cloned() else {
            return;
        };
        let Some(mut settings) = facts.launch_settings() else {
            return;
        };
        // These policies apply to every replacement process; keep the other
        // captured launch settings that Retry promises to preserve.
        settings.codex_sandbox = self.settings.codex_sandbox;
        settings.codex_command_approvals = self.settings.codex_command_approvals;
        facts.set_retrying(true);
        cx.notify();
        let snapshot = record.clone();
        let preparation = cx
            .background_executor()
            .spawn(async move { prepare(&snapshot) });
        let root = cx.weak_entity();
        let ui = self.ui();
        window.spawn(cx, async move |cx| {
            let prepared = preparation.await;
            let conversation = match prepared {
                Choice::Launch(conversation) => Ok(Some(conversation)),
                Choice::Refuse(reason) => Err(reason),
                Choice::ConfirmFresh => {
                    let answers = [gpui::PromptButton::new(ui.text(Text::HarnessRetry)), gpui::PromptButton::cancel(ui.text(Text::Cancel))];
                    let answer = cx.update(|window, cx| window.prompt(gpui::PromptLevel::Warning,
                        ui.text(Text::HarnessRetry), Some(ui.text(Text::HarnessRetryUnknown)), &answers, cx));
                    let confirmed = match answer { Ok(answer) => answer.await == Ok(0), Err(_) => false };
                    Ok(confirmed.then(|| Conversation::New(session.to_string())))
                }
            };
            let result = match conversation {
                Ok(Some(conversation)) => {
                    let snapshot = record.clone();
                    let exact = conversation.clone();
                    let admitted = cx.background_executor().spawn(async move {
                        if std::fs::read_dir(&snapshot.cwd).is_err() { return Err(Text::HarnessRetryDirectory); }
                        admit(&snapshot, &exact).map_err(|_| Text::HarnessRetryHeld)
                    }).await;
                    admitted.map(|()| Some(conversation))
                }
                Ok(None) => Ok(None),
                Err(reason) => Err(reason),
            };
            let _ = cx.update(|_, cx| root.update(cx, |root, cx| {
                facts.set_retrying(false);
                // Recheck the exact runtime, record and launch target after every await.
                let valid = root.desk.session(session) == Some(&record)
                    && root.tabs.iter().find(|open| open.id == tab).and_then(|open| open.panes.body_of(pane))
                        .is_some_and(|body| matches!(body, Body::Terminal { session: now, view, facts }
                            if *now == session && view.entity_id() == view_id && facts.exited() && facts.failure().is_some()));
                if !valid { cx.notify(); return; }
                match result {
                    Err(reason) => root.warn(ui.text(reason).to_owned()),
                    Ok(None) => {},
                    Ok(Some(conversation)) => {
                        let held = root.desk.sessions().any(|other| other.id != session
                            && other.runs.harness() == record.runs.harness()
                            && other.harness_session.as_deref() == Some(conversation.id())
                            && root.tabs.iter().flat_map(crate::Tab::bodies).any(|body| body.session() == Some(other.id)));
                        if held { root.warn(ui.text(Text::HarnessRetryHeld).to_owned()); cx.notify(); return; }
                        // No command, task or draft prompt is replayed. The saved record
                        // remains the provenance authority; no close/forget path runs.
                        if std::fs::read_dir(&record.cwd).is_err() {
                            root.warn(ui.text(Text::HarnessRetryDirectory).to_owned());
                        } else {
                            // Presentation follows the window at replacement time,
                            // after preparation/confirmation/admission have awaited.
                            // Keep the captured command/selection and existing policy rules.
                            settings.theme_mode = root.settings.theme_mode;
                            settings.theme_custom = root.settings.theme_custom;
                            settings.terminal_font = root.settings.terminal_font.clone();
                            settings.terminal_font_size = root.settings.terminal_font_size;
                            settings.language = root.settings.language;
                            settings.copy_on_select = root.settings.copy_on_select;
                            settings.adaptive_terminal_refresh = root.settings.adaptive_terminal_refresh;
                            let body = Body::open(session, record.runs, &record.cwd, Some(conversation),
                                &settings, root.theme, root.ide_port(), root.hook_launch(), None,
                                record.launch_selection.as_ref(), &root.start_size, cx);
                            match body {
                                Body::Broken { reason, .. } => root.warn(reason),
                                body => {
                                    // Process telemetry belongs to the replaced runtime,
                                    // unlike the saved session, history and usage.
                                    root.forget_status(session);
                                    let open = root.tabs.iter_mut().find(|open| open.id == tab).unwrap();
                                    open.panes.take(pane);
                                    let _ = open.panes.fill(pane, body);
                                    root.focusing_front = true;
                                    root.save();
                                }
                            }
                        }
                    }
                }
                cx.notify();
            }));
        }).detach();
    }
}

#[cfg(test)]
mod runtime_tests;
