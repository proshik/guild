use super::*;
use gpui::{
    KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase,
    prelude::*,
};
use std::{cell::Cell, rc::Rc};

struct SafeHost {
    tab: Entity<DiffTab>,
    push_attempts: Rc<Cell<usize>>,
}
impl gpui::Render for SafeHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .size_full()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, _| {
                if event.keystroke.key == "enter" {
                    this.push_attempts.set(this.push_attempts.get() + 1);
                }
            }))
            .child(self.tab.clone())
    }
}

#[gpui::test]
fn preview_button_enter_activates_on_keyup_without_invoking_host_push(cx: &mut TestAppContext) {
    let pushed = Rc::new(Cell::new(0));
    let mut tab = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let t = cx.new(|cx| {
            DiffTab::open_proposal(
                std::path::Path::new("/tmp"),
                "review.txt".into(),
                Some(b"old\n".to_vec()),
                b"new\n".to_vec(),
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
        let host = cx.new(|_| SafeHost {
            tab: t,
            push_attempts: pushed.clone(),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let tab = tab.unwrap();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    // Next change, file picker, source, Side by side, Unified: ordinary traversal.
    cx.simulate_keystrokes("tab tab tab tab tab");
    let enter = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    assert_eq!(
        pushed.get(),
        0,
        "preview Enter must not invoke the Push host sentinel"
    );
    cx.update(|_, cx| assert_eq!(tab.read(cx).mode(), DiffView::SideBySide));
    cx.simulate_event(KeyUpEvent { keystroke: enter });
    cx.update(|_, cx| {
        assert_eq!(
            tab.read(cx).mode(),
            DiffView::Unified,
            "KeyUp must still activate Unified"
        )
    });
    assert_eq!(pushed.get(), 0);
    let counter = cx.debug_bounds("diff-file-counter").unwrap();
    cx.simulate_click(counter.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-file-picker").is_some());
    let enter = Keystroke::parse("enter").unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke: enter });
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-file-picker").is_none());
    assert_eq!(
        pushed.get(),
        0,
        "popup Enter must not invoke the Push host sentinel"
    );
}

struct SatelliteHost {
    tab: Entity<DiffTab>,
    focus: gpui::FocusHandle,
}
impl gpui::Render for SatelliteHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::view::satellite::frame(&self.focus, Default::default(), window, cx)
            .child(self.tab.clone())
    }
}

#[gpui::test]
fn escape_cancels_a_gesture_before_real_satellite_close(cx: &mut TestAppContext) {
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
        let host = cx.new(|cx| SatelliteHost {
            tab: t,
            focus: cx.focus_handle(),
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
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse("escape").unwrap(),
        is_held: false,
        prefer_character_input: false,
    });
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "gesture Escape must not close satellite"
    );
    cx.update(|_, cx| assert!(!tab.read(cx).viewer.resizing));
    cx.run_until_parked();
    let gutter = cx.debug_bounds("diff-numbers-old-1").unwrap();
    cx.simulate_mouse_down(
        gutter.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.simulate_mouse_up(
        gutter.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-authors-toggle").is_some());
    cx.simulate_keystrokes("escape");
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "authors-menu Escape must not close satellite"
    );
    cx.update(|window, cx| {
        assert!(tab.read(cx).authors_menu.is_none());
        assert!(tab.read(cx).focus.is_focused(window));
    });
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse("escape").unwrap(),
        is_held: false,
        prefer_character_input: false,
    });
    assert!(
        cx.cx.windows().is_empty(),
        "ordinary Escape must keep the existing satellite close behavior"
    );
}

#[gpui::test]
fn lone_cr_lines_can_be_selected_and_copied_without_invalid_rows(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\rb\n", "a\rc\n");
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.simulate_keystrokes("cmd-a");
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        let t = tab.read(cx);
        let doc = &t.documents[&PathBuf::from("review.txt")];
        assert_eq!(doc.sources[1].line(1), "c");
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("a\rc\n")
        );
    });
}

#[gpui::test]
fn listed_file_b_after_change_a_owns_copy_source_and_mode(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old A\n", "new A\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.target = None;
            t.files.push(super::super::super::FileDiff::compute(
                "B.txt".into(),
                Some(b"old B\n"),
                Some(b"unique B source\n"),
            ));
            t.old.insert("B.txt".into(), Ok(Some(b"old B\n".to_vec())));
            t.working
                .insert("B.txt".into(), Some(b"unique B source\n".to_vec()));
            t.rebuild();
            t.step_change(true, cx);
        })
    });
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            // Expose the list while retaining the earlier logical change owner.
            t.scroll.scroll_to_item_strict(0, gpui::ScrollStrategy::Top);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let listed = cx.debug_bounds("diff-listed-2").expect("actual B list row");
    cx.simulate_click(listed.center(), Default::default());
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("unique B source\n")
        );
        assert_eq!(tab.read(cx).current_file(), 1);
    });
    let opened = Rc::new(std::cell::RefCell::new(None));
    let captured = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&tab, move |_, event: &DiffEvent, _| {
            if let DiffEvent::OpenAt { path, line } = event {
                *captured.borrow_mut() = Some((path.clone(), *line));
            }
        })
        .detach()
    });
    cx.run_until_parked();
    let source = cx
        .debug_bounds("diff-edit")
        .expect("workspace source button must be visible");
    cx.simulate_click(source.center(), Default::default());
    assert_eq!(*opened.borrow(), Some((PathBuf::from("B.txt"), 1)));
    cx.update(|_, cx| tab.update(cx, |t, cx| t.toggle_mode(cx)));
    cx.update(|_, cx| assert_eq!(tab.read(cx).current_file(), 1));
}

#[gpui::test]
fn scheduled_edge_scroll_uses_the_pointer_returned_inside_before_next_frame(
    cx: &mut TestAppContext,
) {
    let text = "selectable text\n".repeat(100);
    let (tab, cx) = fixture(cx, "old\n", &text);
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    cx.run_until_parked();
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
    let inside = hit.bounds.center();
    let outside =
        cx.update(|_, cx| point(inside.x, tab.read(cx).viewer.viewport.bottom() + px(30.)));
    cx.simulate_mouse_down(inside, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(outside, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(inside, MouseButton::Left, Default::default());
    let offset = cx.update(|_, cx| tab.read(cx).scroll.0.borrow().base_handle.offset());
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert_eq!(t.viewer.pointer, Some(inside));
        assert!(
            t.viewer.selecting,
            "returning inside must not replay/cancel the selection"
        );
        assert_eq!(t.scroll.0.borrow().base_handle.offset(), offset);
    });
}

#[gpui::test]
fn mode_changes_preserve_a_manually_scrolled_source_after_large_replacement(
    cx: &mut TestAppContext,
) {
    let old = (0..230).map(|i| format!("old {i}\n")).collect::<String>();
    let new = (0..230)
        .map(|i| {
            if i < 100 || i == 115 {
                format!("new {i}\n")
            } else {
                format!("old {i}\n")
            }
        })
        .collect::<String>();
    let (tab, cx) = fixture(cx, &old, &new);
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            let count = t.documents[&PathBuf::from("review.txt")].equals.len();
            t.expanded.insert("review.txt".into(), (0..count).collect());
            t.project_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let (position, delta) = cx.update(|_, cx| {
        let t = tab.read(cx);
        let row = t
            .lines
            .iter()
            .position(|l| {
                matches!(
                    l,
                    Line::Source {
                        row: crate::diff::document::DisplayRow::Text { new: Some(115), .. },
                        ..
                    }
                )
            })
            .unwrap();
        (t.viewer.viewport.center(), row as f32 * t.row_px)
    });
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.), px(-delta))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            assert!(t.viewer.active_change.is_none());
            assert!(matches!(
                &t.lines[t.top_row()],
                Line::Source {
                    row: crate::diff::document::DisplayRow::Text { new: Some(115), .. },
                    ..
                }
            ));
            t.toggle_mode(cx);
        })
    });
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert!(
            matches!(
                &t.lines[t.top_row()],
                Line::Source {
                    row: crate::diff::document::DisplayRow::Text { new: Some(115), .. },
                    ..
                }
            ),
            "mode switch must reveal the same manually scrolled source line"
        );
    });
}

#[gpui::test]
fn selection_paint_ignores_a_stale_out_of_bounds_display_line(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\n", "b\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.select_all_text(cx);
            t.lines.push(Line::Source {
                file: 0,
                row: crate::diff::document::DisplayRow::Text {
                    old: None,
                    new: Some(99),
                    block: None,
                },
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("b\n")
        )
    });
}

#[gpui::test]
fn consecutive_cr_and_crlf_counts_agree_with_the_source_rows(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "a\r\r\nb", "a\r\nb");
    cx.update(|_, cx| {
        let t = tab.read(cx);
        assert_eq!(t.files()[0].counts, (0, 1));
        assert_eq!(t.files()[0].state, super::super::super::State::Hunks);
        assert_eq!(
            t.documents[&PathBuf::from("review.txt")].sources[0]
                .lines
                .len(),
            3
        );
    });
}

#[gpui::test]
fn double_click_copies_a_complete_combining_word(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old\n", "е\u{301}\n");
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.side == 1)
            .unwrap()
            .clone()
    });
    let position = gpui::point(hit.bounds.left() + px(1.), hit.bounds.center().y);
    cx.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position,
        click_count: 2,
        modifiers: Default::default(),
        first_mouse: false,
    });
    cx.simulate_mouse_up(position, MouseButton::Left, Default::default());
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("е\u{301}")
        )
    });
}

#[gpui::test]
fn changed_file_lists_preserve_path_and_clamp_removed_file(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old A\n", "new A\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.target = None;
            for path in ["B.txt", "C.txt"] {
                t.old.insert(path.into(), Ok(Some(b"old\n".to_vec())));
            }
            let files = |paths: &[&str]| {
                paths
                    .iter()
                    .map(|p| (PathBuf::from(p), Some(b"new\n".to_vec())))
                    .collect()
            };
            t.set_files(files(&["review.txt", "B.txt", "C.txt"]));
            t.step_file(true, cx);
            t.step_file(true, cx);
            assert_eq!(t.current_file(), 2);
            t.set_files(files(&["C.txt", "review.txt", "B.txt"]));
            assert_eq!(
                t.files[t.current_file()].path,
                PathBuf::from("C.txt"),
                "identity survives reorder"
            );
            t.working
                .insert("C.txt".into(), Some(b"updated source\n".to_vec()));
            t.rebuild();
            assert_eq!(
                t.files[t.current_file()].path,
                PathBuf::from("C.txt"),
                "identity survives a later source answer"
            );
            t.step_file(true, cx);
            t.step_file(true, cx);
            t.set_files(files(&["review.txt"]));
            assert_eq!(
                t.current_file(),
                0,
                "a nonempty list always has a valid current file"
            );
        })
    });
    settle(&tab, cx);
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.select_all_text(cx);
            t.copy_text(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("new\n")
            );
        })
    });
}

#[test]
fn long_word_selection_reaches_both_literal_boundaries() {
    let text = "x".repeat(1024 * 1024);
    assert_eq!(word_selection(&text, text.len() - 1), Some(0..1024 * 1024));
}

#[test]
fn word_selection_visits_each_cluster_once_for_long_unicode_words() {
    for count in [1024, 16384, 262144] {
        let text = "е\u{301}".repeat(count);
        let mut visits = 0;
        let mut bytes = 0;
        let range = word_selection_with(&text, text.len() - 4, |cluster| {
            visits += 1;
            bytes += cluster.len();
            cluster.chars().any(|ch| ch.is_alphanumeric() || ch == '_')
        });
        assert_eq!(range, Some(0..count * 4));
        assert_eq!(visits, count);
        assert_eq!(bytes, count * 4);
    }
    assert_eq!(word_selection("foo_бар e\u{301} 🧭", 11), Some(11..14));
    assert_eq!(word_selection("foo!", 3), None);
    assert_eq!(word_selection("", 0), None);
}

#[gpui::test]
fn selecting_visible_file_b_after_change_a_owns_copy_and_source_reveal(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old A\n", "new A\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.target = None;
            t.files.push(super::super::super::FileDiff::compute(
                "B.txt".into(),
                Some(b"old B\n"),
                Some(b"unique B source\n"),
            ));
            t.old.insert("B.txt".into(), Ok(Some(b"old B\n".to_vec())));
            t.working
                .insert("B.txt".into(), Some(b"unique B source\n".to_vec()));
            t.rebuild();
            t.step_change(true, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    let hit = cx.update(|_, cx| {
        tab.read(cx)
            .viewer
            .hits
            .borrow()
            .iter()
            .find(|h| h.file == 1 && h.side == 1)
            .cloned()
            .expect("both files must be visible")
    });
    cx.simulate_mouse_down(hit.bounds.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(hit.bounds.center(), MouseButton::Left, Default::default());
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("unique B source\n")
        );
        assert_eq!(tab.read(cx).current_file(), 1);
    });
    let opened = Rc::new(std::cell::RefCell::new(None));
    let captured = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&tab, move |_, event: &DiffEvent, _| {
            if let DiffEvent::OpenAt { path, line } = event {
                *captured.borrow_mut() = Some((path.clone(), *line));
            }
        })
        .detach()
    });
    cx.run_until_parked();
    let source = cx
        .debug_bounds("diff-edit")
        .expect("workspace source button must be visible");
    cx.simulate_click(source.center(), Default::default());
    assert_eq!(*opened.borrow(), Some((PathBuf::from("B.txt"), 1)));
}

#[gpui::test]
fn removed_active_file_uses_a_valid_fallback_for_copy(cx: &mut TestAppContext) {
    let (tab, cx) = fixture(cx, "old A\n", "new A\n");
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.target = None;
            for path in ["B.txt", "C.txt"] {
                t.old.insert(path.into(), Ok(Some(b"old\n".to_vec())));
            }
            t.set_files(
                ["review.txt", "B.txt", "C.txt"]
                    .map(|p| (p.into(), Some(b"new\n".to_vec())))
                    .to_vec(),
            );
            t.step_file(true, cx);
            t.step_file(true, cx);
            t.set_files(vec![(
                "review.txt".into(),
                Some(b"remaining literal\n".to_vec()),
            )]);
            assert_eq!(t.current_file(), 0);
        })
    });
    settle(&tab, cx);
    cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.select_all_text(cx);
            t.copy_text(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("remaining literal\n")
            );
        })
    });
}
