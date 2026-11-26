//! When a worktree's merge request is asked again, and what a reply may
//! change — GLD-063. Pure, on virtual time: no glab, no window.
use guild_desktop::forge::{
    Answer, Batch, Candidate, Forge, Problem, Progress, Resolution, Shown, Trigger, Why, resolve,
};
use guild_session::merge_requests::{
    glab::Error,
    hosts::Hosts,
    status::{MergeRequest, State},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const URL: &str = "git@gitlab.example.com:team/app.git";
const HOST: &str = "gitlab.example.com";
const PROJECT: &str = "team/app";

fn repo() -> PathBuf {
    PathBuf::from("/work/app/.git")
}

fn candidate(worktree: &str, branch: &str) -> Candidate {
    Candidate {
        worktree: PathBuf::from(worktree),
        url: URL.into(),
        branch: branch.into(),
    }
}

fn mr(iid: u64) -> MergeRequest {
    MergeRequest {
        iid,
        title: format!("Change {iid}"),
        web_url: format!("https://{HOST}/{PROJECT}/-/merge_requests/{iid}"),
        state: State::Open,
        target_branch: "main".into(),
        draft: false,
        conflict: false,
        mergeable: true,
        needs_rebase: false,
        checking: false,
        block: None,
        approvals: None,
        unresolved_threads: Some(0),
        pipeline: None,
        updated_at: None,
        finished_at: None,
    }
}

/// The answer a refresh would bring for what it was asked.
fn answer(forge_ask: &guild_desktop::forge::Ask, found: &[(&str, Option<u64>)]) -> Answer {
    Answer {
        repo: forge_ask.repo.clone(),
        candidates: forge_ask.candidates.clone(),
        generation: forge_ask.generation,
        resolutions: forge_ask
            .candidates
            .iter()
            .map(|c| {
                (
                    c.clone(),
                    Resolution::Asked {
                        host: HOST.into(),
                        project: PROJECT.into(),
                    },
                )
            })
            .collect(),
        batches: vec![Batch {
            host: HOST.into(),
            project: PROJECT.into(),
            result: Ok(found
                .iter()
                .map(|(branch, iid)| ((*branch).to_owned(), Ok(iid.map(mr))))
                .collect()),
        }],
    }
}

/// The answer a refresh would bring when each asked branch's own read
/// timed out.
fn failing(forge_ask: &guild_desktop::forge::Ask) -> Answer {
    let mut failed = answer(forge_ask, &[]);
    failed.batches[0].result = Ok(forge_ask
        .candidates
        .iter()
        .map(|c| (c.branch.clone(), Err(Error::TimedOut)))
        .collect());
    failed
}

fn shown_iid(forge: &Forge, worktree: &str) -> Option<u64> {
    match forge.shown(Path::new(worktree)) {
        Shown::Mr { mr, .. } => Some(mr.iid),
        _ => None,
    }
}

#[test]
fn a_worktree_never_asked_is_asked_once() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let asks = forge.take_due(now);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].candidates, vec![candidate("/work/app", "feat")]);
    assert!(forge.take_due(now).is_empty(), "one in flight is enough");
    forge.land(answer(&asks[0], &[("feat", Some(294))]), now);
    assert_eq!(shown_iid(&forge, "/work/app"), Some(294));
    // The same inputs again ask nothing.
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    assert!(forge.take_due(now).is_empty());
}

#[test]
fn focus_asks_again_only_after_thirty_seconds() {
    let start = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], start);
    let ask = forge.take_due(start).remove(0);
    forge.land(answer(&ask, &[("feat", Some(1))]), start);
    forge.trigger(None, Trigger::Focus, start + Duration::from_secs(10));
    assert!(forge.take_due(start + Duration::from_secs(10)).is_empty());
    forge.trigger(None, Trigger::Focus, start + Duration::from_secs(31));
    assert_eq!(forge.take_due(start + Duration::from_secs(31)).len(), 1);
}

#[test]
fn the_clock_asks_every_two_minutes() {
    let start = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], start);
    let ask = forge.take_due(start).remove(0);
    forge.land(answer(&ask, &[("feat", Some(1))]), start);
    forge.trigger(None, Trigger::Tick, start + Duration::from_secs(119));
    assert!(forge.take_due(start + Duration::from_secs(119)).is_empty());
    forge.trigger(None, Trigger::Tick, start + Duration::from_secs(120));
    assert_eq!(forge.take_due(start + Duration::from_secs(120)).len(), 1);
}

#[test]
fn a_push_or_a_switch_from_guild_asks_once_its_snapshot_lands() {
    let start = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], start);
    let ask = forge.take_due(start).remove(0);
    forge.land(answer(&ask, &[("feat", Some(1))]), start);
    let then = start + Duration::from_secs(1);
    forge.wrote(&repo(), then);
    assert!(
        forge.take_due(then).is_empty(),
        "not before the snapshot after it"
    );
    forge.snapshot_landed(&repo());
    assert_eq!(forge.take_due(then).len(), 1);
    // Another repository's write asks nothing of this one.
    let later = start + Duration::from_secs(2);
    forge.wrote(Path::new("/elsewhere/.git"), later);
    assert!(forge.take_due(later).is_empty());
}

#[test]
fn triggers_while_a_refresh_is_out_are_one_pending_refresh() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    for _ in 0..3 {
        forge.trigger(None, Trigger::Tick, now + Duration::from_secs(600));
    }
    assert!(forge.take_due(now).is_empty(), "not beside the one out");
    forge.land(answer(&ask, &[("feat", Some(1))]), now);
    assert_eq!(forge.take_due(now).len(), 1, "one pending refresh");
    assert!(forge.take_due(now).is_empty());
}

/// bugs+impl-5, in the window's own order: the switch completes (its
/// write's line lands) while the read for A is out and **before** the
/// worktree list after the switch has landed — the column still says A.
/// A's reply is dropped, nothing is asked until the new list lands, and
/// then B is asked.
#[test]
fn a_reply_for_a_branch_switched_while_it_was_out_is_dropped() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "a")], now);
    let first = forge.take_due(now).remove(0);
    forge.wrote(&repo(), now);
    // The next poll still builds A from the list before the switch.
    forge.inputs(&repo(), vec![candidate("/work/app", "a")], now);
    forge.land(answer(&first, &[("a", Some(1))]), now);
    assert_eq!(shown_iid(&forge, "/work/app"), None, "A's MR is not B's");
    assert!(forge.take_due(now).is_empty(), "A is not asked again");
    // The list after the switch lands.
    forge.snapshot_landed(&repo());
    forge.inputs(&repo(), vec![candidate("/work/app", "b")], now);
    let second = forge.take_due(now);
    assert_eq!(second.len(), 1, "the pending refresh");
    assert_eq!(second[0].candidates, vec![candidate("/work/app", "b")]);
    forge.land(answer(&second[0], &[("b", Some(2))]), now);
    assert_eq!(shown_iid(&forge, "/work/app"), Some(2));
}

#[test]
fn off_asks_nothing_forgets_everything_and_drops_a_late_reply() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(1))]), now);
    forge.wrote(&repo(), now);
    forge.snapshot_landed(&repo());
    let out = forge.take_due(now).remove(0);
    forge.set_enabled(false, now);
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
    for trigger in [Trigger::Focus, Trigger::Tick] {
        forge.trigger(Some(&repo()), trigger, now + Duration::from_secs(600));
    }
    forge.wrote(&repo(), now);
    forge.snapshot_landed(&repo());
    forge.inputs(&repo(), vec![candidate("/work/x", "new")], now);
    assert!(forge.take_due(now + Duration::from_secs(600)).is_empty());
    // A reply from before the switch went off and on is dropped.
    forge.set_enabled(true, now);
    forge.land(answer(&out, &[("feat", Some(9))]), now);
    assert_eq!(shown_iid(&forge, "/work/app"), None);
    // On again: every repository is asked at once.
    assert_eq!(forge.take_due(now).len(), 1);
}

#[test]
fn a_failed_batch_keeps_the_last_state_and_says_so() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    let failed = failing(&ask);
    forge.land(failed, later);
    match forge.shown(Path::new("/work/app")) {
        Shown::Mr { mr, failure, .. } => {
            assert_eq!(mr.iid, 5, "never shown as no MR");
            assert_eq!(
                failure.map(|(why, _)| why),
                Some(&guild_session::merge_requests::glab::Error::TimedOut)
            );
        }
        other => panic!("{other:?}"),
    }
}

// --- which worktrees are asked, and why the others say nothing

fn hosts() -> Hosts {
    Hosts::parse(
        "hosts:\n    gitlab.example.com:\n        use_keyring: true\n    gitlab.other.example:\n        token: present\n    gitlab.signedout.example:\n        user: someone\n",
    )
}

#[test]
fn signed_in_hosts_are_batched_by_host_and_project() {
    let candidates = vec![
        candidate("/work/app", "a"),
        candidate("/work/app-b", "b"),
        Candidate {
            worktree: "/work/app-c".into(),
            url: "https://gitlab.other.example/team/app.git".into(),
            branch: "c".into(),
        },
    ];
    let (resolutions, batches) = resolve(&candidates, true, &hosts());
    assert_eq!(batches.len(), 2, "{batches:?}");
    let first = batches
        .iter()
        .find(|b| b.0 == HOST && b.1 == PROJECT)
        .unwrap();
    assert_eq!(first.2, vec!["a".to_owned(), "b".to_owned()]);
    assert!(
        resolutions
            .iter()
            .all(|(_, r)| matches!(r, Resolution::Asked { .. }))
    );
}

#[test]
fn a_reason_is_said_only_where_it_helps() {
    let at = |url: &str| Candidate {
        worktree: "/w".into(),
        url: url.into(),
        branch: "b".into(),
    };
    let one = |candidate: Candidate, located: bool| {
        let (resolutions, batches) = resolve(&[candidate], located, &hosts());
        assert!(
            batches.is_empty() || located,
            "no request without glab: {batches:?}"
        );
        resolutions.into_iter().next().unwrap().1
    };
    assert_eq!(
        one(at("git@gitlab.signedout.example:team/app.git"), true),
        Resolution::Why(Why::NotLoggedIn("gitlab.signedout.example".into()))
    );
    assert_eq!(
        one(at("git@gitlab.example.com:team/app.git"), false),
        Resolution::Why(Why::NotFound)
    );
    assert_eq!(
        one(at("git@github.com:team/app.git"), false),
        Resolution::Silent,
        "glab missing, and nothing that looks like GitLab"
    );
    assert_eq!(
        one(at("git@github.com:team/app.git"), true),
        Resolution::Silent,
        "a host glab does not know"
    );
    assert_eq!(
        one(at("/srv/app.git"), true),
        Resolution::Silent,
        "no forge"
    );
    let (_, batches) = resolve(
        &[at("git@gitlab.signedout.example:team/app.git")],
        true,
        &hosts(),
    );
    assert!(batches.is_empty(), "a signed-out host is asked nothing");
}

/// glab 1.118 refuses `--hostname host:port` ("invalid hostname"): a remote
/// whose host carries an explicit port is inactive, even signed in — no
/// glab is started for it, and the row's hover says why. The port stays in
/// the host's identity (code review r1, arch+quality-3).
#[test]
fn a_host_with_a_port_is_inactive_and_says_why() {
    use guild_desktop::i18n::Ui;
    use guild_session::Language;
    let hosts = Hosts::parse("hosts:\n    gitlab.example.com:8443:\n        token: present\n");
    let candidate = Candidate {
        worktree: "/w".into(),
        url: "https://gitlab.example.com:8443/team/app.git".into(),
        branch: "b".into(),
    };
    let (resolutions, batches) = resolve(&[candidate], true, &hosts);
    assert!(
        batches.is_empty(),
        "no glab for a host with a port: {batches:?}"
    );
    let why = Why::PortedHost("gitlab.example.com:8443".into());
    assert_eq!(resolutions[0].1, Resolution::Why(why.clone()));
    assert_eq!(
        Ui(Language::English).mr_why(&why),
        "glab cannot address a host with a port: gitlab.example.com:8443"
    );
    assert_eq!(
        Ui(Language::Russian).mr_why(&why),
        "glab не умеет обращаться к хосту с портом: gitlab.example.com:8443"
    );
}

/// A reason no glab was started is the repository's state, not the row's:
/// the row draws nothing, the status says why, and off hides it.
#[test]
fn a_reason_is_the_repository_s_not_the_row_s() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    let mut why = answer(&ask, &[]);
    why.resolutions[0].1 = Resolution::Why(Why::NotLoggedIn(HOST.into()));
    why.batches.clear();
    forge.land(why, now);
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
    let status = forge.status(&repo()).unwrap();
    assert_eq!(
        status.problems,
        &[Problem::Why(Why::NotLoggedIn(HOST.into()))]
    );
    assert_eq!(status.checked, Some(now));
    forge.set_enabled(false, now);
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
    assert_eq!(forge.status(&repo()), None);
}

// --- what the card and the status bar say, in both languages

#[test]
fn every_line_of_the_path_to_merge_is_said_in_both_languages() {
    use guild_desktop::i18n::Ui;
    use guild_session::Language;
    use guild_session::merge_requests::status::{Block, Check, Pipeline};
    let checks = [
        Check::Approvals {
            given: 1,
            required: 1,
        },
        Check::Approvals {
            given: 0,
            required: 0,
        },
        Check::Threads(0),
        Check::Threads(6),
        Check::Pipeline(Pipeline::Passed),
        Check::Pipeline(Pipeline::Running),
        Check::Pipeline(Pipeline::Failed),
        Check::Pipeline(Pipeline::Missing),
        Check::Pipeline(Pipeline::Other("manual".into())),
        Check::Conflict {
            target: "master".into(),
        },
        Check::NoConflict {
            target: "main".into(),
        },
        Check::Rebase {
            target: "main".into(),
        },
        Check::Blocked(Block::AnotherMergeRequest),
        Check::Blocked(Block::Unknown("odd_status".into())),
        Check::Draft,
        Check::Unpushed(2),
        Check::Merged(Some(0)),
        Check::Closed(None),
    ];
    for language in [Language::English, Language::Russian] {
        let ui = Ui(language);
        for check in &checks {
            let said = ui.mr_check(check, 3_600);
            assert!(!said.trim().is_empty(), "{language:?} {check:?}");
        }
        assert!(
            ui.mr_check(
                &Check::Conflict {
                    target: "master".into()
                },
                0
            )
            .contains("master")
        );
        assert!(
            ui.mr_check(&Check::Blocked(Block::Unknown("odd_status".into())), 0)
                .contains("odd_status"),
            "an unknown status says GitLab's word"
        );
        assert!(ui.mr_why(&Why::NotLoggedIn(HOST.into())).contains(HOST));
        assert!(!ui.mr_why(&Why::NotFound).is_empty());
        assert_eq!(ui.mr_meta(294, "1m ago"), "!294 · 1m ago");
    }
    let english = Ui(Language::English);
    assert_eq!(
        english.mr_check(&Check::Threads(6), 0),
        "6 unresolved threads"
    );
    assert_eq!(
        english.mr_check(&Check::Threads(1), 0),
        "1 unresolved thread"
    );
    assert_eq!(
        english.mr_status_line(&Check::Threads(6)),
        "6 unresolved comments"
    );
    assert_eq!(
        english.mr_check(
            &Check::Approvals {
                given: 1,
                required: 1
            },
            0
        ),
        "Approved 1 of 1"
    );
    assert_eq!(
        english.mr_check(&Check::Unpushed(2), 0),
        "2 commits not pushed"
    );
    assert_eq!(
        english.mr_check(&Check::Blocked(Block::AnotherMergeRequest), 0),
        "Blocked: waits for another merge request"
    );
    assert_eq!(
        english.mr_why(&Why::NotLoggedIn(HOST.into())),
        format!("glab is not logged in to {HOST}")
    );
    assert_eq!(english.mr_why(&Why::NotFound), "glab not found");
    let russian = Ui(Language::Russian);
    assert_eq!(russian.mr_check(&Check::Threads(1), 0), "1 нерешённый тред");
    assert_eq!(
        russian.mr_check(&Check::Threads(3), 0),
        "3 нерешённых треда"
    );
    assert_eq!(
        russian.mr_check(&Check::Threads(6), 0),
        "6 нерешённых тредов"
    );
    assert_eq!(
        russian.mr_check(&Check::Threads(11), 0),
        "11 нерешённых тредов"
    );
}

// --- code review r1

fn other(url: &str, worktree: &str, branch: &str) -> Candidate {
    Candidate {
        worktree: PathBuf::from(worktree),
        url: url.into(),
        branch: branch.into(),
    }
}

/// bugs+impl-6: a worktree whose push destination changed to another
/// project does not keep the first project's MR.
#[test]
fn a_destination_change_forgets_the_old_project() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    assert_eq!(shown_iid(&forge, "/work/app"), Some(5));
    forge.inputs(
        &repo(),
        vec![other(
            "git@gitlab.example.com:team/other.git",
            "/work/app",
            "feat",
        )],
        now,
    );
    assert_eq!(shown_iid(&forge, "/work/app"), None, "not team/app's !5");
    assert_eq!(forge.take_due(now).len(), 1, "the new destination is asked");
}

/// bugs+impl-13: two clones of one project keep their own state — one's
/// success does not clear the other's failure, nor its failure show on
/// the other.
#[test]
fn clones_keep_their_own_state() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    let one = PathBuf::from("/work/one/.git");
    let two = PathBuf::from("/work/two/.git");
    forge.inputs(&one, vec![candidate("/work/one", "a")], now);
    forge.inputs(&two, vec![candidate("/work/two", "b")], now);
    let asks = forge.take_due(now);
    let (ask_one, ask_two) = if asks[0].repo == one {
        (&asks[0], &asks[1])
    } else {
        (&asks[1], &asks[0])
    };
    forge.land(answer(ask_one, &[("a", Some(1))]), now);
    let failed = failing(ask_two);
    forge.land(failed, now);
    match forge.shown(Path::new("/work/one")) {
        Shown::Mr { failure, .. } => assert_eq!(failure, None, "two's failure is two's"),
        other => panic!("{other:?}"),
    }
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let asks = forge.take_due(later);
    let ask_one = asks.iter().find(|a| a.repo == one).unwrap();
    let failed = failing(ask_one);
    forge.land(failed, later);
    let ask_two = asks.iter().find(|a| a.repo == two).unwrap();
    forge.land(answer(ask_two, &[("b", Some(2))]), later);
    match forge.shown(Path::new("/work/one")) {
        Shown::Mr { failure, .. } => assert!(failure.is_some(), "two's success is not one's"),
        other => panic!("{other:?}"),
    }
}

/// adversarial-9: a first refresh that failed is unknown, and says so —
/// not the look of a confirmed "no MR".
#[test]
fn a_failed_first_refresh_is_unknown_not_no_mr() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    let failed = failing(&ask);
    forge.land(failed, now);
    match forge.shown(Path::new("/work/app")) {
        Shown::Unknown { error, .. } => {
            assert_eq!(error, &guild_session::merge_requests::glab::Error::TimedOut)
        }
        other => panic!("{other:?}"),
    }
}

/// bugs+impl-2: a repository closed while its refresh is out is not asked
/// again beside it when reopened; the old refresh is named for its owner
/// to stop.
#[test]
fn a_repository_closed_while_asked_is_not_asked_twice() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let out = forge.take_due(now).remove(0);
    let gone = forge.retain(|_| false);
    assert_eq!(gone, vec![repo()], "its refresh is still out");
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    assert!(forge.take_due(now).is_empty(), "not beside the one out");
    // Reopened with the same candidates, the cancelled refresh's answer is
    // still not the rows' (code review r2, bugs+impl-7): its success is not
    // shown as fresh, and its stop is not shown as a failure.
    assert!(!forge.land(answer(&out, &[("feat", Some(1))]), now));
    assert_eq!(shown_iid(&forge, "/work/app"), None);
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
    assert_eq!(forge.take_due(now).len(), 1, "asked once the old one ended");
}

/// Code review r2, bugs+impl-7: a stopped refresh's own failure is not
/// published after a reopen either.
#[test]
fn a_reopened_repository_shows_no_cancelled_failure() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let out = forge.take_due(now).remove(0);
    forge.retain(|_| false);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let mut stopped = answer(&out, &[]);
    stopped.batches[0].result = Err(guild_session::merge_requests::glab::Error::Stopped);
    assert!(!forge.land(stopped, now));
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
}

/// Code review r2, bugs+impl-11: a branch last read with no MR, whose next
/// refresh failed, is unknown and says why — not a confirmed absence.
#[test]
fn a_failure_after_an_empty_read_is_unknown() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", None)]), now);
    assert_eq!(forge.shown(Path::new("/work/app")), Shown::Nothing);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    let failed = failing(&ask);
    forge.land(failed, later);
    match forge.shown(Path::new("/work/app")) {
        Shown::Unknown { error, .. } => {
            assert_eq!(error, &guild_session::merge_requests::glab::Error::TimedOut)
        }
        other => panic!("{other:?}"),
    }
}

// --- the cache across a workspace switch

/// A repository that leaves the column keeps its last known MRs: shown
/// again, its rows draw them at once, and it is asked in the background.
#[test]
fn a_repository_shown_again_draws_its_last_mrs_and_is_asked() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    assert!(forge.retain(|_| false).is_empty(), "nothing out to stop");
    assert_eq!(
        shown_iid(&forge, "/work/app"),
        None,
        "not drawn while hidden"
    );
    let later = now + Duration::from_secs(600);
    for trigger in [Trigger::Focus, Trigger::Tick] {
        forge.trigger(None, trigger, later);
    }
    assert!(
        forge.take_due(later).is_empty(),
        "a hidden repository is not asked"
    );
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], later);
    assert_eq!(
        shown_iid(&forge, "/work/app"),
        Some(5),
        "at once, before any answer"
    );
    let asks = forge.take_due(later);
    assert_eq!(asks.len(), 1, "and asked again in the background");
    forge.land(answer(&asks[0], &[("feat", Some(6))]), later);
    assert_eq!(shown_iid(&forge, "/work/app"), Some(6));
}

/// An answer that lands after its repository left the column never
/// publishes — not while hidden, not after its return — and the last known
/// MR stays.
#[test]
fn an_answer_landing_after_its_repository_left_never_publishes() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let out = forge.take_due(later).remove(0);
    assert_eq!(
        forge.retain(|_| false),
        vec![repo()],
        "its refresh is stopped"
    );
    assert!(!forge.land(answer(&out, &[("feat", Some(9))]), later));
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], later);
    assert_eq!(
        shown_iid(&forge, "/work/app"),
        Some(5),
        "the last known, not the late one"
    );
    assert_eq!(forge.take_due(later).len(), 1);
}

/// Off forgets a hidden repository's MRs too.
#[test]
fn off_forgets_a_hidden_repository() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    forge.retain(|_| false);
    forge.set_enabled(false, now);
    forge.set_enabled(true, now);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    assert_eq!(shown_iid(&forge, "/work/app"), None);
    assert_eq!(forge.take_due(now).len(), 1);
}

// --- each branch on its own, and the repository's state

/// One branch's failure leaves the other branch of the same project
/// answered, and keeps its own last known MR with the failure beside it.
#[test]
fn one_branch_failing_leaves_the_others_answered() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    let both = vec![candidate("/work/a", "a"), candidate("/work/b", "b")];
    forge.inputs(&repo(), both, now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("a", Some(1)), ("b", Some(2))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    let mut partial = answer(&ask, &[("a", Some(3))]);
    if let Ok(branches) = &mut partial.batches[0].result {
        branches.push(("b".into(), Err(Error::Changed)));
    }
    forge.land(partial, later);
    match forge.shown(Path::new("/work/a")) {
        Shown::Mr { mr, failure, .. } => {
            assert_eq!(mr.iid, 3, "a's answer is not lost to b's failure");
            assert_eq!(failure, None, "b's failure is not a's");
        }
        other => panic!("{other:?}"),
    }
    match forge.shown(Path::new("/work/b")) {
        Shown::Mr { mr, failure, .. } => {
            assert_eq!(mr.iid, 2, "b keeps its last known MR");
            assert_eq!(failure.map(|(why, _)| why), Some(&Error::Changed));
        }
        other => panic!("{other:?}"),
    }
    assert!(forge.status(&repo()).unwrap().problems.is_empty());
}

/// A batch that failed as a whole — the deadline spent before it started,
/// glab's own proxy — is the repository's problem: the rows keep their last
/// known MRs with no failure of their own, and the next answer that holds
/// clears it.
#[test]
fn a_batch_failing_as_a_whole_is_the_repository_s() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    assert_eq!(forge.status(&repo()).unwrap().progress, Progress::Checking);
    let mut failed = answer(&ask, &[]);
    failed.batches[0].result = Err(Error::TimedOut);
    forge.land(failed, later);
    match forge.shown(Path::new("/work/app")) {
        Shown::Mr { mr, failure, .. } => {
            assert_eq!(mr.iid, 5);
            assert_eq!(failure, None, "not the row's");
        }
        other => panic!("{other:?}"),
    }
    let status = forge.status(&repo()).unwrap();
    assert_eq!(status.progress, Progress::Idle);
    assert_eq!(
        status.problems,
        &[Problem::Batch {
            host: HOST.into(),
            project: PROJECT.into(),
            error: Error::TimedOut,
        }]
    );
    let again = later + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, again);
    let ask = forge.take_due(again).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), again);
    assert!(forge.status(&repo()).unwrap().problems.is_empty());
}

/// A host found signed out — glab's config, or a 401 — keeps the rows'
/// last known MRs; the repository's state says why.
#[test]
fn signed_out_keeps_the_badges() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    let mut out = answer(&ask, &[]);
    out.resolutions[0].1 = Resolution::Why(Why::NotLoggedIn(HOST.into()));
    out.batches.clear();
    forge.land(out, later);
    assert_eq!(shown_iid(&forge, "/work/app"), Some(5));
    assert_eq!(
        forge.status(&repo()).unwrap().problems,
        &[Problem::Why(Why::NotLoggedIn(HOST.into()))]
    );
}

/// A refresh due while another is out is queued; the problems' words are
/// said in both languages.
#[test]
fn the_status_says_queued_and_its_problems_in_words() {
    use guild_desktop::i18n::Ui;
    use guild_session::Language;
    let now = Instant::now();
    let mut forge = Forge::new(true);
    assert_eq!(forge.status(&repo()), None, "never taken in");
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    assert_eq!(forge.status(&repo()).unwrap().progress, Progress::Queued);
    let ask = forge.take_due(now).remove(0);
    forge.wrote(&repo(), now);
    assert_eq!(forge.status(&repo()).unwrap().progress, Progress::Checking);
    forge.land(answer(&ask, &[("feat", None)]), now);
    assert_eq!(forge.status(&repo()).unwrap().progress, Progress::Queued);
    forge.retain(|_| false);
    assert_eq!(forge.status(&repo()), None, "hidden");
    let batch = Problem::Batch {
        host: HOST.into(),
        project: PROJECT.into(),
        error: Error::TimedOut,
    };
    assert_eq!(
        Ui(Language::English).mr_problem(&batch),
        "team/app: merge requests not read: glab did not answer in time"
    );
    assert_eq!(
        Ui(Language::Russian).mr_problem(&batch),
        "team/app: merge requests не прочитаны: glab не ответил вовремя"
    );
    assert_eq!(
        Ui(Language::English).mr_problem(&Problem::Why(Why::NotFound)),
        "glab not found"
    );
}

// --- a frame only for a visible change

/// `land` says the rows may have changed only when what they or the
/// repository's state draw did: the same MRs again ask for no frame.
#[test]
fn an_answer_that_changes_nothing_drawn_asks_for_no_frame() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let mut at = now;
    let mut next = |forge: &mut Forge| {
        at += Duration::from_secs(200);
        forge.trigger(None, Trigger::Tick, at);
        (forge.take_due(at).remove(0), at)
    };
    let ask = forge.take_due(now).remove(0);
    assert!(
        forge.land(answer(&ask, &[("feat", Some(5))]), now),
        "a first MR"
    );
    let (ask, at1) = next(&mut forge);
    assert!(
        !forge.land(answer(&ask, &[("feat", Some(5))]), at1),
        "the same MR, read again"
    );
    let (ask, at2) = next(&mut forge);
    assert!(
        forge.land(answer(&ask, &[("feat", Some(6))]), at2),
        "another MR"
    );
    let (ask, at3) = next(&mut forge);
    assert!(forge.land(failing(&ask), at3), "a failure beside it");
    let (ask, at4) = next(&mut forge);
    assert!(
        !forge.land(failing(&ask), at4),
        "the same failure again: drawn the same, aged from the same read"
    );
    let (ask, at5) = next(&mut forge);
    assert!(
        forge.land(answer(&ask, &[("feat", Some(6))]), at5),
        "the failure gone"
    );
    let (ask, at6) = next(&mut forge);
    let mut whole = answer(&ask, &[]);
    whole.batches[0].result = Err(Error::TimedOut);
    assert!(forge.land(whole, at6), "a problem of the repository's");
    let (ask, at7) = next(&mut forge);
    let mut whole = answer(&ask, &[]);
    whole.batches[0].result = Err(Error::TimedOut);
    assert!(!forge.land(whole, at7), "the same problem again");
}

/// A branch with no MR read again with no MR asks for no frame either.
#[test]
fn no_mr_read_again_asks_for_no_frame() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", None)]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    assert!(!forge.land(answer(&ask, &[("feat", None)]), later));
}

/// GitLab's `updated_at` is kept but not drawn: a refresh that moves only
/// it asks for no frame, and the MR kept is the newer read.
#[test]
fn a_change_nothing_draws_asks_for_no_frame() {
    let now = Instant::now();
    let mut forge = Forge::new(true);
    forge.inputs(&repo(), vec![candidate("/work/app", "feat")], now);
    let ask = forge.take_due(now).remove(0);
    forge.land(answer(&ask, &[("feat", Some(5))]), now);
    let later = now + Duration::from_secs(200);
    forge.trigger(None, Trigger::Tick, later);
    let ask = forge.take_due(later).remove(0);
    let mut touched = answer(&ask, &[("feat", Some(5))]);
    if let Ok(branches) = &mut touched.batches[0].result
        && let Ok(Some(mr)) = &mut branches[0].1
    {
        mr.updated_at = Some(1_800_000_000);
    }
    assert!(!forge.land(touched, later));
    match forge.shown(Path::new("/work/app")) {
        Shown::Mr { mr, .. } => assert_eq!(mr.updated_at, Some(1_800_000_000)),
        other => panic!("{other:?}"),
    }
}
