//! The schedule on an injected clock: no window, no git.

use super::*;

fn repo(name: &str) -> PathBuf {
    PathBuf::from(format!("/r/{name}/.git"))
}

/// A run in flight whose thread never answers, and its cancellation.
fn running() -> (Running, Arc<AtomicBool>, mpsc::Sender<Finished>) {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let console = ProjectId::new();
    (
        Running {
            cancelled: cancelled.clone(),
            waiting: Waiting::Thread {
                receiver,
                checkout: None,
            },
            console,
        },
        cancelled,
        sender,
    )
}

fn idle(_: &Path) -> bool {
    false
}

#[test]
fn a_repository_is_due_when_first_seen_and_five_minutes_after_it_lands() {
    let now = Instant::now();
    let a = repo("a");
    let mut schedule = Schedule::default();
    schedule.reconcile(std::slice::from_ref(&a), now);
    assert_eq!(
        schedule.starts(std::slice::from_ref(&a), idle, now),
        std::slice::from_ref(&a)
    );

    let (run, _, _keep) = running();
    schedule.started(&a, run);
    assert!(
        schedule
            .starts(std::slice::from_ref(&a), idle, now)
            .is_empty(),
        "one run per repository"
    );
    schedule.landed(&a, Some("main".into()), &Landed::UpToDate, now);
    let open = std::slice::from_ref(&a);
    assert!(
        schedule
            .starts(open, idle, now + EVERY - Duration::from_secs(1))
            .is_empty()
    );
    assert_eq!(
        schedule.starts(open, idle, now + EVERY),
        std::slice::from_ref(&a)
    );
}

#[test]
fn two_roots_of_one_repository_are_one_entry_and_the_cap_is_two() {
    let now = Instant::now();
    let open = [repo("a"), repo("a"), repo("b"), repo("c")];
    let mut schedule = Schedule::default();
    schedule.reconcile(&open, now);
    let distinct = [repo("a"), repo("b"), repo("c")];
    assert_eq!(
        schedule.starts(&distinct, idle, now),
        [repo("a"), repo("b")]
    );
    let (first, _, _k1) = running();
    let (second, _, _k2) = running();
    schedule.started(&repo("a"), first);
    schedule.started(&repo("b"), second);
    assert!(
        schedule.starts(&distinct, idle, now).is_empty(),
        "the cap holds"
    );
    schedule.landed(&repo("a"), None, &Landed::NoPlan, now);
    assert_eq!(schedule.starts(&distinct, idle, now), [repo("c")]);
}

#[test]
fn a_manual_update_in_flight_holds_the_run_until_it_is_done() {
    let now = Instant::now();
    let a = repo("a");
    let mut schedule = Schedule::default();
    schedule.reconcile(std::slice::from_ref(&a), now);
    assert!(
        schedule
            .starts(std::slice::from_ref(&a), |_| true, now)
            .is_empty()
    );
    assert_eq!(schedule.starts(std::slice::from_ref(&a), idle, now), [a]);
}

/// The review's case: closing the last root keeps the run's slot until it
/// lands, so reopening cannot start a second run, a manual update beside
/// it, or a run past the cap.
#[test]
fn a_run_closed_under_keeps_its_slot_until_it_lands() {
    let now = Instant::now();
    let a = repo("a");
    let b = repo("b");
    let c = repo("c");
    let mut schedule = Schedule::default();
    schedule.reconcile(&[a.clone(), b.clone()], now);
    let (run_a, cancelled, _keep) = running();
    let (run_b, _, _keep_b) = running();
    schedule.started(&a, run_a);
    schedule.started(&b, run_b);

    schedule.reconcile(std::slice::from_ref(&b), now);
    assert!(cancelled.load(Ordering::Acquire), "closed: cancelled");
    assert!(schedule.running(&a), "still in flight for the manual gate");
    assert_eq!(schedule.in_flight(), 2, "still counted in the cap");
    schedule.reconcile(&[a.clone(), b.clone(), c.clone()], now);
    assert!(
        schedule
            .starts(&[a.clone(), b.clone(), c.clone()], idle, now)
            .is_empty(),
        "reopened: no second run of a, and c waits for the cap"
    );

    schedule.landed(&a, Some("main".into()), &Landed::Cancelled, now);
    assert_eq!(
        schedule.starts(&[a.clone(), b.clone(), c.clone()], idle, now),
        std::slice::from_ref(&a),
        "the reopened repository runs once the cancelled run has landed"
    );
}

#[test]
fn a_repository_closed_and_landed_is_forgotten() {
    let now = Instant::now();
    let a = repo("a");
    let mut schedule = Schedule::default();
    schedule.reconcile(std::slice::from_ref(&a), now);
    let (run, _, _keep) = running();
    schedule.started(&a, run);
    schedule.reconcile(&[], now);
    schedule.landed(&a, None, &Landed::Cancelled, now);
    assert!(!schedule.running(&a));
    assert_eq!(schedule.in_flight(), 0);
    schedule.reconcile(std::slice::from_ref(&a), now);
    assert_eq!(schedule.starts(std::slice::from_ref(&a), idle, now), [a]);
}

#[test]
fn a_skip_is_kept_until_a_run_finds_the_branch_current() {
    let now = Instant::now();
    let a = repo("a");
    let mut schedule = Schedule::default();
    schedule.reconcile(std::slice::from_ref(&a), now);
    schedule.landed(
        &a,
        Some("main".into()),
        &Landed::Skipped(Reason::Dirty),
        now,
    );
    assert_eq!(schedule.skipped(&a), Some(&("main".into(), Reason::Dirty)));
    schedule.landed(&a, Some("main".into()), &Landed::Cancelled, now);
    assert_eq!(
        schedule.skipped(&a),
        Some(&("main".into(), Reason::Dirty)),
        "a cancelled run says nothing new"
    );
    schedule.landed(&a, Some("main".into()), &Landed::FastForwarded, now);
    assert_eq!(schedule.skipped(&a), None);
}

#[test]
fn turning_the_setting_off_cancels_and_on_makes_everything_due() {
    let now = Instant::now();
    let a = repo("a");
    let mut schedule = Schedule::default();
    schedule.reconcile(std::slice::from_ref(&a), now);
    let (run, cancelled, _keep) = running();
    schedule.started(&a, run);
    schedule.cancel_all();
    assert!(cancelled.load(Ordering::Acquire));
    schedule.landed(&a, None, &Landed::UpToDate, now);
    assert!(
        schedule
            .starts(std::slice::from_ref(&a), idle, now)
            .is_empty()
    );
    schedule.all_due(now);
    assert_eq!(schedule.starts(std::slice::from_ref(&a), idle, now), [a]);
}

#[test]
fn a_skip_is_in_the_source_list_everywhere_and_in_the_hint_only_on_the_branch() {
    let ui = crate::i18n::Ui(guild_session::Language::English);
    let skipped = ("main".to_owned(), Reason::Dirty);
    let on_main = Some("refs/heads/main");
    let on_feature = Some("refs/heads/feature");
    let line = skip_line(Some(&skipped), on_main, Surface::Hint, ui).unwrap();
    assert!(
        line.contains("main") && line.contains("uncommitted"),
        "{line}"
    );
    assert_eq!(
        skip_line(Some(&skipped), on_feature, Surface::Hint, ui),
        None
    );
    assert_eq!(skip_line(Some(&skipped), None, Surface::Hint, ui), None);
    assert!(skip_line(Some(&skipped), on_feature, Surface::Sources, ui).is_some());
    assert_eq!(skip_line(None, on_main, Surface::Sources, ui), None);
}
