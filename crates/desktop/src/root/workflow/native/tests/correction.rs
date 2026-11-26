//! Installed SDK findings -> actual assigned PTY -> changed-source parent re-review.
use super::*;
use reconciliation::{ask, tick};
fn reply(answer: Answer) -> WorkflowReply {
    match answer {
        Answer::Workflow { reply } => reply,
        _ => panic!("{answer:?}"),
    }
}
fn status(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    run: RunId,
    parent: SessionId,
) -> (u64, serde_json::Value) {
    match reply(ask(
        entity,
        cx,
        WorkflowCommand::Status { version: 1, run },
        parent,
    )) {
        WorkflowReply::Snapshot {
            revision, snapshot, ..
        } => (revision, snapshot),
        other => panic!("{other:?}"),
    }
}
fn user_keys(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    child: SessionId,
    keys: &[(&str, Option<&str>)],
) {
    entity.update(cx, |r, cx| {
        let view = r
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|b| match b {
                crate::Body::Terminal { session, view, .. } if *session == child => {
                    Some(view.clone())
                }
                _ => None,
            })
            .unwrap();
        view.update(cx, |v, cx| {
            for (name, text) in keys {
                let mut keystroke = gpui::Keystroke::parse(name).unwrap();
                keystroke.key_char = text.map(str::to_owned);
                v.on_key(
                    &gpui::KeyDownEvent {
                        keystroke,
                        is_held: false,
                        prefer_character_input: false,
                    },
                    cx,
                );
            }
        });
    });
}
fn observe_manual_turn(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    child: SessionId,
    witness: &std::path::Path,
    expected_inputs: usize,
) {
    // The actual fixture process, after consuming bytes, owns the delayed hook/rollout.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let input = witness.join(format!("{child}.inputs.jsonl"));
    loop {
        if std::fs::read_to_string(&input).is_ok_and(|s| s.lines().count() >= expected_inputs) {
            break;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::remove_file(witness.join(format!("{child}.suppress-submit"))).unwrap();
    loop {
        tick(entity, cx);
        let done = entity.update(cx, |r, cx| {
            r.refresh_handoff_prompts(cx);
            r.status_of(child) == guild_session::status::Status::Done
                && r.workflows
                    .as_ref()
                    .unwrap()
                    .prompts
                    .read()
                    .unwrap()
                    .get(&child.to_string())
                    .is_some_and(|p| !p.pending && !p.draft)
        });
        if done {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "actual manual turn observation did not clear pending"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
pub(super) fn exercise(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    run: RunId,
    parent: SessionId,
    child: SessionId,
    path: &std::path::Path,
    mut report: CheckpointRequest,
) {
    let witness = PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    let cfg = witness.join("correction-config");
    std::fs::create_dir_all(cfg.join("prompts/profiles")).unwrap();
    let provider = witness.join("correction-provider.py");
    std::fs::write(&provider,r#"#!/usr/bin/python3
import sys,pathlib,json
root=pathlib.Path(__file__).parent
with (root/'correction-starts').open('a') as f:f.write('start\n')
sys.stdin.read()
f=[] if (root/'corrected-review').exists() else [{'file':'new-source.txt','line':1,'severity':'major','confidence':100,'title':'Controlled correction fixture','body':'Deterministic fixture: update the original assigned source before resubmission.','fix':'Write the controlled corrected source.','lenses':['bugs']}]
print(json.dumps({'findings':f}))
"#).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(
        cfg.join("config"),
        "profile = workflow-fixture\nstagger-delay = 0s\n",
    )
    .unwrap();
    std::fs::write(cfg.join("prompts/profiles/workflow-fixture.md"),"---\ndescription: Deterministic correction fixture\nmodel: codex/fixture:high\nagents:\n  - name: logic\n    lenses: [bugs]\n---\nInspect {{SCOPE}} with {{CONTEXT}} in {{WORKDIR}}.\n").unwrap();
    std::fs::write(cfg.join("extensions.json"),serde_json::to_vec(&serde_json::json!({"version":1,"policy":{"executors":{"claude":{"executable":provider},"codex":{"executable":provider},"opencode":{"executable":provider}},"budget":{"max_calls":1},"stages":{"synthesis":{"enabled":false},"verify":{"enabled":false}},"runtime":{"hard_timeout":"5s","idle_timeout":"5s"}}})).unwrap()).unwrap();
    let (revision, _) = status(entity, cx, run, parent);
    let mut spec=ReviewRequest {version:1,request_id:"findings-round".into(),expected_revision:revision,step:"code-review".into(),package:std::env::var_os("GUILD_TEST_REVUST_PACKAGE").unwrap().into(),config_dir:Some(cfg.clone()),profile:Some("workflow-fixture".into()),scope:"Controlled source-specific findings for the assigned child; no actual model or independent evaluation.".into(),allowance:1,keep_open:false,prior:vec![]};
    // Failed inspection used to poison code-review-01/defaults forever.
    let extensions = std::fs::read(cfg.join("extensions.json")).unwrap();
    std::fs::write(cfg.join("extensions.json"), "{invalid controlled config").unwrap();
    let refused = reply(ask(
        entity,
        cx,
        WorkflowCommand::Review {
            run,
            spec: spec.clone(),
        },
        parent,
    ));
    assert!(matches!(&refused, WorkflowReply::Gap { .. }), "{refused:?}");
    let (revision, failed) = status(entity, cx, run, parent);
    assert!(failed["reviews"].as_array().unwrap().is_empty());
    assert_eq!(failed["review_ledger"]["rounds"], serde_json::json!([]));
    assert!(
        failed["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["observation"]["operation"] == "review_preparation_refused"),
        "preparation was not reached: {refused:?}"
    );
    assert!(!witness.join("correction-starts").exists());
    let duplicate = reply(ask(
        entity,
        cx,
        WorkflowCommand::Review {
            run,
            spec: spec.clone(),
        },
        parent,
    ));
    assert!(matches!(duplicate, WorkflowReply::Gap { .. }));
    assert_eq!(
        status(entity, cx, run, parent).0,
        revision,
        "refused preparation is not repeated"
    );
    std::fs::write(cfg.join("extensions.json"), extensions).unwrap();
    spec.request_id = "findings-after-proven-preparation-refusal".into();
    spec.expected_revision = revision;
    let started = reply(ask(
        entity,
        cx,
        WorkflowCommand::Review {
            run,
            spec: spec.clone(),
        },
        parent,
    ));
    assert!(
        matches!(started, WorkflowReply::Receipt { .. }),
        "{started:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let findings = loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["reviews"][0]["result"]["archive"]["classification"] == "findings"
            && s["reviews"][0]["job"]["state"] == "exited"
            && s["reviews"][0]["observation"]["observer"]["pid"].is_number()
        {
            break s;
        }
        assert!(
            std::time::Instant::now() < until,
            "actual SDK findings unavailable: {}",
            s["reviews"]
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(findings["nodes"][1]["state"], "finished");
    let attempt: AttemptId =
        serde_json::from_value(findings["reviews"][0]["attempt"].clone()).unwrap();
    let request = |revision, key: &str| WorkflowCommand::Retry {
        run,
        spec: RetryRequest {
            version: 1,
            request_id: key.into(),
            expected_revision: revision,
            attempt,
        },
    };
    let (revision, _) = status(entity, cx, run, parent);
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            request(revision, "busy-correction"),
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    let child_source = report.evidence[0].source.target.clone();
    let rollout = guild_session::harness::CodexHarness::home().join("sessions/2026/10/20");
    std::fs::create_dir_all(&rollout).unwrap();
    std::fs::write(
        rollout.join(format!("rollout-2026-10-20T00-00-00-{child}.jsonl")),
        serde_json::json!({"type":"session_meta","payload":{"id":child,"cwd":child_source}})
            .to_string()
            + "\n",
    )
    .unwrap();
    for _ in 0..5 {
        tick(entity, cx);
    }
    std::fs::write(
        witness.join(format!("{child}.handoff-ready")),
        "independent original executable completion gate",
    )
    .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        if entity.update(cx, |r, _| r.status_of(child)) == guild_session::status::Status::Done {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "assigned child did not independently complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // A real manual Enter, with its adapter hook held back, must not admit corrections.
    let input = witness.join(format!("{child}.inputs.jsonl"));
    // Empty Enter may produce completion only, never a PromptSubmitted witness.
    std::fs::write(
        witness.join(format!("{child}.suppress-submit")),
        "hold input hook",
    )
    .unwrap();
    user_keys(entity, cx, child, &[("enter", None)]);
    std::fs::write(
        witness.join(format!("{child}.completion-only")),
        "emit Stop only",
    )
    .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        tick(entity, cx);
        if witness
            .join(format!("{child}.completion-observed"))
            .exists()
            && entity.update(cx, |r, _| r.status_of(child)) == guild_session::status::Status::Done
        {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "actual completion-only event not observed"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let (before, s) = status(entity, cx, run, parent);
    let refused = reply(ask(
        entity,
        cx,
        request(before, "workflow-still-blocked-after-stop"),
        parent,
    ));
    assert!(
        matches!(refused, WorkflowReply::Gap { .. }),
        "Stop alone released strict workflow input"
    );
    assert_eq!(Journal::inspect(path).unwrap().revision, before);
    assert_eq!(
        status(entity, cx, run, parent).1["review_ledger"],
        s["review_ledger"]
    );
    let (send, received) = mpsc::channel();
    entity.update(cx, |r, cx| {
        r.take_command(
            crate::hooks::server::Asked {
                command: guild_session::command::Command::AgentSend {
                    session: child.to_string(),
                    text: "Explicit generic caller after completion only".into(),
                    caller: Some(parent.to_string()),
                },
                answer: send,
            },
            cx,
        )
    });
    assert!(
        !matches!(received.try_recv(), Ok(Answer::Refused { .. })),
        "generic caller was denied by workflow-only pending gate"
    );
    observe_manual_turn(entity, cx, child, &witness, 2);
    loop {
        tick(entity, cx);
        if let Ok(answer) = received.try_recv() {
            assert!(matches!(answer, Answer::Sent { .. }), "{answer:?}");
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "explicit generic send not observed"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    for (key, keys) in [
        (
            "manual-submission",
            vec![("m", Some("manual input")), ("enter", None)],
        ),
        (
            "multiline-draft",
            vec![
                ("m", Some("multiline input")),
                ("shift-enter", None),
                ("alt-enter", None),
            ],
        ),
        ("unknown-navigation", vec![("up", None)]),
    ] {
        std::fs::write(
            witness.join(format!("{child}.suppress-submit")),
            "hold actual adapter acknowledgement",
        )
        .unwrap();
        let expected_inputs = std::fs::read_to_string(&input)
            .unwrap_or_default()
            .lines()
            .count()
            + 1;
        user_keys(entity, cx, child, &keys);
        let before = Journal::inspect(path).unwrap();
        let (_, previous) = status(entity, cx, run, parent);
        let refused = reply(ask(entity, cx, request(before.revision, key), parent));
        assert!(
            matches!(refused, WorkflowReply::Gap { .. }),
            "{key} admitted a second payload: {refused:?}"
        );
        assert_eq!(
            Journal::inspect(path).unwrap().revision,
            before.revision,
            "no correction reservation on blocked input"
        );
        let (_, after) = status(entity, cx, run, parent);
        assert_eq!(after["review_ledger"], previous["review_ledger"]);
        assert!(
            !std::fs::read_to_string(&input)
                .unwrap_or_default()
                .contains("Controlled correction fixture")
        );
        if key != "manual-submission" {
            // Navigation alone is not a prompt. Only real text can justify a later start event.
            if key == "unknown-navigation" {
                user_keys(
                    entity,
                    cx,
                    child,
                    &[("t", Some("genuine text after navigation"))],
                );
            }
            user_keys(entity, cx, child, &[("enter", None)]);
        }
        observe_manual_turn(entity, cx, child, &witness, expected_inputs);
    }
    let manual_input = std::fs::read_to_string(&input).unwrap();
    assert!(manual_input.contains("manual input"));
    let lines: Vec<String> = manual_input
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .any(|s| s.contains("multiline input\n\x1b\r\r")),
        "actual multiline bytes differ: {lines:?}"
    );
    let (revision, _) = status(entity, cx, run, parent);
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            request(revision, "wrong-child-controller"),
            child
        )),
        WorkflowReply::Gap { .. }
    ));
    entity.update(cx, |r, _| {
        r.desk
            .record_harness_session(child, "different-conversation".into())
    });
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            request(revision, "wrong-conversation"),
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    entity.update(cx, |r, _| {
        r.desk.record_harness_session(child, child.to_string())
    });
    // Pause only UI callback consumption after the real worker's durable reserve.
    // Source changes before actual input must be a positive refusal, retaining
    // its claim so an explicit retry does not charge another correction.
    for (key, pending_input) in [
        ("pending-before-bytes", true),
        ("refused-before-bytes", false),
    ] {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tick(entity, cx);
            if entity.update(cx, |r, _| {
                r.workflows
                    .as_ref()
                    .is_some_and(|q| !q.in_flight && q.waiting.is_empty())
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "fixture queue not quiescent before callback pause"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let revision = Journal::inspect(path).unwrap().revision;
        let first = request(revision, key);
        let (send, receive) = mpsc::channel();
        entity.update(cx, |r, cx| {
            r.command_workflow(&first, Some(&parent.to_string()), send, cx)
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let h = Journal::inspect(path).unwrap();
            if h.receipts
                .iter()
                .any(|r| r.request.as_ref().is_some_and(|m| m.request_id == key))
            {
                break;
            }
            if let Ok(answer) = receive.try_recv() {
                panic!("correction refused before reservation: {answer:?}");
            }
            assert!(
                std::time::Instant::now() < until,
                "correction {key} not reserved before callback"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Hold the actual writer-qualified callback, not a fabricated verified flag.
        let verified = if pending_input {
            loop {
                let completion = entity.update(cx, |r, _| {
                    r.workflows.as_ref().unwrap().completed.try_recv().ok()
                });
                if let Some(completion) = completion {
                    let Some(Effect::Correction(effect)) = &completion.effect else {
                        panic!("expected correction callback: {:?}", completion.reply)
                    };
                    if effect.verified {
                        break Some(completion);
                    }
                    let effect = *effect.clone();
                    entity.update(cx, |r, cx| {
                        r.perform_workflow_correction(effect, completion, cx)
                    });
                }
                assert!(
                    std::time::Instant::now() < until,
                    "actual correction preflight did not qualify callback"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        } else {
            None
        };
        let changed = child_source.join("changed-before-corrective-input");
        let prior_refusals = Journal::inspect(path)
            .unwrap()
            .receipts
            .iter()
            .filter(|r| {
                r.observation.as_ref().is_some_and(|o| {
                    o["operation"] == "correction_delivery" && o["stage"] == "refused"
                })
            })
            .count();
        let expected_inputs = std::fs::read_to_string(&input).unwrap().lines().count() + 1;
        if pending_input {
            std::fs::write(
                witness.join(format!("{child}.suppress-submit")),
                "hold final owner pending submission",
            )
            .unwrap();
            user_keys(entity, cx, child, &[("enter", None)]);
        } else {
            std::fs::write(&changed, "invalidate asynchronous callback").unwrap();
        }
        if let Some(completion) = verified {
            let Some(Effect::Correction(effect)) = &completion.effect else {
                unreachable!()
            };
            assert!(effect.verified);
            let effect = *effect.clone();
            entity.update(cx, |r, cx| {
                r.perform_workflow_correction(effect, completion, cx)
            });
        }
        loop {
            tick(entity, cx);
            let h = Journal::inspect(path).unwrap();
            if h.receipts
                .iter()
                .filter(|r| {
                    r.observation.as_ref().is_some_and(|o| {
                        o["operation"] == "correction_delivery" && o["stage"] == "refused"
                    })
                })
                .count()
                > prior_refusals
            {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "positive pre-input refusal not retained"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let answer = loop {
            if let Ok(answer) = receive.try_recv() {
                break answer;
            }
            tick(entity, cx);
            assert!(
                std::time::Instant::now() < until,
                "refused callback receipt not delivered to original caller"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(matches!(answer, Answer::Workflow { .. }));
        assert!(
            !std::fs::read_to_string(&input)
                .unwrap()
                .contains("Controlled correction fixture"),
            "native callback delivered correction into pending input"
        );
        if pending_input {
            // Observe the first PTY read before another write can merge into its record.
            // Keep suppress-submit until the later genuine text is also consumed.
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                tick(entity, cx);
                if std::fs::read_to_string(&input)
                    .is_ok_and(|s| s.lines().count() >= expected_inputs)
                {
                    break;
                }
                assert!(std::time::Instant::now() < until, "held Enter not consumed");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // The empty Enter remains pending; do not fabricate a start event to release it.
            user_keys(
                entity,
                cx,
                child,
                &[
                    ("t", Some("genuine text after held callback")),
                    ("enter", None),
                ],
            );
            observe_manual_turn(entity, cx, child, &witness, expected_inputs + 1);
        } else {
            std::fs::remove_file(changed).unwrap();
        }
    }
    let manual_count = std::fs::read_to_string(&input).unwrap().lines().count();
    let (revision, s) = status(entity, cx, run, parent);
    let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
    assert_eq!(ledger.corrections_used(), 1);
    let unknown = std::env::var_os("GUILD_TEST_REVUST_CORRECTION_UNKNOWN").is_some();
    if unknown {
        std::fs::write(
            witness.join(format!("{child}.suppress-submit")),
            "Do not emit an adapter acknowledgement for actual input",
        )
        .unwrap();
    }
    let correction = request(revision, "same-session-findings");
    let accepted = reply(ask(entity, cx, correction.clone(), parent));
    assert!(
        matches!(accepted, WorkflowReply::Receipt { .. }),
        "{accepted:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        tick(entity, cx);
        let history = Journal::inspect(path).unwrap();
        if history.receipts.iter().any(|r| {
            r.observation.as_ref().is_some_and(|o| {
                o["operation"] == "correction_delivery"
                    && o["stage"] == if unknown { "unknown" } else { "observed" }
            })
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "actual corrective input/turn not observed: {:?}",
            history.receipts.last()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let input = witness.join(format!("{child}.inputs.jsonl"));
    let before = std::fs::read_to_string(&input).unwrap();
    assert_eq!(before.lines().count(), manual_count + 1);
    assert!(before.contains("Controlled correction fixture"));
    assert!(matches!(
        reply(ask(entity, cx, correction, parent)),
        WorkflowReply::Receipt { .. }
    ));
    assert_eq!(std::fs::read_to_string(&input).unwrap(), before);
    let (revision, s) = status(entity, cx, run, parent);
    let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
    assert_eq!(ledger.used(), 1);
    assert_eq!(ledger.corrections_used(), 1);
    if unknown {
        let refused = reply(ask(
            entity,
            cx,
            request(revision, "never-replay-unknown"),
            parent,
        ));
        assert!(matches!(refused, WorkflowReply::Gap { .. }), "{refused:?}");
        assert_eq!(std::fs::read_to_string(&input).unwrap(), before);
        assert_eq!(ledger.corrections_used(), 1);
        assert_eq!(s["nodes"][1]["state"], "finished");
        eprintln!("CONTROLLED_CORRECTION_UNKNOWN {}", s);
        return;
    }
    // Source mutation is controlled fixture work, never a model or acceptance claim.
    std::fs::write(
        child_source.join("corrected-source.txt"),
        "controlled changed source after same-session correction",
    )
    .unwrap();
    report.request_id = "corrected-child-report".into();
    report.expected_revision = revision;
    report.evidence[0].id = uuid::Uuid::new_v4();
    report.evidence[0].source = crate::root::workflow::source::capture(
        &child_source,
        report.evidence[0].source.base.as_deref(),
    )
    .unwrap();
    let submitted = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: report.clone(),
        },
        child,
    ));
    let WorkflowReply::Receipt { revision, .. } = submitted else {
        panic!("{submitted:?}")
    };
    report.request_id = "parent-accept-correction".into();
    report.expected_revision = revision;
    let accepted = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: report.clone(),
        },
        parent,
    ));
    let WorkflowReply::Receipt { revision, .. } = accepted else {
        panic!("{accepted:?}")
    };
    std::fs::write(
        witness.join("corrected-review"),
        "second deterministic SDK result is clean",
    )
    .unwrap();
    std::fs::write(
        cfg.join("prompts/profiles/workflow-fixture.md"),
        "invalid mutable profile must not be reloaded",
    )
    .unwrap();
    let next = ReviewRequest {
        request_id: "source-qualified-recheck".into(),
        expected_revision: revision,
        ..spec.clone()
    };
    let started = reply(ask(
        entity,
        cx,
        WorkflowCommand::Review { run, spec: next },
        parent,
    ));
    assert!(
        matches!(started, WorkflowReply::Receipt { .. }),
        "{started:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["reviews"].as_array().unwrap().len() == 2
            && s["reviews"][1]["result"]["archive"]["classification"] == "clean"
            && s["reviews"][1]["job"]["state"] == "exited"
            && s["reviews"][1]["observation"]["observer"]["pid"].is_number()
        {
            let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
            assert_eq!(ledger.used(), 2);
            assert_eq!(ledger.corrections_used(), 1);
            assert_eq!(
                s["reviews"][1]["round"]["subject"]["round"],
                "code-review-02"
            );
            assert_ne!(
                s["reviews"][0]["round"]["subject"]["source"],
                s["reviews"][1]["round"]["subject"]["source"]
            );
            eprintln!("CONTROLLED_CORRECTION {}", s);
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "subsequent correction review did not qualify: {}",
            s["reviews"]
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let h = Journal::inspect(path).unwrap();
    let round: RevustRound = serde_json::from_value(
        h.receipts
            .iter()
            .filter_map(|r| r.request.as_ref())
            .find(|m| m.context["operation"] == "external_review")
            .unwrap()
            .context["round"]
            .clone(),
    )
    .unwrap();
    store::review::prelaunch_refusal_fixture(path, round);
    // A later source report after a clean round is not a findings correction.
    // It must admit a serial review of the new bytes without reusing old approval.
    std::fs::write(
        child_source.join("post-clean-change.txt"),
        "new source after clean review",
    )
    .unwrap();
    let (revision, _) = status(entity, cx, run, parent);
    report.request_id = "post-clean-child-report".into();
    report.expected_revision = revision;
    report.evidence[0].id = uuid::Uuid::new_v4();
    report.evidence[0].source = crate::root::workflow::source::capture(
        &child_source,
        report.evidence[0].source.base.as_deref(),
    )
    .unwrap();
    let WorkflowReply::Receipt { revision, .. } = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: report.clone(),
        },
        child,
    )) else {
        panic!("post-clean child report refused")
    };
    report.request_id = "parent-accept-post-clean-source".into();
    report.expected_revision = revision;
    let WorkflowReply::Receipt { revision, .. } = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint { run, spec: report },
        parent,
    )) else {
        panic!("post-clean parent checkpoint refused")
    };
    let next = ReviewRequest {
        request_id: "review-new-source-after-clean".into(),
        expected_revision: revision,
        ..spec
    };
    let reviewed = reply(ask(
        entity,
        cx,
        WorkflowCommand::Review { run, spec: next },
        parent,
    ));
    assert!(
        matches!(reviewed, WorkflowReply::Receipt { .. }),
        "completed clean owner must release admission: {reviewed:?}"
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["reviews"][2]["result"]["archive"]["classification"] == "clean"
            && s["reviews"][2]["job"]["state"] == "exited"
        {
            let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
            assert_eq!(ledger.used(), 3);
            assert_eq!(ledger.reserved(), 0);
            assert_eq!(ledger.corrections_used(), 1);
            assert_eq!(
                s["reviews"][2]["round"]["subject"]["round"],
                "code-review-03"
            );
            eprintln!("CONTROLLED_POST_CLEAN_REVIEW {}", s);
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "post-clean serial review did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let (revision, snapshot) = status(entity, cx, run, parent);
    let source: SourceIdentity = serde_json::from_value(snapshot["source"].clone()).unwrap();
    assert_ne!(
        source.target,
        entity
            .update(cx, |r, _| r.desk.session(parent).unwrap().cwd.clone())
            .canonicalize()
            .unwrap()
    );
    let contract = snapshot["contract"]["sha256"].as_str().unwrap().to_owned();
    let api_output = path.join("artifacts/accepted-public-status.json");
    std::fs::write(&api_output, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let mut accept:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":parent,"contract":contract,"source":source,"scenario":"accept","step":"accept","tool":{"program":"guild owning Root API fixture","version":"1","argv":["workflow","status"]},"observed_at_ms":1,"artifacts":[{"path":api_output,"sha256":digest(&std::fs::read(&api_output).unwrap())}],"observation":{"kind":"api","operation":"workflow.status","status":200}})).unwrap();
    let mut accept_request = CheckpointRequest {
        checks: None,
        version: 1,
        request_id: "accept-child-checkout".into(),
        expected_revision: revision,
        step: "accept".into(),
        outcome: "accept".into(),
        evidence: vec![accept.clone()],
    };
    let mut wrong = accept_request.clone();
    wrong.evidence[0].source = crate::root::workflow::source::capture(
        &entity.update(cx, |r, _| r.desk.session(parent).unwrap().cwd.clone()),
        source.base.as_deref(),
    )
    .unwrap();
    let wrong_reply = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint { run, spec: wrong },
        parent,
    ));
    assert!(
        matches!(&wrong_reply,WorkflowReply::Gap {message,..} if message.contains("source")),
        "{wrong_reply:?}"
    );
    accept_request.expected_revision = status(entity, cx, run, parent).0;
    let accepted = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: accept_request,
        },
        parent,
    ));
    assert!(
        matches!(accepted, WorkflowReply::Receipt { .. }),
        "{accepted:?}"
    );
    let (revision, snapshot) = status(entity, cx, run, parent);
    let description = path.join("artifacts/merge-request.md");
    std::fs::write(
        &description,
        "Controlled local delivery description, no publication.",
    )
    .unwrap();
    let branch =
        String::from_utf8(crate::git::run(&source.target, &["branch", "--show-current"]).unwrap())
            .unwrap();
    accept.id = uuid::Uuid::new_v4();
    accept.step = "delivery".into();
    accept.scenario = "delivery".into();
    accept.artifacts = vec![EvidenceArtifact {
        path: description.clone(),
        sha256: digest(&std::fs::read(&description).unwrap()),
    }];
    accept.observation = Observation::Delivery {
        branch: branch.trim().into(),
        description,
    };
    let delivered = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: CheckpointRequest {
                checks: None,
                version: 1,
                request_id: "deliver-child-checkout".into(),
                expected_revision: revision,
                step: "delivery".into(),
                outcome: "delivery".into(),
                evidence: vec![accept],
            },
        },
        parent,
    ));
    assert!(
        matches!(delivered, WorkflowReply::Receipt { .. }),
        "{delivered:?}"
    );
    let (_, final_snapshot) = status(entity, cx, run, parent);
    for step in ["actual-builder", "code-review", "accept", "delivery"] {
        assert_eq!(
            final_snapshot["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["step"] == step)
                .unwrap()["state"],
            "finished",
            "accepted child source must remain current for {step}"
        );
    }
    assert_eq!(snapshot["review_ledger"], final_snapshot["review_ledger"]);
    assert_eq!(
        std::fs::read_to_string(witness.join("correction-starts"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

/// Actual SDK contract findings are a parent checkpoint, never a child send.
pub(super) fn parent_contract(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    run: RunId,
    parent: SessionId,
    repo: &std::path::Path,
    path: &std::path::Path,
    request: ReviewRequest,
) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let (revision, s) = loop {
        tick(entity, cx);
        let (r, s) = status(entity, cx, run, parent);
        if s["reviews"][0]["result"]["archive"]["classification"] == "findings"
            && s["reviews"][0]["job"]["state"] == "exited"
            && s["reviews"][0]["observation"]["observer"]["pid"].is_number()
        {
            break (r, s);
        }
        assert!(
            std::time::Instant::now() < until,
            "contract findings unavailable: {}",
            s["reviews"]
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let attempt = serde_json::from_value(s["reviews"][0]["attempt"].clone()).unwrap();
    let retry = WorkflowCommand::Retry {
        run,
        spec: RetryRequest {
            version: 1,
            request_id: "original-parent-contract-correction".into(),
            expected_revision: revision,
            attempt,
        },
    };
    assert!(matches!(
        reply(ask(entity, cx, retry.clone(), parent)),
        WorkflowReply::Receipt { .. }
    ));
    assert!(matches!(
        reply(ask(entity, cx, retry, parent)),
        WorkflowReply::Receipt { .. }
    ));
    let (revision, s) = status(entity, cx, run, parent);
    assert_eq!(s["nodes"][0]["state"], "active");
    let h = Journal::inspect(path).unwrap();
    assert_eq!(
        h.receipts
            .iter()
            .filter(|r| r.observation.as_ref().is_some_and(|o| o["stage"]
                == "parent_checkpoint_required"
                && o["input_sent"] == false))
            .count(),
        1
    );
    let old = h
        .receipts
        .iter()
        .filter_map(|r| r.request.as_ref())
        .find(|m| m.context["operation"] == "checkpoint")
        .unwrap();
    let mut checkpoint: CheckpointRequest =
        serde_json::from_value(old.context["spec"].clone()).unwrap();
    checkpoint.request_id = "unchanged-contract-refused".into();
    checkpoint.expected_revision = revision;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Checkpoint {
                run,
                spec: checkpoint.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    let note = repo.join("task.md");
    let mut text = std::fs::read_to_string(&note).unwrap();
    text.push_str("\nControlled contract correction after checked findings.\n");
    std::fs::write(note, text).unwrap();
    let contract = pin_contract(repo, Some(std::path::Path::new("task.md")), None).unwrap();
    checkpoint.request_id = "changed-parent-contract".into();
    checkpoint.evidence[0].id = uuid::Uuid::new_v4();
    checkpoint.evidence[0].contract = contract.sha256;
    checkpoint.evidence[0].source = crate::root::workflow::source::capture(repo, None).unwrap();
    let WorkflowReply::Receipt { revision, .. } = reply(ask(
        entity,
        cx,
        WorkflowCommand::Checkpoint {
            run,
            spec: checkpoint,
        },
        parent,
    )) else {
        panic!("corrected contract refused")
    };
    let witness = PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    std::fs::write(witness.join("plan-corrected"), "actual contract changed").unwrap();
    let next = ReviewRequest {
        request_id: "contract-review-round-two".into(),
        expected_revision: revision,
        ..request
    };
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review { run, spec: next },
            parent
        )),
        WorkflowReply::Receipt { .. }
    ));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["reviews"][1]["result"]["archive"]["classification"] == "clean" {
            let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
            assert_eq!(ledger.used(), 2);
            assert_eq!(ledger.corrections_used(), 1);
            assert_ne!(
                s["reviews"][0]["round"]["subject"]["contract"],
                s["reviews"][1]["round"]["subject"]["contract"]
            );
            assert_eq!(
                s["reviews"][1]["round"]["subject"]["round"],
                "plan-review-02"
            );
            eprintln!("CONTROLLED_PARENT_CORRECTION {}", s);
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "contract re-review unavailable: {}",
            s["reviews"]
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Let the actual SDK finish while the Root callback clock is paused. Cancellation
/// must win even when a complete clean archive is already on disk.
pub(super) fn late_cancel(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    run: RunId,
    parent: SessionId,
    path: &std::path::Path,
) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        tick(entity, cx);
        let h = Journal::inspect(path).unwrap();
        if h.receipts.iter().any(|r| {
            r.observation.as_ref().is_some_and(|o| {
                o["operation"] == "review_job_observed" && o["job"]["state"] == "running"
            })
        }) {
            break;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    loop {
        tick(entity, cx);
        if entity.update(cx, |r, _| {
            r.workflows
                .as_ref()
                .is_some_and(|q| !q.in_flight && q.waiting.is_empty())
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "archive clock did not quiesce before fixture release"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let h = Journal::inspect(path).unwrap();
    let m = h
        .receipts
        .iter()
        .filter_map(|r| r.request.as_ref())
        .find(|m| m.context["operation"] == "external_review")
        .unwrap();
    let round: RevustRound = serde_json::from_value(m.context["round"].clone()).unwrap();
    let witness = PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    std::fs::write(witness.join("allow-review-finish"), "positive SDK release").unwrap();
    loop {
        let manifest = std::fs::read(round.archive.join("manifest.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        if manifest.is_some_and(|m| m["finished_at"].is_string()) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "SDK did not finish before cancel"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let command = WorkflowCommand::Cancel {
        run,
        spec: ControlRequest {
            version: 1,
            request_id: "cancel-before-archive-admission".into(),
            expected_revision: Journal::inspect(path).unwrap().revision,
        },
    };
    let (send, receive) = mpsc::channel();
    entity.update(cx, |r, cx| {
        r.command_workflow(&command, Some(&parent.to_string()), send, cx)
    });
    let answer = loop {
        tick(entity, cx);
        if let Ok(a) = receive.try_recv() {
            break reply(a);
        }
        assert!(std::time::Instant::now() < until, "cancel not observed");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(
        matches!(answer, WorkflowReply::Receipt { .. }),
        "{answer:?}"
    );
    loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["reviews"][0]["result"]["history_only"] == true
            && s["reviews"][0]["job"]["state"] == "exited"
        {
            assert!(
                s["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|n| n["state"] == "cancelled")
            );
            assert!(s["reviews"][0]["result"]["qualified"].is_null());
            let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
            assert_eq!(ledger.used(), 1);
            if let Some(lease) = round.environment {
                assert!(!lease.retained().unwrap());
            }
            eprintln!("CONTROLLED_CANCELLED_LATE_ARCHIVE {}", s);
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "cancelled graph advanced or late archive not retained: {}",
            s["reviews"]
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Actual owning Root/SDK import accounting survives refusals and later omission.
pub(super) fn durable_import(
    entity: &gpui::Entity<Root>,
    cx: &mut gpui::TestAppContext,
    subject: (RunId, SessionId),
    mut start: StartRequest,
    mut request: ReviewRequest,
    previous_snapshot: &serde_json::Value,
    contract: &str,
) {
    use guild_session::workflow::{ControlRequest, ReviewImport};
    let (previous, parent) = subject;
    let witness = PathBuf::from(std::env::var_os("WORKFLOW_WITNESS").unwrap());
    let archive = PathBuf::from(
        previous_snapshot["reviews"][0]["round"]["archive"]
            .as_str()
            .unwrap(),
    );
    let original_manifest = std::fs::read(archive.join("manifest.json")).unwrap();
    let original_events = std::fs::read(archive.join("revust/events.jsonl")).unwrap();
    let copy = witness.join("explicit-interrupted-archive-copy");
    std::fs::create_dir_all(copy.join("revust")).unwrap();
    std::fs::write(copy.join("manifest.json"), &original_manifest).unwrap();
    let lines = std::str::from_utf8(&original_events)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines.last().unwrap()).unwrap()["type"],
        "terminal"
    );
    std::fs::write(
        copy.join("revust/events.jsonl"),
        lines[..lines.len() - 1].join("\n") + "\n",
    )
    .unwrap();
    let (revision, _) = status(entity, cx, previous, parent);
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Cancel {
                run: previous,
                spec: ControlRequest {
                    version: 1,
                    request_id: "end-controlled-prior-subject".into(),
                    expected_revision: revision
                }
            },
            parent
        )),
        WorkflowReply::Receipt { .. }
    ));
    start.request_id = "native-durable-import-subject".into();
    let WorkflowReply::Receipt {
        run,
        revision,
        snapshot,
        ..
    } = reply(ask(
        entity,
        cx,
        WorkflowCommand::Start {
            spec: start.clone(),
        },
        parent,
    ))
    else {
        panic!("new controlled subject");
    };
    let path = start
        .target
        .join(".guild/workflow-runs")
        .join(run.to_string());
    std::fs::create_dir(path.join("artifacts")).unwrap();
    let out = path.join("artifacts/contract-test.txt");
    let note = start.target.join(start.selection.as_ref().unwrap());
    assert!(
        std::process::Command::new("/bin/test")
            .arg("-s")
            .arg(&note)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        &out,
        "Actual /bin/test -s passed on selected fixture contract",
    )
    .unwrap();
    let proof:Evidence=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"producer":parent,"contract":contract,"source":snapshot["source"],"scenario":"contract","step":"contract","tool":{"program":"/bin/test","version":"controlled host executable","argv":["-s",note]},"observed_at_ms":1,"artifacts":[{"path":out,"sha256":digest(&std::fs::read(&out).unwrap())}],"observation":{"kind":"contract","exit_code":0,"schema":"lightweight-note"}})).unwrap();
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Checkpoint {
                run,
                spec: CheckpointRequest {
                    checks: None,
                    version: 1,
                    request_id: "import-subject-contract".into(),
                    expected_revision: revision,
                    step: "contract".into(),
                    outcome: "contract".into(),
                    evidence: vec![proof]
                }
            },
            parent
        )),
        WorkflowReply::Receipt { .. }
    ));
    let prior = ReviewImport {
        id: "actual-controlled-prior".into(),
        archive: copy.clone(),
        source: previous_snapshot["source"]["sha256"]
            .as_str()
            .unwrap()
            .into(),
        contract: contract.into(),
        correction: false,
    };
    request.prior = vec![prior.clone()];
    request.request_id = "incomplete-prior".into();
    request.expected_revision = status(entity, cx, run, parent).0;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review {
                run,
                spec: request.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    let (revision, s) = status(entity, cx, run, parent);
    let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (1, 17, 0)
    );
    request.prior.clear();
    request.request_id = "omission-cannot-launch".into();
    request.expected_revision = revision;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review {
                run,
                spec: request.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(witness.join("review-starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    std::fs::write(copy.join("revust/events.jsonl"), &original_events).unwrap();
    request.prior = vec![prior];
    request.request_id = "complete-prior-exhausted-request".into();
    request.expected_revision = status(entity, cx, run, parent).0;
    request.allowance = 18;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review {
                run,
                spec: request.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    let (_, s) = status(entity, cx, run, parent);
    let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
    assert_eq!(
        (ledger.used(), ledger.reserved(), ledger.remaining()),
        (1, 0, 17)
    );
    let selected_package = request.package.clone();
    request.allowance = 1;
    request.package = witness.join("absent-package");
    request.request_id = "package-refusal-retains-prior".into();
    request.expected_revision = status(entity, cx, run, parent).0;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review {
                run,
                spec: request.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    request.package = selected_package;
    request.profile = Some("absent-private-profile".into());
    request.request_id = "preparation-refusal-retains-prior".into();
    request.expected_revision = status(entity, cx, run, parent).0;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review {
                run,
                spec: request.clone()
            },
            parent
        )),
        WorkflowReply::Gap { .. }
    ));
    let (_, s) = status(entity, cx, run, parent);
    assert_eq!(
        serde_json::from_value::<ReviewLedger>(s["review_ledger"].clone())
            .unwrap()
            .used(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(witness.join("review-starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    request.profile = Some("workflow-fixture".into());
    request.prior.clear();
    request.request_id = "qualified-next-fixed-round".into();
    request.expected_revision = status(entity, cx, run, parent).0;
    assert!(matches!(
        reply(ask(
            entity,
            cx,
            WorkflowCommand::Review { run, spec: request },
            parent
        )),
        WorkflowReply::Receipt { .. }
    ));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        tick(entity, cx);
        let (_, s) = status(entity, cx, run, parent);
        if s["nodes"][1]["state"] == "finished" && s["reviews"][0]["job"]["state"] == "exited" {
            let ledger: ReviewLedger = serde_json::from_value(s["review_ledger"].clone()).unwrap();
            assert_eq!((ledger.used(), ledger.reserved()), (2, 0));
            assert_eq!(
                std::fs::read_to_string(witness.join("review-starts"))
                    .unwrap()
                    .lines()
                    .count(),
                2
            );
            assert_eq!(
                std::fs::read(archive.join("manifest.json")).unwrap(),
                original_manifest
            );
            assert_eq!(
                std::fs::read(archive.join("revust/events.jsonl")).unwrap(),
                original_events
            );
            eprintln!("CONTROLLED_DURABLE_IMPORT {}", s);
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "imported accounting prevented qualified fixed round: {}",
            s
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
