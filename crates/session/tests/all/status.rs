//! What the agent in a tab is doing — M5.10.
//!
//! The table is decision 7 of the plan, row by row. The payloads are
//! **recorded from a live `claude` 2.1.268** by task 0, with the paths
//! rewritten; their shape is taken from disk rather than from memory,
//! which is the lesson of the state file's `records` key.

use guild_session::SessionId;
use guild_session::status::{Signal, Signal::*, Status::*, Why, from_hook};

fn payload(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("a recorded payload is JSON")
}

#[test]
fn every_row_of_the_table() {
    assert_eq!(Idle.next(PromptSubmitted), Working);
    assert_eq!(Done.next(PromptSubmitted), Working);
    assert_eq!(Waiting(Why::Permission).next(PromptSubmitted), Working);
    assert_eq!(Idle.next(ToolStarted), Working);
    assert_eq!(Working.next(PermissionAsked), Waiting(Why::Permission));
    assert_eq!(Working.next(QuestionAsked), Waiting(Why::Question));
    assert_eq!(
        Waiting(Why::Permission).next(ToolFinished),
        Waiting(Why::Permission)
    );
    assert_eq!(
        Waiting(Why::Question).next(ToolFinished),
        Waiting(Why::Question)
    );
    assert_eq!(Working.next(TurnEnded), Done);
    assert_eq!(
        Waiting(Why::Question).next(TurnEnded),
        Waiting(Why::Question)
    );
    assert_eq!(Working.next(SessionEnded), Idle);
    assert_eq!(Done.next(SessionEnded), Idle);
    assert_eq!(Working.next(Interrupted), Idle);
    assert_eq!(Waiting(Why::Permission).next(Interrupted), Idle);
    assert_eq!(Done.next(Seen), Idle);
}

#[test]
fn a_tool_starting_does_not_answer_a_wait() {
    // A parallel call's PreToolUse can land while another waits.
    assert_eq!(
        Waiting(Why::Permission).next(ToolStarted),
        Waiting(Why::Permission)
    );
}

#[test]
fn looking_at_a_waiting_agent_does_not_answer_it() {
    assert_eq!(
        Waiting(Why::Permission).next(Seen),
        Waiting(Why::Permission)
    );
    assert_eq!(Working.next(Seen), Working);
    assert_eq!(Idle.next(Seen), Idle);
}

#[test]
fn a_late_post_tool_use_does_not_restart_a_finished_turn() {
    // PostToolUse is asynchronous, so it promises no order against Stop.
    assert_eq!(Done.next(ToolFinished), Done);
    assert_eq!(Idle.next(ToolFinished), Idle);
}

#[test]
fn an_interrupt_leaves_a_finished_turn_finished() {
    assert_eq!(Done.next(Interrupted), Done);
    assert_eq!(Idle.next(Interrupted), Idle);
}

#[test]
fn what_a_status_asks_of_the_person() {
    use guild_session::status::Need;

    assert_eq!(Idle.need(), None);
    assert_eq!(Working.need(), None);
    assert_eq!(Waiting(Why::Question).need(), Some(Need::Blocked));
    assert_eq!(Done.need(), Some(Need::Done));
    assert!(Need::Blocked > Need::Done);
    assert!(Working.working() && !Done.working());
}

#[test]
fn the_most_urgent_is_the_one_blocked() {
    let mut order = [Done, Idle, Waiting(Why::Permission), Working];
    order.sort_by_key(|status| status.urgency());
    assert_eq!(order, [Idle, Working, Done, Waiting(Why::Permission)]);
}

#[test]
fn every_recorded_hook_names_its_signal() {
    let cases: [(&str, Signal); 8] = [
        (
            include_str!("../fixtures/hooks/user-prompt-submit.json"),
            PromptSubmitted,
        ),
        (
            include_str!("../fixtures/hooks/pre-tool-use-bash.json"),
            ToolStarted,
        ),
        (
            include_str!("../fixtures/hooks/permission-request.json"),
            PermissionAsked,
        ),
        (
            include_str!("../fixtures/hooks/post-tool-use.json"),
            ToolFinished,
        ),
        (
            include_str!("../fixtures/hooks/pre-tool-use-ask.json"),
            QuestionAsked,
        ),
        (include_str!("../fixtures/hooks/stop.json"), TurnEnded),
        (
            include_str!("../fixtures/hooks/session-end.json"),
            SessionEnded,
        ),
        // Task 0: AskUserQuestion asks for permission as well, and that
        // permission is the question, not a second reason to wait.
        (
            include_str!("../fixtures/hooks/permission-request-ask.json"),
            QuestionAsked,
        ),
    ];
    for (json, signal) in cases {
        let payload = payload(json);
        let (session, got) = from_hook(&payload).expect("a recorded hook is understood");
        assert_eq!(got, signal, "{}", payload["hook_event_name"]);
        assert_eq!(
            session.to_string(),
            payload["session_id"].as_str().unwrap(),
            "the session is the one the payload names"
        );
    }
}

#[test]
fn a_hook_without_a_session_is_nothing() {
    let payload = serde_json::json!({"hook_event_name": "Stop"});
    assert_eq!(from_hook(&payload), None);
    let payload = serde_json::json!({"hook_event_name": "Stop", "session_id": "not-a-uuid"});
    assert_eq!(from_hook(&payload), None);
}

#[test]
fn a_hook_nobody_asked_for_is_nothing() {
    // A newer CLI, or a hook the person's own settings add: not an error.
    let payload = serde_json::json!({
        "hook_event_name": "PreCompact",
        "session_id": SessionId::new().to_string(),
    });
    assert_eq!(from_hook(&payload), None);
}

#[test]
fn a_session_id_reads_back_what_it_writes() {
    let id = SessionId::new();
    assert_eq!(id.to_string().parse::<SessionId>(), Ok(id));
    assert!("not-a-uuid".parse::<SessionId>().is_err());
}

mod board {
    use super::*;
    use guild_session::status::{Board, Counts, Kind, Reported};

    fn sessions<const N: usize>() -> [SessionId; N] {
        std::array::from_fn(|_| SessionId::new())
    }

    const ASKS: Reported = Reported::Blocked(Some(Kind::Permission));
    const QUESTION: Reported = Reported::Blocked(Some(Kind::Question));

    fn report(board: &mut Board, session: SessionId, record: Reported) -> bool {
        board.report(session, Some(record)).changed
    }

    #[test]
    fn it_answers_whether_anything_changed() {
        // So the window saves and redraws only when something did.
        let mut board = Board::default();
        let [a] = sessions();
        assert!(report(&mut board, a, Reported::Working));
        assert!(!report(&mut board, a, Reported::Working), "a repeat");
        assert_eq!(board.status(a), Working);
        assert!(report(&mut board, a, Reported::Done));
        assert_eq!(board.status(a), Done);
    }

    #[test]
    fn a_session_nothing_has_mentioned_is_idle() {
        assert_eq!(Board::default().status(SessionId::new()), Idle);
    }

    #[test]
    fn a_forgotten_session_is_idle_again() {
        let mut board = Board::default();
        let [a] = sessions();
        report(&mut board, a, ASKS);
        board.forget(a);
        assert_eq!(board.status(a), Idle);
        assert_eq!(board.counts(), Counts::default());
    }

    #[test]
    fn a_project_shows_the_most_urgent_of_its_sessions() {
        let mut board = Board::default();
        let [working, done, waiting] = sessions();
        report(&mut board, working, Reported::Working);
        report(&mut board, done, Reported::Done);
        report(&mut board, waiting, ASKS);
        assert_eq!(
            board.most_urgent([working, done, waiting]),
            Waiting(Why::Permission)
        );
        assert_eq!(board.most_urgent([working, done]), Done);
        assert_eq!(board.most_urgent([working]), Working);
        assert_eq!(board.most_urgent([]), Idle);
    }

    #[test]
    fn it_counts_the_blocked_the_finished_and_the_working() {
        let mut board = Board::default();
        let [a, b, c, d, e] = sessions();
        report(&mut board, a, ASKS);
        report(&mut board, b, Reported::Done);
        report(&mut board, c, Reported::Done);
        report(&mut board, d, Reported::Working);
        report(&mut board, e, Reported::Idle);
        assert_eq!(
            board.counts(),
            Counts {
                blocked: 1,
                done: 2,
                working: 1,
                failed: 0,
            }
        );
    }

    #[test]
    fn the_blocked_come_first_then_the_finished_each_oldest_first() {
        let mut board = Board::default();
        let s: [SessionId; 4] = sessions();
        report(&mut board, s[0], Reported::Done);
        report(&mut board, s[1], ASKS);
        report(&mut board, s[2], Reported::Done);
        report(&mut board, s[3], QUESTION);

        let mut visited = Vec::new();
        let mut current = None;
        for _ in 0..5 {
            current = board.next_waiting(current, &s);
            visited.push(current.expect("somebody is waiting"));
        }
        // …and from the last it wraps round to the first.
        assert_eq!(visited, [s[1], s[3], s[0], s[2], s[1]]);
    }

    #[test]
    fn from_a_session_that_is_not_waiting_it_starts_at_the_front() {
        let mut board = Board::default();
        let [waiting, elsewhere] = sessions();
        report(&mut board, waiting, ASKS);
        assert_eq!(
            board.next_waiting(Some(elsewhere), &[waiting, elsewhere]),
            Some(waiting)
        );
    }

    #[test]
    fn a_session_with_no_tab_is_not_visited() {
        let mut board = Board::default();
        let [closed, open] = sessions();
        report(&mut board, closed, ASKS);
        assert_eq!(board.next_waiting(None, &[open]), None);
    }

    #[test]
    fn with_nobody_waiting_there_is_nowhere_to_go() {
        let mut board = Board::default();
        let [a] = sessions();
        report(&mut board, a, Reported::Working);
        assert_eq!(board.next_waiting(None, &[a]), None);
    }

    #[test]
    fn waiting_again_is_waiting_at_the_back() {
        let mut board = Board::default();
        let [first, second] = sessions();
        report(&mut board, first, ASKS);
        report(&mut board, second, ASKS);
        // Answered, and then another prompt: that is a new wait.
        report(&mut board, first, Reported::Working);
        report(&mut board, first, ASKS);
        assert_eq!(board.next_waiting(None, &[first, second]), Some(second));
    }

    #[test]
    fn a_session_whose_tab_has_gone_is_nobody_to_wait_for() {
        // A terminal session's process is its tab's: once the tab is
        // closed, nothing is left to be waiting.
        let mut board = Board::default();
        let [kept, closed] = sessions();
        report(&mut board, kept, Reported::Done);
        report(&mut board, closed, ASKS);

        assert!(board.retain(|session| session == kept));
        assert_eq!(board.status(closed), Idle);
        assert_eq!(board.status(kept), Done);
        assert!(
            !board.retain(|session| session == kept),
            "nothing more went"
        );
    }
}

#[test]
fn scoped_activity_retains_independent_counts_and_deduplicates() {
    use guild_session::status::{Kind, Reported};
    let mut board = guild_session::status::Board::default();
    let [wait, fail, work, done, idle, elsewhere] =
        std::array::from_fn::<_, 6, _>(|_| SessionId::new());
    for (id, record) in [
        (wait, Reported::Blocked(Some(Kind::Question))),
        (fail, Reported::Error),
        (work, Reported::Working),
        (done, Reported::Done),
        (elsewhere, Reported::Blocked(Some(Kind::Permission))),
    ] {
        board.report(id, Some(record));
    }
    let all = board.summary([wait, fail, work, done, idle, work]);
    assert_eq!(
        (all.waiting, all.failed, all.working, all.unread, all.idle),
        (1, 1, 1, 1, 1)
    );
    assert!(matches!(all.primary(), Waiting(_)));
    assert_eq!(board.summary([fail, work, done]).primary(), Failed);
    assert_eq!(board.summary([work, done]).primary(), Working);
    assert!(board.summary([fail, work, done]).busy());
    assert_eq!(board.summary([done]).primary(), Done);
    board.seen(done);
    assert_eq!(board.summary([done]).unread, 0);
    assert_eq!(board.summary([]).primary(), Idle);
    assert_eq!(board.status(elsewhere), Waiting(Why::Permission));
}

// ── The managed-input latch: the hook machinery the board used to be ────
//
// Moved, not rewritten, when the board began to show OSC 7501 only; the
// rows below are its evidence rules for managed live input.

/// Reading a tab and saved history are not observations of its new process.
#[test]
fn an_unobserved_process_stays_unknown_until_a_lifecycle_signal() {
    let id = guild_session::SessionId::new();
    let mut board = guild_session::status::Latch::default();
    assert_eq!(board.observed_status(id), "unknown");
    for signal in [
        guild_session::status::Signal::Seen,
        guild_session::status::Signal::ToolFinished,
        guild_session::status::Signal::Interrupted,
    ] {
        board.apply(id, signal);
        assert_eq!(board.observed_status(id), "unknown");
    }
    board.apply(id, guild_session::status::Signal::PromptSubmitted);
    assert_eq!(board.observed_status(id), "working");
    board.apply(id, guild_session::status::Signal::TurnEnded);
    board.apply(id, guild_session::status::Signal::Seen);
    assert_eq!(board.observed_status(id), "idle");
    board.forget(id);
    assert_eq!(board.observed_status(id), "unknown");
}

#[test]
fn unrelated_parallel_tool_completion_preserves_unanswered_permission() {
    let session = SessionId::new();
    let mut board = guild_session::status::Latch::default();
    board.apply(session, PromptSubmitted);
    board.apply(session, PermissionAsked);
    board.apply(session, ToolFinished);
    assert_eq!(board.status(session), Waiting(Why::Permission));
}

#[test]
fn payload_completion_answers_only_its_actual_request() {
    let id = SessionId::new();
    let mut board = guild_session::status::Latch::default();
    board.apply_payload(
        id,
        QuestionAsked,
        &serde_json::json!({"tool_use_id":"question"}),
    );
    board.apply_payload(
        id,
        ToolFinished,
        &serde_json::json!({"tool_use_id":"parallel"}),
    );
    assert_eq!(board.status(id), Waiting(Why::Question));
    board.apply_payload(
        id,
        ToolFinished,
        &serde_json::json!({"tool_use_id":"question"}),
    );
    assert_eq!(board.status(id), Working);
    board.apply_payload(id, PermissionAsked, &serde_json::json!({}));
    board.apply_payload(id, Resumed, &serde_json::json!({}));
    assert_eq!(board.status(id), Waiting(Why::Permission));
    board.apply(id, PromptSubmitted);
    assert_eq!(board.status(id), Working);
}

#[test]
fn activity_secondary_counts_are_bounded_without_losing_exact_counts() {
    let activity = guild_session::status::Activity {
        waiting: usize::MAX,
        failed: usize::MAX,
        working: usize::MAX,
        unread: usize::MAX,
        idle: 0,
    };
    assert_eq!(secondary(activity), ["!9+", "●9+"]);
    assert_eq!(activity.unread, usize::MAX);
}

fn secondary(activity: guild_session::status::Activity) -> Vec<String> {
    activity
        .secondary()
        .into_iter()
        .map(|count| count.text)
        .collect()
}

#[test]
fn activity_secondary_counts_only_what_asks_for_the_person() {
    use guild_session::status::Activity;
    // Working is never a secondary count, whatever leads.
    let working = Activity {
        working: 3,
        unread: 1,
        ..Activity::default()
    };
    assert_eq!(working.primary(), Working);
    assert_eq!(secondary(working), ["●1"]);
    let failed_while_working = Activity {
        failed: 1,
        working: 2,
        ..Activity::default()
    };
    assert_eq!(failed_while_working.primary(), Failed);
    assert_eq!(secondary(failed_while_working), Vec::<String>::new());
    // The primary state is not repeated; the rest keep the `?`, `!`, `●` order
    // and say which state each count is, so the window can colour it like its mark.
    let all = Activity {
        waiting: 2,
        failed: 12,
        working: 4,
        unread: 5,
        idle: 7,
    };
    let counts = all.secondary();
    assert_eq!(
        counts
            .iter()
            .map(|count| (count.status, count.text.as_str()))
            .collect::<Vec<_>>(),
        [(Failed, "!9+"), (Done, "●5")]
    );
    let done = Activity {
        unread: 2,
        idle: 1,
        ..Activity::default()
    };
    assert_eq!(done.primary(), Done);
    assert_eq!(secondary(done), Vec::<String>::new());
}

#[test]
fn late_idle_after_interruption_waits_for_a_new_turn() {
    let session = SessionId::new();
    let mut board = guild_session::status::Latch::default();
    board.apply(session, PromptSubmitted);
    board.apply(session, Interrupted);
    board.apply(session, TurnEnded);
    assert_eq!(board.status(session), Idle);
    board.apply(session, PromptSubmitted);
    board.apply(session, TurnEnded);
    assert_eq!(board.status(session), Done);
}

#[test]
fn question_permission_duplicate_without_identity_does_not_block_matched_answer() {
    let session = SessionId::new();
    let mut board = guild_session::status::Latch::default();
    board.apply_payload(
        session,
        QuestionAsked,
        &serde_json::json!({"tool_use_id":"q"}),
    );
    board.apply_payload(
        session,
        QuestionAsked,
        &serde_json::json!({"hook_event_name":"PermissionRequest", "tool_name":"AskUserQuestion"}),
    );
    board.apply_payload(
        session,
        ToolFinished,
        &serde_json::json!({"tool_use_id":"q"}),
    );
    assert_eq!(board.status(session), Working);
}

#[test]
fn recorded_permission_without_id_resolves_the_unique_pretool_identity() {
    let mut board = guild_session::status::Latch::default();
    let pre = payload(include_str!("../fixtures/hooks/pre-tool-use-bash.json"));
    let permission = payload(include_str!("../fixtures/hooks/permission-request.json"));
    let post = payload(include_str!("../fixtures/hooks/post-tool-use.json"));
    let (session, signal) = from_hook(&pre).unwrap();
    board.apply_payload(session, signal, &pre);
    board.apply_payload(session, PermissionAsked, &permission);
    board.apply_payload(
        session,
        ToolFinished,
        &serde_json::json!({"tool_use_id":"unrelated"}),
    );
    assert_eq!(board.status(session), Waiting(Why::Permission));
    board.apply_payload(session, ToolFinished, &post);
    assert_eq!(board.status(session), Working);
}

#[test]
fn delayed_matching_completion_retains_authoritative_turn_end() {
    use guild_session::status::Latch;
    let session = SessionId::new();
    let mut board = Latch::default();
    let request = serde_json::json!({"tool_use_id":"last"});
    board.apply(session, PromptSubmitted);
    board.apply_payload(session, PermissionAsked, &request);
    board.apply(session, TurnEnded);
    board.apply_payload(
        session,
        ToolFinished,
        &serde_json::json!({"tool_use_id":"other"}),
    );
    assert_eq!(board.status(session), Waiting(Why::Permission));
    board.apply_payload(session, ToolFinished, &request);
    assert_eq!(
        board.status(session),
        Done,
        "async completion must not resurrect a finished turn"
    );
    for boundary in [ExecutionFailed, Interrupted, PromptSubmitted] {
        let mut board = Latch::default();
        board.apply_payload(session, PermissionAsked, &request);
        board.apply(session, TurnEnded);
        board.apply(session, boundary);
        board.apply_payload(session, ToolFinished, &request);
        assert_eq!(
            board.status(session),
            match boundary {
                ExecutionFailed => Failed,
                Interrupted => Idle,
                _ => Working,
            }
        );
    }
}

#[test]
fn retain_drops_closed_turn_end_but_keeps_an_open_pending_completion() {
    use guild_session::status::Latch;
    let closed = SessionId::new();
    let kept = SessionId::new();
    let mut board = Latch::default();
    let request = serde_json::json!({"tool_use_id":"approval"});
    for session in [closed, kept] {
        board.apply_payload(session, PermissionAsked, &request);
        board.apply(session, TurnEnded);
    }
    board.retain(|session| session == kept);
    // A reopened session can first report permission, without a prompt witness.
    // Its former process's Stop must not finish the new operation.
    board.apply_payload(closed, PermissionAsked, &request);
    board.apply_payload(closed, ToolFinished, &request);
    assert_eq!(board.status(closed), Working);
    board.apply_payload(kept, ToolFinished, &request);
    assert_eq!(board.status(kept), Done);
}

/// Codex 0.162.1 schema: turn_id, no ID on PermissionRequest, and a
/// description added only to approval input. The previous fixtures invented
/// compatible request IDs and never exercised this public contract.
#[test]
fn codex_real_turn_and_command_correlate_idless_approval() {
    use guild_session::harness::CodexHarness;
    use guild_session::status::Latch;
    let session = SessionId::new();
    let mut board = Latch::default();
    for (event, id, input, expected) in [
        (
            "PreToolUse",
            Some("exec-approved"),
            serde_json::json!({"command":"cargo check"}),
            Working,
        ),
        (
            "PermissionRequest",
            None,
            serde_json::json!({"command":"cargo check","description":"Build the project"}),
            Waiting(Why::Permission),
        ),
        (
            "PostToolUse",
            Some("exec-parallel"),
            serde_json::json!({"command":"pwd"}),
            Waiting(Why::Permission),
        ),
        (
            "PostToolUse",
            Some("exec-approved"),
            serde_json::json!({"command":"cargo check"}),
            Working,
        ),
    ] {
        let mut p = serde_json::json!({"session_id":session.to_string(),"codex_session":"11111111-2222-3333-4444-555555555555","turn_id":"turn-current","tool_name":"Bash","tool_input":input,"hook_event_name":event});
        if let Some(id) = id {
            p["tool_use_id"] = id.into();
        }
        let (sid, signal) = CodexHarness.status_from_payload(&p).unwrap();
        board.apply_payload(sid, signal, &p);
        assert_eq!(board.status(session), expected, "{event} {id:?}");
    }
}

#[test]
fn codex_identical_parallel_commands_are_not_guessed() {
    use guild_session::status::Latch;
    let session = SessionId::new();
    let mut board = Latch::default();
    for id in ["exec-one", "exec-two"] {
        let p = serde_json::json!({"hook_event_name":"PreToolUse","codex_session":"native","turn_id":"turn","tool_name":"Bash","tool_input":{"command":"pwd"},"tool_use_id":id});
        board.apply_payload(session, ToolStarted, &p);
    }
    let p = serde_json::json!({"hook_event_name":"PermissionRequest","codex_session":"native","turn_id":"turn","tool_name":"Bash","tool_input":{"command":"pwd"}});
    board.apply_payload(session, PermissionAsked, &p);
    for id in ["exec-one", "exec-two"] {
        board.apply_payload(
            session,
            ToolFinished,
            &serde_json::json!({"tool_use_id":id}),
        );
        board.apply(session, Seen);
        assert_eq!(board.status(session), Waiting(Why::Permission));
    }
}

// ── The chime and the unread mark: a root record's change into `done` ──
//
// Open decision 2's approved answer: a completion is a change into `done`
// while unseen; a repeated `done` with nothing between is a repeat; a seen
// `done` shows as rest; `error` stays; `idle` (OpenCode) is no completion.

mod chime {
    use guild_session::SessionId;
    use guild_session::status::{Board, Kind, Reported, Status, should_chime};

    fn report(board: &mut Board, session: SessionId, record: Option<Reported>) {
        board.report(session, record);
    }

    #[test]
    fn edges_coalesce_and_distinguish_turns_between_polls() {
        let mut board = Board::default();
        let a = SessionId::new();
        let b = SessionId::new();
        let empty = board.done_completions();
        assert!(!should_chime(&empty, &empty));
        report(&mut board, a, Some(Reported::Working));
        report(&mut board, a, Some(Reported::Done));
        report(&mut board, b, Some(Reported::Done));
        let first = board.done_completions();
        assert_eq!(first.len(), 2);
        assert!(should_chime(&empty, &first));
        assert!(!should_chime(&first, &board.done_completions()));
        report(&mut board, a, Some(Reported::Done));
        assert_eq!(first, board.done_completions(), "a repeat keeps its turn");
        report(&mut board, a, Some(Reported::Working));
        report(&mut board, a, Some(Reported::Done));
        assert!(should_chime(&first, &board.done_completions()));
    }

    #[test]
    fn a_seen_done_is_rest_and_a_repeat_brings_nothing_back() {
        let mut board = Board::default();
        let session = SessionId::new();
        let empty = board.done_completions();
        report(&mut board, session, Some(Reported::Working));
        report(&mut board, session, Some(Reported::Done));
        assert_eq!(board.status(session), Status::Done);
        assert!(should_chime(&empty, &board.done_completions()));
        board.seen(session);
        assert_eq!(board.status(session), Status::Idle, "seen: rest");
        assert!(board.done_completions().is_empty());
        report(&mut board, session, Some(Reported::Done));
        assert_eq!(board.status(session), Status::Idle, "no mark comes back");
        assert!(!should_chime(&empty, &board.done_completions()));
        report(&mut board, session, Some(Reported::Working));
        report(&mut board, session, Some(Reported::Done));
        assert!(should_chime(&empty, &board.done_completions()));
    }

    #[test]
    fn only_unseen_finished_turns_survive_a_pass() {
        let asks = Reported::Blocked(Some(Kind::Permission));
        let question = Reported::Blocked(Some(Kind::Question));
        for reports in [
            vec![],
            vec![Some(Reported::Working)],
            vec![Some(asks)],
            vec![Some(question)],
            vec![Some(Reported::Blocked(None))],
            vec![Some(Reported::Working), Some(Reported::Error)],
            vec![Some(Reported::Working), Some(Reported::Idle)],
            vec![Some(Reported::Done), None],
        ] {
            let mut board = Board::default();
            let session = SessionId::new();
            for record in reports.clone() {
                report(&mut board, session, record);
            }
            assert!(board.done_completions().is_empty(), "{reports:?}");
        }
        let mut board = Board::default();
        let session = SessionId::new();
        report(&mut board, session, Some(Reported::Done));
        board.seen(session);
        assert!(board.done_completions().is_empty(), "done, then seen");
    }

    #[test]
    fn completion_memory_clears_with_new_work_and_forgetting() {
        let session = SessionId::new();
        for restart in [
            Some(Reported::Working),
            Some(Reported::Blocked(None)),
            Some(Reported::Idle),
            Some(Reported::Error),
            None,
        ] {
            let mut board = Board::default();
            report(&mut board, session, Some(Reported::Done));
            board.seen(session);
            report(&mut board, session, restart);
            report(&mut board, session, Some(Reported::Done));
            assert_eq!(board.done_completions().len(), 1, "{restart:?}");
        }
        for retain in [false, true] {
            let mut board = Board::default();
            report(&mut board, session, Some(Reported::Done));
            board.seen(session);
            if retain {
                board.retain(|_| false);
            } else {
                board.forget(session);
            }
            report(&mut board, session, Some(Reported::Done));
            assert_eq!(board.done_completions().len(), 1);
        }
    }

    #[test]
    fn an_error_survives_being_seen() {
        let mut board = Board::default();
        let session = SessionId::new();
        report(&mut board, session, Some(Reported::Working));
        report(&mut board, session, Some(Reported::Error));
        board.seen(session);
        assert_eq!(board.status(session), Status::Failed);
        assert!(
            board.done_completions().is_empty(),
            "an error does not chime"
        );
    }

    #[test]
    fn idle_after_working_is_a_turn_but_no_completion() {
        // OpenCode 2.2 reports idle, not done, when a turn ends.
        let mut board = Board::default();
        let session = SessionId::new();
        report(&mut board, session, Some(Reported::Working));
        assert!(board.report(session, Some(Reported::Idle)).ended);
        assert_eq!(board.status(session), Status::Idle);
        assert!(board.done_completions().is_empty());
    }

    #[test]
    fn blocked_shows_its_reason_only_when_the_program_said_it() {
        use guild_session::status::Why;
        let mut board = Board::default();
        let session = SessionId::new();
        for (kind, why) in [
            (Some(Kind::Permission), Why::Permission),
            (Some(Kind::Question), Why::Question),
            (Some(Kind::Auth), Why::Auth),
            (None, Why::Unstated),
        ] {
            report(&mut board, session, Some(Reported::Blocked(kind)));
            assert_eq!(board.status(session), Status::Waiting(why));
        }
        assert_eq!(board.observed_status(session), "waiting");
        report(&mut board, session, None);
        assert_eq!(board.status(session), Status::Idle);
        assert_eq!(board.observed_status(session), "unknown", "no record");
    }
}

// ── A session that takes over a terminal's records (rebind) ─────────────

mod seed {
    use guild_session::SessionId;
    use guild_session::status::{Board, Kind, Reported, Status};

    /// **A rebound session starts from the terminal's current record**, as a
    /// fact rather than a transition — review r1, finding 1: no completion,
    /// no unread mark, no turn; a repeat of it afterwards is a repeat.
    #[test]
    fn a_seeded_record_is_a_fact_not_a_transition() {
        let mut board = Board::default();
        let session = SessionId::new();
        board.seed(session, Some(Reported::Done));
        assert_eq!(board.reported(session), Some(Reported::Done));
        assert_eq!(
            board.observed_status(session),
            "idle",
            "a done nobody watched end"
        );
        assert_eq!(board.status(session), Status::Idle);
        assert!(board.done_completions().is_empty(), "no chime, no unread");
        assert!(
            !board.report(session, Some(Reported::Done)).changed,
            "a repeat"
        );
        let next = board.report(session, Some(Reported::Working));
        assert!(next.changed && next.began);
        assert!(board.report(session, Some(Reported::Done)).ended);
        assert_eq!(
            board.done_completions().len(),
            1,
            "a real turn after it counts"
        );

        let mut board = Board::default();
        board.seed(session, Some(Reported::Working));
        assert_eq!(board.status(session), Status::Working);
        assert!(
            board.report(session, Some(Reported::Done)).ended,
            "the seeded turn goes on"
        );

        let mut board = Board::default();
        board.seed(session, Some(Reported::Blocked(Some(Kind::Question))));
        assert_eq!(board.observed_status(session), "waiting");
        board.seed(session, None);
        assert_eq!(board.observed_status(session), "unknown");
    }
}

// ── OSC 7501: what the program reports of itself ────────────────────────

mod reports {
    use guild_session::SessionId;
    use guild_session::status::{Board, Kind, Reported};

    #[test]
    fn a_turn_is_working_then_done_idle_or_error() {
        for end in [Reported::Done, Reported::Idle, Reported::Error] {
            let mut board = Board::default();
            let session = SessionId::new();
            let began = board.report(session, Some(Reported::Working));
            assert!(began.changed && began.began && !began.ended);
            let ended = board.report(session, Some(end));
            assert!(ended.changed && ended.ended, "{end:?}");
            assert_eq!(board.reported(session), Some(end));
        }
    }

    #[test]
    fn done_without_working_first_is_no_turn() {
        let mut board = Board::default();
        let session = SessionId::new();
        let first = board.report(session, Some(Reported::Done));
        assert!(first.changed && !first.ended && !first.began);
        assert!(
            !board.report(session, Some(Reported::Done)).changed,
            "a repeat"
        );
    }

    #[test]
    fn blocked_is_inside_the_turn() {
        let mut board = Board::default();
        let session = SessionId::new();
        board.report(session, Some(Reported::Working));
        let blocked = board.report(session, Some(Reported::Blocked(Some(Kind::Question))));
        assert!(blocked.changed && !blocked.began && !blocked.ended);
        let resumed = board.report(session, Some(Reported::Working));
        assert!(!resumed.began, "the same turn goes on");
        assert!(board.report(session, Some(Reported::Done)).ended);
    }

    #[test]
    fn a_record_that_ends_mid_turn_ends_no_turn() {
        let mut board = Board::default();
        let session = SessionId::new();
        board.report(session, Some(Reported::Working));
        let gone = board.report(session, None);
        assert!(gone.changed && !gone.ended);
        assert_eq!(board.reported(session), None);
        assert!(
            !board.report(session, Some(Reported::Done)).ended,
            "the process exit dropped the turn"
        );
    }
}

/// **The timeline is the order transitions were observed in**, newest
/// first, even when the wall clock gives two of them the same instant —
/// which a burst of reports in one tick does. A tie must not fall back to
/// the order the sessions happen to be listed in.
#[test]
fn the_timeline_orders_transitions_in_one_clock_tick_newest_first() {
    use guild_session::status::{Board, Reported};
    let sessions: Vec<SessionId> = (0..200).map(|_| SessionId::new()).collect();
    let mut board = Board::default();
    for session in &sessions {
        board.report(*session, Some(Reported::Working));
    }
    let mut newest_first = sessions.clone();
    newest_first.reverse();
    let never = SessionId::new();
    let mut open = sessions.clone();
    open.insert(0, never);
    let mut expected = newest_first;
    expected.push(never);
    assert_eq!(board.timeline(&open), expected);
}
