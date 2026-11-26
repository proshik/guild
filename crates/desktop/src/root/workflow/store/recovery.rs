//! One compound serialized recovery/startup reservation; no implied continuation.
use super::*;
use guild_session::{HarnessKind, PermissionMode, SessionRecord, harness::LaunchSelection};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::root::workflow) struct Binding {
    pub origin: OriginBinding,
    pub selection: Option<LaunchSelection>,
    pub permission: Option<PermissionMode>,
}
impl Binding {
    pub(in crate::root::workflow) fn from_record(
        record: &SessionRecord,
        area: Option<guild_session::AreaId>,
    ) -> Result<Self, String> {
        let harness = record
            .runs
            .harness()
            .ok_or("workflow.recovery: saved harness is missing")?;
        let conversation = if harness == HarnessKind::Claude {
            Some(record.id.to_string())
        } else {
            record.harness_session.clone()
        };
        if record.task || conversation.as_deref().is_none_or(str::is_empty) {
            return Err(
                "workflow.recovery: exact original parent conversation is unavailable".into(),
            );
        }
        Ok(Self {
            origin: OriginBinding {
                session: record.id,
                project: record.project,
                workspace: record.workspace,
                area,
                harness,
                conversation,
                cwd: record.cwd.clone(),
            },
            selection: record.launch_selection.clone(),
            permission: record.launch_permission,
        })
    }
}
#[derive(Clone)]
pub(in crate::root::workflow) struct Startup {
    pub text: String,
    pub fingerprint: String,
    pub witnesses: BTreeMap<String, reconciliation::Native>,
}
#[derive(Clone)]
pub(in crate::root::workflow) struct Reserved {
    pub startup: Option<Startup>,
    pub run: RunId,
    pub attempt: AttemptId,
    pub revision: u64,
    pub path: PathBuf,
    pub frozen: String,
    pub binding: Binding,
    pub original: OriginBinding,
    pub source: SourceIdentity,
    pub reconciliation: serde_json::Value,
    pub caller: SessionId,
    pub peer: OriginBinding,
    pub peer_pid: Option<u32>,
    pub request_epoch: u64,
    pub inspected: bool,
    pub stopped_qualified: bool,
    pub inspection: Option<Result<(), String>>,
}
impl Runs {
    pub(super) fn recover_run(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &ControlRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        let caller =
            caller.ok_or("workflow.recovery: actual parent or assigned-child caller required")?;
        let peer = peer
            .ok_or("workflow.recovery: native caller binding required")?
            .clone();
        let record = self
            .recovery_records
            .iter()
            .find(|r| r.id == frozen.origin.session)
            .ok_or(
                "workflow.recovery: saved origin record is unavailable; no new-session fallback",
            )?;
        let binding = Binding::from_record(record, None)?;
        if binding.origin.harness != frozen.origin.harness
            || binding.origin.cwd != frozen.origin.cwd
            || binding.origin.project != frozen.origin.project
            || frozen
                .origin
                .conversation
                .as_ref()
                .is_some_and(|c| Some(c) != binding.origin.conversation.as_ref())
        {
            return Err(
                "workflow.recovery: frozen original conversation/harness/project binding changed"
                    .into(),
            );
        }
        if let std::collections::btree_map::Entry::Vacant(e) = self.journals.entry(frozen.run) {
            e.insert(Journal::open(path)?);
        }
        let journal = self
            .journals
            .get_mut(&frozen.run)
            .ok_or("workflow.recovery: run claim unavailable")?;
        // Retries and concurrent stale requests observe the existing attempt, never replay it.
        let previous = journal
            .receipts()
            .iter()
            .rev()
            .find(|r| {
                r.request
                    .as_ref()
                    .is_some_and(|m| m.context["operation"] == "origin_recovery")
            })
            .cloned();
        if let Some(previous) = previous {
            let request = previous.request.as_ref().unwrap();
            if request.request_id == spec.request_id
                && request.context["spec"]
                    != serde_json::to_value(spec).map_err(|e| e.to_string())?
            {
                return Err("workflow.request: recovery key context conflict".into());
            }
            if !journal
                .receipts()
                .iter()
                .any(|r| r.attempt == previous.attempt && r.observation.is_some())
            {
                return Err("workflow.recovery: prior resume reservation is delivery-unknown; reconcile actual owner, do not replay".into());
            }

            if request.request_id == spec.request_id
                || (spec.expected_revision < journal.revision()
                    && spec.expected_revision >= previous.preceding_revision)
            {
                if request.context["binding"]
                    != serde_json::to_value(&binding).map_err(|e| e.to_string())?
                {
                    return Err("workflow.recovery: retry binding changed".into());
                }
                let history = History {
                    run: frozen.run,
                    revision: journal.revision(),
                    receipts: journal.receipts().to_vec(),
                };
                let model = reconstruct(frozen, &history)?;
                return Ok(WorkflowReply::Receipt {
                    run: frozen.run,
                    revision: history.revision,
                    attempt: previous.attempt,
                    receipt: Box::new(previous),
                    snapshot: snapshot(frozen, &history, &model),
                });
            }
        }
        if !journal.unresolved().is_empty() {
            return Err("workflow.recovery: unresolved effect/launch receipt; delivery unknown, automatic replay refused".into());
        }
        let source =
            super::super::source::capture(&frozen.start.target, frozen.source.base.as_deref())?;
        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let reconciliation = reconciliation(frozen, &history, &source)?;
        let startup = startup_summary(frozen, &history, &self.observations)?;
        let context = serde_json::json!({"operation":"origin_recovery","authorization":if self.trusted_navigation {"native_process_navigation"} else {"original_parent_request"},"handoff_intent":true,"startup":startup.as_ref().map(|s|serde_json::json!({"path":"startup","digest":digest(s.text.as_bytes()),"assessment":s.fingerprint,"ready":false,"continued":false})),"phase":"resume_reserved","binding":binding,"caller":caller,"peer":peer,"peer_pid":self.recovery_caller_pid,"source":source.sha256,"reconciliation":reconciliation,"spec":spec,"continued":false});
        let mutation = Mutation::new(&spec.request_id, spec.expected_revision, context)?;
        let (receipt, fresh) = journal.reserve(&mutation)?;
        if fresh {
            self.recovery_effect = Some(Reserved {
                startup,
                run: frozen.run,
                attempt: receipt.attempt,
                revision: journal.revision(),
                path: path.to_path_buf(),
                frozen: frozen.sha256.clone(),
                binding,
                original: frozen.origin.clone(),
                source,
                reconciliation,
                caller,
                peer,
                peer_pid: self.recovery_caller_pid,
                request_epoch: 0,
                inspected: false,
                stopped_qualified: false,
                inspection: None,
            });
        }
        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        Ok(WorkflowReply::Receipt {
            run: frozen.run,
            revision: history.revision,
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
    pub(in crate::root::workflow) fn recovery_preflight(effect: &Reserved) -> Result<(), String> {
        let frozen = read_frozen(&effect.path)?;
        let history = Journal::inspect(&effect.path)?;
        if frozen.sha256 != effect.frozen
            || history.revision != effect.revision
            || !history
                .receipts
                .iter()
                .any(|r| r.attempt == effect.attempt && r.request.is_some())
        {
            return Err("workflow.recovery: stale source/revision callback".into());
        }
        let source =
            super::super::source::capture(&effect.source.target, effect.source.base.as_deref())?;
        if source != effect.source {
            return Err("workflow.recovery: source changed during asynchronous holder inspection; retry explicitly".into());
        }
        if reconciliation(&frozen, &history, &source)? != effect.reconciliation {
            return Err(
                "workflow.recovery: retained child/source/review state changed during inspection"
                    .into(),
            );
        }
        Ok(())
    }
    pub(in crate::root::workflow) fn startup_preflight(
        &self,
        effect: &Reserved,
    ) -> Result<(), String> {
        let Some(startup) = &effect.startup else {
            return Ok(());
        };
        let frozen = read_frozen(&effect.path)?;
        let mut history = Journal::inspect(&effect.path)?;
        history.receipts.retain(|r| r.attempt != effect.attempt);
        let current = startup_summary(&frozen, &history, &self.observations)?;
        if current
            .as_ref()
            .is_none_or(|s| s.fingerprint != startup.fingerprint || s.text != startup.text)
        {
            return Err("workflow.recovery: startup snapshot changed before native launch; no payload replay".into());
        }
        Ok(())
    }
    pub(in crate::root::workflow) fn settle_startup(
        &mut self,
        effect: &Reserved,
        observed: bool,
    ) -> Result<(), String> {
        let j = self
            .journals
            .get_mut(&effect.run)
            .ok_or("startup claim unavailable")?;
        let mutation = Mutation::new(
            &format!("startup-observation-{}", effect.attempt),
            j.revision(),
            serde_json::json!({"operation":"origin_handoff_observation","handoff":effect.attempt}),
        )?;
        let (r, fresh) = j.reserve(&mutation)?;
        if fresh {
            j.resolve(r.attempt,r.revision,serde_json::json!({"operation":"origin_handoff_observed","handoff":effect.attempt,"stage":if observed {"observed"} else {"unknown"},"path":"startup","reason":if observed {"turn_start_observed"} else {"startup_without_qualified_turn_start"},"ready":false,"continued":false}))?;
        }
        Ok(())
    }
    pub(in crate::root::workflow) fn observe_recovery(
        &mut self,
        effect: &Reserved,
        observation: serde_json::Value,
    ) -> Result<(), String> {
        let journal = self
            .journals
            .get_mut(&effect.run)
            .ok_or("workflow.recovery: retained run claim missing")?;
        let mut value = serde_json::json!({"operation":"recovery_observed","recovery":observation});
        if effect.startup.is_some() && value["recovery"]["resume_attempted"] == true {
            value["handoff"] = serde_json::json!(effect.attempt);
            value["stage"] =
                serde_json::json!(if value["recovery"]["phase"] == "resume_launched" {
                    "submitted"
                } else {
                    "unknown"
                });
            value["ready"] = false.into();
            value["continued"] = false.into();
        }
        journal.resolve(effect.attempt, effect.revision, value)?;
        Ok(())
    }
}

fn startup_summary(
    frozen: &Frozen,
    history: &History,
    observations: &reconciliation::Observations,
) -> Result<Option<Startup>, String> {
    // OpenCode does not provide qualified empty-input readiness. Restore only;
    // neither reserve nor supply a startup payload that could bypass that policy.
    if frozen.origin.harness == guild_session::HarnessKind::OpenCode || handoff::uncertain(history)
    {
        return Ok(None);
    }
    let mut current = reconciliation::assessment(frozen, history, observations)?;
    let id = frozen.origin.session.to_string();
    if let Some(native) = current["basis"]["native"].as_object_mut() {
        native.remove(&id);
    }
    if let Some(blockers) = current["blockers"].as_array_mut() {
        blockers.retain(|b| {
            !(b["session"] == serde_json::json!(frozen.origin.session)
                && matches!(
                    b["code"].as_str(),
                    Some("origin_unknown" | "status_unknown")
                ))
        });
        if blockers.iter().any(|b| {
            matches!(
                b["code"].as_str(),
                Some("source_changed" | "contract_changed" | "effect_unknown" | "holder_unknown")
            )
        }) {
            return Ok(None);
        }
    }
    // The stopped holder is independently qualified off-thread before native
    // launch. It is not a prompt-readiness witness and is never called ready.
    current["basis"]["blockers"] = current["blockers"].clone();
    current["clear"] =
        serde_json::json!(current["blockers"].as_array().is_some_and(|b| b.is_empty()));
    current["fingerprint"] = serde_json::Value::Null;
    let fingerprint = digest(&serde_json::to_vec(&current).map_err(|e| e.to_string())?);
    current["fingerprint"] = fingerprint.clone().into();
    let text = handoff::summary(frozen, history, &current)?;
    let witnesses =
        serde_json::from_value(current["basis"]["native"].clone()).map_err(|e| e.to_string())?;
    Ok(Some(Startup {
        text,
        fingerprint,
        witnesses,
    }))
}

/// Navigation alone adds no continuation barrier. Every actual or uncertain
/// resume remains a barrier, even after a later successful live-view reveal.
/// Unknown legacy observations fail closed; only native no-launch facts waive it.
pub(super) fn has_resume_barrier(history: &History) -> bool {
    history
        .receipts
        .iter()
        .filter(|r| {
            r.request
                .as_ref()
                .is_some_and(|m| m.context["operation"] == "origin_recovery")
        })
        .any(|request| {
            let state = history
                .receipts
                .iter()
                .find(|r| r.attempt == request.attempt && r.observation.is_some())
                .and_then(|r| r.observation.as_ref())
                .map(|o| &o["recovery"]);
            match state {
                Some(state) if state["phase"] == "resume_launched" => true,
                Some(state) if state["resume_attempted"] == true => true,
                Some(state) if state["phase"] == "origin_live_reused" => false,
                Some(state)
                    if state["phase"] == "recovery_gap" && state["resume_attempted"] == false =>
                {
                    false
                }
                _ => true,
            }
        })
}

pub(super) fn requires_reconciliation(history: &History) -> bool {
    // This historical flag is refined against current source/native facts by
    // reconciliation::guard before every admission. A later reveal alone is
    // never an acknowledgement.
    let acknowledged = history
        .receipts
        .iter()
        .rev()
        .find(|r| {
            r.observation
                .as_ref()
                .is_some_and(|o| o["operation"] == "parent_reconciled")
        })
        .map(|r| r.revision);
    if let Some(revision) = acknowledged {
        let later = History {
            run: history.run,
            revision: history.revision,
            receipts: history
                .receipts
                .iter()
                .filter(|r| r.revision > revision)
                .cloned()
                .collect(),
        };
        has_resume_barrier(&later)
    } else {
        has_resume_barrier(history)
    }
}

pub(super) fn view(history: &History) -> serde_json::Value {
    let Some(request) = history.receipts.iter().rev().find(|r| {
        r.request
            .as_ref()
            .is_some_and(|m| m.context["operation"] == "origin_recovery")
    }) else {
        return serde_json::Value::Null;
    };
    let observed = history
        .receipts
        .iter()
        .find(|r| r.attempt == request.attempt && r.observation.is_some())
        .and_then(|r| r.observation.as_ref());
    let mut state = observed
        .and_then(|v| v.get("recovery").filter(|state| state.is_object()).cloned())
        .unwrap_or(serde_json::json!({"phase":"resume_reserved","ready":false,"continued":false}));
    state["parent_attention_required"] = requires_reconciliation(history).into();
    serde_json::json!({"attempt":request.attempt,"reservation_revision":request.revision,"state":state,"source":request.request.as_ref().unwrap().context["source"]})
}

fn reconciliation(
    frozen: &Frozen,
    history: &History,
    source: &SourceIdentity,
) -> Result<serde_json::Value, String> {
    let model = reconstruct(frozen, history)?;
    let mut children = Vec::new();
    for row in model
        .assignments
        .as_array()
        .ok_or("workflow.recovery: corrupt assignment rows")?
    {
        let observed = &row["observation"];
        if observed["bound"] != true {
            continue;
        }
        let session: SessionId =
            serde_json::from_value(observed["session"].clone()).map_err(|e| e.to_string())?;
        let checkout = PathBuf::from(
            observed["checkout"]
                .as_str()
                .ok_or("workflow.recovery: assignment checkout missing")?,
        );
        let current = super::super::source::capture(&checkout, row["plan"]["base"].as_str());
        children.push(serde_json::json!({"session":session,"checkout":checkout,"source":current.as_ref().ok().map(|s|&s.sha256),"gap":current.err()}));
    }
    Ok(
        serde_json::json!({"source":source.sha256,"source_changed":*source!=model.source,"children":children,"review_ledger":model.review_ledger,"review_rounds":model.reviews.len(),"resource_recovery":"parent_attention_required","live_review_usage":"not_reexecuted_or_inferred"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_and_prelaunch_refusal_do_not_gate_but_resume_uncertainty_survives_reveal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run");
        let mut journal = Journal::create(&path, RunId::new()).unwrap();
        for (index, state, expected) in [
            // A legacy live-only receipt is unambiguous, even with the old
            // incorrectly recorded attention flag.
            (
                0,
                serde_json::json!({"phase":"origin_live_reused","parent_attention_required":true}),
                false,
            ),
            (
                1,
                serde_json::json!({"phase":"recovery_gap","resume_attempted":false}),
                false,
            ),
            (
                2,
                serde_json::json!({"phase":"recovery_gap","resume_attempted":true}),
                true,
            ),
            (
                3,
                serde_json::json!({"phase":"origin_live_reused","resume_attempted":false}),
                true,
            ),
        ] {
            let mutation = Mutation::new(
                &format!("navigate-{index}"),
                journal.revision(),
                serde_json::json!({"operation":"origin_recovery"}),
            )
            .unwrap();
            let (reserved, _) = journal.reserve(&mutation).unwrap();
            assert!(
                requires_reconciliation(&Journal::inspect(&path).unwrap()),
                "an unresolved/crashed navigation reservation must fail closed"
            );
            journal
                .resolve(
                    reserved.attempt,
                    reserved.revision,
                    serde_json::json!({"operation":"recovery_observed","recovery":state}),
                )
                .unwrap();
            let history = Journal::inspect(&path).unwrap();
            assert_eq!(requires_reconciliation(&history), expected);
            assert_eq!(
                view(&history)["state"]["parent_attention_required"],
                expected
            );
        }
        // A known successful launch and an old gap without no-launch evidence
        // also require reconciliation independently of any later live reveal.
        for state in [
            serde_json::json!({"phase":"resume_launched","resume_attempted":false}),
            serde_json::json!({"phase":"recovery_gap"}),
        ] {
            let path = dir.path().join(uuid::Uuid::new_v4().to_string());
            let mut journal = Journal::create(&path, RunId::new()).unwrap();
            let (reserved, _) = journal
                .reserve(
                    &Mutation::new(
                        "attempt",
                        0,
                        serde_json::json!({"operation":"origin_recovery"}),
                    )
                    .unwrap(),
                )
                .unwrap();
            journal
                .resolve(
                    reserved.attempt,
                    reserved.revision,
                    serde_json::json!({"operation":"recovery_observed","recovery":state}),
                )
                .unwrap();
            assert!(requires_reconciliation(&Journal::inspect(&path).unwrap()));
        }
    }
}
