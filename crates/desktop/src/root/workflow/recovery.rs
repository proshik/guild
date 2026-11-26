//! Native operational navigation through the existing session reopen owner.
use super::*;
use crate::Body;
use crate::root::command::Settles;
use guild_session::{Conversation, HarnessKind};
use store::recovery::{Binding, Reserved};

impl Root {
    fn recovery_binding(&self, effect: &Reserved, cx: &gpui::App) -> Result<Binding, String> {
        let record = self
            .desk
            .session(effect.original.session)
            .ok_or("workflow.recovery: original record disappeared")?;
        let binding = Binding::from_record(record, None)?;
        if binding != effect.binding {
            return Err(
                "workflow.recovery: native origin binding/settings changed during inspection"
                    .into(),
            );
        }
        let peer = self.workflow_binding(effect.caller)?;
        let mut pids = std::collections::HashSet::new();
        for body in self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter(|b| b.session() == Some(effect.caller))
        {
            if let Body::Terminal { view, facts, .. } = body {
                if facts.resumed().is_some_and(|(kind, id)| {
                    kind != effect.peer.harness || Some(id) != effect.peer.conversation
                }) {
                    return Err("workflow.recovery: requesting terminal changed conversation during inspection".into());
                }
                if !view.read(cx).exited()
                    && let Some(pid) = view.read(cx).process_id()
                {
                    pids.insert(pid);
                }
            }
        }
        if !self.desk.session(effect.caller).is_some_and(|r| r.open)
            || pids.len() != 1
            || effect.peer_pid.is_none_or(|pid| !pids.contains(&pid))
        {
            return Err(
                "workflow.recovery: requesting native runtime closed or changed during inspection"
                    .into(),
            );
        }
        if peer.session != effect.peer.session
            || peer.harness != effect.peer.harness
            || peer.conversation != effect.peer.conversation
            || peer.cwd != effect.peer.cwd
            || peer.project != effect.peer.project
            || peer.workspace != effect.peer.workspace
        {
            return Err("workflow.recovery: requesting parent/child binding changed".into());
        }
        Ok(binding)
    }
    fn recovery_live_origin(&self, effect: &Reserved, cx: &gpui::App) -> Result<bool, String> {
        let origin = &effect.binding.origin;
        let conversation = origin
            .conversation
            .as_deref()
            .ok_or("workflow.recovery: original conversation missing")?;
        let mut living = 0;
        for body in self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter(|b| b.session() == Some(origin.session))
        {
            match body {
                Body::Terminal { view, facts, .. } => {
                    if facts
                        .resumed()
                        .is_some_and(|(kind, id)| kind != origin.harness || id != conversation)
                    {
                        return Err(
                            "workflow.recovery: retained terminal reports a different conversation"
                                .into(),
                        );
                    }
                    if !view.read(cx).exited() {
                        if view.read(cx).process_id().is_none() {
                            return Err(
                                "workflow.recovery: retained origin runtime ownership is unknown"
                                    .into(),
                            );
                        }
                        living += 1;
                    }
                }
                _ => {
                    return Err(
                        "workflow.recovery: retained origin view has unknown runtime ownership"
                            .into(),
                    );
                }
            }
        }
        if living > 1 {
            return Err(
                "workflow.recovery: multiple live origin holders; reconcile without launching"
                    .into(),
            );
        }
        if self.desk.sessions().any(|r| {
            r.id != origin.session
                && r.runs.harness() == Some(origin.harness)
                && r.harness_session.as_deref() == Some(conversation)
        }) {
            return Err(
                "workflow.recovery: exact conversation belongs to another native record".into(),
            );
        }
        if living == 1 {
            return Ok(true);
        }
        if self
            .desk
            .session(origin.session)
            .is_some_and(|r| r.open && !self.restored_workspaces.contains(&r.workspace))
        {
            return Err(
                "workflow.recovery: origin workspace/holder has not been reconciled".into(),
            );
        }
        Ok(false)
    }
    pub(super) fn perform_workflow_recovery(
        &mut self,
        effect: Reserved,
        previous: Completed,
        cx: &mut Context<Self>,
    ) {
        if effect.inspected {
            let mut resume_attempted = false;
            let observation = match self.finish_workflow_recovery(
                &effect,
                &mut resume_attempted,
                cx,
            ) {
                Ok(value) => value,
                Err(reason) => {
                    self.warn(format!(
                        "{}: {}",
                        self.ui().text(crate::i18n::Text::ProcessRecoveryGap),
                        reason
                    ));
                    serde_json::json!({"phase":"recovery_gap","reason":reason,"ready":false,"continued":false,"resume_attempted":resume_attempted,"parent_attention_required":resume_attempted})
                }
            };
            if let Some(queue) = self.workflows.as_mut()
                && queue
                    .send
                    .try_send(Work::RecoveryObserved {
                        previous: Box::new(previous),
                        observation,
                    })
                    .is_err()
            {
                tracing::error!("recovery observation undelivered; reservation remains uncertain");
            }
            return;
        }
        let admitted = self
            .recovery_binding(&effect, cx)
            .and_then(|_| self.recovery_live_origin(&effect, cx));
        // The owning workflow worker performs bounded external probes, while the
        // native callback rechecks identity before touching a terminal.
        if let Some(queue) = self.workflows.as_mut()
            && queue
                .send
                .try_send(Work::RecoveryInspected {
                    previous: Box::new(previous),
                    inspection: admitted,
                })
                .is_err()
        {
            tracing::error!("recovery probe undelivered; reservation remains uncertain");
        }
    }
    fn finish_workflow_recovery(
        &mut self,
        effect: &Reserved,
        resume_attempted: &mut bool,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        effect
            .inspection
            .as_ref()
            .ok_or("workflow.recovery: holder inspection absent")?
            .clone()?;
        self.recovery_binding(effect, cx)?;
        if self
            .processes
            .closed_at
            .get(&effect.original.session)
            .is_some_and(|closed| *closed > effect.request_epoch)
        {
            return Err(
                "workflow.recovery: origin was closed after navigation; stale callback refused"
                    .into(),
            );
        }
        let live = self.recovery_live_origin(effect, cx)?;
        let origin = &effect.binding.origin;
        let mut placement = serde_json::Value::Null;
        if !live {
            if !effect.stopped_qualified {
                return Err("workflow.recovery: live origin stopped during callback; inspect exact holders again before resume".into());
            }
            if origin.workspace != effect.original.workspace
                || origin.project != effect.original.project
            {
                return Err("workflow.recovery: original project/workspace binding changed".into());
            }
            if !origin.cwd.is_dir()
                || self.desk.workspace(origin.workspace).is_none()
                || self.desk.project(origin.project).is_none()
            {
                return Err("workflow.recovery: original cwd/project/workspace unavailable".into());
            }
            if !self.restored_workspaces.contains(&origin.workspace)
                && self
                    .desk
                    .sessions()
                    .any(|r| r.workspace == origin.workspace && r.id != origin.session && r.open)
            {
                return Err("workflow.recovery: original workspace has unreconciled saved runtimes; restore it explicitly first".into());
            }
            let seat = guild_session::Seat {
                workspace: origin.workspace,
                project: origin.project,
            };
            let areas = self
                .stage
                .areas(seat)
                .ok_or("workflow.recovery: original seat areas unavailable")?;
            let (area, fallback) = recovery_area(areas, effect.original.area)?;
            placement = serde_json::json!({"workspace":origin.workspace,"project":origin.project,"original_area":effect.original.area,"area":area,"fallback":fallback});
            if self.hook_launch().is_none() {
                return Err(
                    "workflow.recovery: actual native provenance channel unavailable".into(),
                );
            }
            if effect.binding.permission.is_none() {
                return Err("workflow.recovery: original permission policy is unknown on this legacy record".into());
            }
            if Self::harness_missing(self.desk.session(origin.session).unwrap().runs).is_some() {
                return Err("workflow.recovery: original harness is unavailable".into());
            }
            if origin.harness == HarnessKind::OpenCode && Self::opencode_floor_missed() {
                return Err(
                    "workflow.recovery: original OpenCode does not support required native adapter"
                        .into(),
                );
            }
            if origin.harness == HarnessKind::OpenCode && effect.startup.is_some() {
                return Err("workflow.recovery: OpenCode empty-input readiness is unqualified; startup input refused".into());
            }
            if let Some(startup) = &effect.startup {
                self.refresh_reconciliation_observations(cx);
                let q = self
                    .workflows
                    .as_ref()
                    .ok_or("workflow.recovery: startup coordinator unavailable")?;
                let observations = q.observations.read().unwrap_or_else(|e| e.into_inner());
                if startup
                    .witnesses
                    .iter()
                    .any(|(id, expected)| observations.get(id) != Some(expected))
                {
                    return Err("workflow.recovery: child/runtime snapshot changed before startup; no launch".into());
                }
            }
            // A stopped body must not be mistaken for the newly resumed one.
            let targets: Vec<_> = self
                .tabs
                .iter()
                .flat_map(|t| {
                    t.panes.bodies().into_iter().filter_map(move |(p, b)| {
                        (b.session() == Some(origin.session)).then_some((t.id, p))
                    })
                })
                .collect();
            for (tab, pane) in targets {
                self.close_pane(tab, pane, cx);
            }
            let conversation = origin
                .conversation
                .clone()
                .ok_or("workflow.recovery: exact conversation missing")?;
            // From this native door onward even a refused/failed return may
            // have created a holder. Keep its continuation fail-closed.
            *resume_attempted = true;
            self.forget_status(origin.session);
            let tab = self
                .reopen_prompt(
                    origin.session,
                    Some(Conversation::Resume(conversation)),
                    effect.startup.as_ref().map(|s| s.text.clone()),
                    false,
                    cx,
                )
                .ok_or("workflow.recovery: native reopen refused")?;
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab) {
                tab.area = area;
            }
            if !self.recovery_live_origin(effect, cx)? {
                return Err("workflow.recovery: launcher failed; no ready/continued claim; inspect original surface".into());
            }
            if effect.startup.is_some() {
                let pid = self
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|b| match b {
                        Body::Terminal { session, view, .. } if *session == origin.session => {
                            view.read(cx).process_id()
                        }
                        _ => None,
                    })
                    .ok_or("workflow.recovery: startup holder unavailable")?;
                let (answer, receive) = mpsc::channel();
                self.pending_settle.push((
                    Settles::Began {
                        session: origin.session,
                        by: std::time::Instant::now() + crate::root::command::BEGIN_PATIENCE,
                    },
                    answer,
                    Answer::Sent {
                        session: origin.session.to_string(),
                    },
                ));
                if let Some(q) = self.workflows.as_mut() {
                    q.startups.push((effect.clone(), pid, receive));
                }
            }
        }
        if !live {
            // Child navigation may open historical evidence before native reopen.
            // Restore the standalone linked view behind its parent, without
            // moving a mixed tab's unrelated panes (group movement is separate).
            let parent = self
                .tabs
                .iter()
                .find(|t| t.sessions().any(|s| s == origin.session))
                .map(|t| (t.id, t.seat, t.area));
            if let Some((parent, seat, area)) = parent
                && let Some(index) = self.tabs.iter().position(|t| {
                    t.seat == seat
                        && t.bodies().count() == 1
                        && matches!(t.body(),Some(Body::Process {run}) if *run==effect.run)
                })
            {
                let mut process = self.tabs.remove(index);
                process.area = area;
                let parent = self
                    .tabs
                    .iter()
                    .position(|t| t.id == parent)
                    .expect("retained native origin");
                self.tabs.insert(parent + 1, process);
            }
        }
        self.open_process(effect.run, true, cx)
            .ok_or("workflow.recovery: Process placement unavailable")?;
        let pid = self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|b| match b {
                Body::Terminal { session, view, .. } if *session == origin.session => {
                    view.read(cx).process_id()
                }
                _ => None,
            });
        let children:Vec<_>=effect.reconciliation["children"].as_array().into_iter().flatten().filter_map(|child|serde_json::from_value::<SessionId>(child["session"].clone()).ok()).map(|session| {
            let status=self.status_of(session);
            serde_json::json!({"session":session,"status":format!("{status:?}"),"waiting":matches!(status,guild_session::status::Status::Waiting(_)),"owned_runtime":self.tabs.iter().flat_map(crate::Tab::bodies).any(|body|matches!(body,Body::Terminal {session:s,view,..} if *s==session && !view.read(cx).exited()))})
        }).collect();
        Ok(
            serde_json::json!({"phase":if live {"origin_live_reused"} else {"resume_launched"},"session":origin.session,"conversation":origin.conversation,"harness":origin.harness,"pid":pid,"ready":false,"continued":false,"resume_attempted":!live,"parent_attention_required":!live,"placement":placement,"reconciliation":effect.reconciliation,"children":children,"native_parent_status":format!("{:?}",self.status_of(origin.session))}),
        )
    }
}

/// The accepted same-seat visible fallback never chooses between several areas.
fn recovery_area(
    areas: &guild_session::Areas,
    original: Option<guild_session::AreaId>,
) -> Result<(guild_session::AreaId, bool), String> {
    if let Some(area) = original.filter(|a| areas.contains(*a)) {
        return Ok((area, false));
    }
    match areas.ids().as_slice() {
        [area] => Ok((*area, true)),
        _ => Err(
            "workflow.recovery: original area unavailable and fallback is ambiguous; no launch"
                .into(),
        ),
    }
}

/// Strict exact-store and process probes. No exported content enters durable receipts.
pub(super) fn inspect_origin(binding: &Binding) -> Result<(), String> {
    inspect_origin_with(binding, process_snapshot)
}

pub(super) fn process_snapshot() -> Result<String, String> {
    let mut command = std::process::Command::new("/bin/ps");
    command.args(["-axww", "-o", "pid=,ppid=,args="]);
    let output = super::super::resume_handoff::inspect_command(
        command,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err("workflow.recovery: OpenCode holder inspection failed".into());
    }
    let snapshot = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    #[cfg(test)]
    if std::env::var_os("WORKFLOW_WITNESS").is_some() {
        // The subprocess fixture owns real PTYs/export executables, but may
        // not borrow or control unrelated OpenCode holders on this host.
        // Production always inspects the complete process table below.
        let mut owned = std::collections::HashSet::from([std::process::id()]);
        loop {
            let before = owned.len();
            for line in snapshot.lines() {
                let mut words = line.split_whitespace();
                if let (Some(pid), Some(ppid)) = (
                    words.next().and_then(|s| s.parse::<u32>().ok()),
                    words.next().and_then(|s| s.parse::<u32>().ok()),
                ) && owned.contains(&ppid)
                {
                    owned.insert(pid);
                }
            }
            if before == owned.len() {
                break;
            }
        }
        return Ok(snapshot
            .lines()
            .filter(|line| {
                line.split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .is_some_and(|p| owned.contains(&p))
            })
            .collect::<Vec<_>>()
            .join("\n"));
    }
    Ok(snapshot)
}

fn inspect_origin_with(
    binding: &Binding,
    process_snapshot: impl FnOnce() -> Result<String, String>,
) -> Result<(), String> {
    let origin = &binding.origin;
    let conversation = origin
        .conversation
        .as_deref()
        .ok_or("workflow.recovery: conversation missing")?;
    if !origin.cwd.is_dir() {
        return Err("workflow.recovery: original cwd is missing; HOME fallback refused".into());
    }
    match origin.harness {
        HarnessKind::Claude => {
            let projects = crate::paths::claude_projects_dir();
            let candidates: Vec<_> = std::fs::read_dir(&projects)
                .map_err(|_| "workflow.recovery: saved Claude conversation unavailable")?
                .filter_map(Result::ok)
                .map(|d| d.path().join(format!("{conversation}.jsonl")))
                .filter(|p| p.is_file())
                .collect();
            if candidates.len() != 1
                || !guild_session::transcript::has_conversation(&projects, conversation)
            {
                return Err(
                    "workflow.recovery: exact Claude conversation missing or ambiguous".into(),
                );
            }
            let file = std::fs::File::open(&candidates[0]).map_err(|e| e.to_string())?;
            let len = file.metadata().map_err(|e| e.to_string())?.len();
            let (cwd, _) =
                guild_session::history::claude::read(&mut std::io::BufReader::new(file), len)
                    .ok_or("workflow.recovery: Claude history metadata unavailable")?;
            if cwd != origin.cwd {
                return Err("workflow.recovery: Claude conversation belongs to another cwd".into());
            }
            super::super::resume_handoff::inspect(conversation, &candidates[0])
        }
        HarnessKind::Codex => {
            let file = guild_session::codex_transcript::path(
                &guild_session::harness::CodexHarness::home(),
                conversation,
            )
            .ok_or("workflow.recovery: exact Codex rollout missing or ambiguous")?;
            use std::io::BufRead;
            let mut line = String::new();
            std::io::BufReader::new(std::fs::File::open(&file).map_err(|e| e.to_string())?)
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            let meta: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if meta["payload"]["cwd"].as_str() != origin.cwd.to_str() {
                return Err("workflow.recovery: Codex conversation belongs to another cwd".into());
            }
            super::super::resume_handoff::inspect(conversation, &file)
        }
        HarnessKind::OpenCode => {
            let path = guild_session::login_path::path()
                .or_else(|| std::env::var_os("PATH"))
                .ok_or("workflow.recovery: executable PATH unavailable")?;
            guild_session::history::opencode::qualify_conversation(
                &path,
                &origin.cwd,
                conversation,
            )?;
            let ps = process_snapshot()?;
            super::super::resume_handoff::checked_holders(&ps, conversation)?;
            if ps.lines().any(|line| {
                line.trim_start()
                    .split_once(char::is_whitespace)
                    .and_then(|(_, rest)| {
                        rest.trim_start()
                            .split_once(char::is_whitespace)
                            .map(|(_, args)| args)
                    })
                    .is_some_and(|args| {
                        HarnessKind::OpenCode
                            .impl_()
                            .detect_in_front(args.trim_start())
                    })
            }) {
                return Err("workflow.recovery: OpenCode runtime holder exists; exact ownership is unknown, no attach or duplicate launch".into());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
