//! Narrow native correction delivery; public task/session send authority is unchanged.
use super::*;
impl Root {
    pub(super) fn perform_workflow_correction(
        &mut self,
        e: store::correction::Pending,
        previous: Completed,
        cx: &mut Context<Self>,
    ) {
        self.refresh_reconciliation_observations(cx);
        self.refresh_handoff_prompts(cx);
        let valid = self.workflow_parent_binding(&e.origin, cx).is_ok()
            && self
                .reconciliation_native_permit(e.run, e.native.as_ref(), cx)
                .is_ok()
            && self.workflows.as_ref().is_some_and(|q| {
                q.prompts
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&e.prompt.native.binding.session.to_string())
                    == Some(&e.prompt)
            });
        if valid && !e.verified {
            if let Some(q) = self.workflows.as_mut()
                && q.send
                    .try_send(Work::CorrectionPermitted {
                        previous: Box::new(previous),
                    })
                    .is_err()
            {
                tracing::error!("correction callback lost; retained reservation remains unknown");
            }
            return;
        }
        let mut stage = "refused";
        if valid && e.verified {
            let (answer, receive) = mpsc::channel();
            self.submit_agent_prompt(
                e.prompt.native.binding.session,
                &e.text,
                crate::root::agent_submit::Origin::Command,
                answer,
                cx,
            );
            if receive.try_recv().is_err() {
                stage = "submitted";
                if let Some(q) = self.workflows.as_mut() {
                    q.corrections.push((e, receive));
                }
            }
        }
        if let Some(q) = self.workflows.as_mut()
            && q.send
                .try_send(Work::CorrectionSubmitted {
                    previous: Box::new(previous),
                    stage: stage.into(),
                })
                .is_err()
        {
            tracing::error!("correction submission receipt lost; no replay");
        }
    }
}
