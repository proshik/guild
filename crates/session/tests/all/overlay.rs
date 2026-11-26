use guild_session::{
    SessionId,
    overlay::{Board, Content, Target},
};

#[test]
fn hiding_or_closing_a_view_has_no_operation_side_effect() {
    let target = Target::Session(SessionId::new());
    let mut board = Board::default();
    board
        .open(
            "a",
            "owner",
            target.clone(),
            "Build output",
            Content::Text {
                text: "Compiling".into(),
            },
        )
        .unwrap();
    board.hide("a", "owner").unwrap();
    assert!(board.visible(&target).is_none());
    assert_eq!(board.list("owner").len(), 1);
    board.show("a", "owner").unwrap();
    assert!(board.visible(&target).is_some());
    board.close("a", "owner").unwrap();
    assert!(board.list("owner").is_empty());
}

#[test]
fn overlays_are_owned_and_do_not_cover_other_panes() {
    let a = Target::Session(SessionId::new());
    let b = Target::Session(SessionId::new());
    let mut board = Board::default();
    board
        .open(
            "a",
            "one",
            a.clone(),
            "Progress",
            Content::Text {
                text: "Working".into(),
            },
        )
        .unwrap();
    board
        .open(
            "b",
            "two",
            b.clone(),
            "Result",
            Content::Text {
                text: "Done".into(),
            },
        )
        .unwrap();
    assert!(board.hide("a", "two").is_err());
    assert!(board.close("a", "two").is_err());
    assert!(
        board
            .open(
                "a",
                "two",
                b.clone(),
                "Replace",
                Content::Text { text: "bad".into() }
            )
            .is_err()
    );
    assert_eq!(board.visible(&a).unwrap().title, "Progress");
    assert_eq!(board.visible(&b).unwrap().title, "Result");
}

#[test]
fn producer_updates_do_not_reopen_hidden_views_and_wire_roundtrips() {
    let target = Target::Session(SessionId::new());
    let mut board = Board::default();
    board
        .open(
            "log",
            "owner",
            target.clone(),
            "Build",
            Content::Text {
                text: "First".into(),
            },
        )
        .unwrap();
    board.hide("log", "owner").unwrap();
    board
        .open(
            "log",
            "owner",
            target.clone(),
            "Build",
            Content::Text {
                text: "Second".into(),
            },
        )
        .unwrap();
    assert!(board.visible(&target).is_none());
    let original = board.list("owner").remove(0);
    let decoded: guild_session::overlay::Overlay =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn switching_surfaces_only_hides_the_same_target_and_keeps_content() {
    let pane = Target::Pane(guild_session::PaneId::new());
    let sibling = Target::Pane(guild_session::PaneId::new());
    let mut board = Board::default();
    for (id, target) in [
        ("build", pane.clone()),
        ("test", pane.clone()),
        ("search", sibling.clone()),
    ] {
        board
            .open(id, "native", target, id, Content::Text { text: id.into() })
            .unwrap();
    }
    assert_eq!(board.visible(&pane).unwrap().id, "test");
    board.show("build", "native").unwrap();
    assert_eq!(board.visible(&pane).unwrap().id, "build");
    assert_eq!(board.visible(&sibling).unwrap().id, "search");
    assert!(!board.get("test").unwrap().visible);
    assert_eq!(
        board.get("test").unwrap().content,
        Content::Text {
            text: "test".into()
        }
    );
    board.close("build", "native").unwrap();
    assert!(board.get("build").is_none());
    assert_eq!(board.visible(&sibling).unwrap().id, "search");
}

#[test]
fn text_and_terminal_visibility_are_independent_channels() {
    let target = Target::Session(SessionId::new());
    let mut board = Board::default();
    board
        .open(
            "terminal-task",
            "owner",
            target.clone(),
            "Terminal",
            Content::Terminal {
                job: guild_session::terminal_job::JobId::new_v4(),
            },
        )
        .unwrap();
    for id in ["log", "build"] {
        board
            .open(
                id,
                "owner",
                target.clone(),
                id,
                Content::Text { text: id.into() },
            )
            .unwrap();
        assert!(
            board.get("terminal-task").unwrap().visible,
            "text must not hide terminal"
        );
    }
    assert!(!board.get("log").unwrap().visible);
    board.show("log", "owner").unwrap();
    assert!(board.get("terminal-task").unwrap().visible);
    assert!(!board.get("build").unwrap().visible);
    board.hide("terminal-task", "owner").unwrap();
    assert!(board.get("log").unwrap().visible);
    board.show("terminal-task", "owner").unwrap();
    assert!(board.get("log").unwrap().visible);
    board.hide("log", "owner").unwrap();
    assert!(board.get("terminal-task").unwrap().visible);
}

#[test]
fn historical_overlay_json_needs_no_presentation_fields() {
    let old = r#"{"id":"log","owner":"native","target":{"kind":"pane","id":"00000000-0000-0000-0000-000000000001"},"title":"Build","content":{"kind":"text","text":"<b>literal</b>"},"visible":false}"#;
    let decoded: guild_session::overlay::Overlay = serde_json::from_str(old).unwrap();
    assert!(!decoded.visible);
    assert_eq!(
        serde_json::to_value(decoded).unwrap(),
        serde_json::from_str::<serde_json::Value>(old).unwrap()
    );
}

#[test]
fn presentation_follows_live_session_identity_and_is_removed_with_its_owner() {
    use guild_session::overlay::{Board, Content, Target};
    let session = guild_session::SessionId::new();
    let unrelated = guild_session::SessionId::new();
    let mut board = Board::default();
    board
        .open(
            "terminal-task",
            "owner",
            Target::Session(session),
            "Terminal",
            Content::Terminal {
                job: guild_session::terminal_job::JobId::new_v4(),
            },
        )
        .unwrap();
    board.prune_sessions(&[unrelated, session]);
    assert!(
        board.get("terminal-task").is_some(),
        "moving a live owner preserves its identity"
    );
    board.prune_sessions(&[unrelated]);
    assert!(
        board.get("terminal-task").is_none(),
        "closed owners cannot attach to another session"
    );
}

#[test]
fn a_text_update_cannot_replace_an_owned_terminal_channel() {
    use guild_session::overlay::{Board, Content, Target};
    let session = guild_session::SessionId::new();
    let mut board = Board::default();
    board
        .open(
            "terminal-task",
            "owner",
            Target::Session(session),
            "Terminal",
            Content::Terminal {
                job: guild_session::terminal_job::JobId::new_v4(),
            },
        )
        .unwrap();
    let before = board.get("terminal-task").unwrap().clone();
    assert!(
        board
            .open(
                "terminal-task",
                "owner",
                Target::Session(session),
                "literal",
                Content::Text {
                    text: "literal".into()
                }
            )
            .is_err()
    );
    assert_eq!(board.get("terminal-task"), Some(&before));
}
