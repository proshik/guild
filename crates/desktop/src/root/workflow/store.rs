//! Strict run initialization and inspection behind Root's one queue.
pub(super) mod assignment;
mod controls;
pub(super) mod correction;
pub(super) mod handoff;
mod partial;
pub(super) mod reconciliation;
pub(super) mod recovery;
mod reports;
pub(super) mod review;
use guild_session::{SessionId, workflow::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frozen {
    version: u32,
    run: RunId,
    start: StartRequest,
    origin: OriginBinding,
    definition: Definition,
    source: SourceIdentity,
    route: ContractKind,
    sha256: String,
}
#[derive(Default)]
pub(super) struct Runs {
    journals: BTreeMap<RunId, Journal>,
    paths: BTreeMap<RunId, PathBuf>,
    global_skills: Vec<PathBuf>,
    effect_admission: bool,
    pub(super) entry_enabled: bool,
    pub(super) trusted_navigation: bool,
    active_handoffs: std::collections::HashSet<AttemptId>,
    pub(super) prompts: handoff::Prompts,
    pub(super) observations: reconciliation::Observations,
    pub(super) recovery_records: Vec<guild_session::SessionRecord>,
    pub(super) recovery_caller_pid: Option<u32>,
    pub(super) recovery_effect: Option<recovery::Reserved>,
    pub(super) review_host: Option<RevustHost>,
    launchers: BTreeMap<RunId, RevustLauncher>,
    pub(super) review_effect: Option<review::PendingReview>,
    pub(super) correction_effect: Option<correction::Pending>,
    pub(super) retry_effect: Option<assignment::Reserved>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum SkillPin {
    Captured { skill: PinnedSkill },
    Skipped { reason: String },
    Gap { reason: String },
}
pub(super) fn global_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        roots.push(home.join(".agents/skills"));
        roots.push(home.join(".claude/skills"));
    }
    let codex = guild_session::harness::CodexHarness::home();
    if codex.is_absolute() {
        roots.push(codex.join("skills"));
    }
    roots
}
fn main_root(target: &Path) -> Result<PathBuf, String> {
    let target = target
        .canonicalize()
        .map_err(|e| format!("workflow.target: {e}"))?;
    match crate::git::worktrees::common_dir(&target) {
        Ok(common) => crate::column::main_root(&common).ok_or_else(|| {
            "workflow.repository: bare repository has no native target checkout".into()
        }),
        Err(error) => {
            if target.ancestors().any(|p| p.join(".git").exists()) {
                Err(error.to_string())
            } else {
                Ok(target)
            }
        }
    }
}
fn id(origin: SessionId, key: &str) -> Result<RunId, String> {
    let hash = digest(&serde_json::to_vec(&(origin, key)).map_err(|e| e.to_string())?);
    format!(
        "{}-{}-{}-{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[12..16],
        &hash[16..20],
        &hash[20..32]
    )
    .parse()
    .map_err(|e| format!("workflow.identity: {e}"))
}
fn read_frozen(path: &Path) -> Result<Frozen, String> {
    let bytes = bounded_json_file(&path.join("effective.json"))?;
    let value = parse_workflow_input(&bytes)?;
    let frozen: Frozen = serde_json::from_value(value).map_err(|e| e.to_string())?;
    frozen.start.validate()?;
    frozen.definition.validate()?;
    frozen.source.verify()?;
    if frozen.version != 1
        || digest(
            &serde_json::to_vec(&(
                &frozen.start,
                &frozen.origin,
                &frozen.definition,
                &frozen.source,
                frozen.route,
            ))
            .map_err(|e| e.to_string())?,
        ) != frozen.sha256
    {
        return Err("workflow.effective: frozen inputs changed".into());
    }
    Ok(frozen)
}
impl Runs {
    pub(super) fn with_global_skills(global_skills: Vec<PathBuf>) -> Self {
        Self {
            global_skills,
            ..Default::default()
        }
    }
    pub(super) fn take_effect_admission(&mut self) -> bool {
        std::mem::take(&mut self.effect_admission)
    }
    pub(super) fn execute(
        &mut self,
        request: &WorkflowCommand,
        caller: Option<SessionId>,
        origin: Option<OriginBinding>,
        roots: &[PathBuf],
    ) -> WorkflowReply {
        self.effect_admission = false;
        let affected = request.run().or_else(|| {
            if let WorkflowCommand::Start { spec } = request {
                id(
                    origin.as_ref().map_or(spec.origin, |o| o.session),
                    &spec.request_id,
                )
                .ok()
            } else {
                None
            }
        });
        // All observation and archive IO remains on this sole worker. Reading
        // status can reconcile owned observations but never repeats execution.
        if let Some(run) = request.run()
            && !matches!(
                request,
                WorkflowCommand::Show { .. } | WorkflowCommand::Cancel { .. }
            )
        {
            self.poll_review(run);
        }
        match self.apply(request, caller, origin, roots) {
            Ok(mut reply) => {
                self.decorate_reconciliation(&mut reply);
                self.decorate_handoff(&mut reply);
                reply
            }
            Err(message) => WorkflowReply::Gap {
                code: "workflow".into(),
                message,
                run: affected,
                revision: affected.and_then(|r| self.journals.get(&r).map(Journal::revision)),
            },
        }
    }
    fn apply(
        &mut self,
        request: &WorkflowCommand,
        caller: Option<SessionId>,
        origin: Option<OriginBinding>,
        roots: &[PathBuf],
    ) -> Result<WorkflowReply, String> {
        request.validate()?;
        if matches!(request, WorkflowCommand::Capabilities { .. }) {
            return Ok(WorkflowReply::Capabilities {
                version: 1,
                entry_before_design: self.entry_enabled,
                finalize_checks: true,
                operations: [
                    "start",
                    "status",
                    "show",
                    "checkpoint",
                    "review",
                    "retry",
                    "cancel",
                    "recover",
                    "reconcile",
                ]
                .into_iter()
                .filter(|operation| *operation != "start" || self.entry_enabled)
                .map(str::to_owned)
                .collect(),
            });
        }
        if let WorkflowCommand::Start { spec } = request {
            return self.start(
                spec,
                caller,
                origin.ok_or("workflow.origin: native binding required")?,
            );
        }
        let run = request.run().ok_or("workflow.run: missing identity")?;
        let path = self.locate(run, roots)?;
        let frozen = read_frozen(&path)?;
        if frozen.run != run {
            return Err("workflow.run: frozen identity differs".into());
        }
        if caller == Some(frozen.origin.session)
            && matches!(
                request,
                WorkflowCommand::Checkpoint { .. }
                    | WorkflowCommand::Cancel { .. }
                    | WorkflowCommand::Review { .. }
                    | WorkflowCommand::Retry { .. }
            )
            && let Some(binding) = origin.as_ref()
            && (binding.harness != frozen.origin.harness
                || binding.cwd != frozen.origin.cwd
                || binding.conversation.is_none()
                || binding.conversation != frozen.origin.conversation)
        {
            return Err(
                "workflow.origin: original conversation binding changed; reconcile before mutation"
                    .into(),
            );
        }
        if let Some(id) = caller.filter(|id| *id != frozen.origin.session) {
            let history = Journal::inspect(&path)?;
            if !reports::is_assigned(&history, id, origin.as_ref())? {
                return Err(
                    "workflow.authority: caller is not this run's origin or assigned child".into(),
                );
            }
            if !matches!(
                request,
                WorkflowCommand::Checkpoint { .. }
                    | WorkflowCommand::Status { .. }
                    | WorkflowCommand::Show { .. }
            ) && !(self.trusted_navigation && matches!(request, WorkflowCommand::Recover { .. }))
            {
                return Err("workflow.authority: assigned children may inspect and report their own assignment only".into());
            }
        }
        if let WorkflowCommand::Reconcile { spec, .. } = request {
            return self.reconcile_run(&path, &frozen, spec, caller, origin.as_ref());
        }
        if let WorkflowCommand::Recover { spec, .. } = request {
            return self.recover_run(&path, &frozen, spec, caller, origin.as_ref());
        }
        if matches!(
            request,
            WorkflowCommand::Review { .. } | WorkflowCommand::Retry { .. }
        ) && reconciliation::guard(&frozen, &Journal::inspect(&path)?, &self.observations)?
        {
            return Err("workflow.recovery: Parent attention required; resource reconciliation/automatic continuation remains unqualified".into());
        }
        if let WorkflowCommand::Retry { spec, .. } = request {
            if let Some(reply) =
                self.retry_correction(&path, &frozen, spec, caller, origin.as_ref())?
            {
                return Ok(reply);
            }
            return self.retry_partial(&path, &frozen, spec, caller, origin.as_ref());
        }
        if let WorkflowCommand::Review { spec, .. } = request {
            return self.review_run(&path, &frozen, spec, caller, origin.as_ref());
        }
        if let WorkflowCommand::Cancel { spec, .. } = request {
            return self.cancel_run(&path, &frozen, spec, caller);
        }
        if let WorkflowCommand::Checkpoint { spec, .. } = request {
            return self.checkpoint(&path, &frozen, spec, caller, origin.as_ref());
        }
        let history = Journal::inspect(&path)?;
        let model = reconstruct(&frozen, &history)?;
        let mut value = snapshot(&frozen, &history, &model);
        // Read-only preparation observations are never accepted outcomes. The
        // checkpoint owner recaptures and compares these facts before admission.
        if caller == Some(frozen.origin.session)
            && !model
                .graph
                .nodes()
                .iter()
                .any(|n| n.state == NodeState::Cancelled)
        {
            let pending: Vec<_> = frozen
                .definition
                .steps
                .iter()
                .filter(|step| {
                    step.action == Action::ContractPrepare
                        && frozen.definition.roles[&step.role].owner == Owner::Parent
                        && model.graph.nodes().iter().any(|n| {
                            n.step == step.id
                                && matches!(n.state, NodeState::Ready | NodeState::Active)
                        })
                })
                .collect();
            if !pending.is_empty() {
                let context = (|| -> Result<serde_json::Value, String> {
                    let source =
                        super::source::capture(&frozen.start.target, model.source.base.as_deref())?;
                    let cli = openspec_program();
                    let contract = pin_contract(
                        &frozen.start.target,
                        frozen.start.selection.as_deref(),
                        cli.as_deref(),
                    )?;
                    Ok(
                        serde_json::json!({"producer":frozen.origin.session,"source":source,"contract":contract,"steps":pending,"checks":model.checks}),
                    )
                })();
                value["preparation"] = match context {
                    Ok(context) => serde_json::json!({"context":context}),
                    Err(gap) => serde_json::json!({"gap":gap}),
                };
            }
        }
        match request {
            WorkflowCommand::Status { .. } | WorkflowCommand::Show { .. } => Ok(WorkflowReply::Snapshot { run, revision: history.revision, snapshot: value }),
            _ => Err("workflow.reconciliation: this run has not admitted the requested effect; inspect its retained graph and receipts".into()),
        }
    }
    fn checkpoint(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &CheckpointRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        if let Some(step) = frozen
            .definition
            .steps
            .iter()
            .find(|s| s.id == spec.step && s.outcomes.contains(&spec.outcome))
            && frozen.definition.roles[&step.role].owner == Owner::Managed
        {
            return self.managed_checkpoint(path, frozen, spec, caller, peer);
        }
        if caller.is_some_and(|id| id != frozen.origin.session) {
            return Err(
                "workflow.authority: child cannot accept or steer another role's outcome".into(),
            );
        }
        let run = frozen.run;
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(run) {
            entry.insert(Journal::open(path)?);
        }
        let journal = self.journals.get_mut(&run).expect("claimed run");
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let mut model = reconstruct(frozen, &history)?;
        let mutation = Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            serde_json::json!({"operation":"checkpoint","caller":caller,"spec":spec}),
        )?;
        // Resolve identical lost-reply retries before re-reading mutable source artifacts.
        if let Some(old) = journal.receipts().iter().find(|r| {
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
                attempt: old.attempt,
                receipt: Box::new(old.clone()),
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
        let step = frozen
            .definition
            .steps
            .iter()
            .find(|s| s.id == spec.step && s.outcomes.contains(&spec.outcome))
            .ok_or("workflow.checkpoint: step/outcome is not declared")?;
        if frozen.definition.roles[&step.role].owner != Owner::Parent {
            return Err("workflow.checkpoint: managed and review outcomes require their actual admitted assignment/job receipt".into());
        }
        let validator = frozen.definition.outcomes[&spec.outcome].validator;
        // Execution evidence follows the accepted assigned checkout. The sole
        // contract still has its separately pinned original provenance.
        let source_target = if step.action == Action::ContractPrepare {
            &frozen.start.target
        } else {
            &model.source.target
        };
        let source = super::source::capture(source_target, model.source.base.as_deref())?;
        if history
            .receipts
            .iter()
            .rev()
            .filter_map(|r| r.request.as_ref())
            .any(|m| {
                m.context["operation"] == "same_session_correction"
                    && m.context["mode"] == "parent_checkpoint"
                    && m.context["to"] == spec.step
                    && m.context["source"]["sha256"] == source.sha256
            })
        {
            return Err("workflow.correction: contract findings require changed source and a fresh qualified checkpoint".into());
        }
        let cli = if frozen.route == ContractKind::Openspec {
            openspec_program()
        } else {
            None
        };
        let contract = if step.action == Action::ContractPrepare {
            pin_contract(
                &frozen.start.target,
                frozen.start.selection.as_deref(),
                cli.as_deref(),
            )?
        } else {
            let accepted = model
                .accepted_contract
                .as_ref()
                .ok_or("workflow.checkpoint: accepted contract snapshot unavailable")?;
            accepted.verify()?;
            accepted.contract.clone()
        };
        if contract.kind != frozen.route {
            return Err("workflow.contract: target route changed; original run cannot silently adopt another contract".into());
        }
        let checks = checks_for_checkpoint(frozen, &model, spec, caller)?;
        let requirements: Vec<_> = checks
            .iter()
            .filter(|c| c.step == spec.step && c.outcome == spec.outcome)
            .cloned()
            .collect();
        let requirements = if requirements.is_empty() {
            let surface = match validator {
                Validator::Contract => Surface::Contract,
                Validator::LocalDelivery => Surface::Delivery,
                _ => return Err(
                    "workflow.checkpoint: accepted observable scenario requirements are missing"
                        .into(),
                ),
            };
            vec![CheckRequirement {
                step: spec.step.clone(),
                outcome: spec.outcome.clone(),
                scenario: spec.outcome.clone(),
                surface,
                expectation: if surface == Surface::Contract {
                    Expectation::Success
                } else {
                    Expectation::Observed
                },
            }]
        } else {
            requirements
        };
        if spec.evidence.len() != requirements.len() {
            return Err(
                "workflow.checkpoint: required scenario coverage is incomplete or duplicated"
                    .into(),
            );
        }
        let roots = [path.to_path_buf(), source.target.clone()];
        let mut checked = Vec::new();
        let mut scenarios = std::collections::BTreeSet::new();
        model.graph.invalidate_source(&source.sha256);
        for requirement in requirements {
            if !scenarios.insert(requirement.scenario.clone()) {
                return Err("workflow.checkpoint: accepted scenario repeated".into());
            }
            let evidence = spec
                .evidence
                .iter()
                .find(|e| e.scenario == requirement.scenario)
                .ok_or("workflow.checkpoint: required scenario has no evidence")?;
            let context = EvidenceContext {
                producer: frozen.origin.session,
                contract: &contract.sha256,
                source: &source,
                step: &spec.step,
                scenario: &requirement.scenario,
                surface: requirement.surface,
                expectation: requirement.expectation,
                allowed_roots: &roots,
            };
            let qualified =
                QualifiedOutcome::check(&spec.step, &spec.outcome, evidence.clone(), &context)?;
            model.graph.qualify(qualified.clone())?;
            checked.push(qualified);
        }
        let accepted_contract = contract.snapshot(&path.join("contracts"))?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        self.effect_admission = admitted;
        if admitted {
            let observed = serde_json::json!({
                "operation":"checkpoint",
                "accepted_contract":accepted_contract,
                "source":source,
                "contract":contract,
                "qualified":checked,
            });
            let mut observed = observed;
            if step.action == Action::ContractPrepare {
                observed["checks"] = serde_json::json!(checks);
            }
            journal.resolve(receipt.attempt, journal.revision(), observed)?;
        }
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
    fn locate(&mut self, run: RunId, roots: &[PathBuf]) -> Result<PathBuf, String> {
        if let Some(path) = self.paths.get(&run) {
            return Ok(path.clone());
        }
        let mut found = std::collections::BTreeSet::new();
        for root in roots {
            if let Ok(main) = main_root(root) {
                let path = main.join(".guild/workflow-runs").join(run.to_string());
                if path.is_dir() {
                    found.insert(path);
                }
            }
        }
        if found.len() != 1 {
            return Err(
                "workflow.run: missing or ambiguous among this window's repositories".into(),
            );
        }
        let path = found.into_iter().next().expect("one path");
        self.paths.insert(run, path.clone());
        Ok(path)
    }
    fn start(
        &mut self,
        spec: &StartRequest,
        caller: Option<SessionId>,
        origin: OriginBinding,
    ) -> Result<WorkflowReply, String> {
        if caller.is_some_and(|id| id != origin.session) {
            return Err("workflow.authority: only the actual origin initiates".into());
        }
        if origin.conversation.as_deref().is_none_or(str::is_empty) {
            return Err("workflow.origin: exact conversation is not yet qualified; wait for the native identity hook before starting".into());
        }
        let main = main_root(&spec.target)?;
        if main_root(&origin.cwd)? != main {
            return Err(
                "workflow.target: origin and target must belong to the same repository".into(),
            );
        }
        let run = id(origin.session, &spec.request_id)?;
        let directory = main.join(".guild/workflow-runs");
        let path = directory.join(run.to_string());
        // A retained intent still goes through its original identity/journal guards.
        // Refuse a fresh intent before creating even the admission directory.
        if !self.entry_enabled && !path.try_exists().map_err(|e| e.to_string())? {
            return Err("workflow.experimental: enable Development workflows in Settings > Experimental to start a new process; existing processes remain available".into());
        }
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let admission = OriginAdmission::acquire(&directory, origin.session)?;
        // Existing intent retries use their own journal even when legacy runs
        // retain another association. Only a genuinely new run needs admission.
        if !path.try_exists().map_err(|e| e.to_string())? {
            let mut associated: std::collections::BTreeSet<RunId> =
                admission.runs()?.into_iter().collect();
            // Migrate attributable legacy records without semantically validating
            // another origin's frozen definition. Broken un-attributed legacy
            // bytes have no authority over unrelated origins.
            for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    continue;
                }
                let Ok(existing_run) = entry.file_name().to_string_lossy().parse::<RunId>() else {
                    continue;
                };
                let raw = bounded_json_file(&entry.path().join("effective.json"))
                    .and_then(|bytes| parse_workflow_input(&bytes))
                    .ok();
                let attributable = raw
                    .as_ref()
                    .is_some_and(|raw| raw["origin"]["session"] == origin.session.to_string())
                    || Journal::inspect(&entry.path()).is_ok_and(|history| {
                        history.receipts.iter().any(|r| {
                            r.request.as_ref().is_some_and(|m| {
                                m.context["operation"] == "start"
                                    && m.context["origin"]["session"] == origin.session.to_string()
                            })
                        })
                    });
                if attributable {
                    admission.reserve(existing_run)?;
                    associated.insert(existing_run);
                }
            }
            for existing_run in associated {
                if existing_run == run {
                    continue;
                }
                let existing_path = directory.join(existing_run.to_string());
                if !std::fs::symlink_metadata(&existing_path).is_ok_and(|m| m.is_dir()) {
                    return Err(format!(
                        "workflow.origin: retained run {existing_run} directory is missing or uncertain"
                    ));
                }
                let existing = read_frozen(&existing_path).map_err(|e| {
                    format!("workflow.origin: retained run {existing_run} is uncertain: {e}")
                })?;
                if existing.origin.session != origin.session || existing.run != existing_run {
                    return Err(
                        "workflow.origin: retained attribution differs; reconcile before starting"
                            .into(),
                    );
                }
                let history = Journal::inspect(&existing_path)?;
                let model = reconstruct(&existing, &history)?;
                let cancelled = model
                    .graph
                    .nodes()
                    .iter()
                    .all(|n| n.state == NodeState::Cancelled);
                let unresolved = history.receipts.iter().any(|r| {
                    r.request.is_some()
                        && !history
                            .receipts
                            .iter()
                            .any(|o| o.attempt == r.attempt && o.observation.is_some())
                });
                let retained_child = history
                    .receipts
                    .iter()
                    .filter_map(|r| r.observation.as_ref())
                    .any(|o| {
                        o["operation"] == "assignment_observed"
                            && o["assignment"]["state"] == "launch_observed"
                            && !history
                                .receipts
                                .iter()
                                .filter_map(|r| r.observation.as_ref())
                                .any(|stopped| {
                                    stopped["operation"] == "assignment_observed"
                                        && stopped["assignment"]["state"] == "stopped"
                                        && stopped["assignment"]["session"]
                                            == o["assignment"]["session"]
                                })
                    });
                let retained_review = model.review_ledger.reserved() > 0
                    || model.reviews.iter().any(|r| {
                        !(matches!(
                            r["job"]["state"].as_str(),
                            Some("exited" | "failed_to_start")
                        ) || review::never_started(r))
                    });
                if unresolved
                    || !(model.graph.ready() || (cancelled && !retained_child && !retained_review))
                {
                    return Err(format!(
                        "workflow.origin: active run {} already belongs to this origin; inspect or cancel it before a distinct start",
                        existing.run
                    ));
                }
            }
        }
        let mut mutation = Mutation::new(
            &spec.request_id,
            0,
            serde_json::json!({"operation":"start","caller":caller,"spec":spec,"origin":origin}),
        )?;
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(run) {
            if path.try_exists().map_err(|e| e.to_string())? {
                entry.insert(Journal::open(&path)?);
            } else {
                let selected = spec
                    .definition
                    .clone()
                    .unwrap_or_else(|| ".guild/workflows/development.json".into());
                let file = spec.target.join(&selected);
                let project = match bounded_json_file(&file) {
                    Ok(bytes) => Some(bytes),
                    Err(_)
                        if !file.try_exists().map_err(|e| e.to_string())?
                            && spec.definition.is_none() =>
                    {
                        None
                    }
                    Err(e) => return Err(format!("workflow.definition {}: {e}", file.display())),
                };
                if project.is_some()
                    && !file.canonicalize().map_err(|e| e.to_string())?.starts_with(
                        spec.target
                            .canonicalize()
                            .map_err(|e| e.to_string())?
                            .join(".guild/workflows"),
                    )
                {
                    return Err(
                        "workflow.definition: selected resource escapes project workflows".into(),
                    );
                }
                let explicit = spec
                    .overrides
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let definition = Definition::resolve(project.as_deref(), explicit.as_deref())?;
                for check in &spec.checks {
                    if !definition
                        .steps
                        .iter()
                        .any(|s| s.id == check.step && s.outcomes.contains(&check.outcome))
                    {
                        return Err(
                            "workflow.checks: step/outcome not declared in effective definition"
                                .into(),
                        );
                    }
                }
                std::fs::create_dir_all(
                    path.parent()
                        .ok_or("workflow.storage: parent unavailable")?,
                )
                .map_err(|e| e.to_string())?;
                let source = super::source::capture(&spec.target, None)?;
                let route = match std::fs::metadata(spec.target.join("openspec")) {
                    Ok(meta) if meta.is_dir() => ContractKind::Openspec,
                    Ok(_) => {
                        return Err(
                            "workflow.contract: openspec exists but is not a directory".into()
                        );
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => ContractKind::Lightweight,
                    Err(e) => return Err(format!("workflow.contract: {e}")),
                };
                admission.reserve(run)?;
                let journal = Journal::create(&path, run)?;
                let sha256 = digest(
                    &serde_json::to_vec(&(spec, &origin, &definition, &source, route))
                        .map_err(|e| e.to_string())?,
                );
                let frozen = Frozen {
                    version: 1,
                    run,
                    start: spec.clone(),
                    origin,
                    definition,
                    source,
                    route,
                    sha256,
                };
                // Synchronous atomic Store, never a queued UI save or backup fallback.
                save_effective(&path, &frozen)?;
                entry.insert(journal);
            }
        }
        self.paths.insert(run, path.clone());
        let frozen = read_frozen(&path)?;
        mutation.context["effective_sha256"] = serde_json::json!(frozen.sha256);
        let native = self
            .observations
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&frozen.origin.session.to_string())
            .cloned();
        let native_support = native.as_ref().and_then(|n| {
            (n.binding.session == frozen.origin.session
                && n.binding.project == frozen.origin.project
                && n.binding.workspace == frozen.origin.workspace
                && n.binding.cwd == frozen.origin.cwd
                && n.binding.harness == frozen.origin.harness
                && n.binding.conversation == frozen.origin.conversation
                && n.identity_qualified
                && !n.task
                && n.holders.len() <= 1)
                .then_some(n.holders.len() == 1)
        });
        let openspec = openspec_program();
        let capabilities = serde_json::json!({
            "git-checkout":{"supported":frozen.source.commit.is_some(),"source":frozen.source.sha256},
            "native-origin":{"supported":native_support,"observation":native},
            "openspec-executable":{"supported":openspec.is_some(),"program":openspec}
        });
        let journal = self.journals.get_mut(&run).expect("claimed run");
        let (receipt, admitted) = journal.reserve(&mutation)?;
        self.effect_admission = admitted;
        if admitted {
            let project = [
                frozen.start.target.join(".agents/skills"),
                frozen.start.target.join(".claude/skills"),
            ];
            let mut skills = BTreeMap::new();
            for reference in &frozen.definition.skills {
                let pin = match snapshot_skill(
                    reference,
                    &project,
                    &self.global_skills,
                    &path.join("skills"),
                ) {
                    Ok(skill) => SkillPin::Captured { skill },
                    Err(reason)
                        if !reference.required
                            && reason.starts_with(&format!(
                                "optional skill {}: not found in qualified",
                                reference.name
                            )) =>
                    {
                        SkillPin::Skipped { reason }
                    }
                    Err(reason) => SkillPin::Gap { reason },
                };
                skills.insert(reference.id.clone(), pin);
            }
            journal.resolve(
                receipt.attempt,
                journal.revision(),
                serde_json::json!({"operation":"started","preparation_started":true,"effective_sha256":frozen.sha256,"skills":skills,"capabilities":capabilities}),
            )?;
        }
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(&frozen, &history)?;
        Ok(WorkflowReply::Receipt {
            run,
            revision: journal.revision(),
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(&frozen, &history, &model),
        })
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
struct Saved(serde_json::Value);
impl guild_session::store::Normalise for Saved {}
fn save_effective(path: &Path, frozen: &Frozen) -> Result<(), String> {
    guild_session::store::Store::new(path.join("effective.json"))
        .save(&Saved(
            serde_json::to_value(frozen).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())
}

fn bounded_json_file(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("workflow.json {}: {e}", path.display()))?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("workflow.json: exceeds 4 MiB".into());
    }
    Ok(bytes)
}

struct Model {
    graph: RunGraph,
    source: SourceIdentity,
    contract: Option<PinnedContract>,
    accepted_contract: Option<ContractSnapshot>,
    assignments: serde_json::Value,
    reports: Vec<serde_json::Value>,
    reviews: Vec<serde_json::Value>,
    review_ledger: ReviewLedger,
    checks: Vec<CheckRequirement>,
    checks_finalized: bool,
    skills: BTreeMap<String, SkillPin>,
}
fn reconstruct(frozen: &Frozen, history: &History) -> Result<Model, String> {
    let mut model = Model {
        graph: RunGraph::new(frozen.definition.clone(), frozen.start.architecture_only)?,
        source: frozen.source.clone(),
        contract: None,
        accepted_contract: None,
        assignments: serde_json::json!([]),
        reports: vec![],
        reviews: vec![],
        review_ledger: ReviewLedger::new(frozen.run, frozen.definition.limits.clone())?,
        checks: frozen.start.checks.clone(),
        checks_finalized: false,
        skills: BTreeMap::new(),
    };
    model.graph.contract_route(frozen.route);
    for receipt in &history.receipts {
        if let Some(observed) = &receipt.observation {
            if observed["operation"] == "started" && !observed["skills"].is_null() {
                model.skills = serde_json::from_value(observed["skills"].clone())
                    .map_err(|e| e.to_string())?;
                if let Some(capabilities) = observed["capabilities"].as_object() {
                    for (name, observation) in capabilities {
                        model
                            .graph
                            .capability(name, observation["supported"].as_bool());
                    }
                }
            }
            if observed["operation"] == "started" && observed["preparation_started"] == true {
                for step in &frozen.definition.steps {
                    if step.action == Action::ContractPrepare
                        && frozen.definition.roles[&step.role].owner == Owner::Parent
                        && model
                            .graph
                            .nodes()
                            .iter()
                            .any(|n| n.step == step.id && n.state == NodeState::Ready)
                    {
                        model.graph.admitted(&step.id)?;
                    }
                }
            }
            if observed["operation"] == "cancel_requested" {
                model.graph.cancel();
            }
            if observed["operation"] == "child_reported" {
                model.reports.push(observed.clone());
            }
            if (observed["operation"] == "checkpoint"
                || observed["operation"] == "review_qualified")
                && !model
                    .graph
                    .nodes()
                    .iter()
                    .any(|n| n.state == NodeState::Cancelled)
            {
                let source: SourceIdentity = serde_json::from_value(observed["source"].clone())
                    .map_err(|e| e.to_string())?;
                source.verify()?;
                model.graph.invalidate_source(&source.sha256);
                if observed["operation"] == "review_qualified" {
                    model.graph.reconciled(
                        observed["step"]
                            .as_str()
                            .ok_or("workflow.review: checked step missing")?,
                    );
                }
                if let Some(checks) = observed.get("checks") {
                    let checks: Vec<CheckRequirement> =
                        serde_json::from_value(checks.clone()).map_err(|e| e.to_string())?;
                    if model.checks_finalized && model.checks != checks {
                        return Err(
                            "workflow.checks: accepted requirements changed in retained history"
                                .into(),
                        );
                    }
                    model.checks = checks;
                }
                let checked: Vec<QualifiedOutcome> =
                    serde_json::from_value(observed["qualified"].clone())
                        .map_err(|e| e.to_string())?;
                for outcome in checked {
                    model.graph.qualify(outcome)?;
                }
                model.source = source;
                model.contract = Some(
                    serde_json::from_value(observed["contract"].clone())
                        .map_err(|e| e.to_string())?,
                );
                model.accepted_contract = observed
                    .get("accepted_contract")
                    .map(|value| serde_json::from_value(value.clone()).map_err(|e| e.to_string()))
                    .transpose()?;
                model.checks_finalized |= model.accepted_contract.is_some();
            }
        }
        let retained_imports = model.review_ledger.clone();
        correction::reduce(&mut model, history, receipt)?;
        review::reduce(&mut model, history, receipt)?;
        assignment::reduce(&mut model, receipt)?;
        if !receipt
            .request
            .as_ref()
            .is_some_and(|r| r.context["operation"] == "review_import")
        {
            model.review_ledger.retain_imports(&retained_imports)?;
        }
    }
    for step in &frozen.definition.steps {
        for reference in &step.skills {
            match model.skills.get(reference) {
                Some(SkillPin::Captured { .. }) | Some(SkillPin::Skipped { .. }) => {}
                Some(SkillPin::Gap { reason }) => {
                    model.graph.wait(&step.id, reason)?;
                }
                None => {
                    model.graph.wait(&step.id,&format!("{reference}: run has no frozen skill receipt; reconcile retained history before dispatch"))?;
                }
            }
        }
    }
    Ok(model)
}
fn checks_for_checkpoint(
    frozen: &Frozen,
    model: &Model,
    spec: &CheckpointRequest,
    caller: Option<SessionId>,
) -> Result<Vec<CheckRequirement>, String> {
    let Some(proposed) = &spec.checks else {
        return Ok(model.checks.clone());
    };
    if caller != Some(frozen.origin.session)
        || !frozen
            .definition
            .steps
            .iter()
            .any(|s| s.id == spec.step && s.action == Action::ContractPrepare)
    {
        return Err(
            "workflow.checks: only original-parent ContractPrepare can finalize requirements"
                .into(),
        );
    }
    if model.checks_finalized && proposed != &model.checks {
        return Err(
            "workflow.checks: accepted requirements are frozen; cannot clear or change them".into(),
        );
    }
    if model.checks.iter().any(|check| !proposed.contains(check)) {
        return Err(
            "workflow.checks: initial explicit requirements cannot be removed or changed".into(),
        );
    }
    let mut keys = std::collections::BTreeSet::new();
    for check in proposed {
        if !frozen
            .definition
            .steps
            .iter()
            .any(|s| s.id == check.step && s.outcomes.contains(&check.outcome))
            || !keys.insert((&check.step, &check.outcome, &check.scenario))
        {
            return Err("workflow.checks: undeclared or duplicate step/outcome/scenario".into());
        }
    }
    Ok(proposed.clone())
}

fn snapshot(frozen: &Frozen, history: &History, model: &Model) -> serde_json::Value {
    serde_json::json!({"version":1,"run":frozen.run,"origin":frozen.origin,"target":frozen.start.target,"source":model.source,"contract":model.contract,"accepted_contract":model.accepted_contract,"contract_kind":frozen.route,"checks":model.checks,"checks_finalized":model.checks_finalized,"skills":model.skills,"assignments":model.assignments,"reports":model.reports,"reviews":model.reviews,"review_ledger":model.review_ledger,"graph":model.graph,"nodes":model.graph.nodes(),"events":history.receipts,"recovery":recovery::view(history)})
}
fn openspec_program() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = guild_session::login_path::path().or_else(|| std::env::var_os("PATH"))?;
    std::env::split_paths(&path)
        .map(|p| p.join("openspec"))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Retained assignments stay under their original workflow delivery authority.
pub(super) fn continuation_membership(record: &guild_session::SessionRecord) -> Result<(), String> {
    // Persisted attribution survives coordinator and journal loss. Discovery is
    // only a fallback for legacy records, never permission to erase an owner.
    if let Some(run) = record.workflow_run {
        return Err(format!(
            "workflow_owned: task {} belongs to retained run {run}; use its existing workflow route even if its journal is unavailable",
            record.id
        ));
    }
    let main = main_root(&record.cwd)?;
    let directory = main.join(".guild/workflow-runs");
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("workflow_ownership_unqualified: {e}")),
    };
    for (count, entry) in entries.enumerate() {
        if count >= 10000 {
            return Err(
                "workflow_ownership_unqualified: retained run inventory exceeds bound".into(),
            );
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let Ok(run) = entry.file_name().to_string_lossy().parse::<RunId>() else {
            continue;
        };
        let history = Journal::inspect(&entry.path())
            .map_err(|e| format!("workflow_ownership_unqualified: {run}: {e}"))?;
        for receipt in &history.receipts {
            if let Some(observation) = &receipt.observation
                && observation["operation"] == "assignment_observed"
            {
                let native: assignment::Observed =
                    serde_json::from_value(observation["assignment"].clone())
                        .map_err(|e| format!("workflow_ownership_unqualified: {e}"))?;
                if matches!(native,assignment::Observed::Launched {session,..} if session==record.id)
                {
                    return Err(format!(
                        "workflow_owned: task {} belongs to run {run}; use its existing workflow route",
                        record.id
                    ));
                }
            }
            if let Some(request) = &receipt.request
                && request.context["operation"] == "native_assignment"
            {
                let intent: assignment::Intent =
                    serde_json::from_value(request.context["effect"].clone())
                        .map_err(|e| format!("workflow_ownership_unqualified: {e}"))?;
                if matches!(intent.operation,assignment::Operation::Launch {checkout} if checkout==record.cwd)
                {
                    return Err(format!(
                        "workflow_owned: retained launch for this worktree belongs to run {run}"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> (StartRequest, OriginBinding) {
        let session = SessionId::new();
        let spec = StartRequest {
            version: 1,
            request_id: "one".into(),
            expected_revision: 0,
            origin: session,
            target: root.to_path_buf(),
            selection: None,
            definition: None,
            overrides: None,
            task: "Explicit controlled runtime fixture; start no harness or provider".into(),
            architecture_only: false,
            checks: vec![],
        };
        let binding = OriginBinding {
            session,
            project: guild_session::ProjectId::new(),
            workspace: guild_session::WorkspaceId::new(),
            area: None,
            harness: guild_session::HarnessKind::Codex,
            conversation: Some("retained-fixture-conversation".into()),
            cwd: root.to_path_buf(),
        };
        (spec, binding)
    }
    #[test]
    fn start_retry_preserves_run_and_attempt_and_status_is_read_only_under_another_claim() {
        let dir = tempfile::tempdir().unwrap();
        let (spec, binding) = fixture(dir.path());
        let command = WorkflowCommand::Start { spec: spec.clone() };
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(&command, Some(binding.session), Some(binding.clone()), &[]);
        let WorkflowReply::Receipt {
            run,
            attempt,
            revision,
            ..
        } = first
        else {
            panic!("{first:?}")
        };
        assert_eq!(revision, 2);
        let retry = runs.execute(&command, Some(binding.session), Some(binding.clone()), &[]);
        assert!(
            matches!(retry,WorkflowReply::Receipt { run:r,attempt:a,revision:2,.. } if r==run && a==attempt)
        );
        runs.entry_enabled = false;
        assert!(
            matches!(runs.execute(&command, Some(binding.session), Some(binding.clone()), &[]), WorkflowReply::Receipt {run:r,revision:2,..} if r == run)
        );
        let mut other_window = Runs::default();
        let read = other_window.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[dir.path().to_path_buf()],
        );
        assert!(
            matches!(read, WorkflowReply::Snapshot { revision: 2, .. }),
            "{read:?}"
        );
        let blocked =
            other_window.execute(&command, Some(binding.session), Some(binding.clone()), &[]);
        assert!(
            matches!(blocked,WorkflowReply::Gap { message,.. } if message.contains("coordinator"))
        );
        let mut changed = spec;
        changed.task.push_str(" changed context");
        let conflict = runs.execute(
            &WorkflowCommand::Start { spec: changed },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        assert!(
            matches!(conflict,WorkflowReply::Gap { message,.. } if message.contains("conflict"))
        );
        drop(runs);
        // A restarted coordinator with the default-off setting reopens only the
        // original intent; it never creates another run or resets its revision.
        let restored = other_window.execute(&command, Some(binding.session), Some(binding), &[]);
        assert!(
            matches!(restored, WorkflowReply::Receipt {run:r,attempt:a,revision:2,..} if r == run && a == attempt),
            "{restored:?}"
        );
    }
    #[test]
    fn unknown_conversation_refuses_start_without_freezing_a_run() {
        let dir = tempfile::tempdir().unwrap();
        let (spec, mut binding) = fixture(dir.path());
        binding.conversation = None;
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let reply = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        assert!(
            matches!(&reply, WorkflowReply::Gap {message,..} if message.contains("conversation")),
            "{reply:?}"
        );
        assert!(!dir.path().join(".guild/workflow-runs").exists());
        binding.conversation = Some("actual-hook-conversation".into());
        assert!(matches!(
            runs.execute(
                &WorkflowCommand::Start { spec },
                Some(binding.session),
                Some(binding),
                &[]
            ),
            WorkflowReply::Receipt { .. }
        ));
    }
    #[test]
    fn broken_run_storage_is_isolated_but_its_attributed_origin_remains_reserved() {
        for mode in [
            "corrupt",
            "legacy-corrupt",
            "missing",
            "old-capability",
            "unreadable",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (spec, binding) = fixture(dir.path());
            let mut runs = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            let reply = runs.execute(
                &WorkflowCommand::Start { spec: spec.clone() },
                Some(binding.session),
                Some(binding.clone()),
                &[],
            );
            let WorkflowReply::Receipt { run, .. } = reply else {
                panic!("{reply:?}")
            };
            let path = dir
                .path()
                .join(".guild/workflow-runs")
                .join(run.to_string());
            use std::os::unix::fs::PermissionsExt;
            let file = path.join("effective.json");
            match mode {
                "corrupt" | "legacy-corrupt" => {
                    std::fs::write(&file, b"broken retained bytes").unwrap();
                }
                "missing" => {
                    std::fs::remove_file(&file).unwrap();
                }
                "old-capability" => {
                    let mut frozen = read_frozen(&path).unwrap();
                    frozen.definition.steps[0].condition = Some(Condition::Capability {
                        name: "legacy-custom-capability".into(),
                        supported: true,
                    });
                    frozen.sha256 = digest(
                        &serde_json::to_vec(&(
                            &frozen.start,
                            &frozen.origin,
                            &frozen.definition,
                            &frozen.source,
                            frozen.route,
                        ))
                        .unwrap(),
                    );
                    save_effective(&path, &frozen).unwrap();
                }
                "unreadable" => {
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o0)).unwrap();
                }
                _ => unreachable!(),
            }
            if mode == "legacy-corrupt" {
                // Pre-index retained history still attributes a damaged frozen file.
                std::fs::write(
                    path.parent()
                        .unwrap()
                        .join(format!(".origin-{}.lock", binding.session)),
                    b"",
                )
                .unwrap();
            }
            let before = if mode == "unreadable" {
                None
            } else {
                std::fs::read(&file).ok()
            };
            drop(runs);
            let mut other = binding.clone();
            other.session = SessionId::new();
            other.conversation = Some(other.session.to_string());
            let mut restored = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            let mut other_spec = spec.clone();
            other_spec.origin = other.session;
            let unrelated = restored.execute(
                &WorkflowCommand::Start { spec: other_spec },
                Some(other.session),
                Some(other),
                &[],
            );
            assert!(
                matches!(unrelated, WorkflowReply::Receipt { .. }),
                "{unrelated:?}"
            );
            let mut next = spec;
            next.request_id = "new-intent-after-corruption".into();
            let own = restored.execute(
                &WorkflowCommand::Start { spec: next },
                Some(binding.session),
                Some(binding),
                &[],
            );
            assert!(matches!(own, WorkflowReply::Gap { .. }), "{own:?}");
            if mode == "unreadable" {
                assert_eq!(
                    std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                    0
                );
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            } else {
                assert_eq!(
                    std::fs::read(&file).ok(),
                    before,
                    "broken bytes must survive {mode}"
                );
            }
        }
    }
    #[test]
    fn interrupted_creation_remains_attributed_before_any_run_files_exist() {
        let dir = tempfile::tempdir().unwrap();
        let (mut spec, origin) = fixture(dir.path());
        let directory = dir.path().join(".guild/workflow-runs");
        std::fs::create_dir_all(&directory).unwrap();
        let lost = RunId::new();
        {
            let claim = OriginAdmission::acquire(&directory, origin.session).unwrap();
            claim.reserve(lost).unwrap();
        }
        let mut other = origin.clone();
        other.session = SessionId::new();
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let mut other_spec = spec.clone();
        other_spec.origin = other.session;
        let reply = runs.execute(
            &WorkflowCommand::Start { spec: other_spec },
            Some(other.session),
            Some(other),
            &[],
        );
        assert!(matches!(reply, WorkflowReply::Receipt { .. }), "{reply:?}");
        spec.request_id = "after-interrupted-creation".into();
        let reply = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(origin.session),
            Some(origin.clone()),
            &[],
        );
        assert!(
            matches!(reply,WorkflowReply::Gap {message,..} if message.contains(&lost.to_string())),
            "attributed uncertainty must survive a new coordinator"
        );
        assert_eq!(
            OriginAdmission::acquire(&directory, origin.session)
                .unwrap()
                .runs()
                .unwrap(),
            vec![lost]
        );
        assert!(!directory.join(lost.to_string()).exists());
    }
    #[test]
    fn distinct_requests_cannot_allocate_parallel_active_runs_for_one_origin() {
        let dir = tempfile::tempdir().unwrap();
        let (mut spec, binding) = fixture(dir.path());
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, .. } = first else {
            panic!("{first:?}")
        };
        // A retained legacy/pre-start run cannot turn this run's lost-reply
        // retry into a new-start admission. No effect is fabricated for it.
        let first_path = dir
            .path()
            .join(".guild/workflow-runs")
            .join(run.to_string());
        let mut legacy = read_frozen(&first_path).unwrap();
        legacy.start.request_id = "legacy-pre-start".into();
        legacy.run = id(binding.session, &legacy.start.request_id).unwrap();
        legacy.sha256 = digest(
            &serde_json::to_vec(&(
                &legacy.start,
                &legacy.origin,
                &legacy.definition,
                &legacy.source,
                legacy.route,
            ))
            .unwrap(),
        );
        let legacy_path = first_path.parent().unwrap().join(legacy.run.to_string());
        drop(Journal::create(&legacy_path, legacy.run).unwrap());
        save_effective(&legacy_path, &legacy).unwrap();
        let retry = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        assert!(
            matches!(retry,WorkflowReply::Receipt {run:r,..} if r==run),
            "lost reply of an existing run must coalesce despite a retained legacy association: {retry:?}"
        );
        // The same admission holds after losing the in-memory coordinator.
        drop(runs);
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        spec.request_id = "second-intent".into();
        let reply = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding),
            &[],
        );
        assert!(
            matches!(&reply, WorkflowReply::Gap {message,..} if message.contains(&run.to_string()) || message.contains(&legacy.run.to_string())),
            "{reply:?}"
        );
    }
    #[test]
    fn configured_capabilities_replay_qualified_positive_negative_and_unknown_host_observations() {
        for (native_support, expected_support, ambiguous) in [
            (None, false, false),
            (Some(false), false, false),
            (Some(true), false, false),
            (None, false, true),
            (None, true, false),
            (Some(false), true, false),
            (Some(true), true, false),
            (None, true, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (mut spec, binding) = fixture(dir.path());
            spec.overrides = Some(
                serde_json::json!({"steps":[{"id":"contract","label":"Native-qualified contract","action":"contract.prepare","role":"coordinator","depends_on":[],"outcomes":["contract"],"condition":{"op":"capability","name":"native-origin","supported":expected_support}}],"corrections":[],"outcomes":{"plan-review":{"required":false},"build":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}}),
            );
            let mut runs = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            if native_support.is_some() || ambiguous {
                runs.observations.write().unwrap().insert(
                    binding.session.to_string(),
                    reconciliation::Native {
                        binding: binding.clone(),
                        task: false,
                        selection: None,
                        permission: Some(guild_session::PermissionMode::Manual),
                        status: "Idle".into(),
                        holders: if ambiguous {
                            vec![42, 43]
                        } else if native_support == Some(true) {
                            vec![42]
                        } else {
                            vec![]
                        },
                        identity_qualified: true,
                    },
                );
            }
            let reply = runs.execute(
                &WorkflowCommand::Start { spec },
                Some(binding.session),
                Some(binding.clone()),
                &[],
            );
            let WorkflowReply::Receipt { run, snapshot, .. } = reply else {
                panic!("{reply:?}")
            };
            let expected = match native_support {
                None => "waiting",
                Some(value) if value == expected_support => "active",
                Some(_) => "skipped",
            };
            assert_eq!(snapshot["nodes"][0]["state"], expected);
            assert_eq!(
                snapshot["events"][1]["observation"]["capabilities"]["git-checkout"]["supported"],
                false
            );
            drop(runs);
            let mut reopened = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            let reply = reopened.execute(
                &WorkflowCommand::Status { version: 1, run },
                Some(binding.session),
                None,
                &[dir.path().into()],
            );
            let WorkflowReply::Snapshot { snapshot, .. } = reply else {
                panic!("{reply:?}")
            };
            assert_eq!(
                snapshot["nodes"][0]["state"], expected,
                "reconstruction must use the retained host receipt, not an empty capability map"
            );
        }
    }
    #[test]
    fn recovery_reservation_is_durable_idempotent_and_refuses_stale_source() {
        let dir = tempfile::tempdir().unwrap();
        let (spec, binding) = fixture(dir.path());
        let mut store = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let started = store.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt {
            run,
            revision,
            snapshot: before,
            ..
        } = started
        else {
            panic!("{started:?}")
        };
        let mut record = guild_session::SessionRecord::new(
            "parent".into(),
            guild_session::Seat {
                workspace: binding.workspace,
                project: binding.project,
            },
            binding.cwd.clone(),
            guild_session::Runs::Codex(guild_session::SessionMode::Terminal),
        );
        record.id = binding.session;
        record.harness_session = binding.conversation.clone();
        record.launch_permission = Some(guild_session::PermissionMode::Manual);
        record.launch_selection = Some(guild_session::harness::LaunchSelection {
            model: Some("fixture-model".into()),
            effort: Some("high".into()),
        });
        store.recovery_records = vec![record];
        let spec = ControlRequest {
            version: 1,
            request_id: "recover-once".into(),
            expected_revision: revision,
        };
        let reply = store.execute(
            &WorkflowCommand::Recover {
                run,
                spec: spec.clone(),
            },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt {
            attempt,
            revision: reserved,
            ..
        } = reply
        else {
            panic!("{reply:?}")
        };
        let effect = store
            .recovery_effect
            .take()
            .expect("fresh native recovery effect");
        assert_eq!(reserved, revision + 1);
        assert_eq!(
            Journal::inspect(&effect.path)
                .unwrap()
                .receipts
                .last()
                .unwrap()
                .attempt,
            attempt
        );
        assert!(Runs::recovery_preflight(&effect).is_ok());
        std::fs::write(dir.path().join("changed.txt"), "changed while probing").unwrap();
        assert!(
            Runs::recovery_preflight(&effect)
                .unwrap_err()
                .contains("source changed")
        );
        let duplicate = store.execute(
            &WorkflowCommand::Recover {
                run,
                spec: spec.clone(),
            },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        assert!(
            matches!(duplicate,WorkflowReply::Gap {message,..} if message.contains("delivery-unknown"))
        );
        assert!(store.recovery_effect.is_none());
        store.observe_recovery(&effect,serde_json::json!({"phase":"recovery_gap","reason":"controlled stale-source fixture","ready":false,"continued":false,"parent_attention_required":true})).unwrap();
        let duplicate = store.execute(
            &WorkflowCommand::Recover {
                run,
                spec: spec.clone(),
            },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt {
            attempt: again,
            snapshot,
            ..
        } = duplicate
        else {
            panic!("{duplicate:?}")
        };
        assert_eq!(again, attempt);
        assert!(store.recovery_effect.is_none());
        assert_eq!(snapshot["review_ledger"], before["review_ledger"]);
        assert_eq!(snapshot["recovery"]["state"]["continued"], false);
        let mut conflict = spec;
        conflict.expected_revision += 1;
        assert!(
            matches!(store.execute(&WorkflowCommand::Recover {run,spec:conflict},Some(binding.session),Some(binding),&[]),WorkflowReply::Gap {message,..} if message.contains("conflict"))
        );
    }

    #[test]
    fn configured_runtime_advertises_operational_recovery() {
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let reply = runs.execute(
            &WorkflowCommand::Capabilities { version: 1 },
            None,
            None,
            &[],
        );
        assert!(
            matches!(reply,WorkflowReply::Capabilities {operations,..} if operations.iter().any(|v|v=="recover"))
        );
    }

    #[test]
    fn prior_import_refusal_is_durable_across_omission_reload_and_later_failure() {
        for complete in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let archive = tempfile::tempdir().unwrap();
            std::fs::create_dir(archive.path().join("revust")).unwrap();
            std::fs::write(
                archive.path().join("manifest.json"),
                r#"{"task":"controlled-prior","run":"plan-01","finished_at":"fixture"}"#,
            )
            .unwrap();
            let id = serde_json::json!({"stage":"find","unit":"fixture","number":0});
            let mut events = format!(
                "{}\n{}\n",
                serde_json::json!({"type":"started","attempt":id}),
                serde_json::json!({"type":"finished","attempt":id,"result":{}})
            );
            let terminal = format!(
                "{}\n",
                serde_json::json!({"type":"terminal","report":{"state":"complete","units":[{"attempts":[{"id":id}]}]}})
            );
            if complete {
                events.push_str(&terminal);
            }
            std::fs::write(archive.path().join("revust/events.jsonl"), &events).unwrap();
            let (mut start, binding) = fixture(dir.path());
            start.overrides = Some(
                serde_json::json!({"steps":[{"id":"plan-review","label":"Ready external review","action":"review.request","role":"reviewer","depends_on":[],"outcomes":["plan-review"]}],"corrections":[],"outcomes":{"contract":{"required":false},"build":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}}),
            );
            let mut runs = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            let WorkflowReply::Receipt { run, revision, .. } = runs.execute(
                &WorkflowCommand::Start { spec: start },
                Some(binding.session),
                Some(binding.clone()),
                &[],
            ) else {
                panic!("start");
            };
            let prior = ReviewImport {
                id: "retained-parent".into(),
                archive: archive.path().into(),
                source: digest(b"old source"),
                contract: digest(b"old contract"),
                correction: false,
            };
            let mut request = ReviewRequest {
                version: 1,
                request_id: "import-then-refuse".into(),
                expected_revision: revision,
                step: "plan-review".into(),
                package: dir.path().join("no-package"),
                config_dir: None,
                profile: None,
                scope: "Controlled prior admission only".into(),
                allowance: 1,
                keep_open: false,
                prior: vec![prior.clone()],
            };
            assert!(matches!(
                runs.execute(
                    &WorkflowCommand::Review {
                        run,
                        spec: request.clone()
                    },
                    Some(binding.session),
                    Some(binding.clone()),
                    &[]
                ),
                WorkflowReply::Gap { .. }
            ));
            let read = |runs: &mut Runs| match runs.execute(
                &WorkflowCommand::Status { version: 1, run },
                Some(binding.session),
                None,
                &[dir.path().into()],
            ) {
                WorkflowReply::Snapshot {
                    revision, snapshot, ..
                } => (revision, snapshot),
                other => panic!("{other:?}"),
            };
            let (after, snapshot) = read(&mut runs);
            assert_eq!(
                snapshot["review_ledger"]["rounds"]
                    .as_array()
                    .unwrap()
                    .len(),
                1,
                "refusal discarded imported accounting"
            );
            assert!(after > revision);
            let before = snapshot["review_ledger"].clone();
            drop(runs);
            let mut runs = Runs {
                entry_enabled: true,
                ..Runs::default()
            };
            let (revision, snapshot) = read(&mut runs);
            assert_eq!(snapshot["review_ledger"], before);
            request.request_id = "omit-after-reload".into();
            request.expected_revision = revision;
            request.prior.clear();
            assert!(matches!(
                runs.execute(
                    &WorkflowCommand::Review {
                        run,
                        spec: request.clone()
                    },
                    Some(binding.session),
                    Some(binding.clone()),
                    &[]
                ),
                WorkflowReply::Gap { .. }
            ));
            assert_eq!(read(&mut runs).1["review_ledger"], before);
            if !complete {
                events.push_str(&terminal);
                std::fs::write(archive.path().join("revust/events.jsonl"), &events).unwrap();
                request.request_id = "reconcile-same-import".into();
                request.expected_revision = read(&mut runs).0;
                request.prior = vec![prior.clone()];
                assert!(
                    matches!(
                        runs.execute(
                            &WorkflowCommand::Review {
                                run,
                                spec: request.clone()
                            },
                            Some(binding.session),
                            Some(binding.clone()),
                            &[]
                        ),
                        WorkflowReply::Gap { .. }
                    ),
                    "later missing contract/package remains a refusal"
                );
                let snapshot = read(&mut runs).1;
                assert_eq!(
                    snapshot["review_ledger"]["rounds"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(
                    snapshot["review_ledger"]["rounds"][0]["usage"]["closed"],
                    true
                );
            }
            for fault in [
                "id",
                "archive",
                "source",
                "contract",
                "correction",
                "events",
                "manifest",
                "unreadable",
            ] {
                let manifest_path = archive.path().join("manifest.json");
                let event_path = archive.path().join("revust/events.jsonl");
                let saved_manifest = std::fs::read(&manifest_path).unwrap();
                let mut changed = prior.clone();
                match fault {
                    "id" => changed.id = "another-id".into(),
                    "archive" => changed.archive = archive.path().join("other-root"),
                    "source" => changed.source = digest(b"changed source"),
                    "contract" => changed.contract = digest(b"changed contract"),
                    "correction" => changed.correction = true,
                    "events" => {
                        std::fs::write(&event_path, events.replace("fixture", "another")).unwrap()
                    }
                    "manifest" => std::fs::write(
                        &manifest_path,
                        r#"{"task":"substitute","run":"plan-01","finished_at":"fixture"}"#,
                    )
                    .unwrap(),
                    "unreadable" => std::fs::remove_file(&event_path).unwrap(),
                    _ => unreachable!(),
                }
                request.request_id = format!("conflicting-{fault}");
                request.expected_revision = read(&mut runs).0;
                request.prior = vec![changed];
                assert!(
                    matches!(
                        runs.execute(
                            &WorkflowCommand::Review {
                                run,
                                spec: request.clone()
                            },
                            Some(binding.session),
                            Some(binding.clone()),
                            &[]
                        ),
                        WorkflowReply::Gap { .. }
                    ),
                    "{fault}"
                );
                let held: ReviewLedger =
                    serde_json::from_value(read(&mut runs).1["review_ledger"].clone()).unwrap();
                assert_eq!(
                    (held.used(), held.remaining()),
                    (1, 0),
                    "{fault} must retain original hold"
                );
                request.request_id = format!("omit-{fault}");
                request.expected_revision = read(&mut runs).0;
                request.prior.clear();
                assert!(matches!(
                    runs.execute(
                        &WorkflowCommand::Review {
                            run,
                            spec: request.clone()
                        },
                        Some(binding.session),
                        Some(binding.clone()),
                        &[]
                    ),
                    WorkflowReply::Gap { .. }
                ));
                std::fs::write(&event_path, &events).unwrap();
                std::fs::write(&manifest_path, &saved_manifest).unwrap();
                request.request_id = format!("restore-original-{fault}");
                request.expected_revision = read(&mut runs).0;
                request.prior = vec![prior.clone()];
                assert!(matches!(
                    runs.execute(
                        &WorkflowCommand::Review {
                            run,
                            spec: request.clone()
                        },
                        Some(binding.session),
                        Some(binding.clone()),
                        &[]
                    ),
                    WorkflowReply::Gap { .. }
                ));
                let reconciled: ReviewLedger =
                    serde_json::from_value(read(&mut runs).1["review_ledger"].clone()).unwrap();
                assert_eq!(
                    (reconciled.used(), reconciled.reserved()),
                    (1, 0),
                    "only original {fault} evidence reconciles"
                );
            }
        }
    }

    #[test]
    fn review_request_requires_a_ready_configured_node_before_package_or_effect_work() {
        let dir = tempfile::tempdir().unwrap();
        let (spec, binding) = fixture(dir.path());
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let reply = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, revision, .. } = reply else {
            panic!("{reply:?}")
        };
        let spec = ReviewRequest {
            version: 1,
            request_id: "review-before-contract".into(),
            expected_revision: revision,
            step: "plan-review".into(),
            package: dir.path().join("absent-package"),
            config_dir: None,
            profile: None,
            scope: "Do not execute before contract".into(),
            allowance: 2,
            keep_open: false,
            prior: vec![],
        };
        let reply = runs.execute(
            &WorkflowCommand::Review { run, spec },
            Some(binding.session),
            Some(binding),
            &[],
        );
        assert!(
            matches!(reply,WorkflowReply::Gap {message,revision:Some(2),..} if message.contains("configured review step is not ready")),
            "ready guard must precede package lookup/effects"
        );
        assert_eq!(runs.journals[&run].revision(), 2);
    }
    #[test]
    fn project_role_is_available_to_explicit_steps_only_after_complete_composition() {
        let dir = tempfile::tempdir().unwrap();
        let (mut spec, binding) = fixture(dir.path());
        std::fs::create_dir_all(dir.path().join(".guild/workflows")).unwrap();
        std::fs::write(
            dir.path().join(".guild/workflows/development.json"),
            r#"{"roles":{"fixture-parent":{"label":"Project parent","owner":"parent"}}}"#,
        )
        .unwrap();
        let mut steps = serde_json::to_value(Definition::shipped().unwrap().steps).unwrap();
        steps[0]["role"] = serde_json::json!("fixture-parent");
        steps[0]["label"] = serde_json::json!("Explicit step");
        spec.overrides = Some(serde_json::json!({"steps":steps}));
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let reply = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { snapshot, .. } = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(
            snapshot["graph"]["definition"]["steps"][0]["role"],
            "fixture-parent"
        );
        // A separate qualified origin isolates graph validation from the one-active-run guard.
        let mut binding = binding;
        binding.session = SessionId::new();
        spec.origin = binding.session;
        spec.request_id = "invalid-role".into();
        spec.overrides.as_mut().unwrap()["steps"][0]["role"] = serde_json::json!("absent-role");
        let reply = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding),
            &[],
        );
        assert!(
            matches!(reply,WorkflowReply::Gap {message,..} if message.contains("unavailable role absent-role"))
        );
    }
    #[test]
    fn frozen_project_graph_survives_original_edits_and_foreign_caller_cannot_control_it() {
        let dir = tempfile::tempdir().unwrap();
        let (spec, binding) = fixture(dir.path());
        std::fs::create_dir_all(dir.path().join(".guild/workflows")).unwrap();
        std::fs::write(
            dir.path().join(".guild/workflows/development.json"),
            r#"{"label":"Configured real graph"}"#,
        )
        .unwrap();
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let reply = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, snapshot, .. } = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(
            snapshot["graph"]["definition"]["label"],
            "Configured real graph"
        );
        std::fs::write(
            dir.path().join(".guild/workflows/development.json"),
            "broken new defaults",
        )
        .unwrap();
        let status = runs.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Snapshot { snapshot, .. } = status else {
            panic!("{status:?}")
        };
        assert_eq!(
            snapshot["graph"]["definition"]["label"],
            "Configured real graph"
        );
        let foreign = runs.execute(
            &WorkflowCommand::Cancel {
                run,
                spec: ControlRequest {
                    version: 1,
                    request_id: "forged".into(),
                    expected_revision: 2,
                },
            },
            Some(SessionId::new()),
            None,
            &[],
        );
        assert!(matches!(foreign,WorkflowReply::Gap {message,..} if message.contains("authority")));
    }
    #[test]
    fn final_parent_contract_freezes_designed_api_checks_and_same_run_acceptance() {
        use std::io::{Read, Write};
        use std::os::unix::net::{UnixListener, UnixStream};
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("task.md"),
            "Draft: calculator; interface undecided",
        )
        .unwrap();
        let (mut spec, binding) = fixture(dir.path());
        spec.selection = Some("task.md".into());
        spec.overrides = Some(serde_json::json!({"steps":[
            {"id":"contract","label":"Design","action":"contract.prepare","role":"coordinator","depends_on":[],"outcomes":["contract"]},
            {"id":"accept","label":"Observe API","action":"acceptance.collect","role":"acceptance","depends_on":["contract"],"outcomes":["accept"]}
        ],"skills":[],"corrections":[],"outcomes":{"plan-review":{"required":false},"build":{"required":false},"code-review":{"required":false},"delivery":{"required":false}}}));
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, .. } = first else {
            panic!("{first:?}")
        };
        std::fs::write(
            dir.path().join("task.md"),
            "Final: GET /sum?left=1&right=2 returns 3; require actual API observation",
        )
        .unwrap();
        let status = runs.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Snapshot {
            snapshot, revision, ..
        } = status
        else {
            panic!("{status:?}")
        };
        let context = &snapshot["preparation"]["context"];
        let artifacts = dir
            .path()
            .join(".guild/workflow-runs")
            .join(run.to_string())
            .join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let output = artifacts.join("contract.log");
        std::fs::write(&output, "Actual final note check").unwrap();
        let evidence = serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":binding.session,"contract":context["contract"]["sha256"],"source":context["source"],"scenario":"contract","step":"contract","tool":{"program":"/bin/test","version":"fixture","argv":["-s","task.md"]},"observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"Actual final note check")}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}});
        let checks = serde_json::json!([{"step":"accept","outcome":"accept","scenario":"sum-api","surface":"api","expectation":{"kind":"http_status","value":200}}]);
        let value = serde_json::json!({"version":1,"request_id":"final-api-design","expected_revision":revision,"step":"contract","outcome":"contract","checks":checks,"evidence":[evidence]});
        let request: CheckpointRequest = serde_json::from_value(value.clone())
            .expect("final criteria must be public typed checkpoint input");
        let foreign = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(SessionId::new()),
            None,
            &[],
        );
        assert!(matches!(foreign, WorkflowReply::Gap { .. }));
        let mut stale = request.clone();
        stale.expected_revision = 0;
        assert!(matches!(
            runs.execute(
                &WorkflowCommand::Checkpoint { run, spec: stale },
                Some(binding.session),
                None,
                &[]
            ),
            WorkflowReply::Gap { .. }
        ));
        let mut wrong_source = request.clone();
        wrong_source.evidence[0].source.sha256 = "f".repeat(64);
        assert!(matches!(
            runs.execute(
                &WorkflowCommand::Checkpoint {
                    run,
                    spec: wrong_source
                },
                Some(binding.session),
                None,
                &[]
            ),
            WorkflowReply::Gap { .. }
        ));
        let accepted = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Receipt {
            snapshot, revision, ..
        } = accepted
        else {
            panic!("{accepted:?}")
        };
        assert_eq!(snapshot["checks"], checks);
        assert_eq!(snapshot["nodes"][1]["state"], "ready");
        drop(runs);
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let replay = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(binding.session),
            None,
            &[dir.path().into()],
        );
        assert!(matches!(replay,WorkflowReply::Receipt {revision:r,..} if r==revision));
        let mut change = value;
        change["request_id"] = serde_json::json!("clear-final-checks");
        change["expected_revision"] = serde_json::json!(revision);
        change["checks"] = serde_json::json!([]);
        let refused = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: serde_json::from_value(change).unwrap(),
            },
            Some(binding.session),
            None,
            &[],
        );
        assert!(matches!(refused,WorkflowReply::Gap {message,..} if message.contains("checks")));
        // Actual private API transport, not command output relabelled as API.
        let socket_owner = tempfile::Builder::new()
            .prefix("gc-api-")
            .tempdir_in("/tmp")
            .unwrap();
        let socket = socket_owner.path().join("a.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut input = [0; 256];
            let n = stream.read(&mut input).unwrap();
            assert!(String::from_utf8_lossy(&input[..n]).contains("GET /sum?left=1&right=2"));
            let sum = 1 + 2;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n{sum}").unwrap();
        });
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .write_all(b"GET /sum?left=1&right=2 HTTP/1.1\r\nHost: fixture\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        server.join().unwrap();
        assert!(response.ends_with("3"));
        let output = artifacts.join("api-response.txt");
        std::fs::write(&output, &response).unwrap();
        let proof:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":binding.session,"contract":context["contract"]["sha256"],"source":context["source"],"scenario":"sum-api","step":"accept","tool":{"program":"controlled-unix-http","version":"fixture-v1","argv":["GET","/sum?left=1&right=2"]},"observed_at_ms":2,"artifacts":[{"path":output,"sha256":digest(response.as_bytes())}],"observation":{"kind":"api","operation":"GET /sum?left=1&right=2","status":200}})).unwrap();
        let accept:CheckpointRequest=serde_json::from_value(serde_json::json!({"version":1,"request_id":"observed-sum-api","expected_revision":revision,"step":"accept","outcome":"accept","evidence":[proof]})).unwrap();
        let result = runs.execute(
            &WorkflowCommand::Checkpoint { run, spec: accept },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Receipt {
            snapshot,
            revision: final_revision,
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        assert!(
            snapshot["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|n| n["state"] == "finished")
        );
        assert_eq!(snapshot["checks"], checks);
        let mut later = request;
        later.request_id = "post-accept-clear".into();
        later.expected_revision = final_revision;
        later.checks = Some(vec![]);
        let rejected = runs.execute(
            &WorkflowCommand::Checkpoint { run, spec: later },
            Some(binding.session),
            None,
            &[],
        );
        assert!(
            matches!(rejected,WorkflowReply::Gap {message,revision:Some(r),..} if message.contains("checks") && r==final_revision)
        );
    }

    #[test]
    fn early_parent_design_repins_final_contract_without_starting_downstream_work() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("task.md"),
            "Draft: requested calculator; design pending",
        )
        .unwrap();
        let (mut spec, binding) = fixture(dir.path());
        spec.selection = Some("task.md".into());
        let initial = CheckRequirement {
            step: "contract".into(),
            outcome: "contract".into(),
            scenario: "contract".into(),
            surface: Surface::Contract,
            expectation: Expectation::Success,
        };
        spec.checks.push(initial.clone());
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, snapshot, .. } = first else {
            panic!("{first:?}")
        };
        assert_eq!(snapshot["nodes"][0]["state"], "active");
        assert!(snapshot["accepted_contract"].is_null());
        assert_eq!(snapshot["nodes"][1]["state"], "blocked");
        assert_eq!(snapshot["assignments"], serde_json::json!([]));
        assert_eq!(snapshot["reviews"], serde_json::json!([]));
        std::fs::write(
            dir.path().join("task.md"),
            "Final calculator scope and observable acceptance cases",
        )
        .unwrap();
        let status = runs.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Snapshot {
            snapshot, revision, ..
        } = status
        else {
            panic!("{status:?}")
        };
        let context = &snapshot["preparation"]["context"];
        assert_eq!(
            context["contract"]["artifacts"][0]["sha256"],
            digest(b"Final calculator scope and observable acceptance cases")
        );
        assert_ne!(context["source"]["sha256"], snapshot["source"]["sha256"]);
        let path = dir
            .path()
            .join(".guild/workflow-runs")
            .join(run.to_string());
        std::fs::create_dir_all(path.join("artifacts")).unwrap();
        let output = path.join("artifacts/note-check.txt");
        std::fs::write(&output, "actual fixture note validation").unwrap();
        let evidence: Evidence = serde_json::from_value(serde_json::json!({
            "id":uuid::Uuid::new_v4(), "producer":binding.session,
            "contract":context["contract"]["sha256"], "source":context["source"],
            "scenario":"contract",
            "step":"contract", "tool":{"program":"/bin/test","version":"fixture","argv":["-s","task.md"]},
            "observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"actual fixture note validation")}],
            "observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}
        })).unwrap();
        let mut request = CheckpointRequest {
            checks: None,
            version: 1,
            request_id: "final-design".into(),
            expected_revision: revision,
            step: "contract".into(),
            outcome: "contract".into(),
            evidence: vec![evidence],
        };
        request.checks = Some(vec![]);
        let removed = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(binding.session),
            None,
            &[],
        );
        assert!(
            matches!(removed,WorkflowReply::Gap {message,..} if message.contains("initial explicit"))
        );
        request.checks = Some(vec![initial]);
        let reply = runs.execute(
            &WorkflowCommand::Checkpoint { run, spec: request },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Receipt { snapshot, .. } = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(snapshot["nodes"][0]["state"], "finished");
        assert_eq!(snapshot["nodes"][1]["state"], "ready");
        assert_eq!(snapshot["assignments"], serde_json::json!([]));
        assert_eq!(snapshot["reviews"], serde_json::json!([]));
        let accepted: ContractSnapshot =
            serde_json::from_value(snapshot["accepted_contract"].clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(accepted.directory.join("task.md")).unwrap(),
            "Final calculator scope and observable acceptance cases"
        );
    }

    #[test]
    fn actual_contract_checkpoint_advances_only_checked_outcome_and_refuses_stale_revision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("task.md"),
            "Accepted lightweight scope and observable scenarios",
        )
        .unwrap();
        let (mut spec, binding) = fixture(dir.path());
        spec.selection = Some("task.md".into());
        spec.overrides = Some(serde_json::json!({"steps":[
            {"id":"contract","label":"Prepare","action":"contract.prepare","role":"coordinator","depends_on":[],"outcomes":["contract"]},
            {"id":"custom-builder","label":"Configured builder","action":"session.assign","role":"builder","depends_on":["contract"],"outcomes":["build"]}
        ],"roles":{"builder":{"harness":"codex","model":"fixture-model","effort":"high"}},
        "corrections":[],"outcomes":{"plan-review":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}}));
        for args in [
            vec!["init", "-q"],
            vec!["add", "task.md"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-qm",
                "Fixture baseline",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(dir.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(
            &WorkflowCommand::Start { spec: spec.clone() },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, .. } = first else {
            panic!("{first:?}")
        };
        let path = dir
            .path()
            .join(".guild/workflow-runs")
            .join(run.to_string());
        std::fs::create_dir(path.join("artifacts")).unwrap();
        let output = path.join("artifacts/contract-check.log");
        std::fs::write(
            &output,
            "Actual controlled note check: present, nonempty, selected target",
        )
        .unwrap();
        let source = super::super::source::capture(dir.path(), None).unwrap();
        let contract =
            pin_contract(dir.path(), Some(std::path::Path::new("task.md")), None).unwrap();
        let evidence:Evidence=serde_json::from_value(serde_json::json!({
            "id":uuid::Uuid::new_v4(),"producer":binding.session,"contract":contract.sha256,"source":source,
            "scenario":"contract","step":"contract","tool":{"program":"controlled-note-check","version":"1","argv":["controlled-note-check","task.md"]},
            "observed_at_ms":1,"artifacts":[{"path":output,"sha256":digest(b"Actual controlled note check: present, nonempty, selected target")}],
            "observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}
        })).unwrap();
        let mut request = CheckpointRequest {
            checks: None,
            version: 1,
            request_id: "contract-proof".into(),
            expected_revision: 2,
            step: "contract".into(),
            outcome: "contract".into(),
            evidence: vec![evidence],
        };
        for observation in [
            Observation::Claim {
                message: "done".into(),
            },
            Observation::Contract {
                exit_code: 1,
                schema: "lightweight-note".into(),
            },
        ] {
            let mut invalid = request.clone();
            invalid.evidence[0].observation = observation;
            let refusal = runs.execute(
                &WorkflowCommand::Checkpoint { run, spec: invalid },
                Some(binding.session),
                None,
                &[],
            );
            assert!(
                matches!(
                    refusal,
                    WorkflowReply::Gap {
                        revision: Some(2),
                        ..
                    }
                ),
                "{refusal:?}"
            );
        }
        let mut wrong = request.clone();
        wrong.evidence[0].producer = SessionId::new();
        let refusal = runs.execute(
            &WorkflowCommand::Checkpoint { run, spec: wrong },
            Some(binding.session),
            None,
            &[],
        );
        assert!(
            matches!(refusal,WorkflowReply::Gap {message,revision:Some(2),..} if message.contains("producer"))
        );
        let reply = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Receipt {
            snapshot, revision, ..
        } = reply
        else {
            panic!("{reply:?}")
        };
        assert_eq!(snapshot["nodes"][0]["state"], "finished");
        assert_eq!(snapshot["nodes"][1]["state"], "ready");
        let accepted: ContractSnapshot =
            serde_json::from_value(snapshot["accepted_contract"].clone()).unwrap();
        accepted.verify().unwrap();
        assert_eq!(
            std::fs::read_to_string(accepted.directory.join("task.md")).unwrap(),
            "Accepted lightweight scope and observable scenarios"
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            accepted.directory.join("task.md"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::set_permissions(&accepted.directory, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let retry = runs.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: request.clone(),
            },
            Some(binding.session),
            None,
            &[],
        );
        assert!(matches!(retry,WorkflowReply::Receipt {revision:r,..} if r==revision));
        let start_retry = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { snapshot, .. } = start_retry else {
            panic!("{start_retry:?}")
        };
        assert_eq!(
            snapshot["nodes"][0]["state"], "finished",
            "start retry must expose the same reconstructed state as status/checkpoint"
        );
        request.request_id = "different-key".into();
        let stale = runs.execute(
            &WorkflowCommand::Checkpoint { run, spec: request },
            Some(binding.session),
            None,
            &[],
        );
        assert!(matches!(stale,WorkflowReply::Gap {message,..} if message.contains("revision")));
        let effect = runs
            .reserve_assignment(run)
            .unwrap()
            .expect("configured ready assignment");
        assert_eq!(effect.plan.step, "custom-builder");
        assert_eq!(effect.plan.harness, guild_session::HarnessKind::Codex);
        assert_eq!(
            effect.plan.selection.model.as_deref(),
            Some("fixture-model")
        );
        assert!(
            effect
                .plan
                .prompt
                .contains(&effect.plan.contract.as_ref().unwrap().contract.sha256)
        );
        assert!(effect.plan.prompt.contains("Do not delegate"));
        assert_eq!(
            std::fs::read_to_string(&effect.plan.task_file).unwrap(),
            effect.plan.prompt
        );
        assert!(
            runs.reserve_assignment(run).is_err(),
            "uncertain checkout must never be replayed"
        );
        assert!(matches!(
            effect.operation,
            super::assignment::Operation::Checkout
        ));
        let mut wrong_callback = effect.clone();
        wrong_callback.revision += 1;
        assert!(
            runs.observe_assignment(
                &wrong_callback,
                super::assignment::Observed::Refused {
                    reason: "wrong revision".into()
                }
            )
            .is_err()
        );
        let checkout = dir.path().join("owned-checkout");
        std::fs::create_dir(&checkout).unwrap();
        runs.observe_assignment(
            &effect,
            super::assignment::Observed::CheckoutCreated {
                path: checkout.clone(),
                branch: effect.plan.branch.clone(),
                base: Some(effect.plan.base.clone()),
            },
        )
        .unwrap();
        let launch = runs
            .reserve_assignment(run)
            .unwrap()
            .expect("launch after created checkout");
        assert!(
            matches!(&launch.operation, super::assignment::Operation::Launch {checkout:path} if path==&checkout)
        );
        assert_ne!(launch.attempt, effect.attempt);
        runs.observe_assignment(
            &launch,
            super::assignment::Observed::Refused {
                reason: "controlled process launch failure".into(),
            },
        )
        .unwrap();
        assert!(
            runs.reserve_assignment(run).unwrap().is_none(),
            "partial effect must not repeat the checkout or launch"
        );
        let status = runs.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Snapshot { snapshot, .. } = status else {
            panic!("{status:?}")
        };
        assert_eq!(snapshot["assignments"].as_array().unwrap().len(), 2);
        assert_eq!(snapshot["nodes"][1]["state"], "waiting");
        let revision = runs.journals[&run].revision();
        let retry = WorkflowCommand::Retry {
            run,
            spec: RetryRequest {
                version: 1,
                request_id: "explicit-partial-retry".into(),
                expected_revision: revision,
                attempt: launch.attempt,
            },
        };
        std::fs::write(checkout.join("changed.txt"), "preserve source").unwrap();
        let refused = runs.execute(&retry, Some(binding.session), Some(binding.clone()), &[]);
        assert!(
            matches!(refused,WorkflowReply::Gap {message,..} if message.contains("checkout source changed"))
        );
        assert_eq!(runs.journals[&run].revision(), revision);
        std::fs::remove_file(checkout.join("changed.txt")).unwrap();
        let result = runs.execute(&retry, Some(binding.session), Some(binding.clone()), &[]);
        assert!(
            matches!(result, WorkflowReply::Receipt { .. }),
            "{result:?}"
        );
        let effect = runs.retry_effect.take().unwrap();
        assert!(
            matches!(effect.operation,super::assignment::Operation::Launch {checkout:ref path} if path==&checkout)
        );
        assert!(
            runs.reserve_assignment(run).is_err(),
            "unacknowledged retry never replays"
        );
        let duplicate = runs.execute(&retry, Some(binding.session), Some(binding), &[]);
        assert!(matches!(duplicate, WorkflowReply::Receipt { .. }));
        assert!(
            runs.retry_effect.is_none(),
            "same request must not create another native callback"
        );
    }
    #[test]
    fn run_pins_selected_project_skill_before_original_changes_and_records_required_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join(".agents/skills/engineering-quality");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "Pinned project fixture instructions",
        )
        .unwrap();
        let (spec, binding) = fixture(dir.path());
        let mut runs = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let first = runs.execute(
            &WorkflowCommand::Start { spec },
            Some(binding.session),
            Some(binding.clone()),
            &[],
        );
        let WorkflowReply::Receipt { run, snapshot, .. } = first else {
            panic!("{first:?}")
        };
        let saved = snapshot["skills"]["engineering-quality"]["skill"]["directory"]
            .as_str()
            .expect("run-owned skill snapshot");
        let saved = std::path::PathBuf::from(saved);
        assert_eq!(
            std::fs::read_to_string(saved.join("SKILL.md")).unwrap(),
            "Pinned project fixture instructions"
        );
        std::fs::write(
            skill.join("SKILL.md"),
            "Changed originals must not be consumed",
        )
        .unwrap();
        let status = runs.execute(
            &WorkflowCommand::Status { version: 1, run },
            Some(binding.session),
            None,
            &[],
        );
        let WorkflowReply::Snapshot { snapshot, .. } = status else {
            panic!("{status:?}")
        };
        assert_eq!(
            std::fs::read_to_string(saved.join("SKILL.md")).unwrap(),
            "Pinned project fixture instructions"
        );
        assert_eq!(snapshot["skills"]["acceptance-testing"]["state"], "gap");
        // Only this fixture's sealed snapshot is made writable for TempDir cleanup.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            saved.join("SKILL.md"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::set_permissions(saved, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}
