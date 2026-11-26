//! Exercise the real footer and popup through GPUI event dispatch,
//! without a native window, a Root, agents, or the person's settings.

use super::*;
use gpui::{Modifiers, TestAppContext};

struct Foot {
    dialog: Entity<PushDialog>,
    seen: Seen,
}

impl gpui::Render for Foot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.dialog.update(cx, |dialog, cx| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(dialog.tree(&self.seen, window, cx)),
                )
                .child(dialog.foot(&self.seen, window, cx))
                .children(dialog.popup(&self.seen, window, cx))
        })
    }
}

#[gpui::test]
fn tags_open_a_menu_and_clicking_it_does_not_close_it_through_the_backdrop(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let theme = theme::Theme::of(theme::Kind::Dark);
        Foot {
            dialog: cx
                .new(|cx| PushDialog::new(WeakEntity::new_invalid(), ProjectId::new(), theme, cx)),
            seen: Seen {
                ui: Ui(crate::i18n::Language::English),
                look: crate::file_tab::Look {
                    family: "monospace".into(),
                    font_size: 13.0,
                    theme,
                    autosave: false,
                },
                name: "fixture".to_owned(),
                ahead: Some(Ok(())),
                remotes: vec!["origin".to_owned()],
                to: Some(Destination {
                    branch: "main".to_owned(),
                    onto: "main".to_owned(),
                    remote: "origin".to_owned(),
                    set_upstream: false,
                }),
                commits: vec![("c1".to_owned(), "one".to_owned())],
                opened: Some("c1".to_owned()),
                selected: Vec::new(),
                detail: None,
                files: Some(Ok(Vec::new())),
                running: false,
                refused: None,
            },
        }
    });
    cx.simulate_resize(gpui::size(px(1000.0), px(620.0)));
    let click = |cx: &mut gpui::VisualTestContext, selector| {
        let bounds = cx
            .debug_bounds(selector)
            .expect("the actual control is drawn");
        cx.simulate_click(bounds.center(), Modifiers::default());
    };
    let dialog = cx.update(|_, cx| view.read(cx).dialog.clone());
    click(cx, "push-tags");
    cx.update(|_, cx| assert!(dialog.read(cx).tags_on));
    click(cx, "push-tags-kind");
    cx.update(|_, cx| assert!(matches!(dialog.read(cx).popup, Some((Popup::Tags, _)))));
    click(cx, "push-popup-row-1");
    cx.update(|_, cx| {
        assert_eq!(dialog.read(cx).tags, PushTags::CurrentBranch);
        assert!(dialog.read(cx).popup.is_none());
    });
    // A press outside the card dismisses it and does not toggle the
    // hook checkbox behind the backdrop in that same gesture.
    click(cx, "push-tags-kind");
    click(cx, "push-hooks");
    cx.update(|_, cx| {
        assert!(dialog.read(cx).popup.is_none());
        assert!(dialog.read(cx).run_hooks);
    });
    click(cx, "push-hooks");
    cx.update(|_, cx| assert!(!dialog.read(cx).run_hooks));
    // Both footer translations fit and keep the chevron's full height.
    for language in [
        crate::i18n::Language::English,
        crate::i18n::Language::Russian,
    ] {
        view.update(cx, |view, cx| {
            view.seen.ui = Ui(language);
            cx.notify();
        });
        let arrow = cx.debug_bounds("push-options").unwrap();
        assert_eq!(arrow.size.height, px(28.0));
        assert!(arrow.right() <= px(1000.0));
        cx.update(|window, _cx| {
            window.set_rem_size(crate::scale::rem_size(20.0));
            window.refresh();
        });
        let arrow = cx.debug_bounds("push-options").unwrap();
        assert!(
            arrow.right() <= px(1000.0),
            "large UI text keeps Push within the window"
        );
        assert!(cx.debug_bounds("push-footer-controls").unwrap().right() <= px(1000.0));
        cx.update(|window, _cx| {
            window.set_rem_size(crate::scale::rem_size(13.0));
            window.refresh();
        });
    }
}

#[gpui::test]
fn file_modifier_clicks_and_keyboard_keep_selection_and_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(crate::actions::ordered(
            vec![
                ("cmd-a".into(), crate::actions::keymap::Named::SelectAll),
                ("cmd-c".into(), crate::actions::keymap::Named::Copy),
            ],
            &[],
        ))
    });
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let theme = theme::Theme::of(theme::Kind::Dark);
        Foot {
            dialog: cx
                .new(|cx| PushDialog::new(WeakEntity::new_invalid(), ProjectId::new(), theme, cx)),
            seen: Seen {
                ui: Ui(crate::i18n::Language::English),
                look: crate::file_tab::Look {
                    family: "monospace".into(),
                    font_size: 13.0,
                    theme,
                    autosave: false,
                },
                name: "fixture".into(),
                ahead: Some(Ok(())),
                remotes: vec![],
                to: None,
                commits: vec![("c1".into(), "one".into())],
                opened: Some("c1".into()),
                selected: vec![],
                detail: None,
                files: Some(Ok(["a.rs", "b.rs", "c.rs"]
                    .map(|path| FileChange {
                        path: path.into(),
                        mark: theme::Mark::Modified,
                        from: None,
                    })
                    .to_vec())),
                running: false,
                refused: None,
            },
        }
    });
    cx.simulate_resize(gpui::size(px(1000.0), px(620.0)));
    let dialog = cx.update(|_, cx| view.read(cx).dialog.clone());
    let click = |cx: &mut gpui::VisualTestContext, index, mods| {
        let bounds = cx
            .debug_bounds(["push-file-0", "push-file-1", "push-file-2"][index])
            .expect("file row is drawn");
        cx.simulate_click(bounds.center(), mods);
    };
    click(cx, 0, Modifiers::default());
    click(
        cx,
        2,
        Modifiers {
            platform: true,
            ..Default::default()
        },
    );
    cx.update(|window, cx| {
        let dialog = dialog.read(cx);
        assert_eq!(dialog.file_selection.copied_paths(), "a.rs\nc.rs");
        assert!(dialog.file_focus.is_focused(window));
    });
    click(
        cx,
        0,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    cx.update(|_, cx| assert_eq!(dialog.read(cx).file_selection.selected.len(), 3));
    // Decision 54: Command-Down moves the selection, so the diff always
    // has a file.
    click(cx, 0, Modifiers::default());
    cx.simulate_keystrokes("cmd-down");
    cx.update(|_, cx| {
        let dialog = dialog.read(cx);
        assert_eq!(dialog.file_selection.copied_paths(), "b.rs");
        assert_eq!(
            dialog.selected_file.as_deref(),
            Some(std::path::Path::new("b.rs"))
        );
    });
    click(cx, 2, Modifiers::default());
    click(
        cx,
        0,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    cx.simulate_keystrokes("shift-down");
    cx.update(|_, cx| assert_eq!(dialog.read(cx).file_selection.copied_paths(), "b.rs\nc.rs"));
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| assert_eq!(dialog.read(cx).file_selection.selected.len(), 3));
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "a.rs\nb.rs\nc.rs"
        )
    });
    click(
        cx,
        1,
        Modifiers {
            platform: true,
            ..Default::default()
        },
    );
    cx.update(|_, cx| {
        assert!(
            dialog.read(cx).selected_file.is_none(),
            "a deselected cursor must not remain a diff target"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(dialog.read(cx).focus.is_focused(window)));
}

struct ResizeTrial(Entity<PushDialog>);
impl gpui::Render for ResizeTrial {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |dialog, cx| {
            let (width, _) = dialog
                .sides
                .sizes(f32::from(window.viewport_size().width) - 9., MIN_SIDE);
            let (height, _) = dialog
                .panes
                .sizes(f32::from(window.viewport_size().height) - 9., MIN_PANE);
            div()
                .size_full()
                .relative()
                .flex()
                .child(dialog.resize_events(cx))
                .child(div().w(px(width)).h_full())
                .child(dialog.grip(Grip::Sides, cx))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(div().h(px(height)).w_full())
                        .child(dialog.grip(Grip::Panes, cx))
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .occlude()
                                .on_mouse_move(|_, _, cx| cx.stop_propagation()),
                        ),
                )
        })
    }
}

#[gpui::test]
fn push_splits_follow_slow_moves_release_and_repeated_grabs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        ResizeTrial(cx.new(|cx| {
            PushDialog::new(
                WeakEntity::new_invalid(),
                ProjectId::new(),
                theme::Theme::default(),
                cx,
            )
        }))
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let dialog = cx.update(|_, cx| view.read(cx).0.clone());
    for (grip, selector) in [
        (Grip::Sides, "push-grip-sides"),
        (Grip::Panes, "push-grip-panes"),
    ] {
        for direction in [1., -1.] {
            let before = cx.update(|_, cx| match grip {
                Grip::Sides => dialog.read(cx).sides.fraction(),
                Grip::Panes => dialog.read(cx).panes.fraction(),
            });
            let start = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
            cx.run_until_parked();
            for step in 1..=40 {
                let at = start
                    + match grip {
                        Grip::Sides => gpui::point(px(step as f32 * direction), px(0.)),
                        Grip::Panes => gpui::point(px(0.), px(step as f32 * direction)),
                    };
                cx.simulate_mouse_move(at, MouseButton::Left, Default::default());
                cx.run_until_parked();
            }
            let after = cx.update(|_, cx| match grip {
                Grip::Sides => dialog.read(cx).sides.fraction(),
                Grip::Panes => dialog.read(cx).panes.fraction(),
            });
            assert!(
                (after - before) * direction >= 0.03,
                "slow {selector} drag must accumulate movement: {before} -> {after}"
            );
            let end = start
                + match grip {
                    Grip::Sides => gpui::point(px(60. * direction), px(0.)),
                    Grip::Panes => gpui::point(px(0.), px(60. * direction)),
                };
            cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                let dialog = dialog.read(cx);
                assert!(dialog.drag.is_none());
                let final_fraction = match grip {
                    Grip::Sides => dialog.sides.fraction(),
                    Grip::Panes => dialog.panes.fraction(),
                };
                assert!(
                    (final_fraction - after) * direction > 0.01,
                    "release must apply the final coordinate"
                );
            });
            let start = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(start, None, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(dialog.read(cx).drag.is_none(), "lost release recovers"));
        }
    }
}
