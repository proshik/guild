//! Managed live input's private witness of a Codex turn.
//!
//! Before OSC 7501 the window's [`Board`](super::Board) was moved by hook
//! payloads, the Codex rollout, Esc/Ctrl-C and refused IDE proposals. The
//! board now shows only what a program reports of itself; this latch keeps
//! the old machinery, moved rather than rewritten, for the one consumer that
//! still qualifies on it: automatic live input into a managed Codex session
//! (`docs/runtime-ownership.md`). It is never displayed.

use serde_json::Value;

use super::{QUESTION_TOOL, Signal, Status};
use crate::SessionId;

/// The latch: a status per session, moved by [`Signal`]s.
#[derive(Debug, Default)]
pub struct Latch {
    /// Only sessions that are not idle: an idle one is simply absent.
    entries: std::collections::HashMap<SessionId, Status>,
    observed: std::collections::HashSet<SessionId>,
    interrupted: std::collections::HashSet<SessionId>,
    ended_while_waiting: std::collections::HashSet<SessionId>,
    tools: std::collections::HashMap<SessionId, std::collections::HashMap<String, String>>,
    requests: std::collections::HashMap<SessionId, std::collections::HashSet<String>>,
}

impl Latch {
    /// Move a session by a signal. Answers whether anything changed, so
    /// the window redraws and saves only when something did.
    pub fn apply(&mut self, session: SessionId, signal: Signal) -> bool {
        if signal == Signal::Interrupted {
            self.interrupted.insert(session);
        }
        if matches!(
            signal,
            Signal::PromptSubmitted | Signal::Resumed | Signal::SessionEnded
        ) {
            self.interrupted.remove(&session);
        }
        if matches!(signal, Signal::TurnEnded | Signal::TurnCompleted)
            && self.interrupted.contains(&session)
        {
            return false;
        }
        let observed = matches!(
            signal,
            Signal::PromptSubmitted
                | Signal::ToolStarted
                | Signal::PermissionAsked
                | Signal::QuestionAsked
                | Signal::TurnEnded
                | Signal::TurnCompleted
                | Signal::ExecutionFailed
                | Signal::Resumed
        ) && self.observed.insert(session);
        if signal == Signal::SessionEnded {
            self.observed.remove(&session);
        }
        if matches!(
            signal,
            Signal::PromptSubmitted
                | Signal::Interrupted
                | Signal::SessionEnded
                | Signal::ExecutionFailed
        ) {
            self.requests.remove(&session);
            self.tools.remove(&session);
        }
        if matches!(
            signal,
            Signal::PromptSubmitted
                | Signal::Interrupted
                | Signal::SessionEnded
                | Signal::ExecutionFailed
                | Signal::TurnCompleted
        ) {
            self.ended_while_waiting.remove(&session);
        }
        if signal == Signal::TurnCompleted {
            self.requests.remove(&session);
            self.tools.remove(&session);
        }
        let before = self.status(session);
        if signal == Signal::TurnEnded && matches!(before, Status::Waiting(_)) {
            self.ended_while_waiting.insert(session);
        }
        let after = before.next(signal);
        if after == before {
            return observed;
        }
        if after == Status::Idle {
            self.entries.remove(&session);
        } else {
            self.entries.insert(session, after);
        }
        true
    }

    /// Apply an adapter signal with its original request identity. Parallel
    /// completions never answer another request; identity-free waits require
    /// a new prompt or an authoritative resume rather than a guess.
    pub fn apply_payload(&mut self, session: SessionId, signal: Signal, payload: &Value) -> bool {
        let mut id = ["tool_use_id", "request_id", "requestID", "id"]
            .into_iter()
            .find_map(|key| payload.get(key).and_then(Value::as_str))
            .map(str::to_owned);
        // A failed tool is still a correlated completion, not a failed turn.
        // An abort cannot make the turn green, even when another request waits.
        let tool_interrupted = signal == Signal::ToolFinished
            && payload["hook_event_name"] == "PostToolUseFailure"
            && payload["is_interrupt"] == true;
        if tool_interrupted {
            self.interrupted.insert(session);
            self.ended_while_waiting.remove(&session);
        }
        let fingerprint = tool_fingerprint(payload);
        if payload["hook_event_name"] == "PreToolUse"
            && let (Some(id), Some(fingerprint)) = (id.as_ref(), fingerprint.as_ref())
        {
            self.tools
                .entry(session)
                .or_default()
                .insert(id.clone(), fingerprint.clone());
        }
        if id.is_none()
            && signal == Signal::PermissionAsked
            && let Some(fingerprint) = fingerprint.as_ref()
        {
            let candidates: Vec<_> = self
                .tools
                .get(&session)
                .into_iter()
                .flat_map(|tools| tools.iter())
                .filter(|(_, known)| *known == fingerprint)
                .map(|(id, _)| id.clone())
                .collect();
            if candidates.len() == 1 {
                id = candidates.into_iter().next();
            }
        }
        if signal == Signal::ToolFinished
            && let (Some(tools), Some(id)) = (self.tools.get_mut(&session), id.as_ref())
        {
            tools.remove(id);
        }
        if matches!(signal, Signal::QuestionAsked | Signal::PermissionAsked) {
            let requests = self.requests.entry(session).or_default();
            let duplicate_question_permission = signal == Signal::QuestionAsked
                && payload["hook_event_name"] == "PermissionRequest"
                && payload["tool_name"] == QUESTION_TOOL
                && !requests.is_empty();
            if id.is_some() || !duplicate_question_permission {
                requests.insert(id.clone().unwrap_or_else(|| "unknown".into()));
            }
        }
        if matches!(signal, Signal::ToolFinished | Signal::Resumed)
            && matches!(self.status(session), Status::Waiting(_))
        {
            let matched = id.as_ref().is_some_and(|id| {
                self.requests
                    .get_mut(&session)
                    .is_some_and(|requests| requests.remove(id))
            });
            if !matched
                || self
                    .requests
                    .get(&session)
                    .is_some_and(|requests| !requests.is_empty())
            {
                return false;
            }
            self.requests.remove(&session);
            let ended = self.ended_while_waiting.remove(&session);
            // A delayed completion honors Stop; an explicit request reply instead
            // witnesses resumed execution (OpenCode may report idle while asking).
            let resumed = if signal == Signal::ToolFinished && self.interrupted.contains(&session) {
                Signal::Interrupted
            } else if signal == Signal::ToolFinished && ended {
                Signal::TurnCompleted
            } else {
                Signal::Resumed
            };
            return self.apply(session, resumed);
        }
        if signal == Signal::Resumed {
            return false;
        }
        self.apply(
            session,
            if tool_interrupted {
                Signal::Interrupted
            } else {
                signal
            },
        )
    }

    /// What a session is doing, as far as the latch can tell.
    pub fn status(&self, session: SessionId) -> Status {
        self.entries.get(&session).copied().unwrap_or_default()
    }

    /// A CLI status distinguishes silence from an observed idle process.
    pub fn observed_status(&self, session: SessionId) -> &'static str {
        if self.observed.contains(&session) {
            crate::command::status_word(self.status(session))
        } else {
            "unknown"
        }
    }

    /// Drop a session that is gone.
    pub fn forget(&mut self, session: SessionId) {
        self.entries.remove(&session);
        self.interrupted.remove(&session);
        self.observed.remove(&session);
        self.ended_while_waiting.remove(&session);
        self.requests.remove(&session);
        self.tools.remove(&session);
    }

    /// Keep only the sessions `keep` says yes to.
    pub fn retain(&mut self, keep: impl Fn(SessionId) -> bool) {
        self.observed.retain(|session| keep(*session));
        self.interrupted.retain(|session| keep(*session));
        self.ended_while_waiting.retain(|session| keep(*session));
        self.requests.retain(|session, _| keep(*session));
        self.tools.retain(|session, _| keep(*session));
        self.entries.retain(|session, _| keep(*session));
    }
}

/// A missing permission ID can be resolved only against one active pre-tool
/// request. Codex/Bash matches turn, tool and command; other Codex tools match
/// turn, tool and complete input. Other payloads match prompt, tool and complete
/// input. Names alone collide.
fn tool_fingerprint(payload: &Value) -> Option<String> {
    let name = payload.get("tool_name")?.as_str()?;
    let input = payload.get("tool_input")?;
    // Codex's approval schema omits tool_use_id and uses turn_id. Bash's
    // description is approval UI text, absent from PreToolUse, not tool input.
    if payload
        .get("codex_session")
        .and_then(Value::as_str)
        .is_some()
    {
        let turn = payload.get("turn_id")?.as_str()?;
        if name == "Bash" {
            let command = input.get("command")?.as_str()?;
            return serde_json::to_string(&(turn, name, command)).ok();
        }
        return serde_json::to_string(&(turn, name, input)).ok();
    }
    let prompt = payload.get("prompt_id")?.as_str()?;
    serde_json::to_string(&(prompt, name, input)).ok()
}
