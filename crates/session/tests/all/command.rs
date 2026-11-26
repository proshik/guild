//! What the `guild` command and the window say to each other — M13.

use guild_session::command::{
    Answer, Command, SESSION_PROMPT, refuses_send, refuses_session, with_session,
};

/// **Who may start a task** — decision 5 of M13's plan. The window cannot
/// see intent, so it checks provenance: a session started with a task may
/// not start another with a task. Everything else is allowed, and the
/// table is written out whole because it is the rule.
#[test]
fn a_task_may_not_start_a_task_and_nothing_else_is_refused() {
    // prompt, caller_is_task
    assert!(refuses_session(true, true), "a task starting a task");
    assert!(
        !refuses_session(true, false),
        "a person or a plain session starting a task"
    );
    assert!(
        !refuses_session(false, true),
        "a task opening a tab for a person to type into"
    );
    assert!(!refuses_session(false, false), "a plain tab");
}

/// **The wire is tagged, and a tag is data.** `guild` and the window are
/// two binaries that may be built at different times; the tags they agree
/// on are what a newer command and an older window still share, so they
/// are pinned here rather than left to whatever serde derives.
#[test]
fn the_wire_tags_are_what_both_ends_agree_on() {
    let status = serde_json::to_value(Command::Status).unwrap();
    assert_eq!(status, serde_json::json!({ "command": "status" }));

    let new = serde_json::to_value(Command::SessionNew {
        worktree: "/w/guild-feat".into(),
        prompt: Some("fix the build".to_owned()),
        caller: None,
        front: false,
        workspace: None,
        harness: None,
    })
    .unwrap();
    assert_eq!(new["command"], "session_new");
    assert_eq!(new["prompt"], "fix the build");

    let refused = serde_json::to_value(Answer::Refused {
        because: "locked".to_owned(),
    })
    .unwrap();
    assert_eq!(
        refused,
        serde_json::json!({ "answer": "refused", "because": "locked" })
    );
}

#[test]
fn session_new_carries_the_harness_and_an_old_message_defaults_to_claude() {
    use guild_session::HarnessKind;
    let json = serde_json::to_value(Command::SessionNew {
        worktree: "/w".into(),
        prompt: None,
        caller: None,
        front: false,
        workspace: None,
        harness: Some(HarnessKind::OpenCode),
    })
    .unwrap();
    assert_eq!(json["harness"], "open_code");
    match serde_json::from_value(json).unwrap() {
        Command::SessionNew { harness, .. } => assert_eq!(harness, Some(HarnessKind::OpenCode)),
        _ => unreachable!(),
    }
}

#[test]
fn workspace_wire_is_tagged_and_old_messages_still_parse() {
    let commands = [
        Command::WorkspaceList,
        Command::WorkspaceCreate {
            name: "Side".into(),
            icon: Some("rocket".into()),
            select: true,
        },
        Command::WorkspaceSelect {
            workspace: "00000000-0000-0000-0000-000000000001".into(),
        },
    ];
    for command in commands {
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
    }

    let old: Command = serde_json::from_value(serde_json::json!({
        "command": "session_new", "worktree": "/w", "prompt": null,
        "caller": null, "front": false
    }))
    .unwrap();
    assert!(matches!(
        old,
        Command::SessionNew {
            workspace: None,
            ..
        }
    ));

    let old_agent: guild_session::command::Agent = serde_json::from_value(serde_json::json!({
        "session": "s", "worktree": "/w", "status": "idle", "turned": false,
        "task": false, "open": true, "exited": false
    }))
    .unwrap();
    assert!(old_agent.workspace.is_empty());
}

/// **`worktree create` gained a base, M13.1, and the old wire still
/// parses**: a `guild` built before it sends no `base`, and a window built
/// before it answers `created` without one.
#[test]
fn a_worktree_create_without_a_base_is_the_old_wire() {
    let old: Command = serde_json::from_value(serde_json::json!({
        "command": "worktree_create", "from": "/w/guild", "name": "feat"
    }))
    .unwrap();
    assert!(matches!(old, Command::WorktreeCreate { base: None, .. }));

    let old: Answer = serde_json::from_value(serde_json::json!({
        "answer": "created", "path": "/w/guild-feat", "branch": "feat"
    }))
    .unwrap();
    assert!(matches!(
        old,
        Answer::Created {
            base: None,
            session: None,
            ..
        }
    ));
}

/// **A worktree made with its task carries the task on the wire** —
/// M13.1, decision 3 — and an old `worktree_create` is one without.
#[test]
fn a_worktree_create_carries_its_task_and_the_old_wire_has_none() {
    use guild_session::HarnessKind;
    let command = Command::WorktreeCreate {
        from: "/w/guild".into(),
        name: "feat".into(),
        base: None,
        prompt: Some("fix the build".into()),
        caller: Some("11111111-2222-3333-4444-555555555555".into()),
        front: true,
        workspace: Some("00000000-0000-0000-0000-000000000001".into()),
        harness: Some(HarnessKind::Codex),
    };
    let json = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);

    let old: Command = serde_json::from_value(serde_json::json!({
        "command": "worktree_create", "from": "/w/guild", "name": "feat"
    }))
    .unwrap();
    assert!(matches!(
        old,
        Command::WorktreeCreate {
            prompt: None,
            caller: None,
            front: false,
            workspace: None,
            harness: None,
            ..
        }
    ));
}

/// **What a worktree made with its task answers** — M13.1. The session's
/// start joins the worktree's answer; a session refused after the worktree
/// was made says the worktree is there, because an agent told only
/// *refused* would make it again and meet git's *already exists*.
#[test]
fn a_worktree_s_task_joins_its_answer_or_says_the_worktree_is_there() {
    let started = with_session(
        "/w/guild-feat".into(),
        "feat".into(),
        Some("main".into()),
        Answer::SessionStarted {
            session: "s-1".into(),
            worktree: "/w/guild-feat".into(),
        },
    );
    assert_eq!(
        started,
        Answer::Created {
            path: "/w/guild-feat".into(),
            branch: "feat".into(),
            base: Some("main".into()),
            session: Some("s-1".into()),
        }
    );

    let Answer::Refused { because } = with_session(
        "/w/guild-feat".into(),
        "feat".into(),
        None,
        Answer::Refused {
            because: "the window could not open a session".into(),
        },
    ) else {
        panic!("a refused session was reported as started")
    };
    assert!(
        because.contains("/w/guild-feat")
            && because.contains("feat")
            && because.contains("could not open a session"),
        "{because}"
    );
}

/// **What an agent said is its words, not its tool calls** — M13, for
/// `guild agent read`. A reply is a list of parts; the caller waiting on a
/// task wants the report, oldest first, and only as many as it asked for.
#[test]
fn a_transcript_s_replies_are_the_agent_s_words_the_last_ones_first_to_last() {
    use guild_session::transcript::replies_in;

    let transcript = [
        r#"{"type":"user","message":{"content":"fix the build"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Looking at the error."},{"type":"tool_use","name":"Bash","input":{"command":"cargo check"}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"error[E0308]"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{}}]}}"#,
        "not json at all",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Fixed: a mismatched type."},{"type":"text","text":"Tests pass."}]}}"#,
    ]
    .join("\n");

    // The tool-only reply says nothing and is not a reply; the two text
    // parts of the last one are one reply.
    assert_eq!(
        replies_in(&transcript, 5),
        [
            "Looking at the error.",
            "Fixed: a mismatched type.\nTests pass."
        ]
    );
    assert_eq!(
        replies_in(&transcript, 1),
        ["Fixed: a mismatched type.\nTests pass."]
    );
    assert!(replies_in("", 3).is_empty());
}

/// **`idle` alone is not finished** — M13, for `guild agent wait`. A
/// session just started reads `idle` before its first turn's hook arrives;
/// waiting for *not working* would return before the agent did anything.
#[test]
fn a_session_is_finished_when_a_turn_has_ended_or_it_is_asking() {
    use guild_session::command::settled;

    assert!(
        !settled("idle", false),
        "just started, nothing has happened yet"
    );
    assert!(!settled("working", true), "in the middle of a turn");
    assert!(settled("done", true), "a turn ended");
    assert!(settled("waiting", true), "it stopped to ask the person");
    assert!(
        settled("waiting", false),
        "it asked before its first turn ended"
    );
    assert!(settled("idle", true), "a done the person already looked at");
    assert!(
        !settled("something new", true),
        "a word this binary does not know is not finished"
    );
}

/// **A wait succeeds only on a task whose tab is open and whose process is
/// alive** — M5.12, decision 16. A closed tab is `Closed` whatever its turn
/// did, and an ended process is `Exited` whatever the board last heard: the
/// thing waited for is gone, and waiting on it would run out the clock.
#[test]
fn a_wait_ends_on_a_finished_turn_a_closed_tab_or_an_ended_process() {
    use guild_session::command::{Agent, Waited, waited};

    let agent = |status: &str, turned: bool, open: bool, exited: bool| Agent {
        session: "s".to_owned(),
        harness: Some(guild_session::HarnessKind::Claude),
        conversation: None,
        worktree: std::path::PathBuf::from("/w"),
        workspace: String::new(),
        status: status.to_owned(),
        kind: None,
        turned,
        task: true,
        open,
        exited,
        tab: true,
    };
    let table = [
        (
            agent("failed", false, true, false),
            Waited::Finished,
            "observed failure",
        ),
        (
            agent("failed", true, false, false),
            Waited::Closed,
            "closed failed session",
        ),
        (
            agent("failed", true, true, true),
            Waited::Exited,
            "exited failed session",
        ),
        // Open and alive: what `settled` says.
        (
            agent("idle", false, true, false),
            Waited::Going,
            "just started",
        ),
        (
            agent("working", true, true, false),
            Waited::Going,
            "mid-turn",
        ),
        (
            agent("done", true, true, false),
            Waited::Finished,
            "a turn ended",
        ),
        (
            agent("waiting", false, true, false),
            Waited::Finished,
            "it asked",
        ),
        (
            agent("idle", true, true, false),
            Waited::Finished,
            "a done looked at",
        ),
        // Closed comes first, even after a finished turn.
        (
            agent("idle", true, false, false),
            Waited::Closed,
            "closed after a finished turn is still closed",
        ),
        (
            agent("done", true, false, false),
            Waited::Closed,
            "closed on done",
        ),
        (
            agent("working", false, false, false),
            Waited::Closed,
            "closed mid-turn",
        ),
        (
            agent("idle", false, false, true),
            Waited::Closed,
            "closed and exited is closed",
        ),
        // Then an ended process.
        (
            agent("working", true, true, true),
            Waited::Exited,
            "exited while working is exited",
        ),
        (
            agent("done", true, true, true),
            Waited::Exited,
            "exited after a turn",
        ),
        (
            agent("idle", false, true, true),
            Waited::Exited,
            "exited before a turn",
        ),
    ];
    for (agent, expected, why) in table {
        assert_eq!(waited(&agent), expected, "{why}");
    }
}

/// **A `guild` built after M5.12 reads a window built before it**, which
/// sends no `exited` — so the field is `serde(default)`, and its absence
/// reads as a process still alive.
#[test]
fn an_agent_from_a_window_without_exited_reads_as_alive() {
    let old = r#"{"session":"s","worktree":"/w","status":"working","turned":true,"task":true,"open":true}"#;
    let agent: guild_session::command::Agent =
        serde_json::from_str(old).expect("an old window's agent");
    assert!(!agent.exited);
    assert!(agent.open);
}

/// **A session Guild starts is told the command is there**, or it never
/// learns: a session asked to make a worktree did it with plain `git
/// worktree` beside the window, and the rules in `guild instructions` are
/// read only by an agent that knows to ask for them. It names the
/// instructions and the thing not to do, and it stays short, because it
/// is part of every turn of every session the window starts.
#[test]
fn a_session_is_told_where_the_rules_are_and_no_more() {
    assert!(SESSION_PROMPT.contains("`guild instructions`"));
    assert!(SESSION_PROMPT.contains("`guild help`"));
    assert!(SESSION_PROMPT.len() < 600);
}

/// **A removal says whether what git ignores may go, and who is asking** —
/// M13 task 8. The flag is on the wire rather than kept in the command,
/// because the window is the one that lists the ignored files and refuses;
/// and the caller travels as it does for `session new`, because a task may
/// not remove a worktree.
#[test]
fn a_removal_carries_its_consent_and_its_caller() {
    let remove = serde_json::to_value(Command::WorktreeRemove {
        path: "/w/guild-feat".into(),
        delete_ignored: true,
        caller: Some("11111111-2222-3333-4444-555555555555".to_owned()),
    })
    .unwrap();
    assert_eq!(
        remove,
        serde_json::json!({
            "command": "worktree_remove",
            "path": "/w/guild-feat",
            "delete_ignored": true,
            "caller": "11111111-2222-3333-4444-555555555555",
        })
    );

    let removed = serde_json::to_value(Answer::Removed {
        path: "/w/guild-feat".into(),
    })
    .unwrap();
    assert_eq!(
        removed,
        serde_json::json!({ "answer": "removed", "path": "/w/guild-feat" })
    );
}

/// **Who may send to whom, and when** — M13 task 9. A send is typing into a
/// session's prompt, so it goes only to a task, only from something that is
/// not one, and only while the session is reading a prompt: not mid-turn,
/// not at a permission or a question — a keystroke there chooses blind —
/// and not before its first turn, which is where the trust dialog is.
#[test]
fn a_send_goes_to_a_task_reading_a_prompt_and_comes_from_a_person() {
    // target_is_task, caller_is_task, status, turned
    assert_eq!(refuses_send(true, false, "done", true), None);
    assert_eq!(
        refuses_send(true, false, "done", false),
        None,
        "a turn ended"
    );
    assert_eq!(
        refuses_send(true, false, "idle", true),
        None,
        "seen after a turn"
    );

    let person = refuses_send(false, false, "done", true).expect("a person's session");
    assert!(person.contains("person"), "{person}");
    let task = refuses_send(true, true, "done", true).expect("a task steering a task");
    assert!(task.contains("task"), "{task}");
    let busy = refuses_send(true, false, "working", true).expect("mid-turn");
    assert!(busy.contains("guild agent wait"), "{busy}");
    let waiting = refuses_send(true, false, "waiting", true).expect("a question or a permission");
    assert!(waiting.contains("permission"), "{waiting}");
    let fresh = refuses_send(true, false, "idle", false).expect("no turn yet");
    assert!(fresh.contains("first turn"), "{fresh}");

    // Whose session it is is said before what state it is in.
    assert_eq!(refuses_send(false, true, "working", false), Some(person));
}

/// **A send carries its text and its caller, and is answered by the session
/// it reached** — the wire of M13 task 9.
#[test]
fn a_send_carries_its_text_and_its_caller() {
    let send = serde_json::to_value(Command::AgentSend {
        session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
        text: "two\nlines".to_owned(),
        caller: None,
    })
    .unwrap();
    assert_eq!(
        send,
        serde_json::json!({
            "command": "agent_send",
            "session": "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "text": "two\nlines",
            "caller": null,
        })
    );
    let sent = serde_json::to_value(Answer::Sent {
        session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
    })
    .unwrap();
    assert_eq!(
        sent,
        serde_json::json!({ "answer": "sent", "session": "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d" })
    );
}

/// An unobserved process must not be typed into, even with saved history.
#[test]
fn unknown_telemetry_never_permits_send_or_finishes_wait() {
    assert!(!guild_session::command::settled("unknown", true));
    assert!(guild_session::command::refuses_send(true, false, "unknown", true).is_some());
}

#[test]
fn failed_live_agent_settles_and_keeps_send_provenance() {
    use guild_session::command::{refuses_send, settled};
    assert!(settled("failed", false));
    assert_eq!(refuses_send(true, false, "failed", true), None);
    assert!(refuses_send(false, false, "failed", true).is_some());
    assert!(refuses_send(true, true, "failed", true).is_some());
}

#[test]
fn startup_discovery_is_public_api_guidance() {
    assert!(SESSION_PROMPT.contains("guild help"));
    assert!(SESSION_PROMPT.contains("GUILD_SOCKET"));
    assert!(SESSION_PROMPT.contains("GUILD_SESSION"));
    for policy in [
        "Delegate",
        "Before implementing",
        "review",
        "OpenSpec",
        "revust",
        "workflow",
    ] {
        assert!(
            !SESSION_PROMPT.contains(policy),
            "application bootstrap contains {policy}"
        );
    }
}

#[test]
fn obsolete_native_review_wire_requests_are_unsupported() {
    let old = r#"{"command":"review","operation":"ready","payload":"{}","caller":null}"#;
    assert!(serde_json::from_str::<guild_session::command::Command>(old).is_err());
}

/// **A waiting session says why, when its program said why** — OSC 7501's
/// `kind`, beside the unchanged status word; absent when not reported, and
/// absent from a window built before it, which a newer `guild` still reads.
#[test]
fn a_waiting_reason_rides_beside_the_status_word() {
    use guild_session::command::Agent;
    let agent = |kind: Option<&str>| Agent {
        session: "s".to_owned(),
        harness: Some(guild_session::HarnessKind::Codex),
        conversation: None,
        worktree: std::path::PathBuf::from("/w"),
        workspace: String::new(),
        status: "waiting".to_owned(),
        kind: kind.map(str::to_owned),
        turned: true,
        task: true,
        open: true,
        exited: false,
        tab: true,
    };
    let json = serde_json::to_value(agent(Some("question"))).unwrap();
    assert_eq!(json["status"], "waiting");
    assert_eq!(json["kind"], "question");
    let json = serde_json::to_value(agent(None)).unwrap();
    assert!(json.get("kind").is_none(), "no reason is not an empty one");
    let old: Agent = serde_json::from_value(serde_json::json!({
        "session":"s","worktree":"/w","status":"waiting","turned":true,"task":true,"open":true
    }))
    .unwrap();
    assert_eq!(old.kind, None);
    assert!(
        old.tab,
        "a window built before `tab` held every open record in one"
    );
}

/// **No OSC 7501 report, no send — and the reason says so** rather than
/// calling the session idle. A program that reported a turn and then
/// `idle` (OpenCode 2.2, which has no `done`) is finished.
#[test]
fn a_send_to_a_session_without_a_report_names_the_missing_report() {
    use guild_session::command::{refuses_send, settled};
    let reason = refuses_send(true, false, "unknown", true).expect("refused");
    assert!(reason.contains("status unavailable"), "{reason}");
    assert!(reason.contains("OSC 7501"), "{reason}");
    assert!(!reason.contains("idle prompt"), "{reason}");
    assert_eq!(
        refuses_send(true, false, "idle", true),
        None,
        "working, then idle"
    );
    assert!(settled("idle", true));
    assert!(!settled("unknown", true));
}

/// GLD-055: the group door is a variant of its own, so a window that does
/// not know it refuses it by name instead of starting the session in a
/// `worktree` it would read past a dropped field.
#[test]
fn the_group_session_and_the_group_check_have_tags_of_their_own_and_round_trip() {
    use guild_session::command::{GroupCheck, GroupMember};
    let new = Command::SessionNewInGroup {
        group: "acme".to_owned(),
        prompt: Some("where is AUTH_URL read?".to_owned()),
        caller: Some("caller".to_owned()),
        front: true,
        workspace: Some("Default".to_owned()),
        harness: None,
    };
    let json = serde_json::to_value(&new).unwrap();
    assert_eq!(json["command"], "session_new_in_group");
    assert_eq!(json["group"], "acme");
    assert!(json.get("worktree").is_none());
    assert_eq!(serde_json::from_value::<Command>(json).unwrap(), new);

    let check = Command::GroupCheck {
        group: None,
        workspace: None,
        caller: None,
    };
    let json = serde_json::to_value(&check).unwrap();
    assert_eq!(json["command"], "group_check");
    assert_eq!(serde_json::from_value::<Command>(json).unwrap(), check);
    // The shortest form the command line can send.
    assert_eq!(
        serde_json::from_value::<Command>(serde_json::json!({ "command": "group_check" })).unwrap(),
        check
    );

    let answer = Answer::Groups {
        groups: vec![GroupCheck {
            name: "acme".to_owned(),
            workspace: "w".to_owned(),
            folder: Some("/w/acme".into()),
            matches: Some(false),
            members: vec![GroupMember {
                checkout: "/w/helm".into(),
                inside: Some(false),
            }],
        }],
    };
    let json = serde_json::to_value(&answer).unwrap();
    assert_eq!(json["answer"], "groups");
    assert_eq!(serde_json::from_value::<Answer>(json).unwrap(), answer);
}
