//! Parent-owned installed revust admission; never the native review engine.
use super::*;
use std::io::Read;
pub(super) mod archive;

#[derive(Clone)]
pub(in crate::root::workflow) struct ReviewJob {
    pub run: RunId,
    pub attempt: AttemptId,
    pub round_sha256: String,
    pub receipt: guild_session::terminal_job::JobReceipt,
}

#[derive(Clone)]
pub(in crate::root::workflow) struct PendingReview {
    pub run: RunId,
    pub step: String,
    pub attempt: AttemptId,
    pub revision: u64,
    pub origin: OriginBinding,
    pub round: RevustRound,
    pub ledger: ReviewLedger,
    pub host: RevustHost,
    pub native: Option<serde_json::Value>,
}

/// Only a trusted pre-launch receipt can establish a missing job as absence.
/// An observer exit, missing job or saved PID alone never does.
pub(super) fn never_started(row: &serde_json::Value) -> bool {
    row["observation"]["operation"] == "review_launched"
        && row["observation"]["not_started"] == true
        && row["observation"]["launcher_pid"].is_null()
        && row.get("job").is_none()
        && (row["round"]["environment"].is_null()
            || row["observation"]["environment_released"] == true)
}

pub(super) fn reduce(
    model: &mut Model,
    history: &History,
    receipt: &EventReceipt,
) -> Result<(), String> {
    if let Some(request) = &receipt.request
        && request.context["operation"] == "review_import"
    {
        model.review_ledger =
            serde_json::from_value(request.context["ledger"].clone()).map_err(|e| e.to_string())?;
        model.review_ledger.validate()?;
    }
    if let Some(request) = &receipt.request
        && request.context["operation"] == "external_review"
    {
        model.review_ledger =
            serde_json::from_value(request.context["ledger"].clone()).map_err(|e| e.to_string())?;
        model.review_ledger.validate()?;
        let round: RevustRound =
            serde_json::from_value(request.context["round"].clone()).map_err(|e| e.to_string())?;
        let step = request.context["step"]
            .as_str()
            .ok_or("workflow.review: step missing")?;
        model.graph.reconciled(step);
        if model
            .graph
            .nodes()
            .iter()
            .any(|n| n.step == step && n.state == NodeState::Ready)
        {
            model.graph.admitted(step)?;
        }
        model.graph.activity(
            step,
            "Review allowance reserved; actual launcher/job observation pending",
        )?;
        model
            .reviews
            .push(serde_json::json!({"step":step,"attempt":receipt.attempt,"round":round}));
    }
    let Some(observed) = &receipt.observation else {
        return Ok(());
    };
    if let Some(ledger) = observed.get("review_ledger") {
        model.review_ledger = serde_json::from_value(ledger.clone()).map_err(|e| e.to_string())?;
        model.review_ledger.validate()?;
    }
    if observed["operation"] == "review_environment_released" {
        if let Some(row) = model
            .reviews
            .iter_mut()
            .find(|r| r["round"]["sha256"] == observed["round_sha256"])
        {
            row["result"]["environment_released"] = true.into();
        }
        return Ok(());
    }
    let Some(operation) = observed["operation"].as_str().filter(|op| {
        matches!(
            *op,
            "review_qualified"
                | "review_result"
                | "review_job_observed"
                | "review_observed"
                | "review_launched"
        )
    }) else {
        return Ok(());
    };
    let step = observed["step"]
        .as_str()
        .ok_or("workflow.review: observation step missing")?;
    let context = &history
        .receipts
        .iter()
        .find(|r| r.attempt == receipt.attempt && r.request.is_some())
        .and_then(|r| r.request.as_ref())
        .ok_or("workflow.review: observation reservation missing")?
        .context;
    let round = context["round_sha256"]
        .as_str()
        .or_else(|| context["round"]["sha256"].as_str())
        .ok_or("workflow.review: observation round identity missing")?;
    let latest = model
        .reviews
        .iter()
        .rev()
        .find(|r| r["step"] == step)
        .is_some_and(|r| r["round"]["sha256"] == round);
    let row = model
        .reviews
        .iter_mut()
        .find(|r| r["round"]["sha256"] == round && r["step"] == step)
        .ok_or("workflow.review: observation precedes its exact round reservation")?;
    match operation {
        "review_qualified" | "review_result" => {
            row["result"] = observed.clone();
            if model
                .graph
                .nodes()
                .iter()
                .any(|n| n.state == NodeState::Cancelled)
            {
                return Ok(());
            }
            if operation == "review_qualified" {
                model.graph.reconciled(step);
                model.graph.activity(
                    step,
                    "Checked installed review archive and actual calls qualify this source",
                )?;
            } else if let Some(gap) = observed["gap"].as_str() {
                model.graph.wait(step, gap)?;
            }
        }
        "review_job_observed" => {
            row["job"] = observed["job"].clone();
            if row.get("result").is_none() {
                model.graph.activity(
                    step,
                    "Actual linked terminal job observed; checked archive outcome pending",
                )?;
            }
        }
        _ => {
            row["observation"] = observed.clone();
            if observed["not_started"] == true {
                row["result"] = observed.clone();
                model.graph.completed(step)?;
                model.graph.wait(
                    step,
                    observed["gap"]
                        .as_str()
                        .unwrap_or("Review refused before launcher start"),
                )?;
            }
            if row.get("result").is_none() {
                if let Some(gap) = observed["gap"].as_str() {
                    model.graph.wait(step, gap)?;
                } else {
                    model.graph.activity(step,"Installed launcher started; actual job/archive outcome awaits reconciliation")?;
                }
            }
        }
    }
    // Archive completion can precede presentation exit. Both actual owner
    // completion and closed usage are required before freeing execution state.
    if latest
        && row.get("result").is_some()
        && model.review_ledger.reserved() == 0
        && matches!(
            row["job"]["state"].as_str(),
            Some("exited" | "failed_to_start")
        )
    {
        model.graph.completed(step)?;
    }
    Ok(())
}
fn bytes(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(max + 1).read_to_end(&mut result))
        .map_err(|e| format!("workflow.review evidence {}: {e}", path.display()))?;
    if result.len() as u64 > max {
        return Err(
            "workflow.review: archive observation exceeds bounds; reservation retained".into(),
        );
    }
    Ok(result)
}
/// Read only the exact retained archive files for recovery freshness. No profile
/// reload, provider process or result acceptance is implied by their digest.
pub(super) fn recovery_usage(root: &Path) -> Result<ReviewUsage, String> {
    ReviewUsage::from_events(&archive::read(
        root,
        "revust/events.jsonl",
        64 * 1024 * 1024,
    )?)
}
pub(super) fn recovery_archive(root: &Path, relative: &str) -> Result<String, String> {
    archive::read(root, relative, 64 * 1024 * 1024).map(|bytes| digest(&bytes))
}
// Accounting receipts precede package/preparation/effect admission. A reserved
// import already retains its hold if the process dies before its resolution.
fn import_records(journal: &Journal) -> BTreeMap<String, serde_json::Value> {
    let mut records = BTreeMap::new();
    for receipt in journal.receipts() {
        let Some(m) = &receipt.request else { continue };
        if m.context["operation"] == "review_import" {
            if let Some(id) = m.context["prior"]["id"].as_str() {
                records.insert(id.into(), m.context.clone());
            }
        } else if m.context["operation"] == "external_review" {
            for old in m.context["imported"].as_array().into_iter().flatten() {
                if let Some(id) = old["receipt"]["id"].as_str() {
                    records.entry(id.into()).or_insert_with(|| serde_json::json!({"prior":old["receipt"],"manifest_sha256":old["manifest_sha256"],"legacy_events_sha256":old["events_sha256"]}));
                }
            }
        }
    }
    records
}
fn persist_import(
    journal: &mut Journal,
    ledger: &ReviewLedger,
    mut context: serde_json::Value,
) -> Result<(), String> {
    context["operation"] = "review_import".into();
    context["ledger"] = serde_json::to_value(ledger).map_err(|e| e.to_string())?;
    let previous = journal.receipts().iter().rev().find(|r| {
        r.request.as_ref().is_some_and(|m| {
            m.context["operation"] == "review_import"
                && m.context["prior"]["id"] == context["prior"]["id"]
        })
    });
    if previous.is_some_and(|r| r.request.as_ref().is_some_and(|m| m.context == context)) {
        return Ok(());
    }
    let key = format!(
        "import-{}",
        digest(
            &serde_json::to_vec(&(previous.map(|r| r.attempt), &context))
                .map_err(|e| e.to_string())?
        )
    );
    let mutation = Mutation::new(&key, journal.revision(), context)?;
    let (receipt, _) = journal.reserve(&mutation)?;
    journal.resolve(
        receipt.attempt,
        journal.revision(),
        serde_json::json!({"operation":"review_import_recorded","providers_started":false}),
    )?;
    Ok(())
}
fn import_priors(
    journal: &mut Journal,
    ledger: &mut ReviewLedger,
    priors: &[ReviewImport],
) -> Result<Vec<serde_json::Value>, String> {
    use std::os::unix::fs::MetadataExt;
    // Finishing a bookkeeping receipt claims no provider completion. Its request
    // already contains the retained ledger; never overwrite it with an old copy.
    for old in journal.unresolved() {
        if old
            .request
            .as_ref()
            .is_some_and(|m| m.context["operation"] == "review_import")
        {
            journal.resolve(old.attempt,journal.revision(),serde_json::json!({"operation":"review_import_recovered","providers_started":false}))?;
        }
    }
    let mut error = None;
    let mut eligible = Vec::new();
    // Retain every supplied identity before attempting any archive read.
    for prior in priors {
        let records = import_records(journal);
        let canonical = prior.archive.canonicalize().ok();
        let old = records.get(&prior.id).or_else(|| {
            records.values().find(|v| {
                v["prior"]["archive"] == serde_json::json!(prior.archive)
                    || canonical
                        .as_ref()
                        .is_some_and(|p| v["canonical"] == serde_json::json!(p))
            })
        });
        if let Some(old) = old
            && old["prior"] != serde_json::json!(prior)
        {
            let original: ReviewImport =
                serde_json::from_value(old["prior"].clone()).map_err(|e| e.to_string())?;
            ledger.import_unknown(
                &original.id,
                &original.source,
                &original.contract,
                original.correction,
            )?;
            let mut held = old.clone();
            held["error"] =
                "workflow.review: prior archive/id/source/contract/correction conflict".into();
            persist_import(journal, ledger, held)?;
            error.get_or_insert("workflow.review: prior identity conflict".to_owned());
            continue;
        }
        if old.is_none() {
            ledger.import_unknown(&prior.id, &prior.source, &prior.contract, prior.correction)?;
            persist_import(
                journal,
                ledger,
                serde_json::json!({"prior":prior,"error":"prior accounting not yet read"}),
            )?;
        }
        eligible.push(prior);
    }
    let mut imported = Vec::new();
    for prior in eligible {
        let mut context = import_records(journal)[&prior.id].clone();
        let checked: Result<(), String> = (|| {
            let root = prior.archive.canonicalize().map_err(|e| e.to_string())?;
            let metadata = std::fs::metadata(&root).map_err(|e| e.to_string())?;
            let owner = serde_json::json!({"device":metadata.dev(),"inode":metadata.ino()});
            if !context["canonical"].is_null() && context["canonical"] != serde_json::json!(root)
                || !context["archive_owner"].is_null() && context["archive_owner"] != owner
            {
                return Err("workflow.review: original archive owner/path changed".into());
            }
            context["canonical"] = serde_json::json!(root);
            context["archive_owner"] = owner;
            let manifest = archive::read(&root, "manifest.json", 4 * 1024 * 1024)?;
            let manifest_sha = digest(&manifest);
            if !context["manifest_sha256"].is_null() && context["manifest_sha256"] != manifest_sha {
                return Err("workflow.review: original manifest changed".into());
            }
            let metadata = parse_workflow_input(&manifest)?;
            if metadata["task"].as_str().is_none_or(str::is_empty)
                || metadata["run"].as_str().is_none_or(str::is_empty)
            {
                return Err("workflow.review: prior archive identity missing".into());
            }
            let identity = serde_json::json!({"task":metadata["task"],"run":metadata["run"],"profile":metadata["profile"],"roster":metadata["roster"],"prompts":metadata["prompts"],"scope_path":metadata["scope_path"]});
            if !context["archive_identity"].is_null() && context["archive_identity"] != identity {
                return Err("workflow.review: original archive metadata differs".into());
            }
            for (id, old) in import_records(journal) {
                if id != prior.id && old["archive_identity"] == identity {
                    return Err(
                        "workflow.review: archive history already bound to another import id"
                            .into(),
                    );
                }
            }
            context["archive_identity"] = identity;
            let finished = metadata["finished_at"]
                .as_str()
                .is_some_and(|s| !s.is_empty());
            if finished {
                context["manifest_sha256"] = manifest_sha.into();
            }
            let events = archive::read(&root, "revust/events.jsonl", 64 * 1024 * 1024)?;
            if let Some(n) = context["prefix"]["bytes"].as_u64()
                && ((events.len() as u64) < n
                    || digest(&events[..n as usize]) != context["prefix"]["sha256"])
            {
                return Err("workflow.review: prior event prefix changed or shrank".into());
            }
            if let Some(old) = context["legacy_events_sha256"].as_str()
                && digest(&events) != old
            {
                return Err("workflow.review: retained legacy completed events changed".into());
            }
            context["prefix"] = serde_json::json!({"bytes":events.len(),"sha256":digest(&events)});
            let usage = match ReviewUsage::from_events(&events) {
                Ok(usage) => usage,
                Err(e) => {
                    // A partial last event cannot erase earlier positive starts.
                    let end = events
                        .iter()
                        .rposition(|b| !b.is_ascii_whitespace())
                        .map_or(0, |n| n + 1);
                    if let Some(end) = events[..end].iter().rposition(|b| *b == b'\n')
                        && let Ok(prefix) = ReviewUsage::from_events(&events[..=end])
                    {
                        let _ = ledger.import(
                            &prior.id,
                            &prior.source,
                            &prior.contract,
                            prefix,
                            prior.correction,
                        );
                    }
                    return Err(e);
                }
            };
            ledger.import(
                &prior.id,
                &prior.source,
                &prior.contract,
                usage,
                prior.correction,
            )?;
            if !finished {
                return Err("workflow.review: prior manifest completion remains unknown".into());
            }
            Ok(())
        })();
        match checked {
            Ok(()) => {
                context["error"] = serde_json::Value::Null;
            }
            Err(reason) => {
                ledger.import_unknown(
                    &prior.id,
                    &prior.source,
                    &prior.contract,
                    prior.correction,
                )?;
                context["error"] = reason.clone().into();
                error.get_or_insert(reason);
            }
        }
        persist_import(journal, ledger, context.clone())?;
        imported.push(serde_json::json!({"receipt":prior,"manifest_sha256":context["manifest_sha256"],"events_sha256":context["prefix"]["sha256"]}));
    }
    if let Some(error) = error {
        return Err(error);
    }
    Ok(imported)
}

impl Runs {
    pub(super) fn review_run(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &ReviewRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        if caller != Some(frozen.origin.session)
            || peer.is_none_or(|p| {
                p.session != frozen.origin.session
                    || p.project != frozen.origin.project
                    || p.workspace != frozen.origin.workspace
                    || p.harness != frozen.origin.harness
                    || p.cwd != frozen.origin.cwd
                    || p.conversation.is_none()
                    || p.conversation != frozen.origin.conversation
            })
        {
            return Err("workflow.review: exact live original parent binding required".into());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(frozen.run) {
            entry.insert(Journal::open(path)?);
        }
        let journal = self
            .journals
            .get_mut(&frozen.run)
            .ok_or("workflow.review: exclusive coordinator unavailable")?;
        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        // A public retry retains its original context before mutable source/profile
        // reads. It cannot prepare another round, spend allowance or spawn again.
        if let Some(old) = history
            .receipts
            .iter()
            .find(|r| {
                r.request
                    .as_ref()
                    .is_some_and(|r| r.request_id == spec.request_id)
            })
            .or_else(|| {
                history.receipts.iter().find(|r| {
                    r.request.as_ref().is_some_and(|r| {
                        r.context["operation"] == "review_preparation"
                            && r.context["spec"]["request_id"] == spec.request_id
                    })
                })
            })
        {
            if old
                .request
                .as_ref()
                .is_none_or(|r| r.context["spec"] != serde_json::json!(spec))
            {
                return Err("workflow.review: request key context conflict".into());
            }
            if old
                .request
                .as_ref()
                .is_some_and(|r| r.context["operation"] == "review_preparation")
            {
                let result = history
                    .receipts
                    .iter()
                    .find(|r| r.attempt == old.attempt && r.observation.is_some());
                return Err(format!(
                    "workflow.review: retained pre-start preparation {}; inspect receipt {}; use a new source-qualified request only after a proven preparation refusal",
                    if result.is_some() {
                        "refused/interrupted before launch"
                    } else {
                        "unknown"
                    },
                    old.attempt
                ));
            }
            return Ok(WorkflowReply::Receipt {
                run: frozen.run,
                revision: history.revision,
                receipt: Box::new(old.clone()),
                attempt: old.attempt,
                snapshot: snapshot(frozen, &history, &model),
            });
        }
        if model
            .reviews
            .iter()
            .any(|r| r["observation"]["not_started"] == true && !never_started(r))
        {
            return Err("workflow.review: pre-start resource/job ownership changed or remains uncertain; reconcile before another review".into());
        }
        if history.revision != spec.expected_revision {
            return Err("workflow.review: stale expected revision".into());
        }
        if journal.unresolved().iter().any(|r| {
            r.request
                .as_ref()
                .is_none_or(|m| m.context["operation"] != "review_import")
        }) {
            return Err(
                "workflow.review: unresolved preparation/effect reservation; no replay".into(),
            );
        }
        if history
            .receipts
            .iter()
            .filter_map(|r| r.observation.as_ref())
            .any(|o| {
                o["operation"] == "review_prepared"
                    && !history
                        .receipts
                        .iter()
                        .filter_map(|r| r.request.as_ref())
                        .any(|m| {
                            m.context["operation"] == "external_review"
                                && m.context["round"]["sha256"] == o["round"]
                        })
            })
        {
            return Err("workflow.review: prepared owner has no launch reservation; reconcile its retained archive/resources before another request".into());
        }
        let step = frozen
            .definition
            .steps
            .iter()
            .find(|s| {
                s.id == spec.step
                    && s.action == Action::ReviewRequest
                    && frozen.definition.roles[&s.role].owner == Owner::Review
            })
            .ok_or("workflow.review: configured external review node required")?;
        let retryable = model
            .graph
            .nodes()
            .iter()
            .any(|n| n.step == step.id && n.state == NodeState::Waiting)
            && model
                .reviews
                .iter()
                .rev()
                .find(|r| r["step"] == step.id)
                .is_some_and(|r| {
                    r.get("result").is_some()
                        && r["result"]["archive"]["classification"] != "findings"
                });
        if !retryable
            && !model
                .graph
                .nodes()
                .iter()
                .any(|n| n.step == step.id && n.state == NodeState::Ready)
        {
            return Err("workflow.review: configured review step is not ready".into());
        }
        let mut ledger = model.review_ledger.clone();
        let imported = import_priors(journal, &mut ledger, &spec.prior)?;
        let contract = model
            .accepted_contract
            .clone()
            .ok_or("workflow.review: accepted contract snapshot unavailable")?;
        let source =
            super::super::source::capture(&model.source.target, model.source.base.as_deref())?;
        if source != model.source {
            return Err(
                "workflow.review: source changed since parent checkpoint; requalify before review"
                    .into(),
            );
        }
        let number = model
            .reviews
            .iter()
            .filter(|r| r["step"] == step.id)
            .count()
            + 1;
        let round_id = format!("{}-{number:02}", step.id);
        let correction = history
            .receipts
            .iter()
            .rev()
            .filter_map(|r| r.request.as_ref())
            .find(|m| {
                m.context["operation"] == "same_session_correction"
                    && m.context["from"] == step.id
                    && m.context["claim"]
                        .as_str()
                        .is_some_and(|c| ledger.correction_pending(c))
            });
        if let Some(correction) = correction {
            ledger.reserve_for_correction(
                &round_id,
                &source.sha256,
                &contract.contract.sha256,
                spec.allowance,
                correction.context["claim"].as_str().unwrap(),
            )?;
        } else {
            ledger.reserve(
                &round_id,
                &source.sha256,
                &contract.contract.sha256,
                spec.allowance,
                false,
            )?;
        }
        let host = self
            .review_host
            .as_ref()
            .filter(|h| h.session == frozen.origin.session)
            .ok_or(
                "workflow.review: live native parent/socket/CLI unavailable; no launcher started",
            )?;
        let package = RevustPackage::qualify(&spec.package)?;
        if self.launchers.contains_key(&frozen.run) {
            return Err("workflow.review: previous launcher observer still owned; wait for actual exit before next serial round".into());
        }
        // Persist a bounded preparation owner before SDK/filesystem operations.
        // A synchronous failure is positively pre-launch: retain its archive and
        // resource receipts, without charging the external-review ledger. A crash
        // retains uncertainty and cannot be replayed by another request.
        let preparation_key = format!("prepare-{}", digest(spec.request_id.as_bytes()));
        let preparation_mutation = Mutation::new(
            &preparation_key,
            journal.revision(),
            serde_json::json!({"operation":"review_preparation","caller":caller,"spec":spec,"step":step.id,"round":round_id}),
        )?;
        let (preparation, fresh) = journal.reserve(&preparation_mutation)?;
        if !fresh {
            return Err("workflow.review: retained preparation must be reconciled".into());
        }
        let prepared: Result<RevustRound, String> = (|| {
            let role = &frozen.definition.roles[&step.role];
            let owned = path
                .join("reviews/preparations")
                .join(preparation.attempt.to_string());
            std::fs::create_dir_all(&owned).map_err(|e| e.to_string())?;
            let configuration = if let Some(first) = model.reviews.first() {
                let first: RevustRound =
                    serde_json::from_value(first["round"].clone()).map_err(|e| e.to_string())?;
                if first.package != package
                    || spec
                        .profile
                        .as_ref()
                        .is_some_and(|p| p != &first.configuration.profile)
                {
                    return Err("workflow.review: active run retains its selected package/profile; reconcile before changing selection".into());
                }
                let initial = history
                    .receipts
                    .iter()
                    .find_map(|r| {
                        r.request
                            .as_ref()
                            .filter(|r| r.context["operation"] == "external_review")
                    })
                    .ok_or("workflow.review: initial configuration selection unavailable")?;
                if initial.context["spec"]["config_dir"] != serde_json::json!(spec.config_dir) {
                    return Err(
                        "workflow.review: active run retains its initial config selection".into(),
                    );
                }
                first
                    .configuration
                    .with_allowance(&owned.join("control"), spec.allowance)?
            } else {
                let defaults = owned.join("defaults");
                package.export_defaults(&frozen.origin.cwd, &defaults)?;
                let effective = package.inspect_config(
                    &frozen.origin.cwd,
                    spec.config_dir.as_deref(),
                    spec.profile.as_deref(),
                    &source.target,
                )?;
                RevustConfigSnapshot::capture(
                    &frozen.origin.cwd,
                    &defaults,
                    &effective,
                    &owned.join("control"),
                    spec.allowance,
                )?
            };
            // Explicit role selections must agree with the actual selected roster;
            // never ignore them or silently replace the user's external profile.
            if role.harness.is_some() || role.model.is_some() || role.effort.is_some() {
                let roster = configuration.selected_profile["roster"]
                    .as_array()
                    .ok_or("workflow.review: selected profile roster unavailable")?;
                for runner in roster {
                    let executor = role.harness.map(|h| match h {
                        guild_session::HarnessKind::Claude => "claude",
                        guild_session::HarnessKind::Codex => "codex",
                        guild_session::HarnessKind::OpenCode => "opencode",
                    });
                    if executor.is_some_and(|v| runner["executor"] != v)
                        || role.model.as_ref().is_some_and(|v| runner["model"] != *v)
                        || role.effort.as_ref().is_some_and(|v| runner["effort"] != *v)
                    {
                        return Err("workflow.review: explicit role selection conflicts with selected external profile; reconcile selection before launch".into());
                    }
                }
            }
            let subject = RevustSubject {
                tasks_dir: path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .join("reviews/tasks")
                    .join(preparation.attempt.to_string()),
                task: format!("workflow-{}", frozen.run),
                round: round_id,
                source: source.clone(),
                contract,
                scope: spec.scope.clone(),
                keep_open: spec.keep_open,
            };
            let round = package.prepare_round(&configuration, subject)?;
            if super::super::source::capture(&source.target, source.base.as_deref())? != source {
                return Err("workflow.review: source changed during preparation; no launch".into());
            }
            Ok(round)
        })();
        let round = match prepared {
            Ok(round) => {
                journal.resolve(preparation.attempt, journal.revision(), serde_json::json!({"operation":"review_prepared","round":round.sha256,"providers_started":false}))?;
                round
            }
            Err(reason) => {
                journal.resolve(preparation.attempt, journal.revision(), serde_json::json!({"operation":"review_preparation_refused","reason":reason,"providers_started":false,"allowance_consumed":false}))?;
                return Err(reason);
            }
        };
        let mutation = Mutation::new(
            &spec.request_id,
            journal.revision(),
            serde_json::json!({"operation":"external_review","caller":caller,"spec":spec,"step":step.id,"round":round,"ledger":ledger,"imported":imported}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if !admitted {
            return Err("workflow.review: existing launch reservation needs reconciliation".into());
        }

        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(frozen, &history)?;
        self.review_effect = Some(PendingReview {
            native: super::reconciliation::permit(&history),
            run: frozen.run,
            step: step.id.clone(),
            attempt: receipt.attempt,
            revision: journal.revision(),
            origin: frozen.origin.clone(),
            round,
            ledger,
            host: host.clone(),
        });
        Ok(WorkflowReply::Receipt {
            run: frozen.run,
            revision: history.revision,
            attempt: receipt.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
    pub(in crate::root::workflow) fn launch_review(
        &mut self,
        effect: &PendingReview,
        permission: Result<(), String>,
    ) -> Result<(), String> {
        let journal = self
            .journals
            .get_mut(&effect.run)
            .ok_or("workflow.review: exclusive coordinator unavailable")?;
        if journal.revision() != effect.revision
            || !journal.receipts().iter().any(|r| {
                r.attempt == effect.attempt
                    && r.request.as_ref().is_some_and(|r| {
                        r.context["operation"] == "external_review"
                            && r.context["round"] == serde_json::json!(effect.round)
                    })
            })
        {
            return Err(
                "workflow.review: stale or changed native callback; no launcher started".into(),
            );
        }
        let ready = permission.and_then(|()| {
            let actual = super::super::source::capture(
                &effect.round.subject.source.target,
                effect.round.subject.source.base.as_deref(),
            )?;
            if actual != effect.round.subject.source {
                return Err("workflow.review: source changed before native launch".into());
            }
            Ok(())
        });
        let (pid, gap) =
            match ready.and_then(|()| RevustLauncher::spawn(&effect.round, &effect.host)) {
                Ok(launcher) => {
                    let pid = launcher.id();
                    self.launchers.insert(effect.run, launcher);
                    (Some(pid), None)
                }
                Err(error) => (None, Some(error)),
            };
        // spawn returns Err only before acquiring a child owner. Persist a
        // closed zero-start usage receipt; a missing job after a spawned
        // launcher is deliberately NOT handled by this path.
        let mut ledger = effect.ledger.clone();
        let not_started = pid.is_none();
        let mut environment_released = false;
        let mut gap = gap;
        if not_started {
            let usage = ReviewUsage::from_events(
                b"{\"type\":\"terminal\",\"report\":{\"state\":\"not_started\",\"units\":[]}}\n",
            )?;
            ledger.observe(&effect.round.subject.round, usage)?;
            if let Some(environment) = &effect.round.environment {
                match environment.release() {
                    Ok(()) => environment_released = true,
                    Err(error) => {
                        gap = Some(format!(
                            "{}; private resource remains uncertain: {error}",
                            gap.unwrap_or_default()
                        ))
                    }
                }
            }
        }
        journal.resolve(effect.attempt,journal.revision(),serde_json::json!({"operation":"review_launched","step":effect.step,"launcher_pid":pid,"gap":gap,"not_started":not_started,"environment_released":environment_released,"review_ledger":ledger}))?;
        Ok(())
    }
    pub(in crate::root::workflow) fn observe_review_job(
        &mut self,
        observed: &ReviewJob,
    ) -> Result<(), String> {
        let path = self
            .paths
            .get(&observed.run)
            .ok_or("workflow.review: linked job has no claimed run")?;
        let frozen = read_frozen(path)?;
        let journal = self
            .journals
            .get_mut(&observed.run)
            .ok_or("workflow.review: linked job has no coordinator")?;
        let reservation = journal
            .receipts()
            .iter()
            .find(|r| {
                r.attempt == observed.attempt
                    && r.request
                        .as_ref()
                        .is_some_and(|r| r.context["operation"] == "external_review")
            })
            .ok_or("workflow.review: job reservation unavailable")?;
        let request = reservation
            .request
            .as_ref()
            .ok_or("workflow.review: request missing")?;
        let round: RevustRound =
            serde_json::from_value(request.context["round"].clone()).map_err(|e| e.to_string())?;
        let step = request.context["step"]
            .as_str()
            .ok_or("workflow.review: job step unavailable")?
            .to_owned();
        if round.sha256 != observed.round_sha256
            || observed.receipt.owner != frozen.origin.session
            || observed.receipt.project != frozen.origin.project
            || observed.receipt.revmux_archive.as_ref().is_none_or(|a| {
                a.directory != round.archive || a.program != round.package.root.join("bin/revust")
            })
        {
            return Err(
                "workflow.review: actual native job binding differs from reserved round".into(),
            );
        }
        let history = History {
            run: frozen.run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(&frozen, &history)?;
        if model
            .reviews
            .iter()
            .find(|r| r["round"]["sha256"] == round.sha256)
            .and_then(|r| r["job"]["id"].as_str())
            .is_some_and(|id| id != observed.receipt.id.to_string())
        {
            return Err(
                "workflow.review: one reserved round cannot bind another actual job".into(),
            );
        }
        let fingerprint =
            digest(&serde_json::to_vec(&observed.receipt).map_err(|e| e.to_string())?);
        let key = format!("review-job-{}-{}", observed.receipt.id, &fingerprint[..12]);
        if journal
            .receipts()
            .iter()
            .any(|r| r.request.as_ref().is_some_and(|r| r.request_id == key))
        {
            return Ok(());
        }
        let mutation = Mutation::new(
            &key,
            journal.revision(),
            serde_json::json!({"operation":"review_job_observation","round_sha256":round.sha256,"job":observed.receipt}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if admitted {
            journal.resolve(receipt.attempt,journal.revision(),serde_json::json!({"operation":"review_job_observed","step":step,"job":observed.receipt}))?;
        }
        Ok(())
    }
    pub(super) fn poll_review(&mut self, run: RunId) {
        if let Err(error) = self.release_review_environments(run) {
            tracing::warn!(%error,"review lease retained; actual owner completion unqualified");
        }
        if let Err(error) = self.observe_launcher(run) {
            tracing::warn!(%error,%run,"workflow review observation retained as unresolved");
        }
        if let Err(error) = self.observe_review_archive(run) {
            tracing::warn!(%error,%run,"workflow review archive remains unqualified");
        }
    }
    fn observe_review_archive(&mut self, run: RunId) -> Result<(), String> {
        let Some(journal) = self.journals.get(&run) else {
            return Ok(());
        };
        let path = self
            .paths
            .get(&run)
            .ok_or("workflow.review: owning path missing")?
            .clone();
        let frozen = read_frozen(&path)?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(&frozen, &history)?;
        let Some(row) = model
            .reviews
            .iter()
            .find(|r| r.get("result").is_none() && r.get("job").is_some())
        else {
            return Ok(());
        };
        let job: guild_session::terminal_job::JobReceipt =
            serde_json::from_value(row["job"].clone()).map_err(|e| e.to_string())?;
        if !job.state.is_terminal()
            && archive::read(
                &PathBuf::from(row["round"]["archive"].as_str().unwrap_or("")),
                "manifest.json",
                4 * 1024 * 1024,
            )
            .ok()
            .and_then(|b| parse_workflow_input(&b).ok())
            .is_none_or(|v| v["finished_at"].as_str().is_none_or(str::is_empty))
        {
            return Ok(());
        }
        let round: RevustRound =
            serde_json::from_value(row["round"].clone()).map_err(|e| e.to_string())?;
        let step = row["step"]
            .as_str()
            .ok_or("workflow.review: result step missing")?;
        let mut ledger = model.review_ledger.clone();
        let usage = archive::read(&round.archive, "revust/events.jsonl", 64 * 1024 * 1024)
            .and_then(|bytes| ReviewUsage::from_events(&bytes));
        let mut checked = None;
        let cancelled = model
            .graph
            .nodes()
            .iter()
            .any(|n| n.state == NodeState::Cancelled);
        let qualified = match usage {
            Ok(usage) if usage.closed() => {
                ledger.observe(&round.subject.round, usage)?;
                match archive::checked(&frozen, &path, &model, step, &round, &job) {
                    Ok(result) => {
                        checked = Some(result.clone());
                        if cancelled {
                            Err("Cancelled graph: late archive is history only".into())
                        } else {
                            archive::qualify(&frozen, &path, step, &round, &model, &result)
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            Ok(usage) => {
                ledger.observe(&round.subject.round, usage)?;
                ledger.uncertain(&round.subject.round)?;
                Err("UNVERIFIED: actual terminal job ended without complete attempt accounting; reservation retained".into())
            }
            Err(error) => {
                ledger.uncertain(&round.subject.round)?;
                Err(format!(
                    "UNVERIFIED: actual review usage unreadable; reservation retained: {error}"
                ))
            }
        };
        // Only the actual terminal registry's terminal receipt proves this
        // resource is no longer in use. Observer exit alone never releases it.
        let released = job.state == guild_session::terminal_job::JobState::Exited
            && round.environment.is_some();
        if released && let Some(environment) = &round.environment {
            environment.release()?;
        }
        let (qualified, gap) = match qualified {
            Ok(qualified) => (Some(qualified), None),
            Err(error) => (None, Some(error)),
        };
        let mutation = Mutation::new(
            &format!("{}-archive-result", round.subject.round),
            history.revision,
            serde_json::json!({"operation":"review_archive_result","round_sha256":round.sha256,"job":job.id,"qualified":qualified,"gap":gap,"checked":checked,"ledger":ledger}),
        )?;
        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.review: coordinator unavailable")?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if admitted {
            journal.resolve(receipt.attempt,journal.revision(),serde_json::json!({"operation":if qualified.is_some(){"review_qualified"}else{"review_result"},"step":step,"job":job.id,"qualified":qualified,"gap":gap,"review_ledger":ledger,"source":round.subject.source,"contract":round.subject.contract.contract,"accepted_contract":round.subject.contract,"environment_released":released,"archive":checked,"history_only":cancelled}))?;
            self.effect_admission = qualified.is_some();
        }
        Ok(())
    }
    fn release_review_environments(&mut self, run: RunId) -> Result<(), String> {
        let Some(journal) = self.journals.get(&run) else {
            return Ok(());
        };
        let path = self.paths.get(&run).ok_or("review lease: path missing")?;
        let frozen = read_frozen(path)?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(&frozen, &history)?;
        for row in &model.reviews {
            if row.get("result").is_none() || row["result"]["environment_released"] == true {
                continue;
            }
            let Some(job) = row.get("job").cloned().and_then(|v| {
                serde_json::from_value::<guild_session::terminal_job::JobReceipt>(v).ok()
            }) else {
                continue;
            };
            if job.state != guild_session::terminal_job::JobState::Exited {
                continue;
            }
            let round: RevustRound =
                serde_json::from_value(row["round"].clone()).map_err(|e| e.to_string())?;
            let Some(environment) = &round.environment else {
                continue;
            };
            environment.release()?;
            let journal = self.journals.get_mut(&run).unwrap();
            let mutation = Mutation::new(
                &format!("{}-environment-released", round.subject.round),
                journal.revision(),
                serde_json::json!({"operation":"review_environment_release","round_sha256":round.sha256}),
            )?;
            let (r, fresh) = journal.reserve(&mutation)?;
            if fresh {
                journal.resolve(r.attempt,r.revision,serde_json::json!({"operation":"review_environment_released","round_sha256":round.sha256}))?;
            }
        }
        Ok(())
    }
    fn observe_launcher(&mut self, run: RunId) -> Result<(), String> {
        let Some(launcher) = self.launchers.get_mut(&run) else {
            return Ok(());
        };
        let Some(observed) = launcher.try_wait()? else {
            return Ok(());
        };
        let path = self
            .paths
            .get(&run)
            .ok_or("workflow.review: owning path missing")?;
        let frozen = read_frozen(path)?;
        let journal = self
            .journals
            .get_mut(&run)
            .ok_or("workflow.review: exclusive coordinator unavailable")?;
        let history = History {
            run,
            revision: journal.revision(),
            receipts: journal.receipts().to_vec(),
        };
        let model = reconstruct(&frozen, &history)?;
        let row = model
            .reviews
            .last()
            .ok_or("workflow.review: launcher has no reserved round")?;
        let round: RevustRound =
            serde_json::from_value(row["round"].clone()).map_err(|e| e.to_string())?;
        let step = row["step"]
            .as_str()
            .ok_or("workflow.review: observed step missing")?;
        let mut ledger = model.review_ledger.clone();
        let events = archive::read(&round.archive, "revust/events.jsonl", 64 * 1024 * 1024)
            .and_then(|bytes| ReviewUsage::from_events(&bytes));
        let mut gap = match events {
            Ok(usage) => {
                ledger.observe(&round.subject.round, usage)?;
                Some("Launcher ended; actual terminal job and checked review outcome still require reconciliation".to_owned())
            }
            Err(error) => {
                ledger.uncertain(&round.subject.round)?;
                Some(format!(
                    "Launcher ended with uncertain usage; reservation retained: {error}"
                ))
            }
        };
        if let Some(result) = row.get("result") {
            gap = result["gap"].as_str().map(str::to_owned);
        }

        let mutation = Mutation::new(
            &format!("{}-observer-exit", round.subject.round),
            journal.revision(),
            serde_json::json!({"operation":"review_observer_exit","round_sha256":round.sha256,"observer":observed}),
        )?;
        let (receipt, admitted) = journal.reserve(&mutation)?;
        if admitted {
            journal.resolve(receipt.attempt,journal.revision(),serde_json::json!({"operation":"review_observed","step":step,"observer":observed,"gap":gap,"review_ledger":ledger}))?;
        }
        self.launchers.remove(&run);
        Ok(())
    }
}

#[cfg(test)]
pub(in crate::root::workflow) fn prelaunch_refusal_fixture(path: &Path, round: RevustRound) {
    // Reuse a genuinely SDK-prepared fixture packet, but never launch it. The
    // trusted pre-launch callback must settle its reservation before cancellation.
    let frozen = read_frozen(path).unwrap();
    let step = frozen
        .definition
        .steps
        .iter()
        .find(|s| s.action == Action::ReviewRequest)
        .unwrap()
        .id
        .clone();
    let mut origin = frozen.origin.clone();
    origin.session = SessionId::new();
    origin.conversation = Some(origin.session.to_string());
    let mut spec = frozen.start.clone();
    spec.origin = origin.session;
    spec.request_id = "controlled-never-started-review".into();
    let mut runs = Runs {
        entry_enabled: true,
        ..Runs::default()
    };
    let WorkflowReply::Receipt { run, .. } = runs.execute(
        &WorkflowCommand::Start { spec: spec.clone() },
        Some(origin.session),
        Some(origin.clone()),
        &[],
    ) else {
        panic!("isolated cancellation fixture did not start")
    };
    let mut ledger = ReviewLedger::new(run, frozen.definition.limits.clone()).unwrap();
    ledger
        .reserve(
            &round.subject.round,
            &round.subject.source.sha256,
            &round.subject.contract.contract.sha256,
            1,
            false,
        )
        .unwrap();
    let journal = runs.journals.get_mut(&run).unwrap();
    let mutation = Mutation::new("controlled-native-reservation",journal.revision(),serde_json::json!({"operation":"external_review","step":step,"round":round,"ledger":ledger})).unwrap();
    let (receipt, _) = journal.reserve(&mutation).unwrap();
    let effect = PendingReview {
        run,
        step,
        attempt: receipt.attempt,
        revision: receipt.revision,
        origin: origin.clone(),
        round,
        ledger,
        host: RevustHost {
            session: origin.session,
            socket: path.join("never-created.socket"),
            cli: path.join("never-created-cli"),
        },
        native: None,
    };
    runs.launch_review(
        &effect,
        Err("controlled positive native pre-launch refusal".into()),
    )
    .unwrap();
    assert!(runs.launchers.is_empty());
    let journal = runs.journals.get(&run).unwrap();
    let history = History {
        run,
        revision: journal.revision(),
        receipts: journal.receipts().to_vec(),
    };
    let owned = read_frozen(runs.paths.get(&run).unwrap()).unwrap();
    let model = reconstruct(&owned, &history).unwrap();
    assert_eq!(
        model.review_ledger.reserved(),
        0,
        "positively never-started review must not retain unknown usage"
    );
    assert_eq!(model.review_ledger.used(), 0);
    let cancelled = runs.execute(
        &WorkflowCommand::Cancel {
            run,
            spec: ControlRequest {
                version: 1,
                request_id: "cancel-never-started-review".into(),
                expected_revision: history.revision,
            },
        },
        Some(origin.session),
        Some(origin.clone()),
        &[],
    );
    assert!(
        matches!(cancelled, WorkflowReply::Receipt { .. }),
        "{cancelled:?}"
    );
    spec.request_id = "new-run-after-absent-review-owner".into();
    let next = runs.execute(
        &WorkflowCommand::Start { spec },
        Some(origin.session),
        Some(origin),
        &[],
    );
    assert!(
        matches!(next, WorkflowReply::Receipt { .. }),
        "confirmed absent review must release only its cancelled origin: {next:?}"
    );
    eprintln!(
        "CONTROLLED_NEVER_STARTED_REVIEW reserved=0 used=0 cancelled origin released; no launcher/job/provider"
    );
}
