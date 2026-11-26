//! A merge request as the column sees it, whoever hosts it — GLD-063.
//!
//! Provider-neutral and pure: an adapter reads GitLab (or, later, another
//! forge) into a [`MergeRequest`], and this module says what the row
//! draws — the icon's colour ([`verdict`]), the corner badge ([`badge`])
//! and the hover card's path to merge ([`checklist`]). Words are the
//! window's: a [`Check`] is a fact, `i18n` says it.

/// Where an MR is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Open (GitLab's `opened`, and its transient `locked`).
    Open,
    /// Merged into its target.
    Merged,
    /// Closed without a merge.
    Closed,
}

/// The MR head's pipeline, in four words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pipeline {
    /// It passed.
    Passed,
    /// It has not finished: created, pending, running, waiting.
    Running,
    /// It failed.
    Failed,
    /// GitLab requires one and none ran (`ci_must_pass` with no head
    /// pipeline).
    Missing,
    /// It stopped another way — canceled, skipped, manual — in GitLab's word.
    Other(String),
}

/// Why GitLab will not merge an MR, when the checklist's own lines do not
/// say it: GitLab's `detailed_merge_status` read into a reason the window
/// can translate, or kept raw when Guild does not know it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `merge_request_blocked` (`blocked_status` before 16.x): it depends
    /// on another MR.
    AnotherMergeRequest,
    /// `external_status_checks`.
    ExternalChecks,
    /// `requested_changes`.
    ChangesRequested,
    /// `jira_association_missing`.
    JiraIssue,
    /// `merge_time`: a scheduled merge time not reached.
    MergeTime,
    /// `security_policy_violations` and its evaluation.
    SecurityPolicy,
    /// `locked_paths`.
    LockedPaths,
    /// `locked_lfs_files`.
    LockedLfsFiles,
    /// `title_regex`.
    TitlePattern,
    /// `commits_status`.
    CommitStatus,
    /// `not_approved` with no approvals line failing to say it: the
    /// approvals not offered, or contradicting GitLab (code review r2,
    /// bugs+impl-9).
    NotApproved,
    /// A status Guild does not know, as GitLab spelled it.
    Unknown(String),
}

/// Approvals given against approvals required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approvals {
    /// How many approved.
    pub given: u32,
    /// How many the rules ask for.
    pub required: u32,
    /// GitLab's own answer that the rules are met.
    pub satisfied: bool,
}

/// One MR, read. A check the answer did not establish is `None`, and a
/// `None` never earns the ✓ badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequest {
    /// The project-local number, `!294`.
    pub iid: u64,
    /// The title, plain text.
    pub title: String,
    /// Where it opens in the browser.
    pub web_url: String,
    /// Open, merged or closed.
    pub state: State,
    /// The branch it merges into.
    pub target_branch: String,
    /// A draft cannot be merged.
    pub draft: bool,
    /// It conflicts with its target.
    pub conflict: bool,
    /// GitLab's detailed status is `mergeable`.
    pub mergeable: bool,
    /// GitLab asks for a rebase onto the target (`need_rebase`).
    pub needs_rebase: bool,
    /// GitLab has not settled its status yet (`checking`, `unchecked`, …).
    pub checking: bool,
    /// A block the checklist does not otherwise explain.
    pub block: Option<Block>,
    /// The approvals, when read.
    pub approvals: Option<Approvals>,
    /// Threads a person can resolve and nobody has, when read.
    pub unresolved_threads: Option<u32>,
    /// The head's pipeline; `None` when there is none.
    pub pipeline: Option<Pipeline>,
    /// When GitLab last changed it, seconds since the epoch.
    pub updated_at: Option<i64>,
    /// When it was merged or closed, seconds since the epoch.
    pub finished_at: Option<i64>,
}

/// The icon's colour: the MR's overall verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Green: GitLab would merge it.
    Mergeable,
    /// Brass: open, waiting on people or on GitLab.
    Waiting,
    /// Rust: needs a code change — a conflict or a failed pipeline.
    NeedsChange,
    /// Grey: a draft.
    Draft,
    /// Violet: merged.
    Merged,
    /// Faint: closed.
    Closed,
}

/// The icon's corner: the most important fact still in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// ✗ the pipeline failed.
    Failed,
    /// 💬 unresolved threads.
    Threads,
    /// ↓ behind the target: a rebase is needed.
    Behind,
    /// 👁 awaiting approval.
    Approval,
    /// ⏱ the pipeline is running.
    Running,
    /// ✓ everything else is fine.
    Fine,
}

/// One fact on the path to merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// `Approved 1 of 1`.
    Approvals {
        /// Given.
        given: u32,
        /// Required.
        required: u32,
    },
    /// `6 unresolved threads`, or none.
    Threads(u32),
    /// The pipeline's state.
    Pipeline(Pipeline),
    /// A conflict with the target.
    Conflict {
        /// The target branch.
        target: String,
    },
    /// No conflict with the target.
    NoConflict {
        /// The target branch.
        target: String,
    },
    /// A rebase onto the target is needed.
    Rebase {
        /// The target branch.
        target: String,
    },
    /// GitLab blocks it for its own reason.
    Blocked(Block),
    /// It is a draft.
    Draft,
    /// Commits on the branch a push would send.
    Unpushed(u32),
    /// Merged, and when.
    Merged(Option<i64>),
    /// Closed, and when.
    Closed(Option<i64>),
}

/// A line of the hover card: a fact, and whether it is in the way —
/// `Some(true)` ✓, `Some(false)` ✗, `None` neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The fact.
    pub check: Check,
    /// ✓, ✗ or neither.
    pub ok: Option<bool>,
}

/// The icon's colour, first match winning: merged, closed, draft, a red
/// condition, GitLab's mergeable, and brass for everything else open —
/// GitLab still checking and a block it names included.
pub fn verdict(mr: &MergeRequest) -> Verdict {
    match mr.state {
        State::Merged => Verdict::Merged,
        State::Closed => Verdict::Closed,
        State::Open if mr.draft => Verdict::Draft,
        State::Open if mr.conflict || mr.pipeline == Some(Pipeline::Failed) => Verdict::NeedsChange,
        State::Open if mr.mergeable && mr.block.is_none() => Verdict::Mergeable,
        State::Open => Verdict::Waiting,
    }
}

/// The corner badge, first match winning: ✗ failed pipeline, 💬 threads,
/// ↓ rebase, 👁 approval, ⏱ running, and ✓ **only when every other check
/// is established and passing** — approvals and threads read, nothing
/// running, GitLab settled and naming no block. A conflict is never a
/// badge: the red colour says it. Merged and closed have none.
pub fn badge(mr: &MergeRequest) -> Option<Badge> {
    if mr.state != State::Open {
        return None;
    }
    if mr.pipeline == Some(Pipeline::Failed) {
        return Some(Badge::Failed);
    }
    if mr.unresolved_threads.is_some_and(|n| n > 0) {
        return Some(Badge::Threads);
    }
    if mr.needs_rebase {
        return Some(Badge::Behind);
    }
    if mr.approvals.is_some_and(|a| !a.satisfied) {
        return Some(Badge::Approval);
    }
    if mr.pipeline == Some(Pipeline::Running) {
        return Some(Badge::Running);
    }
    let established = mr.approvals.is_some_and(|a| a.satisfied)
        && mr.unresolved_threads == Some(0)
        && !mr.checking
        && mr.block.is_none()
        && !matches!(mr.pipeline, Some(Pipeline::Other(_) | Pipeline::Missing));
    established.then_some(Badge::Fine)
}

/// The hover card's lines, in order: approvals, threads, pipeline,
/// conflict, rebase, GitLab's block, draft, then the commits not pushed.
/// A merged or closed MR says only that. A check not read adds no line.
pub fn checklist(mr: &MergeRequest, unpushed: Option<u32>) -> Vec<Line> {
    match mr.state {
        State::Merged => {
            return vec![Line {
                check: Check::Merged(mr.finished_at),
                ok: None,
            }];
        }
        State::Closed => {
            return vec![Line {
                check: Check::Closed(mr.finished_at),
                ok: None,
            }];
        }
        State::Open => {}
    }
    let mut lines = Vec::new();
    if let Some(approvals) = mr.approvals {
        lines.push(Line {
            check: Check::Approvals {
                given: approvals.given,
                required: approvals.required,
            },
            ok: Some(approvals.satisfied),
        });
    }
    if let Some(threads) = mr.unresolved_threads {
        lines.push(Line {
            check: Check::Threads(threads),
            ok: Some(threads == 0),
        });
    }
    if let Some(pipeline) = &mr.pipeline {
        lines.push(Line {
            ok: Some(*pipeline == Pipeline::Passed),
            check: Check::Pipeline(pipeline.clone()),
        });
    }
    let target = mr.target_branch.clone();
    lines.push(if mr.conflict {
        Line {
            check: Check::Conflict { target },
            ok: Some(false),
        }
    } else {
        Line {
            check: Check::NoConflict { target },
            ok: Some(true),
        }
    });
    if mr.needs_rebase {
        lines.push(Line {
            check: Check::Rebase {
                target: mr.target_branch.clone(),
            },
            ok: Some(false),
        });
    }
    if let Some(block) = &mr.block {
        lines.push(Line {
            check: Check::Blocked(block.clone()),
            ok: Some(false),
        });
    }
    if mr.draft {
        lines.push(Line {
            check: Check::Draft,
            ok: Some(false),
        });
    }
    if let Some(n) = unpushed.filter(|n| *n > 0) {
        lines.push(Line {
            check: Check::Unpushed(n),
            ok: None,
        });
    }
    lines
}

/// The first line in the way — what the status bar says beside `!N ↗`.
pub fn first_failing(mr: &MergeRequest) -> Option<Line> {
    checklist(mr, None)
        .into_iter()
        .find(|line| line.ok == Some(false))
}

/// GitLab's `detailed_merge_status` values the checklist's own lines, or
/// the verdict, already explain. Anything else is a [`Block`].
pub const EXPLAINED: &[&str] = &[
    "mergeable",
    "conflict",
    "need_rebase",
    "discussions_not_resolved",
    "not_approved",
    "ci_must_pass",
    "ci_still_running",
    "draft_status",
    "not_open",
    "checking",
    "unchecked",
    "preparing",
    "approvals_syncing",
];

/// What GitLab's `detailed_merge_status` adds to an MR: whether it is
/// mergeable, needs a rebase, is still being checked, or is blocked for a
/// reason only GitLab names.
pub fn read_detailed_status(status: Option<&str>, mr: &mut MergeRequest) {
    let Some(status) = status else {
        // No status at all: nothing is established.
        mr.checking = true;
        return;
    };
    match status {
        "mergeable" => mr.mergeable = true,
        "conflict" => mr.conflict = true,
        "need_rebase" => mr.needs_rebase = true,
        "checking" | "unchecked" | "preparing" | "approvals_syncing" => mr.checking = true,
        other if EXPLAINED.contains(&other) => {}
        "merge_request_blocked" | "blocked_status" => mr.block = Some(Block::AnotherMergeRequest),
        "external_status_checks" | "status_checks_must_pass" => {
            mr.block = Some(Block::ExternalChecks)
        }
        "requested_changes" => mr.block = Some(Block::ChangesRequested),
        "jira_association_missing" => mr.block = Some(Block::JiraIssue),
        "merge_time" => mr.block = Some(Block::MergeTime),
        "security_policy_violations" | "security_policy_evaluation" => {
            mr.block = Some(Block::SecurityPolicy)
        }
        "locked_paths" => mr.block = Some(Block::LockedPaths),
        "locked_lfs_files" => mr.block = Some(Block::LockedLfsFiles),
        "title_regex" => mr.block = Some(Block::TitlePattern),
        "commits_status" => mr.block = Some(Block::CommitStatus),
        other => mr.block = Some(Block::Unknown(other.to_owned())),
    }
}

/// GitLab's `not_approved`, once the approvals were read: explained by the
/// approvals line when that line fails, and otherwise GitLab's own block,
/// said without a count (code review r2, bugs+impl-9).
pub fn settle_approvals(status: Option<&str>, mr: &mut MergeRequest) {
    if status == Some("not_approved") && !mr.approvals.is_some_and(|a| !a.satisfied) {
        mr.block = Some(Block::NotApproved);
    }
}

/// GitLab's pipeline word, read into four.
pub fn read_pipeline(status: &str) -> Pipeline {
    match status {
        "success" | "passed" => Pipeline::Passed,
        "failed" => Pipeline::Failed,
        "created"
        | "pending"
        | "running"
        | "preparing"
        | "waiting_for_resource"
        | "waiting_for_callback"
        | "scheduled" => Pipeline::Running,
        other => Pipeline::Other(other.to_owned()),
    }
}
