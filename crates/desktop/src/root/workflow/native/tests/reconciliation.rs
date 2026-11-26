//! Actual free-executable native recovery followed by public parent reconciliation.
use super::*;
pub(super) fn tick(entity: &gpui::Entity<Root>, cx: &mut gpui::TestAppContext) {
    entity.update(cx, |r, cx| {
        r.poll_workflows(cx);
        r.poll_git(cx);
        r.poll_status(cx);
        r.answer_settled(cx);
    });
}
pub(super) fn ask(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    request: WorkflowCommand,
    caller: SessionId,
) -> Answer {
    let (send, receive) = mpsc::channel();
    entity.update(cx, |r, cx| {
        r.command_workflow(&request, Some(&caller.to_string()), send, cx)
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        if let Ok(answer) = receive.try_recv() {
            return answer;
        }
        assert!(
            std::time::Instant::now() < until,
            "public reconciliation command did not settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn navigate(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    request: WorkflowCommand,
    caller: SessionId,
) -> Answer {
    let (send, receive) = mpsc::channel();
    entity.update(cx, |r, cx| {
        r.navigate_workflow_recovery(&request, caller, send, cx)
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        if let Ok(answer) = receive.try_recv() {
            return answer;
        }
        assert!(
            std::time::Instant::now() < until,
            "native navigation did not settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
pub(super) fn after_actual_resume(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    original: &StartRequest,
    repo: &std::path::Path,
    pinned: &PinnedContract,
) {
    run_after_resume(
        entity,
        cx,
        original,
        repo,
        pinned,
        guild_session::HarnessKind::Codex,
        false,
    );
}
pub(super) fn startup_cases(cx: &mut gpui::TestAppContext) {
    let witness = PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    for harness in guild_session::HarnessKind::ALL {
        let owned = witness.join(format!("startup-{harness:?}"));
        std::fs::create_dir(&owned).unwrap();
        let repo = owned.join("repository");
        std::fs::create_dir(&repo).unwrap();
        let worktrees = owned.join("worktrees");
        std::fs::create_dir(&worktrees).unwrap();
        std::fs::write(repo.join("task.md"), "Accepted startup fixture scope").unwrap();
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
                    .current_dir(&repo)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let pinned = pin_contract(&repo, Some(std::path::Path::new("task.md")), None).unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &repo);
        let entity = cx.new(|cx| {
            let mut root = Root::diagnostic(desk, owned.join("state"), cx);
            root.settings.development_workflows = true;
            root
        });
        entity.update(cx, |root, _| {
            root.attach_test_hooks(&owned);
            root.desk.set_repository(project, Some(repo.join(".git")));
            root.scoped_settings.identities.insert(
                project,
                crate::repository_settings::resolve_repository(&repo),
            );
            let key = root.repository_identity(project).unwrap().key.clone();
            root.settings
                .repository_settings
                .entry(key)
                .or_default()
                .worktree_directory = Some(worktrees);
            root.status_workers
                .insert(project, crate::root::git::StatusWorker::start(repo.clone()));
        });
        let spec = StartRequest {
            version: 1,
            request_id: "startup-fixture".into(),
            expected_revision: 0,
            origin: SessionId::new(),
            target: repo.clone(),
            selection: Some("task.md".into()),
            definition: None,
            overrides: None,
            task: "Free startup fixture".into(),
            architecture_only: false,
            checks: vec![],
        };
        // run_after_resume only needs the native project of this original record.
        let seed = entity.update(cx, |r, cx| {
            r.open_tab_with(project, Runs::Codex(SessionMode::Terminal), None, false, cx)
                .unwrap()
        });
        let spec = StartRequest {
            origin: seed,
            ..spec
        };
        run_after_resume(&entity, cx, &spec, &repo, &pinned, harness, true);
        drop(entity);
    }
}
fn run_after_resume(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    original: &StartRequest,
    repo: &std::path::Path,
    pinned: &PinnedContract,
    harness: guild_session::HarnessKind,
    startup: bool,
) {
    // Keep the original run's synthetic approval unanswered. This separately
    // labelled run has its own actual parent/children and no waiting request.
    let origin = entity.update(cx, |root, cx| {
        let record = root.desk.session(original.origin).unwrap();
        let project = record.project;
        let session = root
            .open_tab_with_selection(
                project,
                match harness {
                    guild_session::HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
                    guild_session::HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
                    guild_session::HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
                },
                None,
                false,
                Some(guild_session::harness::LaunchSelection {
                    model: Some("fixture/model".into()),
                    effort: Some("high".into()),
                }),
                cx,
            )
            .unwrap();
        root.desk.record_harness_session(
            session,
            if harness == guild_session::HarnessKind::OpenCode {
                format!("ses_{}", session.to_string().replace('-', ""))
            } else {
                session.to_string()
            },
        );
        session
    });
    let conversation = entity.update(cx, |r, _| {
        r.desk
            .session(origin)
            .unwrap()
            .harness_session
            .clone()
            .unwrap()
    });
    let mut spec = original.clone();
    spec.origin = origin;
    spec.request_id = "separate-free-reconciliation-success".into();
    spec.checks.clear();
    spec.task="Separately owned free executable fixture for reconciliation; no model/provider or real assignment".into();
    spec.overrides = Some(serde_json::json!({"steps":[
        {"id":"contract","label":"Free fixture contract","action":"contract.prepare","role":"coordinator","depends_on":[],"outcomes":["contract"]},
        {"id":"first-fixture","label":"Free existing child","action":"session.assign","role":"builder","depends_on":["contract"],"outcomes":["build"]},
        {"id":"resume-ready","label":"After actual resume","action":"contract.prepare","role":"coordinator","depends_on":["contract"],"outcomes":["resume-ready"]},
        {"id":"reconciled-builder","label":"Free unfinished child","action":"session.assign","role":"builder","depends_on":["resume-ready"],"outcomes":["resume-build"]}
    ],"roles":{"builder":{"harness":"codex","model":"fixture-model","effort":"high"}},"corrections":[],"outcomes":{"resume-ready":{"validator":"contract","required":true},"resume-build":{"validator":"managed_session","required":true},"plan-review":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}}));
    let started = ask(
        entity,
        cx,
        WorkflowCommand::Start { spec: spec.clone() },
        origin,
    );
    let Answer::Workflow {
        reply: WorkflowReply::Receipt { run, revision, .. },
    } = started
    else {
        panic!("{started:?}")
    };
    let path = repo.join(".guild/workflow-runs").join(run.to_string());
    std::fs::create_dir_all(path.join("artifacts")).unwrap();
    let output = path.join("artifacts/contract.txt");
    std::fs::write(&output, "controlled nonempty note check passed").unwrap();
    let proof:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":origin,"contract":pinned.sha256,"source":crate::root::workflow::source::capture(repo,None).unwrap(),"scenario":"contract","step":"contract","tool":{"program":"/bin/test","version":"controlled fixture","argv":["test","-s","task.md"]},"observed_at_ms":1,"artifacts":[{"path":output.canonicalize().unwrap(),"sha256":digest(b"controlled nonempty note check passed")}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}})).unwrap();
    let qualified = ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: CheckpointRequest {
                checks: None,
                version: 1,
                request_id: "separate-contract".into(),
                expected_revision: revision,
                step: "contract".into(),
                outcome: "contract".into(),
                evidence: vec![proof],
            },
        },
        origin,
    );
    assert!(
        matches!(
            qualified,
            Answer::Workflow {
                reply: WorkflowReply::Receipt { .. }
            }
        ),
        "{qualified:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let child = loop {
        tick(entity, cx);
        let h = Journal::inspect(&path).unwrap();
        let child = h.receipts.iter().find_map(|r| {
            r.observation
                .as_ref()
                .filter(|o| o["operation"] == "assignment_delivered")
                .and_then(|o| serde_json::from_value::<SessionId>(o["session"].clone()).ok())
        });
        if let Some(child) = child
            && entity.update(cx, |r, _| {
                r.status_of(origin) == guild_session::status::Status::Working
                    && r.workflows.as_ref().is_some_and(|q| !q.in_flight)
            })
        {
            break child;
        }
        assert!(
            std::time::Instant::now() < until,
            "separate free fixture not delivered: harness={harness:?}, startup={startup}, history={:?}, parent={:?}",
            Journal::inspect(&path).unwrap().receipts.last(),
            entity.update(cx, |r, _| r.status_of(origin))
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let parent_pid = entity.update(cx, |root, cx| {
        root.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|b| match b {
                crate::Body::Terminal { session, view, .. } if *session == origin => {
                    view.read(cx).process_id()
                }
                _ => None,
            })
            .unwrap()
    });
    let ledger = entity.update(cx, |r, _| {
        r.processes.runs[&run].value.as_ref().unwrap()["review_ledger"].clone()
    });
    let rollout = guild_session::harness::CodexHarness::home().join("sessions/2026/10/19");
    std::fs::create_dir_all(&rollout).unwrap();
    std::fs::write(
        rollout.join(format!("rollout-2026-10-19T00-00-00-{}.jsonl", spec.origin)),
        serde_json::json!({"type":"session_meta","payload":{"id":spec.origin,"cwd":repo}})
            .to_string()
            + "\n",
    )
    .unwrap();
    if harness == guild_session::HarnessKind::Claude {
        let p = crate::paths::claude_projects_dir().join("controlled");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(format!("{conversation}.jsonl")),serde_json::json!({"type":"user","cwd":repo,"sessionId":conversation,"message":{"role":"user","content":"retained controlled history"}}).to_string()+"\n").unwrap();
    }
    if harness == guild_session::HarnessKind::OpenCode {
        let p = std::path::PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap())
            .join(format!("history-{conversation}.json"));
        std::fs::write(
            p,
            serde_json::json!({"info":{"id":conversation,"directory":repo},"messages":[]})
                .to_string(),
        )
        .unwrap();
    }
    entity.update(cx, |root, _| {
        let workspace = root.desk.session(spec.origin).unwrap().workspace;
        assert!(
            root.desk
                .sessions()
                .filter(|r| r.workspace == workspace && r.open)
                .all(|r| root
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .any(|b| b.session() == Some(r.id))),
            "fixture must have created every retained runtime, not a lazy saved seat"
        );
        root.restored_workspaces.insert(workspace);
    });
    let stopped = entity.update(cx, |root, cx| {
        root.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|b| match b {
                crate::Body::Terminal { session, view, .. } if *session == spec.origin => {
                    view.update(cx, |v, _| v.let_go())
                }
                _ => None,
            })
            .expect("owned parent PTY completion")
    });
    stopped
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    entity.update(cx, |root, cx| {
        let (tab, pane) = root
            .tabs
            .iter()
            .find_map(|t| {
                t.panes
                    .find(|b| b.session() == Some(spec.origin))
                    .map(|p| (t.id, p))
            })
            .unwrap();
        root.close_pane(tab, pane, cx);
    });
    let changed_while_stopped = repo.join("controlled-stopped-source-change.txt");
    if !startup {
        std::fs::write(
            &changed_while_stopped,
            "free fixture source changed while origin stopped",
        )
        .unwrap();
    }
    let refused = ask(
        entity,
        cx,
        WorkflowCommand::Recover {
            run,
            spec: ControlRequest {
                version: 1,
                request_id: "child-ipc-cannot-resume".into(),
                expected_revision: Journal::inspect(&path).unwrap().revision,
            },
        },
        child,
    );
    assert!(
        matches!(refused,Answer::Workflow {reply:WorkflowReply::Gap {message,..}} if message.contains("authority"))
    );
    let before_navigation = Journal::inspect(&path).unwrap().revision;
    let recovered = navigate(
        entity,
        cx,
        WorkflowCommand::Recover {
            run,
            spec: ControlRequest {
                version: 1,
                request_id: "actual-stopped-parent".into(),
                expected_revision: Journal::inspect(&path).unwrap().revision,
            },
        },
        child,
    );
    assert!(Journal::inspect(&path).unwrap().revision > before_navigation);
    assert!(
        matches!(
            recovered,
            Answer::Workflow {
                reply: WorkflowReply::Receipt { .. }
            }
        ),
        "{recovered:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        let ready=entity.update(cx,|root,cx| (startup || root.status_of(spec.origin)==guild_session::status::Status::Working) && root.tabs.iter().flat_map(crate::Tab::bodies).any(|b|matches!(b,crate::Body::Terminal{session,view,..} if *session==spec.origin && view.read(cx).process_id().is_some_and(|p|p!=parent_pid))) );
        if ready
            && Journal::inspect(&path).unwrap().receipts.iter().any(|r| {
                r.observation
                    .as_ref()
                    .is_some_and(|o| o["recovery"]["phase"] == "resume_launched")
            })
        {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "actual resumed parent not observed: {:?}",
            Journal::inspect(&path).unwrap().receipts.last()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let handoff = ask(
        entity,
        cx,
        WorkflowCommand::Status { version: 1, run },
        spec.origin,
    );
    let Answer::Workflow {
        reply: WorkflowReply::Snapshot { snapshot, .. },
    } = handoff
    else {
        panic!("{handoff:?}")
    };
    if harness == guild_session::HarnessKind::OpenCode {
        let witness = std::path::PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap())
            .join(spec.origin.to_string());
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(witness.with_extension("json")).unwrap())
                .unwrap();
        let argv = actual["argv"].as_array().unwrap();
        assert!(
            !argv.iter().any(|v| v == "--prompt"
                || v.as_str()
                    .is_some_and(|s| s.starts_with("Workflow recovery handoff"))),
            "unqualified OpenCode startup input: {actual}"
        );
        assert!(
            argv.windows(2)
                .any(|w| w[0] == "--session" && w[1] == conversation)
        );
        assert_eq!(
            actual["cwd"],
            serde_json::json!(repo.canonicalize().unwrap())
        );
        assert_eq!(snapshot["handoff"]["stage"], "waiting");
        assert_eq!(snapshot["handoff"]["reason"], "opencode_input_unqualified");
        assert_eq!(snapshot["handoff"]["ready"], false);
        assert_eq!(snapshot["handoff"]["continued"], false);
        let before = Journal::inspect(&path).unwrap();
        for _ in 0..2 {
            let reply = navigate(
                entity,
                cx,
                WorkflowCommand::Recover {
                    run,
                    spec: ControlRequest {
                        version: 1,
                        request_id: "actual-stopped-parent".into(),
                        expected_revision: before_navigation,
                    },
                },
                child,
            );
            assert!(
                matches!(
                    reply,
                    Answer::Workflow {
                        reply: WorkflowReply::Receipt { .. }
                    }
                ),
                "{reply:?}"
            );
        }
        let after = Journal::inspect(&path).unwrap();
        assert_eq!(after.revision, before.revision);
        assert!(!after.receipts.iter().any(|r| {
            r.observation
                .as_ref()
                .is_some_and(|v| v["handoff"].is_string())
        }));
        assert!(!witness.with_extension("inputs.jsonl").exists());
        assert!(!witness.with_extension("startup.jsonl").exists());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(witness.with_extension("json")).unwrap()
            )
            .unwrap()["pid"],
            actual["pid"]
        );
        assert_eq!(
            entity.update(cx, |r, _| r
                .desk
                .session(spec.origin)
                .unwrap()
                .harness_session
                .clone()),
            Some(conversation)
        );
        eprintln!(
            "OpenCode controlled same-CID resume: no startup/input, persistent unqualified-input reason, repeated recovery reuses holder/history"
        );
        return;
    }
    if startup {
        let witness = std::path::PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap())
            .join(format!("{}.json", spec.origin));
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(witness).unwrap()).unwrap();
        assert!(
            v["argv"].as_array().unwrap().iter().any(|v| v
                .as_str()
                .is_some_and(|s| s.starts_with("Workflow recovery handoff"))),
            "silent initial resume omitted its literal startup handoff: {v}"
        );
    } else {
        assert_eq!(
            snapshot["handoff"]["stage"], "waiting",
            "resumed busy origin must expose the pending, unsent handoff"
        );
        let changed = ask(
            entity,
            cx,
            WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "inspect-stopped-source-change".into(),
                    expected_revision: 0,
                    assessment: None,
                },
            },
            spec.origin,
        );
        let Answer::Workflow {
            reply:
                WorkflowReply::Snapshot {
                    revision,
                    snapshot: changed,
                    ..
                },
        } = changed
        else {
            panic!("{changed:?}")
        };
        assert!(
            changed["reconciliation"]["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "source_changed")
        );
        let refused = ask(
            entity,
            cx,
            WorkflowCommand::Reconcile {
                run,
                spec: ReconcileRequest {
                    version: 1,
                    request_id: "cannot-ack-stopped-change".into(),
                    expected_revision: revision,
                    assessment: Some(
                        changed["reconciliation"]["fingerprint"]
                            .as_str()
                            .unwrap()
                            .into(),
                    ),
                },
            },
            spec.origin,
        );
        assert!(matches!(
            refused,
            Answer::Workflow {
                reply: WorkflowReply::Gap { .. }
            }
        ));
        std::fs::remove_file(changed_while_stopped).unwrap();
    }
    let witness = std::path::PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap())
        .join(format!("{}.json", spec.origin));
    let witness: serde_json::Value =
        serde_json::from_slice(&std::fs::read(witness).unwrap()).unwrap();
    let argv = witness["argv"].as_array().unwrap();
    eprintln!(
        "CONTROLLED_RECOVERY_WITNESS {}",
        serde_json::json!({"harness":harness,"startup":startup,"run":run,"origin":origin,"conversation":conversation,"witness":witness})
    );
    assert!(
        argv.iter().any(|v| v == &conversation)
            && argv.iter().any(|v| v
                == if harness == guild_session::HarnessKind::Codex {
                    "resume"
                } else if harness == guild_session::HarnessKind::Claude {
                    "--resume"
                } else {
                    "--session"
                })
    );
    assert!(!argv.iter().any(|v| v == &spec.task));
    if harness == guild_session::HarnessKind::OpenCode {
        assert_eq!(witness["inline"]["model"], "fixture/model#high");
    } else {
        assert!(argv.iter().any(|a| a == "fixture/model"));
        assert!(argv.iter().any(|a| a
            == if harness == guild_session::HarnessKind::Claude {
                "high"
            } else {
                "model_reasoning_effort=\"high\""
            }));
    }

    // The real controlled process independently enables its prompt and emits
    // Stop only after this external gate, then witnesses actual PTY bytes.
    let fixture = std::path::PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    if startup {
        assert_eq!(snapshot["handoff"]["stage"], "submitted");
        assert_eq!(snapshot["handoff"]["path"], "startup");
        assert_eq!(
            snapshot["handoff"]["ready"], false,
            "argv is not prompt readiness"
        );
        assert_ne!(
            snapshot["handoff"]["stage"], "observed",
            "no adapter turn start before independent executable gate"
        );
        std::fs::write(
            fixture.join(format!("{}.startup-accept", spec.origin)),
            "independent executable lifecycle gate",
        )
        .unwrap();
    } else {
        std::fs::write(
            fixture.join(format!("{}.handoff-ready", spec.origin)),
            "independent free prompt gate",
        )
        .unwrap();
    }
    let inputs = fixture.join(format!(
        "{}.{}.jsonl",
        spec.origin,
        if startup { "startup" } else { "inputs" }
    ));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        let status = ask(
            entity,
            cx,
            WorkflowCommand::Status { version: 1, run },
            spec.origin,
        );
        if matches!(&status,Answer::Workflow{reply:WorkflowReply::Snapshot {snapshot,..}} if snapshot["handoff"]["stage"]=="observed")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "handoff not observed through actual native PTY: {:?}",
            match status {
                Answer::Workflow {
                    reply: WorkflowReply::Snapshot { snapshot, .. },
                } => snapshot["handoff"].clone(),
                _ => serde_json::Value::Null,
            }
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let delivered = std::fs::read_to_string(&inputs).unwrap();
    assert_eq!(delivered.lines().count(), 1);
    assert!(delivered.contains(&run.to_string()));
    assert!(delivered.contains("Workflow recovery handoff"));
    assert!(!delivered.contains(&spec.task));
    for index in 0..2 {
        let revealed = navigate(
            entity,
            cx,
            WorkflowCommand::Recover {
                run,
                spec: ControlRequest {
                    version: 1,
                    request_id: format!("after-handoff-reveal-{index}"),
                    expected_revision: Journal::inspect(&path).unwrap().revision,
                },
            },
            child,
        );
        assert!(
            matches!(
                revealed,
                Answer::Workflow {
                    reply: WorkflowReply::Receipt { .. }
                }
            ),
            "{revealed:?}"
        );
        for _ in 0..10 {
            tick(entity, cx);
        }
    }
    assert_eq!(
        std::fs::read_to_string(&inputs).unwrap(),
        delivered,
        "repeated navigation must not send twice"
    );
    let proof:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":spec.origin,"contract":pinned.sha256,"source":crate::root::workflow::source::capture(repo,None).unwrap(),"scenario":"resume-ready","step":"resume-ready","tool":{"program":"/bin/test","version":"controlled fixture","argv":["test","-s","task.md"]},"observed_at_ms":1,"artifacts":[{"path":output.canonicalize().unwrap(),"sha256":digest(b"controlled nonempty note check passed")}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}})).unwrap();
    let checkpoint = ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: CheckpointRequest {
                checks: None,
                version: 1,
                request_id: "parent-after-actual-resume".into(),
                expected_revision: Journal::inspect(&path).unwrap().revision,
                step: "resume-ready".into(),
                outcome: "resume-ready".into(),
                evidence: vec![proof],
            },
        },
        spec.origin,
    );
    assert!(
        matches!(
            checkpoint,
            Answer::Workflow {
                reply: WorkflowReply::Receipt { .. }
            }
        ),
        "{checkpoint:?}"
    );
    let read = ask(
        entity,
        cx,
        WorkflowCommand::Reconcile {
            run,
            spec: ReconcileRequest {
                version: 1,
                request_id: "inspect-resumed".into(),
                expected_revision: 0,
                assessment: None,
            },
        },
        spec.origin,
    );
    let Answer::Workflow {
        reply:
            WorkflowReply::Snapshot {
                revision,
                snapshot: assessment,
                ..
            },
    } = read
    else {
        panic!("{read:?}")
    };
    assert_eq!(
        assessment["reconciliation"]["clear"], true,
        "{}",
        assessment["reconciliation"]
    );
    assert!(
        !Journal::inspect(&path).unwrap().receipts.iter().any(|r| r
            .request
            .as_ref()
            .is_some_and(|m| m.context["effect"]["plan"]["step"] == "reconciled-builder")),
        "observation alone dispatched work"
    );
    let ack = WorkflowCommand::Reconcile {
        run,
        spec: ReconcileRequest {
            version: 1,
            request_id: "actual-parent-ack".into(),
            expected_revision: revision,
            assessment: Some(
                assessment["reconciliation"]["fingerprint"]
                    .as_str()
                    .unwrap()
                    .into(),
            ),
        },
    };
    let denied = ask(entity, cx, ack.clone(), child);
    assert!(
        matches!(
            denied,
            Answer::Workflow {
                reply: WorkflowReply::Gap { .. }
            }
        ),
        "child cannot acknowledge as parent"
    );
    std::fs::write(&output, "resource changed after assessment").unwrap();
    let stale = ask(entity, cx, ack.clone(), spec.origin);
    assert!(
        matches!(
            stale,
            Answer::Workflow {
                reply: WorkflowReply::Gap { .. }
            }
        ),
        "changed evidence cannot be acknowledged"
    );
    std::fs::write(&output, "controlled nonempty note check passed").unwrap();
    let accepted = ask(entity, cx, ack.clone(), spec.origin);
    assert!(
        matches!(
            accepted,
            Answer::Workflow {
                reply: WorkflowReply::Receipt { .. }
            }
        ),
        "{accepted:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        let h = Journal::inspect(&path).unwrap();
        if h.receipts
            .iter()
            .filter(|r| {
                r.observation
                    .as_ref()
                    .is_some_and(|o| o["operation"] == "assignment_delivered")
            })
            .count()
            == 2
            && h.receipts.iter().filter(|r| r.request.is_some()).all(|r| {
                h.receipts
                    .iter()
                    .any(|o| o.attempt == r.attempt && o.observation.is_some())
            })
        {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "qualified reconciliation did not finish checkout+launch+delivery: {:?}",
            h.receipts.last()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before = Journal::inspect(&path).unwrap();
    let repeated = ask(entity, cx, ack, spec.origin);
    assert!(
        matches!(
            repeated,
            Answer::Workflow {
                reply: WorkflowReply::Receipt { .. }
            }
        ),
        "{repeated:?}"
    );
    assert_eq!(Journal::inspect(&path).unwrap().revision, before.revision);
    assert_eq!(assessment["review_ledger"], ledger);
    assert_eq!(assessment["recovery"]["state"]["ready"], false);
    assert_eq!(assessment["recovery"]["state"]["continued"], false);
    eprintln!(
        "Actual {harness:?} native origin resume (startup={startup}) -> explicit parent reconciliation -> one unfinished child delivery; no original task/approval injection"
    );
}

/// A managed designer uses the same real checkout/session/report owners as a builder,
/// while its contract becomes accepted only through the initiating parent.
pub(super) fn managed_designer_case(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    repo: &std::path::Path,
    project: guild_session::ProjectId,
) {
    let parent = entity.update(cx, |r, cx| {
        let session = r
            .open_tab_with(project, Runs::Codex(SessionMode::Terminal), None, false, cx)
            .unwrap();
        r.desk.record_harness_session(session, session.to_string());
        session
    });
    let spec = StartRequest {
        version: 1,
        request_id: "managed-designer-contract".into(),
        expected_revision: 0,
        origin: parent,
        target: repo.into(),
        selection: Some("task.md".into()),
        definition: None,
        overrides: Some(serde_json::json!({"steps":[
            {"id":"contract","label":"Managed design preparation","action":"contract.prepare","role":"designer","depends_on":[],"outcomes":["contract"]},
            {"id":"plan-review","label":"Parent-owned next review","action":"review.request","role":"reviewer","depends_on":["contract"],"outcomes":["plan-review"]},
            {"id":"review-gate","label":"Wait for the review outcome","action":"checkpoint.decide","role":"coordinator","depends_on":["plan-review"],"outcomes":["review-gate"],"condition":{"op":"outcome","id":"plan-review"}},
            {"id":"build","label":"Must not dispatch before review","action":"session.assign","role":"builder","depends_on":["review-gate"],"outcomes":["build"]}
        ],"roles":{"designer":{"harness":"codex","model":"fixture-model","effort":"high"}},"corrections":[],"outcomes":{"review-gate":{"validator":"observation","required":true},"build":{"required":false},"code-review":{"required":false},"accept":{"required":false},"delivery":{"required":false}}})),
        task: "Prepare a controlled contract fixture, not a real implementation task.".into(),
        architecture_only: false,
        checks: vec![],
    };
    let started = ask(entity, cx, WorkflowCommand::Start { spec }, parent);
    let Answer::Workflow {
        reply: WorkflowReply::Receipt { run, .. },
    } = started
    else {
        panic!("{started:?}")
    };
    let path = repo.join(".guild/workflow-runs").join(run.to_string());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let (child, assignment) = loop {
        tick(entity, cx);
        let history = Journal::inspect(&path).unwrap();
        let delivered = history
            .receipts
            .iter()
            .filter_map(|r| r.observation.as_ref())
            .find(|o| o["operation"] == "assignment_delivered");
        if let Some(delivered) = delivered {
            let child: SessionId = serde_json::from_value(delivered["session"].clone()).unwrap();
            if entity.update(cx, |r, _| {
                r.workflows.as_ref().is_some_and(|q| !q.in_flight)
            }) {
                let assignment = history
                    .receipts
                    .iter()
                    .filter_map(|r| r.request.as_ref())
                    .find(|m| m.context["operation"] == "native_assignment")
                    .unwrap()
                    .context["effect"]["plan"]
                    .clone();
                break (child, assignment);
            }
        }
        assert!(
            std::time::Instant::now() < until,
            "managed contract.prepare never reached its actual assigned session: {:?}",
            history.receipts.last()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(
        assignment["contract"].is_null(),
        "designer bootstrap must not invent an accepted contract"
    );
    assert!(
        assignment["prompt"]
            .as_str()
            .unwrap()
            .contains("No contract has been accepted yet")
    );
    let checkout = entity.update(cx, |r, _| r.desk.session(child).unwrap().cwd.clone());
    std::fs::write(
        checkout.join("task.md"),
        "Controlled newly prepared contract with observable acceptance.",
    )
    .unwrap();
    let report = PathBuf::from(assignment["report"].as_str().unwrap());
    std::fs::write(&report, "Actual deterministic designer report, no model.").unwrap();
    let contract = pin_contract(&checkout, Some(std::path::Path::new("task.md")), None).unwrap();
    let source =
        crate::root::workflow::source::capture(&checkout, assignment["base"].as_str()).unwrap();
    let proof:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":child,"contract":contract.sha256,"source":source,"scenario":"contract","step":"contract","tool":{"program":"controlled note check","version":"1","argv":["test","-s","task.md"]},"observed_at_ms":1,"artifacts":[{"path":report,"sha256":digest(&std::fs::read(&report).unwrap())}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}})).unwrap();
    let mut checkpoint = CheckpointRequest {
        checks: None,
        version: 1,
        request_id: "designer-report".into(),
        expected_revision: Journal::inspect(&path).unwrap().revision,
        step: "contract".into(),
        outcome: "contract".into(),
        evidence: vec![proof],
    };
    let reported = ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: checkpoint.clone(),
        },
        child,
    );
    let Answer::Workflow {
        reply: WorkflowReply::Receipt {
            revision, snapshot, ..
        },
    } = reported
    else {
        panic!("{reported:?}")
    };
    assert_eq!(snapshot["nodes"][0]["state"], "active");
    assert!(snapshot["accepted_contract"].is_null());
    checkpoint.request_id = "parent-accept-designer-contract".into();
    checkpoint.expected_revision = revision;
    let accepted = ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: checkpoint,
        },
        parent,
    );
    let Answer::Workflow {
        reply: WorkflowReply::Receipt { snapshot, .. },
    } = accepted
    else {
        panic!("{accepted:?}")
    };
    assert_eq!(snapshot["nodes"][0]["state"], "finished");
    assert_eq!(snapshot["nodes"][1]["state"], "ready");
    assert_eq!(snapshot["source"]["sha256"], source.sha256);
    assert_eq!(
        snapshot["accepted_contract"]["contract"]["sha256"],
        contract.sha256
    );
    assert!(snapshot["reviews"].as_array().unwrap().is_empty());
    assert_eq!(snapshot["nodes"][2]["state"], "blocked");
    assert_eq!(snapshot["nodes"][3]["state"], "blocked");
    for _ in 0..5 {
        tick(entity, cx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        Journal::inspect(&path)
            .unwrap()
            .receipts
            .iter()
            .filter_map(|r| r.request.as_ref())
            .filter(|m| m.context["operation"] == "native_assignment")
            .all(|m| m.context["effect"]["plan"]["step"] == "contract"),
        "unfinished review predicate must not dispatch another managed child"
    );
    eprintln!(
        "CONTROLLED_MANAGED_DESIGNER run={run} child={child} source={}",
        source.sha256
    );
}
