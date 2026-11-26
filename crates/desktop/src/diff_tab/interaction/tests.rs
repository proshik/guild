use super::*;
use crate::diff;
use crate::{file_tab::Look, i18n::Ui, theme};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use gpui_component::input::EditorState;
use guild_session::{DiffView, Language};
use std::{cell::Cell, path::Path, sync::Arc};

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    old: &str,
    new: &str,
) -> (Entity<DiffTab>, &'a mut VisualTestContext) {
    cx.executor().allow_parking();
    let mut tab = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| {
            DiffTab::open_proposal(
                std::path::Path::new("/tmp"),
                "review.txt".into(),
                Some(old.as_bytes().to_vec()),
                new.as_bytes().to_vec(),
                DiffView::SideBySide,
                Look {
                    family: "Menlo".into(),
                    font_size: 12.5,
                    theme: theme::Theme::of(theme::Kind::Dark),
                    autosave: false,
                },
                Ui(Language::English),
                cx,
            )
        });
        let focus = entity.read(cx).focus.clone();
        focus.focus(window, cx);
        tab = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    let tab = tab.unwrap();
    settle(&tab, cx);
    (tab, cx)
}

fn settle(tab: &Entity<DiffTab>, cx: &mut VisualTestContext) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.computation.idle()
                    && t.files
                        .iter()
                        .all(|f| f.state != super::super::State::Loading)
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

#[gpui::test]
fn short_file_navigation_retains_the_requested_file(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\n", "b\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.target = None;
            t.files.push(super::super::FileDiff::compute(
                "second.txt".into(),
                Some(b"c\n"),
                Some(b"d\n"),
            ));
            t.old.insert("second.txt".into(), Ok(Some(b"c\n".to_vec())));
            t.working.insert("second.txt".into(), Some(b"d\n".to_vec()));
            t.rebuild();
            t.step_file(true, cx);
            assert_eq!(
                t.current_file(),
                1,
                "logical file must survive a viewport taller than its rows"
            );
            t.step_file(false, cx);
            assert_eq!(t.current_file(), 0);
        })
    });
}

#[gpui::test]
fn last_change_and_fold_counts_are_independent_of_clamped_scroll(cx: &mut TestAppContext) {
    let old = "start\na\ncontext\nb\nend\n";
    let new = "start\nA\ncontext\nB\nend\n";
    let (tab, cx) = fixture(cx, old, new);
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            assert_eq!(t.change_targets().len(), 2);
            t.step_change(true, cx);
            assert_eq!(t.current_change(), Some(0));
            t.step_change(true, cx);
            assert_eq!(t.current_change(), Some(1));
            assert!(!t.can_step_change(true));
            assert!(t.can_step_change(false));
            t.toggle_mode(cx);
            assert_eq!(t.current_change(), Some(1));
            t.step_change(false, cx);
            assert_eq!(t.current_change(), Some(0));
            assert!(!t.can_step_change(false));
        })
    });
}

#[gpui::test]
fn composed_character_boundaries_keep_emoji_and_accents_whole(_cx: &mut TestAppContext) {
    let text = "яе\u{301}👨‍👩‍👧‍👦🇫🇷\r\n";
    let b = crate::system::grapheme_boundaries(text);
    let parts = b.windows(2).map(|r| &text[r[0]..r[1]]).collect::<Vec<_>>();
    assert_eq!(parts, ["я", "е\u{301}", "👨‍👩‍👧‍👦", "🇫🇷", "\r\n"]);
}

#[gpui::test]
fn painted_long_lines_scroll_and_copy_the_same_source_side(cx: &mut TestAppContext) {
    use gpui::{MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
    const TAIL: &str = "ХВОСТ_Ж🧭";
    let prefix = format!("{} :: ", "длинная строка ".repeat(35));
    let expected_start = prefix.len();
    let expected_end = expected_start + TAIL.len();
    let text = format!("{prefix}{TAIL} ::\nвторая строка\n");
    let (tab, cx) = fixture(cx, "старый текст\n", &text);
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 1 && h.line == 0)
            .cloned()
            .expect("right source must be painted")
    });
    cx.simulate_event(ScrollWheelEvent {
        position: hit.bounds.center(),
        delta: ScrollDelta::Pixels(point(px(-5000.), px(0.))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        assert!(tab.read(cx).viewer.horizontal[1].0.borrow().offset > px(0.));
        assert_eq!(tab.read(cx).viewer.horizontal[0].0.borrow().offset, px(0.));
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 1 && h.line == 0)
            .cloned()
            .unwrap()
    });
    // Coordinates are the known literal's shaped glyph boundaries after the
    // actual scroll, not a range supplied by the viewer's hit-testing logic.
    let offset = cx.update(|_, cx| tab.read(cx).viewer.horizontal[1].0.borrow().offset);
    let start = point(
        hit.bounds.left() + hit.shaped.x_for_index(expected_start) - offset + px(0.05),
        hit.bounds.center().y,
    );
    let end = point(
        hit.bounds.left() + hit.shaped.x_for_index(expected_end) - offset + px(0.05),
        hit.bounds.center().y,
    );
    assert!(
        hit.bounds.contains(&start) && hit.bounds.contains(&end),
        "the complete unique tail must be visible"
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let sel = t
            .viewer
            .selection
            .as_ref()
            .expect("drag must select source");
        assert_eq!(sel.side, 1);
        assert!(!sel.range().is_empty());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("ХВОСТ_Ж🧭")
        );
        assert_eq!(sel.range(), expected_start..expected_end);
        assert_eq!(
            t.documents[&PathBuf::from("review.txt")].sources[1]
                .text
                .as_ref(),
            text
        );
    });
}

#[gpui::test]
fn copy_retains_crlf_and_hidden_context_without_mutating_sources(cx: &mut TestAppContext) {
    let text = format!("{}tail", "скрытая строка\r\n".repeat(30));
    let (tab, cx) = fixture(cx, &text, &text);
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.viewer.selection = Some(TextSelection {
                path: "review.txt".into(),
                side: 0,
                anchor: 0,
                head: text.len(),
            });
            t.copy_text(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(text.as_str())
            );
            let before = t.viewer.selection.as_ref().unwrap().range();
            t.expand_context(0, 0, cx);
            t.toggle_mode(cx);
            assert_eq!(t.viewer.selection.as_ref().unwrap().range(), before);
            assert_eq!(
                t.documents[&PathBuf::from("review.txt")].sources[0]
                    .text
                    .as_ref(),
                text
            );
            t.set_working("review.txt".into(), Some(b"replacement".to_vec()), cx);
            assert!(
                t.viewer.selection.is_none(),
                "source replacement invalidates copied ranges"
            );
        })
    });
}

#[gpui::test]
fn stale_blob_replies_cannot_replace_labels_or_source_bytes(cx: &mut TestAppContext) {
    use super::super::Which;
    let (tab, cx) = fixture(cx, "old", "new");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            let path = PathBuf::from("review.txt");
            t.base = guild_session::Base::Commit("selected".into());
            t.expected_fetch.borrow_mut().insert(path.clone(), 2);
            for (which, bytes, label) in [
                (Which::Old, b"before".to_vec(), "parent:old-name.txt"),
                (Which::New, b"after".to_vec(), "selected:review.txt"),
            ] {
                t.asks
                    .send((which, path.clone(), Ok(Some(bytes)), Some(label.into()), 2))
                    .unwrap();
            }
            t.poll(cx);
            t.asks
                .send((
                    Which::Old,
                    path.clone(),
                    Ok(Some(b"stale".to_vec())),
                    Some("stale:other.txt".into()),
                    1,
                ))
                .unwrap();
            t.poll(cx);
        })
    });
    settle(&tab, cx);
    cx.update(|_, cx| {
        tab.update(cx, |t, _| {
            let path = PathBuf::from("review.txt");
            assert_eq!(t.documents[&path].sources[0].text.as_ref(), "before");
            assert_eq!(t.documents[&path].sources[1].text.as_ref(), "after");
            assert_eq!(
                t.revision_labels(0),
                ["parent:old-name.txt", "selected:review.txt"]
            );
            assert!(!t.follows(&path));
        })
    });
}

#[gpui::test]
fn unified_old_selection_can_cross_unchanged_context(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\nold\ncontext\nend\n", "a\nnew\ncontext\nend\n");
    cx.update(|_, cx| tab.update(cx, |t, cx| t.toggle_mode(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let hit = t
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 1 && h.line == 2)
            .cloned()
            .unwrap();
        let at = t
            .hit_text(
                point(hit.bounds.left(), hit.bounds.center().y),
                Some((&PathBuf::from("review.txt"), 0)),
            )
            .unwrap();
        assert_eq!(
            at.2,
            "a\nold\n".len(),
            "unchanged unified row must map to the locked old source"
        );
    });
}

#[gpui::test]
fn divider_release_uses_final_position_and_narrow_panes_keep_text(cx: &mut TestAppContext) {
    let text = "content\n".repeat(50);
    let (tab, cx) = fixture(cx, "old\n", &text);
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let bounds = cx.update(|_, cx| tab.read(cx).viewer.viewport);
    let grip = point(
        bounds.left() + bounds.size.width * 0.5 + px(4.),
        bounds.top() + px(35.),
    );
    cx.simulate_mouse_down(grip, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(bounds.left() + px(300.), grip.y),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_up(
        point(bounds.right() + px(100.), grip.y),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert!(!t.viewer.resizing);
        assert!(t.viewer.ratio > 0.65, "outside release must apply final x");
    });
    cx.simulate_resize(gpui::size(px(320.), px(360.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert!(
            t.viewer
                .hits
                .borrow()
                .iter()
                .all(|h| h.bounds.size.width > px(20.))
        );
    });
}

#[gpui::test]
fn shift_wheel_and_unified_horizontal_position_have_their_own_extents(cx: &mut TestAppContext) {
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    let text = format!("{}\n", "контекст ".repeat(80));
    let (tab, cx) = fixture(cx, &text, &text.replace("контекст", "новое"));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 0)
            .cloned()
            .unwrap()
    });
    cx.simulate_event(ScrollWheelEvent {
        position: hit.bounds.center(),
        delta: ScrollDelta::Lines(point(0., -5.)),
        touch_phase: TouchPhase::Moved,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
    });
    cx.update(|_, cx| {
        assert!(tab.read(cx).viewer.horizontal[0].0.borrow().offset > px(0.));
        assert_eq!(tab.read(cx).viewer.horizontal[1].0.borrow().offset, px(0.));
        tab.update(cx, |t, cx| t.toggle_mode(cx));
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 1)
            .cloned()
            .unwrap()
    });
    cx.simulate_event(ScrollWheelEvent {
        position: hit.bounds.center(),
        delta: ScrollDelta::Pixels(point(px(-10000.), px(0.))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let h = t.viewer.horizontal[2].0.borrow();
        assert_eq!(h.offset, (h.width - h.bounds.size.width).max(px(0.)));
    });
}

#[gpui::test]
fn horizontal_scrollbar_drag_reaches_the_source_tail(cx: &mut TestAppContext) {
    let text = format!("{}\n", "длинный текст ".repeat(100));
    let (tab, cx) = fixture(cx, &text, &text.replace("текст", "строка"));
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let (bounds, bottom) = cx.update(|_, cx| {
        let t = tab.read(cx);
        (
            t.viewer.horizontal[1].0.borrow().bounds,
            t.viewer.viewport.bottom(),
        )
    });
    let start = point(bounds.left() + px(5.), bottom + px(4.));
    let end = point(bounds.right() - px(2.), start.y);
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let h = t.viewer.horizontal[1].0.borrow();
        assert!(
            h.offset >= (h.width - h.bounds.size.width) * 0.95,
            "scrollbar must reach the text tail: {:?} of {:?}",
            h.offset,
            h.width - h.bounds.size.width
        );
    });
}

#[gpui::test]
fn combined_rename_inspection_reads_real_blobs_and_ignores_working_text(cx: &mut TestAppContext) {
    combined_rename_inspection(cx, false);
}

#[gpui::test]
fn log_combined_diff_tab_reads_selected_blobs_and_ignores_working_text(cx: &mut TestAppContext) {
    combined_rename_inspection(cx, true);
}

fn combined_rename_inspection(cx: &mut TestAppContext, log_tab: bool) {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir.path())
            .args([
                "-c",
                "user.name=Diff Test",
                "-c",
                "user.email=diff@example.test",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    let before = (0..20).map(|i| format!("line {i}\r\n")).collect::<String>();
    std::fs::write(dir.path().join("old.txt"), &before).unwrap();
    git(&["add", "--", "old.txt"]);
    git(&["commit", "-qm", "base"]);
    let base = git(&["rev-parse", "HEAD"]);
    git(&["mv", "old.txt", "new.txt"]);
    let first = before.replace("line 0", "first 0");
    std::fs::write(dir.path().join("new.txt"), &first).unwrap();
    git(&["add", "--", "new.txt"]);
    git(&["commit", "-qm", "rename"]);
    let oldest = git(&["rev-parse", "HEAD"]);
    let after = first.replace("line 2", "last 2");
    std::fs::write(dir.path().join("new.txt"), &after).unwrap();
    git(&["add", "--", "new.txt"]);
    git(&["commit", "-qm", "last"]);
    let newest = git(&["rev-parse", "HEAD"]);
    let (_, cx) = fixture(cx, "x", "y");
    let tab = cx.update(|_, cx| {
        cx.new(|cx| {
            let key = format!("{newest},{oldest}");
            let look = Look {
                family: "monospace".into(),
                font_size: 12.5,
                theme: Default::default(),
                autosave: false,
            };
            if log_tab {
                DiffTab::open_file(
                    dir.path(),
                    "new.txt".into(),
                    guild_session::Base::Commit(key),
                    DiffView::SideBySide,
                    Some(b"uncommitted changes".to_vec()),
                    look,
                    Ui(Language::English),
                    cx,
                )
            } else {
                DiffTab::open_inspection(
                    dir.path(),
                    "new.txt".into(),
                    key,
                    DiffView::SideBySide,
                    look,
                    Ui(Language::English),
                    cx,
                )
            }
        })
    });
    for _ in 0..200 {
        let loaded = cx.update(|_, cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                t.documents.contains_key(&PathBuf::from("new.txt"))
            })
        });
        if loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let unchanged_document =
        cx.update(|_, cx| tab.read(cx).documents[Path::new("new.txt")].clone());
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            let path = PathBuf::from("new.txt");
            assert_eq!(
                t.revision_labels(0),
                [format!("{oldest}^:old.txt"), format!("{newest}:new.txt")]
            );
            assert_eq!(t.documents[&path].sources[0].text.as_ref(), before);
            assert_eq!(t.documents[&path].sources[1].text.as_ref(), after);
            t.set_working(
                path.clone(),
                Some(b"must not replace historical text".to_vec()),
                cx,
            );
            t.head_moved(cx);
        })
    });
    settle(&tab, cx);
    cx.update(|_, cx| {
        tab.update(cx, |t, _| {
            let path = PathBuf::from("new.txt");
            assert!(
                Arc::ptr_eq(&unchanged_document, &t.documents[&path]),
                "unchanged refreshed blobs recomputed their source mapping"
            );
            assert_eq!(t.documents[&path].sources[1].text.as_ref(), after);
        })
    });
    tab.update(cx, |t, cx| {
        t.toggle_authors(0, cx);
        t.toggle_authors(1, cx);
        t.ensure_authors_for(0, cx);
    });
    for _ in 0..200 {
        cx.run_until_parked();
        if cx.update(|_, cx| {
            tab.read(cx)
                .authors
                .entries
                .values()
                .all(|entry| matches!(entry.data, super::super::AuthorData::Ready(_)))
        }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let before = &t.authors.entries[&(PathBuf::from("new.txt"), 0)];
        let after = &t.authors.entries[&(PathBuf::from("new.txt"), 1)];
        assert_eq!(
            before.identity.as_ref().unwrap().path,
            PathBuf::from("old.txt")
        );
        assert_eq!(
            before.identity.as_ref().unwrap().revision,
            format!("{oldest}^")
        );
        assert_eq!(after.identity.as_ref().unwrap().revision, newest);
        let super::super::AuthorData::Ready(old_lines) = &before.data else {
            panic!("{:#?}", before.data)
        };
        let super::super::AuthorData::Ready(new_lines) = &after.data else {
            panic!("{:#?}", after.data)
        };
        assert_eq!(old_lines.len(), 20);
        assert!(
            old_lines
                .iter()
                .all(|line| line.as_ref().unwrap().sha == base)
        );
        assert_eq!(new_lines[0].as_ref().unwrap().sha, oldest);
        assert_eq!(new_lines[2].as_ref().unwrap().sha, newest);
        assert_eq!(new_lines[1].as_ref().unwrap().sha, base);
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("new.txt")).unwrap(),
        after
    );
    assert_eq!(git(&["status", "--porcelain"]), "");
}

#[gpui::test]
fn source_reveal_starts_at_the_change_instead_of_its_context(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\nb\nc\nd\nold\nf\n", "a\nb\nc\nd\nnew\nf\n");
    cx.update(|_, cx| assert_eq!(tab.read(cx).first_change_line(), 5));
}

#[gpui::test]
fn escape_cancels_the_divider_before_the_host_panel_can_close(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    use std::{cell::Cell, rc::Rc};
    struct Host {
        tab: Entity<DiffTab>,
        closed: Rc<Cell<bool>>,
    }
    impl gpui::Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
                .size_full()
                .key_context("panel")
                .on_action(
                    cx.listener(|this, _: &crate::actions::ClosePanel, _, _| this.closed.set(true)),
                )
                .child(self.tab.clone())
        }
    }
    let closed = Rc::new(Cell::new(false));
    let mut tab = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let t = cx.new(|cx| {
            DiffTab::open_proposal(
                std::path::Path::new("/tmp"),
                "review.txt".into(),
                Some(b"a\n".to_vec()),
                b"b\n".to_vec(),
                DiffView::SideBySide,
                Look {
                    family: "monospace".into(),
                    font_size: 12.5,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(Language::English),
                cx,
            )
        });
        let focus = t.read(cx).focus.clone();
        focus.focus(window, cx);
        tab = Some(t.clone());
        let host = cx.new(|_| Host {
            tab: t,
            closed: closed.clone(),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let tab = tab.unwrap();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.viewer.resizing = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(!tab.read(cx).viewer.resizing));
    assert!(
        !closed.get(),
        "Escape must cancel the owned gesture, not close its host"
    );
}

mod corrections;

mod native_feedback;

mod gutters;

#[gpui::test]
fn actual_editor_changes_are_scheduled_and_publish_only_matching_sources(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "before\r\nстрока\r\n", "initial\n");
    let editor = cx
        .update(|window, cx| cx.new(|cx| EditorState::new(window, cx).default_value("initial\n")));
    tab.update(cx, |t, cx| {
        t.follow(
            "review.txt".into(),
            &editor,
            |_| crate::highlight::LineEnding::Lf,
            cx,
        )
    });
    settle(&tab, cx);
    let before = diff::UI_COMPUTATIONS.with(Cell::get);
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_all("latest\nЮникод 🦀\n", window, cx)
        })
    });
    let after = diff::UI_COMPUTATIONS.with(Cell::get);
    assert_eq!(
        before, after,
        "InputEvent::Change ran a diff algorithm on GPUI"
    );
    settle(&tab, cx);
    cx.update(|_, cx| {
        let d = &tab.read(cx).documents[Path::new("review.txt")];
        assert_eq!(d.sources[1].text.as_ref(), "latest\nЮникод 🦀\n");
    });
    tab.update(cx, |t, cx| {
        t.unfollow(Path::new("review.txt"), Some(b"disk\r\n".to_vec()), cx)
    });
    settle(&tab, cx);
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx).documents[Path::new("review.txt")].sources[1]
                .text
                .as_ref(),
            "disk\r\n"
        )
    });
}

#[gpui::test]
fn scrolling_does_not_clone_offscreen_diff_payload(cx: &mut TestAppContext) {
    let old = (0..5000).map(|i| format!("old {i}\n")).collect::<String>();
    let new = (0..5000).map(|i| format!("new {i}\n")).collect::<String>();
    let (tab, cx) = fixture(cx, &old, &new);
    super::super::FILE_DIFF_CLONES.with(|n| n.set(0));
    for _ in 0..3 {
        let bounds = cx.debug_bounds("diff-body").unwrap();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: bounds.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-100.))),
            touch_phase: gpui::TouchPhase::Moved,
            ..Default::default()
        });
        cx.run_until_parked();
    }
    assert_eq!(
        super::super::FILE_DIFF_CLONES.with(|n| n.get()),
        0,
        "scroll-only rendering must retain offscreen hunk strings"
    );
    cx.update(|_, cx| {
        let tab = tab.read(cx);
        assert_eq!(tab.files()[0].counts, (5000, 5000));
        assert!(tab.top_row() > 0);
    });
}
