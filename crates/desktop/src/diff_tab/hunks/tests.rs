//! A diff's hunk actions over a real repository: the offers per shown
//! block, and the write a click makes. Private state is read here — the
//! offers are not a public surface.
use super::*;
use crate::file_tab::Look;
use crate::git::Settled;
use crate::i18n::Ui;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use guild_session::{DiffView, Language};
use std::cell::RefCell;
use std::rc::Rc;

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
    base: Base,
) -> (Entity<DiffTab>, &'a mut VisualTestContext) {
    cx.executor().allow_parking();
    let mut tab = None;
    let working = std::fs::read(root.join("f")).ok();
    let root = root.to_path_buf();
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| {
            DiffTab::open_file(
                &root,
                "f".into(),
                base,
                DiffView::SideBySide,
                working,
                Look {
                    family: "Menlo".into(),
                    font_size: 12.5,
                    theme: crate::theme::Theme::of(crate::theme::Kind::Dark),
                    autosave: false,
                },
                Ui(Language::English),
                cx,
            )
        });
        tab = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    (tab.unwrap(), cx)
}

/// Poll until the offers of block 0 are in.
fn offered(tab: &Entity<DiffTab>, cx: &mut VisualTestContext) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.hunk_offer(0, 0).is_some()
            })
        }) {
            return;
        }
        assert!(std::time::Instant::now() < until, "no offers");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn writes(tab: &Entity<DiffTab>, cx: &mut VisualTestContext) -> Rc<RefCell<Vec<Option<Write>>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(tab, move |_, event: &DiffEvent, _| match event {
            DiffEvent::Hunk(write) => sink.borrow_mut().push(Some(write.clone())),
            DiffEvent::HunkStale => sink.borrow_mut().push(None),
            _ => {}
        })
        .detach()
    });
    seen
}

#[gpui::test]
fn a_head_diff_offers_each_block_and_a_click_is_the_write(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\nc\nd\ne\nf\ng\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "A\nb\nc\nd\ne\nf\ng\n").unwrap();
    git(root, &["add", "f"]);
    std::fs::write(root.join("f"), "A\nb\nc\nd\ne\nF\ng\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Head);
    let seen = writes(&tab, cx);
    offered(&tab, cx);
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert_eq!(t.hunk_offer(0, 0), Some((Action::Unstage, Ok(()))));
        assert_eq!(t.hunk_offer(0, 1), Some((Action::Stage, Ok(()))));
    });
    tab.update(cx, |t, cx| {
        let token = t.hunk_token(0, 1).unwrap();
        t.stage_or_unstage(&token, cx)
    });
    cx.run_until_parked();
    let write = seen.borrow_mut().pop().flatten().expect("a write");
    let Settled::Ready(write) = git::settle(root, write) else {
        panic!("settles");
    };
    git::write(root, &write).unwrap();
    assert_eq!(
        git::blob(root, ":./f").unwrap().unwrap(),
        b"A\nb\nc\nd\ne\nF\ng\n"
    );
    // The working text moved on and the document has not caught up: a
    // click says so and writes nothing.
    tab.update(cx, |t, cx| {
        let token = t.hunk_token(0, 0).unwrap();
        t.working.insert("f".into(), Some(b"changed\n".to_vec()));
        t.stage_or_unstage(&token, cx);
    });
    cx.run_until_parked();
    assert_eq!(seen.borrow().last(), Some(&None));
}

#[gpui::test]
fn a_file_that_takes_no_hunks_says_why(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\n").unwrap();
    git(root, &["add", "f"]);
    let (tab, cx) = open(cx, root, Base::Unstaged);
    std::fs::write(root.join("f"), "a\nB\n").unwrap();
    tab.update(cx, |t, cx| {
        t.set_working("f".into(), Some(b"a\nB\n".to_vec()), cx)
    });
    offered(&tab, cx);
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx).hunk_offer(0, 0),
            Some((Action::Stage, Err(Text::HunkWholeFile))),
            "an added file stages whole"
        );
    });
}

/// Review r1 `adversarial-9`: a file reloaded from the disk with other line
/// endings keeps its editor, and the diff following it takes the buffer in
/// the endings the file has now — what a save would write.
#[gpui::test]
fn a_followed_buffer_takes_the_line_endings_after_a_reload(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\n").unwrap();
    git(&root, &["add", "f"]);
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let look = Look {
            family: "Menlo".into(),
            font_size: 12.5,
            theme: crate::theme::Theme::of(crate::theme::Kind::Dark),
            autosave: false,
        };
        let file = cx.new(|cx| {
            crate::file_tab::FileTab::open(
                &root,
                Path::new("f"),
                look.clone(),
                Ui(Language::English),
                window,
                cx,
            )
        });
        let diff = cx.new(|cx| {
            DiffTab::open_file(
                &root,
                "f".into(),
                Base::Unstaged,
                DiffView::SideBySide,
                Some(b"a\nb\n".to_vec()),
                look,
                Ui(Language::English),
                cx,
            )
        });
        made = Some((file.clone(), diff));
        gpui_component::Root::new(file, window, cx)
    });
    cx.run_until_parked();
    let (file, diff) = made.unwrap();
    let editor = cx.update(|_, cx| file.read(cx).editor().cloned().unwrap());
    cx.update(|_, cx| {
        let weak = file.downgrade();
        diff.update(cx, |d, cx| {
            d.follow(
                "f".into(),
                &editor,
                move |cx: &gpui::App| {
                    weak.upgrade()
                        .map_or(crate::highlight::LineEnding::Lf, |f| {
                            f.read(cx).line_ending()
                        })
                },
                cx,
            )
        })
    });
    std::fs::write(root.join("f"), "a\r\nb\r\n").unwrap();
    cx.update(|window, cx| file.update(cx, |f, cx| f.reload(window, cx)));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while cx.update(|_, cx| file.read(cx).line_ending()) != crate::highlight::LineEnding::Crlf {
        cx.run_until_parked();
        assert!(std::time::Instant::now() < until, "no reload");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update(|window, cx| editor.update(cx, |e, cx| e.replace_all("a\nB\n", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            diff.read(cx)
                .working
                .get(Path::new("f"))
                .cloned()
                .flatten()
                .as_deref(),
            Some(&b"a\r\nB\r\n"[..])
        )
    });
}

/// Review r1 `arch+quality-3`: an offers job still running when the index
/// moves — a `git add` in a terminal — must not publish offers computed from
/// the old index, though the document it was computed for is reused.
#[gpui::test]
fn offers_computed_from_a_superseded_index_are_not_published(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\nc\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "A\nb\nc\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Head);
    // Poll until an offers job has started, and in the same turn stage the
    // block from outside and refresh, before the job can run.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let raced = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                if t.hunks.pending.is_empty() {
                    return false;
                }
                git(root, &["add", "f"]);
                t.head_moved(cx);
                true
            })
        });
        if raced {
            break;
        }
        assert!(std::time::Instant::now() < until, "no job started");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // The refreshed texts land and the same document comes back while the
    // old job has still not run.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.poll(cx);
            t.documents.contains_key(Path::new("f"))
        })
    }) {
        assert!(std::time::Instant::now() < until, "no document back");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let offer = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.hunk_offer(0, 0)
            })
        });
        match offer {
            Some((Action::Unstage, Ok(()))) => break,
            Some((Action::Stage, _)) => panic!("an offer from the superseded index was shown"),
            _ => {}
        }
        assert!(std::time::Instant::now() < until, "no fresh offers");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Review r1 `arch+quality-9`: a file that leaves a tree diff takes its hunk
/// state with it.
#[gpui::test]
fn a_file_that_leaves_the_list_drops_its_hunk_state(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\n").unwrap();
    git(root, &["add", "f"]);
    let (tab, cx) = open(cx, root, Base::Unstaged);
    offered(&tab, cx);
    cx.update(|_, cx| {
        tab.update(cx, |t, _| {
            assert!(t.hunks.facts.contains_key(Path::new("f")));
            t.set_files(Vec::new());
            assert!(t.hunks.facts.is_empty());
            assert!(t.hunks.offers.is_empty());
            assert!(t.hunks.pending.is_empty());
        })
    });
}

/// Review r2 `bugs+impl-2`: a reload replaces the buffer without a Change
/// event. A click must not stage the text the reload discarded: the shown
/// bytes are checked against the buffer as it is now.
#[gpui::test]
fn a_click_after_a_silent_buffer_change_is_stale(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "a\nB\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Unstaged);
    let editor = cx.update(|window, cx| {
        cx.new(|cx| gpui_component::input::EditorState::new(window, cx).default_value("a\nB\n"))
    });
    tab.update(cx, |t, cx| {
        t.follow(
            "f".into(),
            &editor,
            |_| crate::highlight::LineEnding::Lf,
            cx,
        )
    });
    let seen = writes(&tab, cx);
    offered(&tab, cx);
    // `set_value` is what a reload does: no InputEvent::Change.
    cx.update(|window, cx| editor.update(cx, |e, cx| e.set_value("a\nreloaded\n", window, cx)));
    tab.update(cx, |t, cx| {
        let token = t.hunk_token(0, 0).unwrap();
        t.stage_or_unstage(&token, cx);
    });
    cx.run_until_parked();
    assert_eq!(seen.borrow().last(), Some(&None), "stale, nothing staged");
    // Following again — what the window does on the reload's BufferChanged —
    // takes the buffer as it is now.
    tab.update(cx, |t, cx| {
        t.follow(
            "f".into(),
            &editor,
            |_| crate::highlight::LineEnding::Lf,
            cx,
        )
    });
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx)
                .working
                .get(Path::new("f"))
                .cloned()
                .flatten()
                .as_deref(),
            Some(&b"a\nreloaded\n"[..])
        )
    });
}

/// Review r2 `bugs+impl-9`: a control acts on the offer it was drawn for.
/// Stage drawn before a `git add` from a terminal must not run as the
/// Unstage the refreshed diff offers.
#[gpui::test]
fn a_control_acts_only_on_the_offer_it_was_drawn_for(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\nc\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "A\nb\nc\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Head);
    let seen = writes(&tab, cx);
    offered(&tab, cx);
    let drawn = cx.update(|_, cx| tab.read(cx).hunk_token(0, 0).unwrap());
    git(root, &["add", "f"]);
    tab.update(cx, |t, cx| t.head_moved(cx));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let now = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.hunk_offer(0, 0)
            })
        });
        if now == Some((Action::Unstage, Ok(()))) {
            break;
        }
        assert!(std::time::Instant::now() < until, "no fresh offers");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    tab.update(cx, |t, cx| t.stage_or_unstage(&drawn, cx));
    cx.run_until_parked();
    assert_eq!(
        seen.borrow().last(),
        Some(&None),
        "the drawn Stage is stale"
    );
}

/// Review r2 `bugs+impl-10`: a facts batch superseded before it reads a
/// path spends no git read on it.
#[gpui::test]
fn a_superseded_facts_batch_reads_nothing_more(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\n").unwrap();
    git(root, &["add", "f"]);
    let (tab, cx) = open(cx, root, Base::Unstaged);
    offered(&tab, cx);
    let reads = cx.update(|_, cx| tab.read(cx).hunks.reads.clone());
    let before = reads.load(std::sync::atomic::Ordering::SeqCst);
    tab.update(cx, |t, _| {
        let gate = t.hunks.gate.clone();
        let held = gate.lock().unwrap();
        t.fetch_hunk_facts(&["f".into()], 1_000);
        t.fetch_hunk_facts(&["f".into()], 1_001);
        drop(held);
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while reads.load(std::sync::atomic::Ordering::SeqCst) < before + 1 {
        assert!(std::time::Instant::now() < until, "no read");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(
        reads.load(std::sync::atomic::Ordering::SeqCst),
        before + 1,
        "the superseded batch read nothing"
    );
}

/// Review r3 `bugs+impl-5`: with `core.autocrlf` on, the Staged diff's
/// Unstage would put `HEAD`'s CRLF bytes back where the index holds LF —
/// bytes git converts. The offer is disabled as converted, not refused
/// after the click.
#[gpui::test]
fn an_unstage_writing_bytes_git_converts_is_disabled(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "core.autocrlf", "false"]);
    git(root, &["config", "core.attributesFile", "/dev/null"]);
    std::fs::write(root.join("f"), "a\r\nb\r\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "a\nB\n").unwrap();
    git(root, &["add", "f"]);
    git(root, &["config", "core.autocrlf", "true"]);
    let (tab, cx) = open(cx, root, Base::Index);
    offered(&tab, cx);
    let offer = cx.update(|_, cx| tab.read(cx).hunk_offer(0, 0));
    assert_eq!(
        offer,
        Some((Action::Unstage, Err(Text::NoticeHunkConverted)))
    );
}

/// Review r3 `bugs+impl-6`: a Stage refused because the disk moved, with no
/// watcher delivering the edit (a tracked file under an ignored folder). The
/// refusal's refetch reads the working side again, so the same offer over
/// the old text is not enabled again.
#[gpui::test]
fn a_refetch_reads_the_working_side_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "a\nB\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Head);
    offered(&tab, cx);
    std::fs::write(root.join("f"), "a\nC\n").unwrap();
    tab.update(cx, |t, cx| t.refetch(Path::new("f"), cx));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let shown = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.working.get(Path::new("f")).cloned().flatten()
            })
        });
        if shown.as_deref() == Some(&b"a\nC\n"[..]) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "the working side stayed {shown:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Review r3 `arch+quality-5`: while a write for the file is unanswered, a
/// second activation is ignored — no write, no stale notice — before any
/// snapshot of the file is taken.
#[gpui::test]
fn an_activation_while_a_write_is_in_flight_is_ignored(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("f"), "a\nb\n").unwrap();
    git(root, &["add", "f"]);
    git(
        root,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(root.join("f"), "a\nB\n").unwrap();
    let (tab, cx) = open(cx, root, Base::Head);
    offered(&tab, cx);
    let seen = writes(&tab, cx);
    let file = root.join("f");
    cx.update(|_, cx| crate::root::hunks::mark_in_flight_for_test(cx, &file));
    tab.update(cx, |t, cx| {
        let token = t.hunk_token(0, 0).unwrap();
        t.stage_or_unstage(&token, cx);
        t.hunk_action_on_current(Action::Stage, cx);
    });
    cx.run_until_parked();
    assert!(seen.borrow().is_empty(), "ignored: {:?}", seen.borrow());
}

/// A HEAD diff of `f`, committed as `a\nb\n` and edited to `a\nB\n`.
fn head_diff_fixture(dir: &Path) {
    git(dir, &["init", "-q"]);
    std::fs::write(dir.join("f"), "a\nb\n").unwrap();
    git(dir, &["add", "f"]);
    git(
        dir,
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.invalid",
            "commit",
            "-qm",
            "x",
        ],
    );
    std::fs::write(dir.join("f"), "a\nB\n").unwrap();
}

/// Review r4 `adversarial-6`: the file was replaced by a sparse one far past
/// the diff's limit. The refusal's re-read takes at most one byte past the
/// limit — enough to show it as too large — and never the whole file.
#[gpui::test]
fn a_refetch_reads_no_more_than_one_byte_past_the_limit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    head_diff_fixture(root);
    let (tab, cx) = open(cx, root, Base::Head);
    offered(&tab, cx);
    let huge = std::fs::File::create(root.join("f")).unwrap();
    huge.set_len(1 << 34).unwrap();
    drop(huge);
    tab.update(cx, |t, cx| t.refetch(Path::new("f"), cx));
    let limit = crate::diff::DIFF_LIMIT as usize;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let read = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.working
                    .get(Path::new("f"))
                    .cloned()
                    .flatten()
                    .map(|w| w.len())
            })
        });
        if read.is_some_and(|len| len > 4) {
            assert_eq!(read, Some(limit + 1), "one byte past the limit");
            break;
        }
        assert!(std::time::Instant::now() < until, "never re-read");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Review r4 `bugs+impl-7`: a refusal's re-read is superseded by a newer
/// working text — here a watcher's — before it lands. The older read is
/// dropped, not put over the newer text.
#[gpui::test]
fn a_superseded_refetch_does_not_overwrite_newer_text(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    head_diff_fixture(root);
    let (tab, cx) = open(cx, root, Base::Head);
    offered(&tab, cx);
    std::fs::write(root.join("f"), "a\nOLD\n").unwrap();
    tab.update(cx, |t, cx| {
        t.refetch(Path::new("f"), cx);
        t.set_working("f".into(), Some(b"a\nNEWER\n".to_vec()), cx);
    });
    for _ in 0..50 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let shown = cx.update(|_, cx| tab.read(cx).working.get(Path::new("f")).cloned().flatten());
    assert_eq!(shown.as_deref(), Some(&b"a\nNEWER\n"[..]));
}

/// Review r5 `bugs+impl-5`: the working tree's diff drops a path while a
/// refusal's re-read of it is in flight. Its revision goes with its bytes,
/// so the late read finds nothing to match and puts nothing back.
#[gpui::test]
fn a_path_dropped_from_the_tree_takes_its_revision_with_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    head_diff_fixture(&root);
    let mut tab = None;
    let working = std::fs::read(root.join("f")).ok();
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| {
            DiffTab::open_tree(
                &root,
                vec![("f".into(), working)],
                DiffView::SideBySide,
                Look {
                    family: "Menlo".into(),
                    font_size: 12.5,
                    theme: crate::theme::Theme::of(crate::theme::Kind::Dark),
                    autosave: false,
                },
                Ui(Language::English),
                cx,
            )
        });
        tab = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let tab = tab.unwrap();
    tab.update(cx, |t, cx| {
        t.refetch(Path::new("f"), cx);
        t.set_files(Vec::new());
    });
    for _ in 0..50 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert!(!t.working.contains_key(Path::new("f")), "nothing put back");
        assert!(!t.working_revision.contains_key(Path::new("f")), "pruned");
    });
}
