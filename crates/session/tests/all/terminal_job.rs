use guild_session::{ProjectId, SessionId, overlay::Target, terminal_job::*};

fn launch(owner: SessionId) -> LaunchSpec {
    serde_json::from_value(serde_json::json!({
        "version": 1, "request_id": "review-one", "program": "/bin/echo",
        "args": ["literal $HOME; `touch nope`", "a\nb"], "cwd": "/tmp",
        "target": {"kind": "session", "id": owner}
    }))
    .unwrap()
}

#[test]
fn same_request_reserves_one_job_and_changed_request_is_refused() {
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut book = Book::default();
    let spec = launch(owner);
    let first = book.reserve(owner, project, &spec).unwrap();
    let again = book.reserve(owner, project, &spec).unwrap();
    assert_eq!(first.job_id(), again.job_id());
    assert!(matches!(again, Reservation::Existing(_)));
    assert_eq!(book.receipts().len(), 1);
    let mut changed = spec.clone();
    changed.args.push("different".into());
    assert!(book.reserve(owner, project, &changed).is_err());
    changed = spec.clone();
    changed.env.insert("TOKEN".into(), "other".into());
    assert!(book.reserve(owner, project, &changed).is_err());
    assert!(book.reserve(owner, ProjectId::new(), &spec).is_err());
}

#[test]
fn owners_do_not_share_request_ids() {
    let mut book = Book::default();
    let project = ProjectId::new();
    let a = SessionId::new();
    let b = SessionId::new();
    let first = book.reserve(a, project, &launch(a)).unwrap();
    let second = book.reserve(b, project, &launch(b)).unwrap();
    assert_ne!(first.job_id(), second.job_id());
    assert_eq!(book.get(first.job_id()).unwrap().owner, a);
}

#[test]
fn invalid_launch_never_reserves() {
    let owner = SessionId::new();
    let good = serde_json::to_value(launch(owner)).unwrap();
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("program", serde_json::json!("")),
        ("request_id", serde_json::json!("")),
        ("cwd", serde_json::json!("relative")),
        ("args", serde_json::json!(["bad\u{0000}arg"])),
        ("env", serde_json::json!({"BAD=KEY":"value"})),
        ("env", serde_json::json!({"GUILD_SESSION":"impostor"})),
        (
            "geometry",
            serde_json::json!({"width_percent":0,"height_percent":80}),
        ),
    ] {
        let mut value_copy = good.clone();
        value_copy[key] = value;
        let spec: LaunchSpec = serde_json::from_value(value_copy).unwrap();
        let mut book = Book::default();
        assert!(
            book.reserve(owner, ProjectId::new(), &spec).is_err(),
            "{key}"
        );
        assert!(book.receipts().is_empty());
    }
}

#[test]
fn receipts_do_not_serialize_environment_or_arguments() {
    let owner = SessionId::new();
    let mut spec = launch(owner);
    spec.env
        .insert("PRIVATE_TOKEN".into(), "do-not-save-me".into());
    let mut book = Book::default();
    let id = book.reserve(owner, ProjectId::new(), &spec).unwrap();
    let serialized = serde_json::to_string(book.get(id.job_id()).unwrap()).unwrap();
    assert!(!serialized.contains("do-not-save-me"));
    assert!(!serialized.contains("PRIVATE_TOKEN"));
    assert!(!serialized.contains("touch nope"));
    assert_eq!(spec.args[0], "literal $HOME; `touch nope`");
    assert_eq!(spec.target, Target::Session(owner));
}

#[test]
fn terminal_states_cannot_be_reopened_and_restart_does_not_relaunch() {
    let owner = SessionId::new();
    let spec = launch(owner);
    let project = ProjectId::new();
    let mut book = Book::default();
    let id = *book.reserve(owner, project, &spec).unwrap().job_id();
    book.transition(&id, JobState::Running).unwrap();
    let receipts = book.receipts();
    let mut restored = Book::restore(receipts).unwrap();
    assert_eq!(restored.get(&id).unwrap().state, JobState::Interrupted);
    assert!(restored.reserve(owner, project, &spec).is_err());
    book.transition(&id, JobState::Exited).unwrap();
    assert!(book.transition(&id, JobState::Running).is_err());
    assert!(book.get(&id).unwrap().state.is_terminal());
}

#[test]
fn terminal_wire_and_literal_overlays_survive_native_review_removal() {
    use guild_session::{
        command::{Answer, Command},
        overlay::Content,
    };
    let owner = SessionId::new();
    let request = serde_json::json!({"command":"terminal", "caller":owner,
        "request":{"operation":"start", "spec":launch(owner)}});
    let decoded: Command = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), request);
    let reply = serde_json::json!({"answer":"terminal", "reply":{
        "kind":"capabilities", "version":1, "operations":["start","status","show","hide","close","cancel"]}});
    let decoded: Answer = serde_json::from_value(reply.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), reply);
    let old = serde_json::json!({"kind":"text","text":"$(do not execute)"});
    let content: Content = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(content).unwrap(), old);
    assert!(
        serde_json::from_value::<Content>(
            serde_json::json!({"kind":"review","project":"p","task":"t"})
        )
        .is_err(),
        "native review overlays are no longer an executable content channel"
    );
}

#[test]
fn chronology_survives_reload_with_unknown_times_last_and_stable_ties() {
    let owner = SessionId::new();
    let mut book = Book::default();
    let mut records = Vec::new();
    for (index, started) in [None, Some(2000_u64), Some(1000), Some(2000)]
        .into_iter()
        .enumerate()
    {
        let mut spec = launch(owner);
        spec.request_id = format!("job-{index}");
        let id = *book
            .reserve(owner, ProjectId::new(), &spec)
            .unwrap()
            .job_id();
        let mut record = serde_json::to_value(book.get(&id).unwrap()).unwrap();
        record["id"] = serde_json::json!(uuid::Uuid::from_u128(index as u128 + 1));
        if let Some(started) = started {
            record["started_at_ms"] = serde_json::json!(started);
        } else {
            record.as_object_mut().unwrap().remove("started_at_ms");
        }
        records.push(serde_json::from_value(record).unwrap());
    }
    let loaded = Book::restore(records).unwrap();
    let ordered = loaded.receipts();
    assert_eq!(
        ordered
            .iter()
            .map(|r| r.request_id.as_str())
            .collect::<Vec<_>>(),
        ["job-1", "job-3", "job-2", "job-0"]
    );
    let old = serde_json::to_value(&ordered[3]).unwrap();
    assert!(old["started_at_ms"].is_null());
}

#[test]
fn reservation_records_time_once_without_backfilling_legacy_receipts() {
    let owner = SessionId::new();
    let project = ProjectId::new();
    let spec = launch(owner);
    let mut book = Book::default();
    let id = *book.reserve(owner, project, &spec).unwrap().job_id();
    let first = serde_json::to_value(book.get(&id).unwrap()).unwrap();
    assert!(first["started_at_ms"].as_u64().is_some());
    std::thread::sleep(std::time::Duration::from_millis(2));
    book.reserve(owner, project, &spec).unwrap();
    assert_eq!(
        serde_json::to_value(book.get(&id).unwrap()).unwrap()["started_at_ms"],
        first["started_at_ms"]
    );
    let mut legacy = first;
    legacy.as_object_mut().unwrap().remove("started_at_ms");
    legacy.as_object_mut().unwrap().remove("output_deleted");
    let restored = Book::restore(vec![serde_json::from_value(legacy).unwrap()]).unwrap();
    let result = serde_json::to_value(&restored.receipts()[0]).unwrap();
    assert!(result["started_at_ms"].is_null());
    assert_eq!(result["output_deleted"], false);
}

#[test]
fn archived_compatible_view_survives_restart_without_replaying_review() {
    for program in [
        "/opt/review tools/custom-viewer",
        "/usr/local/bin/revmux",
        "/usr/local/bin/revust",
        "/tmp/bin/renamed-review$(literal)",
    ] {
        let owner = SessionId::new();
        let mut value = serde_json::to_value(launch(owner)).unwrap();
        value["revmux_archive"] = serde_json::json!({
            "directory": "/tmp/review archive/$(not-a-command)",
            "program": program
        });
        let mut spec: LaunchSpec = serde_json::from_value(value).unwrap();
        spec.env
            .insert("REVIEW_ONLY".into(), "must-not-replay".into());
        let mut book = Book::default();
        let id = *book
            .reserve(owner, ProjectId::new(), &spec)
            .unwrap()
            .job_id();
        book.transition(&id, JobState::Running).unwrap();
        if program != "/opt/review tools/custom-viewer" {
            book.transition(&id, JobState::Exited).unwrap();
        }
        let encoded = serde_json::to_string(&book.receipts()).unwrap();
        let restored = Book::restore(serde_json::from_str(&encoded).unwrap()).unwrap();
        let receipt = restored.get(&id).unwrap();
        assert_eq!(
            receipt.state,
            if program == "/opt/review tools/custom-viewer" {
                JobState::Interrupted
            } else {
                JobState::Exited
            }
        );
        assert!(!encoded.contains("must-not-replay"));
        let view = receipt.archive_view(Target::Session(owner)).unwrap();
        assert_eq!(view.program, program);
        assert_eq!(
            view.args,
            ["view", "--archive", "/tmp/review archive/$(not-a-command)"]
        );
        assert_eq!(view.cwd, spec.revmux_archive.as_ref().unwrap().directory);
        assert_eq!(view.target, Target::Session(owner));
        assert!(view.validate().is_ok());
        assert!(view.env.is_empty());
        assert!(view.revmux_archive.is_none());
        assert_eq!(restored.get(&id).unwrap(), receipt);
    }
}

#[test]
fn archived_view_requires_completed_job_and_valid_absolute_paths() {
    let owner = SessionId::new();
    let mut book = Book::default();
    let id = *book
        .reserve(owner, ProjectId::new(), &launch(owner))
        .unwrap()
        .job_id();
    assert!(
        book.get(&id)
            .unwrap()
            .archive_view(Target::Session(owner))
            .is_err()
    );
    for directory in ["relative", "/tmp/bad\0path"] {
        let mut value = serde_json::to_value(launch(owner)).unwrap();
        value["revmux_archive"] =
            serde_json::json!({"directory":directory,"program":"/bin/revmux"});
        let spec: LaunchSpec = serde_json::from_value(value).unwrap();
        assert!(spec.validate().is_err());
    }
}

#[test]
fn restored_history_keeps_original_conversation_context_separate_from_owner() {
    let owner = SessionId::new();
    let conversation = SessionId::new();
    let mut book = Book::default();
    let id = *book
        .reserve(owner, ProjectId::new(), &launch(owner))
        .unwrap()
        .job_id();
    let mut saved = serde_json::to_value(book.get(&id).unwrap()).unwrap();
    saved["launch_context"] = serde_json::json!({
        "conversation": conversation, "session_label": "Original conversation",
        "root_path": "/roots/original", "root_label": "original",
        "repository": "/repositories/original/.git", "repository_label": "original",
        "branch": "before-switch", "program_cwd": "/program/subdirectory"
    });
    let restored = Book::restore(vec![serde_json::from_value(saved.clone()).unwrap()]).unwrap();
    let actual = serde_json::to_value(restored.get(&id).unwrap()).unwrap();
    assert_eq!(
        actual["launch_context"], saved["launch_context"],
        "restart must retain original conversation and context, even after its owner/root disappear"
    );
    assert_eq!(actual["owner"], serde_json::json!(owner));
}

fn context(conversation: SessionId, repository: Option<&str>) -> LaunchContext {
    LaunchContext {
        conversation: Some(conversation),
        session_label: Some("same name".into()),
        root_path: Some("/roots/original".into()),
        root_label: Some("same name".into()),
        repository: repository.map(Into::into),
        repository_label: Some("same name".into()),
        branch: Some("main".into()),
        program_cwd: "/tmp".into(),
    }
}

#[test]
fn four_scopes_keep_conversations_roots_and_same_named_repositories_separate() {
    let (a, b) = (SessionId::new(), SessionId::new());
    let (root, sibling, foreign) = (ProjectId::new(), ProjectId::new(), ProjectId::new());
    let mut book = Book::default();
    for (index, conversation, project, repo) in [
        (0, a, root, "/one/.git"),
        (1, b, root, "/one/.git"),
        (2, a, sibling, "/one/.git"),
        (3, a, foreign, "/two/.git"),
    ] {
        let mut spec = launch(a);
        spec.request_id = index.to_string();
        book.reserve_in(a, project, &spec, Some(context(conversation, Some(repo))))
            .unwrap();
    }
    let ctx = HistoryContext {
        conversation: Some(b),
        project: Some(root),
        repositories: [(root, "/one/.git".into())].into(),
        ..Default::default()
    };
    let receipts = book.receipts();
    let count = |scope| {
        let g = ctx.select(scope, &receipts);
        g.ongoing.len() + g.completed.len()
    };
    assert_eq!(count(Scope::Session), 1);
    assert_eq!(count(Scope::Worktree), 2);
    assert_eq!(count(Scope::Project), 3);
    assert_eq!(count(Scope::All), 4);
}

#[test]
fn unknown_repository_enriches_from_exact_extant_root_without_rewriting_history() {
    let owner = SessionId::new();
    let root = ProjectId::new();
    let sibling = ProjectId::new();
    let mut book = Book::default();
    let id = *book
        .reserve_in(owner, sibling, &launch(owner), Some(context(owner, None)))
        .unwrap()
        .job_id();
    let mut legacy = launch(owner);
    legacy.request_id = "legacy".into();
    book.reserve(owner, sibling, &legacy).unwrap();
    let original = book.receipts();
    let mut ctx = HistoryContext {
        project: Some(root),
        ..Default::default()
    };
    assert!(ctx.select(Scope::Project, &original).ongoing.is_empty());
    ctx.repositories.insert(root, "/one/.git".into());
    assert!(ctx.select(Scope::Project, &original).ongoing.is_empty());
    ctx.repositories.insert(sibling, "/one/.git".into());
    assert_eq!(ctx.select(Scope::Project, &original).ongoing.len(), 2);
    assert_eq!(
        book.receipts(),
        original,
        "membership enrichment cannot relabel old receipts"
    );
    assert!(
        book.get(&id)
            .unwrap()
            .launch_context
            .as_ref()
            .unwrap()
            .repository
            .is_none()
    );
    ctx.repositories.remove(&sibling);
    assert!(ctx.select(Scope::Project, &original).ongoing.is_empty());
    assert_eq!(ctx.select(Scope::All, &original).ongoing.len(), 2);
}

#[test]
fn grouping_uses_original_times_unknown_last_and_stable_ids_then_moves_on_completion() {
    let owner = SessionId::new();
    let root = ProjectId::new();
    let mut book = Book::default();
    let mut ids = Vec::new();
    for (index, time, state) in [
        (1, None, JobState::Starting),
        (2, Some(20), JobState::Running),
        (3, Some(20), JobState::Cancelling),
        (4, Some(30), JobState::Exited),
        (5, None, JobState::Interrupted),
        (6, Some(10), JobState::FailedToStart),
    ] {
        let mut spec = launch(owner);
        spec.request_id = index.to_string();
        let id = *book.reserve(owner, root, &spec).unwrap().job_id();
        let r = book.get_mut(&id).unwrap();
        r.started_at_ms = time;
        r.state = state;
        r.id = uuid::Uuid::from_u128(index);
        ids.push(id);
    }
    let ctx = HistoryContext::default();
    let mut receipts = book.receipts();
    receipts.reverse(); // Selection also promises ordering for an unordered input slice.
    let groups = ctx.select(Scope::All, &receipts);
    let names = |rows: Vec<&JobReceipt>| {
        rows.iter()
            .map(|r| r.request_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(groups.ongoing), ["2", "3", "1"]);
    assert_eq!(names(groups.completed), ["4", "6", "5"]);
    book.get_mut(&ids[1]).unwrap().state = JobState::Exited;
    let mut receipts = book.receipts();
    receipts.reverse(); // Selection also promises ordering for an unordered input slice.
    let groups = ctx.select(Scope::All, &receipts);
    assert_eq!(names(groups.ongoing), ["3", "1"]);
    assert_eq!(names(groups.completed), ["4", "2", "6", "5"]);
}

#[test]
fn reservation_context_is_immutable_on_retry_reattachment_and_restart() {
    let (owner, resumed) = (SessionId::new(), SessionId::new());
    let root = ProjectId::new();
    let mut book = Book::default();
    let spec = launch(owner);
    let id = *book
        .reserve_in(owner, root, &spec, Some(context(owner, Some("/one/.git"))))
        .unwrap()
        .job_id();
    let first = book.get(&id).unwrap().clone();
    let mut changed = context(resumed, Some("/two/.git"));
    changed.branch = Some("new".into());
    assert_eq!(
        *book
            .reserve_in(owner, root, &spec, Some(changed))
            .unwrap()
            .job_id(),
        id
    );
    assert_eq!(book.get(&id).unwrap(), &first);
    let r = book.get_mut(&id).unwrap();
    r.target = Some(Target::Session(resumed));
    let saved = serde_json::to_string(&book.receipts()).unwrap();
    let restored = Book::restore(serde_json::from_str(&saved).unwrap()).unwrap();
    let receipts = restored.receipts();
    assert_eq!(receipts[0].launch_context, first.launch_context);
    assert_eq!(receipts[0].owner, owner);
    let ctx = HistoryContext {
        conversation: Some(resumed),
        ..Default::default()
    };
    assert!(ctx.select(Scope::Session, &receipts).completed.is_empty());
    let ctx = HistoryContext {
        conversation: Some(owner),
        ..Default::default()
    };
    assert_eq!(ctx.select(Scope::Session, &receipts).completed.len(), 1);
    let mut legacy = serde_json::to_value(first).unwrap();
    legacy.as_object_mut().unwrap().remove("launch_context");
    let legacy: JobReceipt = serde_json::from_value(legacy).unwrap();
    assert!(ctx.contains(Scope::Session, &legacy));
    assert!(
        !HistoryContext {
            conversation: Some(resumed),
            ..Default::default()
        }
        .contains(Scope::Session, &legacy)
    );
}

#[test]
fn archived_view_rejects_malformed_program_paths() {
    let owner = SessionId::new();
    for program in ["", "custom-viewer", "/", "/bin/..", "/bin/custom-viewer\0"] {
        let mut value = serde_json::to_value(launch(owner)).unwrap();
        value["revmux_archive"] = serde_json::json!({
            "directory": "/tmp/review", "program": program
        });
        let spec: LaunchSpec = serde_json::from_value(value).unwrap();
        assert!(spec.validate().is_err(), "accepted {program:?}");
    }
}

#[test]
fn linked_archive_receipts_require_the_original_owner_and_project() {
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut spec = launch(owner);
    spec.revmux_archive = Some(Box::new(RevmuxArchive {
        directory: "/tmp/archive".into(),
        program: "/opt/bin/custom-viewer".into(),
    }));
    let mut book = Book::default();
    let original = *book.reserve(owner, project, &spec).unwrap().job_id();
    book.transition(&original, JobState::Running).unwrap();
    book.transition(&original, JobState::Exited).unwrap();
    let before = book.get(&original).unwrap().clone();
    let viewer_spec = before.archive_view(Target::Session(owner)).unwrap();
    assert_eq!(viewer_spec.title, "custom-viewer (archive)");
    let viewer = *book.reserve(owner, project, &viewer_spec).unwrap().job_id();
    assert!(
        book.get(&viewer).unwrap().revmux_archive.is_none(),
        "legacy-compatible link without copied metadata"
    );
    assert_eq!(book.archive_source(&viewer), Some(&before));
    let restored = Book::restore(book.receipts()).unwrap();
    assert_eq!(
        restored.archive_source(&viewer).map(|r| r.id),
        Some(original)
    );
    book.get_mut(&viewer).unwrap().owner = SessionId::new();
    assert!(book.archive_source(&viewer).is_none());
    book.get_mut(&viewer).unwrap().owner = owner;
    book.get_mut(&viewer).unwrap().project = ProjectId::new();
    assert!(book.archive_source(&viewer).is_none());
    book.get_mut(&viewer).unwrap().project = project;
    book.get_mut(&viewer).unwrap().request_id =
        format!("archive-view-{}-{}", JobId::new_v4(), JobId::new_v4());
    assert!(book.archive_source(&viewer).is_none());
    assert_eq!(book.get(&original), Some(&before));
}

#[test]
fn completed_history_excludes_deleted_worktrees_without_losing_live_jobs_or_receipts() {
    let owner = SessionId::new();
    let root = ProjectId::new();
    let mut book = Book::default();
    for (request, state) in [
        ("completed", JobState::Exited),
        ("running", JobState::Running),
    ] {
        let mut spec = launch(owner);
        spec.request_id = request.into();
        let id = *book
            .reserve_in(owner, root, &spec, Some(context(owner, Some("/one/.git"))))
            .unwrap()
            .job_id();
        book.get_mut(&id).unwrap().state = state;
    }
    let receipts = book.receipts();
    let mut ctx = HistoryContext {
        conversation: Some(owner),
        project: Some(root),
        ..Default::default()
    };
    assert_eq!(ctx.select(Scope::All, &receipts).completed.len(), 1);
    ctx.worktrees.insert("/one/.git".into(), Default::default());
    for scope in Scope::ALL {
        let groups = ctx.select(scope, &receipts);
        assert!(
            groups.completed.is_empty(),
            "deleted worktree remains in {scope:?}"
        );
        assert_eq!(
            groups.ongoing.len(),
            1,
            "live controls must remain reachable"
        );
    }
    assert_eq!(
        book.receipts(),
        receipts,
        "filtering must preserve archive metadata"
    );
    ctx.worktrees
        .get_mut(std::path::Path::new("/one/.git"))
        .unwrap()
        .insert(
            receipts[0]
                .launch_context
                .as_ref()
                .unwrap()
                .root_path
                .clone()
                .unwrap(),
        );
    assert_eq!(ctx.select(Scope::All, &receipts).completed.len(), 1);
}

#[test]
fn deleted_worktree_selection_preserves_unknown_nested_and_reloaded_history() {
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut book = Book::default();
    let id = *book
        .reserve_in(
            owner,
            project,
            &launch(owner),
            Some(context(owner, Some("/one/.git"))),
        )
        .unwrap()
        .job_id();
    book.get_mut(&id).unwrap().state = JobState::Exited;
    book.get_mut(&id).unwrap().presentation = Presentation::Closed;
    book.get_mut(&id).unwrap().target = None;
    let encoded = serde_json::to_vec(&book.receipts()).unwrap();
    let restored = Book::restore(serde_json::from_slice(&encoded).unwrap()).unwrap();
    let original = restored.receipts();
    let mut ctx = HistoryContext {
        conversation: Some(owner),
        project: Some(project),
        worktrees: [("/one/.git".into(), ["/roots".into()].into())].into(),
        ..Default::default()
    };
    for scope in Scope::ALL {
        assert_eq!(
            ctx.select(scope, &original).completed.len(),
            1,
            "listed ancestor preserves subdirectory and ambiguous nested history"
        );
    }
    ctx.worktrees
        .insert("/one/.git".into(), ["/roots/orig".into()].into());
    assert!(
        ctx.select(Scope::All, &original).completed.is_empty(),
        "string prefixes are not checkout containment"
    );
    assert_eq!(serde_json::to_vec(&restored.receipts()).unwrap(), encoded);
    for missing in ["snapshot", "repository", "root", "legacy"] {
        let mut receipts = original.clone();
        let mut unknown = ctx.clone();
        match missing {
            "snapshot" => unknown.worktrees.clear(),
            "repository" => receipts[0].launch_context.as_mut().unwrap().repository = None,
            "root" => receipts[0].launch_context.as_mut().unwrap().root_path = None,
            "legacy" => receipts[0].launch_context = None,
            _ => unreachable!(),
        }
        for scope in Scope::ALL {
            assert_eq!(
                unknown.select(scope, &receipts).completed.len(),
                1,
                "missing {missing} in {scope:?}"
            );
        }
    }
    let mut receipts = original.clone();
    receipts[0].launch_context.as_mut().unwrap().repository = None;
    ctx.repositories.insert(project, "/one/.git".into());
    assert!(
        ctx.select(Scope::All, &receipts).completed.is_empty(),
        "exact original root discovery supplies repository identity"
    );
    for state in [JobState::Starting, JobState::Running, JobState::Cancelling] {
        receipts[0].state = state;
        assert_eq!(ctx.select(Scope::All, &receipts).ongoing.len(), 1);
    }
    for state in [
        JobState::Exited,
        JobState::FailedToStart,
        JobState::Interrupted,
    ] {
        receipts[0].state = state;
        assert!(ctx.select(Scope::All, &receipts).completed.is_empty());
    }
}

#[test]
fn history_keeps_unresolved_original_and_snapshot_paths_visible() {
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut book = Book::default();
    let mut original = context(owner, Some("/one/.git"));
    original.root_path = Some("/projects/../projects/extant".into());
    let id = *book
        .reserve_in(owner, project, &launch(owner), Some(original))
        .unwrap()
        .job_id();
    book.get_mut(&id).unwrap().state = JobState::Exited;
    let mut receipts = book.receipts();
    let mut ctx = HistoryContext {
        conversation: Some(owner),
        project: Some(project),
        worktrees: [("/one/.git".into(), ["/projects/extant".into()].into())].into(),
        ..Default::default()
    };
    let saved = serde_json::to_vec(&receipts).unwrap();
    for scope in Scope::ALL {
        assert_eq!(
            ctx.select(scope, &receipts).completed.len(),
            1,
            "raw original path may name the listed extant checkout in {scope:?}"
        );
    }
    assert_eq!(serde_json::to_vec(&receipts).unwrap(), saved);
    receipts[0].launch_context.as_mut().unwrap().root_path = Some("projects/extant".into());
    assert_eq!(
        ctx.select(Scope::All, &receipts).completed.len(),
        1,
        "relative original identity has no known base"
    );
    receipts[0].launch_context.as_mut().unwrap().root_path = Some("/projects/extant".into());
    for snapshot in ["/projects/../projects/extant", "projects/extant"] {
        ctx.worktrees.insert(
            "/one/.git".into(),
            [snapshot.into(), "/projects/other".into()].into(),
        );
        for scope in Scope::ALL {
            assert_eq!(
                ctx.select(scope, &receipts).completed.len(),
                1,
                "unresolved snapshot identity cannot prove absence in {scope:?}"
            );
        }
    }
    ctx.worktrees
        .insert("/one/.git".into(), ["/projects/other".into()].into());
    for scope in Scope::ALL {
        assert!(
            ctx.select(scope, &receipts).completed.is_empty(),
            "ordinary normalized deleted path still filters in {scope:?}"
        );
    }
}

#[test]
fn neutral_archive_contract_normalizes_legacy_retries_and_restart() {
    for completed in [false, true] {
        let owner = SessionId::new();
        let project = ProjectId::new();
        let mut wire = serde_json::to_value(launch(owner)).unwrap();
        let archive =
            serde_json::json!({"directory":"/tmp/saved round", "program":"/opt/tools/any-viewer"});
        wire["archive_viewer"] = archive.clone();
        wire["env"] = serde_json::json!({"PRIVATE_REVIEW":"never-replay"});
        let neutral: LaunchSpec = serde_json::from_value(wire.clone()).unwrap();
        wire.as_object_mut().unwrap().remove("archive_viewer");
        wire["revmux_archive"] = archive.clone();
        let legacy: LaunchSpec = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(neutral, legacy);
        let mut book = Book::default();
        let id = *book.reserve(owner, project, &legacy).unwrap().job_id();
        assert_eq!(
            book.reserve(owner, project, &neutral).unwrap(),
            Reservation::Existing(id)
        );
        wire["revmux_archive"]["program"] = serde_json::json!("/opt/tools/other-viewer");
        let changed: LaunchSpec = serde_json::from_value(wire).unwrap();
        assert!(book.reserve(owner, project, &changed).is_err());
        book.transition(&id, JobState::Running).unwrap();
        if completed {
            book.transition(&id, JobState::Exited).unwrap();
        }
        let saved = serde_json::to_value(book.get(&id).unwrap()).unwrap();
        assert_eq!(saved["archive_viewer"], archive);
        assert!(saved.get("revmux_archive").is_none());
        assert!(!saved.to_string().contains("never-replay"));
        for field in ["archive_viewer", "revmux_archive"] {
            let mut old = saved.clone();
            old.as_object_mut().unwrap().remove("archive_viewer");
            old[field] = archive.clone();
            let mut restored = Book::restore(vec![serde_json::from_value(old).unwrap()]).unwrap();
            let receipt = restored.get(&id).unwrap();
            assert_eq!(
                receipt.state,
                if completed {
                    JobState::Exited
                } else {
                    JobState::Interrupted
                }
            );
            let view = receipt.archive_view(Target::Session(owner)).unwrap();
            assert_eq!(view.program, "/opt/tools/any-viewer");
            assert_eq!(view.args, ["view", "--archive", "/tmp/saved round"]);
            assert!(view.env.is_empty());
            assert!(
                serde_json::to_value(&view)
                    .unwrap()
                    .get("archive_viewer")
                    .is_none()
            );
            assert!(
                restored.reserve(owner, project, &neutral).is_err(),
                "restart cannot prove original private invocation equality"
            );
            assert_eq!(restored.receipts().len(), 1);
        }
    }
}

#[test]
fn archive_wire_rejects_simultaneous_and_duplicate_fields_including_null() {
    let owner = SessionId::new();
    let mut book = Book::default();
    let id = *book
        .reserve(owner, ProjectId::new(), &launch(owner))
        .unwrap()
        .job_id();
    let archive = r#"{"directory":"/tmp/round","program":"/opt/viewer"}"#;
    for base in [
        serde_json::to_string(&launch(owner)).unwrap(),
        serde_json::to_string(book.get(&id).unwrap()).unwrap(),
    ] {
        for (first, second) in [
            ("archive_viewer", "revmux_archive"),
            ("revmux_archive", "archive_viewer"),
            ("archive_viewer", "archive_viewer"),
            ("revmux_archive", "revmux_archive"),
        ] {
            for (a, b) in [
                (archive, archive),
                ("null", archive),
                (archive, "null"),
                ("null", "null"),
                (archive, r#"{"directory":"/other","program":"/opt/other"}"#),
            ] {
                let raw = format!(
                    "{},\"{first}\":{a},\"{second}\":{b}}}",
                    &base[..base.len() - 1]
                );
                if base.contains("\"owner\"") {
                    assert!(serde_json::from_str::<JobReceipt>(&raw).is_err(), "{raw}");
                } else {
                    assert!(serde_json::from_str::<LaunchSpec>(&raw).is_err(), "{raw}");
                    let request = format!(
                        r#"{{"command":"terminal","caller":"{owner}","request":{{"operation":"start","spec":{raw}}}}}"#
                    );
                    assert!(
                        serde_json::from_str::<guild_session::command::Command>(&request).is_err(),
                        "{request}"
                    );
                }
            }
        }
    }
}
