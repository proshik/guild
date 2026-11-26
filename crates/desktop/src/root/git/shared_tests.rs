//! Production Root scheduling over real disposable linked repositories.
use super::*;
use gpui::{AppContext, Entity, TestAppContext};
use std::time::{Duration, Instant};

fn fixture() -> tempfile::TempDir {
    let folder = tests::scratch("shared-list");
    let repo = folder.path().join("repo");
    git::run(
        &repo,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    )
    .unwrap();
    folder
}
fn root(cx: &mut TestAppContext, paths: &[PathBuf]) -> Entity<Root> {
    cx.executor().allow_parking();
    cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        for path in paths {
            desk.open_in(desk.active_workspace(), path);
        }
        cx.new(|cx| {
            Root::from_state_with_policy(
                Default::default(),
                desk,
                crate::root::isolation::Policy::Diagnostic {
                    state_dir: paths[0].parent().unwrap().join("state"),
                },
                cx,
            )
        })
    })
}
fn settle(cx: &mut TestAppContext, root: &Entity<Root>, groups: usize) {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let ready = cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.poll_git(cx);
                r.worktrees.len() == groups
                    && r.status_workers
                        .keys()
                        .all(|id| r.git_status.contains_key(id))
                    && r.worktree_reads.idle()
            })
        });
        if ready {
            break;
        }
        assert!(Instant::now() < until, "Root did not publish Git answers");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn lists(folder: &Path) -> usize {
    git::ran_under(folder, &folder.canonicalize().unwrap())
        .iter()
        .filter(|a| a.starts_with(&["worktree".into(), "list".into()]))
        .count()
}
#[gpui::test]
fn linked_roots_share_one_production_read_per_wave(cx: &mut TestAppContext) {
    let fixture = fixture();
    let r = root(
        cx,
        &[fixture.path().join("repo"), fixture.path().join("linked")],
    );
    settle(cx, &r, 1);
    let before = lists(fixture.path());
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    settle(cx, &r, 1);
    assert_eq!(
        lists(fixture.path()) - before,
        1,
        "linked roots must share the actual list command"
    );
}

fn wait(cx: &mut TestAppContext, r: &Entity<Root>, ready: impl Fn(&Root) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if cx.update(|cx| {
            r.update(cx, |r, cx| {
                r.poll_git(cx);
                ready(r)
            })
        }) {
            return;
        }
        assert!(Instant::now() < deadline, "production Root did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[gpui::test]
fn unrelated_repositories_and_worktree_statuses_stay_independent(cx: &mut TestAppContext) {
    let a = fixture();
    let b = fixture();
    std::fs::write(a.path().join("linked/untracked"), "only linked").unwrap();
    let r = root(
        cx,
        &[
            a.path().join("repo"),
            a.path().join("linked"),
            b.path().join("repo"),
        ],
    );
    settle(cx, &r, 2);
    cx.update(|cx| {
        r.update(cx, |r, _| {
            for p in r.desk.projects() {
                let status = r.git_status[&p.id].as_ref().unwrap();
                assert_eq!(!status.is_empty(), p.path.ends_with("linked"));
            }
        })
    });
    let before = [lists(a.path()), lists(b.path())];
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    settle(cx, &r, 2);
    assert_eq!(
        [lists(a.path()) - before[0], lists(b.path()) - before[1]],
        [1, 1]
    );
}
#[gpui::test]
fn forced_refresh_during_real_read_rejects_old_snapshot_and_coalesces(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    git::run(
        &f.path().join("repo"),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "intermediate",
            "../intermediate",
        ],
    )
    .unwrap();
    let before = lists(f.path());
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    git::run(
        &f.path().join("repo"),
        &["worktree", "add", "-q", "-b", "new", "../new"],
    )
    .unwrap();
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            for _ in 0..100 {
                r.ask_worktrees();
                r.poll_git(cx);
            }
        })
    });
    let (latest_entered, latest_release) = git::worktrees::read_gate::hold(f.path());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.update(|cx| r.update(cx, |r, cx| r.poll_git(cx)));
        if latest_entered.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update(|cx| {
        r.update(cx, |r, _| {
            assert_eq!(
                r.worktrees.values().next().unwrap().len(),
                2,
                "obsolete 3-row snapshot has no publication authority"
            )
        })
    });
    latest_release.send(()).unwrap();
    wait(cx, &r, |r| {
        r.worktree_reads.idle() && r.worktrees.values().next().unwrap().len() == 4
    });
    assert_eq!(
        lists(f.path()) - before,
        2,
        "one active plus one latest follow-up"
    );
}
#[gpui::test]
fn errors_recover_and_last_subscriber_releases_shared_state(cx: &mut TestAppContext) {
    let f = fixture();
    let repo = f.path().join("repo");
    let r = root(cx, std::slice::from_ref(&repo));
    settle(cx, &r, 1);
    std::fs::rename(repo.join(".git"), repo.join(".hidden-git")).unwrap();
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    wait(cx, &r, |r| r.worktree_reads.idle());
    std::fs::rename(repo.join(".hidden-git"), repo.join(".git")).unwrap();
    let before = lists(f.path());
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    settle(cx, &r, 1);
    assert_eq!(lists(f.path()) - before, 1);
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            let ids: Vec<_> = r.desk.projects().map(|p| p.id).collect();
            for id in ids {
                r.desk.remove_project(id);
            }
            r.poll_git(cx);
            assert_eq!(r.worktree_reads.retained(), (0, 0));
            assert!(r.status_workers.is_empty());
        })
    });
}
#[gpui::test]
fn named_writes_refresh_shared_repository_after_completion(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    let made = f.path().join("made");
    cx.update(|cx| {
        r.update(cx, |r, _| {
            r.status_workers
                .values()
                .next()
                .unwrap()
                .run(Write::WorktreeAdd {
                    path: made.clone(),
                    branch: "made".into(),
                    base: None,
                })
        })
    });
    wait(cx, &r, |r| {
        r.worktree_reads.idle()
            && r.worktrees
                .values()
                .next()
                .unwrap()
                .iter()
                .any(|w| w.branch.as_deref() == Some("made"))
    });
    cx.update(|cx| {
        r.update(cx, |r, _| {
            r.status_workers
                .values()
                .next()
                .unwrap()
                .run(Write::WorktreeRemove(made.clone()))
        })
    });
    wait(cx, &r, |r| {
        r.worktree_reads.idle() && r.worktrees.values().next().unwrap().len() == 2
    });
    assert!(!made.exists());
}

#[gpui::test]
fn dirty_count_refresh_is_shared_without_reusing_stale_numbers(cx: &mut TestAppContext) {
    let f = fixture();
    let repo = f.path().join("repo");
    std::fs::write(repo.join("tracked"), "before\n").unwrap();
    git::run(&repo, &["add", "tracked"]).unwrap();
    git::run(
        &repo,
        &[
            "-c",
            "user.name=Guild",
            "-c",
            "user.email=guild@example.invalid",
            "commit",
            "-qm",
            "tracked",
        ],
    )
    .unwrap();
    git::run(&f.path().join("linked"), &["reset", "--keep", "HEAD"]).unwrap();
    let r = root(cx, &[repo.clone(), f.path().join("linked")]);
    settle(cx, &r, 1);
    let before = lists(f.path());
    std::fs::write(repo.join("tracked"), "after\nmore\n").unwrap();
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            let id = r.desk.projects().find(|p| p.path == repo).unwrap().id;
            r.ask_git(id, false, cx);
            r.poll_worktrees();
        })
    });
    wait(cx, &r, |r| {
        r.worktree_reads.idle()
            && r.worktrees
                .values()
                .next()
                .unwrap()
                .iter()
                .any(|w| w.lines == Some((2, 1)))
    });
    assert_eq!(lists(f.path()) - before, 1);
}
#[gpui::test]
fn representative_close_and_worker_replacement_do_not_strand_waiters(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    // Removing both representatives also revokes the outstanding read's publication.
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            let ids: Vec<_> = r.desk.projects().map(|p| p.id).collect();
            for id in ids {
                r.desk.remove_project(id);
            }
            r.poll_git(cx);
            assert_eq!(r.worktree_reads.retained(), (0, 0));
            assert!(r.worktrees.is_empty());
        })
    });
    release.send(()).unwrap();
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.desk
                .open_in(r.desk.active_workspace(), &f.path().join("linked"));
            r.poll_git(cx);
        })
    });
    settle(cx, &r, 1);
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            let id = r.desk.projects().next().unwrap().id;
            let path = r.desk.project(id).unwrap().path.clone();
            r.status_workers.insert(id, StatusWorker::start(path));
            r.poll_git(cx);
        })
    });
    settle(cx, &r, 1);
}
#[gpui::test]
fn unresolved_identity_retries_without_joining_another_repository(cx: &mut TestAppContext) {
    let f = fixture();
    let unknown = f.path().join("unknown");
    std::fs::create_dir(&unknown).unwrap();
    let r = root(cx, &[f.path().join("repo"), unknown.clone()]);
    settle(cx, &r, 1);
    cx.update(|cx| {
        r.update(cx, |r, _| {
            assert_eq!(r.worktree_reads.retained(), (2, 1));
        })
    });
    git::run(&unknown, &["init", "-q"]).unwrap();
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    settle(cx, &r, 2);
    cx.update(|cx| r.update(cx, |r, _| assert_eq!(r.worktree_reads.retained(), (2, 2))));
}

#[gpui::test]
fn closing_only_the_read_representative_keeps_the_other_subscriber_live(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    let representative = entered.recv_timeout(Duration::from_secs(10)).unwrap();
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            let id = r
                .desk
                .projects()
                .find(|p| p.path == representative)
                .unwrap()
                .id;
            r.desk.remove_project(id);
            r.poll_git(cx);
            r.ask_worktrees();
        })
    });
    release.send(()).unwrap();
    settle(cx, &r, 1);
    cx.update(|cx| {
        r.update(cx, |r, _| {
            assert_eq!(r.worktree_reads.retained(), (1, 1));
            assert_eq!(r.status_workers.len(), 1);
            assert_eq!(r.worktrees.values().next().unwrap().len(), 2);
        })
    });
}

#[gpui::test]
fn a_completed_write_revokes_an_already_finished_read_in_the_same_poll(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    git::run(
        &f.path().join("repo"),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "intermediate",
            "../intermediate",
        ],
    )
    .unwrap();
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    let representative = entered.recv_timeout(Duration::from_secs(10)).unwrap();
    let made = f.path().join("made");
    let write_fence = cx.update(|cx| {
        r.update(cx, |r, _| {
            let worker = r
                .status_workers
                .values()
                .find(|w| Path::new(&w.root) != representative)
                .unwrap();
            worker.run(Write::WorktreeAdd {
                path: made.clone(),
                branch: "made".into(),
                base: None,
            });
            let (tx, rx) = mpsc::channel();
            worker.asks.send(Ask::Common(tx)).unwrap();
            rx
        })
    });
    write_fence
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let read_fence = cx.update(|cx| {
        r.update(cx, |r, _| {
            let worker = r
                .status_workers
                .values()
                .find(|w| Path::new(&w.root) == representative)
                .unwrap();
            let (tx, rx) = mpsc::channel();
            worker.asks.send(Ask::Common(tx)).unwrap();
            rx
        })
    });
    release.send(()).unwrap();
    read_fence
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    // Both old read and write completion are waiting before Root's next turn.
    let (latest, release_latest) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| r.update(cx, |r, cx| r.poll_git(cx)));
    latest.recv_timeout(Duration::from_secs(10)).unwrap();
    cx.update(|cx| {
        r.update(cx, |r, _| {
            assert_eq!(r.worktrees.values().next().unwrap().len(), 2)
        })
    });
    release_latest.send(()).unwrap();
    wait(cx, &r, |r| {
        r.worktree_reads.idle() && r.worktrees.values().next().unwrap().len() == 4
    });
}

#[gpui::test]
fn a_write_does_not_recount_an_unrelated_repository(cx: &mut TestAppContext) {
    let a = fixture();
    let b = fixture();
    let r = root(
        cx,
        &[
            a.path().join("repo"),
            a.path().join("linked"),
            b.path().join("repo"),
        ],
    );
    settle(cx, &r, 2);
    let before = lists(b.path());
    let made = a.path().join("made");
    cx.update(|cx| {
        r.update(cx, |r, _| {
            r.status_workers
                .values()
                .find(|w| Path::new(&w.root).starts_with(a.path()))
                .unwrap()
                .run(Write::WorktreeAdd {
                    path: made.clone(),
                    branch: "made".into(),
                    base: None,
                })
        })
    });
    wait(cx, &r, |r| {
        r.worktree_reads.idle()
            && r.worktrees
                .values()
                .any(|list| list.iter().any(|w| w.branch.as_deref() == Some("made")))
    });
    assert_eq!(
        lists(b.path()),
        before,
        "an unrelated repository retains its own refresh epoch"
    );
}

#[gpui::test]
fn an_externally_removed_representative_cannot_strand_live_siblings(cx: &mut TestAppContext) {
    let f = fixture();
    git::run(
        &f.path().join("repo"),
        &["worktree", "add", "-q", "-b", "second", "../second"],
    )
    .unwrap();
    let r = root(cx, &[f.path().join("linked"), f.path().join("second")]);
    settle(cx, &r, 1);
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    let representative = entered.recv_timeout(Duration::from_secs(10)).unwrap();
    git::run(
        &f.path().join("repo"),
        &["worktree", "remove", representative.to_str().unwrap()],
    )
    .unwrap();
    cx.update(|cx| r.update(cx, |r, _| r.ask_worktrees()));
    release.send(()).unwrap();
    wait(cx, &r, |r| {
        r.worktree_reads.idle()
            && r.worktrees.values().next().unwrap().len() == 2
            && r.desk.projects().count() == 1
    });
}

#[gpui::test]
fn ordinary_cadence_publishes_current_read_before_held_followup(cx: &mut TestAppContext) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    git::run(
        &f.path().join("repo"),
        &["worktree", "add", "-q", "-b", "third", "../third"],
    )
    .unwrap();
    let before = lists(f.path());
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.poll_worktrees();
            r.poll_git(cx);
        })
    });
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    // No edit, write or forced refresh occurs while this current snapshot is held.
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            for _ in 0..5 {
                r.poll_worktrees();
                r.poll_git(cx);
            }
        })
    });
    let (followup, release_followup) = git::worktrees::read_gate::hold(f.path());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.update(|cx| r.update(cx, |r, cx| r.poll_git(cx)));
        if followup.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update(|cx| {
        r.update(cx, |r, _| {
            assert_eq!(
                r.worktrees.values().next().unwrap().len(),
                3,
                "current read must publish while the second real read remains held"
            )
        })
    });
    release_followup.send(()).unwrap();
    settle(cx, &r, 1);
    assert_eq!(
        lists(f.path()) - before,
        2,
        "one active and one coalesced followup"
    );
}

#[gpui::test]
fn exhausted_dead_worker_keeps_its_error_but_sibling_completes_full_refresh(
    cx: &mut TestAppContext,
) {
    let f = fixture();
    let r = root(cx, &[f.path().join("repo"), f.path().join("linked")]);
    settle(cx, &r, 1);
    let (entered, release) = git::worktrees::read_gate::hold(f.path());
    cx.update(|cx| {
        r.update(cx, |r, cx| {
            r.ask_worktrees();
            r.poll_git(cx);
        })
    });
    let representative = entered.recv_timeout(Duration::from_secs(10)).unwrap();
    release.send(()).unwrap();
    settle(cx, &r, 1);
    let failure = GitError::Failed("owned test worker channel failed".into());
    let (dead, serial) = cx.update(|cx| {
        r.update(cx, |r, cx| {
            let id = r
                .desk
                .projects()
                .find(|p| p.path == representative)
                .unwrap()
                .id;
            let worker = r.status_workers.get_mut(&id).unwrap();
            let serial = worker.serial;
            let (asks, closed) = mpsc::channel();
            drop(closed);
            worker.asks = asks;
            let (answer, answers) = mpsc::channel();
            answer.send(Answer::Map(Err(failure.clone()))).unwrap();
            drop(answer);
            worker.answers = answers;
            worker.restarts = 1; // Exhausted policy, same worker/serial/root; no replacement.
            r.poll_git(cx);
            assert!(r.status_workers[&id].dead);
            assert_eq!(r.status_workers[&id].serial, serial);
            (id, serial)
        })
    });
    for _ in 0..3 {
        let before = lists(f.path());
        let (receipt, received) = mpsc::channel();
        let path = f.path().join("repo").canonicalize().unwrap();
        cx.update(|cx| {
            r.update(cx, |r, _| {
                r.worktrees.clear();
                r.pending_settle.push((
                    crate::root::command::Settles::Listed(path),
                    receipt,
                    guild_session::command::Answer::Done,
                ));
                r.ask_worktrees();
            })
        });
        settle(cx, &r, 1);
        assert!(
            matches!(
                received.try_recv(),
                Ok(guild_session::command::Answer::Done)
            ),
            "full refresh must complete actual Listed settlement on unchanged data"
        );
        assert_eq!(
            lists(f.path()) - before,
            1,
            "only the healthy sibling executes Git; disconnected attempts must finish"
        );
        cx.update(|cx| {
            r.update(cx, |r, _| {
                assert!(r.status_workers[&dead].dead);
                assert_eq!(r.status_workers[&dead].serial, serial);
                assert_eq!(r.git_status[&dead].as_ref().err(), Some(&failure));
                assert!(
                    r.git_status
                        .iter()
                        .filter(|(id, _)| **id != dead)
                        .all(|(_, s)| s.is_ok())
                );
                assert_eq!(
                    r.worktrees.values().next().unwrap().len(),
                    2,
                    "unchanged raw rows still produce the requested full reply"
                );
            })
        });
    }
}
