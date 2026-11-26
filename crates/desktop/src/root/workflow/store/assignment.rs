//! Trusted native effects reserved by the run writer, never deserialized from CLI.
use super::*;
use guild_session::{HarnessKind, harness::LaunchSelection};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::root::workflow) struct Plan {
    pub run: RunId,
    pub step: String,
    pub origin: OriginBinding,
    pub harness: HarnessKind,
    pub selection: LaunchSelection,
    pub target: PathBuf,
    pub source: String,
    pub base: String,
    pub branch: String,
    #[serde(default)]
    pub contract: Option<ContractSnapshot>,
    pub prompt: String,
    pub task_file: PathBuf,
    pub report: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::root::workflow) enum Operation {
    Checkout,
    Launch {
        checkout: PathBuf,
    },
    Stop {
        session: SessionId,
        checkout: PathBuf,
        #[serde(default)]
        pid: Option<u32>,
        conversation: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub(super) plan: Plan,
    pub(super) operation: Operation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::root::workflow) struct Reserved {
    pub attempt: AttemptId,
    pub revision: u64,
    pub plan: Plan,
    pub operation: Operation,
    #[serde(default)]
    pub native: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::root::workflow) enum Observed {
    CheckoutCreated {
        path: PathBuf,
        branch: String,
        base: Option<String>,
    },
    #[serde(rename = "launch_observed")]
    Launched {
        checkout: PathBuf,
        session: SessionId,
        pid: Option<u32>,
        #[serde(default)]
        bound: bool,
        gap: Option<String>,
    },
    Stopped {
        session: SessionId,
    },
    Refused {
        reason: String,
    },
}
impl Runs {
    /// Reserve at most one actual native effect. Unacknowledged reservations are
    /// uncertainty, never instructions to repeat a checkout or process launch.
    pub(in crate::root::workflow) fn reserve_assignment(
        &mut self,
        run: RunId,
    ) -> Result<Option<Reserved>, String> {
        let path = self
            .paths
            .get(&run)
            .ok_or("workflow.assignment: run is not held")?
            .clone();
        let frozen = read_frozen(&path)?;
        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.assignment: coordinator claim required")?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        if history.receipts.iter().any(|r| {
            r.request.is_some()
                && !history
                    .receipts
                    .iter()
                    .any(|o| o.attempt == r.attempt && o.observation.is_some())
        }) {
            return Err("workflow.assignment: unresolved effect reservation; reconcile actual owner before retry".into());
        }
        if history.receipts.iter().any(|r| {
            r.observation
                .as_ref()
                .is_some_and(|o| o["operation"] == "cancel_requested")
        }) {
            return reserve_stop(journal, &history);
        }
        if super::reconciliation::guard(&frozen, &history, &self.observations)? {
            // Evidence can be recorded while automatic resource recovery waits.
            return Ok(None);
        }
        let model = reconstruct(&frozen, &history)?;
        let mut continuation = None;
        let mut attempted = std::collections::BTreeSet::new();
        for receipt in &history.receipts {
            let Some(request) = &receipt.request else {
                continue;
            };
            if request.context["operation"] != "native_assignment" {
                continue;
            }
            let reserved: Intent = serde_json::from_value(request.context["effect"].clone())
                .map_err(|e| e.to_string())?;
            attempted.insert(reserved.plan.step.clone());
            let observation = history
                .receipts
                .iter()
                .find(|o| o.attempt == receipt.attempt && o.observation.is_some())
                .and_then(|o| o.observation.as_ref())
                .ok_or("workflow.assignment: effect receipt missing")?;
            let observed: Observed = serde_json::from_value(observation["assignment"].clone())
                .map_err(|e| e.to_string())?;
            if let (Operation::Checkout, Observed::CheckoutCreated { path, branch, base }) =
                (&reserved.operation, observed)
            {
                if branch != reserved.plan.branch || base.as_deref() != Some(&reserved.plan.base) {
                    return Err(
                        "workflow.assignment: created checkout does not match reserved branch/base"
                            .into(),
                    );
                }
                continuation = Some((reserved.plan, Operation::Launch { checkout: path }));
            } else if matches!(reserved.operation, Operation::Launch { .. }) {
                continuation = None;
            }
        }
        let (plan, operation) = if let Some(continuation) = continuation {
            continuation
        } else {
            let Some(node) = model.graph.nodes().into_iter().find(|n| {
                n.state == NodeState::Ready
                    && !attempted.contains(&n.step)
                    && frozen.definition.steps.iter().any(|s| {
                        s.id == n.step
                            && matches!(s.action, Action::SessionAssign | Action::ContractPrepare)
                            && frozen.definition.roles[&s.role].owner == Owner::Managed
                    })
            }) else {
                return Ok(None);
            };
            let step = frozen
                .definition
                .steps
                .iter()
                .find(|s| s.id == node.step)
                .ok_or("workflow.assignment: configured node unavailable")?;
            let role = &frozen.definition.roles[&step.role];
            if role.owner != Owner::Managed {
                return Err("workflow.assignment: visible assignment requires managed role".into());
            }
            let contract = model.accepted_contract.clone();
            if step.action != Action::ContractPrepare && contract.is_none() {
                return Err(
                    "workflow.assignment: accepted contract snapshot is unavailable".into(),
                );
            }
            if let Some(contract) = &contract {
                contract.verify()?;
                contract.contract.verify()?;
            }
            let base = model.source.commit.clone().ok_or(
                "workflow.assignment: a committed Git baseline is required for a separate checkout",
            )?;
            let harness = role.harness.unwrap_or(frozen.origin.harness);
            let selection = LaunchSelection {
                model: role.model.clone(),
                effort: role.effort.clone(),
            };
            selection.validate()?;
            let branch = format!("workflow-{}-{}", &run.to_string()[..8], step.id);
            let report = path
                .join("artifacts")
                .join(format!("{}-report.md", step.id));
            let mut skills = Vec::new();
            for name in &step.skills {
                match model.skills.get(name) {
                    Some(SkillPin::Captured { skill }) => {
                        skill.verify()?;
                        skills.push(serde_json::json!({"id":name,"directory":skill.directory,"sha256":skill.sha256}));
                    }
                    Some(SkillPin::Skipped { reason }) => {
                        skills.push(serde_json::json!({"id":name,"skipped":reason}))
                    }
                    _ => {
                        return Err(format!(
                            "workflow.assignment: required skill {name} has no qualified pin"
                        ));
                    }
                }
            }
            let contract_instructions = if let Some(contract) = &contract {
                format!(
                    "Accepted contract: {}\nRead every artifact in {}. Exact artifact manifest:\n{}",
                    contract.contract.sha256,
                    contract.directory.display(),
                    serde_json::to_string_pretty(&contract.contract.artifacts)
                        .map_err(|e| e.to_string())?
                )
            } else {
                format!(
                    "Prepare the sole {:?} contract for this requested task. Selected target-relative contract resource: {:?}. No contract has been accepted yet. Do not implement the product or launch independent reviews. Submit checked contract artifacts and the durable assigned report for the initiating parent's separate acceptance; preparing them is not approval.",
                    frozen.route, frozen.start.selection
                )
            };
            let prompt = format!(
                "You are the managed {} of configured step {} in workflow {}.\n\n{}\n\n{}\n\nFrozen external skill resources:\n{}\n\nSource target: {}\nExact source identity: {}\nCheckout starts at commit {}. Source observations, including dirty/untracked layers, are retained in this run's checkpoint events; do not claim they describe the new checkout unchanged.\n\nWork only in your assigned separate visible checkout. Preserve unrelated changes. Do not delegate or launch independent reviews or model evaluations; only the initiating parent owns review and final acceptance. No push, publication, main merge, deployment, installation, global configuration edits or worktree cleanup. Do not answer another session's permission prompt.\n\nDurable report: {}. You may write this run-owned report and its sibling evidence resources for your assigned task. Record actual checks/source identities and distinguish fixtures from real model/native observations. Report questions to the parent, and retain this same session for corrections. A done turn is not acceptance.\n",
                if step.action == Action::ContractPrepare {
                    "designer"
                } else {
                    "implementer"
                },
                step.id,
                run,
                frozen.start.task,
                contract_instructions,
                serde_json::to_string_pretty(&skills).map_err(|e| e.to_string())?,
                model.source.target.display(),
                model.source.sha256,
                base,
                report.display()
            );
            let task_file = path
                .join("artifacts")
                .join(format!("{}-assignment.md", step.id));
            (
                Plan {
                    run,
                    step: step.id.clone(),
                    origin: frozen.origin.clone(),
                    harness,
                    selection,
                    target: model.source.target.clone(),
                    source: model.source.sha256.clone(),
                    base,
                    branch,
                    contract,
                    prompt,
                    task_file,
                    report,
                },
                Operation::Checkout,
            )
        };
        if let Operation::Launch { checkout } = &operation {
            super::partial::verify_checkout(&history, &plan, checkout)?;
        }
        let current = super::super::source::capture(&plan.target, model.source.base.as_deref())?;
        if current.sha256 != plan.source {
            return Err(
                "workflow.assignment: source changed before admission; parent checkpoint required"
                    .into(),
            );
        }
        if let Some(contract) = &plan.contract {
            contract.verify()?;
            contract.contract.verify()?;
        }
        // This literal file is the durable task handed to the native launch;
        // retries verify it rather than overwrite a person's changed artifact.
        std::fs::create_dir_all(path.join("artifacts")).map_err(|e| e.to_string())?;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&plan.task_file)
        {
            Ok(mut file) => {
                use std::io::Write;
                use std::os::unix::fs::PermissionsExt;
                file.write_all(plan.prompt.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|e| e.to_string())?;
                file.set_permissions(std::fs::Permissions::from_mode(0o400))
                    .map_err(|e| e.to_string())?;
                std::fs::File::open(path.join("artifacts"))
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata =
                    std::fs::symlink_metadata(&plan.task_file).map_err(|e| e.to_string())?;
                if !metadata.is_file()
                    || bounded_json_file(&plan.task_file)? != plan.prompt.as_bytes()
                {
                    return Err(
                        "workflow.assignment: literal task file changed; dispatch withheld".into(),
                    );
                }
            }
            Err(error) => return Err(format!("workflow.assignment: task file: {error}")),
        }
        let next_revision = journal
            .revision()
            .checked_add(1)
            .ok_or("workflow.revision: exhausted")?;
        let effect = Intent { plan, operation };
        let key = format!(
            "native-{}-{}",
            effect.plan.step,
            match effect.operation {
                Operation::Checkout => "checkout",
                Operation::Launch { .. } => "launch",
                Operation::Stop { .. } => unreachable!("stop uses its own admission path"),
            }
        );
        let mutation = Mutation::new(
            &key,
            journal.revision(),
            serde_json::json!({"operation":"native_assignment","effect":effect}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if !admitted {
            return Err(
                "workflow.assignment: reservation already exists; no repeat effect admitted".into(),
            );
        }
        // The journal allocates the authoritative attempt, independent of planning.
        Ok(Some(Reserved {
            native: super::reconciliation::permit(&history),
            attempt: receipt.attempt,
            revision: next_revision,
            plan: effect.plan,
            operation: effect.operation,
        }))
    }
    pub(in crate::root::workflow) fn observe_assignment(
        &mut self,
        effect: &Reserved,
        observed: Observed,
    ) -> Result<(), String> {
        let journal = self
            .journals
            .get_mut(&effect.plan.run)
            .ok_or("workflow.assignment: coordinator claim unavailable")?;
        let receipt = journal
            .receipts()
            .iter()
            .find(|r| r.attempt == effect.attempt && r.request.is_some())
            .ok_or("workflow.assignment: reservation unavailable")?;
        let reserved: Intent = serde_json::from_value(
            receipt
                .request
                .as_ref()
                .ok_or("workflow.assignment: request missing")?
                .context["effect"]
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        if effect.revision != receipt.revision
            || serde_json::to_value(&effect.plan).map_err(|e| e.to_string())?
                != serde_json::to_value(&reserved.plan).map_err(|e| e.to_string())?
            || serde_json::to_value(&effect.operation).map_err(|e| e.to_string())?
                != serde_json::to_value(&reserved.operation).map_err(|e| e.to_string())?
        {
            return Err("workflow.assignment: callback identity/revision changed".into());
        }
        let (checkout_source, checkout_gap) =
            if let Observed::CheckoutCreated { path, .. } = &observed {
                match super::super::source::capture(path, Some(&effect.plan.base)) {
                    Ok(source) => (Some(source), None),
                    Err(error) => (None, Some(error)),
                }
            } else {
                (None, None)
            };
        journal.resolve(
            effect.attempt,
            effect.revision,
            serde_json::json!({"operation":"assignment_observed","assignment":observed,"checkout_source":checkout_source,"checkout_gap":checkout_gap}),
        )?;
        Ok(())
    }
    /// Only Root's verified native prompt event calls this internal method.
    /// An initial prompt beginning is delivery knowledge, never a result verdict.
    pub(in crate::root::workflow) fn record_submission(
        &mut self,
        binding: &OriginBinding,
    ) -> Result<WorkflowReply, String> {
        let session = binding.session;
        let mut found = Vec::new();
        for (run, journal) in &self.journals {
            for receipt in journal.receipts() {
                let Some(observation) = &receipt.observation else {
                    continue;
                };
                if observation["operation"] != "assignment_observed" {
                    continue;
                }
                let observed: Observed = serde_json::from_value(observation["assignment"].clone())
                    .map_err(|e| e.to_string())?;
                if let Observed::Launched {
                    session: actual,
                    pid: Some(_),
                    bound: true,
                    checkout,
                    ..
                } = observed
                    && actual == session
                {
                    let reservation = journal
                        .receipts()
                        .iter()
                        .find(|r| r.attempt == receipt.attempt && r.request.is_some())
                        .ok_or("workflow.delivery: launch reservation unavailable")?;
                    let intent: Intent = serde_json::from_value(
                        reservation
                            .request
                            .as_ref()
                            .ok_or("workflow.delivery: reservation missing")?
                            .context["effect"]
                            .clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    if !matches!(&intent.operation,Operation::Launch {checkout:expected} if *expected==checkout)
                    {
                        return Err("workflow.delivery: actual child checkout differs".into());
                    }
                    found.push((*run, receipt.attempt, intent.plan));
                }
            }
        }
        if found.len() != 1 {
            return Err(
                "workflow.delivery: no unique bound launched assignment for native child event"
                    .into(),
            );
        }
        let (run, launch, plan) = found.pop().expect("one assignment");
        if binding.conversation.as_deref().is_none_or(str::is_empty)
            || binding.harness != plan.harness
            || binding.workspace != plan.origin.workspace
        {
            return Err("workflow.delivery: exact bound child conversation/harness/workspace is unqualified".into());
        }
        let checkout = binding.cwd.canonicalize().map_err(|e| e.to_string())?;
        let launched = self.journals[&run]
            .receipts()
            .iter()
            .find(|r| r.attempt == launch && r.observation.is_some())
            .ok_or("workflow.delivery: launch observation unavailable")?;
        let observed: Observed = serde_json::from_value(
            launched
                .observation
                .as_ref()
                .ok_or("workflow.delivery: observation missing")?["assignment"]
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        if !matches!(observed,Observed::Launched {checkout:actual,..} if actual==checkout) {
            return Err("workflow.delivery: current child checkout binding changed".into());
        }

        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.delivery: coordinator claim unavailable")?;
        let key = format!("native-{}-submitted", plan.step);
        if !journal
            .receipts()
            .iter()
            .any(|r| r.request.as_ref().is_some_and(|r| r.request_id == key))
        {
            let mutation = Mutation::new(
                &key,
                journal.revision(),
                serde_json::json!({"operation":"assignment_delivery","step":plan.step,"session":session,"launch_attempt":launch,"prompt_sha256":digest(plan.prompt.as_bytes())}),
            )?;
            let (receipt, admitted) = journal.reserve(&mutation)?;
            if admitted {
                journal.resolve(receipt.attempt,journal.revision(),serde_json::json!({"operation":"assignment_delivered","step":plan.step,"session":session,"binding":binding,"launch_attempt":launch,"prompt_sha256":digest(plan.prompt.as_bytes()),"observed_at_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis()}))?;
            }
        }
        Ok(self.execute(
            &WorkflowCommand::Status { version: 1, run },
            None,
            None,
            &[],
        ))
    }
}

/// Reflect actual reserved/native facts without turning launch into completion.
pub(super) fn reduce(model: &mut Model, receipt: &EventReceipt) -> Result<(), String> {
    if let Some(request) = &receipt.request
        && request.context["operation"] == "native_assignment"
    {
        let intent: Intent =
            serde_json::from_value(request.context["effect"].clone()).map_err(|e| e.to_string())?;
        if model
            .graph
            .nodes()
            .iter()
            .any(|n| n.step == intent.plan.step && n.state == NodeState::Ready)
        {
            model.graph.admitted(&intent.plan.step)?;
        }
        let detail = "effect reserved; native delivery unconfirmed; do not replay";
        model.graph.activity(&intent.plan.step, detail)?;
        model.assignments.as_array_mut().ok_or("workflow.assignment: invalid reconstructed rows")?.push(serde_json::json!({"attempt":receipt.attempt,"revision":receipt.revision,"plan":intent.plan,"effect":intent.operation,"observation":null,"detail":detail}));
    }
    let Some(observed) = &receipt.observation else {
        return Ok(());
    };
    if observed["operation"] == "assignment_observed" {
        let row = model
            .assignments
            .as_array_mut()
            .ok_or("workflow.assignment: invalid reconstructed rows")?
            .iter_mut()
            .find(|r| r["attempt"] == serde_json::json!(receipt.attempt))
            .ok_or("workflow.assignment: observation precedes its actual reservation")?;
        let step = row["plan"]["step"]
            .as_str()
            .ok_or("workflow.assignment: observed step missing")?
            .to_owned();
        let value: Observed =
            serde_json::from_value(observed["assignment"].clone()).map_err(|e| e.to_string())?;
        let detail = match value {
            Observed::CheckoutCreated { .. } => {
                "checkout created; session not yet launched".to_owned()
            }
            Observed::Launched { gap, bound, .. } => {
                if bound {
                    model.graph.reconciled(&step);
                }
                gap.unwrap_or("process launched; task delivery remains unconfirmed".into())
            }
            Observed::Stopped { .. } => {
                // A qualified later absence resolves the prior stop refusal,
                // without changing cancellation or any review/resource ledger.
                model.graph.reconciled(&step);
                "assigned runtime confirmed stopped/absent; checkout and evidence retained".into()
            }
            Observed::Refused { reason } => {
                model.graph.wait(&step, &reason)?;
                reason
            }
        };
        row["observation"] = observed["assignment"].clone();
        row["detail"] = serde_json::json!(detail);
        model.graph.activity(&step, &detail)?;
    }
    if observed["operation"] == "assignment_delivered" {
        let step = observed["step"]
            .as_str()
            .ok_or("workflow.delivery: configured step missing")?;
        model.graph.activity(step, "task delivery observed")?;
    }
    // No future observation or older delivery is consulted here. The caller
    // applies this receipt at its actual revision, alongside checked outcomes.
    Ok(())
}

fn reserve_stop(journal: &mut Journal, history: &History) -> Result<Option<Reserved>, String> {
    let cancellation = history
        .receipts
        .iter()
        .rev()
        .find(|r| {
            r.observation
                .as_ref()
                .is_some_and(|o| o["operation"] == "cancel_requested")
        })
        .ok_or("workflow.cancel: cancellation receipt missing")?;
    let mut attempted: std::collections::HashSet<SessionId> = history
        .receipts
        .iter()
        .filter_map(|r| r.observation.as_ref())
        .filter(|o| {
            o["operation"] == "assignment_observed" && o["assignment"]["state"] == "stopped"
        })
        .filter_map(|o| serde_json::from_value(o["assignment"]["session"].clone()).ok())
        .collect();
    for receipt in &history.receipts {
        if receipt.revision < cancellation.revision {
            continue;
        }
        let Some(request) = &receipt.request else {
            continue;
        };
        if request.context["operation"] != "native_assignment" {
            continue;
        }
        let intent: Intent =
            serde_json::from_value(request.context["effect"].clone()).map_err(|e| e.to_string())?;
        if let Operation::Stop { session, .. } = intent.operation {
            attempted.insert(session);
        }
    }
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
            pid,
            ..
        } = native
        else {
            continue;
        };
        if attempted.contains(&session) {
            continue;
        }
        let reservation = history
            .receipts
            .iter()
            .find(|r| r.attempt == receipt.attempt && r.request.is_some())
            .ok_or("workflow.cancel: native launch reservation unavailable")?;
        let mut intent: Intent = serde_json::from_value(
            reservation
                .request
                .as_ref()
                .ok_or("workflow.cancel: native request missing")?
                .context["effect"]
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        let conversation = history
            .receipts
            .iter()
            .filter_map(|r| r.observation.as_ref())
            .find(|o| {
                o["operation"] == "assignment_delivered"
                    && o["session"] == serde_json::json!(session)
            })
            .and_then(|o| o["binding"]["conversation"].as_str())
            .map(str::to_owned);
        intent.operation = Operation::Stop {
            session,
            checkout,
            pid,
            conversation,
        };
        let mutation = Mutation::new(
            &format!(
                "native-{}-stop-{session}-{}",
                intent.plan.step, cancellation.attempt
            ),
            journal.revision(),
            serde_json::json!({"operation":"native_assignment","effect":intent}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if !admitted {
            return Err(
                "workflow.cancel: stop already reserved; reconcile owner instead of repeating it"
                    .into(),
            );
        }
        return Ok(Some(Reserved {
            native: None,
            attempt: receipt.attempt,
            revision: receipt.revision,
            plan: intent.plan,
            operation: intent.operation,
        }));
    }
    Ok(None)
}
