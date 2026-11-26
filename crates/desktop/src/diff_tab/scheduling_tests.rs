use super::*;
use gpui::{AppContext, TestAppContext};

fn proposal(cx: &mut TestAppContext) -> Entity<DiffTab> {
    proposal_at(cx, "a.txt")
}

fn proposal_at(cx: &mut TestAppContext, path: &str) -> Entity<DiffTab> {
    cx.executor().allow_parking();
    cx.update(|cx| {
        cx.new(|cx| {
            DiffTab::open_proposal(
                Path::new("/tmp"),
                path.into(),
                Some(b"old\n".to_vec()),
                b"new\n".to_vec(),
                DiffView::SideBySide,
                Look {
                    family: "Menlo".into(),
                    font_size: 12.5,
                    theme: crate::theme::Theme::of(crate::theme::Kind::Dark),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                cx,
            )
        })
    })
}

#[gpui::test]
fn real_hunks_and_documents_never_execute_on_the_ui_thread(cx: &mut TestAppContext) {
    let before = diff::UI_COMPUTATIONS.with(Cell::get);
    let tab = proposal(cx);
    let after = diff::UI_COMPUTATIONS.with(Cell::get);
    assert_eq!(
        after, before,
        "constructor ran the real hunk/document algorithms on GPUI"
    );
    drop(tab);
}

#[gpui::test]
fn an_unchanged_file_retains_its_computed_hunks(cx: &mut TestAppContext) {
    let tab = proposal(cx);
    cx.update(|cx| {
        tab.update(cx, |t, _| {
            t.target = None;
            t.files.push(FileDiff::loading("b.txt".into()));
            t.old.insert("b.txt".into(), Ok(Some(b"one\n".to_vec())));
            t.working.insert("b.txt".into(), Some(b"two\n".to_vec()));
            t.rebuild();
        })
    });
    settle(&tab, cx);
    let before = cx.update(|cx| tab.read(cx).files[1].hunks.as_ptr() as usize);
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            t.set_working("a.txt".into(), Some(b"newer\n".to_vec()), cx)
        })
    });
    settle(&tab, cx);
    let after = cx.update(|cx| tab.read(cx).files[1].hunks.as_ptr() as usize);
    assert_eq!(after, before, "editing A recomputed B's real hunks");
}

#[gpui::test]
fn stale_real_syntax_reply_cannot_overwrite_a_current_pair(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let tab = proposal_at(cx, "a.rs");
    settle(&tab, cx);
    let (entered, release) = cx.update(|cx| tab.read(cx).computation.gate());
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            t.set_working("a.rs".into(), Some(b"obsolete\n".to_vec()), cx)
        })
    });
    entered
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            for i in 0..100 {
                t.set_working(
                    "a.rs".into(),
                    Some(format!("latest {i}\n").into_bytes()),
                    cx,
                );
            }
            assert_eq!(t.computation.counts(), (1, 1, 0));
            assert!(
                t.documents.is_empty(),
                "loading rows must not use old source bytes"
            );
        })
    });
    release.send(()).unwrap();
    settle(&tab, cx);
    let text = cx.update(|cx| {
        tab.read(cx).documents[Path::new("a.rs")].sources[1]
            .text
            .to_string()
    });
    assert_eq!(text, "latest 99\n");
    cx.update(|cx| {
        let paint = tab
            .read(cx)
            .paint(Path::new("a.rs"))
            .expect("real Rust parser produced a paint");
        assert!(paint.new.of_line(1, "latest 99").is_some());
        assert!(paint.new.of_line(1, "obsolete").is_none());
    });
}

#[gpui::test]
fn closing_an_active_owner_releases_its_worker_and_snapshots(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let tab = proposal(cx);
    settle(&tab, cx);
    let (entered, release) = cx.update(|cx| tab.read(cx).computation.gate());
    let weak = cx.update(|cx| tab.read(cx).computation.weak());
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            t.set_working("a.txt".into(), Some(b"closing\n".to_vec()), cx)
        })
    });
    entered
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    drop(tab);
    cx.run_until_parked();
    release.send(()).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while weak.upgrade().is_some() {
        cx.run_until_parked();
        cx.update(|_| {});
        assert!(
            std::time::Instant::now() < until,
            "worker/snapshots outlived owner shutdown"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub(super) fn settle(tab: &Entity<DiffTab>, cx: &mut TestAppContext) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.computation.idle()
            })
        }) {
            return;
        }
        assert!(
            std::time::Instant::now() < until,
            "diff worker never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn expired_algorithm_deadlines_still_cover_complete_sources() {
    let old = "alpha\nbeta\ngamma\n";
    let new = "delta\nepsilon\nzeta\n";
    let deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let hunks = diff::hunks_with_deadline(old, new, deadline);
    let rows = diff::rows(&hunks);
    let removed: String = rows
        .iter()
        .filter_map(|row| match row {
            diff::Row::Removed { text, .. } => Some(format!("{text}\n")),
            _ => None,
        })
        .collect();
    let added: String = rows
        .iter()
        .filter_map(|row| match row {
            diff::Row::Added { text, .. } => Some(format!("{text}\n")),
            _ => None,
        })
        .collect();
    assert_eq!((removed.as_str(), added.as_str()), (old, new));
    let doc = Document::with_deadline(old, new, deadline);
    assert_eq!(doc.blocks.iter().map(|b| b.old.len()).sum::<usize>(), 3);
    assert_eq!(doc.blocks.iter().map(|b| b.new.len()).sum::<usize>(), 3);
    assert_eq!(doc.sources[0].text.as_ref(), old);
    assert_eq!(doc.sources[1].text.as_ref(), new);
}

#[gpui::test]
fn asynchronous_results_keep_absent_binary_size_and_error_states(cx: &mut TestAppContext) {
    let tab = proposal(cx);
    settle(&tab, cx);
    let huge = vec![b'x'; DIFF_LIMIT as usize + 1];
    for (old, new, expected) in [
        (Ok(None), None, State::NoChanges),
        (
            Ok(Some(b"\0bin".to_vec())),
            Some(b"text".to_vec()),
            State::Binary,
        ),
        (Ok(Some(huge)), Some(b"text".to_vec()), State::TooLarge),
        (Err(GitError::NoRepository), None, State::NoRepository),
        (Err(GitError::NoGit), None, State::NoGit),
        (
            Err(GitError::Failed("refused".into())),
            None,
            State::Failed("refused".into()),
        ),
    ] {
        cx.update(|cx| {
            tab.update(cx, |t, _| {
                t.old.insert("a.txt".into(), old);
                t.working.insert("a.txt".into(), new);
                t.rebuild();
            })
        });
        settle(&tab, cx);
        cx.update(|cx| assert_eq!(tab.read(cx).files()[0].state, expected));
    }
}

#[gpui::test]
fn removing_a_path_revokes_its_already_calculated_result(cx: &mut TestAppContext) {
    let tab = proposal(cx);
    settle(&tab, cx);
    let (entered, release) = cx.update(|cx| tab.read(cx).computation.gate());
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            t.set_working("a.txt".into(), Some(b"removed\n".to_vec()), cx)
        })
    });
    entered
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    cx.update(|cx| tab.update(cx, |t, _| t.set_files(Vec::new())));
    release.send(()).unwrap();
    settle(&tab, cx);
    cx.update(|cx| {
        let t = tab.read(cx);
        assert!(t.files.is_empty() && t.documents.is_empty() && t.paints.is_empty());
        assert_eq!(t.computation.counts(), (0, 0, 0));
    });
}
