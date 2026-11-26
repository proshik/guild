//! Parent-owned correction of a checked review, using the assigned native session.
use super::*;
use reports::{assignments, matches_peer};

#[derive(Clone)]
pub(in crate::root::workflow) struct Pending {
    pub run: RunId,
    pub attempt: AttemptId,
    pub revision: u64,
    pub path: PathBuf,
    pub source: SourceIdentity,
    pub origin: OriginBinding,
    pub native: Option<serde_json::Value>,
    pub prompt: handoff::Prompt,
    pub text: String,
    pub verified: bool,
}
fn retained(j: &Journal, run: RunId) -> History {
    History {
        run,
        revision: j.revision(),
        receipts: j.receipts().to_vec(),
    }
}
pub(super) fn reduce(model: &mut Model, history: &History, r: &EventReceipt) -> Result<(), String> {
    if let Some(m) = &r.request
        && m.context["operation"] == "same_session_correction"
    {
        model.review_ledger =
            serde_json::from_value(m.context["ledger"].clone()).map_err(|e| e.to_string())?;
        model.review_ledger.validate()?;
    }
    let Some(o) = &r.observation else {
        return Ok(());
    };
    if o["operation"] != "correction_delivery"
        || !matches!(
            o["stage"].as_str(),
            Some("observed" | "parent_checkpoint_required")
        )
    {
        return Ok(());
    }
    let attempt: AttemptId =
        serde_json::from_value(o["correction"].clone()).map_err(|e| e.to_string())?;
    let m = &history
        .receipts
        .iter()
        .find(|r| r.attempt == attempt && r.request.is_some())
        .ok_or("correction reservation missing")?
        .request
        .as_ref()
        .unwrap()
        .context;
    if model
        .graph
        .nodes()
        .iter()
        .any(|n| n.state == NodeState::Cancelled)
    {
        return Ok(());
    }
    let from = m["from"].as_str().ok_or("correction source missing")?;
    let to = m["to"].as_str().ok_or("correction target missing")?;
    model.graph.correct(from, to)?;
    model.graph.admitted(to)?;
    model.graph.activity(to,if o["stage"]=="parent_checkpoint_required" {"Checked contract findings returned to the original parent; edit pinned artifacts and submit a fresh checkpoint. No harness input sent."} else {"Correction delivered to the same assigned conversation; fresh source-qualified report required"})?;
    Ok(())
}
impl Runs {
    pub(super) fn retry_correction(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &RetryRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<Option<WorkflowReply>, String> {
        let history = Journal::inspect(path)?;
        let Some(review) = history.receipts.iter().find(|r| {
            r.attempt == spec.attempt
                && r.request
                    .as_ref()
                    .is_some_and(|m| m.context["operation"] == "external_review")
        }) else {
            return Ok(None);
        };
        if caller != Some(frozen.origin.session)
            || peer.is_none_or(|p| {
                p.session != frozen.origin.session
                    || p.conversation != frozen.origin.conversation
                    || p.cwd != frozen.origin.cwd
                    || p.project != frozen.origin.project
                    || p.workspace != frozen.origin.workspace
                    || p.harness != frozen.origin.harness
            })
        {
            return Err("workflow.correction: exact original parent required".into());
        }
        if let Some(old) = history.receipts.iter().find(|r| {
            r.request
                .as_ref()
                .is_some_and(|m| m.request_id == spec.request_id)
        }) {
            if old.request.as_ref().unwrap().context["spec"] != serde_json::json!(spec) {
                return Err("workflow.correction: request context conflict".into());
            }
            let model = reconstruct(frozen, &history)?;
            return Ok(Some(WorkflowReply::Receipt {
                run: frozen.run,
                revision: history.revision,
                attempt: old.attempt,
                receipt: Box::new(old.clone()),
                snapshot: snapshot(frozen, &history, &model),
            }));
        }
        if history.revision != spec.expected_revision {
            return Err("workflow.correction: stale expected revision".into());
        }
        let model = reconstruct(frozen, &history)?;
        if model
            .graph
            .nodes()
            .iter()
            .any(|n| n.state == NodeState::Cancelled)
        {
            return Err("workflow.correction: cancelled graph is history only".into());
        }
        let row = model
            .reviews
            .iter()
            .find(|r| r["attempt"] == serde_json::json!(review.attempt))
            .ok_or("workflow.correction: original review row missing")?;
        if row["result"]["archive"]["classification"] != "findings"
            || row["result"]["history_only"] == true
        {
            return Err(
                "workflow.correction: complete source-qualified mandatory findings required".into(),
            );
        }
        let round: RevustRound =
            serde_json::from_value(row["round"].clone()).map_err(|e| e.to_string())?;
        let from = row["step"]
            .as_str()
            .ok_or("workflow.correction: review node missing")?;
        let edges = frozen
            .definition
            .corrections
            .iter()
            .filter(|e| e.from == from)
            .collect::<Vec<_>>();
        if edges.len() != 1 {
            return Err("workflow.correction: no unique configured target".into());
        }
        let to = &edges[0].to;
        let mut graph = model.graph.clone();
        graph.correct(from, to)?;
        let assigned = assignments(&history)?
            .into_iter()
            .filter(|a| a.plan.step == *to)
            .collect::<Vec<_>>();
        if assigned.is_empty()
            && frozen.definition.steps.iter().any(|s| {
                s.id == *to
                    && s.action == Action::ContractPrepare
                    && frozen.definition.roles[&s.role].owner == Owner::Parent
            })
        {
            let source = super::super::source::capture(
                &round.subject.source.target,
                round.subject.source.base.as_deref(),
            )?;
            if source != round.subject.source || source != model.source {
                return Err(
                    "workflow.correction: contract source changed before parent decision".into(),
                );
            }
            let job = serde_json::from_value(row["job"].clone()).map_err(|e| e.to_string())?;
            let checked = review::archive::checked(frozen, path, &model, from, &round, &job)?;
            if checked != row["result"]["archive"] {
                return Err("workflow.correction: contract review archive changed".into());
            }
            let claim = format!("correction-{}", round.subject.round);
            if history.receipts.iter().any(|r| {
                r.request
                    .as_ref()
                    .is_some_and(|m| m.context["claim"] == claim)
            }) {
                return Err("workflow.correction: contract correction already admitted; use original receipt and fresh checkpoint".into());
            }
            let mut ledger = model.review_ledger.clone();
            ledger.claim_correction(&claim)?;
            if let std::collections::btree_map::Entry::Vacant(e) = self.journals.entry(frozen.run) {
                e.insert(Journal::open(path)?);
            }
            let j = self.journals.get_mut(&frozen.run).unwrap();
            let mutation = Mutation::new(
                &spec.request_id,
                spec.expected_revision,
                serde_json::json!({"operation":"same_session_correction","mode":"parent_checkpoint","spec":spec,"caller":caller,"review":spec.attempt,"round":round.sha256,"from":from,"to":to,"claim":claim,"ledger":ledger,"source":source}),
            )?;
            let (receipt, fresh) = j.reserve(&mutation)?;
            if fresh {
                j.resolve(receipt.attempt,receipt.revision,serde_json::json!({"operation":"correction_delivery","correction":receipt.attempt,"stage":"parent_checkpoint_required","session":frozen.origin.session,"input_sent":false}))?;
            }
            let h = retained(j, frozen.run);
            let model = reconstruct(frozen, &h)?;
            return Ok(Some(WorkflowReply::Receipt {
                run: frozen.run,
                revision: h.revision,
                attempt: receipt.attempt,
                receipt: Box::new(receipt),
                snapshot: snapshot(frozen, &h, &model),
            }));
        }
        if assigned.len() != 1 {
            return Err("workflow.correction: target has no unique assigned session; parent-owned contract correction needs its own checked checkpoint".into());
        }
        let a = &assigned[0];
        let prompt = self
            .prompts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&a.session.to_string())
            .cloned()
            .ok_or("workflow.correction: assigned native prompt unavailable")?;
        if !prompt.native.task
            || !prompt.native.identity_qualified
            || prompt.native.holders.len() != 1
            || !matches_peer(a, &prompt.native.binding)?
            || prompt.native.selection.as_ref() != Some(&a.plan.selection)
            || prompt.draft
            || prompt.covered
            || prompt.pending
            || !prompt.turned
            || !matches!(prompt.observed.as_str(), "done" | "idle")
            || prompt.native.permission.is_none()
            || !prompt.transport
        {
            return Err("workflow.correction: assigned session must be the same owned done/idle prompt, without draft, unobserved/unknown input, question, permission or competing sender".into());
        }
        let source = super::super::source::capture(&a.checkout, Some(&a.plan.base))?;
        if source != round.subject.source || source != model.source {
            return Err("workflow.correction: reviewed source changed; requalify instead of sending stale findings".into());
        }
        let job = serde_json::from_value(row["job"].clone()).map_err(|e| e.to_string())?;
        let checked = review::archive::checked(frozen, path, &model, from, &round, &job)?;
        if checked != row["result"]["archive"] {
            return Err("workflow.correction: retained mandatory archive changed".into());
        }
        let claim = format!("correction-{}", round.subject.round);
        for previous in history.receipts.iter().filter(|r| {
            r.request.as_ref().is_some_and(|m| {
                m.context["operation"] == "same_session_correction" && m.context["claim"] == claim
            })
        }) {
            let stage = history
                .receipts
                .iter()
                .rev()
                .filter_map(|r| r.observation.as_ref())
                .find(|o| o["correction"] == serde_json::json!(previous.attempt))
                .and_then(|o| o["stage"].as_str());
            if stage != Some("refused") {
                return Err(
                    "workflow.correction: prior delivery observed or uncertain; never replay"
                        .into(),
                );
            }
        }
        let mut ledger = model.review_ledger.clone();
        ledger.claim_correction(&claim)?;
        let text = format!(
            "Correction for existing workflow {} / step {}. Stay in this same conversation and checkout; do not delegate, review, reset history/budget or broaden the task. Accepted contract: {}. Reviewed source: {}. Review round/archive: {} / {}. Actual checked findings: {}. Retained evidence: {}. Correct these findings only, then update your original report {} and submit fresh source-qualified evidence through the existing checkpoint path. Parent alone owns the subsequent review; do not answer pending approvals.",
            frozen.run,
            to,
            round.subject.contract.contract.sha256,
            source.sha256,
            round.subject.round,
            round.archive.display(),
            checked["findings"],
            checked["artifacts"],
            a.plan.report.display()
        );
        if text.len() > 64 * 1024 {
            return Err(
                "workflow.correction: findings payload exceeds bounds; inspect archive".into(),
            );
        }
        let context = serde_json::json!({"operation":"same_session_correction","spec":spec,"caller":caller,"review":spec.attempt,"round":round.sha256,"from":from,"to":to,"claim":claim,"ledger":ledger,"source":source,"prompt":prompt,"digest":digest(text.as_bytes())});
        if let std::collections::btree_map::Entry::Vacant(e) = self.journals.entry(frozen.run) {
            e.insert(Journal::open(path)?);
        }
        let j = self.journals.get_mut(&frozen.run).unwrap();
        let (receipt, fresh) = j.reserve(&Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            context,
        )?)?;
        if fresh {
            self.correction_effect = Some(Pending {
                run: frozen.run,
                attempt: receipt.attempt,
                revision: receipt.revision,
                path: path.to_path_buf(),
                source,
                origin: frozen.origin.clone(),
                native: reconciliation::permit(&history),
                prompt,
                text,
                verified: false,
            });
        }
        let history = retained(j, frozen.run);
        let model = reconstruct(frozen, &history)?;
        Ok(Some(WorkflowReply::Receipt {
            run: frozen.run,
            revision: history.revision,
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        }))
    }
    pub(in crate::root::workflow) fn correction_preflight(
        &self,
        e: &Pending,
    ) -> Result<(), String> {
        let h = Journal::inspect(&e.path)?;
        if h.revision != e.revision
            || super::super::source::capture(&e.source.target, e.source.base.as_deref())?
                != e.source
            || self
                .prompts
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&e.prompt.native.binding.session.to_string())
                != Some(&e.prompt)
        {
            return Err(
                "workflow.correction: stale source/generation/prompt callback; no input".into(),
            );
        }
        let m = h
            .receipts
            .iter()
            .find(|r| r.attempt == e.attempt && r.request.is_some())
            .ok_or("workflow.correction: reservation missing")?
            .request
            .as_ref()
            .unwrap();
        let frozen = read_frozen(&e.path)?;
        let model = reconstruct(&frozen, &h)?;
        if model
            .graph
            .nodes()
            .iter()
            .any(|n| n.state == NodeState::Cancelled)
        {
            return Err("workflow.correction: cancelled callback; no input".into());
        }
        let row = model
            .reviews
            .iter()
            .find(|r| r["attempt"] == m.context["review"])
            .ok_or("workflow.correction: original round unavailable")?;
        let round: RevustRound =
            serde_json::from_value(row["round"].clone()).map_err(|e| e.to_string())?;
        let job = serde_json::from_value(row["job"].clone()).map_err(|e| e.to_string())?;
        let checked = review::archive::checked(
            &frozen,
            &e.path,
            &model,
            m.context["from"].as_str().unwrap_or(""),
            &round,
            &job,
        )?;
        if checked != row["result"]["archive"] {
            return Err("workflow.correction: mandatory archive changed before input".into());
        }
        if m.context["digest"] != digest(e.text.as_bytes()) {
            return Err("workflow.correction: payload changed".into());
        }
        Ok(())
    }
    pub(in crate::root::workflow) fn correction_delivery(
        &mut self,
        e: &Pending,
        stage: &str,
    ) -> Result<(), String> {
        let j = self
            .journals
            .get_mut(&e.run)
            .ok_or("workflow.correction: claim unavailable")?;
        let value = serde_json::json!({"operation":"correction_delivery","correction":e.attempt,"stage":stage,"session":e.prompt.native.binding.session});
        if matches!(stage, "submitted" | "refused") {
            j.resolve(e.attempt, e.revision, value)?;
        } else {
            let mutation = Mutation::new(
                &format!("correction-observed-{}", e.attempt),
                j.revision(),
                serde_json::json!({"operation":"correction_observation","correction":e.attempt}),
            )?;
            let (r, fresh) = j.reserve(&mutation)?;
            if fresh {
                j.resolve(r.attempt, r.revision, value)?;
            }
        }
        Ok(())
    }
}
