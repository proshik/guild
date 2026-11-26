//! Managed same-conversation continuation. No blind input or automatic replay.
mod opencode_service;

use crate::{Body, Root};
use gpui::Context;
use guild_session::{
    Conversation, SessionId,
    command::Answer,
    resume_handoff::{self, Attempt, Receipt},
};

fn reply(result: Result<Receipt, String>) -> Answer {
    match result {
        Ok(receipt) => Answer::SessionContinuation {
            receipt: Box::new(receipt),
        },
        Err(because) => Answer::Refused { because },
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Runtime {
    Closed,
    Live {
        entity: gpui::EntityId,
        pid: u32,
        epoch: u64,
        completion: Option<String>,
        foreground: Option<u32>,
    },
    Retained {
        entity: gpui::EntityId,
        leader: u32,
        epoch: u64,
    },
}
impl Runtime {
    fn path(&self) -> resume_handoff::DeliveryPath {
        if matches!(self, Self::Live { .. }) {
            resume_handoff::DeliveryPath::Live
        } else {
            resume_handoff::DeliveryPath::Startup
        }
    }
}
// In-memory authority granted only by a fresh durable reservation. Never replayed.
struct ReservedContinuation(Attempt);
enum PreparedContinuation {
    Existing(Receipt),
    Reserved(ReservedContinuation),
}
impl Root {
    fn continuation_controller(&self, caller: Option<&str>) -> Result<Option<SessionId>, String> {
        let controller = caller
            .map(|raw| {
                raw.parse::<SessionId>()
                    .map_err(|_| "Invalid continuation caller".to_owned())
            })
            .transpose()?
            .map(|id| crate::binding::resolve(id, &self.started_as));
        resume_handoff::authorize_controller(&self.desk, controller)?;
        Ok(controller)
    }
    pub(super) fn command_continuation_status(
        &self,
        session: &str,
        key: &str,
        caller: Option<&str>,
    ) -> Answer {
        reply((|| {
            resume_handoff::validate(key, "status")?;
            let controller = self.continuation_controller(caller)?;
            let id = session
                .parse::<SessionId>()
                .map_err(|_| "Invalid task session")?;
            self.continuation_receipts.lookup(controller, id, key)
        })())
    }
    fn continuation_runtime(&self, id: SessionId, cx: &gpui::App) -> Result<Runtime, String> {
        let record = self.desk.session(id).ok_or("Task is not in this window")?;
        let conversation = resume_handoff::target_conversation(record)?;
        let kind = record.runs.harness().ok_or("Task harness missing")?;
        if self.workflow_holds_task(id) {
            return Err("workflow_owned: use the retained workflow delivery route".into());
        }
        if self.desk.sessions().any(|r| {
            r.id != id
                && r.runs.harness() == Some(kind)
                && r.harness_session.as_deref() == Some(&conversation)
        }) {
            return Err(
                "conversation_conflict: exact conversation belongs to another window record".into(),
            );
        }
        let bodies: Vec<_> = self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter(|b| b.session() == Some(id))
            .collect();
        if bodies.is_empty() {
            if record.open {
                return Err("ownership_unqualified: saved open task has no reconciled runtime; no tab was restored".into());
            }
            return Ok(Runtime::Closed);
        }
        if bodies.len() != 1 {
            return Err("ownership_unqualified: task has multiple native bodies".into());
        }
        let Body::Terminal { view, facts, .. } = bodies[0] else {
            return Err("ownership_unqualified: task is not a terminal body".into());
        };
        if facts
            .resumed()
            .is_some_and(|(k, id)| k != kind || id != conversation)
        {
            return Err("binding_changed: terminal conversation differs".into());
        }
        let terminal = view.read(cx);
        if terminal.launched_harness() != Some(kind) {
            return Err(
                "ownership_unqualified: runtime was not launched by its saved adapter".into(),
            );
        }
        if terminal.exited() {
            return Ok(Runtime::Retained {
                entity: view.entity_id(),
                leader: terminal
                    .recovery_leader()
                    .ok_or("ownership_unqualified: exited body lacks a retained owner")?,
                epoch: terminal.input_epoch(),
            });
        }
        Ok(Runtime::Live {
            entity: view.entity_id(),
            pid: terminal
                .process_id()
                .ok_or("ownership_unqualified: runtime leader missing")?,
            epoch: terminal.input_epoch(),
            completion: terminal.managed_completion().map(str::to_owned),
            foreground: terminal.managed_foreground(),
        })
    }
    fn continuation_attempt(
        &self,
        session: &str,
        key: &str,
        prompt: &str,
        caller: Option<&str>,
        front: bool,
    ) -> Result<Attempt, String> {
        use resume_handoff::DeliveryPath;
        resume_handoff::validate(key, prompt)?;
        let controller = self.continuation_controller(caller)?;
        let id = session
            .parse::<SessionId>()
            .map_err(|_| "Invalid task session")?;
        let record = self.desk.session(id).ok_or("Task is not in this window")?;
        let conversation = resume_handoff::target_conversation(record)?;
        let kind = record.runs.harness().ok_or("Task harness missing")?;
        let previous = self.continuation_receipts.previous(controller, id, key)?;
        if previous.as_ref().is_some_and(|r| r.request.kind() != kind) {
            return Err("Continuation harness changed".into());
        }
        let legacy = previous.as_ref().is_some_and(|r| r.request.version == 1);
        let path=previous.as_ref().and_then(|r|r.request.path).unwrap_or_else(|| {
            if self.tabs.iter().flat_map(crate::Tab::bodies).any(|b|matches!(b,Body::Terminal {session,facts,..} if *session==id && !facts.exited())) {DeliveryPath::Live} else {DeliveryPath::Startup}
        });
        let fingerprint = if legacy {
            resume_handoff::fingerprint(&(
                prompt,
                front,
                &conversation,
                record.project,
                record.workspace,
                &record.cwd,
            ))
        } else {
            resume_handoff::fingerprint(&(
                2,
                kind,
                path,
                prompt,
                front,
                &conversation,
                record.project,
                record.workspace,
                &record.cwd,
                &record.launch_selection,
                &record.launch_permission,
                self.desk.project(record.project).map(|p| &p.path),
                {
                    // Only the effective harness launch configuration belongs to
                    // request identity; window/repository preferences do not.
                    let settings = self.session_launch_settings(id);
                    (
                        settings.permission_mode,
                        settings.launch_prefixes.get(kind).to_owned(),
                        super::body::launch_arguments(&settings, kind).into_owned(),
                        crate::network::snapshot()
                            .agent_environment(kind)
                            .map_err(str::to_owned)?,
                    )
                },
            ))
        }
        .map_err(|_| "Could not fingerprint continuation")?;
        Ok(Attempt {
            controller,
            session: id,
            request_id: key.into(),
            version: if legacy { 1 } else { 2 },
            harness: (!legacy).then_some(kind),
            path: (!legacy).then_some(path),
            conversation,
            fingerprint,
        })
    }
    fn continuation_live_gap(&self, id: SessionId, cx: &gpui::App) -> Option<String> {
        use guild_session::{
            HarnessKind,
            status::{Status, Why},
        };
        let record = self.desk.session(id)?;
        match self.latch.status(id) {
            Status::Working => {
                return Some("working: wait for the current turn; no input sent".into());
            }
            Status::Waiting(Why::Permission) => {
                return Some(
                    "human_permission: the person must answer the actual permission".into(),
                );
            }
            Status::Waiting(Why::Question) => {
                return Some("human_question: the person must answer the actual question".into());
            }
            _ => {}
        }
        // What the program itself reports refuses too, after the latch's own
        // reasons: a turn in progress or a wait for the person (OSC 7501).
        // Never looser than the latch alone.
        match self.board.reported(id) {
            Some(guild_session::status::Reported::Working) => {
                return Some(
                    "working: the program reports a turn in progress (OSC 7501); no input sent"
                        .into(),
                );
            }
            Some(guild_session::status::Reported::Blocked(_)) => {
                return Some(
                    "human_wait: the program reports it is waiting for the person (OSC 7501); no input sent"
                        .into(),
                );
            }
            // No record — cleared, or never reported — is no finished turn.
            None => {
                return Some(
                    "status_unavailable: the program has not reported its state (OSC 7501); no input sent"
                        .into(),
                );
            }
            _ => {}
        }
        match record.runs.harness()? {
            HarnessKind::Claude=>return Some("claude_input_unqualified: Stop hooks may continue; no post-all-hooks readiness witness".into()),
            HarnessKind::OpenCode=>return Some("opencode_input_unqualified: the adapter cannot establish empty live composer readiness".into()),
            HarnessKind::Codex=>{},
        }
        if self.latch.observed_status(id) == "unknown" {
            return Some("telemetry_unavailable: no current completed-turn witness; runtime remains untouched".into());
        }
        if !matches!(self.latch.observed_status(id), "done" | "idle") {
            return Some(
                "completion_unqualified: current turn did not complete successfully".into(),
            );
        }
        let ready=self.tabs.iter().flat_map(crate::Tab::bodies).any(|b|matches!(b,Body::Terminal {session,view,..} if *session==id && view.read(cx).managed_codex_ready()));
        if !ready {
            return Some("input_unqualified: require a current-runtime Codex turn hook, matching fresh disk start/completion, unchanged input and ready transport".into());
        }
        if self
            .pending_settle
            .iter()
            .any(|(s, _, _)| s.session() == Some(id))
            || self.continuation_starts.contains_key(&id)
        {
            return Some("delivery_unknown: a submission is awaiting its actual turn start".into());
        }
        if self.tabs.iter().any(|t| {
            t.panes.bodies().iter().any(|(pane, b)| {
                b.session() == Some(id)
                    && (self
                        .overlays
                        .visible(&guild_session::overlay::Target::Session(id))
                        .is_some()
                        || self
                            .overlays
                            .visible(&guild_session::overlay::Target::Pane(*pane))
                            .is_some())
            })
        }) {
            return Some("input_unqualified: the target input surface is covered".into());
        }
        None
    }
    /// Nonmutating launch constraints shared by preflight and effect admission.
    fn continuation_launch_admission(
        &self,
        record: &guild_session::SessionRecord,
    ) -> Result<(), String> {
        if !record.cwd.is_absolute() || !record.cwd.is_dir() {
            return Err("cwd_unavailable: task cwd is not an available absolute directory".into());
        }
        if self
            .desk
            .project(record.project)
            .is_none_or(|p| !record.cwd.starts_with(&p.path))
        {
            return Err("worktree_binding: task no longer belongs to its project directory".into());
        }
        if self.hook_launch().is_none() {
            return Err(
                "Guild session provenance is unavailable; continuation was not launched".into(),
            );
        }
        Ok(())
    }
    fn continuation_preflight_runtime(
        &self,
        id: SessionId,
        cx: &gpui::App,
    ) -> Result<Runtime, String> {
        let record = self.desk.session(id).ok_or("Task is not in this window")?;
        self.continuation_launch_admission(record)?;
        self.continuation_runtime(id, cx)
    }
    fn continuation_admission(
        &self,
        attempt: &Attempt,
        runtime: &Runtime,
        cx: &gpui::App,
    ) -> Result<(), String> {
        let record = self
            .desk
            .session(attempt.session)
            .ok_or("Task is no longer in this window")?;
        if resume_handoff::target_conversation(record)? != attempt.conversation
            || record.runs.harness() != Some(attempt.kind())
        {
            return Err("binding_changed: task conversation/harness changed".into());
        }
        self.continuation_launch_admission(record)?;
        if &self.continuation_runtime(attempt.session, cx)? != runtime {
            return Err("binding_changed: runtime or input epoch changed during inspection".into());
        }
        if matches!(runtime, Runtime::Live { .. })
            && let Some(gap) = self.continuation_live_gap(attempt.session, cx)
        {
            return Err(gap);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn command_resume_task(
        &mut self,
        session: &str,
        key: &str,
        prompt: &str,
        caller: Option<&str>,
        front: bool,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Self>,
    ) {
        let prepared = (|| {
            let attempt = self.continuation_attempt(session, key, prompt, caller, front)?;
            if let Some(receipt) = self.continuation_receipts.existing(&attempt)? {
                return Ok((attempt, None, Some(receipt)));
            }
            let runtime = self.continuation_runtime(attempt.session, cx)?;
            self.continuation_admission(&attempt, &runtime, cx)?;
            Ok((attempt, Some(runtime), None))
        })();
        let (attempt, runtime, previous) = match prepared {
            Ok(v) => v,
            Err(because) => {
                let _ = answer.send(Answer::Refused { because });
                return;
            }
        };
        if let Some(receipt) = previous {
            let _ = answer.send(reply(Ok(receipt)));
            return;
        }
        let runtime = runtime.expect("fresh attempt captures runtime");
        let record = self
            .desk
            .session(attempt.session)
            .expect("admitted")
            .clone();
        let (prompt, caller) = (prompt.to_owned(), caller.map(str::to_owned));
        let conversation = attempt.conversation.clone();
        let observed = runtime.clone();
        let allow_service = self.continuation_service_launch(record.id);
        cx.spawn(async move |root, cx| {
            let first_record = record.clone();
            let first_conversation = conversation.clone();
            let first_runtime = observed.clone();
            let inspection = cx
                .background_executor()
                .spawn(async move {
                    inspect_continuation_snapshot(
                        &first_record,
                        &first_conversation,
                        &first_runtime,
                        allow_service,
                    )
                })
                .await;
            let first_witness = inspection.as_ref().ok().cloned().flatten();
            let prepared = root.update(cx, |root, cx| {
                root.reserve_checked_continuation(
                    &attempt,
                    &prompt,
                    front,
                    caller.as_deref(),
                    &runtime,
                    inspection.map(|_| ()),
                    cx,
                )
            });
            let reserved = match prepared {
                Ok(Ok(PreparedContinuation::Reserved(reserved))) => reserved,
                Ok(Ok(PreparedContinuation::Existing(receipt))) => {
                    let _ = answer.send(reply(Ok(receipt)));
                    return;
                }
                Ok(Err(error)) => {
                    let _ = answer.send(reply(Err(error)));
                    return;
                }
                Err(_) => return,
            };
            // Reservation is durable before this second probe. A crash or lost
            // callback cannot re-admit it. In particular OpenCode's create-if-
            // absent --session must not run after a known history disappearance.
            let inspection = cx
                .background_executor()
                .spawn(async move {
                    inspect_continuation_snapshot(&record, &conversation, &observed, allow_service)
                })
                .await;
            let inspection = inspection.and_then(|witness| {
                if witness == first_witness {
                    Ok(())
                } else {
                    Err("opencode_inspection: outer service snapshots changed; no launch".into())
                }
            });
            let _ = root.update(cx, |root, cx| {
                let result = root.finish_reserved_continuation(
                    reserved,
                    prompt,
                    front,
                    caller.as_deref(),
                    &runtime,
                    inspection,
                    cx,
                );
                let _ = answer.send(reply(result));
            });
        })
        .detach();
    }
    fn continuation_service_launch(&self, id: guild_session::SessionId) -> bool {
        let settings = self.session_launch_settings(id);
        let kind = guild_session::HarnessKind::OpenCode;
        settings.launch_prefixes.get(kind).is_empty()
            && settings.launch_arguments.get(kind).is_empty()
    }
    fn continuation_context(
        &self,
        attempt: &Attempt,
        prompt: &str,
        front: bool,
        caller: Option<&str>,
        runtime: &Runtime,
        cx: &gpui::App,
    ) -> Result<(), String> {
        let current = self.continuation_attempt(
            &attempt.session.to_string(),
            &attempt.request_id,
            prompt,
            caller,
            front,
        )?;
        if current != *attempt {
            return Err("Continuation context changed during process inspection".into());
        }
        self.continuation_admission(attempt, runtime, cx)?;
        if attempt.path.is_some_and(|p| p != runtime.path()) {
            return Err("Continuation delivery path changed".into());
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn reserve_checked_continuation(
        &mut self,
        attempt: &Attempt,
        prompt: &str,
        front: bool,
        caller: Option<&str>,
        runtime: &Runtime,
        inspection: Result<(), String>,
        cx: &gpui::App,
    ) -> Result<PreparedContinuation, String> {
        if let Some(receipt) = self.continuation_receipts.existing(attempt)? {
            return Ok(PreparedContinuation::Existing(receipt));
        }
        inspection?;
        self.continuation_context(attempt, prompt, front, caller, runtime, cx)?;
        let (receipt, fresh) = self.continuation_receipts.reserve(attempt)?;
        Ok(if fresh {
            PreparedContinuation::Reserved(ReservedContinuation(attempt.clone()))
        } else {
            PreparedContinuation::Existing(receipt)
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn finish_reserved_continuation(
        &mut self,
        reserved: ReservedContinuation,
        prompt: String,
        front: bool,
        caller: Option<&str>,
        runtime: &Runtime,
        inspection: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Result<Receipt, String> {
        let attempt = reserved.0;
        let valid = inspection
            .and_then(|_| self.continuation_context(&attempt, &prompt, front, caller, runtime, cx));
        if let Err(error) = valid {
            self.continuation_receipts.finish(&attempt, false)?;
            return Err(error);
        }
        let receipt = self.continuation_receipts.lookup(
            attempt.controller,
            attempt.session,
            &attempt.request_id,
        )?;
        if receipt.request != attempt || receipt.state != resume_handoff::State::Reserved {
            return Err("Continuation reservation is no longer exclusive".into());
        }
        let launched = match runtime {
            Runtime::Live { entity, .. } => {
                let view = self
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|b| match b {
                        Body::Terminal { view, .. } if view.entity_id() == *entity => {
                            Some(view.clone())
                        }
                        _ => None,
                    })
                    .ok_or("Retained runtime disappeared")?;
                view.update(cx, |v, cx| {
                    v.submit_baselined(&prompt, Some((&attempt.conversation, true)), cx)
                })
            }
            Runtime::Closed => {
                self.forget_status(attempt.session);
                self.reopen_prompt(attempt.session,Some(Conversation::Resume(attempt.conversation.clone())),Some(prompt),front,cx).is_some_and(|tab|self.tabs.iter().find(|t|t.id==tab).is_some_and(|t|t.bodies().any(|b|matches!(b,Body::Terminal {session,..} if *session==attempt.session))))
            }
            Runtime::Retained { entity, leader, .. } => self.reopen_quiescent_task(
                attempt.session,
                *entity,
                *leader,
                &attempt.conversation,
                prompt,
                front,
                cx,
            )?,
        };
        // Once input or startup occurred, exclusion must precede any fallible
        // persistence. A missing outcome/start remains blocked, never replayable.
        if launched {
            self.forget_status(attempt.session);
            if let Some((entity, epoch)) =
                self.tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|b| match b {
                        Body::Terminal { session, view, .. } if *session == attempt.session => {
                            Some((view.entity_id(), view.read(cx).input_epoch()))
                        }
                        _ => None,
                    })
            {
                self.continuation_starts
                    .insert(attempt.session, (attempt.clone(), entity, epoch));
            }
        }
        self.continuation_receipts.finish(&attempt, launched)
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn finish_checked_continuation(
        &mut self,
        attempt: Attempt,
        prompt: String,
        front: bool,
        caller: Option<&str>,
        runtime: &Runtime,
        inspection: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Result<Receipt, String> {
        match self.reserve_checked_continuation(
            &attempt, &prompt, front, caller, runtime, inspection, cx,
        )? {
            PreparedContinuation::Existing(receipt) => Ok(receipt),
            PreparedContinuation::Reserved(reserved) => self.finish_reserved_continuation(
                reserved,
                prompt,
                front,
                caller,
                runtime,
                Ok(()),
                cx,
            ),
        }
    }
    #[cfg(test)]
    fn finish_continuation(
        &mut self,
        attempt: Attempt,
        prompt: String,
        front: bool,
        caller: Option<&str>,
        inspection: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Result<Receipt, String> {
        // Existing direct boundary fixtures deliberately supply their owned probe result.
        if let Some(receipt) = self.continuation_receipts.existing(&attempt)? {
            return Ok(receipt);
        }
        let runtime = self.continuation_runtime(attempt.session, cx)?;
        self.finish_checked_continuation(attempt, prompt, front, caller, &runtime, inspection, cx)
    }
    pub(super) fn continuation_began(
        &mut self,
        id: SessionId,
        observed: gpui::EntityId,
        input_epoch: u64,
    ) {
        let Some((attempt, entity, expected_epoch)) = self.continuation_starts.get(&id).cloned()
        else {
            return;
        };
        let same_conversation = self.desk.session(id).is_some_and(|r| {
            r.runs.harness() == Some(attempt.kind())
                && resume_handoff::target_conversation(r).ok().as_deref()
                    == Some(&attempt.conversation)
        });
        if same_conversation && input_epoch==expected_epoch && observed==entity && self.tabs.iter().flat_map(crate::Tab::bodies).any(|b|matches!(b,Body::Terminal {session,view,..} if *session==id && view.entity_id()==entity)) {
            match self.continuation_receipts.mark_started(&attempt) {
                Ok(())=>{self.continuation_starts.remove(&id);},
                Err(error)=>tracing::warn!(%error,"continuation start outcome is not durable; no retry"),
            }
        }
    }
    pub(super) fn command_inspect_continuation(
        &mut self,
        session: &str,
        caller: Option<&str>,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Self>,
    ) {
        let capture = (|| {
            self.continuation_controller(caller)?;
            let id = session
                .parse::<SessionId>()
                .map_err(|_| "Invalid task session")?;
            let record = self
                .desk
                .session(id)
                .ok_or("Task is not in this window")?
                .clone();
            let conversation = resume_handoff::target_conversation(&record)?;
            let runtime = self.continuation_preflight_runtime(id, cx);
            Ok((record, conversation, runtime))
        })();
        let (record, conversation, runtime) = match capture {
            Ok(v) => v,
            Err(because) => {
                let _ = answer.send(Answer::Refused { because });
                return;
            }
        };
        let allow_service = self.continuation_service_launch(record.id);
        let original = record.clone();
        let expected = runtime.clone();
        cx.spawn(async move |root, cx| {
            let inspect_conversation = conversation.clone();
            let inspection = cx
                .background_executor()
                .spawn(async move {
                    match &expected {
                        Ok(runtime) => inspect_continuation_snapshot(
                            &record,
                            &inspect_conversation,
                            runtime,
                            allow_service,
                        )
                        .map(|_| ()),
                        Err(gap) => Err(gap.clone()),
                    }
                })
                .await;
            let _ = root.update(cx, |root, cx| {
                let current = root.desk.session(original.id);
                let unchanged = current.is_some_and(|r| {
                    r.cwd == original.cwd
                        && r.runs == original.runs
                        && r.project == original.project
                        && r.workspace == original.workspace
                        && resume_handoff::target_conversation(r).ok().as_deref()
                            == Some(&conversation)
                }) && root.continuation_preflight_runtime(original.id, cx)
                    == runtime;
                let inspected = inspection.is_ok();
                let gap = if !unchanged {
                    Some("binding_changed: runtime changed during preflight".into())
                } else {
                    inspection.err().or_else(|| {
                        match root.continuation_receipts.unresolved(
                            original.id,
                            original.runs.harness().expect("harness"),
                            &conversation,
                        ) {
                            Err(error) => Some(error),
                            Ok(true) => Some(
                                "delivery_unknown: a retained attempt has no durable start outcome"
                                    .into(),
                            ),
                            Ok(false) => {
                                if matches!(runtime, Ok(Runtime::Live { .. })) {
                                    root.continuation_live_gap(original.id, cx)
                                } else {
                                    None
                                }
                            }
                        }
                    })
                };
                let observation = resume_handoff::Observation {
                    session: original.id,
                    harness: original.runs.harness().expect("task harness"),
                    conversation,
                    status: root.board.observed_status(original.id).into(),
                    runtime: if unchanged
                        && inspected
                        && matches!(runtime, Ok(Runtime::Live { .. }))
                    {
                        "live"
                    } else if gap.is_none() {
                        "stopped"
                    } else {
                        "unknown"
                    }
                    .into(),
                    readiness: if gap.is_none() {
                        "ready"
                    } else if gap
                        .as_ref()
                        .is_some_and(|g| g.starts_with("working:") || g.starts_with("human_"))
                    {
                        "blocked"
                    } else {
                        "unqualified"
                    }
                    .into(),
                    reason: gap.unwrap_or_else(|| {
                        if matches!(runtime, Ok(Runtime::Live { .. })) {
                            "qualified_codex_completion"
                        } else {
                            "startup_continuation"
                        }
                        .into()
                    }),
                };
                let _ = answer.send(Answer::SessionContinuationStatus {
                    observation: Box::new(observation),
                });
            });
        })
        .detach();
    }
}

#[cfg(test)]
fn inspect_continuation(
    record: &guild_session::SessionRecord,
    conversation: &str,
    runtime: &Runtime,
) -> Result<(), String> {
    inspect_continuation_snapshot(record, conversation, runtime, false).map(|_| ())
}
fn inspect_continuation_snapshot(
    record: &guild_session::SessionRecord,
    conversation: &str,
    runtime: &Runtime,
    allow_service: bool,
) -> Result<Option<opencode_service::Witness>, String> {
    super::workflow::continuation_membership(record)?;
    record
        .cwd
        .canonicalize()
        .map_err(|_| "cwd_unavailable: exact task directory disappeared")?;
    if let Runtime::Live {
        pid, foreground, ..
    } = runtime
    {
        let kind = record.runs.harness().ok_or("Task harness missing")?;
        inspect_live_runtime(*pid, *foreground, kind)?;
        if kind != guild_session::HarnessKind::Codex {
            // These adapters cannot qualify live input; retain their precise
            // limitation instead of probing another harness's history store.
            return Ok(None);
        }
        let rollout = guild_session::codex_transcript::path(
            &guild_session::harness::CodexHarness::home(),
            conversation,
        )
        .ok_or("ownership_unqualified: exact Codex rollout missing or ambiguous")?;
        inspect_with_owner(conversation, &rollout, Some(*pid), || {})?;
        return inspect_live_runtime(*pid, *foreground, kind).map(|_| None);
    }
    if let Some(kind) = Root::harness_missing(record.runs) {
        return Err(format!(
            "adapter_unavailable: {} is not on the PATH",
            kind.program()
        ));
    }
    if record.runs.harness() == Some(guild_session::HarnessKind::OpenCode)
        && Root::opencode_floor_missed()
    {
        return Err("adapter_unavailable: OpenCode V2 is required".into());
    }
    if let Runtime::Retained { leader, .. } = runtime {
        crate::terminal::ownership::quiescent_session(*leader)?;
    }
    if record.runs.harness() == Some(guild_session::HarnessKind::OpenCode) {
        opencode_service::inspect(record, conversation, allow_service)
    } else {
        super::workflow::inspect_task(record, conversation).map(|_| None)
    }
}
fn inspect_live_runtime(
    leader: u32,
    foreground: Option<u32>,
    kind: guild_session::HarnessKind,
) -> Result<(), String> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // This is the retained PTY child, not an arbitrary PID. No wait reaps it.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            leader as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } != 0
        || info.si_pid != 0
    {
        return Err("ownership_unqualified: live leader ended or changed; no input sent".into());
    }
    let foreground = foreground.ok_or("ownership_unqualified: no foreground harness group")?;
    if unsafe { libc::getsid(foreground as i32) } != leader as i32 {
        return Err("ownership_unqualified: foreground group is not this retained session".into());
    }
    let mut command = std::process::Command::new("/bin/ps");
    command.args(["-axww", "-o", "pid=,ppid=,args="]);
    let output = inspect_command(
        command,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err("ownership_unqualified: live process inspection failed".into());
    }
    let ps = String::from_utf8(output.stdout)
        .map_err(|_| "ownership_unqualified: invalid process snapshot")?;
    checked_holders(&ps, "snapshot-validation")?;
    let args = ps
        .lines()
        .find_map(|line| {
            let mut words = line.trim_start().splitn(2, char::is_whitespace);
            if words.next()?.parse::<u32>().ok()? != foreground {
                return None;
            }
            let rest = words.next()?.trim_start();
            let (_, args) = rest.split_once(char::is_whitespace)?;
            Some(args.trim_start())
        })
        .ok_or("ownership_unqualified: foreground process disappeared")?;
    if !kind.impl_().detect_in_front(args) {
        return Err(format!(
            "ownership_unqualified: foreground program is not {}; no input sent",
            kind.program()
        ));
    }
    Ok(())
}

/// Snapshot both argv and kernel holders of the exact saved rollout. An app-server
/// or a fresh unrelated TUI need not advertise its conversation in argv. Conversely,
/// /resume may change the conversation without changing argv. Neither absence in
/// argv nor an old transcript lifecycle event establishes that a task is free.
pub(super) fn inspect(conversation: &str, rollout: &std::path::Path) -> Result<(), String> {
    inspect_observed(conversation, rollout, || {})
}

fn inspect_observed(
    conversation: &str,
    rollout: &std::path::Path,
    observed_holder: impl Fn(),
) -> Result<(), String> {
    inspect_with_owner(conversation, rollout, None, observed_holder)
}

fn inspect_with_owner(
    conversation: &str,
    rollout: &std::path::Path,
    live_leader: Option<u32>,
    observed_holder: impl Fn(),
) -> Result<(), String> {
    use std::{
        os::unix::fs::MetadataExt,
        process::Command,
        time::{Duration, Instant},
    };
    let metadata = rollout
        .metadata()
        .map_err(|_| "Saved rollout is unavailable")?;
    let identity = (metadata.dev(), metadata.ino());
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let ownership = crate::terminal::ownership::Snapshot::capture();
        let mut files = Command::new("/usr/sbin/lsof");
        files.args(["-nP", "+w", "-Fpf", "--"]).arg(rollout);
        let output = inspect_command(files, until)?;
        let file_holders =
            checked_file_holders(output.status.code(), &output.stdout, &output.stderr)?;
        let mut ps = Command::new("/bin/ps");
        ps.args(["-axww", "-o", "pid=,ppid=,args="]);
        let output = inspect_command(ps, until)?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err("Process inspection failed".into());
        }
        let ps = std::str::from_utf8(&output.stdout)
            .map_err(|_| "Process inspection output is invalid")?;
        let mut holders = checked_holders(ps, conversation)?;
        let mut changed_snapshot = false;
        for pid in file_holders {
            let parent = ps.lines().find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()?.parse::<u32>().ok()? == pid)
                    .then(|| words.next()?.parse::<u32>().ok())
                    .flatten()
            });
            let Some(parent) = parent else {
                changed_snapshot = true;
                break;
            };
            holders.push((pid, parent, String::new()));
        }
        let current = rollout
            .metadata()
            .map_err(|_| "Saved rollout changed during inspection")?;
        if identity != (current.dev(), current.ino()) {
            return Err("Saved rollout changed during inspection".into());
        }
        if changed_snapshot {
            // An exiting wrapper child may disappear between lsof and ps.
            // Reinspect both witnesses; uncertainty never admits a continuation.
            if Instant::now() >= until {
                return Err(
                    "Rollout holder changed during process inspection; retry explicitly".into(),
                );
            }
            continue;
        }
        if let Some(leader) = live_leader {
            // The caller retains this unreaped PTY leader and verifies it live
            // before and after inspection. Only its actual kernel session is
            // owned; matching argv or ancestry strings alone grant no exclusion.
            holders.retain(|(pid, _, _)| unsafe { libc::getsid(*pid as i32) } != leader as i32);
        }
        if holders.is_empty() {
            return Ok(());
        }
        observed_holder();
        if holders.iter().any(|(pid, _, _)| {
            matches!(
                ownership.holder(*pid),
                crate::terminal::ownership::Holder::Foreign
                    | crate::terminal::ownership::Holder::Uncertain
            )
        }) || Instant::now() >= until
        {
            return Err(
                "Task conversation still has a process holder; inspect its tab before retrying"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A missing tool, warnings, a signal, truncated output and an unexpected status
/// are inspection uncertainty, never evidence that the rollout has no holder.
fn checked_file_holders(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Vec<u32>, String> {
    if !stderr.is_empty() {
        return Err("Rollout holder inspection failed".into());
    }
    if code == Some(1) && stdout.is_empty() {
        return Ok(Vec::new());
    }
    if code != Some(0) || stdout.is_empty() {
        return Err("Rollout holder inspection is uncertain".into());
    }
    let text =
        std::str::from_utf8(stdout).map_err(|_| "Rollout holder inspection output is invalid")?;
    let mut pids = Vec::new();
    let mut has_file = true;
    for line in text.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            if !has_file {
                return Err("Rollout holder inspection output is incomplete".into());
            }
            pids.push(
                pid.parse::<u32>()
                    .ok()
                    .filter(|pid| *pid > 0)
                    .ok_or("Rollout holder inspection PID is invalid")?,
            );
            has_file = false;
        } else if let Some(fd) = line.strip_prefix('f') {
            if pids.is_empty() || fd.is_empty() || !fd.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Rollout holder inspection descriptor is invalid".into());
            }
            has_file = true;
        } else {
            return Err("Rollout holder inspection output is invalid".into());
        }
    }
    if pids.is_empty() || !has_file {
        return Err("Rollout holder inspection output is incomplete".into());
    }
    Ok(pids)
}

pub(super) fn inspect_command(
    command: std::process::Command,
    until: std::time::Instant,
) -> Result<std::process::Output, String> {
    inspect_command_input(command, until, None)
}
fn inspect_command_input(
    mut command: std::process::Command,
    until: std::time::Instant,
    input: Option<Vec<u8>>,
) -> Result<std::process::Output, String> {
    use std::{
        io::{Read, Write},
        process::Stdio,
        time::{Duration, Instant},
    };
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = guild_session::process::ProcessGroup::spawn(&mut command)
        .map_err(|_| "Process inspection could not start")?;
    let writer = if let Some(input) = input {
        let mut pipe = child.take_stdin().ok_or("Inspection input unavailable")?;
        Some(std::thread::spawn(move || pipe.write_all(&input)))
    } else {
        None
    };
    let stdout = child
        .take_stdout()
        .ok_or("Process inspection has no output")?;
    let stderr = child
        .take_stderr()
        .ok_or("Process inspection has no errors pipe")?;
    let read = |pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| "Process inspection failed")? {
            break status;
        }
        if Instant::now() >= until {
            return Err("Process inspection timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let collect =
        |reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>| -> Result<Vec<u8>, String> {
            let bytes = reader
                .join()
                .map_err(|_| "Process inspection reader failed")?
                .map_err(|_| "Process inspection output failed")?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("Process inspection output is too large".into());
            }
            Ok(bytes)
        };
    if let Some(writer) = writer {
        writer
            .join()
            .map_err(|_| "Inspection writer failed")?
            .map_err(|_| "Inspection input failed")?;
    }
    Ok(std::process::Output {
        status,
        stdout: collect(out)?,
        stderr: collect(err)?,
    })
}

pub(super) fn checked_holders(
    ps: &str,
    conversation: &str,
) -> Result<Vec<(u32, u32, String)>, String> {
    if ps.trim().is_empty() {
        return Err("Process inspection returned no processes".into());
    }
    for line in ps.lines() {
        let mut words = line.trim_start().splitn(2, char::is_whitespace);
        words
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or("Ambiguous process inspection")?;
        let rest = words
            .next()
            .ok_or("Ambiguous process inspection")?
            .trim_start();
        let (parent, args) = rest
            .split_once(char::is_whitespace)
            .ok_or("Ambiguous process inspection")?;
        parent
            .parse::<u32>()
            .map_err(|_| "Ambiguous process inspection")?;
        if args.trim().is_empty() {
            return Err("Ambiguous process inspection".into());
        }
    }
    Ok(guild_session::harness::holders_with_parents(
        ps,
        conversation,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;
    #[test]
    fn unrelated_codex_process_is_not_a_target_conversation_holder() {
        // The actual target rollout is checked separately using kernel file holders.
        // A daemon or a different fresh conversation has no target ID in argv.
        assert!(
            checked_holders(
                "1 2 codex app-server daemon\n3 2 codex --no-daemon",
                "exact",
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn rollout_inspection_requires_a_successful_unambiguous_snapshot() {
        assert!(checked_file_holders(Some(1), b"", b"").unwrap().is_empty());
        assert_eq!(
            checked_file_holders(Some(0), b"p123\nf4\np456\nf5\n", b"").unwrap(),
            vec![123, 456]
        );
        for (code, out, err) in [
            (None, &b""[..], &b""[..]),
            (Some(2), b"", b""),
            (Some(0), b"", b""),
            (Some(1), b"123\n", b""),
            (Some(1), b"", b"permission denied"),
            (Some(0), b"123\n", b"warning"),
            (Some(0), b"bad\n", b""),
            (Some(0), b"p0\nf4\n", b""),
            (Some(0), b"p123\n", b""),
            (Some(0), b"f4\n", b""),
            (Some(0), b"p123\nf4\np456\n", b""),
        ] {
            assert!(checked_file_holders(code, out, err).is_err());
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn kernel_rollout_holder_blocks_even_without_conversation_in_argv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-exact.jsonl");
        std::fs::write(&path, "fixture").unwrap();
        let held = std::fs::File::open(&path).unwrap();
        assert!(
            inspect("not-in-any-argv", &path)
                .unwrap_err()
                .contains("process holder")
        );
        drop(held);
        assert!(inspect("not-in-any-argv", &path).is_ok());
        std::fs::remove_file(&path).unwrap();
        assert!(inspect("not-in-any-argv", &path).is_err());
    }

    #[test]
    fn inspection_deadline_stops_the_query_process() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 10"]);
        let start = std::time::Instant::now();
        assert!(
            inspect_command(command, start + std::time::Duration::from_millis(20))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn continuation_waits_for_a_wrapped_owned_rollout_holder() {
        use crate::terminal::emulator::{Emulator, GridSize};
        let fixture = tempfile::tempdir().unwrap();
        let rollout = fixture.path().join("rollout.jsonl");
        let ready = fixture.path().join("ready");
        let release = fixture.path().join("release");
        std::fs::write(&rollout, "fixture").unwrap();
        let script = fixture.path().join("holder");
        std::fs::write(&script, "trap '' HUP\nexec 9< \"$1\"\necho $$ > \"$2\"\nwhile [ ! -f \"$3\" ]; do /bin/sleep 0.01 9<&-; done\n").unwrap();
        let mut command = portable_pty::CommandBuilder::new("/bin/zsh");
        command.args([
            "-fic",
            "/bin/sh \"$1\" \"$2\" \"$3\" \"$4\"; true",
            "fixture",
        ]);
        command.arg(&script);
        command.arg(&rollout);
        command.arg(&ready);
        command.arg(&release);
        let mut terminal = Emulator::spawn(command, fixture.path(), GridSize::new(80, 24)).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !ready.exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ready.exists());
        let over = terminal.let_go().unwrap();
        let (sent, result) = std::sync::mpsc::channel();
        let (seen, observed) = std::sync::mpsc::channel();
        let inspected = rollout.clone();
        let inspector = std::thread::spawn(move || {
            sent.send(inspect_observed(
                "fixture-conversation-without-provider",
                &inspected,
                || {
                    let _ = seen.send(());
                },
            ))
            .unwrap();
        });
        let witnessed = observed.recv_timeout(std::time::Duration::from_millis(700));
        let premature = result.recv_timeout(std::time::Duration::from_millis(100));
        // Release even if the assertion would fail, so the fixture cannot leak.
        std::fs::write(&release, "release").unwrap();
        over.recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert!(
            witnessed.is_ok(),
            "inspector did not witness a live holder; result={premature:?}, pid={:?}",
            std::fs::read_to_string(&ready)
        );
        assert!(
            matches!(premature, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
            "inspection completed while the rollout holder was alive: {premature:?}"
        );
        result
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .unwrap();
        inspector.join().unwrap();
    }

    #[test]
    fn rollout_holder_exiting_after_ps_is_reobserved_before_continuation() {
        let fixture = tempfile::tempdir().unwrap();
        let rollout = fixture.path().join("rollout");
        let ready = fixture.path().join("ready");
        std::fs::write(&rollout, "fixture").unwrap();
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                "exec 9< \"$1\"; echo ready > \"$2\"; while :; do /bin/sleep 1 9<&-; done",
                "fixture",
            ])
            .arg(&rollout)
            .arg(&ready);
        let child = std::sync::Mutex::new(
            guild_session::process::ProcessGroup::spawn(&mut command).unwrap(),
        );
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !ready.exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ready.exists());
        let observed = std::sync::atomic::AtomicBool::new(false);
        let result = inspect_observed("snapshot-exit-rollout-fixture", &rollout, || {
            child.lock().unwrap().stop().unwrap();
            observed.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        assert!(observed.load(std::sync::atomic::Ordering::Relaxed));
        assert!(
            result.is_ok(),
            "an exited rollout holder must be reobserved: {result:?}"
        );
    }

    #[test]
    fn process_uncertainty_never_means_no_holder() {
        for ps in ["", "bad", "1 bad codex resume exact"] {
            assert!(checked_holders(ps, "exact").is_err());
        }
        assert!(
            checked_holders("1 2 codex --no-daemon", "exact")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            checked_holders("1 2 codex resume exact --no-daemon", "exact")
                .unwrap()
                .len(),
            1
        );
        assert!(
            checked_holders("1 2 codex resume different", "exact")
                .unwrap()
                .is_empty()
        );
    }
    #[gpui::test]
    fn bound_controllers_and_open_retries_do_not_spawn_or_type(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::bound_controllers_and_open_retries_do_not_spawn_or_type"
        )) {
            return;
        }
        use guild_session::{Desk, Runs, SessionMode, SessionRecord};
        let dir = tempfile::tempdir().unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let record =
            || SessionRecord::new("operator".into(), project, dir.path().into(), Runs::Shell);
        let launched = desk.add_session(record());
        let current = desk.add_session(record());
        desk.close_session(launched);
        let mut task = SessionRecord::new(
            "task".into(),
            project,
            dir.path().into(),
            Runs::Codex(SessionMode::Terminal),
        );
        task.task = true;
        task.open = false;
        task.harness_session = Some("01a1401a-2873-7fe1-9693-049581a0bad4".into());
        let target = desk.add_session(task);
        let entity = cx.new(|cx| Root::from_state(Default::default(), desk, cx));
        cx.update(|cx| {
            entity.update(cx, |root, cx| {
                root.continuation_receipts =
                    resume_handoff::Ledger::load(dir.path().join("receipts"));
                root.started_as.insert(launched, current);
                assert_eq!(
                    root.continuation_controller(Some(&launched.to_string()))
                        .unwrap(),
                    Some(current)
                );
                assert_eq!(root.continuation_controller(None).unwrap(), None);
                assert!(root.continuation_controller(Some("bad")).is_err());
                let session = target.to_string();
                let attempt = root
                    .continuation_attempt(&session, "checkpoint", "continue", None, false)
                    .unwrap();
                root.continuation_receipts.reserve(&attempt).unwrap();
                root.continuation_receipts.finish(&attempt, true).unwrap();
                root.desk.reopen_session(target);
                for (key, text, front, expected) in [
                    ("checkpoint", "continue", false, true),
                    ("checkpoint", "changed", false, false),
                    ("checkpoint", "continue", true, false),
                    ("new", "continue", false, false),
                ] {
                    let (answer, received) = std::sync::mpsc::channel();
                    root.command_resume_task(&session, key, text, None, front, answer, cx);
                    assert_eq!(
                        matches!(
                            received.try_recv().unwrap(),
                            Answer::SessionContinuation { .. }
                        ),
                        expected
                    );
                    assert!(root.tabs.is_empty());
                }
                root.desk.mark_task(current);
                assert!(
                    root.continuation_controller(Some(&launched.to_string()))
                        .is_err()
                );
            })
        });
    }
    #[test]
    fn isolated_literal_startup_handoff() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let fake = bin.join("codex");
        std::fs::write(
            &fake,
            r#"#!/bin/sh
printf '%s\0' "$0" "$@" > "$HANDOFF_ATTEMPT"
printf '%s\0' "$@" > "$HANDOFF_CAPTURE"
# Codex accepts global policy options before its resume subcommand.
while :; do
 case "$1" in
  --sandbox|--ask-for-approval) shift 2 ;;
  resume) break ;;
  *) printf 'HANDOFF fake rejected leading argument: %s\n' "$1" >&2; exit 2 ;;
 esac
done
printf x >> "$HANDOFF_COUNT"
pwd > "$HANDOFF_CWD"
printf '%s' "$GUILD_SESSION" > "$HANDOFF_OWNER"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let stdout = dir.path().join("stdout");
        let stderr = dir.path().join("stderr");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "root::resume_handoff::tests::literal_startup_handoff_fixture",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
            )
            .env("GUILD_STATE_DIR", dir.path().join("state"))
            .env("CODEX_HOME", dir.path().join("codex-home"))
            .env("HANDOFF_COUNT", dir.path().join("count"))
            .env("HANDOFF_CAPTURE", dir.path().join("argv"))
            .env("HANDOFF_CWD", dir.path().join("cwd"))
            .env("HANDOFF_OWNER", dir.path().join("owner"))
            .env("HANDOFF_FAKE", &fake)
            .env("HANDOFF_ATTEMPT", dir.path().join("attempt"))
            .env("HOME", dir.path().join("home"))
            .env("SHELL", "/bin/sh")
            .env_remove("ENV")
            .env_remove("BASH_ENV")
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(&stdout).unwrap())
            .stderr(std::fs::File::create(&stderr).unwrap());
        let mut child = guild_session::process::ProcessGroup::spawn(&mut command).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if std::time::Instant::now() >= deadline {
                child.stop().unwrap();
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(
            status.is_some_and(|status| status.success()),
            "fixture status={status:?}\n{}\n{}",
            std::fs::read_to_string(stdout).unwrap(),
            std::fs::read_to_string(stderr).unwrap()
        );
    }

    #[gpui::test]
    #[ignore = "runs with a private fake Codex in isolated_literal_startup_handoff"]
    fn literal_startup_handoff_fixture(cx: &mut gpui::TestAppContext) {
        use guild_session::{Desk, Runs, SessionMode, SessionRecord};
        // Restore Root's GPUI lease before propagating assertions, so the
        // outer cleanup can always revoke its independently owned PTYs.
        fn checked<T>(
            entity: &gpui::Entity<Root>,
            cx: &mut gpui::App,
            f: impl FnOnce(&mut Root, &mut gpui::Context<Root>) -> T,
        ) -> T {
            entity
                .update(cx, |root, cx| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(root, cx)))
                })
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        }
        cx.executor().allow_parking();
        let fake =
            std::path::PathBuf::from(std::env::var_os("HANDOFF_FAKE").expect("isolated fake"));
        assert!(fake.is_absolute() && fake.is_file());
        // Do not trust a child PATH as a provider isolation boundary. The
        // established launch-prefix seam execs only this absolute owned fake.
        // No login-path worker is started by this exact private test process.
        assert!(guild_session::login_path::path().is_none());
        let selected = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join("codex"))
            .find(|path| path.is_file())
            .unwrap();
        assert_eq!(
            selected.canonicalize().unwrap(),
            fake.canonicalize().unwrap()
        );
        let mut settings = guild_session::Settings::default();
        settings.launch_prefixes.codex =
            format!("shift; exec {}", shell_words::quote(fake.to_str().unwrap()));
        let count =
            std::path::PathBuf::from(std::env::var_os("HANDOFF_COUNT").expect("isolated fixture"));
        let dir = tempfile::tempdir().unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let mut record = SessionRecord::new(
            "task".into(),
            project,
            dir.path().into(),
            Runs::Codex(SessionMode::Terminal),
        );
        record.task = true;
        record.open = false;
        let conversation = "01a1401a-2873-7fe1-9693-049581a0bad4";
        record.harness_session = Some(conversation.into());
        let target = desk.add_session(record);
        let workspace = desk.session(target).unwrap().workspace;
        let prompt = "--help\nпродолжит $HOME; `not-a-command`";
        let entity = cx.update(|cx| {
            crate::editor::init(cx);
            cx.new(|cx| Root::from_state(settings, desk, cx))
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let request = cx.update(|cx| {
                checked(&entity, cx, |root, cx| {
                    root.attach_test_hooks(dir.path());
                    let request = root
                        .continuation_attempt(&target.to_string(), "startup", prompt, None, false)
                        .unwrap();
                    let first = root
                        .finish_continuation(
                            request.clone(),
                            prompt.into(),
                            false,
                            None,
                            Ok(()),
                            cx,
                        )
                        .unwrap();
                    assert_eq!(first.state, resume_handoff::State::Launched);
                    assert_eq!(
                        root.finish_continuation(
                            request.clone(),
                            prompt.into(),
                            false,
                            None,
                            Err("snapshot no longer needed for a retry".into()),
                            cx
                        )
                        .unwrap(),
                        first
                    );
                    let record = root.desk.session(target).unwrap();
                    assert!(record.task && record.open);
                    assert_eq!(record.harness_session.as_deref(), Some(conversation));
                    assert_eq!(record.cwd, dir.path());
                    assert_eq!(record.project, project);
                    assert_eq!(record.workspace, workspace);
                    assert!(
                        root.active_tab.is_none(),
                        "background launch must not focus its tab"
                    );
                    request
                })
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let owner = std::path::PathBuf::from(std::env::var_os("HANDOFF_OWNER").unwrap());
            while !owner.exists() && std::time::Instant::now() < until {
                let ended = entity.update(cx, |root, cx| {
                    root.terminals()
                        .into_iter()
                        .any(|view| view.update(cx, |view, _| view.exit_status().is_some()))
                });
                if ended {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let diagnostics = entity.update(cx, |root, cx| {
                root.terminals()
                    .into_iter()
                    .map(|view| view.update(cx, |view, _| view.startup_diagnostics_for_test()))
                    .collect::<Vec<_>>()
            });
            let attempt =
                std::fs::read(std::env::var_os("HANDOFF_ATTEMPT").unwrap()).unwrap_or_default();
            let attempted: Vec<_> = attempt
                .split(|b| *b == 0)
                .filter(|arg| !arg.is_empty())
                .map(String::from_utf8_lossy)
                .collect();
            assert!(
                owner.exists(),
                "startup witness absent: count={:?}, attempted={attempted:?}, terminal={diagnostics:?}",
                std::fs::read_to_string(&count)
            );
            assert_eq!(std::fs::read_to_string(&count).unwrap(), "x");
            let capture = std::fs::read(std::env::var_os("HANDOFF_CAPTURE").unwrap()).unwrap();
            let args: Vec<_> = capture
                .split(|b| *b == 0)
                .filter(|v| !v.is_empty())
                .map(|b| std::str::from_utf8(b).unwrap())
                .collect();
            assert_eq!(
                &args[..4],
                [
                    "--sandbox",
                    "workspace-write",
                    "--ask-for-approval",
                    "on-request"
                ]
            );
            assert_eq!(&args[4..6], ["resume", conversation]);
            assert_eq!(
                std::path::Path::new(attempted[0].as_ref())
                    .canonicalize()
                    .unwrap(),
                fake.canonicalize().unwrap()
            );
            assert_eq!(&args[args.len() - 2..], ["--", prompt]);
            assert!(args.contains(&"--no-daemon"));
            assert!(!args.contains(&"-s"));
            assert_eq!(std::fs::read_to_string(&owner).unwrap(), target.to_string());
            let cwd = std::fs::read_to_string(std::env::var_os("HANDOFF_CWD").unwrap()).unwrap();
            assert_eq!(
                std::path::Path::new(cwd.trim()).canonicalize().unwrap(),
                dir.path().canonicalize().unwrap()
            );
            let bytes = std::fs::read(crate::paths::state_dir().join("session-continuations.json"))
                .unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains(prompt));
            let mut recovered = resume_handoff::Ledger::load(
                crate::paths::state_dir().join("session-continuations.json"),
            );
            assert!(!recovered.reserve(&request).unwrap().1);
            // PTY child creation is not an acknowledgement that the requested program ran.
            std::fs::write(
                std::env::var_os("HANDOFF_FAKE").unwrap(),
                "#!/missing-guild-fixture-interpreter\n",
            )
            .unwrap();
            let (broken, broken_request, launched) = cx.update(|cx| {
                checked(&entity, cx, |root, cx| {
                    let mut second = root.desk.session(target).unwrap().clone();
                    second.id = SessionId::new();
                    second.open = false;
                    second.harness_session = Some("11111111-2222-3333-4444-555555555555".into());
                    let id = root.desk.add_session(second);
                    let request = root
                        .continuation_attempt(&id.to_string(), "early-exit", prompt, None, false)
                        .unwrap();
                    let receipt = root
                        .finish_continuation(
                            request.clone(),
                            prompt.into(),
                            false,
                            None,
                            Ok(()),
                            cx,
                        )
                        .unwrap();
                    assert_eq!(receipt.state, resume_handoff::State::Launched);
                    assert_eq!(
                        root.finish_continuation(
                            request.clone(),
                            prompt.into(),
                            false,
                            None,
                            Ok(()),
                            cx
                        )
                        .unwrap(),
                        receipt
                    );
                    let view = root
                        .tabs
                        .iter()
                        .flat_map(crate::Tab::bodies)
                        .find_map(|body| match body {
                            Body::Terminal { session, view, .. } if *session == id => {
                                Some(view.clone())
                            }
                            _ => None,
                        })
                        .unwrap();
                    (view, request, receipt)
                })
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let (status, tail, failure) = loop {
                // The real reader wakes GPUI, whose coalesced presentation
                // deadline still belongs to the test's virtual clock.
                cx.executor()
                    .advance_clock(std::time::Duration::from_millis(20));
                cx.run_until_parked();
                let observed = broken.update(cx, |view, _| {
                    let (status, tail) = view.startup_diagnostics_for_test();
                    (status, tail, view.facts().failure())
                });
                if observed.0.is_some() && observed.2.is_some() {
                    break observed;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "broken interpreter completion missing: {observed:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert!(
                status.is_some_and(|code| code != 0),
                "broken interpreter status={status:?}, tail={tail}"
            );
            let failure = failure.unwrap();
            assert_eq!(failure.harness, guild_session::HarnessKind::Codex);
            assert_eq!(Some(failure.code), status);
            assert!(
                !failure.network,
                "an interpreter error is not a provider/network failure"
            );
            assert!(
                tail.contains("missing-guild-fixture-interpreter") || tail.contains("not found"),
                "missing interpreter tail: {tail}"
            );
            cx.update(|cx| {
                checked(&entity, cx, |root, cx| {
                    assert_eq!(
                        root.finish_continuation(
                            broken_request,
                            prompt.into(),
                            false,
                            None,
                            Ok(()),
                            cx
                        )
                        .unwrap(),
                        launched,
                        "an early process failure must not authorize another startup delivery"
                    );
                    assert_eq!(launched.state, resume_handoff::State::Launched);
                })
            });
            assert_eq!(std::fs::read_to_string(count).unwrap(), "x");
            eprintln!(
                "literal handoff: one absolute fake launch, policy/resume/prompt argv and provenance verified; broken interpreter status={status:?}, failure={failure:?}; receipt remains Launched"
            );
        }));
        // Also runs on an assertion failure: revoke IO and await the existing
        // off-thread transport/process owners before this fixture exits.
        entity.update(cx, |root, cx| {
            for terminal in root.terminals() {
                terminal.update(cx, |terminal, cx| terminal.hang_up_for_quit(cx));
            }
        });
        cx.foreground_executor()
            .block_test(crate::transport::finish_closing());
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}

#[cfg(test)]
mod recovery_tests;
