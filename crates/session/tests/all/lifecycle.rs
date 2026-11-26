//! Restoring conversations by their own harness identity.

/// What to do with a saved session is decided by the disk, not by our
/// flag.
///
/// There are three cases, and the third cost a dead tab when restoring.
/// `claude` creates the transcript file as soon as a session starts,
/// before the first message: it considers such an id taken, yet there is
/// nothing to continue there. Both answers were captured from live runs —
/// `Error: Session ID … is already in use` and `No conversation found with
/// session ID`.
#[test]
fn a_session_id_with_only_a_stub_can_neither_be_resumed_nor_started() {
    use guild_session::{Continuation, continuation};

    assert_eq!(
        continuation(true, true),
        Continuation::Resume,
        "there is a conversation — continue"
    );
    assert_eq!(
        continuation(false, false),
        Continuation::Start,
        "the id is free — start"
    );
    assert_eq!(
        continuation(false, true),
        Continuation::Impossible,
        "a stub without a conversation: nothing to continue, nothing to start"
    );
}

/// A record comes back by its own harness's id. Claude's transcripts are
/// asked only for a Claude record: a Codex or OpenCode record's id is
/// Guild's, never a Claude conversation, and asking there answered "start
/// new" or "forget" for a conversation that was sitting in the harness's
/// own history.
#[test]
fn each_harness_comes_back_by_its_own_id() {
    use guild_session::{Continuation, Conversation, Resumption, Runs, SessionMode, resumption};

    let never =
        || -> Continuation { panic!("Claude's transcripts were asked for another harness") };
    for runs in [
        Runs::Codex(SessionMode::Terminal),
        Runs::OpenCode(SessionMode::Terminal),
    ] {
        assert_eq!(
            resumption(runs, "guild-id", Some("theirs"), never),
            Resumption::Continue(Conversation::Resume("theirs".into())),
            "{runs:?} that reported its id resumes that id"
        );
        assert!(
            !matches!(
                resumption(runs, "guild-id", None, never),
                Resumption::Continue(Conversation::New(_))
            ),
            "missing identity must not silently start another conversation"
        );
    }

    let claude = Runs::Claude(SessionMode::Terminal);
    assert_eq!(
        resumption(claude, "id", None, || Continuation::Resume),
        Resumption::Continue(Conversation::Resume("id".into()))
    );
    assert_eq!(
        resumption(claude, "id", None, || Continuation::Start),
        Resumption::Continue(Conversation::New("id".into()))
    );
    assert_eq!(
        resumption(claude, "id", None, || Continuation::Impossible),
        Resumption::Forget
    );

    assert_eq!(
        resumption(Runs::Shell, "id", None, never),
        Resumption::Fresh,
        "a shell has nothing to continue"
    );
}
