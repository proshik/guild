//! A merge request's verdict, badge and path to merge — GLD-063.
//!
//! The model is provider-neutral and pure: what GitLab answered, already
//! read, goes in; the colour, the corner badge and the hover card's lines
//! come out.
use guild_session::merge_requests::status::{
    Approvals, Badge, Block, Check, MergeRequest, Pipeline, State, Verdict, badge, checklist,
    first_failing, verdict,
};

/// An open MR GitLab would merge: approved, no threads, pipeline passed.
fn ready() -> MergeRequest {
    MergeRequest {
        iid: 294,
        title: "[ATL-740] back up the graph store".into(),
        web_url: "https://gitlab.example.com/team/app/-/merge_requests/294".into(),
        state: State::Open,
        target_branch: "master".into(),
        draft: false,
        conflict: false,
        mergeable: true,
        needs_rebase: false,
        checking: false,
        block: None,
        approvals: Some(Approvals {
            given: 1,
            required: 1,
            satisfied: true,
        }),
        unresolved_threads: Some(0),
        pipeline: Some(Pipeline::Passed),
        updated_at: Some(0),
        finished_at: None,
    }
}

#[test]
fn a_mergeable_mr_is_green_with_a_check() {
    let mr = ready();
    assert_eq!(verdict(&mr), Verdict::Mergeable);
    assert_eq!(badge(&mr), Some(Badge::Fine));
}

#[test]
fn a_conflict_with_nothing_else_left_is_red_with_a_check() {
    let mr = MergeRequest {
        conflict: true,
        mergeable: false,
        ..ready()
    };
    assert_eq!(verdict(&mr), Verdict::NeedsChange);
    assert_eq!(
        badge(&mr),
        Some(Badge::Fine),
        "would be mergeable but for it"
    );
}

#[test]
fn a_failed_pipeline_is_red_with_a_cross() {
    let mr = MergeRequest {
        pipeline: Some(Pipeline::Failed),
        mergeable: false,
        ..ready()
    };
    assert_eq!(verdict(&mr), Verdict::NeedsChange);
    assert_eq!(badge(&mr), Some(Badge::Failed));
}

#[test]
fn waiting_on_people_is_brass_and_the_badge_says_what_first() {
    let waiting = MergeRequest {
        mergeable: false,
        ..ready()
    };
    let threads = MergeRequest {
        unresolved_threads: Some(6),
        approvals: Some(Approvals {
            given: 0,
            required: 1,
            satisfied: false,
        }),
        ..waiting.clone()
    };
    assert_eq!(verdict(&threads), Verdict::Waiting);
    assert_eq!(
        badge(&threads),
        Some(Badge::Threads),
        "threads before approval"
    );
    let approval = MergeRequest {
        approvals: Some(Approvals {
            given: 0,
            required: 1,
            satisfied: false,
        }),
        pipeline: Some(Pipeline::Running),
        ..waiting.clone()
    };
    assert_eq!(
        badge(&approval),
        Some(Badge::Approval),
        "approval before running"
    );
    let running = MergeRequest {
        pipeline: Some(Pipeline::Running),
        ..waiting.clone()
    };
    assert_eq!(badge(&running), Some(Badge::Running));
    let behind = MergeRequest {
        needs_rebase: true,
        approvals: Some(Approvals {
            given: 0,
            required: 1,
            satisfied: false,
        }),
        ..waiting
    };
    assert_eq!(
        badge(&behind),
        Some(Badge::Behind),
        "rebase before approval"
    );
}

#[test]
fn a_failed_pipeline_comes_before_every_other_badge() {
    let mr = MergeRequest {
        pipeline: Some(Pipeline::Failed),
        unresolved_threads: Some(3),
        needs_rebase: true,
        ..ready()
    };
    assert_eq!(badge(&mr), Some(Badge::Failed));
}

#[test]
fn a_draft_is_grey_whatever_else_is_true_and_keeps_its_badge() {
    let mr = MergeRequest {
        draft: true,
        conflict: true,
        mergeable: false,
        unresolved_threads: Some(2),
        ..ready()
    };
    assert_eq!(verdict(&mr), Verdict::Draft);
    assert_eq!(badge(&mr), Some(Badge::Threads));
}

#[test]
fn merged_and_closed_have_no_badge() {
    let merged = MergeRequest {
        state: State::Merged,
        ..ready()
    };
    assert_eq!(verdict(&merged), Verdict::Merged);
    assert_eq!(badge(&merged), None);
    let closed = MergeRequest {
        state: State::Closed,
        ..ready()
    };
    assert_eq!(verdict(&closed), Verdict::Closed);
    assert_eq!(badge(&closed), None);
}

#[test]
fn no_check_when_a_check_is_not_established() {
    let no_approvals = MergeRequest {
        approvals: None,
        ..ready()
    };
    assert_eq!(badge(&no_approvals), None, "approvals not read");
    let no_threads = MergeRequest {
        unresolved_threads: None,
        ..ready()
    };
    assert_eq!(badge(&no_threads), None, "discussions not read");
    let checking = MergeRequest {
        checking: true,
        mergeable: false,
        ..ready()
    };
    assert_eq!(badge(&checking), None, "GitLab still checking");
    assert_eq!(verdict(&checking), Verdict::Waiting);
}

#[test]
fn a_block_gitlab_names_is_brass_with_no_badge_and_says_why() {
    let mr = MergeRequest {
        mergeable: false,
        block: Some(Block::AnotherMergeRequest),
        ..ready()
    };
    assert_eq!(verdict(&mr), Verdict::Waiting);
    assert_eq!(badge(&mr), None);
    let lines = checklist(&mr, None);
    assert!(
        lines.iter().any(
            |line| line.check == Check::Blocked(Block::AnotherMergeRequest)
                && line.ok == Some(false)
        ),
        "{lines:?}"
    );
    assert_eq!(
        first_failing(&mr).map(|line| line.check),
        Some(Check::Blocked(Block::AnotherMergeRequest))
    );
    // A red condition still wins the colour.
    let red = MergeRequest {
        conflict: true,
        ..mr
    };
    assert_eq!(verdict(&red), Verdict::NeedsChange);
}

#[test]
fn the_path_to_merge_lists_every_check_in_order() {
    let mr = MergeRequest {
        mergeable: false,
        conflict: true,
        unresolved_threads: Some(6),
        ..ready()
    };
    let lines = checklist(&mr, Some(2));
    let checks: Vec<(Check, Option<bool>)> = lines
        .into_iter()
        .map(|line| (line.check, line.ok))
        .collect();
    assert_eq!(
        checks,
        vec![
            (
                Check::Approvals {
                    given: 1,
                    required: 1
                },
                Some(true)
            ),
            (Check::Threads(6), Some(false)),
            (Check::Pipeline(Pipeline::Passed), Some(true)),
            (
                Check::Conflict {
                    target: "master".into()
                },
                Some(false)
            ),
            (Check::Unpushed(2), None),
        ]
    );
    assert_eq!(
        first_failing(&mr).map(|line| line.check),
        Some(Check::Threads(6))
    );
}

#[test]
fn a_clean_mr_lists_no_conflict_and_nothing_failing() {
    let lines = checklist(&ready(), None);
    assert!(lines.iter().all(|line| line.ok != Some(false)), "{lines:?}");
    assert!(lines.iter().any(|line| line.check
        == Check::NoConflict {
            target: "master".into()
        }));
    assert_eq!(first_failing(&ready()), None);
}

#[test]
fn a_rebase_a_draft_and_a_running_pipeline_are_failing_lines() {
    let mr = MergeRequest {
        mergeable: false,
        needs_rebase: true,
        draft: true,
        pipeline: Some(Pipeline::Running),
        ..ready()
    };
    let lines = checklist(&mr, None);
    for check in [
        Check::Pipeline(Pipeline::Running),
        Check::Rebase {
            target: "master".into(),
        },
        Check::Draft,
    ] {
        assert!(
            lines
                .iter()
                .any(|line| line.check == check && line.ok == Some(false)),
            "{check:?} in {lines:?}"
        );
    }
}

#[test]
fn merged_and_closed_say_only_what_happened() {
    let merged = MergeRequest {
        state: State::Merged,
        finished_at: Some(100),
        ..ready()
    };
    let lines = checklist(&merged, Some(3));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].check, Check::Merged(Some(100)));
    assert_eq!(lines[0].ok, None);
    assert_eq!(first_failing(&merged), None);
}

#[test]
fn a_pipeline_gitlab_has_not_run_adds_no_line() {
    let mr = MergeRequest {
        pipeline: None,
        ..ready()
    };
    assert!(
        checklist(&mr, None)
            .iter()
            .all(|line| !matches!(line.check, Check::Pipeline(_)))
    );
    assert_eq!(badge(&mr), Some(Badge::Fine));
}

#[test]
fn gitlab_detailed_statuses_read_into_the_model() {
    use guild_session::merge_requests::status::{EXPLAINED, read_detailed_status};
    let blank = || MergeRequest {
        mergeable: false,
        approvals: None,
        unresolved_threads: None,
        pipeline: None,
        ..ready()
    };
    let read = |status: Option<&str>| {
        let mut mr = blank();
        read_detailed_status(status, &mut mr);
        mr
    };
    assert!(read(Some("mergeable")).mergeable);
    assert!(read(Some("conflict")).conflict);
    assert!(read(Some("need_rebase")).needs_rebase);
    assert!(read(Some("checking")).checking);
    assert!(read(None).checking, "no status establishes nothing");
    assert_eq!(
        read(Some("merge_request_blocked")).block,
        Some(Block::AnotherMergeRequest)
    );
    assert_eq!(
        read(Some("requested_changes")).block,
        Some(Block::ChangesRequested)
    );
    assert_eq!(
        read(Some("something_new")).block,
        Some(Block::Unknown("something_new".into()))
    );
    // Every status the checklist explains adds no block.
    for status in EXPLAINED {
        assert_eq!(read(Some(status)).block, None, "{status}");
    }
}

#[test]
fn gitlab_pipeline_words_read_into_four() {
    use guild_session::merge_requests::status::read_pipeline;
    assert_eq!(read_pipeline("success"), Pipeline::Passed);
    assert_eq!(read_pipeline("failed"), Pipeline::Failed);
    assert_eq!(read_pipeline("running"), Pipeline::Running);
    assert_eq!(read_pipeline("pending"), Pipeline::Running);
    assert_eq!(
        read_pipeline("canceled"),
        Pipeline::Other("canceled".into())
    );
}

/// Code review r2, bugs+impl-9: GitLab's `not_approved` is explained by the
/// approvals line only when that line fails; otherwise it is GitLab's own
/// block, said without a count.
#[test]
fn not_approved_is_explained_only_by_a_failing_approvals_line() {
    use guild_session::merge_requests::status::settle_approvals;
    let waiting = |approvals| MergeRequest {
        mergeable: false,
        approvals,
        ..ready()
    };
    let mut unknown = waiting(None);
    settle_approvals(Some("not_approved"), &mut unknown);
    assert_eq!(unknown.block, Some(Block::NotApproved));
    assert!(first_failing(&unknown).is_some());

    let mut failing = waiting(Some(Approvals {
        given: 0,
        required: 1,
        satisfied: false,
    }));
    settle_approvals(Some("not_approved"), &mut failing);
    assert_eq!(failing.block, None, "the approvals line says it");

    let mut contradicted = waiting(Some(Approvals {
        given: 1,
        required: 1,
        satisfied: true,
    }));
    settle_approvals(Some("not_approved"), &mut contradicted);
    assert_eq!(contradicted.block, Some(Block::NotApproved));

    let mut other = waiting(None);
    settle_approvals(Some("mergeable"), &mut other);
    assert_eq!(other.block, None);
}
