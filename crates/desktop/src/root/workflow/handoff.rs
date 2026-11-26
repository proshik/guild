//! Native delivery only for the serialized workflow's exact recovered origin.
use super::*;
use crate::Body;
impl Root {
    pub(super) fn refresh_handoff_prompts(&self, cx: &gpui::App) {
        let Some(q) = self.workflows.as_ref() else {
            return;
        };
        let native = q.observations.read().unwrap_or_else(|e| e.into_inner());
        let mut prompts = std::collections::BTreeMap::new();
        for tab in &self.tabs {
            for (pane, body) in tab.panes.bodies() {
                if let Body::Terminal { session, view, .. } = body {
                    let Some(n) = native.get(&session.to_string()) else {
                        continue;
                    };
                    let terminal = view.read(cx);
                    let covered = self
                        .overlays
                        .visible(&guild_session::overlay::Target::Session(*session))
                        .is_some()
                        || self
                            .overlays
                            .visible(&guild_session::overlay::Target::Pane(pane))
                            .is_some();
                    prompts.insert(
                        session.to_string(),
                        store::handoff::Prompt {
                            native: n.clone(),
                            generation: format!("{:?}", view.entity_id()),
                            clean: terminal.initial_prompt(),
                            draft: terminal.has_draft(),
                            turned: self.desk.session(*session).is_some_and(|r| r.had_turn),
                            transport: terminal.accepts_submission(),
                            observed: self.board.observed_status(*session).into(),
                            covered,
                            pending: terminal.input_pending()
                                || self
                                    .pending_settle
                                    .iter()
                                    .any(|(s, _, _)| s.session() == Some(*session)),
                        },
                    );
                }
            }
        }
        *q.prompts.write().unwrap_or_else(|e| e.into_inner()) = prompts;
    }
    pub(super) fn perform_workflow_handoff(
        &mut self,
        effect: store::handoff::Reserved,
        previous: Completed,
        cx: &mut Context<Self>,
    ) {
        self.refresh_reconciliation_observations(cx);
        self.refresh_handoff_prompts(cx);
        let valid = self.workflows.as_ref().is_some_and(|q| {
            q.prompts
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&effect.prompt.native.binding.session.to_string())
                == Some(&effect.prompt)
        });
        if valid && !effect.verified {
            if let Some(q) = self.workflows.as_mut()
                && q.send
                    .try_send(Work::HandoffPermitted {
                        previous: Box::new(previous),
                    })
                    .is_err()
            {
                tracing::error!("handoff preflight callback lost; reservation remains unknown");
            }
            return;
        }
        let mut stage = "refused";
        if valid {
            let (answer, receive) = mpsc::channel();
            // This internal call does not relax command provenance. The public
            // agent-send door still refuses user sessions and child controllers.
            self.submit_agent_prompt(
                effect.prompt.native.binding.session,
                &effect.text,
                crate::root::agent_submit::Origin::Command,
                answer,
                cx,
            );
            // An immediate refusal precedes all input. Otherwise the existing
            // PTY owner awaits a source-authoritative turn-start observation.
            if receive.try_recv().is_err() {
                stage = "submitted";
                if let Some(q) = self.workflows.as_mut() {
                    q.handoffs.push((effect.clone(), receive));
                }
            }
        }
        if let Some(q) = self.workflows.as_mut()
            && q.send
                .try_send(Work::HandoffSubmitted {
                    previous: Box::new(previous),
                    stage: stage.into(),
                })
                .is_err()
        {
            tracing::error!("handoff submission receipt undelivered; no replay");
        }
    }
    pub(super) fn poll_handoff_observations(&mut self, cx: &gpui::App) {
        let Some(q) = self.workflows.as_mut() else {
            return;
        };
        let mut index = 0;
        while index < q.corrections.len() {
            let result = match q.corrections[index].1.try_recv() {
                Ok(answer) => Some(matches!(answer, Answer::Sent { .. })),
                Err(mpsc::TryRecvError::Disconnected) => Some(false),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            let Some(mut observed) = result else {
                index += 1;
                continue;
            };
            let (e, _) = q.corrections.remove(index);
            observed &= q
                .prompts
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&e.prompt.native.binding.session.to_string())
                .is_some_and(|p| {
                    p.generation == e.prompt.generation
                        && p.native.binding == e.prompt.native.binding
                        && p.native.holders == e.prompt.native.holders
                        && p.native.identity_qualified
                        && p.native.task
                        && p.native.selection == e.prompt.native.selection
                        && p.native.permission == e.prompt.native.permission
                });
            q.correction_outcomes.push_back((e, observed));
        }
        let mut index = 0;
        while index < q.startups.len() {
            let result = match q.startups[index].2.try_recv() {
                Ok(answer) => Some(matches!(answer, Answer::Sent { .. })),
                Err(mpsc::TryRecvError::Disconnected) => Some(false),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            let Some(mut observed) = result else {
                index += 1;
                continue;
            };
            let (effect, pid, _) = q.startups.remove(index);
            let prompts = q.prompts.read().unwrap_or_else(|e| e.into_inner());
            observed &= prompts
                .get(&effect.binding.origin.session.to_string())
                .is_some_and(|p| {
                    p.native.binding == effect.binding.origin
                        && p.native.holders == [pid]
                        && p.native.identity_qualified
                        && p.native.selection == effect.binding.selection
                        && p.native.permission == effect.binding.permission
                });
            drop(prompts);
            q.startup_outcomes.push_back((effect, observed));
        }
        let mut index = 0;
        while index < q.handoffs.len() {
            let result = match q.handoffs[index].1.try_recv() {
                Ok(answer) => Some(matches!(answer, Answer::Sent { .. })),
                Err(mpsc::TryRecvError::Disconnected) => Some(false),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            let Some(mut observed) = result else {
                index += 1;
                continue;
            };
            let (effect, _) = q.handoffs.remove(index);
            // Do not attribute a different generation's activity to this send.
            let now = q
                .prompts
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&effect.prompt.native.binding.session.to_string())
                .cloned();
            observed &= now.as_ref().is_some_and(|n| {
                n.generation == effect.prompt.generation
                    && n.native.binding == effect.prompt.native.binding
                    && n.native.holders == effect.prompt.native.holders
                    && n.native.identity_qualified
                    && n.native.selection == effect.prompt.native.selection
                    && n.native.permission == effect.prompt.native.permission
            });
            q.handoff_outcomes.push_back((effect, observed));
            break;
        }
        q.pump();
        let _ = cx;
    }
}
