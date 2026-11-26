use gpui::{AppContext, Focusable, TestAppContext, px, size};
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    theme::Theme,
};
use std::{path::Path, process::Command, time::Duration};

fn git(root: &Path, args: &[&str]) {
    let answer = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        answer.status.success(),
        "{}",
        String::from_utf8_lossy(&answer.stderr)
    );
}

#[test]
fn line_mapping_handles_replacements_boundaries_endings_and_budgets() {
    use guild_desktop::line_changes::{
        BYTE_LIMIT, Kind::*, LINE_LIMIT, MARK_LIMIT, Marker, between,
    };
    let m = |row, kind| Marker { row, kind };
    for (old, new, expected) in [
        ("a\nb\nc\n", "a\nB\nc\n", vec![m(1, Modified)]),
        ("a\n", "a\nx\ny\n", vec![m(1, Added), m(2, Added)]),
        ("a\nb\nc\n", "a\nc\n", vec![m(1, Deleted)]),
        ("x\na\n", "a\n", vec![m(0, Deleted)]),
        ("a\nx", "a\n", vec![m(1, Deleted)]),
        ("a\nx", "a", vec![m(0, Modified), m(1, Deleted)]),
        ("a\n", "", vec![m(0, Deleted)]),
        ("", "\n\n", vec![m(0, Added), m(1, Added)]),
        (
            "a\nb\n",
            "A\nB\nx\n",
            vec![m(0, Modified), m(1, Modified), m(2, Added)],
        ),
        ("имя\r\n\r\nβ\r\n", "имя\n\nβ\n", vec![]),
        ("имя\rβ", "имя\rБ", vec![m(0, Modified)]),
        ("a", "a\n", vec![m(0, Modified)]),
        ("a\n", "a", vec![m(0, Modified)]),
        ("a\n\n", "a\n", vec![m(1, Deleted)]),
        ("same", "same", vec![]),
    ] {
        assert_eq!(between(old, new), Some(expected), "{old:?} -> {new:?}");
    }
    assert!(between("", &"x".repeat(BYTE_LIMIT + 1)).is_none());
    assert!(between("", &"\n".repeat(LINE_LIMIT)).is_none());
    assert!(between("", &"x\n".repeat(MARK_LIMIT + 1)).is_none());
    assert_eq!(
        between("", &"x\n".repeat(MARK_LIMIT)).unwrap().len(),
        MARK_LIMIT
    );
    assert_eq!(between("same\n", "same\n"), Some(vec![]));
}

#[gpui::test]
fn project_file_paints_unsaved_head_change_in_measured_gutter_and_save_keeps_it(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let source = "fn demo() {\n    let value = 1;\n    value\n}\n";
    std::fs::write(dir.path().join("main.rs"), source).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "main.rs"]);
    git(dir.path(), &["commit", "-qm", "baseline"]);
    let theme = Theme::default();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let tab = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("main.rs"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme,
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        file = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    cx.simulate_resize(size(px(650.), px(320.)));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let file = file.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    let stripes = |window: &gpui::Window| {
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                quad.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                    && quad.bounds.size.width.0 <= 4. * window.scale_factor()
                    && quad.bounds.size.height.0 > 4. * window.scale_factor()
            })
            .collect::<Vec<_>>()
    };
    cx.update(|window, _| assert!(stripes(window).is_empty(), "clean HEAD has no stripes"));
    let at = source.find('1').unwrap();
    cx.update(|_, cx| editor.update(cx, |e, cx| e.set_selected_range(at..at + 1, cx)));
    cx.simulate_input("2");
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        let painted = stripes(window);
        assert_eq!(
            painted.len(),
            1,
            "unsaved replacement must paint one Modified stripe"
        );
        let line = editor.read(cx).range_to_bounds(&(at..at)).unwrap();
        assert_eq!(
            painted[0].bounds.origin.y,
            line.origin.scale(window.scale_factor()).y
        );
        assert!(
            painted[0].bounds.right()
                <= editor
                    .read(cx)
                    .input_bounds()
                    .left()
                    .scale(window.scale_factor())
        );
        let center = painted[0].bounds.center();
        let at = gpui::point(
            px(center.x.0 / window.scale_factor()),
            px(center.y.0 / window.scale_factor()),
        );
        assert!(
            !file.read(cx).in_gutter(at),
            "stripes do not acquire Annotate popup hitboxes"
        );
        assert!(
            file.read(cx).in_gutter(gpui::point(at.x - px(8.), at.y)),
            "the adjacent number gutter keeps its target"
        );
        assert!(file.read(cx).is_dirty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("main.rs")).unwrap(),
            source
        );
        file.update(cx, |file, cx| file.save(cx)).unwrap();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(stripes(window).len(), 1, "save retains the HEAD difference");
        assert!(!file.read(cx).is_dirty());
    });
    git(dir.path(), &["add", "main.rs"]);
    git(dir.path(), &["commit", "-qm", "new HEAD"]);
    cx.update(|_, cx| file.update(cx, |file, cx| file.refresh_git_baseline(cx)));
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert!(stripes(window).is_empty(), "commit advances the baseline");
    });
    let disk = source.replace('1', "4");
    std::fs::write(dir.path().join("main.rs"), &disk).unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.disk_changed(window, cx)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(
            stripes(window).len(),
            1,
            "reload follows external text against HEAD"
        );
    });
    cx.update(|window, cx| {
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor.update(cx, |e, cx| e.set_selected_range(at..at + 1, cx));
    });
    cx.simulate_input("2");
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert!(
            stripes(window).is_empty(),
            "revert to HEAD clears marks even when buffer differs from disk"
        );
        assert!(file.read(cx).is_dirty());
    });
    cx.update(|_, cx| file.update(cx, |file, cx| file.save(cx)).unwrap());
    git(dir.path(), &["checkout", "-q", "--detach", "HEAD^"]);
    cx.update(|window, cx| file.update(cx, |file, cx| file.disk_changed(window, cx)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(editor.read(cx).value().as_ref(), source);
        assert!(
            stripes(window).is_empty(),
            "branch checkout reloads both disk and HEAD baseline"
        );
    });
}

#[gpui::test]
fn added_deleted_markers_and_excluded_sources_use_actual_file_views(cx: &mut TestAppContext) {
    use guild_desktop::line_changes::Kind;
    for case in [
        "untracked",
        "unborn",
        "deleted",
        "empty",
        "readonly",
        "nongit",
        "binary_base",
        "corrupt_head",
        "ignored",
        "large_base",
        "large_current",
        "dense_current",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        if case != "nongit" {
            git(dir.path(), &["init", "-q"]);
        }
        if !matches!(case, "unborn" | "nongit") {
            let baseline = if case == "large_base" {
                "a".repeat(guild_desktop::line_changes::BYTE_LIMIT + 1)
            } else if case == "binary_base" {
                "a\0b\n".into()
            } else {
                "first\nremoved\nlast\n".into()
            };
            std::fs::write(&path, baseline).unwrap();
            git(dir.path(), &["add", "file.txt"]);
            git(dir.path(), &["commit", "-qm", "base"]);
        }
        if case == "untracked" {
            std::fs::remove_file(&path).unwrap();
        }
        let relative = if case == "untracked" {
            "new.txt"
        } else {
            "file.txt"
        };
        let text = match case {
            "empty" => "",
            "deleted" => "first\nlast\n",
            _ => "new\nlines\n",
        };
        std::fs::write(dir.path().join(relative), text).unwrap();
        if case == "large_current" {
            std::fs::write(
                dir.path().join(relative),
                "x".repeat(guild_desktop::line_changes::BYTE_LIMIT + 1),
            )
            .unwrap();
        }
        if case == "dense_current" {
            std::fs::write(
                dir.path().join(relative),
                "new\n".repeat(guild_desktop::line_changes::MARK_LIMIT + 1),
            )
            .unwrap();
        }
        if case == "corrupt_head" {
            std::fs::write(dir.path().join(".git/HEAD"), "not-a-reference\n").unwrap();
        }
        if case == "ignored" {
            git(dir.path(), &["rm", "--cached", "file.txt"]);
            git(dir.path(), &["commit", "-qm", "remove tracked path"]);
            std::fs::write(dir.path().join(".gitignore"), "file.txt\n").unwrap();
        }
        let theme = Theme::default();
        let mut file = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            guild_desktop::editor::init(cx);
            let tab = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new(relative),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme,
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
                .read_only(case == "readonly")
            });
            file = Some(tab.clone());
            gpui_component::Root::new(tab, window, cx)
        });
        visual.simulate_resize(size(px(600.), px(300.)));
        visual.run_until_parked();
        for _ in 0..3 {
            visual.executor().advance_clock(Duration::from_millis(200));
            visual.run_until_parked();
        }
        visual.update(|window, cx| {
            window.simulate_next_frame(cx);
            let file = file.as_ref().unwrap().read(cx);
            let editor = file.editor().unwrap().read(cx);
            let marks: Vec<_> = window
                .painted_quads()
                .into_iter()
                .filter_map(|quad| {
                    if quad.bounds.size.width.0 > 6. * window.scale_factor() {
                        return None;
                    }
                    [
                        (Kind::Added, theme.accent.green),
                        (Kind::Modified, theme.accent.brass),
                        (Kind::Deleted, theme.accent.rust),
                    ]
                    .into_iter()
                    .find(|(_, color)| quad.background.as_solid() == Some(gpui::rgb(*color).into()))
                    .map(|(kind, _)| (kind, quad))
                })
                .collect();
            match case {
                "untracked" | "unborn" => assert_eq!(
                    marks.iter().map(|m| m.0).collect::<Vec<_>>(),
                    vec![Kind::Added, Kind::Added],
                    "{case}"
                ),
                "deleted" | "empty" => {
                    assert_eq!(marks.len(), 1, "{case}");
                    assert_eq!(marks[0].0, Kind::Deleted);
                    let offset = if case == "empty" { 0 } else { 6 };
                    let at = editor.range_to_bounds(&(offset..offset)).unwrap();
                    assert_eq!(
                        marks[0].1.bounds.origin.y,
                        at.top().scale(window.scale_factor()),
                        "{case}"
                    );
                }
                _ => assert!(marks.is_empty(), "excluded {case}: {marks:?}"),
            }
        });
        if matches!(case, "large_current" | "dense_current") {
            std::fs::write(dir.path().join(relative), "small\n").unwrap();
            visual.update(|window, cx| {
                file.as_ref()
                    .unwrap()
                    .update(cx, |file, cx| file.reload(window, cx))
            });
            visual.run_until_parked();
            for _ in 0..3 {
                visual.executor().advance_clock(Duration::from_millis(200));
                visual.run_until_parked();
            }
            visual.update(|window, cx| {
                window.simulate_next_frame(cx);
                assert!(
                    window
                        .painted_quads()
                        .iter()
                        .any(|q| q.background.as_solid()
                            == Some(gpui::rgb(theme.accent.brass).into())
                            && q.bounds.size.width.0 <= 4. * window.scale_factor()),
                    "small reload reenables complete markers after {case}"
                );
            });
        }
    }
}

#[gpui::test]
fn folded_body_markers_do_not_project_onto_the_next_visible_source_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let old = "fn first() {\n    let a = 1;\n    a\n}\n\nfn second() {\n    let b = 1;\n    b\n}\n";
    std::fs::write(dir.path().join("fold.rs"), old).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "fold.rs"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("fold.rs"), old.replace("= 1", "= 2")).unwrap();
    let theme = Theme::default();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("fold.rs"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme,
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
    cx.simulate_resize(size(px(650.), px(400.)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let file = made.unwrap();
    let count = |window: &gpui::Window| {
        window
            .painted_quads()
            .iter()
            .filter(|q| {
                q.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                    && q.bounds.size.width.0 <= 4. * window.scale_factor()
            })
            .count()
    };
    let point = cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(count(window), 2);
        let state = file.read(cx).editor().unwrap().read(cx);
        let at = state.range_to_bounds(&(0..0)).unwrap();
        gpui::point(
            (state.input_bounds().left() + at.left()) / 2.,
            at.center().y,
        )
    });
    cx.simulate_click(point, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(
            count(window),
            1,
            "hidden first-body stripe must disappear; second-body stripe stays"
        );
    });
}

#[gpui::test]
fn svg_source_uses_main_source_markers_but_image_preview_does_not(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let old = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>\n";
    std::fs::write(dir.path().join("image.svg"), old).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "image.svg"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("image.svg"), old.replace("red", "blue")).unwrap();
    let theme = Theme::default();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("image.svg"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme,
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
    cx.simulate_resize(size(px(650.), px(300.)));
    cx.run_until_parked();
    let stripes = |window: &gpui::Window| {
        window
            .painted_quads()
            .iter()
            .filter(|q| {
                q.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                    && q.bounds.size.width.0 <= 4. * window.scale_factor()
            })
            .count()
    };
    cx.update(|window, _| assert_eq!(stripes(window), 0));
    let source_control = cx.debug_bounds("image-source").unwrap().center();
    cx.simulate_click(source_control, Default::default());
    cx.run_until_parked();
    // Leave Source before its baseline debounce expires. Finishing that
    // unsupported request must not permanently suppress the next Source start.
    let preview_control = cx.debug_bounds("image-source").unwrap().center();
    cx.simulate_click(preview_control, Default::default());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert!(!made.as_ref().unwrap().read(cx).image_source_visible());
        assert_eq!(stripes(window), 0);
    });
    let source_control = cx.debug_bounds("image-source").unwrap().center();
    cx.simulate_click(source_control, Default::default());
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let file = made.unwrap();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert!(file.read(cx).image_source_visible());
        assert_eq!(stripes(window), 1);
    });
    let marker = first_marker_point(cx);
    cx.simulate_click(marker, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("git-hunk-popup").is_some());
    let preview_control = cx.debug_bounds("image-source").unwrap().center();
    cx.simulate_click(preview_control, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("git-hunk-popup").is_none());
    let source_control = cx.debug_bounds("image-source").unwrap().center();
    cx.simulate_click(source_control, Default::default());
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("git-hunk-popup").is_none(),
        "old popup cannot reappear on returning to Source"
    );
    std::fs::write(dir.path().join("image.svg"), old).unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert_eq!(
            stripes(window),
            0,
            "silent SVG source reload invalidates previous marks"
        );
    });
}

#[gpui::test]
fn marker_geometry_follows_wrap_partial_scroll_resize_and_theme(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let base = format!(
        "header\nold {}\ntail\n{}",
        "long λ text ".repeat(90),
        "context\n".repeat(100)
    );
    std::fs::write(dir.path().join("long.txt"), &base).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "long.txt"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let current = base.replacen("old", "new", 1);
    std::fs::write(dir.path().join("long.txt"), &current).unwrap();
    let theme = Theme::default();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("long.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme,
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
    cx.simulate_resize(size(px(650.), px(300.)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let file = made.unwrap();
    let editor = cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.toggle_soft_wrap(window, cx);
            file.editor().unwrap().clone()
        })
    });
    cx.run_until_parked();
    for (width, scroll) in [(650., 0.), (430., 0.), (430., -50.), (650., -70.)] {
        cx.simulate_resize(size(px(width), px(300.)));
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_scroll_offset(gpui::point(px(0.), px(scroll)), cx)
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            let state = editor.read(cx);
            let first = current.find("new").unwrap();
            let end = current.find("\ntail").unwrap();
            let full = state
                .range_to_bounds(&(first..end))
                .expect("wrapped source row remains laid out");
            let viewport = state.input_bounds();
            let painted: Vec<_> = window
                .painted_quads()
                .into_iter()
                .filter(|q| {
                    q.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                        && q.bounds.size.width.0 <= 4. * window.scale_factor()
                })
                .collect();
            assert_eq!(painted.len(), 1, "width={width} scroll={scroll}");
            assert_eq!(
                painted[0].bounds.top(),
                full.top().max(viewport.top()).scale(window.scale_factor())
            );
            assert_eq!(
                painted[0].bounds.bottom(),
                full.bottom()
                    .min(viewport.bottom())
                    .scale(window.scale_factor())
            );
        });
    }
    let light = Theme::of(guild_desktop::theme::Kind::Light);
    cx.update(|_, cx| {
        file.update(cx, |file, cx| {
            file.restyle(
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: light,
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        assert!(
            window
                .painted_quads()
                .iter()
                .any(
                    |q| q.background.as_solid() == Some(gpui::rgb(light.accent.brass).into())
                        && q.bounds.size.width.0 <= 4. * window.scale_factor()
                ),
            "restyle paints the actual new marker color"
        );
    });
}

#[gpui::test]
fn marker_click_opens_hunk_and_rolls_back_only_that_block_with_undo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let base = "fn first() { 1 }\n\nfn second() { 2 }\n";
    let current = base.replace("1", "10").replace("2", "20");
    std::fs::write(dir.path().join("main.rs"), base).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "main.rs"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let mut made = None;
    let theme = Theme::default();
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("main.rs"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme,
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
    cx.simulate_resize(size(px(750.), px(450.)));
    cx.run_until_parked();
    let file = made.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        editor.update(cx, |state, cx| {
            state.replace_all(current.clone(), window, cx)
        });
        editor
    });
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let hit = cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        let q = window
            .painted_quads()
            .into_iter()
            .find(|q| {
                q.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                    && q.bounds.size.width.0 <= 4. * window.scale_factor()
            })
            .unwrap();
        gpui::point(
            px(q.bounds.center().x.0 / window.scale_factor()),
            px(q.bounds.center().y.0 / window.scale_factor()),
        )
    });
    cx.simulate_click(hit, Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("git-hunk-popup").is_some(),
        "marker click must open the actual hunk popup"
    );
    // Read the actual readonly editor through its normal selection/Copy route.
    let code = cx.debug_bounds("git-hunk-source").unwrap();
    cx.simulate_click(
        gpui::point(code.left() + px(40.), code.top() + px(10.)),
        Default::default(),
    );
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "fn first() { 1 }\n"
        )
    });
    cx.simulate_input("must not edit preview or source");
    cx.update(|_, cx| assert_eq!(editor.read(cx).text().to_string(), current));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("git-hunk-popup").is_none(),
        "Escape from readonly code restores the source"
    );
    cx.update(|window, cx| assert!(editor.read(cx).focus_handle(cx).is_focused(window)));
    let second_hit = cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        let quad = window
            .painted_quads()
            .into_iter()
            .rev()
            .find(|q| {
                q.background.as_solid() == Some(gpui::rgb(theme.accent.brass).into())
                    && q.bounds.size.width.0 <= 4. * window.scale_factor()
            })
            .unwrap();
        gpui::point(
            px(quad.bounds.center().x.0 / window.scale_factor()),
            px(quad.bounds.center().y.0 / window.scale_factor()),
        )
    });
    cx.simulate_click(second_hit, Default::default());
    cx.run_until_parked();
    let code = cx.debug_bounds("git-hunk-source").unwrap();
    cx.simulate_click(
        gpui::point(code.left() + px(40.), code.top() + px(10.)),
        Default::default(),
    );
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "fn second() { 2 }\n",
            "later marker must not hit an earlier cell"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_click(hit, Default::default());
    cx.run_until_parked();
    let previous = cx.debug_bounds("git-hunk-previous").unwrap().center();
    cx.simulate_click(previous, Default::default());
    cx.run_until_parked();
    // Previous at BOF is disabled. Next reaches only this file's second block;
    // next at EOF is disabled, then Previous returns to the first block.
    for (selector, old_code, row) in [
        ("git-hunk-next", "fn second() { 2 }\n", 2),
        ("git-hunk-next", "fn second() { 2 }\n", 2),
        ("git-hunk-previous", "fn first() { 1 }\n", 0),
    ] {
        let hit = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(hit, Default::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("git-hunk-popup").is_some());
        cx.update(|_, cx| assert_eq!(editor.read(cx).cursor_position().line, row));
        let code = cx.debug_bounds("git-hunk-source").unwrap();
        cx.simulate_click(
            gpui::point(code.left() + px(40.), code.top() + px(10.)),
            Default::default(),
        );
        cx.simulate_keystrokes("cmd-a cmd-c");
        cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), old_code));
    }
    let rollback = cx.debug_bounds("git-hunk-rollback").unwrap().center();
    cx.simulate_click(rollback, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).text().to_string(), base.replace("2", "20"));
        assert!(file.read(cx).is_dirty());
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        base
    );
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).text().to_string(), current));
    cx.simulate_keystrokes("cmd-shift-z");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).text().to_string(), base.replace("2", "20")));
    // The pre-popup edit remains below the atomic rollback in the same history.
    cx.simulate_keystrokes("cmd-z cmd-z");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).text().to_string(), base));
    cx.simulate_keystrokes("cmd-shift-z cmd-shift-z");
    cx.run_until_parked();
    let index = Command::new("git")
        .current_dir(dir.path())
        .args(["show", ":main.rs"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(index.stdout).unwrap(), base);
    cx.update(|_, cx| file.update(cx, |file, cx| file.save(cx)).unwrap());
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        base.replace("2", "20")
    );
}

#[test]
fn hunk_ranges_restore_normalized_unicode_and_exact_line_boundaries() {
    use guild_desktop::line_changes::comparison;
    for (base, current) in [
        ("имя\r\nβ\r\n", "имя\nБ\n"),
        ("deleted\nkept\n", "kept\n"),
        ("kept\ndeleted", "kept\n"),
        ("entire\nfile\n", ""),
        ("", "added\n\n"),
        ("a\nb\nc\nkeep\nx\n", "A\nkeep\nX\n"),
        ("a", "a\n"),
        ("a\n", "a"),
        ("bare\rCR\n", "bare\rchanged\n"),
    ] {
        let snapshot = comparison(base, current).unwrap();
        let mut restored = snapshot.current.to_string();
        for block in snapshot.blocks.iter().rev() {
            assert!(snapshot.current.is_char_boundary(block.current.start));
            restored.replace_range(block.current.clone(), &snapshot.old[block.old.clone()]);
        }
        assert_eq!(restored, base.replace("\r\n", "\n"));
        for (marker, &block) in snapshot.markers.iter().zip(&snapshot.marker_blocks) {
            assert!(marker.row >= snapshot.blocks[block].rows.start);
            assert!(marker.row <= snapshot.blocks[block].rows.end);
        }
    }
}

#[gpui::test]
fn hunk_rollback_restores_deleted_edges_empty_added_and_crlf_unicode(cx: &mut TestAppContext) {
    for (base, current) in [
        ("удалено λ\r\nkeep\r\n", "keep\r\n"),
        ("keep\nlast😀", "keep\n"),
        ("entire\nfile\n", ""),
        ("", "added\n\n"),
        ("a\r\nβ\r\nc\r\nkeep\r\n", "A\r\nkeep\r\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, base).unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["add", "file.txt"]);
        git(dir.path(), &["commit", "-qm", "base"]);
        std::fs::write(&path, current).unwrap();
        let mut made = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            guild_desktop::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new("file.txt"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Theme::default(),
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
        visual.simulate_resize(size(px(650.), px(350.)));
        for _ in 0..3 {
            visual.executor().advance_clock(Duration::from_millis(200));
            visual.run_until_parked();
        }
        let hit = first_marker_point(visual);
        visual.simulate_click(hit, Default::default());
        visual.run_until_parked();
        assert!(visual.debug_bounds("git-hunk-popup").is_some());
        assert_eq!(
            visual.debug_bounds("git-hunk-source").is_some(),
            !base.is_empty()
        );
        let rollback = visual.debug_bounds("git-hunk-rollback").unwrap().center();
        visual.simulate_click(rollback, Default::default());
        visual.run_until_parked();
        visual.update(|_, cx| {
            let file = made.as_ref().unwrap().read(cx);
            assert_eq!(
                file.editor().unwrap().read(cx).text().to_string(),
                base.replace("\r\n", "\n")
            );
            assert!(file.is_dirty());
        });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), current);
        visual.simulate_keystrokes("cmd-z");
        visual.run_until_parked();
        visual.update(|_, cx| {
            assert_eq!(
                made.as_ref()
                    .unwrap()
                    .read(cx)
                    .editor()
                    .unwrap()
                    .read(cx)
                    .text()
                    .to_string(),
                current.replace("\r\n", "\n")
            )
        });
    }
}

fn click_before_frame(
    window: &mut gpui::Window,
    cx: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
) {
    window.dispatch_event(
        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: Default::default(),
            click_count: 1,
        }),
        cx,
    );
}

#[gpui::test]
fn old_marker_geometry_and_popup_callbacks_cannot_rollback_a_new_snapshot(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let base = format!("old\n{}", "unchanged\n".repeat(80));
    let current = base.replacen("old", "new", 1);
    std::fs::write(dir.path().join("file.txt"), &base).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "file.txt"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("file.txt"), &current).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("file.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
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
    cx.simulate_resize(size(px(700.), px(300.)));
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let file = made.unwrap();
    let editor = cx.update(|_, cx| file.read(cx).editor().unwrap().clone());
    let hit = first_marker_point(cx);
    cx.update(|window, cx| {
        let scroll = editor.read(cx).scroll_offset();
        window.dispatch_event(
            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position: editor.read(cx).input_bounds().center(),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-500.))),
                ..Default::default()
            }),
            cx,
        );
        assert_ne!(
            editor.read(cx).scroll_offset(),
            scroll,
            "wheel applied before old handler dispatch"
        );
        click_before_frame(window, cx, hit);
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("git-hunk-popup").is_none(),
        "previous frame marker cannot act after scroll"
    );
    cx.update(|_, cx| {
        editor.update(cx, |state, cx| {
            state.set_scroll_offset(gpui::point(px(0.), px(0.)), cx)
        })
    });
    cx.run_until_parked();
    for invalidation in ["text", "HEAD", "reload"] {
        for _ in 0..3 {
            cx.executor().advance_clock(Duration::from_millis(200));
            cx.run_until_parked();
        }
        let hit = first_marker_point(cx);
        cx.simulate_click(hit, Default::default());
        cx.run_until_parked();
        let rollback = cx.debug_bounds("git-hunk-rollback").unwrap().center();
        cx.update(|window, cx| {
            match invalidation {
                "text" => editor.update(cx, |state, cx| {
                    state.replace_all(format!("{current}extra\n"), window, cx)
                }),
                "HEAD" => file.update(cx, |file, cx| file.refresh_git_baseline(cx)),
                _ => file.update(cx, |file, cx| file.reload(window, cx)),
            }
            click_before_frame(window, cx, rollback);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("git-hunk-popup").is_none(),
            "{invalidation} closes stale popup"
        );
        cx.update(|_, cx| {
            assert!(
                editor.read(cx).text().to_string().starts_with("new\n"),
                "{invalidation} stale rollback must not install old text"
            )
        });
    }
}

fn first_marker_point(cx: &mut gpui::VisualTestContext) -> gpui::Point<gpui::Pixels> {
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        let theme = Theme::default();
        let quad = window
            .painted_quads()
            .into_iter()
            .find(|q| {
                [theme.accent.brass, theme.accent.green, theme.accent.rust]
                    .iter()
                    .any(|c| q.background.as_solid() == Some(gpui::rgb(*c).into()))
                    && q.bounds.size.width.0 <= 6. * window.scale_factor()
            })
            .expect("actual source marker quad");
        gpui::point(
            px(quad.bounds.center().x.0 / window.scale_factor()),
            px(quad.bounds.center().y.0 / window.scale_factor()),
        )
    })
}

#[gpui::test]
fn rollback_uses_raw_crlf_buffer_offsets_and_undo_preserves_exact_bytes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let base = "α\nb\n// stable\nδ\n";
    let raw = "α\r\nB\r\n// stable\r\nΔ\r\n";
    std::fs::write(dir.path().join("file.txt"), base).unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "file.txt"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("file.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
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
    cx.simulate_resize(size(px(700.), px(400.)));
    cx.run_until_parked();
    let file = made.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        editor.update(cx, |state, cx| state.replace_all(raw, window, cx));
        assert_eq!(
            editor.read(cx).text().to_string().as_bytes(),
            raw.as_bytes(),
            "the actual editable Rope retains raw CRLF before rollback"
        );
        editor
    });
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let hit = first_marker_point(cx);
    cx.simulate_click(hit, Default::default());
    cx.run_until_parked();
    let rollback = cx.debug_bounds("git-hunk-rollback").unwrap().center();
    cx.simulate_click(rollback, Default::default());
    cx.run_until_parked();
    let expected = "α\r\nb\n// stable\r\nΔ\r\n";
    cx.update(|_,cx|assert_eq!(editor.read(cx).text().to_string().as_bytes(),expected.as_bytes(),"only the selected raw current line is replaced; prefix/suffix and the other Unicode block retain their exact bytes"));
    assert_eq!(
        std::fs::read(dir.path().join("file.txt")).unwrap(),
        base.as_bytes()
    );
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).text().to_string().as_bytes(),
            raw.as_bytes(),
            "one Undo restores exact CRLF and Unicode bytes"
        )
    });
}
