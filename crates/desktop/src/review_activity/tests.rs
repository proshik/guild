use super::*;
use serde_json::json;

fn archive() -> (tempfile::TempDir, Source) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("archive");
    std::fs::create_dir_all(dir.join("revust")).unwrap();
    std::fs::create_dir(dir.join("stages")).unwrap();
    std::fs::write(dir.join("manifest.json"), "").unwrap();
    std::fs::write(
        dir.join("revust/run.json"),
        r#"{"version":1,"identity":{"task":"task","run":"run","request":"review"}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("events.jsonl"), "").unwrap();
    let source = Source {
        id: uuid::Uuid::new_v4(),
        directory: dir,
        program: "/fixture/revust".into(),
        fresh: true,
        running: true,
        started: Some(0),
    };
    (temp, source)
}
fn finding(id: &str, severity: &str, verdict: &str) -> serde_json::Value {
    json!({"id":id,"severity":severity,"verdict":verdict,"title":"Keyboard input reaches the wrong pane","body":"Focus remains in the terminal after the popup opens.","file":"src/focus.rs","line":12})
}
fn checkpoint(source: &Source, stage: &str, findings: serde_json::Value) {
    std::fs::write(source.directory.join(format!("stages/{stage}.json")), json!({"scope":{"task":"","run":"","scope_path":""},"stats":{"started_at":"0001-01-01T00:00:00Z","finished_at":"0001-01-01T00:00:00Z"},"findings":findings}).to_string()).unwrap();
}
#[test]
fn real_blank_scope_checkpoint_inherits_pinned_identity_and_replaces_candidates() {
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    checkpoint(
        &s,
        "1-found",
        json!([finding("a", "major", ""), finding("b", "minor", "")]),
    );
    let first = engine.observe(&s).unwrap();
    assert_eq!(first.counts(), (1, 1, 0));
    assert!(!first.clean);
    assert!(!first.assessed);
    checkpoint(
        &s,
        "3-verified",
        json!([finding("b", "minor", "confirmed")]),
    );
    let next = engine.observe(&s).unwrap();
    assert_eq!(next.counts(), (0, 1, 0));
    assert!(next.assessed);
}
#[test]
fn restart_rejects_replaced_archive_and_missing_pin_never_clean() {
    let (temp, mut s) = archive();
    let path = temp.path().join("pins.json");
    let mut engine = Engine::new(path.clone());
    engine.observe(&s).unwrap();
    drop(engine);
    s.fresh = false;
    let mut restored = Engine::new(path.clone());
    assert!(restored.observe(&s).is_ok());
    let header = s.directory.join("revust/run.json");
    std::fs::rename(&header, s.directory.join("old-header")).unwrap();
    std::fs::write(
        &header,
        r#"{"version":1,"identity":{"task":"task","run":"other","request":"review"}}"#,
    )
    .unwrap();
    assert!(Engine::new(path).observe(&s).is_err());
}
#[test]
fn shell_progress_is_details_only_and_partial_tail_waits() {
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    std::fs::write(s.directory.join("events.jsonl"), "{\"kind\":\"stage\",\"stage\":\"find\"}\n{\"kind\":\"agent_progress\",\"text\":\"rg secrets /tmp\"}\n{\"kind\":").unwrap();
    let data = engine.observe(&s).unwrap();
    assert_eq!(data.phase, "find");
    assert!(data.reported.contains("rg secrets"));
    assert!(
        data.narration.is_empty(),
        "Tool-command progress must not become a semantic summary"
    );
    assert!(!data.clean);
}
fn final_report(s: &Source, outcome: serde_json::Value, items: serde_json::Value) {
    let stats = json!({"started_at":"2026-10-24T00:00:00Z","finished_at":"2026-10-24T00:01:00Z"});
    std::fs::write(s.directory.join("findings.json"),json!({"scope":{"task":"task","run":"run"},"outcome":outcome,"findings":items,"sources":{"expected":1,"reported":1,"degraded":[]},"stats":stats}).to_string()).unwrap();
    std::fs::write(s.directory.join("manifest.json"),json!({"task":"task","run":"run","outcome":outcome,"started_at":stats["started_at"],"finished_at":stats["finished_at"]}).to_string()).unwrap();
}
#[test]
fn final_clean_requires_explicit_complete_and_persisted_association() {
    let (temp, mut s) = archive();
    let path = temp.path().join("pins.json");
    let mut engine = Engine::new(path.clone());
    engine.observe(&s).unwrap();
    final_report(
        &s,
        json!({"state":"complete","policy_failures":[],"diagnostics":[]}),
        json!([]),
    );
    assert!(engine.observe(&s).unwrap().clean);
    s.fresh = false;
    s.running = false;
    assert!(Engine::new(path).observe(&s).unwrap().clean);
    let unpinned = Engine::new(temp.path().join("missing.json"))
        .observe(&s)
        .unwrap();
    assert!(unpinned.legacy);
    assert!(!unpinned.clean);
    final_report(
        &s,
        json!({"state":"failed","policy_failures":[],"diagnostics":["launch failed"]}),
        json!([]),
    );
    let failed = engine.observe(&s).unwrap();
    assert!(failed.incomplete);
    assert!(!failed.clean);
    assert_eq!(failed.counts(), (0, 0, 0));
}
#[test]
fn retained_findings_survive_restart_without_receipt_changes() {
    let (temp, mut s) = archive();
    let path = temp.path().join("pins.json");
    let mut engine = Engine::new(path.clone());
    engine.observe(&s).unwrap();
    final_report(
        &s,
        json!({"state":"complete","policy_failures":[],"diagnostics":[]}),
        json!([finding("a", "major", "confirmed")]),
    );
    assert_eq!(engine.observe(&s).unwrap().counts(), (1, 0, 0));
    s.fresh = false;
    s.running = false;
    let restored = Engine::new(path).observe(&s).unwrap();
    assert_eq!(restored.counts(), (1, 0, 0));
    assert!(restored.final_result);
    assert!(restored.assessed);
}
#[test]
fn duplicate_ids_unknown_severity_and_unverified_final_are_not_clean() {
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    engine.observe(&s).unwrap();
    checkpoint(
        &s,
        "1-found",
        json!([finding("a", "major", ""), finding("a", "minor", "")]),
    );
    assert!(engine.observe(&s).is_err());
    std::fs::write(
        s.directory.join("stages/1-found.json"),
        json!({"findings":[finding("a","surprise","")]}).to_string(),
    )
    .unwrap();
    assert_eq!(engine.observe(&s).unwrap().counts(), (0, 0, 1));
    final_report(
        &s,
        json!({"state":"partial","policy_failures":[],"diagnostics":[]}),
        json!([finding("a", "major", "unverified")]),
    );
    let data = engine.observe(&s).unwrap();
    assert!(!data.assessed);
    assert!(!data.clean);
    assert!(data.incomplete);
}
#[test]
fn unsafe_descendants_and_changed_generation_are_rejected() {
    use std::os::unix::fs::symlink;
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    engine.observe(&s).unwrap();
    let stream = s.directory.join("events.jsonl");
    std::fs::rename(&stream, s.directory.join("previous-events")).unwrap();
    symlink(s.directory.join("previous-events"), &stream).unwrap();
    assert!(engine.observe(&s).is_err());
    std::fs::remove_file(&stream).unwrap();
    std::fs::write(&stream, "").unwrap();
    assert!(engine.observe(&s).is_err());
}
#[test]
fn malformed_complete_record_and_truncation_never_promote_results() {
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    std::fs::write(
        s.directory.join("events.jsonl"),
        "{\"kind\":\"stage\",\"stage\":\"find\"}\n",
    )
    .unwrap();
    engine.observe(&s).unwrap();
    std::fs::write(s.directory.join("events.jsonl"), "").unwrap();
    assert!(engine.observe(&s).is_err());
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    std::fs::write(s.directory.join("events.jsonl"), "bad json\n").unwrap();
    assert!(engine.observe(&s).is_err());
}
#[test]
fn corrupted_pin_store_does_not_rebind_or_claim_clean() {
    let (temp, s) = archive();
    let path = temp.path().join("pins.json");
    std::fs::write(&path, "corrupt").unwrap();
    final_report(
        &s,
        json!({"state":"complete","policy_failures":[],"diagnostics":[]}),
        json!([]),
    );
    let snapshot = Engine::new(path.clone()).observe(&s).unwrap();
    assert!(snapshot.legacy);
    assert!(!snapshot.clean);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "corrupt");
}
#[test]
fn append_completes_tail_without_double_counting_and_drop_removes_candidates() {
    use std::io::Write;
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    let event =
        json!({"kind":"findings","agent":"a","findings":[finding("a","major","")]}).to_string();
    let split = event.len() / 2;
    std::fs::write(s.directory.join("events.jsonl"), &event[..split]).unwrap();
    assert_eq!(engine.observe(&s).unwrap().counts(), (0, 0, 0));
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(s.directory.join("events.jsonl"))
        .unwrap();
    writeln!(file, "{}", &event[split..]).unwrap();
    assert_eq!(engine.observe(&s).unwrap().counts(), (1, 0, 0));
    assert_eq!(engine.observe(&s).unwrap().counts(), (1, 0, 0));
    writeln!(
        file,
        "{}",
        json!({"kind":"dropped","findings":[finding("a","major","")]})
    )
    .unwrap();
    let data = engine.observe(&s).unwrap();
    assert_eq!(data.counts(), (0, 0, 0));
    assert_eq!(data.dropped, 1);
}
#[test]
fn oversize_artifacts_and_failed_pin_save_cannot_claim_clean() {
    let (temp, s) = archive();
    let path = temp.path().join("pins").join("pins.json");
    let mut engine = Engine::new(path.clone());
    // Refuse a real write without depending on Store's private scratch name.
    std::fs::write(temp.path().join("pins"), "not a directory").unwrap();
    assert!(engine.observe(&s).is_err());
    final_report(
        &s,
        json!({"state":"complete","policy_failures":[],"diagnostics":[]}),
        json!([]),
    );
    let data = engine.observe(&s).unwrap();
    assert!(data.legacy);
    assert!(!data.clean);
    let (temp, s) = archive();
    let mut engine = Engine::new(temp.path().join("pins.json"));
    std::fs::OpenOptions::new()
        .write(true)
        .open(s.directory.join("events.jsonl"))
        .unwrap()
        .set_len(STREAM_LIMIT + 1)
        .unwrap();
    assert!(engine.observe(&s).is_err());
}

#[test]
#[ignore = "explicit read-only installed archive qualification only"]
fn selected_saved_archive_projects_without_mutation() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("GUILD_REVIEW_ARCHIVE_FIXTURE").expect("Select saved archive explicitly"),
    );
    let temp = tempfile::tempdir().unwrap();
    let s = Source {
        id: uuid::Uuid::new_v4(),
        directory,
        program: "/usr/local/bin/revust".into(),
        fresh: false,
        running: false,
        started: None,
    };
    let names = ["manifest.json", "findings.json", "events.jsonl"];
    let before: Vec<_> = names
        .iter()
        .map(|name| std::fs::read(s.directory.join(name)).unwrap())
        .collect();
    let data = Engine::new(temp.path().join("pins.json"))
        .observe(&s)
        .unwrap();
    for (name, bytes) in names.iter().zip(before) {
        assert_eq!(
            std::fs::read(s.directory.join(name)).unwrap(),
            bytes,
            "Saved archive must remain unchanged"
        );
    }
    assert!(data.final_result);
    assert!(data.legacy);
    assert!(!data.clean);
    println!(
        "saved projection: final={} legacy={} counts={:?}",
        data.final_result,
        data.legacy,
        data.counts()
    );
}

#[test]
fn live_reviews_remain_responsive_while_retained_history_advances_fairly() {
    let (_temp, s) = archive();
    let mut sources = vec![s];
    for _ in 0..40 {
        let mut saved = sources[0].clone();
        saved.id = uuid::Uuid::new_v4();
        saved.running = false;
        sources.push(saved);
    }
    let live = sources[0].id;
    let mut cursor = [0, 0];
    let mut observed = std::collections::HashSet::new();
    for _ in 0..4 {
        let batch = observation_batch(sources.clone(), &mut cursor);
        assert!(batch.len() <= 16);
        assert!(batch.iter().any(|s| s.id == live));
        observed.extend(batch.into_iter().map(|s| s.id));
    }
    assert_eq!(
        observed.len(),
        sources.len(),
        "History must not starve behind current reviews"
    );
}

#[test]
fn authentic_stopped_outcomes_keep_final_findings_after_restart() {
    // revust runtime::pipeline::State::Stopped(runtime::StopReason), serde snake_case.
    for reason in ["cancelled", "deadline", "call_budget"] {
        let (temp, mut s) = archive();
        let pins = temp.path().join("pins.json");
        let mut engine = Engine::new(pins.clone());
        engine.observe(&s).unwrap();
        final_report(
            &s,
            json!({"state":{"stopped":reason},"policy_failures":[],"diagnostics":[]}),
            json!([finding("kept", "major", "unverified")]),
        );
        let data = engine
            .observe(&s)
            .expect("Authentic stopped enum must deserialize");
        assert!(data.final_result);
        assert!(data.incomplete);
        assert!(!data.clean);
        assert_eq!(data.counts(), (1, 0, 0));
        s.fresh = false;
        s.running = false;
        let restored = Engine::new(pins).observe(&s).unwrap();
        assert!(restored.final_result && restored.incomplete && !restored.clean);
        assert_eq!(restored.counts(), (1, 0, 0));
    }
}
#[test]
fn category_records_remain_available_without_actionable_counts() {
    let report:Report=serde_json::from_value(json!({"findings":[finding("reject","major","rejected")],"open_questions":[finding("question","minor","")],"pre_existing":[finding("old","major","pre_existing")],"immaterial":[finding("small","minor","immaterial")]})).unwrap();
    let mut snapshot = Snapshot::default();
    apply(&mut snapshot, report).unwrap();
    assert_eq!(snapshot.counts(), (0, 0, 0));
    assert_eq!(snapshot.categorized.len(), 4);
    for category in [
        Category::Rejected,
        Category::OpenQuestion,
        Category::PreExisting,
        Category::Immaterial,
    ] {
        let (_, record) = snapshot
            .categorized
            .iter()
            .find(|(c, _)| *c == category)
            .unwrap();
        assert!(!record.title.is_empty() && !record.body.is_empty() && !record.file.is_empty());
        assert_eq!(record.line, 12);
    }
}
#[test]
fn category_records_obey_identity_and_text_limits() {
    for name in ["open_questions", "pre_existing", "immaterial"] {
        let mut invalid = finding("", "major", "");
        invalid["body"] = json!("x".repeat(RECORD_LIMIT + 1));
        let report: Report = serde_json::from_value(json!({name:[invalid]})).unwrap();
        assert!(
            apply(&mut Snapshot::default(), report).is_err(),
            "{name} must validate records"
        );
        let report: Report = serde_json::from_value(
            json!({name:[finding("same","major",""),finding("same","minor","")]}),
        )
        .unwrap();
        assert!(
            apply(&mut Snapshot::default(), report).is_err(),
            "{name} must reject ambiguous IDs"
        );
    }
}
#[test]
fn deleting_observed_verified_checkpoint_or_directory_never_revives_candidate() {
    for directory in [false, true] {
        let (temp, s) = archive();
        let mut engine = Engine::new(temp.path().join("pins.json"));
        checkpoint(&s, "1-found", json!([finding("rejected", "major", "")]));
        engine.observe(&s).unwrap();
        checkpoint(&s, "3-verified", json!([]));
        let verified = engine.observe(&s).unwrap();
        assert_eq!(verified.counts(), (0, 0, 0));
        assert!(verified.assessed);
        if directory {
            std::fs::remove_dir_all(s.directory.join("stages")).unwrap();
        } else {
            std::fs::remove_file(s.directory.join("stages/3-verified.json")).unwrap();
        }
        assert!(
            engine.observe(&s).is_err(),
            "Previously accepted evidence cannot become never-created"
        );
        let shown = stale_snapshot(Some(&verified));
        assert!(shown.stale && shown.unavailable && !shown.clean);
        assert_eq!(shown.counts(), (0, 0, 0));
        assert!(shown.assessed);
    }
}

#[test]
fn admission_is_lazy_but_unchanged_receipts_still_observe_disk_and_drain_results() {
    use guild_session::{
        ProjectId, SessionId,
        terminal_job::{Book, LaunchSpec},
    };
    let (temp, source) = archive();
    checkpoint(&source, "1-found", json!([finding("a", "major", "")]));
    let owner = SessionId::new();
    let spec:LaunchSpec=serde_json::from_value(json!({"version":1,"request_id":"activity","program":"/bin/true","cwd":"/tmp","target":{"kind":"session","id":owner},"revmux_archive":{"directory":source.directory,"program":source.program}})).unwrap();
    let mut book = Book::default();
    let id = *book
        .reserve(owner, ProjectId::new(), &spec)
        .unwrap()
        .job_id();
    // The fixture archive intentionally predates reservation; pin its synthetic launch.
    book.get_mut(&id).unwrap().started_at_ms = Some(0);
    let receipts = book.receipt_snapshot();
    let mut activity = Activity::new(temp.path().join("pins.json"));
    activity.launched(id);
    let mut calls = 0;
    activity.poll(|| {
        calls += 1;
        receipts.clone()
    });
    for _ in 0..10 {
        activity.poll(|| {
            calls += 1;
            receipts.clone()
        });
    }
    assert_eq!(
        calls, 1,
        "clock/busy admission must precede metadata derivation"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while activity.busy {
        activity.poll(|| receipts.clone());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(activity.snapshots[&id].counts(), (1, 0, 0));
    checkpoint(
        &source,
        "1-found",
        json!([finding("a", "major", ""), finding("b", "minor", "")]),
    );
    activity.next = Instant::now();
    activity.poll(|| receipts.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut changed = false;
    while activity.busy {
        changed |= activity.poll(|| receipts.clone());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        changed,
        "worker results must drain independently of metadata revision"
    );
    assert_eq!(activity.snapshots[&id].counts(), (1, 1, 0));
}
