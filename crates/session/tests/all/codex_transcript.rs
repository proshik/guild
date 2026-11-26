use guild_session::{codex_transcript, status::Signal};
use std::io::Write;

const ID: &str = "11111111-2222-3333-4444-555555555555";
fn rollout(home: &std::path::Path, id: &str, records: &str) -> std::path::PathBuf {
    let dir = home.join("sessions/2026/10/02");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("rollout-2026-10-02T12-00-00-{id}.jsonl"));
    std::fs::write(
        &path,
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n{records}"),
    )
    .unwrap();
    path
}
fn event(kind: &str) -> String {
    format!("{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"{kind}\"}}}}\n")
}
#[test]
fn exact_identity_and_assistant_text_never_read_a_neighbor() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(
        home.path(),
        ID,
        concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"First\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"First\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"text\":\"Private input\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Last\"}]}}\n"
        ),
    );
    rollout(
        home.path(),
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        &event("task_complete"),
    );
    assert_eq!(codex_transcript::path(home.path(), ID), Some(file.clone()));
    assert_eq!(codex_transcript::replies(&file, 10), ["First", "Last"]);
    assert_eq!(codex_transcript::replies(&file, 1), ["Last"]);
    assert!(codex_transcript::path(home.path(), "missing").is_none());
    std::fs::write(
        &file,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"different\"}}\n",
    )
    .unwrap();
    assert!(codex_transcript::path(home.path(), ID).is_none());
}
#[test]
fn a_resumed_process_does_not_replay_old_completion_and_partial_lines_wait() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(home.path(), ID, &event("task_complete"));
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    assert!(probe.poll(ID).is_empty());
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    append.write_all(event("task_started").as_bytes()).unwrap();
    assert_eq!(probe.poll(ID), [Signal::PromptSubmitted]);
    assert!(probe.poll(ID).is_empty(), "inactivity is not completion");
    let complete = event("task_complete");
    append
        .write_all(&complete.as_bytes()[..complete.len() - 1])
        .unwrap();
    assert!(probe.poll(ID).is_empty());
    append.write_all(b"\n").unwrap();
    assert_eq!(probe.poll(ID), [Signal::TurnEnded]);
    assert!(probe.poll(ID).is_empty());
}

#[test]
fn a_fresh_rollout_observes_only_explicit_completion_and_hooks_supersede_old_records() {
    let home = tempfile::tempdir().unwrap();
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    assert!(probe.poll(ID).is_empty());
    let file = rollout(home.path(), ID, &event("task_started"));
    assert_eq!(probe.poll(ID), [Signal::PromptSubmitted]);
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    append.write_all(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Still working\"}]}}\n").unwrap();
    assert!(probe.poll(ID).is_empty());
    append.write_all(event("task_complete").as_bytes()).unwrap();
    probe.baseline(ID); // A newer hook already reported the next turn.
    assert!(probe.poll(ID).is_empty());
    append.write_all(event("task_complete").as_bytes()).unwrap();
    assert_eq!(probe.poll(ID), [Signal::TurnEnded]);
    let other_dir = home.path().join("sessions/2026/10/03");
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::copy(&file, other_dir.join(file.file_name().unwrap())).unwrap();
    assert!(
        codex_transcript::path(home.path(), ID).is_none(),
        "ambiguous exact copies are not ranked by date"
    );
}

#[test]
fn a_large_backlog_does_not_publish_an_old_completion_before_a_new_start() {
    let home = tempfile::tempdir().unwrap();
    let mut records = event("task_complete");
    records.push_str(&format!(
        "{{\"ignored\":\"{}\"}}\n",
        "x".repeat(1024 * 1024)
    ));
    records.push_str(&event("task_started"));
    rollout(home.path(), ID, &records);
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    assert!(probe.poll(ID).is_empty());
    assert_eq!(probe.poll(ID), [Signal::TurnEnded, Signal::PromptSubmitted]);
}

fn token_count(at: &str, limit_id: &str, used: f64) -> String {
    format!(
        "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":null,\
         \"rate_limits\":{{\"limit_id\":\"{limit_id}\",\"primary\":{{\"used_percent\":{used},\"window_minutes\":10080,\"resets_at\":1791580401}},\"secondary\":null}}}}}}\n"
    )
}

/// The plan's limits ride on every `token_count` Codex records; the cursor
/// that reads the turns keeps the latest of the plan's own (`limit_id`
/// `codex`) and hands it over once, with the line's time.
#[test]
fn the_latest_rate_limits_are_kept_and_handed_over_once() {
    let home = tempfile::tempdir().unwrap();
    let mut records = token_count("2026-10-03T11:30:00.000Z", "codex", 7.0);
    records.push_str(&event("task_started"));
    records.push_str(&token_count("2026-10-03T11:38:40.010Z", "codex", 11.0));
    records.push_str(&token_count(
        "2026-10-03T11:38:41.000Z",
        "codex_other",
        90.0,
    ));
    let file = rollout(home.path(), ID, &records);
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);

    probe.poll(ID);
    let limits = probe.take_rate_limits().expect("seen");
    assert_eq!(limits.at, "2026-10-03T11:38:40.010Z");
    assert_eq!(limits.limits["primary"]["used_percent"], 11.0);
    assert!(probe.take_rate_limits().is_none(), "handed over once");

    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    append
        .write_all(token_count("2026-10-03T11:40:00.000Z", "codex", 12.0).as_bytes())
        .unwrap();
    probe.poll(ID);
    assert_eq!(
        probe.take_rate_limits().expect("a new one").limits["primary"]["used_percent"],
        12.0
    );
}

/// The incident had a failed nested exec CommandExecution. Its outer code-mode
/// custom tool output has a different ID and cannot answer this permission.
#[test]
fn codex_failed_nested_command_is_authoritative_tool_completion() {
    let home = tempfile::tempdir().unwrap();
    let records = format!(
        "{}{}\n",
        event("task_started"),
        serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":ID,"turn_id":"turn-current","item":{"type":"CommandExecution","id":"exec-failed","status":"failed","exit_code":101,"source":"unified_exec_startup"}}})
    );
    rollout(home.path(), ID, &records);
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    assert_eq!(
        probe.poll(ID),
        [Signal::PromptSubmitted, Signal::ToolFinished]
    );
}

fn turn_event(kind: &str, turn: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({"type":"event_msg","payload":{"type":kind,"turn_id":turn}})
    )
}

#[test]
fn codex_hook_overlap_keeps_completion_and_does_not_replay_a_start_or_unread_result() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(home.path(), ID, &turn_event("task_started", "previous"));
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    let prompt = serde_json::json!({"codex_session":ID,"turn_id":"current"});
    assert_eq!(
        probe.hook_signal(&prompt, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    assert_eq!(
        probe.hook_signal(&prompt, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
    let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    write!(
        append,
        "{}{}{}",
        turn_event("task_complete", "previous"),
        turn_event("task_started", "current"),
        turn_event("task_complete", "current")
    )
    .unwrap();
    assert_eq!(
        probe.poll(ID),
        [Signal::TurnEnded],
        "old records and same-turn start cannot answer waiting"
    );
    for signal in [
        Signal::PromptSubmitted,
        Signal::PermissionAsked,
        Signal::TurnCompleted,
        Signal::ToolStarted,
    ] {
        assert_eq!(
            probe.hook_signal(&prompt, signal),
            None,
            "settled turn cannot recreate status"
        );
    }
    append
        .write_all(turn_event("task_started", "next").as_bytes())
        .unwrap();
    assert_eq!(probe.poll(ID), [Signal::PromptSubmitted]);
    assert_eq!(
        probe.hook_signal(&prompt, Signal::PromptSubmitted),
        None,
        "retired hook cannot replace next turn"
    );
}

#[test]
fn codex_tentative_stop_requires_disk_completion() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(home.path(), ID, "");
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    let prompt = serde_json::json!({"codex_session":ID,"turn_id":"current"});
    assert_eq!(
        probe.hook_signal(&prompt, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    assert_eq!(probe.hook_signal(&prompt, Signal::TurnCompleted), None);
    let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    write!(
        append,
        "{}{}",
        turn_event("task_started", "current"),
        turn_event("task_complete", "current")
    )
    .unwrap();
    assert_eq!(probe.poll(ID), [Signal::TurnEnded]);
    assert!(probe.poll(ID).is_empty());
}

#[test]
fn codex_structured_completion_resolves_only_the_exact_turn_and_tool() {
    use guild_session::harness::CodexHarness;
    use guild_session::status::{Latch, Status, Why};
    let home = tempfile::tempdir().unwrap();
    for status in ["completed", "failed", "declined"] {
        let file = rollout(home.path(), ID, "");
        let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
        let session = guild_session::SessionId::new();
        let mut board = Latch::default();
        for (event, id) in [
            ("UserPromptSubmit", None),
            ("PreToolUse", Some("exec-approved")),
            ("PermissionRequest", None),
        ] {
            let mut p = serde_json::json!({"session_id":session.to_string(),"codex_session":ID,"turn_id":"current","hook_event_name":event,"tool_name":"Bash","tool_input":{"command":"cargo check"}});
            if let Some(id) = id {
                p["tool_use_id"] = id.into();
            }
            let (_, signal) = CodexHarness.status_from_payload(&p).unwrap();
            if let Some(signal) = probe.hook_signal(&p, signal) {
                board.apply_payload(session, signal, &p);
            }
        }
        let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
        append
            .write_all(turn_event("task_started", "current").as_bytes())
            .unwrap();
        for (thread, turn, id, item_status) in [
            (
                "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                "current",
                "exec-approved",
                "completed",
            ),
            (ID, "old", "exec-approved", "completed"),
            (ID, "current", "parallel", "failed"),
            (ID, "current", "exec-approved", "in_progress"),
            (ID, "current", "exec-approved", status),
        ] {
            writeln!(append, "{}", serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":thread,"turn_id":turn,"item":{"type":"CommandExecution","id":id,"status":item_status,"exit_code":101,"aggregated_output":"private content is not copied"}}})).unwrap();
            let evidence = probe.poll_evidence(ID);
            for e in evidence {
                assert!(e.payload.get("item").is_none());
                assert!(e.payload.get("aggregated_output").is_none());
                board.apply_payload(session, e.signal, &e.payload);
            }
            assert_eq!(
                board.status(session),
                if thread == ID
                    && turn == "current"
                    && id == "exec-approved"
                    && item_status != "in_progress"
                {
                    Status::Working
                } else {
                    Status::Waiting(Why::Permission)
                }
            );
        }
        board.apply(session, Signal::Interrupted);
        append
            .write_all(turn_event("task_complete", "current").as_bytes())
            .unwrap();
        for e in probe.poll_evidence(ID) {
            board.apply(
                session,
                if e.signal == Signal::TurnEnded {
                    Signal::TurnCompleted
                } else {
                    e.signal
                },
            );
        }
        assert_eq!(
            board.status(session),
            Status::Idle,
            "late completion cannot undo interrupt"
        );
    }
}

#[test]
fn codex_blocking_stop_keeps_same_turn_activity_until_authoritative_completion() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(home.path(), ID, "");
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    let p = serde_json::json!({"codex_session":ID,"turn_id":"continued"});
    assert_eq!(
        probe.hook_signal(&p, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    // Guild cannot know the outcome of the remaining Stop hooks.
    assert_eq!(probe.hook_signal(&p, Signal::TurnCompleted), None);
    assert_eq!(
        probe.hook_signal(&p, Signal::ToolStarted),
        Some(Signal::ToolStarted)
    );
    assert_eq!(
        probe.hook_signal(&p, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
    let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    write!(
        append,
        "{}{}",
        turn_event("task_started", "continued"),
        turn_event("task_complete", "continued")
    )
    .unwrap();
    assert_eq!(probe.poll(ID), [Signal::TurnEnded]);
    for signal in [
        Signal::ToolStarted,
        Signal::PermissionAsked,
        Signal::TurnCompleted,
    ] {
        assert_eq!(probe.hook_signal(&p, signal), None);
    }
}

#[test]
fn codex_hook_named_resume_retains_only_current_turn_completion() {
    let home = tempfile::tempdir().unwrap();
    rollout(home.path(), ID, "");
    let other = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let completion = serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":other,"turn_id":"fresh","item":{"type":"CommandExecution","id":"exec-fresh","status":"failed","exit_code":101}}});
    let file = rollout(
        home.path(),
        other,
        &format!(
            "{}{}{}{}\n",
            turn_event("task_started", "history"),
            turn_event("task_complete", "history"),
            turn_event("task_started", "fresh"),
            completion
        ),
    );
    let mut without_hook = codex_transcript::Probe::new(home.path().into(), Some(ID));
    assert!(
        without_hook.poll(other).is_empty(),
        "a switch without a current-turn hook retains its EOF baseline"
    );
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    let p = serde_json::json!({"codex_session":other,"turn_id":"fresh"});
    assert_eq!(
        probe.hook_signal(&p, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    assert_eq!(
        probe.hook_signal(&p, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
    let evidence = probe.poll_evidence(other);
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].signal, Signal::ToolFinished);
    assert_eq!(evidence[0].payload["tool_use_id"], "exec-fresh");
    assert!(probe.poll(other).is_empty());
    let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    append
        .write_all(turn_event("task_complete", "fresh").as_bytes())
        .unwrap();
    assert_eq!(probe.poll(other), [Signal::TurnEnded]);
    assert_eq!(probe.hook_signal(&p, Signal::PermissionAsked), None);
}

#[test]
fn codex_new_hook_turn_during_history_scan_retains_its_completion() {
    let home = tempfile::tempdir().unwrap();
    rollout(home.path(), ID, "");
    let other = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let records = format!(
        "{}{}\n{}",
        turn_event("task_started", "history"),
        serde_json::json!({"ignored":"x".repeat(1024*1024)}),
        turn_event("task_started", "first")
    );
    let file = rollout(home.path(), other, &records);
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    let first = serde_json::json!({"codex_session":other,"turn_id":"first"});
    assert_eq!(
        probe.hook_signal(&first, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    assert!(
        probe.poll(other).is_empty(),
        "history scan is bounded, not yet at EOF"
    );
    let second = serde_json::json!({"codex_session":other,"turn_id":"second"});
    assert_eq!(
        probe.hook_signal(&second, Signal::PromptSubmitted),
        Some(Signal::PromptSubmitted)
    );
    assert_eq!(
        probe.hook_signal(&second, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
    let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
    write!(append, "{}{}\n{}", turn_event("task_started", "second"), serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":other,"turn_id":"second","item":{"type":"CommandExecution","id":"exec-second","status":"failed","exit_code":101}}}), turn_event("task_complete", "second")).unwrap();
    let evidence = probe.poll_evidence(other);
    assert_eq!(
        evidence.iter().map(|v| v.signal).collect::<Vec<_>>(),
        [Signal::ToolFinished, Signal::TurnEnded]
    );
    assert!(evidence.iter().all(|v| v.payload["turn_id"] == "second"));
    assert_eq!(probe.hook_signal(&first, Signal::PermissionAsked), None);
    assert_eq!(probe.hook_signal(&second, Signal::PermissionAsked), None);
    assert!(probe.poll(other).is_empty());
}

#[test]
fn codex_unread_tail_truncation_drops_escape_boundary_without_replaying_buffered_starts() {
    use guild_session::{
        SessionId,
        status::{Latch, Status},
    };
    for buffered in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let prefix = if buffered {
            format!(
                "{}{}\n",
                turn_event("task_started", "old"),
                serde_json::json!({"ignored":"x".repeat(1024*1024)})
            )
        } else {
            turn_event("task_started", "old")
        };
        let file = rollout(home.path(), ID, &prefix);
        let retained = std::fs::metadata(&file).unwrap().len();
        let mut append = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        if buffered {
            writeln!(
                append,
                "{}",
                serde_json::json!({"ignored":"unread".repeat(2000)})
            )
            .unwrap();
        }
        let mut probe = codex_transcript::Probe::new(home.path().into(), None);
        if buffered {
            assert!(
                probe.poll(ID).is_empty(),
                "old start is pending before bounded scan reaches EOF"
            );
        } else {
            assert_eq!(probe.poll(ID), [Signal::PromptSubmitted]);
            writeln!(
                append,
                "{}",
                serde_json::json!({"ignored":"unread".repeat(2000)})
            )
            .unwrap();
        }
        probe.local_interrupt();
        let session = SessionId::new();
        let mut board = Latch::default();
        board.apply(session, Signal::PromptSubmitted);
        board.apply(session, Signal::Interrupted);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(retained)
            .unwrap();
        write!(
            append,
            "{}{}",
            turn_event("task_started", "new"),
            turn_event("task_complete", "new")
        )
        .unwrap();
        let evidence = probe.poll_evidence(ID);
        assert_eq!(
            evidence
                .iter()
                .map(|v| (v.signal, v.payload["turn_id"].as_str().unwrap()))
                .collect::<Vec<_>>(),
            [(Signal::PromptSubmitted, "new"), (Signal::TurnEnded, "new")],
            "buffered={buffered}"
        );
        for event in evidence {
            let signal = if event.signal == Signal::TurnEnded {
                Signal::TurnCompleted
            } else {
                event.signal
            };
            board.apply_payload(session, signal, &event.payload);
        }
        assert_eq!(board.status(session), Status::Done);
        assert!(probe.poll(ID).is_empty());
    }
}

#[test]
fn automatic_review_is_working_without_hiding_manual_or_user_questions() {
    use guild_session::{
        SessionId,
        status::{Latch, Status, Why},
    };
    // The real CLI emits PermissionRequest before automatic review, and no
    // resolution hook when execution begins. Waiting until completion is wrong.
    for (reviewer, context_turn, tool, expected) in [
        (Some("auto_review"), "current", "Bash", Status::Working),
        (
            Some("auto_review"),
            "current",
            "apply_patch",
            Status::Working,
        ),
        (
            Some("user"),
            "current",
            "Bash",
            Status::Waiting(Why::Permission),
        ),
        (None, "current", "Bash", Status::Waiting(Why::Permission)),
        (
            Some("auto_review"),
            "old",
            "Bash",
            Status::Waiting(Why::Permission),
        ),
        (
            Some("auto_review"),
            "current",
            "mcp__example__write",
            Status::Waiting(Why::Permission),
        ),
        (
            Some("auto_review"),
            "current",
            "AskUserQuestion",
            Status::Waiting(Why::Question),
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let context = serde_json::json!({"type":"turn_context","payload":{"turn_id":context_turn,"approvals_reviewer":reviewer}});
        rollout(
            home.path(),
            ID,
            &format!("{}{}\n", turn_event("task_started", "current"), context),
        );
        let mut probe = codex_transcript::Probe::new(home.path().into(), None);
        let session = SessionId::new();
        let mut board = Latch::default();
        for (signal, event) in [
            (Signal::PromptSubmitted, "UserPromptSubmit"),
            (Signal::PermissionAsked, "PermissionRequest"),
        ] {
            let payload = serde_json::json!({"codex_session":ID,"turn_id":"current","hook_event_name":event,"tool_name":tool});
            let signal = if tool == "AskUserQuestion" && signal == Signal::PermissionAsked {
                Signal::QuestionAsked
            } else {
                signal
            };
            if let Some(signal) = probe.hook_signal(&payload, signal) {
                board.apply_payload(session, signal, &payload);
            }
        }
        assert_eq!(
            board.status(session),
            expected,
            "reviewer={reviewer:?}, turn={context_turn}, tool={tool}"
        );
    }
}

#[test]
fn automatic_review_requires_latest_complete_context_of_exact_conversation() {
    let auto = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"auto_review"}}).to_string();
    let manual = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"user"}}).to_string();
    for (records, expected) in [
        (format!("{auto}\n{manual}\n"), Some(Signal::PermissionAsked)),
        (format!("{manual}\n{auto}"), Some(Signal::PermissionAsked)),
        (format!("{manual}\n{auto}\n"), None),
        (
            format!(
                "{auto}\n{}\n",
                serde_json::json!({"type":"turn_context","payload":{"turn_id":"other","approvals_reviewer":"auto_review"}})
            ),
            Some(Signal::PermissionAsked),
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        rollout(home.path(), ID, &records);
        let mut probe = codex_transcript::Probe::new(home.path().into(), None);
        let payload =
            serde_json::json!({"codex_session":ID,"turn_id":"current","tool_name":"Bash"});
        assert_eq!(
            probe.hook_signal(&payload, Signal::PermissionAsked),
            expected
        );
        let mut neighbor = payload.clone();
        neighbor["codex_session"] = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into();
        assert_eq!(
            probe.hook_signal(&neighbor, Signal::PermissionAsked),
            Some(Signal::PermissionAsked)
        );
    }
}

#[test]
fn automatic_review_survives_large_output_in_the_same_turn() {
    use guild_session::{
        SessionId,
        status::{Latch, Status},
    };
    let home = tempfile::tempdir().unwrap();
    let context = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"auto_review"}});
    let file = rollout(
        home.path(),
        ID,
        &format!("{}{}\n", turn_event("task_started", "current"), context),
    );
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    let session = SessionId::new();
    let mut board = Latch::default();
    let payload = serde_json::json!({"codex_session":ID,"turn_id":"current","tool_name":"Bash"});
    board.apply_payload(session, Signal::PromptSubmitted, &payload);
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    let output = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","output":"x".repeat(2 * 1024 * 1024)}});
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap(),
        "{output}"
    )
    .unwrap();
    if let Some(signal) = probe.hook_signal(&payload, Signal::PermissionAsked) {
        board.apply_payload(session, signal, &payload);
    }
    assert_eq!(board.status(session), Status::Working);
}

#[test]
fn resumed_long_turn_retains_policy_without_replaying_lifecycle() {
    let home = tempfile::tempdir().unwrap();
    let context = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"auto_review"}});
    let output = serde_json::json!({"type":"response_item","payload":{"output":"x".repeat(2 * 1024 * 1024)}});
    rollout(
        home.path(),
        ID,
        &format!(
            "{}{context}\n{output}\n",
            turn_event("task_started", "current")
        ),
    );
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    assert!(probe.poll(ID).is_empty());
    let payload = serde_json::json!({"codex_session":ID,"turn_id":"current","tool_name":"Bash"});
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    assert!(probe.poll(ID).is_empty());
}

#[test]
fn remembered_policy_follows_complete_updates_and_observed_truncation() {
    let home = tempfile::tempdir().unwrap();
    let auto = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"auto_review"}}).to_string();
    let manual = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"user"}}).to_string();
    let file = rollout(home.path(), ID, &format!("{auto}\n"));
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    let payload = serde_json::json!({"codex_session":ID,"turn_id":"current","tool_name":"Bash"});
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    write!(append, "{manual}").unwrap();
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    writeln!(append).unwrap();
    assert_eq!(
        probe.hook_signal(&payload, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
    writeln!(append, "{auto}").unwrap();
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    drop(append);
    rollout(home.path(), ID, &format!("{manual}\n"));
    assert_eq!(
        probe.hook_signal(&payload, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
}

#[test]
fn malformed_policy_update_does_not_reuse_automatic_review() {
    let home = tempfile::tempdir().unwrap();
    let context = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":"auto_review"}});
    let file = rollout(home.path(), ID, &format!("{context}\n"));
    let mut probe = codex_transcript::Probe::new(home.path().into(), None);
    let payload = serde_json::json!({"codex_session":ID,"turn_id":"current","tool_name":"Bash"});
    assert_eq!(probe.hook_signal(&payload, Signal::PermissionAsked), None);
    let context = serde_json::json!({"type":"turn_context","payload":{"turn_id":"current","approvals_reviewer":true}});
    writeln!(
        std::fs::OpenOptions::new().append(true).open(file).unwrap(),
        "{context}"
    )
    .unwrap();
    assert_eq!(
        probe.hook_signal(&payload, Signal::PermissionAsked),
        Some(Signal::PermissionAsked)
    );
}

#[test]
fn codex_mcp_completion_resolves_only_its_wait_after_tool_failure() {
    use guild_session::{
        SessionId,
        status::{Latch, Status, Why},
    };
    for status in ["completed", "failed"] {
        let home = tempfile::tempdir().unwrap();
        let file = rollout(home.path(), ID, "");
        let mut probe = codex_transcript::Probe::new(home.path().into(), None);
        let session = SessionId::new();
        let mut board = Latch::default();
        for (signal, event) in [
            (Signal::PromptSubmitted, "UserPromptSubmit"),
            (Signal::ToolStarted, "PreToolUse"),
            (Signal::PermissionAsked, "PermissionRequest"),
        ] {
            let mut payload = serde_json::json!({"codex_session":ID,"turn_id":"current","hook_event_name":event,"tool_name":"mcp__playwright__browser_run_code_unsafe","tool_input":{"code":"async () => {}"}});
            if signal == Signal::ToolStarted {
                payload["tool_use_id"] = "exec-browser".into();
            }
            if let Some(signal) = probe.hook_signal(&payload, signal) {
                board.apply_payload(session, signal, &payload);
            }
        }
        assert_eq!(board.status(session), Status::Waiting(Why::Permission));
        let mut append = std::fs::OpenOptions::new().append(true).open(file).unwrap();
        for (thread, turn, id, state, expected) in [
            (
                ID,
                "current",
                "exec-other",
                "completed",
                Status::Waiting(Why::Permission),
            ),
            (
                "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                "current",
                "exec-browser",
                status,
                Status::Waiting(Why::Permission),
            ),
            (
                ID,
                "old",
                "exec-browser",
                status,
                Status::Waiting(Why::Permission),
            ),
            (
                ID,
                "current",
                "exec-browser",
                "inProgress",
                Status::Waiting(Why::Permission),
            ),
            (ID, "current", "exec-browser", status, Status::Working),
        ] {
            writeln!(append, "{}", serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":thread,"turn_id":turn,"item":{"type":"McpToolCall","id":id,"server":"playwright","tool":"browser_run_code_unsafe","arguments":{"code":"async () => {}"},"status":state}}})).unwrap();
            for evidence in probe.poll_evidence(ID) {
                board.apply_payload(session, evidence.signal, &evidence.payload);
            }
            assert_eq!(
                board.status(session),
                expected,
                "status={status}, thread={thread}, turn={turn}, id={id}, state={state}"
            );
        }
    }
}

#[test]
fn runtime_start_witness_survives_status_dedup_and_late_local_hook() {
    for hook_first in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let file = rollout(home.path(), ID, "");
        let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
        let payload = serde_json::json!({"codex_session":ID,"turn_id":"owned"});
        if hook_first {
            assert_eq!(
                probe.hook_signal(&payload, Signal::ToolStarted),
                Some(Signal::ToolStarted)
            );
            probe.pin_runtime_turn("owned");
        }
        let mut writer = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        write!(
            writer,
            "{}{}",
            turn_event("task_started", "owned"),
            turn_event("task_complete", "owned")
        )
        .unwrap();
        let signals = probe.poll_evidence(ID);
        if hook_first {
            assert_eq!(
                signals.iter().map(|e| e.signal).collect::<Vec<_>>(),
                [Signal::TurnEnded]
            );
        } else {
            assert_eq!(
                probe.runtime_completion(),
                None,
                "shared history alone is not a runtime witness"
            );
            assert_eq!(
                probe.hook_signal(&payload, Signal::PromptSubmitted),
                None,
                "late status duplicates stay suppressed"
            );
            probe.pin_runtime_turn("owned");
        }
        assert_eq!(probe.take_runtime_starts(), ["owned"]);
        assert_eq!(probe.runtime_completion(), Some("owned"));
        probe.baseline_managed(ID);
        assert_eq!(
            probe.hook_signal(&payload, Signal::ToolStarted),
            None,
            "old launch hook cannot bind the new input epoch"
        );
        probe.pin_runtime_turn("owned");
        assert!(!probe.owns_runtime_turn("owned"));
        assert_eq!(probe.runtime_completion(), None);
    }
}

#[test]
fn runtime_permission_cannot_be_answered_by_shared_history() {
    let home = tempfile::tempdir().unwrap();
    let file = rollout(home.path(), ID, "");
    let mut probe = codex_transcript::Probe::new(home.path().into(), Some(ID));
    let payload = serde_json::json!({"codex_session":ID,"turn_id":"owned"});
    probe.hook_signal(&payload, Signal::PromptSubmitted);
    probe.pin_runtime_turn("owned");
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    write!(writer, "{}", turn_event("task_started", "owned")).unwrap();
    probe.poll_evidence(ID);
    probe.take_runtime_starts();
    probe.hook_signal(&payload, Signal::PermissionAsked);
    write!(
        writer,
        "{}{}{}",
        turn_event("task_started", "foreign"),
        turn_event("task_complete", "foreign"),
        turn_event("task_complete", "owned")
    )
    .unwrap();
    assert!(probe.poll_runtime_evidence(ID, true).is_empty());
    assert_eq!(probe.runtime_completion(), None);
    assert!(
        probe.poll_runtime_evidence(ID, false).is_empty(),
        "consumed foreign completion cannot become ready after holder exit"
    );
    assert_eq!(probe.runtime_completion(), None);
    assert_eq!(
        probe.hook_signal(&payload, Signal::ToolFinished),
        Some(Signal::ToolFinished),
        "shared completion must not settle before local permission resolution"
    );
}
