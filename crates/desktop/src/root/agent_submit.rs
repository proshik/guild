//! One terminal submission path for authorized UI gestures and provenance-checked CLI requests.
use super::command::{BEGIN_PATIENCE, Settles};
use crate::Root;
use gpui::Context;
use guild_session::{SessionId, command::Answer};
/// Provenance-checked CLI input.
#[derive(Clone, Copy)]
pub(crate) enum Origin {
    Command,
}
impl Origin {
    pub(super) fn refusal(
        self,
        status: &str,
        turned: bool,
        _initial_prompt: bool,
        accepts_submission: bool,
    ) -> Option<&'static str> {
        if !accepts_submission {
            return Some("its terminal is not reading a prompt — look at its tab");
        }
        guild_session::command::refuses_send(true, false, status, turned)
    }
}

impl Root {
    pub(crate) fn submit_agent_prompt(
        &mut self,
        id: SessionId,
        text: &str,
        origin: Origin,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Self>,
    ) {
        let session = id.to_string();
        let session = session.as_str();
        let refuse = |because: String| {
            let _ = answer.send(Answer::Refused { because });
        };
        let Some(record) = self.desk.session(id) else {
            refuse("Session is no longer open".into());
            return;
        };
        if !record.open {
            refuse("Session is closed".into());
            return;
        }
        if self.continuation_starts.contains_key(&id) {
            refuse(
                "Managed continuation is awaiting its exact runtime start; inspect its receipt"
                    .into(),
            );
            return;
        }
        let status = self.board.observed_status(id);
        let turned = record.had_turn;
        let record_open = record.open;
        let Some(view) =
            self.tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|body| match body {
                    crate::Body::Terminal { session, view, .. } if *session == id => {
                        Some(view.clone())
                    }
                    _ => None,
                })
        else {
            // Closed — or open in a workspace not restored since the start,
            // which has no tab either: `session resume` brings back both.
            // M13.3: the orchestrator reported on 2026-09-29 had no way back.
            let in_a_tab = self
                .tabs
                .iter()
                .any(|tab| tab.sessions().any(|shown| shown == id));
            refuse(if in_a_tab {
                format!("session {session} is not open in a terminal tab")
            } else if record_open {
                format!(
                    "session {session} has no tab yet — bring it back with `guild session resume {session}`, then send"
                )
            } else {
                format!(
                    "session {session} is closed — bring it back with `guild session resume {session}`, then send"
                )
            });
            return;
        };
        if self
            .pending_settle
            .iter()
            .any(|(settles, _, _)| settles.session() == Some(id))
        {
            refuse("A request is already awaiting delivery".into());
            return;
        }
        if let Some(because) = origin.refusal(
            status,
            turned,
            view.read(cx).initial_prompt(),
            view.read(cx).accepts_submission(),
        ) {
            refuse(format!("{session}: {because}"));
            return;
        }
        if view.read(cx).exited() {
            refuse("Agent process has exited".into());
            return;
        }

        let conversation = self
            .desk
            .session(id)
            .and_then(|r| r.harness_session.clone());
        if !view.update(cx, |view, cx| {
            view.submit_baselined(text, conversation.as_deref().map(|c| (c, false)), cx)
        }) {
            refuse(format!("{session}: {}", view.read(cx).submission_refusal()));
            return;
        }
        // The old completion no longer describes the prompt just submitted.
        // Until a hook or rollout witnesses its start, wait/send must see unknown.
        self.forget_status(id);
        self.pending_settle.push((
            Settles::Began {
                session: id,
                by: std::time::Instant::now() + BEGIN_PATIENCE,
            },
            answer,
            Answer::Sent {
                session: session.to_owned(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Origin;
    #[test]
    fn failed_send_requires_a_ready_terminal() {
        assert!(
            Origin::Command
                .refusal("failed", true, false, false)
                .is_some()
        );
        assert_eq!(Origin::Command.refusal("failed", true, false, true), None);
    }
}
