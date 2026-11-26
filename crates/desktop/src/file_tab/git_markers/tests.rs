use super::*;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn source_owner_rejects_previous_baseline_path_and_text_generations(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "other\n").unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("a.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.run_until_parked();
    let file = made.unwrap();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            let stale = (file.git_markers.epoch, file.git_markers.serial);
            let marks = vec![Marker {
                row: 1,
                kind: line_changes::Kind::Modified,
            }];
            // A newer text request can reuse its matching baseline, but never the
            // previous text's computed marks.
            file.request_git_markers(cx);
            file.git_markers_landed(stale.0, stale.1, Ok("one\ntwo\n".into()), marks.clone(), cx);
            assert!(file.git_markers.marks.is_empty());
            assert!(matches!(file.git_markers.baseline, Some(Ok(_))));
            // A HEAD invalidation cannot reuse either part of the previous answer.
            file.refresh_git_baseline(cx);
            file.git_markers_landed(stale.0, stale.1, Ok("old HEAD".into()), marks.clone(), cx);
            assert!(file.git_markers.baseline.is_none());
            let previous_path = (file.git_markers.epoch, file.git_markers.serial);
            file.relocate(Path::new("b.txt"), window, cx);
            file.git_markers_landed(
                previous_path.0,
                previous_path.1,
                Ok("old path".into()),
                marks.clone(),
                cx,
            );
            assert!(file.git_markers.baseline.is_none());
            file.relocating(true, window, cx);
            let current = (file.git_markers.epoch, file.git_markers.serial);
            file.git_markers_landed(current.0, current.1, Ok("frozen".into()), marks.clone(), cx);
            assert!(file.git_markers.marks.is_empty());
            assert!(file.git_markers.baseline.is_none());
            file.relocating(false, window, cx);
            let editor = file.editor.clone().unwrap();
            editor.update(cx, |state, cx| {
                state.set_value("a".repeat(line_changes::BYTE_LIMIT + 1), window, cx)
            });
            file.request_git_markers(cx);
            file.git_markers_landed(
                file.git_markers.epoch,
                file.git_markers.serial,
                Ok("tiny".into()),
                marks,
                cx,
            );
            assert!(file.git_markers.marks.is_empty());
            assert!(file.git_marker_lane(window, cx).is_none());
        })
    });
}

#[gpui::test]
fn oversized_source_never_asks_git_and_readonly_cannot_attach_a_lane(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("large.txt"),
        "a".repeat(line_changes::BYTE_LIMIT + 1),
    )
    .unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("large.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    let file = made.unwrap();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            assert!(!file.git_markers.running);
            assert!(file.git_markers.baseline.is_none());
            assert!(file.git_marker_lane(window, cx).is_none());
            file.read_only = true;
            file.editor
                .clone()
                .unwrap()
                .update(cx, |e, cx| e.set_value("small", window, cx));
            file.refresh_git_baseline(cx);
            assert!(!file.git_markers.running);
            assert!(file.git_marker_lane(window, cx).is_none());
        })
    });
    assert!(crate::git::ran_under(dir.path(), &dir.path().canonicalize().unwrap()).is_empty());
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.read_only = false;
            file.editor
                .clone()
                .unwrap()
                .update(cx, |e, cx| e.set_value("small", window, cx));
            file.refresh_git_baseline(cx);
            assert!(file.git_markers.running);
        })
    });
    let weak = file.downgrade();
    drop(file);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "the queued job must not keep a closed source alive"
    );
    assert!(
        crate::git::ran_under(dir.path(), &dir.path().canonicalize().unwrap()).is_empty(),
        "closing cancels the queued baseline read"
    );
}

/// GLD-026: a block wholly in the index is drawn as an outline, and the
/// popup's Stage on a filled block asks for the write of exactly that block,
/// in the file's own line endings though the buffer holds `\n`.
#[gpui::test]
fn markers_outline_a_staged_block_and_the_popup_stages_the_other(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        )
    };
    let crlf = |s: &str| s.replace('\n', "\r\n");
    git(&["init", "-q"]);
    // CRLF kept as it is whatever the person's own git config says.
    git(&["config", "core.autocrlf", "false"]);
    git(&["config", "core.attributesFile", "/dev/null"]);
    std::fs::write(dir.path().join("a.txt"), crlf("a\nb\nc\nd\ne\nf\ng\n")).unwrap();
    git(&["add", "a.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "x",
    ]);
    std::fs::write(dir.path().join("a.txt"), crlf("A\nb\nc\nd\ne\nf\ng\n")).unwrap();
    git(&["add", "a.txt"]);
    std::fs::write(dir.path().join("a.txt"), crlf("A\nb\nc\nd\ne\nF\ng\n")).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("a.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        let ready = cx.update(|_, cx| {
            file.update(cx, |f, cx| {
                f.ensure_git_markers(cx);
                f.git_markers.offers.is_some()
            })
        });
        if ready {
            break;
        }
        assert!(std::time::Instant::now() < until, "no offers");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let asked = std::rc::Rc::new(std::cell::RefCell::new(None));
    let sink = asked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&file, move |_, event: &HunkRequest, _| {
            *sink.borrow_mut() = Some(event.0.clone())
        })
        .detach()
    });
    cx.update(|window, cx| {
        file.update(cx, |f, cx| {
            let offers = f.git_markers.offers.clone().unwrap();
            assert_eq!(
                &offers.staged[..],
                [true, false],
                "first outlined, second filled"
            );
            let comparison = f.git_markers.comparison.clone().unwrap();
            f.stage_git_hunk(&comparison, 1, window, cx);
        })
    });
    cx.run_until_parked();
    let Some(crate::git::Write::StageHunk(hunk)) = asked.borrow_mut().take() else {
        panic!("a Stage write");
    };
    assert_eq!(&hunk.preimage[..], crlf("A\nb\nc\nd\ne\nf\ng\n").as_bytes());
    assert_eq!(
        &hunk.postimage[..],
        crlf("A\nb\nc\nd\ne\nF\ng\n").as_bytes()
    );
    assert!(hunk.disk.is_none());
    assert_eq!(&hunk.head[..], crlf("a\nb\nc\nd\ne\nf\ng\n").as_bytes());
}

/// Review r4 `bugs+impl-8`: `HEAD` holds CRLF, the index and the buffer the
/// same edited LF text, and `core.autocrlf` is on. The block is wholly
/// staged, so it is drawn as an outline — though its Unstage is disabled,
/// because it would write `HEAD`'s CRLF back, which git converts.
#[gpui::test]
fn a_staged_block_is_outlined_though_its_unstage_is_disabled(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        )
    };
    git(&["init", "-q"]);
    git(&["config", "core.autocrlf", "false"]);
    git(&["config", "core.attributesFile", "/dev/null"]);
    std::fs::write(dir.path().join("a.txt"), "a\r\nb\r\nc\r\n").unwrap();
    git(&["add", "a.txt"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.invalid",
        "commit",
        "-qm",
        "x",
    ]);
    std::fs::write(dir.path().join("a.txt"), "a\nB\nc\n").unwrap();
    git(&["add", "a.txt"]);
    git(&["config", "core.autocrlf", "true"]);
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("a.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let offers = loop {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        let offers = cx.update(|_, cx| {
            file.update(cx, |f, cx| {
                f.ensure_git_markers(cx);
                f.git_markers.offers.clone()
            })
        });
        if let Some(offers) = offers {
            break offers;
        }
        assert!(std::time::Instant::now() < until, "no offers");
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(!offers.per_block.is_empty(), "a block");
    assert!(
        offers
            .per_block
            .iter()
            .all(|o| *o == crate::hunk::Offer::Converted),
        "Unstage disabled: {:?}",
        offers.per_block
    );
    assert!(
        offers.staged.iter().all(|s| *s) && !offers.staged.is_empty(),
        "outlined"
    );
}
