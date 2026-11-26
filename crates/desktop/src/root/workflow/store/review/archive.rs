//! Read-only installed archive qualification, distinct from observer/job exit.
use super::*;

pub(super) fn read(root: &Path, relative: &str, max: u64) -> Result<Vec<u8>, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("workflow.review archive {relative}: {e}"))?;
    if !path.starts_with(&root) {
        return Err("workflow.review: archive resource escaped retained root".into());
    }
    bytes(&path, max)
}
pub(in crate::root::workflow::store) fn checked(
    _frozen: &Frozen,
    _path: &Path,
    model: &Model,
    _step: &str,
    round: &RevustRound,
    job: &guild_session::terminal_job::JobReceipt,
) -> Result<serde_json::Value, String> {
    use guild_session::terminal_job::JobState;
    if !matches!(job.state, JobState::Running | JobState::Exited)
        || job.signal.is_some()
        || job.cancellation.is_some()
        || job.error.is_some()
    {
        return Err(
            "UNVERIFIED: cancelled/interrupted/unknown native job; archive is history only".into(),
        );
    }
    if job.state == JobState::Exited && !matches!(job.exit_code, Some(0 | 1)) {
        return Err("UNVERIFIED: native SDK job did not finish successfully".into());
    }
    round.verify()?;
    let source = crate::root::workflow::source::capture(
        &round.subject.source.target,
        round.subject.source.base.as_deref(),
    )?;
    if source != round.subject.source || model.source != source {
        return Err("UNVERIFIED: source changed during review; archive remains historical and cannot qualify current source".into());
    }
    let installed = round.package.inspect_archive(&round.archive)?;
    let report = parse_workflow_input(&read(&round.archive, "findings.json", 4 * 1024 * 1024)?)?;
    let manifest = parse_workflow_input(&read(&round.archive, "manifest.json", 4 * 1024 * 1024)?)?;
    if installed["outcome"] != report["outcome"]
        || installed["scope"] != report["scope"]
        || installed["sources"] != report["sources"]
        || installed["findings"] != report["findings"]
    {
        return Err("UNVERIFIED: installed saved reader and retained report disagree".into());
    }
    if manifest["task"] != round.subject.task
        || manifest["run"] != round.subject.round
        || report["scope"]["task"] != round.subject.task
        || report["scope"]["run"] != round.subject.round
        || manifest["finished_at"].as_str().is_none_or(str::is_empty)
        || manifest["profile"] != round.configuration.profile
        || manifest["outcome"] != report["outcome"]
        || report["outcome"]["state"] != "complete"
        || report["outcome"]["policy_failures"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
    {
        return Err(
            "UNVERIFIED: actual archive identity, durable outcome or policy completeness differs"
                .into(),
        );
    }
    let expected = round.configuration.selected_profile["roster"]
        .as_array()
        .ok_or("UNVERIFIED: frozen actual roster unavailable")?;
    let roster = manifest["roster"]
        .as_array()
        .ok_or("UNVERIFIED: actual archive roster unavailable")?;
    if expected.len() != roster.len()
        || expected.is_empty()
        || report["sources"]["expected"].as_u64() != Some(expected.len() as u64)
        || report["sources"]["reported"] != report["sources"]["expected"]
        || report["sources"]["degraded"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
    {
        return Err("UNVERIFIED: actual expected/reported/degraded roster is incomplete".into());
    }
    for (requested, actual) in expected.iter().zip(roster) {
        for field in ["name", "executor", "model", "effort", "lenses"] {
            if requested[field] != actual[field] {
                return Err(format!(
                    "UNVERIFIED: archived actual roster differs at {field}"
                ));
            }
        }
    }
    let agents = report["sources"]["agents"]
        .as_array()
        .ok_or("UNVERIFIED: mandatory source inventory missing")?;
    if agents.len() != expected.len()
        || agents.iter().any(|a| a["degraded"] == true)
        || manifest["degraded"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    {
        return Err("UNVERIFIED: actual mandatory source delivery degraded or absent".into());
    }
    for runner in expected {
        let source = agents
            .iter()
            .find(|a| a["name"] == runner["name"])
            .ok_or("UNVERIFIED: mandatory source absent")?;
        if source["executor"] != runner["executor"]
            || source["requested_model"] != runner["model"]
            || source["effort"] != runner["effort"]
            || source["lenses"] != runner["lenses"]
        {
            return Err("UNVERIFIED: mandatory source identity differs from frozen roster".into());
        }
    }
    let mut artifacts = Vec::new();
    for name in [
        "manifest.json",
        "findings.json",
        "report.md",
        "revust/events.jsonl",
    ] {
        let content = read(&round.archive, name, 16 * 1024 * 1024)?;
        artifacts.push(EvidenceArtifact {
            path: round.archive.join(name),
            sha256: digest(&content),
        });
    }
    let findings = report["findings"]
        .as_array()
        .ok_or("UNVERIFIED: mandatory findings list missing")?;
    if job.state == JobState::Exited && (job.exit_code == Some(0)) != findings.is_empty() {
        return Err("UNVERIFIED: job exit and archived findings disagree".into());
    }
    Ok(
        serde_json::json!({"classification":if !findings.is_empty(){"findings"}else if report["open_questions"].as_array().is_some_and(|q|!q.is_empty()) {"questions"} else {"clean"},"findings":findings,"open_questions":report["open_questions"],"sources":report["sources"],"outcome":report["outcome"],"roster":roster,"finished_at":manifest["finished_at"],"package_version":round.package.version,"package_sha256":round.package.sha256,"source":source,"contract":round.subject.contract,"artifacts":artifacts}),
    )
}
pub(super) fn qualify(
    frozen: &Frozen,
    path: &Path,
    step: &str,
    round: &RevustRound,
    model: &Model,
    checked: &serde_json::Value,
) -> Result<Vec<QualifiedOutcome>, String> {
    if checked["classification"] != "clean" {
        return Err(
            "FAIL: checked mandatory sources reported findings; same-session correction required"
                .into(),
        );
    }
    let source: SourceIdentity =
        serde_json::from_value(checked["source"].clone()).map_err(|e| e.to_string())?;
    let artifacts: Vec<EvidenceArtifact> =
        serde_json::from_value(checked["artifacts"].clone()).map_err(|e| e.to_string())?;
    let configured = frozen
        .definition
        .steps
        .iter()
        .find(|s| s.id == step)
        .ok_or("workflow.review: configured result step missing")?;
    let roots = [path.to_path_buf(), round.archive.clone()];
    let mut results = Vec::new();
    for outcome in &configured.outcomes {
        let declared: Vec<_> = model
            .checks
            .iter()
            .filter(|c| c.step == step && c.outcome == *outcome)
            .cloned()
            .collect();
        let requirements = if declared.is_empty() {
            vec![CheckRequirement {
                step: step.into(),
                outcome: outcome.clone(),
                scenario: outcome.clone(),
                surface: Surface::Review,
                expectation: Expectation::Observed,
            }]
        } else {
            declared
        };
        for requirement in requirements {
            if requirement.surface != Surface::Review {
                return Err("UNVERIFIED: accepted review check requires a different real observation surface".into());
            }
            let evidence = Evidence {
                id: uuid::Uuid::new_v4(),
                producer: frozen.origin.session,
                contract: round.subject.contract.contract.sha256.clone(),
                source: source.clone(),
                scenario: requirement.scenario.clone(),
                step: step.into(),
                tool: ToolObservation {
                    program: round.package.launcher().to_string_lossy().into_owned(),
                    version: round.package.version.clone(),
                    argv: round.args.clone(),
                },
                observed_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_millis()
                    .try_into()
                    .map_err(|_| "workflow.review: timestamp overflow")?,
                artifacts: artifacts.clone(),
                observation: Observation::Review {
                    task: round.subject.task.clone(),
                    round: round.subject.round.clone(),
                    archive: round.archive.clone(),
                },
            };
            results.push(QualifiedOutcome::check(
                step,
                outcome,
                evidence,
                &EvidenceContext {
                    producer: frozen.origin.session,
                    contract: &round.subject.contract.contract.sha256,
                    source: &source,
                    step,
                    scenario: &requirement.scenario,
                    surface: requirement.surface,
                    expectation: requirement.expectation,
                    allowed_roots: &roots,
                },
            )?);
        }
    }
    Ok(results)
}
