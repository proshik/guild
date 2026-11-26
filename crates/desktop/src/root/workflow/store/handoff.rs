//! Workflow-owned one-shot origin handoff. A reservation is never a retry receipt.
use super::*;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::root::workflow) struct Prompt {
    pub native: reconciliation::Native,
    pub generation: String,
    pub clean: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub turned: bool,
    pub transport: bool,
    pub observed: String,
    pub covered: bool,
    pub pending: bool,
}
pub(in crate::root::workflow) type Prompts = Arc<RwLock<BTreeMap<String, Prompt>>>;
#[derive(Clone)]
pub(in crate::root::workflow) struct Reserved {
    pub run: RunId,
    pub attempt: AttemptId,
    pub revision: u64,
    pub prompt: Prompt,
    pub text: String,
    pub fingerprint: String,
    pub path: PathBuf,
    pub recovery: AttemptId,
    pub verified: bool,
}
fn recovery(history: &History) -> Option<(&EventReceipt, &serde_json::Value)> {
    // Only this explicitly admitted recovery intent authorizes input. Historic
    // inspection and a live-only reveal never manufacture a send privilege.
    history
        .receipts
        .iter()
        .rev()
        .filter_map(|r| {
            let m = r.request.as_ref()?;
            if m.context["operation"] != "origin_recovery" || m.context["handoff_intent"] != true {
                return None;
            }
            let o = history
                .receipts
                .iter()
                .find(|o| o.attempt == r.attempt && o.observation.is_some())?
                .observation
                .as_ref()?;
            (o["recovery"]["phase"] == "resume_launched"
                && o["recovery"]["resume_attempted"] == true)
                .then_some((r, &o["recovery"]))
        })
        .next()
}
fn is_intent(r: &EventReceipt, history: &History) -> bool {
    let Some(m) = r.request.as_ref() else {
        return false;
    };
    if m.context["operation"] == "origin_handoff" {
        return true;
    }
    m.context["operation"] == "origin_recovery"
        && m.context["startup"].is_object()
        && history
            .receipts
            .iter()
            .find(|o| o.attempt == r.attempt && o.observation.is_some())
            .is_none_or(|o| o.observation.as_ref().unwrap()["recovery"]["resume_attempted"] == true)
}
fn intent(history: &History, recovery: AttemptId) -> Option<&EventReceipt> {
    history.receipts.iter().rev().find(|r| {
        r.request.as_ref().is_some_and(|m| {
            (m.context["operation"] == "origin_handoff"
                && m.context["recovery"] == serde_json::json!(recovery))
                || (r.attempt == recovery && is_intent(r, history))
        })
    })
}
fn uncertain_attempt(history: &History) -> Option<&EventReceipt> {
    history.receipts.iter().rev().find(|r| {
        is_intent(r, history)
            && history
                .receipts
                .iter()
                .rev()
                .filter_map(|o| o.observation.as_ref())
                .find(|o| o["handoff"] == serde_json::json!(r.attempt))
                .is_none_or(|o| !matches!(o["stage"].as_str(), Some("observed" | "refused")))
    })
}
pub(super) fn uncertain(history: &History) -> bool {
    uncertain_attempt(history).is_some()
}
pub(super) fn state(history: &History) -> Option<serde_json::Value> {
    let recovered = recovery(history).map(|(r, _)| r);
    let recovery = recovered.or_else(|| uncertain_attempt(history))?;
    let Some(r) = uncertain_attempt(history).or_else(|| intent(history, recovery.attempt)) else {
        return Some(
            serde_json::json!({"stage":"waiting","reason":"eligible_prompt","ready":false,"continued":false}),
        );
    };
    let result = history.receipts.iter().rev().find(|o| {
        o.observation
            .as_ref()
            .is_some_and(|v| v["handoff"] == serde_json::json!(r.attempt))
    });
    let mut value=result.and_then(|r|r.observation.clone()).unwrap_or_else(||serde_json::json!({"stage":"unknown","reason":"reservation_without_observation","ready":false}));
    value["attempt"] = serde_json::json!(r.attempt);
    let context = &r.request.as_ref().unwrap().context;
    let startup = context["startup"].is_object();
    value["path"] = serde_json::json!(if startup { "startup" } else { "prompt" });
    value["digest"] = if startup {
        context["startup"]["digest"].clone()
    } else {
        context["digest"].clone()
    };
    value["recovery"] = if startup {
        serde_json::json!(r.attempt)
    } else {
        context["recovery"].clone()
    };
    let acknowledged = history
        .receipts
        .iter()
        .filter(|r| r.revision > result.map_or(u64::MAX, |r| r.revision))
        .find(|r| {
            r.observation
                .as_ref()
                .is_some_and(|v| v["operation"] == "parent_reconciled")
        });
    value["continued"] = (value["stage"] == "observed"
        && acknowledged.is_some()
        && history.receipts.iter().any(|o| {
            o.revision > acknowledged.map_or(u64::MAX, |r| r.revision)
                && o.observation.as_ref().is_some_and(|v| {
                    v["operation"] == "checkpoint" || v["operation"] == "assignment_delivered"
                })
        }))
    .into();
    Some(value)
}
fn input_gap(
    prompt: &Prompt,
    original: &OriginBinding,
    pid: &serde_json::Value,
) -> Option<&'static str> {
    let n = &prompt.native;
    if !n.identity_qualified
        || n.task
        || n.binding != *original
        || n.holders.len() != 1
        || serde_json::json!(n.holders[0]) != *pid
    {
        return Some("origin_generation");
    }
    if prompt.draft {
        return Some("draft_input");
    }
    // This adapter cannot qualify empty original input, even after a completed turn.
    // Local absence of observed typing is not proof of absence of a restored draft.
    if original.harness == guild_session::HarnessKind::OpenCode {
        return Some("opencode_input_unqualified");
    }
    if !prompt.clean {
        return Some("user_input");
    }
    if prompt.covered || prompt.pending {
        return Some("covered_or_sender");
    }
    if matches!(prompt.observed.as_str(), "working" | "waiting") {
        return Some("busy_or_question");
    }
    if prompt.observed != "done" {
        return Some("completion_unqualified");
    }
    if !prompt.transport {
        return Some("prompt_transport_unqualified");
    }
    None
}
fn clean_history(history: &History) -> History {
    let attempts: Vec<_> = history
        .receipts
        .iter()
        .filter(|r| {
            r.request.as_ref().is_some_and(|m| {
                m.context["operation"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("origin_handoff"))
            })
        })
        .map(|r| r.attempt)
        .collect();
    History {
        run: history.run,
        revision: history.revision,
        receipts: history
            .receipts
            .iter()
            .filter(|r| !attempts.contains(&r.attempt))
            .cloned()
            .collect(),
    }
}
fn facts(
    frozen: &Frozen,
    history: &History,
    observations: &reconciliation::Observations,
) -> Result<serde_json::Value, String> {
    reconciliation::assessment(frozen, &clean_history(history), observations)
}
pub(super) fn summary(
    frozen: &Frozen,
    history: &History,
    current: &serde_json::Value,
) -> Result<String, String> {
    let model = reconstruct(frozen, history)?;
    let nodes = model.graph.nodes();
    let completed: Vec<_> = nodes
        .iter()
        .filter(|n| n.state == NodeState::Finished)
        .map(|n| &n.step)
        .collect();
    let next = nodes.iter().find(|n| {
        !matches!(
            n.state,
            NodeState::Finished | NodeState::Skipped | NodeState::Cancelled
        )
    });
    let basis = &current["basis"];
    let reports:Vec<_>=model.reports.iter().map(|r|serde_json::json!({"producer":r["producer"],"digest":digest(&serde_json::to_vec(r).unwrap_or_default())})).collect();
    let statuses: BTreeMap<_, _> = basis["native"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, n)| (id, n["status"].clone()))
        .collect();
    let snapshot = serde_json::json!({"run":frozen.run,"origin":frozen.origin,"contract":model.contract.as_ref().map(|c|&c.sha256),"source":model.source.sha256,"completed":completed,"children":basis["children"],"status":statuses,"reports":reports,"review_usage":{"used":model.review_ledger.used(),"reserved":model.review_ledger.reserved(),"remaining":model.review_ledger.remaining()},"evidence":basis["artifacts"],"leases":basis["leases"],"blockers":current["blockers"],"resource_owners":basis["preparation"],"next":next,"assessment":current["fingerprint"]});
    let text = format!(
        "Workflow recovery handoff (current facts, not the original task): {}\nInspect `guild workflow status {} --json` and request a fresh `workflow reconcile` assessment. Only the exact original parent may explicitly acknowledge the current source-qualified assessment. Continue the next unfinished configured checkpoint through existing guards; preserve completed work, conversation and review accounting. Do not answer pending approvals or replay uncertain effects. This handoff is not an acknowledgement or evidence of continued orchestration.",
        serde_json::to_string(&snapshot).map_err(|e| e.to_string())?,
        frozen.run
    );
    if text.len() > 24 * 1024 {
        return Err("summary_bounds".into());
    }
    Ok(text)
}
impl Runs {
    pub(in crate::root::workflow) fn prepare_handoff(
        &mut self,
        run: RunId,
    ) -> Result<Option<Reserved>, String> {
        let Some(path) = self.paths.get(&run).cloned() else {
            return Ok(None);
        };
        // Only the active exclusive journal owner can admit this effect.
        let Some(journal) = self.journals.get(&run) else {
            return Ok(None);
        };
        let frozen = read_frozen(&path)?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        if reconstruct(&frozen, &history)?
            .graph
            .nodes()
            .iter()
            .any(|n| n.state == NodeState::Cancelled)
        {
            return Ok(None);
        }
        let Some((recovered, observation)) = recovery(&history) else {
            return Ok(None);
        };
        if state(&history).is_some_and(|s| s["stage"] != "waiting" && s["stage"] != "refused") {
            return Ok(None);
        }
        if uncertain(&history) || !journal.unresolved().is_empty() {
            return Ok(None);
        }
        let Some(prompt) = self
            .prompts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&frozen.origin.session.to_string())
            .cloned()
        else {
            return Ok(None);
        };
        let mut original = frozen.origin.clone();
        original.area = None;
        if input_gap(&prompt, &original, &observation["pid"]).is_some() {
            return Ok(None);
        }
        // Retained settings must still be exactly the ones the resume admitted.
        let expected = &recovered.request.as_ref().unwrap().context["binding"];
        if expected["selection"]
            != serde_json::to_value(&prompt.native.selection).map_err(|e| e.to_string())?
            || expected["permission"]
                != serde_json::to_value(prompt.native.permission).map_err(|e| e.to_string())?
        {
            return Ok(None);
        }
        let current = facts(&frozen, &history, &self.observations)?;
        if current["blockers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|b| {
                matches!(
                    b["code"].as_str(),
                    Some(
                        "source_changed"
                            | "contract_changed"
                            | "origin_unknown"
                            | "holder_unknown"
                            | "effect_unknown"
                    )
                )
            })
        {
            return Ok(None);
        }
        let text = summary(&frozen, &history, &current)?;
        let fingerprint = current["fingerprint"]
            .as_str()
            .ok_or("handoff assessment missing")?
            .to_owned();
        let count = history
            .receipts
            .iter()
            .filter(|r| {
                r.request
                    .as_ref()
                    .is_some_and(|m| m.context["operation"] == "origin_handoff")
            })
            .count();
        let mutation = Mutation::new(
            &format!("handoff-{}-{count}", recovered.attempt),
            history.revision,
            serde_json::json!({"operation":"origin_handoff","recovery":recovered.attempt,"origin":frozen.origin,"generation":prompt,"assessment":fingerprint,"digest":digest(text.as_bytes()),"ready":true,"continued":false}),
        )?;
        let journal = self.journals.get_mut(&run).unwrap();
        let (r, fresh) = journal.reserve(&mutation)?;
        if fresh {
            self.active_handoffs.insert(r.attempt);
        }
        Ok(fresh.then_some(Reserved {
            run,
            attempt: r.attempt,
            revision: r.revision,
            prompt,
            text,
            fingerprint,
            path,
            recovery: recovered.attempt,
            verified: false,
        }))
    }
    pub(in crate::root::workflow) fn handoff_preflight(
        &self,
        effect: &Reserved,
    ) -> Result<(), String> {
        let history = Journal::inspect(&effect.path)?;
        let frozen = read_frozen(&effect.path)?;
        if uncertain_attempt(&history).is_some_and(|r| r.attempt != effect.attempt)
            || history.revision != effect.revision
            || recovery(&history).is_none_or(|(r, _)| r.attempt != effect.recovery)
            || facts(&frozen, &history, &self.observations)?["fingerprint"] != effect.fingerprint
        {
            return Err("stale_snapshot".into());
        }
        if self
            .prompts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&effect.prompt.native.binding.session.to_string())
            != Some(&effect.prompt)
        {
            return Err("stale_generation".into());
        }
        Ok(())
    }
    pub(in crate::root::workflow) fn observe_handoff(
        &mut self,
        effect: &Reserved,
        stage: &str,
        reason: &str,
    ) -> Result<(), String> {
        let j = self
            .journals
            .get_mut(&effect.run)
            .ok_or("handoff claim unavailable")?;
        j.resolve(effect.attempt,effect.revision,serde_json::json!({"operation":"origin_handoff_submission","handoff":effect.attempt,"stage":stage,"reason":reason,"ready":true,"continued":false}))?;
        self.active_handoffs.remove(&effect.attempt);
        Ok(())
    }
    pub(in crate::root::workflow) fn settle_handoff(
        &mut self,
        effect: &Reserved,
        observed: bool,
    ) -> Result<(), String> {
        let j = self
            .journals
            .get_mut(&effect.run)
            .ok_or("handoff claim unavailable")?;
        let key = format!("handoff-observation-{}", effect.attempt);
        let mutation = Mutation::new(
            &key,
            j.revision(),
            serde_json::json!({"operation":"origin_handoff_observation","handoff":effect.attempt}),
        )?;
        let (r, fresh) = j.reserve(&mutation)?;
        if fresh {
            j.resolve(r.attempt,r.revision,serde_json::json!({"operation":"origin_handoff_observed","handoff":effect.attempt,"stage":if observed {"observed"} else {"unknown"},"reason":if observed {"turn_start_observed"} else {"submission_without_qualified_turn_start"},"ready":true,"continued":false}))?;
        }
        Ok(())
    }
    pub(super) fn decorate_handoff(&self, reply: &mut WorkflowReply) {
        let (run, value) = match reply {
            WorkflowReply::Snapshot { run, snapshot, .. }
            | WorkflowReply::Receipt { run, snapshot, .. } => (*run, snapshot),
            _ => return,
        };
        let Some(path) = self.paths.get(&run) else {
            return;
        };
        let Ok(history) = Journal::inspect(path) else {
            return;
        };
        let Some(mut state) = state(&history) else {
            return;
        };
        if state["reason"] == "reservation_without_observation"
            && state["attempt"]
                .as_str()
                .and_then(|s| s.parse::<AttemptId>().ok())
                .is_some_and(|a| self.active_handoffs.contains(&a))
        {
            state["stage"] = "reserved".into();
        }
        if (state["stage"] == "waiting" || state["stage"] == "refused")
            && let Ok(frozen) = read_frozen(path)
        {
            let prompts = self.prompts.read().unwrap_or_else(|e| e.into_inner());
            let mut original = frozen.origin.clone();
            original.area = None;
            state["reason"] = serde_json::json!(
                prompts
                    .get(&frozen.origin.session.to_string())
                    .and_then(|p| input_gap(p, &original, &recovery(&history).unwrap().1["pid"]))
                    .unwrap_or(
                        if original.harness == guild_session::HarnessKind::OpenCode {
                            "opencode_input_unqualified"
                        } else {
                            "snapshot_recheck"
                        }
                    )
            );
        }
        value["handoff"] = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_prompt_requires_actual_completion_empty_input_and_exact_native_holder() {
        let binding = OriginBinding {
            session: SessionId::new(),
            project: guild_session::ProjectId::new(),
            workspace: guild_session::WorkspaceId::new(),
            area: None,
            harness: guild_session::HarnessKind::Claude,
            conversation: Some("exact-original".into()),
            cwd: PathBuf::from("/fixture"),
        };
        let native = reconciliation::Native {
            binding: binding.clone(),
            task: false,
            selection: None,
            permission: Some(guild_session::PermissionMode::Manual),
            status: "Done".into(),
            holders: vec![123],
            identity_qualified: true,
        };
        let safe = Prompt {
            native,
            generation: "owned-generation".into(),
            clean: true,
            draft: false,
            turned: true,
            transport: true,
            observed: "done".into(),
            covered: false,
            pending: false,
        };
        for kind in guild_session::HarnessKind::ALL {
            let mut original = binding.clone();
            original.harness = kind;
            let mut p = safe.clone();
            p.native.binding.harness = kind;
            assert_eq!(
                input_gap(&p, &original, &serde_json::json!(123)),
                (kind == guild_session::HarnessKind::OpenCode)
                    .then_some("opencode_input_unqualified")
            );
            for status in ["working", "waiting", "unknown", "idle", "failed"] {
                p.observed = status.into();
                assert!(
                    input_gap(&p, &original, &serde_json::json!(123)).is_some(),
                    "{kind:?} {status}"
                );
            }
        }
        for mutate in [
            |p: &mut Prompt| p.clean = false,
            |p: &mut Prompt| p.transport = false,
            |p: &mut Prompt| p.covered = true,
            |p: &mut Prompt| p.pending = true,
            |p: &mut Prompt| p.native.identity_qualified = false,
            |p: &mut Prompt| p.native.holders.push(456),
            |p: &mut Prompt| p.native.binding.conversation = Some("foreign".into()),
        ] {
            let mut p = safe.clone();
            mutate(&mut p);
            assert!(input_gap(&p, &binding, &serde_json::json!(123)).is_some());
        }
    }
}
