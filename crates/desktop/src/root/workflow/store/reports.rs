//! Assigned children submit checked facts; the initiating parent accepts outcomes.
use super::assignment::{Intent, Observed, Operation, Plan};
use super::*;

pub(super) struct Assignment {
    pub(super) plan: Plan,
    pub(super) session: SessionId,
    pub(super) checkout: PathBuf,
    pub(super) delivery: String,
    pub(super) binding: OriginBinding,
}
pub(super) fn assignments(history: &History) -> Result<Vec<Assignment>, String> {
    let mut result = Vec::new();
    for receipt in &history.receipts {
        let Some(observed) = &receipt.observation else {
            continue;
        };
        if observed["operation"] != "assignment_observed" {
            continue;
        }
        let native: Observed =
            serde_json::from_value(observed["assignment"].clone()).map_err(|e| e.to_string())?;
        let Observed::Launched {
            session,
            checkout,
            pid: Some(_),
            bound: true,
            ..
        } = native
        else {
            continue;
        };
        let reservation = history
            .receipts
            .iter()
            .find(|r| r.attempt == receipt.attempt && r.request.is_some())
            .ok_or("workflow.report: launch reservation missing")?;
        let intent: Intent = serde_json::from_value(
            reservation
                .request
                .as_ref()
                .ok_or("workflow.report: request missing")?
                .context["effect"]
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        if !matches!(intent.operation,Operation::Launch {checkout:ref expected} if *expected==checkout)
        {
            return Err("workflow.report: launch checkout differs from reservation".into());
        }
        let Some(delivery) = history.receipts.iter().find(|r| {
            r.observation.as_ref().is_some_and(|o| {
                o["operation"] == "assignment_delivered"
                    && o["session"] == serde_json::json!(session)
                    && o["launch_attempt"] == serde_json::json!(receipt.attempt)
            })
        }) else {
            continue;
        };
        let Some(binding) = delivery.observation.as_ref().and_then(|o| o.get("binding")) else {
            continue;
        };
        let binding: OriginBinding =
            serde_json::from_value(binding.clone()).map_err(|e| e.to_string())?;
        result.push(Assignment {
            plan: intent.plan,
            session,
            checkout,
            delivery: delivery.attempt.to_string(),
            binding,
        });
    }
    Ok(result)
}
pub(super) fn matches_peer(assignment: &Assignment, peer: &OriginBinding) -> Result<bool, String> {
    Ok(peer.session == assignment.session
        && peer.project == assignment.binding.project
        && peer.workspace == assignment.binding.workspace
        && peer.harness == assignment.binding.harness
        && peer.conversation.is_some()
        && peer.conversation == assignment.binding.conversation
        && peer.cwd.canonicalize().map_err(|e| e.to_string())? == assignment.checkout)
}
pub(super) fn is_assigned(
    history: &History,
    caller: SessionId,
    peer: Option<&OriginBinding>,
) -> Result<bool, String> {
    let Some(peer) = peer else { return Ok(false) };
    for assignment in assignments(history)? {
        if assignment.session == caller && matches_peer(&assignment, peer)? {
            return Ok(true);
        }
    }
    Ok(false)
}
impl Runs {
    pub(super) fn managed_checkpoint(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &CheckpointRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        let run = frozen.run;
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(run) {
            entry.insert(Journal::open(path)?);
        }
        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.report: coordinator claim unavailable")?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let mut model = reconstruct(frozen, &history)?;
        let child = caller.is_some_and(|id| id != frozen.origin.session);
        let mutation = Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            serde_json::json!({"operation":if child {"managed_report"} else {"checkpoint"},"caller":caller,"spec":spec}),
        )?;
        if let Some(old) = history.receipts.iter().find(|r| {
            r.request
                .as_ref()
                .is_some_and(|r| r.request_id == spec.request_id)
        }) {
            if old.request.as_ref() != Some(&mutation) {
                return Err("workflow.request: key context conflict".into());
            }
            return Ok(WorkflowReply::Receipt {
                run,
                revision: journal.revision(),
                receipt: Box::new(old.clone()),
                attempt: old.attempt,
                snapshot: snapshot(frozen, &history, &model),
            });
        }
        if journal.revision() != spec.expected_revision {
            return Err(format!(
                "workflow.revision: expected {}, current {}",
                spec.expected_revision,
                journal.revision()
            ));
        }
        let selected = assignments(&history)?
            .into_iter()
            .filter(|a| a.plan.run == run && a.plan.step == spec.step)
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err(
                "workflow.report: no unique delivered assignment for the configured step".into(),
            );
        }
        let assignment = selected.into_iter().next().expect("one assignment");
        if child
            && (caller != Some(assignment.session)
                || !matches_peer(
                    &assignment,
                    peer.ok_or("workflow.report: native caller binding required")?,
                )?)
        {
            return Err(
                "workflow.authority: child can report only its own current assignment".into(),
            );
        }
        let step = frozen
            .definition
            .steps
            .iter()
            .find(|s| s.id == spec.step)
            .ok_or("workflow.report: configured step missing")?;
        let contract = if step.action == Action::ContractPrepare {
            let cli = if frozen.route == ContractKind::Openspec {
                openspec_program()
            } else {
                None
            };
            let contract = pin_contract(
                &assignment.checkout,
                frozen.start.selection.as_deref(),
                cli.as_deref(),
            )?;
            if contract.kind != frozen.route {
                return Err(
                    "workflow.contract: assigned designer changed the sole contract route".into(),
                );
            }
            contract.snapshot(&path.join("contracts"))?
        } else {
            assignment.plan.contract.clone().ok_or(
                "workflow.report: accepted contract missing from implementation assignment",
            )?
        };
        contract.verify()?;
        contract.contract.verify()?;
        let source =
            super::super::source::capture(&assignment.checkout, Some(&assignment.plan.base))?;
        if let Some(correction) = history
            .receipts
            .iter()
            .rev()
            .filter_map(|r| r.request.as_ref())
            .find(|m| {
                m.context["operation"] == "same_session_correction" && m.context["to"] == spec.step
            })
            && correction.context["source"]["sha256"] == source.sha256
        {
            return Err(
                "workflow.correction: fresh changed-source report required before re-review".into(),
            );
        }
        if let Some(correction) = history.receipts.iter().rev().find(|r| {
            r.request.as_ref().is_some_and(|m| {
                m.context["operation"] == "same_session_correction" && m.context["to"] == spec.step
            })
        }) && history
            .receipts
            .iter()
            .rev()
            .filter_map(|r| r.observation.as_ref())
            .find(|o| o["correction"] == serde_json::json!(correction.attempt))
            .is_none_or(|o| o["stage"] != "observed")
        {
            return Err("workflow.correction: corrective delivery uncertain/refused; fresh report cannot bypass it".into());
        }
        let validator = frozen.definition.outcomes[&spec.outcome].validator;
        let checks = checks_for_checkpoint(frozen, &model, spec, caller)?;
        let declared = checks
            .iter()
            .filter(|c| c.step == spec.step && c.outcome == spec.outcome)
            .cloned()
            .collect::<Vec<_>>();
        let requirements = if declared.is_empty() && validator == Validator::ManagedSession {
            vec![CheckRequirement {
                step: spec.step.clone(),
                outcome: spec.outcome.clone(),
                scenario: spec.outcome.clone(),
                surface: Surface::Managed,
                expectation: Expectation::Observed,
            }]
        } else if declared.is_empty()
            && validator == Validator::Contract
            && step.action == Action::ContractPrepare
        {
            vec![CheckRequirement {
                step: spec.step.clone(),
                outcome: spec.outcome.clone(),
                scenario: spec.outcome.clone(),
                surface: Surface::Contract,
                expectation: Expectation::Success,
            }]
        } else if declared.is_empty() {
            return Err(
                "workflow.report: accepted observable scenario requirements are missing".into(),
            );
        } else {
            declared
        };
        if requirements.len() != spec.evidence.len() {
            return Err(
                "workflow.report: required scenario coverage incomplete or duplicated".into(),
            );
        }
        let roots = [path.to_path_buf(), source.target.clone()];
        let mut checked = Vec::new();
        let mut scenarios = std::collections::BTreeSet::new();
        let mut candidate = model.graph.clone();
        candidate.invalidate_source(&source.sha256);
        for requirement in requirements {
            if !scenarios.insert(requirement.scenario.clone()) {
                return Err("workflow.report: required scenario repeated".into());
            }
            let evidence = spec
                .evidence
                .iter()
                .find(|e| e.scenario == requirement.scenario)
                .ok_or("workflow.report: required scenario has no evidence")?;
            if let Observation::Managed {
                session,
                checkout,
                delivery,
                report,
            } = &evidence.observation
                && (*session != assignment.session
                    || *checkout != assignment.checkout
                    || *delivery != assignment.delivery
                    || report.canonicalize().map_err(|e| e.to_string())?
                        != assignment
                            .plan
                            .report
                            .canonicalize()
                            .map_err(|e| e.to_string())?)
            {
                return Err("workflow.report: source report does not match actual assigned session/delivery/report grant".into());
            }
            if validator == Validator::Contract
                && !evidence.artifacts.iter().any(|a| {
                    a.path.canonicalize().ok() == assignment.plan.report.canonicalize().ok()
                        && assignment.plan.report.is_file()
                })
            {
                return Err("workflow.report: designer contract evidence must include its actual assigned durable report".into());
            }
            if !child {
                let retained = history
                    .receipts
                    .iter()
                    .filter_map(|r| r.observation.as_ref())
                    .filter(|o| {
                        o["operation"] == "child_reported"
                            && o["step"] == spec.step
                            && o["outcome"] == spec.outcome
                    })
                    .map(|o| {
                        serde_json::from_value::<Vec<QualifiedOutcome>>(o["qualified"].clone())
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !retained.iter().flatten().any(|q| q.evidence() == evidence) {
                    return Err("workflow.report: parent acceptance requires the assigned child's retained checked report".into());
                }
            }
            let context = EvidenceContext {
                producer: assignment.session,
                contract: &contract.contract.sha256,
                source: &source,
                step: &spec.step,
                scenario: &requirement.scenario,
                surface: requirement.surface,
                expectation: requirement.expectation,
                allowed_roots: &roots,
            };
            let qualified =
                QualifiedOutcome::check(&spec.step, &spec.outcome, evidence.clone(), &context)?;
            candidate.qualify(qualified.clone())?;
            checked.push(qualified);
        }
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if admitted {
            let observed = if child {
                serde_json::json!({"operation":"child_reported","step":spec.step,"outcome":spec.outcome,"producer":assignment.session,"source":source,"qualified":checked})
            } else {
                serde_json::json!({"operation":"checkpoint","source":source,"contract":contract.contract,"accepted_contract":contract,"qualified":checked,"checks":checks})
            };
            journal.resolve(receipt.attempt, journal.revision(), observed)?;
        }
        self.effect_admission = admitted && !child;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        model = reconstruct(frozen, &history)?;
        Ok(WorkflowReply::Receipt {
            run,
            revision: journal.revision(),
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
}
