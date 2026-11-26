//! Explicit parent controls preserve original histories and resources.
use super::*;
impl Runs {
    pub(super) fn cancel_run(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &ControlRequest,
        caller: Option<SessionId>,
    ) -> Result<WorkflowReply, String> {
        let run = frozen.run;
        if caller.is_some_and(|id| id != frozen.origin.session) {
            return Err("workflow.authority: child cannot cancel its parent run".into());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(run) {
            entry.insert(Journal::open(path)?);
        }
        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.cancel: coordinator claim unavailable")?;
        let mutation = Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            serde_json::json!({"operation":"cancel","caller":caller,"spec":spec}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if admitted {
            journal.resolve(receipt.attempt,journal.revision(),serde_json::json!({"operation":"cancel_requested","reason":"explicit parent cancellation; stop only recorded native owners; retain checkouts and history"}))?;
        }
        self.effect_admission = admitted;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        Ok(WorkflowReply::Receipt {
            run,
            revision: journal.revision(),
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
}
