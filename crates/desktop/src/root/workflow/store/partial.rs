//! Explicit recovery of a known partial native effect, never automatic replay.
use super::*;
use assignment::{Intent, Observed, Operation, Plan, Reserved};

pub(super) fn verify_checkout(
    history: &History,
    plan: &Plan,
    checkout: &Path,
) -> Result<(), String> {
    let saved = history
        .receipts
        .iter()
        .find_map(|r| {
            let observed = r.observation.as_ref()?;
            if observed["operation"] != "assignment_observed"
                || observed["assignment"]["state"] != "checkout_created"
                || observed["assignment"]["path"] != serde_json::json!(checkout)
            {
                return None;
            }
            let request = history
                .receipts
                .iter()
                .find(|q| q.attempt == r.attempt && q.request.is_some())?
                .request
                .as_ref()?;
            (request.context["effect"]["plan"] == serde_json::json!(plan))
                .then(|| observed["checkout_source"].clone())
        })
        .ok_or("workflow.partial: original checkout observation missing")?;
    let saved: SourceIdentity = serde_json::from_value(saved).map_err(
        |_| "workflow.partial: original checkout source unknown; explicit reconciliation required",
    )?;
    let actual = super::super::source::capture(checkout, Some(&plan.base))?;
    if saved != actual {
        return Err("workflow.partial: created checkout source changed; preserve work and reconcile before launch".into());
    }
    Ok(())
}
impl Runs {
    pub(super) fn retry_partial(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &RetryRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        if caller != Some(frozen.origin.session)
            || peer.is_none_or(|p| {
                p.session != frozen.origin.session
                    || p.project != frozen.origin.project
                    || p.workspace != frozen.origin.workspace
                    || p.cwd != frozen.origin.cwd
                    || p.harness != frozen.origin.harness
                    || p.conversation.is_none()
                    || p.conversation != frozen.origin.conversation
            })
        {
            return Err("workflow.partial: exact original parent binding required".into());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(frozen.run) {
            entry.insert(Journal::open(path)?);
        }
        let journal = self
            .journals
            .get_mut(&frozen.run)
            .ok_or("workflow.partial: coordinator claim unavailable")?;
        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        if let Some(old) = history.receipts.iter().find(|r| {
            r.request
                .as_ref()
                .is_some_and(|q| q.request_id == spec.request_id)
        }) {
            if old.request.as_ref().is_none_or(|q| {
                q.context["spec"] != serde_json::json!(spec)
                    || q.context["caller"] != serde_json::json!(caller)
            }) {
                return Err("workflow.partial: request context conflict".into());
            }
            return Ok(WorkflowReply::Receipt {
                run: frozen.run,
                revision: history.revision,
                attempt: old.attempt,
                receipt: Box::new(old.clone()),
                snapshot: snapshot(frozen, &history, &model),
            });
        }
        if history.revision != spec.expected_revision {
            return Err("workflow.partial: stale expected revision".into());
        }
        if model
            .graph
            .nodes()
            .iter()
            .any(|n| n.state == NodeState::Cancelled)
        {
            return Err("workflow.partial: cancelled run cannot launch another effect".into());
        }
        let reservation = history
            .receipts
            .iter()
            .find(|r| r.attempt == spec.attempt && r.request.is_some())
            .ok_or("workflow.partial: selected reservation unavailable")?;
        let request = reservation
            .request
            .as_ref()
            .ok_or("workflow.partial: request missing")?;
        if request.context["operation"] != "native_assignment" {
            return Err("workflow.partial: selected attempt is not a native assignment".into());
        }
        let intent: Intent =
            serde_json::from_value(request.context["effect"].clone()).map_err(|e| e.to_string())?;
        let Operation::Launch { checkout } = &intent.operation else {
            return Err("workflow.partial: only positively failed launch after created checkout may retry; uncertain checkout is never replayed".into());
        };
        let observed=history.receipts.iter().find(|r|r.attempt==spec.attempt && r.observation.is_some()).and_then(|r|r.observation.as_ref()).ok_or("workflow.partial: launch outcome unknown; reconcile actual owner instead of replaying")?;
        if !matches!(
            serde_json::from_value::<Observed>(observed["assignment"].clone())
                .map_err(|e| e.to_string())?,
            Observed::Refused { .. }
        ) {
            return Err("workflow.partial: existing session/runtime must be reconciled in that same conversation".into());
        }
        if history
            .receipts
            .iter()
            .rev()
            .find(|r| {
                r.request.as_ref().is_some_and(|q| {
                    q.context["operation"] == "native_assignment"
                        && q.context["effect"]["plan"]["step"] == intent.plan.step
                })
            })
            .is_none_or(|r| r.attempt != spec.attempt)
        {
            return Err(
                "workflow.partial: selected failed launch already has a later native attempt"
                    .into(),
            );
        }
        verify_checkout(&history, &intent.plan, checkout)?;
        let current =
            super::super::source::capture(&intent.plan.target, model.source.base.as_deref())?;
        if current.sha256 != intent.plan.source
            || model.accepted_contract.as_ref().map(|c| &c.contract.sha256)
                != intent.plan.contract.as_ref().map(|c| &c.contract.sha256)
        {
            return Err("workflow.partial: source or accepted contract changed; parent reconciliation required".into());
        }
        if let Some(contract) = &intent.plan.contract {
            contract.verify()?;
            contract.contract.verify()?;
        }
        if bounded_json_file(&intent.plan.task_file)? != intent.plan.prompt.as_bytes() {
            return Err("workflow.partial: original literal assignment changed".into());
        }
        let mutation = Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            serde_json::json!({"operation":"native_assignment","caller":caller,"spec":spec,"retry_of":spec.attempt,"effect":intent}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if !admitted {
            return Err("workflow.partial: existing reservation requires reconciliation".into());
        }

        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        self.retry_effect = Some(Reserved {
            native: super::reconciliation::permit(&history),
            attempt: receipt.attempt,
            revision: receipt.revision,
            plan: intent.plan,
            operation: intent.operation,
        });
        Ok(WorkflowReply::Receipt {
            run: frozen.run,
            revision: history.revision,
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
}
