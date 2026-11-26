//! Explicit source-qualified acknowledgement, separate from harness continuation.
use super::*;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::root::workflow) struct Native {
    pub binding: OriginBinding,
    pub task: bool,
    pub selection: Option<guild_session::harness::LaunchSelection>,
    pub permission: Option<guild_session::PermissionMode>,
    pub status: String,
    pub holders: Vec<u32>,
    pub identity_qualified: bool,
}
pub(in crate::root::workflow) type Observations = Arc<RwLock<BTreeMap<String, Native>>>;

impl Native {
    fn assessment_basis(&self, origin: SessionId) -> Self {
        let mut basis = self.clone();
        if self.binding.session == origin
            && matches!(self.status.as_str(), "Working" | "Done" | "Idle")
        {
            // Ordinary parent activity does not change orchestration authority.
            // Raw observations and handoff/input readiness remain untouched.
            basis.status = "parent_activity".into();
        }
        basis
    }
}
/// The same stable parent projection guards queued effects at actual execution.
/// Old in-flight map-only permits retain their strict comparison semantics.
pub(in crate::root::workflow) fn check_native_permit(
    expected: &serde_json::Value,
    current: &BTreeMap<String, Native>,
) -> Result<(), String> {
    let (rows, origin) = if let Some(rows) = expected.get("native") {
        (
            rows,
            Some(
                serde_json::from_value::<SessionId>(expected["origin"].clone())
                    .map_err(|_| "workflow.reconcile: invalid native origin")?,
            ),
        )
    } else {
        (expected, None)
    };
    for (id, old) in rows
        .as_object()
        .ok_or("workflow.reconcile: native assessment unavailable")?
    {
        let id: SessionId = id
            .parse()
            .map_err(|_| "workflow.reconcile: invalid native identity")?;
        if current
            .get(&id.to_string())
            .and_then(|n| {
                serde_json::to_value(
                    origin.map_or_else(|| n.clone(), |origin| n.assessment_basis(origin)),
                )
                .ok()
            })
            .as_ref()
            != Some(old)
        {
            return Err(
                "workflow.reconcile: native state changed after acknowledgement; no effect".into(),
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Blocker {
    code: &'static str,
    session: Option<SessionId>,
}
fn blocked(rows: &mut Vec<Blocker>, code: &'static str, session: Option<SessionId>) {
    let row = Blocker { code, session };
    if !rows.contains(&row) {
        rows.push(row);
    }
}
fn native_rows(
    frozen: &Frozen,
    model: &Model,
    observations: &Observations,
) -> BTreeMap<String, Native> {
    let all = observations.read().unwrap_or_else(|e| e.into_inner());
    let mut ids = vec![frozen.origin.session];
    ids.extend(
        model
            .assignments
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                serde_json::from_value::<SessionId>(a["observation"]["session"].clone()).ok()
            }),
    );
    ids.into_iter()
        .filter_map(|id| {
            all.get(&id.to_string())
                .map(|v| (id.to_string(), v.clone()))
        })
        .collect()
}
// Inspect only this run's recorded preparation owners, including preparation
// interrupted before a review reservation. Never enumerate private/global temp
// directories or read environment payloads. Unknown receipts remain blockers.
fn preparation_owners(
    root: &Path,
    depth: usize,
    rows: &mut Vec<serde_json::Value>,
    budget: &mut usize,
) -> Result<(), String> {
    if depth > 6 || rows.len() > 1024 {
        return Err("workflow.reconcile: resource receipt bounds exceeded".into());
    }
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("workflow.reconcile: resource receipts unreadable".into()),
    };
    let mut count = 0;
    for entry in entries {
        *budget = budget
            .checked_sub(1)
            .ok_or("workflow.reconcile: resource receipt bounds exceeded")?;
        count += 1;
        if count > 1024 {
            return Err("workflow.reconcile: resource receipt bounds exceeded".into());
        }
        let entry = entry.map_err(|_| "workflow.reconcile: resource receipt entry unavailable")?;
        let kind = entry
            .file_type()
            .map_err(|_| "workflow.reconcile: resource receipt type unknown")?;
        if kind.is_symlink() {
            return Err("workflow.reconcile: linked resource owner refused".into());
        }
        if kind.is_dir() && !(depth == 0 && entry.file_name() == "tasks") {
            preparation_owners(&entry.path(), depth + 1, rows, budget)?;
        }
        if kind.is_file() && entry.file_name() == "environment-lease.json" {
            let bytes = bounded_json_file(&entry.path())?;
            let lease = parse_workflow_input(&bytes).and_then(|v| {
                serde_json::from_value::<RevustEnvironmentLease>(v).map_err(|e| e.to_string())
            });
            let state = match lease.as_ref().map(|l| l.retained()) {
                Ok(Ok(false)) => "released",
                Ok(Ok(true)) => "retained",
                _ => "unknown",
            };
            rows.push(serde_json::json!({"receipt":entry.path(),"sha256":digest(&bytes),"owner":lease.as_ref().ok().map(|l|l.owner),"state":state}));
        }
    }
    rows.sort_by(|a, b| a["receipt"].as_str().cmp(&b["receipt"].as_str()));
    Ok(())
}
pub(super) fn assessment(
    frozen: &Frozen,
    history: &History,
    observations: &Observations,
) -> Result<serde_json::Value, String> {
    let model = reconstruct(frozen, history)?;
    let native = native_rows(frozen, &model, observations);
    let mut blockers = vec![];
    let current =
        super::super::source::capture(&model.source.target, model.source.base.as_deref()).ok();
    let parent_expected = history
        .receipts
        .iter()
        .rev()
        .filter_map(|r| r.observation.as_ref())
        .filter_map(|o| serde_json::from_value::<SourceIdentity>(o["source"].clone()).ok())
        .find(|s| s.target == frozen.start.target)
        .unwrap_or_else(|| frozen.source.clone());
    let parent_source =
        super::super::source::capture(&frozen.start.target, parent_expected.base.as_deref()).ok();
    if current.as_ref() != Some(&model.source) || parent_source.as_ref() != Some(&parent_expected) {
        blocked(&mut blockers, "source_changed", Some(frozen.origin.session));
    }
    if let Some(contract) = &model.accepted_contract
        && (contract.verify().is_err() || contract.contract.verify().is_err())
    {
        blocked(
            &mut blockers,
            "contract_changed",
            Some(frozen.origin.session),
        );
    }
    let origin = native.get(&frozen.origin.session.to_string());
    if origin.is_none_or(|n| {
        !n.identity_qualified
            || n.task
            || n.holders.len() != 1
            || n.binding.harness != frozen.origin.harness
            || n.binding.conversation != frozen.origin.conversation
            || n.binding.cwd != frozen.origin.cwd
            || n.binding.project != frozen.origin.project
            || n.binding.workspace != frozen.origin.workspace
    }) {
        blocked(&mut blockers, "origin_unknown", Some(frozen.origin.session));
    }
    if let Some(n) = origin {
        for request in history
            .receipts
            .iter()
            .filter_map(|r| r.request.as_ref())
            .filter(|m| m.context["operation"] == "origin_recovery")
        {
            if let Some(expected) = request.context.get("binding")
                && (expected["selection"]
                    != serde_json::to_value(&n.selection).map_err(|e| e.to_string())?
                    || expected["permission"]
                        != serde_json::to_value(n.permission).map_err(|e| e.to_string())?)
            {
                blocked(&mut blockers, "origin_unknown", Some(frozen.origin.session));
            }
        }
        if n.permission.is_none() {
            blocked(&mut blockers, "origin_unknown", Some(frozen.origin.session));
        }
    }
    for n in native.values() {
        let id = n.binding.session;
        if n.status.starts_with("Waiting(") {
            blocked(
                &mut blockers,
                if n.status == "Waiting(Permission)" {
                    "permission"
                } else {
                    "question"
                },
                Some(id),
            );
        }
        if n.status == "Idle" && id != frozen.origin.session {
            blocked(&mut blockers, "status_unknown", Some(id));
        }
        if !n.identity_qualified || n.holders.len() > 1 {
            blocked(&mut blockers, "holder_unknown", Some(id));
        }
    }
    let unresolved: Vec<_> = history
        .receipts
        .iter()
        .filter(|r| {
            r.request.is_some()
                && !history
                    .receipts
                    .iter()
                    .any(|o| o.attempt == r.attempt && o.observation.is_some())
        })
        .map(|r| r.attempt)
        .collect();
    if !unresolved.is_empty() {
        blocked(&mut blockers, "effect_unknown", None);
    }
    if handoff::state(history)
        .is_some_and(|s| matches!(s["stage"].as_str(), Some("submitted" | "unknown")))
    {
        blocked(
            &mut blockers,
            "handoff_unknown",
            Some(frozen.origin.session),
        );
    }
    let mut children = vec![];
    for row in model
        .assignments
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["observation"]["bound"] == true)
    {
        let observed = &row["observation"];
        let id: SessionId =
            serde_json::from_value(observed["session"].clone()).map_err(|e| e.to_string())?;
        let checkout = PathBuf::from(
            observed["checkout"]
                .as_str()
                .ok_or("workflow.reconcile: assignment checkout missing")?,
        );
        let source = super::super::source::capture(&checkout, row["plan"]["base"].as_str());
        let delivered = history.receipts.iter().any(|r| {
            r.observation.as_ref().is_some_and(|v| {
                v["operation"] == "assignment_delivered" && v["session"] == serde_json::json!(id)
            })
        });
        if !delivered {
            blocked(&mut blockers, "delivery_unknown", Some(id));
        }
        if !native.contains_key(&id.to_string()) {
            blocked(&mut blockers, "holder_unknown", Some(id));
        }
        if let Some(n) = native.get(&id.to_string()) {
            if !n.task
                || n.binding.cwd.canonicalize().ok() != checkout.canonicalize().ok()
                || !reports::is_assigned(history, id, Some(&n.binding))?
            {
                blocked(&mut blockers, "child_identity", Some(id));
            }
            if n.holders.is_empty()
                && !model
                    .reports
                    .iter()
                    .any(|r| r["producer"] == serde_json::json!(id))
            {
                blocked(&mut blockers, "report_pending", Some(id));
            }
        }
        if source.is_err() {
            blocked(&mut blockers, "child_source_unknown", Some(id));
        }
        // Reports keep their original source; a changed checkout is visible and
        // cannot be treated as an accepted replacement report.
        for report in model
            .reports
            .iter()
            .filter(|r| r["producer"] == serde_json::json!(id))
        {
            if report.get("source").is_some_and(|v| v.is_object())
                && source
                    .as_ref()
                    .is_ok_and(|s| serde_json::to_value(s).ok().as_ref() != report.get("source"))
            {
                blocked(&mut blockers, "child_report_stale", Some(id));
            }
        }
        children.push(serde_json::json!({"session":id,"checkout":checkout,"source":source.as_ref().ok().map(|s|&s.sha256),"delivered":delivered}));
    }
    if model.review_ledger.reserved() != 0 {
        blocked(&mut blockers, "review_usage_unknown", None);
    }
    let ledger = serde_json::to_value(&model.review_ledger).map_err(|e| e.to_string())?;
    if ledger["rounds"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|r| r["uncertain"] == true || r["usage"]["closed"] != true)
    {
        blocked(&mut blockers, "review_usage_unknown", None);
    }
    let mut leases = vec![];
    let mut archive_facts = vec![];
    for row in &model.reviews {
        if row.get("result").is_none() {
            blocked(&mut blockers, "review_pending", None);
        }
        if let Some(job) = row.get("job")
            && !serde_json::from_value::<guild_session::terminal_job::JobReceipt>(job.clone())
                .is_ok_and(|j| j.state.is_terminal())
        {
            blocked(&mut blockers, "review_pending", None);
        }
        let archive = row["round"]["archive"].as_str().map(PathBuf::from);
        let mut facts = vec![];
        if review::never_started(row) {
            facts.push(serde_json::json!({"state":"not_started","reservation":row["attempt"],"observation":row["observation"]}));
        } else if let Some(archive) = archive {
            let usage = review::recovery_usage(&archive);
            let retained = ledger["rounds"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["id"] == row["round"]["subject"]["round"]);
            if !usage.as_ref().is_ok_and(|u| {
                u.closed()
                    && retained
                        .is_some_and(|r| serde_json::to_value(u).ok().as_ref() == r.get("usage"))
            }) {
                blocked(&mut blockers, "review_usage_unknown", None);
            }

            for relative in [
                "manifest.json",
                "findings.json",
                "report.md",
                "revust/events.jsonl",
            ] {
                let result = review::recovery_archive(&archive, relative);
                if result.is_err() {
                    blocked(&mut blockers, "review_archive_unknown", None);
                }
                facts.push(serde_json::json!({"path":archive.join(relative),"sha256":result.ok()}));
            }
        } else {
            blocked(&mut blockers, "review_archive_unknown", None);
        }
        archive_facts.push(facts);
        if let Some(value) = row["round"].get("environment").filter(|v| !v.is_null()) {
            let lease: RevustEnvironmentLease = serde_json::from_value(value.clone())
                .map_err(|_| "workflow.reconcile: corrupt private resource owner")?;
            let state = match lease.retained() {
                Ok(false) => "released",
                Ok(true) => "retained",
                Err(_) => "unknown",
            };
            if state != "released" {
                blocked(&mut blockers, "resource_owner", None);
            }
            leases.push(serde_json::json!({"owner":lease.owner,"round":lease.round,"state":state}));
        }
    }
    for skill in model.skills.values() {
        if matches!(skill,SkillPin::Captured{skill} if skill.verify().is_err()) {
            blocked(&mut blockers, "resource_changed", None);
        }
    }
    let mut artifacts = vec![];
    for observed in history
        .receipts
        .iter()
        .filter_map(|r| r.observation.as_ref())
    {
        for checked in observed["qualified"].as_array().into_iter().flatten() {
            for artifact in checked["evidence"]["artifacts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let artifact: EvidenceArtifact = serde_json::from_value(artifact.clone())
                    .map_err(|_| "workflow.reconcile: corrupt evidence artifact")?;
                use std::io::Read;
                let bytes = std::fs::File::open(&artifact.path).and_then(|f| {
                    let mut bytes = vec![];
                    f.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                    Ok(bytes)
                });
                let hash = bytes
                    .ok()
                    .filter(|b| b.len() <= 16 * 1024 * 1024)
                    .map(|b| digest(&b));
                if hash.as_ref() != Some(&artifact.sha256) {
                    blocked(&mut blockers, "resource_changed", None);
                }
                artifacts.push(serde_json::json!({"path":artifact.path,"sha256":hash}));
            }
        }
    }
    let mut preparation = vec![];
    let review_root = main_root(&frozen.start.target)?
        .join(".guild/workflow-runs")
        .join(frozen.run.to_string())
        .join("reviews");
    if preparation_owners(&review_root, 0, &mut preparation, &mut 4096).is_err()
        || preparation.iter().any(|p| p["state"] != "released")
    {
        blocked(&mut blockers, "resource_owner", None);
    }
    let assignment_attempts: Vec<_> = history
        .receipts
        .iter()
        .filter(|r| {
            r.request
                .as_ref()
                .is_some_and(|m| m.context["operation"] == "native_assignment")
        })
        .map(|r| r.attempt)
        .collect();
    let navigation_attempts: Vec<_> = history
        .receipts
        .iter()
        .filter(|r| {
            r.request
                .as_ref()
                .is_some_and(|m| m.context["operation"] == "origin_recovery")
                && history.receipts.iter().any(|o| {
                    o.attempt == r.attempt
                        && o.observation.as_ref().is_some_and(|v| {
                            v["recovery"]["phase"] == "origin_live_reused"
                                && v["recovery"]["resume_attempted"] == false
                        })
                })
        })
        .map(|r| r.attempt)
        .collect();
    let events: Vec<_> = history
        .receipts
        .iter()
        .filter(|r| !navigation_attempts.contains(&r.attempt))
        .filter(|r| !assignment_attempts.contains(&r.attempt))
        .filter(|r| {
            !r.request
                .as_ref()
                .is_some_and(|m| m.context["operation"] == "parent_reconciliation")
                && !r
                    .observation
                    .as_ref()
                    .is_some_and(|o| o["operation"] == "parent_reconciled")
        })
        .map(|r| &r.sha256)
        .collect();
    let native: BTreeMap<_, _> = native
        .into_iter()
        .map(|(id, n)| (id, n.assessment_basis(frozen.origin.session)))
        .collect();
    let basis = serde_json::json!({"run":frozen.run,"frozen":frozen.sha256,"origin":frozen.origin,"events":events,"source":current,"accepted_source":model.source,"parent_source":parent_source,"contract":model.accepted_contract,"children":children,"reports":model.reports,"reviews":model.reviews,"ledger":model.review_ledger,"leases":leases,"archives":archive_facts,"artifacts":artifacts,"preparation":preparation,"native":native,"blockers":blockers});
    let fingerprint = digest(&serde_json::to_vec(&basis).map_err(|e| e.to_string())?);
    Ok(
        serde_json::json!({"fingerprint":fingerprint,"clear":blockers.is_empty(),"blockers":blockers,"basis":basis,"ready":false,"continued":false,"transport":"explicit_parent_checkpoint_only"}),
    )
}
fn acknowledgement(history: &History) -> Option<&serde_json::Value> {
    history
        .receipts
        .iter()
        .rev()
        .filter_map(|r| r.observation.as_ref())
        .find(|v| v["operation"] == "parent_reconciled")
}
pub(super) fn permit(history: &History) -> Option<serde_json::Value> {
    recovery::has_resume_barrier(history)
        .then(|| acknowledgement(history).map(|a| serde_json::json!({"origin":a["basis"]["origin"]["session"],"native":a["basis"]["native"]})))
        .flatten()
}
pub(super) fn guard(
    frozen: &Frozen,
    history: &History,
    observations: &Observations,
) -> Result<bool, String> {
    if !recovery::has_resume_barrier(history) {
        return Ok(false);
    }
    let current = assessment(frozen, history, observations)?;
    Ok(!current["clear"].as_bool().unwrap_or(false)
        || acknowledgement(history).is_none_or(|a| a["assessment"] != current["fingerprint"]))
}
impl Runs {
    pub(super) fn reconcile_run(
        &mut self,
        path: &Path,
        frozen: &Frozen,
        spec: &ReconcileRequest,
        caller: Option<SessionId>,
        peer: Option<&OriginBinding>,
    ) -> Result<WorkflowReply, String> {
        if caller != Some(frozen.origin.session) || peer != Some(&frozen.origin) {
            // Area placement is not a new parent identity; compare separately.
            let mut expected = frozen.origin.clone();
            expected.area = peer.and_then(|p| p.area);
            if caller != Some(frozen.origin.session) || peer != Some(&expected) {
                return Err("workflow.reconcile: exact original parent caller required".into());
            }
        }
        let history = Journal::inspect(path)?;
        let current = assessment(frozen, &history, &self.observations)?;
        if spec.assessment.is_none() {
            self.effect_admission = false;
            let model = reconstruct(frozen, &history)?;
            let mut value = snapshot(frozen, &history, &model);
            value["reconciliation"] = current;
            return Ok(WorkflowReply::Snapshot {
                run: frozen.run,
                revision: history.revision,
                snapshot: value,
            });
        }
        let mutation = Mutation::new(
            &spec.request_id,
            spec.expected_revision,
            serde_json::json!({"operation":"parent_reconciliation","caller":caller,"origin":peer,"spec":spec}),
        )?;
        if let Some(previous) = history.receipts.iter().find(|r| {
            r.request
                .as_ref()
                .is_some_and(|m| m.request_id == spec.request_id)
        }) {
            if previous.request.as_ref() != Some(&mutation) {
                return Err("workflow.request: reconciliation key context conflict".into());
            }
            if !history.receipts.iter().any(|r| {
                r.attempt == previous.attempt
                    && r.observation
                        .as_ref()
                        .is_some_and(|o| o["operation"] == "parent_reconciled")
            }) {
                return Err(
                    "workflow.reconcile: acknowledgement reservation unresolved; no replay".into(),
                );
            }
            self.effect_admission = false;
            let model = reconstruct(frozen, &history)?;
            return Ok(WorkflowReply::Receipt {
                run: frozen.run,
                revision: history.revision,
                attempt: previous.attempt,
                receipt: Box::new(previous.clone()),
                snapshot: snapshot(frozen, &history, &model),
            });
        }
        if current["clear"] != true {
            return Err(
                "workflow.reconcile: concrete blockers remain; inspect reconciliation checklist"
                    .into(),
            );
        }
        if spec.expected_revision != history.revision
            || spec.assessment.as_deref() != current["fingerprint"].as_str()
        {
            return Err("workflow.reconcile: stale assessment/source/assignment/native observations; inspect again".into());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.journals.entry(frozen.run) {
            entry.insert(Journal::open(path)?);
        }
        // Native owner observations may change while filesystem facts are read.
        let rechecked = assessment(frozen, &Journal::inspect(path)?, &self.observations)?;
        if rechecked != current {
            return Err("workflow.reconcile: observations changed before acknowledgement".into());
        }
        let journal = self
            .journals
            .get_mut(&frozen.run)
            .ok_or("workflow.reconcile: claim unavailable")?;
        let (reserved, _) = journal.reserve(&mutation)?;
        let receipt=journal.resolve(reserved.attempt,reserved.revision,serde_json::json!({"operation":"parent_reconciled","assessment":spec.assessment,"basis":current["basis"],"ready":false,"continued":false,"transport":"explicit_parent_checkpoint_only"}))?;
        self.effect_admission = true;
        let history = Journal::inspect(path)?;
        let model = reconstruct(frozen, &history)?;
        Ok(WorkflowReply::Receipt {
            run: frozen.run,
            revision: history.revision,
            attempt: reserved.attempt,
            receipt: Box::new(receipt),
            snapshot: snapshot(frozen, &history, &model),
        })
    }
    pub(super) fn decorate_reconciliation(&self, reply: &mut WorkflowReply) {
        let (run, value) = match reply {
            WorkflowReply::Receipt { run, snapshot, .. }
            | WorkflowReply::Snapshot { run, snapshot, .. } => (*run, snapshot),
            _ => return,
        };
        let Some(path) = self.paths.get(&run) else {
            return;
        };
        let result = (|| {
            let frozen = read_frozen(path)?;
            let history = Journal::inspect(path)?;
            if !recovery::has_resume_barrier(&history) && value.get("reconciliation").is_none() {
                return Ok(None);
            }
            let mut current = assessment(&frozen, &history, &self.observations)?;
            current["stale"] = (acknowledgement(&history)
                .is_some_and(|a| a["assessment"] != current["fingerprint"]))
            .into();
            current["acknowledged"] = (recovery::has_resume_barrier(&history)
                && current["clear"] == true
                && acknowledgement(&history)
                    .is_some_and(|a| a["assessment"] == current["fingerprint"]))
            .into();
            current["required"] =
                (recovery::has_resume_barrier(&history) && current["acknowledged"] != true).into();
            Ok::<_, String>(Some(current))
        })();
        match result {
            Ok(None) => {}
            Ok(Some(current)) => {
                if value["recovery"].is_object() {
                    value["recovery"]["state"]["parent_attention_required"] =
                        current["required"].clone();
                }
                value["reconciliation"] = current;
            }
            Err(_) => {
                if value["recovery"].is_object() {
                    value["recovery"]["state"]["parent_attention_required"] = true.into();
                }
                value["reconciliation"] = serde_json::json!({"clear":false,"blockers":[{"code":"assessment_unknown"}],"ready":false,"continued":false});
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn setup(root: &Path) -> (Runs, OriginBinding, RunId, PathBuf) {
        std::fs::write(
            root.join("task.md"),
            "Accepted controlled reconciliation scope",
        )
        .unwrap();
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
                "Fixture",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let origin = OriginBinding {
            session: SessionId::new(),
            project: guild_session::ProjectId::new(),
            workspace: guild_session::WorkspaceId::new(),
            area: None,
            harness: guild_session::HarnessKind::Codex,
            conversation: Some("exact-retained-conversation".into()),
            cwd: root.into(),
        };
        let spec = StartRequest {
            version: 1,
            request_id: "fixture".into(),
            expected_revision: 0,
            origin: origin.session,
            target: root.into(),
            selection: Some("task.md".into()),
            definition: None,
            overrides: Some(serde_json::json!({"steps":[
            {"id":"contract","label":"Prepare","action":"contract.prepare","role":"coordinator","depends_on":[],"outcomes":["contract"]},
            {"id":"worker","label":"Unfinished","action":"session.assign","role":"builder","depends_on":["contract"],"outcomes":["build"]}
        ],"corrections":[],"outcomes":{"plan-review":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}})),
            task: "Free controlled reconciliation fixture, no model".into(),
            architecture_only: false,
            checks: vec![],
        };
        let mut store = Runs {
            entry_enabled: true,
            ..Runs::default()
        };
        let WorkflowReply::Receipt { run, .. } = store.execute(
            &WorkflowCommand::Start { spec },
            Some(origin.session),
            Some(origin.clone()),
            &[],
        ) else {
            panic!("start failed")
        };
        store.observations.write().unwrap().insert(
            origin.session.to_string(),
            Native {
                binding: origin.clone(),
                task: false,
                selection: None,
                permission: Some(guild_session::PermissionMode::Manual),
                status: "Working".into(),
                holders: vec![100],
                identity_qualified: true,
            },
        );
        let path = root.join(".guild/workflow-runs").join(run.to_string());
        let j = store.journals.get_mut(&run).unwrap();
        let (r, _) = j
            .reserve(
                &Mutation::new(
                    "controlled-stopped-resume",
                    j.revision(),
                    serde_json::json!({"operation":"origin_recovery","source":"fixture","handoff_intent":true,"binding":{"selection":null,"permission":"manual"}}),
                )
                .unwrap(),
            )
            .unwrap();
        j.resolve(r.attempt,r.revision,serde_json::json!({"operation":"recovery_observed","recovery":{"phase":"resume_launched","resume_attempted":true,"pid":100,"ready":false,"continued":false}})).unwrap();
        std::fs::create_dir(path.join("artifacts")).unwrap();
        let out = path.join("artifacts/contract.txt");
        std::fs::write(&out, "checked").unwrap();
        let contract = pin_contract(root, Some(Path::new("task.md")), None).unwrap();
        let evidence:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":origin.session,"contract":contract.sha256,"source":super::super::super::source::capture(root,None).unwrap(),"scenario":"contract","step":"contract","tool":{"program":"/bin/test","version":"controlled fixture","argv":["test","-s","task.md"]},"observed_at_ms":1,"artifacts":[{"path":out,"sha256":digest(b"checked")}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}})).unwrap();
        let revision = store.journals[&run].revision();
        let reply = store.execute(
            &WorkflowCommand::Checkpoint {
                run,
                spec: CheckpointRequest {
                    checks: None,
                    version: 1,
                    request_id: "contract-evidence".into(),
                    expected_revision: revision,
                    step: "contract".into(),
                    outcome: "contract".into(),
                    evidence: vec![evidence],
                },
            },
            Some(origin.session),
            Some(origin.clone()),
            &[],
        );
        assert!(matches!(reply, WorkflowReply::Receipt { .. }), "{reply:?}");
        assert!(
            store.reserve_assignment(run).unwrap().is_none(),
            "resume must gate the next unfinished effect"
        );
        (store, origin, run, path)
    }
    fn inspect(store: &mut Runs, origin: &OriginBinding, run: RunId) -> (u64, String) {
        let request = WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "read-current".into(),
                expected_revision: 0,
                assessment: None,
            },
        };
        let WorkflowReply::Snapshot {
            revision, snapshot, ..
        } = store.execute(&request, Some(origin.session), Some(origin.clone()), &[])
        else {
            panic!("assessment failed")
        };
        assert!(
            !store.take_effect_admission(),
            "assessment alone cannot authorize work"
        );
        (
            revision,
            snapshot["reconciliation"]["fingerprint"]
                .as_str()
                .unwrap()
                .into(),
        )
    }
    #[test]
    fn qualified_parent_ack_unblocks_only_unfinished_assignment_preserving_completed_receipts_and_usage()
     {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = setup(dir.path());
        let before = Journal::inspect(&path).unwrap();
        let before_model = reconstruct(&read_frozen(&path).unwrap(), &before).unwrap();
        let (revision, fingerprint) = inspect(&mut store, &origin, run);
        let command = WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "explicit-parent-ack".into(),
                expected_revision: revision,
                assessment: Some(fingerprint),
            },
        };
        let reply = store.execute(&command, Some(origin.session), Some(origin.clone()), &[]);
        assert!(matches!(reply, WorkflowReply::Receipt { .. }), "{reply:?}");
        let after = Journal::inspect(&path).unwrap();
        assert_eq!(
            &after.receipts[..before.receipts.len()],
            before.receipts.as_slice()
        );
        assert_eq!(
            reconstruct(&read_frozen(&path).unwrap(), &after)
                .unwrap()
                .review_ledger,
            before_model.review_ledger
        );
        let repeated = store.execute(&command, Some(origin.session), Some(origin.clone()), &[]);
        assert!(
            matches!(repeated, WorkflowReply::Receipt { .. }),
            "{repeated:?}"
        );
        assert_eq!(Journal::inspect(&path).unwrap().revision, after.revision);
        for status in ["Done", "Idle", "Working"] {
            store
                .observations
                .write()
                .unwrap()
                .get_mut(&origin.session.to_string())
                .unwrap()
                .status = status.into();
            let reply = store.execute(&WorkflowCommand::Show { version: 1, run }, None, None, &[]);
            let WorkflowReply::Snapshot { snapshot, .. } = reply else {
                panic!("{reply:?}")
            };
            assert_eq!(
                snapshot["reconciliation"]["acknowledged"], true,
                "parent {status} must not stale its acknowledgement"
            );
            assert_eq!(snapshot["reconciliation"]["stale"], false);
            assert_eq!(
                Journal::inspect(&path).unwrap().revision,
                after.revision,
                "activity alone writes no reconciliation/replay receipt"
            );
            check_native_permit(
                &permit(&Journal::inspect(&path).unwrap()).unwrap(),
                &store.observations.read().unwrap(),
            )
            .unwrap();
        }
        let original = store.observations.read().unwrap()[&origin.session.to_string()].clone();
        let native_permit = permit(&Journal::inspect(&path).unwrap()).unwrap();
        for change in 0..7 {
            let mut changed = original.clone();
            match change {
                0 => changed.status = "Waiting(Permission)".into(),
                1 => changed.status = "Waiting(Question)".into(),
                2 => changed.holders = vec![101],
                3 => changed.identity_qualified = false,
                4 => changed.binding.conversation = Some("another-conversation".into()),
                5 => changed.permission = None,
                _ => changed.task = true,
            }
            store
                .observations
                .write()
                .unwrap()
                .insert(origin.session.to_string(), changed);
            assert!(
                store.reserve_assignment(run).unwrap().is_none(),
                "changed native fact {change} must guard effect admission"
            );
            assert!(
                check_native_permit(&native_permit, &store.observations.read().unwrap()).is_err(),
                "changed native fact {change} must guard actual execution"
            );
        }
        store
            .observations
            .write()
            .unwrap()
            .insert(origin.session.to_string(), original.clone());
        std::fs::write(dir.path().join("after-ack.txt"), "changed source").unwrap();
        assert!(
            store.reserve_assignment(run).unwrap().is_none(),
            "source changes still invalidate the acknowledgement"
        );
        std::fs::remove_file(dir.path().join("after-ack.txt")).unwrap();
        let mut rows = store.observations.read().unwrap().clone();
        let mut child = original.clone();
        child.binding.session = SessionId::new();
        child.task = true;
        let child_id = child.binding.session.to_string();
        rows.insert(child_id.clone(), child.clone());
        let mut child_permit = native_permit.clone();
        child_permit["native"][&child_id] = serde_json::to_value(&child).unwrap();
        check_native_permit(&child_permit, &rows).unwrap();
        for status in ["Done", "Idle", "Waiting(Permission)"] {
            rows.get_mut(&child_id).unwrap().status = status.into();
            assert!(
                check_native_permit(&child_permit, &rows).is_err(),
                "child {status} must remain significant at execution"
            );
        }
        let legacy = serde_json::to_value(&*store.observations.read().unwrap()).unwrap();
        check_native_permit(&legacy, &store.observations.read().unwrap()).unwrap();
        let mut changed = store.observations.read().unwrap().clone();
        changed.get_mut(&origin.session.to_string()).unwrap().status = "Done".into();
        assert!(
            check_native_permit(&legacy, &changed).is_err(),
            "old map-only permits retain strict comparison"
        );
        // A later live reveal is navigation, not a new pause or a waiver of
        // old uncertainty. It must leave this qualified acknowledgement usable.
        let j = store.journals.get_mut(&run).unwrap();
        let (r, _) = j
            .reserve(
                &Mutation::new(
                    "live-reveal-after-ack",
                    j.revision(),
                    serde_json::json!({"operation":"origin_recovery"}),
                )
                .unwrap(),
            )
            .unwrap();
        j.resolve(r.attempt,r.revision,serde_json::json!({"operation":"recovery_observed","recovery":{"phase":"origin_live_reused","resume_attempted":false}})).unwrap();
        let next = store
            .reserve_assignment(run)
            .unwrap()
            .expect("actual next effect must be admitted");
        assert_eq!(next.plan.step, "worker");
        assert!(matches!(
            next.operation,
            super::super::assignment::Operation::Checkout
        ));
        assert_eq!(before_model.graph.nodes()[0].state, NodeState::Finished);
        assert!(
            store.reserve_assignment(run).is_err(),
            "unresolved reservation cannot allocate another child"
        );
    }
    #[test]
    fn assessment_never_approves_changed_source_waiting_permission_wrong_parent_or_unknown_usage() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = setup(dir.path());
        let (revision, hash) = inspect(&mut store, &origin, run);
        let command = WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "ack".into(),
                expected_revision: revision,
                assessment: Some(hash),
            },
        };
        let wrong = store.execute(&command, Some(SessionId::new()), Some(origin.clone()), &[]);
        assert!(matches!(wrong, WorkflowReply::Gap { .. }));
        store
            .observations
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .status = "Waiting(Permission)".into();
        let waiting = store.execute(
            &WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "read".into(),
                    expected_revision: 0,
                    assessment: None,
                },
            },
            Some(origin.session),
            Some(origin.clone()),
            &[],
        );
        let WorkflowReply::Snapshot { snapshot, .. } = waiting else {
            panic!("{waiting:?}")
        };
        assert!(
            snapshot["reconciliation"]["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "permission"
                    && b["session"] == serde_json::json!(origin.session))
        );
        assert!(matches!(
            store.execute(&command, Some(origin.session), Some(origin.clone()), &[]),
            WorkflowReply::Gap { .. }
        ));
        store
            .observations
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .status = "Working".into();
        std::fs::write(dir.path().join("changed.txt"), "changed while stopped").unwrap();
        assert!(matches!(
            store.execute(&command, Some(origin.session), Some(origin.clone()), &[]),
            WorkflowReply::Gap { .. }
        ));
        std::fs::remove_file(dir.path().join("changed.txt")).unwrap();
        let artifact = path.join("artifacts/contract.txt");
        std::fs::write(&artifact, "modified after assessment").unwrap();
        assert!(matches!(
            store.execute(&command, Some(origin.session), Some(origin.clone()), &[]),
            WorkflowReply::Gap { .. }
        ));
        std::fs::write(&artifact, "checked").unwrap();
        let mut changed_peer = origin.clone();
        changed_peer.conversation = Some("different-conversation".into());
        assert!(matches!(
            store.execute(&command, Some(origin.session), Some(changed_peer), &[]),
            WorkflowReply::Gap { .. }
        ));
        let wrong_hash = WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "wrong-receipt".into(),
                expected_revision: revision,
                assessment: Some(digest(b"stale receipt")),
            },
        };
        assert!(matches!(
            store.execute(&wrong_hash, Some(origin.session), Some(origin.clone()), &[]),
            WorkflowReply::Gap { .. }
        ));
        let frozen = read_frozen(&path).unwrap();
        let mut ledger = ReviewLedger::new(run, frozen.definition.limits.clone()).unwrap();
        ledger
            .reserve(
                "uncertain-fixture",
                &frozen.source.sha256,
                &digest(b"contract"),
                2,
                false,
            )
            .unwrap();
        ledger.uncertain("uncertain-fixture").unwrap();
        let j = store.journals.get_mut(&run).unwrap();
        let (r, _) = j
            .reserve(
                &Mutation::new(
                    "retained-usage",
                    j.revision(),
                    serde_json::json!({"operation":"usage_observation"}),
                )
                .unwrap(),
            )
            .unwrap();
        j.resolve(
            r.attempt,
            r.revision,
            serde_json::json!({"operation":"usage_observed","review_ledger":ledger}),
        )
        .unwrap();
        let (revision, hash) = inspect(&mut store, &origin, run);
        let denied = store.execute(
            &WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "uncertain-ack".into(),
                    expected_revision: revision,
                    assessment: Some(hash),
                },
            },
            Some(origin.session),
            Some(origin),
            &[],
        );
        assert!(matches!(denied, WorkflowReply::Gap { .. }));
        assert!(store.reserve_assignment(run).unwrap().is_none());
    }
    #[test]
    fn later_live_reveal_cannot_clear_an_older_unresolved_effect() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = setup(dir.path());
        let j = store.journals.get_mut(&run).unwrap();
        j.reserve(
            &Mutation::new(
                "unknown-effect",
                j.revision(),
                serde_json::json!({"operation":"origin_recovery"}),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            j.reserve(
                &Mutation::new(
                    "later-reveal",
                    j.revision(),
                    serde_json::json!({"operation":"origin_recovery"})
                )
                .unwrap()
            )
            .is_err(),
            "the serialized owner refuses a later reveal while an older effect is unknown"
        );
        let (revision, hash) = inspect(&mut store, &origin, run);
        let reply = store.execute(
            &WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "cannot-waive".into(),
                    expected_revision: revision,
                    assessment: Some(hash),
                },
            },
            Some(origin.session),
            Some(origin),
            &[],
        );
        assert!(matches!(reply, WorkflowReply::Gap { .. }));
        assert!(recovery::requires_reconciliation(
            &Journal::inspect(&path).unwrap()
        ));
        assert!(store.reserve_assignment(run).is_err());
    }
    #[test]
    fn orphan_private_owner_receipt_blocks_actual_parent_ack_before_any_review_launch() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = setup(dir.path());
        let root = path.join("reviews/interrupted-fixture/control");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("environment-lease.json"),
            b"{\"unknown_owner\":true}",
        )
        .unwrap();
        let (revision, hash) = inspect(&mut store, &origin, run);
        let reply = store.execute(
            &WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "unknown-resource".into(),
                    expected_revision: revision,
                    assessment: Some(hash),
                },
            },
            Some(origin.session),
            Some(origin),
            &[],
        );
        assert!(matches!(reply, WorkflowReply::Gap { .. }));
        assert_eq!(Journal::inspect(&path).unwrap().revision, revision);
        assert!(store.reserve_assignment(run).unwrap().is_none());
    }
    #[test]
    fn native_change_after_ack_invalidates_admission_without_new_receipts_or_budget() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = setup(dir.path());
        let (revision, hash) = inspect(&mut store, &origin, run);
        let command = WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "parent-ack".into(),
                expected_revision: revision,
                assessment: Some(hash),
            },
        };
        assert!(matches!(
            store.execute(&command, Some(origin.session), Some(origin.clone()), &[]),
            WorkflowReply::Receipt { .. }
        ));
        let revision = Journal::inspect(&path).unwrap().revision;
        store
            .observations
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .holders = vec![101];
        assert!(store.reserve_assignment(run).unwrap().is_none());
        assert_eq!(Journal::inspect(&path).unwrap().revision, revision);
    }
    #[test]
    fn parent_reconciliation_is_a_public_owning_command() {
        let request = serde_json::from_value::<WorkflowCommand>(serde_json::json!({
            "operation":"reconcile", "run":RunId::new(),
            "spec":{"version":1,"request_id":"qualified-parent","expected_revision":2,"assessment":null}
        }));
        assert!(
            request.is_ok(),
            "parent cannot inspect or acknowledge recovery: {request:?}"
        );
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    #[test]
    fn compound_startup_reservation_is_source_bound_and_crash_never_replays() {
        let root = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = super::tests::setup(root.path());
        let mut record = guild_session::SessionRecord::new(
            "Original".into(),
            guild_session::Seat {
                project: origin.project,
                workspace: origin.workspace,
            },
            origin.cwd.clone(),
            guild_session::Runs::Codex(guild_session::SessionMode::Terminal),
        );
        record.id = origin.session;
        record.harness_session = origin.conversation.clone();
        record.launch_permission = Some(guild_session::PermissionMode::Manual);
        store.recovery_records.push(record);
        let request = WorkflowCommand::Recover {
            run,
            spec: ControlRequest {
                version: 1,
                request_id: "compound-startup".into(),
                expected_revision: Journal::inspect(&path).unwrap().revision,
            },
        };
        let reply = store.execute(&request, Some(origin.session), Some(origin.clone()), &[]);
        assert!(matches!(reply, WorkflowReply::Receipt { .. }), "{reply:?}");
        let effect = store.recovery_effect.take().unwrap();
        let startup = effect
            .startup
            .as_ref()
            .expect("stopped recovery reserves current summary before launcher");
        assert!(startup.text.starts_with("Workflow recovery handoff"));
        store.startup_preflight(&effect).unwrap();
        let changed = root.path().join("changed-before-startup");
        std::fs::write(&changed, "new source").unwrap();
        assert!(store.startup_preflight(&effect).is_err());
        std::fs::remove_file(changed).unwrap();
        let history = Journal::inspect(&path).unwrap();
        assert!(
            handoff::uncertain(&history),
            "crashed compound reservation must retain uncertain startup"
        );
        assert!(store.prepare_handoff(run).unwrap().is_none());
        store
            .observe_recovery(
                &effect,
                serde_json::json!({"phase":"resume_launched","resume_attempted":true,"pid":100}),
            )
            .unwrap();
        assert_eq!(
            handoff::state(&Journal::inspect(&path).unwrap()).unwrap()["stage"],
            "submitted"
        );
        let observations = store.observations.clone();
        drop(store);
        let mut reopened = Runs {
            observations,
            ..Default::default()
        };
        reopened.paths.insert(run, path.clone());
        reopened.journals.insert(run, Journal::open(&path).unwrap());
        assert!(reopened.prepare_handoff(run).unwrap().is_none());
        reopened.settle_startup(&effect, false).unwrap();
        let j = reopened.journals.get_mut(&run).unwrap();
        let (r, _) = j
            .reserve(
                &Mutation::new(
                    "later-live",
                    j.revision(),
                    serde_json::json!({"operation":"origin_recovery"}),
                )
                .unwrap(),
            )
            .unwrap();
        j.resolve(
            r.attempt,
            r.revision,
            serde_json::json!({"recovery":{"phase":"origin_live_reused","resume_attempted":false}}),
        )
        .unwrap();
        let state = handoff::state(&Journal::inspect(&path).unwrap()).unwrap();
        assert_eq!(state["stage"], "unknown");
        assert_eq!(state["path"], "startup");
        assert!(
            reopened.prepare_handoff(run).unwrap().is_none(),
            "unknown startup cannot race fallback"
        );
    }
    #[test]
    fn reserved_handoff_rechecks_source_generation_and_never_replays_uncertain_input() {
        let root = tempfile::tempdir().unwrap();
        let (mut store, origin, run, path) = super::tests::setup(root.path());
        let native = {
            let mut rows = store.observations.write().unwrap();
            let n = rows.get_mut(&origin.session.to_string()).unwrap();
            n.status = "Done".into();
            n.clone()
        };
        let prompt = handoff::Prompt {
            native,
            generation: "actual-owned-view-1".into(),
            clean: true,
            draft: false,
            turned: true,
            transport: true,
            observed: "done".into(),
            covered: false,
            pending: false,
        };
        store
            .prompts
            .write()
            .unwrap()
            .insert(origin.session.to_string(), prompt.clone());
        // A draft is independently significant even if completion/clean flags remain set.
        store
            .prompts
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .draft = true;
        let before = Journal::inspect(&path).unwrap().revision;
        for _ in 0..2 {
            assert!(
                store.prepare_handoff(run).unwrap().is_none(),
                "known draft admitted input"
            );
            let reply = store.execute(
                &WorkflowCommand::Status { version: 1, run },
                Some(origin.session),
                Some(origin.clone()),
                &[],
            );
            let WorkflowReply::Snapshot { snapshot, .. } = reply else {
                panic!("{reply:?}")
            };
            assert_eq!(snapshot["handoff"]["reason"], "draft_input");
            assert_eq!(snapshot["handoff"]["stage"], "waiting");
        }
        assert_eq!(Journal::inspect(&path).unwrap().revision, before);
        store
            .prompts
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .draft = false;
        let effect = store
            .prepare_handoff(run)
            .unwrap()
            .expect("qualified prompt admits one summary");
        assert!(effect.text.contains("not an acknowledgement"));
        store.handoff_preflight(&effect).unwrap();
        let changed = root.path().join("changed-source.txt");
        std::fs::write(&changed, "changed during callback").unwrap();
        assert!(store.handoff_preflight(&effect).is_err());
        std::fs::remove_file(changed).unwrap();
        store
            .prompts
            .write()
            .unwrap()
            .get_mut(&origin.session.to_string())
            .unwrap()
            .generation = "replacement".into();
        assert!(store.handoff_preflight(&effect).is_err());
        store
            .prompts
            .write()
            .unwrap()
            .insert(origin.session.to_string(), prompt.clone());
        store.handoff_preflight(&effect).unwrap();
        assert!(
            store.prepare_handoff(run).unwrap().is_none(),
            "reservation alone cannot authorize a replay"
        );
        let history = Journal::inspect(&path).unwrap();
        assert_eq!(handoff::state(&history).unwrap()["stage"], "unknown");
        store
            .observe_handoff(&effect, "submitted", "controlled transport boundary")
            .unwrap();
        assert!(store.prepare_handoff(run).unwrap().is_none());
        assert!(
            assessment(
                &read_frozen(&path).unwrap(),
                &Journal::inspect(&path).unwrap(),
                &store.observations
            )
            .unwrap()["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "handoff_unknown")
        );
        let observations = store.observations.clone();
        let prompts = store.prompts.clone();
        drop(store);
        let mut reopened = Runs {
            observations,
            prompts,
            ..Default::default()
        };
        reopened.paths.insert(run, path.clone());
        reopened.journals.insert(run, Journal::open(&path).unwrap());
        assert!(
            reopened.prepare_handoff(run).unwrap().is_none(),
            "restart never replays submitted input without a turn witness"
        );
        reopened.settle_handoff(&effect, false).unwrap();
        assert!(
            reopened.prepare_handoff(run).unwrap().is_none(),
            "unknown outcome remains inspectable and cannot replay"
        );
        let journal = reopened.journals.get_mut(&run).unwrap();
        let (r,_)=journal.reserve(&Mutation::new("later-actual-resume",journal.revision(),serde_json::json!({"operation":"origin_recovery","handoff_intent":true,"binding":{"selection":null,"permission":"manual"}})).unwrap()).unwrap();
        journal.resolve(r.attempt,r.revision,serde_json::json!({"operation":"recovery_observed","recovery":{"phase":"resume_launched","resume_attempted":true,"pid":100}})).unwrap();
        assert!(
            reopened.prepare_handoff(run).unwrap().is_none(),
            "a later resume cannot erase a prior uncertain input"
        );
    }
}
