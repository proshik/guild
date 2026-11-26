use guild_session::{SessionId, resume_handoff::*};

fn attempt() -> Attempt {
    Attempt {
        version: 1,
        harness: None,
        path: None,
        controller: None,
        session: SessionId::new(),
        request_id: "checkpoint".into(),
        conversation: uuid::Uuid::new_v4().to_string(),
        fingerprint: "a".repeat(64),
    }
}

#[test]
fn durable_reservation_precedes_one_attempt_and_retries_do_not_repeat_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let request = attempt();
    let mut ledger = Ledger::load(path.clone());
    let mut launches = 0;
    for _ in 0..2 {
        let (receipt, new) = ledger.reserve(&request).unwrap();
        if new {
            assert!(path.exists());
            let saved = Ledger::load(path.clone());
            assert_eq!(
                saved
                    .lookup(None, request.session, &request.request_id)
                    .unwrap()
                    .state,
                State::DeliveryUnknown
            );
            assert_eq!(receipt.state, State::Reserved);
            launches += 1;
            ledger.finish(&request, true).unwrap();
        }
    }
    assert_eq!(launches, 1);
    assert_eq!(
        ledger
            .lookup(None, request.session, &request.request_id)
            .unwrap()
            .state,
        State::Launched
    );
    let mut changed = request.clone();
    changed.fingerprint = "b".repeat(64);
    assert!(ledger.reserve(&changed).is_err());
    let mut changed = request.clone();
    changed.conversation = uuid::Uuid::new_v4().to_string();
    assert!(ledger.reserve(&changed).is_err());
    let mut loaded = Ledger::load(path);
    assert!(!loaded.reserve(&request).unwrap().1);
}

#[test]
fn crash_storage_failure_and_corruption_never_authorize_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let request = attempt();
    let mut ledger = Ledger::load(path.clone());
    ledger.reserve(&request).unwrap();
    let mut loaded = Ledger::load(path.clone());
    let (receipt, new) = loaded.reserve(&request).unwrap();
    assert!(!new);
    assert_eq!(receipt.state, State::DeliveryUnknown);
    std::fs::write(&path, "broken").unwrap();
    assert!(Ledger::load(path.clone()).reserve(&request).is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(Ledger::load(path).reserve(&request).is_err());
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "file").unwrap();
    assert!(
        Ledger::load(blocker.join("ledger"))
            .reserve(&request)
            .is_err()
    );
}

#[test]
fn scopes_are_stable_and_only_one_pending_launch_can_target_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::load(dir.path().join("ledger"));
    let request = attempt();
    ledger.reserve(&request).unwrap();
    let mut other = request.clone();
    other.controller = Some(SessionId::new());
    assert!(
        ledger
            .lookup(other.controller, other.session, &other.request_id)
            .is_err()
    );
    assert!(ledger.reserve(&other).is_err());
    ledger.finish(&request, false).unwrap();
    assert_eq!(
        ledger
            .lookup(None, request.session, &request.request_id)
            .unwrap()
            .state,
        State::FailedToStart
    );
    assert!(!ledger.reserve(&request).unwrap().1);
}

#[test]
fn continuation_text_and_ids_have_explicit_limits() {
    for text in ["", " ", "bad\0text"] {
        assert!(validate("id", text).is_err());
    }
    assert!(validate("id", &"x".repeat(MAX_PROMPT_BYTES + 1)).is_err());
    assert!(validate("", "continue").is_err());
    assert!(validate("bad\0id", "continue").is_err());
    assert!(validate("id", "--literal\nпродолжит $HOME; `no shell`").is_ok());
}

#[test]
fn external_operator_and_known_non_task_controllers_cannot_resume_user_or_live_tasks() {
    use guild_session::{Desk, Runs, SessionMode, SessionRecord};
    let dir = tempfile::tempdir().unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), dir.path());
    let mut task = SessionRecord::new(
        "task".into(),
        project,
        dir.path().into(),
        Runs::Codex(SessionMode::Terminal),
    );
    task.task = true;
    task.open = false;
    task.harness_session = Some(uuid::Uuid::new_v4().to_string());
    assert!(authorize_controller(&desk, None).is_ok());
    assert!(authorize_controller(&desk, Some(SessionId::new())).is_err());
    let controller = desk.add_session(SessionRecord::new(
        "operator".into(),
        project,
        dir.path().into(),
        Runs::Shell,
    ));
    assert!(authorize_controller(&desk, Some(controller)).is_ok());
    desk.mark_task(controller);
    assert!(authorize_controller(&desk, Some(controller)).is_err());
    assert!(admit_target(&task, false).is_ok());
    let mut relative = task.clone();
    relative.cwd = "relative".into();
    assert!(admit_target(&relative, false).is_err());
    assert!(admit_target(&task, true).is_err());
    task.open = true;
    assert!(admit_target(&task, false).is_err());
    task.open = false;
    task.task = false;
    assert!(admit_target(&task, false).is_err());
    task.task = true;
    task.harness_session = Some("guess".into());
    assert!(admit_target(&task, false).is_err());
    task.harness_session = None;
    assert!(admit_target(&task, false).is_err());
}

#[test]
fn isolated_continuation_store_failure() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "resume_handoff::missing_primary_with_backup_and_failed_outcome_writes_fail_closed",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "runs in isolated_continuation_store_failure to protect Store's global error queue"]
fn missing_primary_with_backup_and_failed_outcome_writes_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let request = attempt();
    let mut ledger = Ledger::load(parent.join("ledger"));
    ledger.reserve(&request).unwrap();
    std::fs::rename(&parent, dir.path().join("kept")).unwrap();
    std::fs::write(&parent, "not a directory").unwrap();
    assert!(ledger.finish(&request, true).is_err());
    assert!(ledger.reserve(&request).is_err());
    let missing = dir.path().join("missing");
    std::fs::write(dir.path().join("missing.bak"), "{}").unwrap();
    assert!(Ledger::load(missing).reserve(&request).is_err());
}

#[test]
fn duplicate_or_malformed_receipts_never_become_a_clean_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger");
    let request = attempt();
    let mut ledger = Ledger::load(path.clone());
    ledger.reserve(&request).unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut duplicate = original.clone();
    let receipt = duplicate["receipts"][0].clone();
    duplicate["receipts"].as_array_mut().unwrap().push(receipt);
    std::fs::write(&path, duplicate.to_string()).unwrap();
    assert!(Ledger::load(path.clone()).reserve(&request).is_err());
    let mut malformed = original;
    malformed["receipts"][0]["request"]["conversation"] = serde_json::json!("not-a-conversation");
    std::fs::write(&path, malformed.to_string()).unwrap();
    assert!(Ledger::load(path).reserve(&request).is_err());
}

#[test]
fn continuation_identity_matches_pre_retirement_json_digest() {
    let context = (
        "line\n\"\\$(x)Привет",
        true,
        "conversation",
        "project",
        "workspace",
        "/tmp/path with spaces",
    );
    assert_eq!(
        fingerprint(&context).unwrap(),
        "89ab302bc0a9ad915c4f9ffeb203ce310da9f2c0ac3eeb781a1e272c77e45deb"
    );
}

#[test]
fn saved_terminal_tasks_of_every_harness_admit_exact_startup_conversations() {
    use guild_session::{Desk, HarnessKind, Runs, SessionMode, SessionRecord};
    let dir = tempfile::tempdir().unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), dir.path());
    for kind in HarnessKind::ALL {
        let runs = match kind {
            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
        };
        let mut record = SessionRecord::new("task".into(), project, dir.path().into(), runs);
        record.task = true;
        record.open = false;
        let id = if kind == HarnessKind::OpenCode {
            "ses_savedExact".into()
        } else if kind == HarnessKind::Claude {
            record.id.to_string()
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        record.harness_session = Some(id.clone());
        assert_eq!(admit_target(&record, false).unwrap(), id, "{kind:?}");
        record.harness_session = Some("--last".into());
        assert!(target_conversation(&record).is_err());
    }
}

#[test]
fn uncertain_delivery_blocks_new_keys_and_other_controllers_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receipts");
    let first = attempt();
    let mut ledger = Ledger::load(path.clone());
    ledger.reserve(&first).unwrap();
    let mut recovered = Ledger::load(path);
    let mut replacement = first.clone();
    replacement.request_id = "different".into();
    replacement.controller = Some(SessionId::new());
    assert!(
        recovered.reserve(&replacement).is_err(),
        "a new key/controller must not replay uncertain input"
    );
}

#[test]
fn version_two_opencode_receipt_survives_recovery_without_replaying() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receipts");
    let base = attempt();
    let mut raw = serde_json::to_value(&base).unwrap();
    raw["version"] = 2.into();
    raw["harness"] = "open_code".into();
    raw["path"] = "startup".into();
    raw["conversation"] = "ses_exact".into();
    let request: Attempt =
        serde_json::from_value(raw).expect("v2 descriptor must name its harness and path");
    let mut ledger = Ledger::load(path.clone());
    ledger.reserve(&request).unwrap();
    let mut recovered = Ledger::load(path);
    assert_eq!(
        recovered.reserve(&request).unwrap().0.state,
        State::DeliveryUnknown
    );
    assert!(!recovered.reserve(&request).unwrap().1);
}

#[test]
fn claude_conversation_provenance_cannot_be_reassigned_by_a_saved_foreign_id() {
    use guild_session::{Desk, Runs, SessionMode, SessionRecord};
    let dir = tempfile::tempdir().unwrap();
    let mut desk = Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), dir.path());
    let mut record = SessionRecord::new(
        "task".into(),
        project,
        dir.path().into(),
        Runs::Claude(SessionMode::Terminal),
    );
    record.task = true;
    record.open = false;
    assert_eq!(target_conversation(&record).unwrap(), record.id.to_string());
    record.harness_session = Some(uuid::Uuid::new_v4().to_string());
    assert!(
        target_conversation(&record).is_err(),
        "a foreign conversation must not inherit this task's authority"
    );
}
