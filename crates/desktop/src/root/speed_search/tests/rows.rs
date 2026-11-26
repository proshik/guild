//! The Explorer's rows — decision 54: one solid
//! selection with no outline, blue while the tree has the keys and grey
//! while they are elsewhere; a softer grey under the pointer; the right
//! button selecting its row before the menu; speed search's match as the
//! selection. Drawn in a real window frame and read off its quads.

use super::{draw_now, explorer_fixture};
use crate::{Origin, Root};
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::path::{Path, PathBuf};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.path().join(name), "").unwrap();
    }
    dir
}

/// The solid fills painted under the middle of a row.
fn fills(cx: &mut VisualTestContext, row: &str) -> Vec<gpui::Hsla> {
    // `debug_bounds` takes a static name; a test's few leak nothing that matters.
    let selector: &'static str = Box::leak(format!("explorer-row-{row}").into_boxed_str());
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("row {row} is drawn"));
    draw_now(cx);
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let at = bounds.center().scale(scale);
        window
            .painted_quads()
            .iter()
            .filter(|quad| quad.bounds.contains(&at))
            .filter_map(|quad| quad.background.as_solid())
            .collect()
    })
}

fn colour(rgba: gpui::Rgba) -> gpui::Hsla {
    rgba.into()
}

/// Whether anything is outlined in `accent.patina` — the cursor's old mark.
fn outlined(cx: &mut VisualTestContext, root: &Entity<Root>) -> bool {
    draw_now(cx);
    cx.update(|window, cx| {
        let patina: gpui::Hsla = gpui::rgb(root.read(cx).theme.accent.patina).into();
        window.painted_quads().iter().any(|quad| {
            quad.border_color == patina && quad.border_widths.top.0 > 0.0 && {
                // The focused pane's outline is decision 19's, not a row's.
                let height = quad.bounds.size.height.0 / window.scale_factor();
                height < 40.0
            }
        })
    })
}

fn select(cx: &mut VisualTestContext, root: &Entity<Root>, name: &str) {
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.select_tree_path(Origin::Project, Path::new(name));
            window.focus(&r.tree().unwrap().focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn selected(cx: &mut VisualTestContext, root: &Entity<Root>) -> Vec<PathBuf> {
    cx.update(|_, cx| {
        root.read(cx)
            .tree()
            .unwrap()
            .files
            .selected
            .iter()
            .map(|(_, path)| path.clone())
            .collect()
    })
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_selected_row_is_blue_with_the_keys_grey_without_and_never_outlined(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let focused = colour(theme.file_row(None, crate::theme::Selected::Focused, theme.surface.app));
    let unfocused =
        colour(theme.file_row(None, crate::theme::Selected::Unfocused, theme.surface.app));
    select(cx, &root, "a.rs");
    assert!(fills(cx, "a.rs").contains(&focused), "blue with the keys");
    assert!(!outlined(cx, &root), "the cursor has no outline of its own");

    // The keys go to the window's own handle: the tree no longer has them.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            !root
                .read(cx)
                .tree()
                .unwrap()
                .focus
                .contains_focused(window, cx)
        )
    });
    let fill = fills(cx, "a.rs");
    assert!(fill.contains(&unfocused), "grey without the keys: {fill:?}");
    assert!(!fill.contains(&focused));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn the_row_under_the_pointer_is_a_soft_grey_and_a_selected_one_keeps_its_fill(
    cx: &mut TestAppContext,
) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let hover = colour(theme.file_row_hover(None, theme.surface.app));
    let focused = colour(theme.file_row(None, crate::theme::Selected::Focused, theme.surface.app));
    select(cx, &root, "a.rs");

    let b = cx.debug_bounds("explorer-row-b.rs").unwrap().center();
    cx.simulate_mouse_move(b, None, Modifiers::none());
    cx.run_until_parked();
    assert!(
        fills(cx, "b.rs").contains(&hover),
        "the pointer's row lights"
    );
    assert!(!fills(cx, "c.rs").contains(&hover), "and only that row");

    let a = cx.debug_bounds("explorer-row-a.rs").unwrap().center();
    cx.simulate_mouse_move(a, None, Modifiers::none());
    cx.run_until_parked();
    let fill = fills(cx, "a.rs");
    assert!(
        fill.contains(&focused) && !fill.contains(&hover),
        "{fill:?}"
    );
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn the_right_button_selects_its_row_before_the_menu_and_keeps_a_group(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let focused = colour(theme.file_row(None, crate::theme::Selected::Focused, theme.surface.app));
    select(cx, &root, "a.rs");

    let b = cx.debug_bounds("explorer-row-b.rs").unwrap().center();
    cx.simulate_mouse_down(b, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(b, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(selected(cx, &root), [PathBuf::from("b.rs")]);
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(r.context_menu.is_some(), "the menu follows the selection");
        assert_eq!(
            r.tree().unwrap().selected.as_deref(),
            Some(Path::new("b.rs"))
        );
        assert!(r.tabs.is_empty(), "the right button never opens");
    });
    assert!(
        fills(cx, "b.rs").contains(&focused),
        "the row stays blue under its own menu"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // A group the row is already in is kept, the row its cursor.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.tree_file_click(
                Origin::Project,
                Path::new("c.rs"),
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert_eq!(
        selected(cx, &root),
        [PathBuf::from("b.rs"), PathBuf::from("c.rs")]
    );
    let b = cx.debug_bounds("explorer-row-b.rs").unwrap().center();
    cx.simulate_mouse_down(b, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(b, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selected(cx, &root),
        [PathBuf::from("b.rs"), PathBuf::from("c.rs")]
    );
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().files.focus,
            Some((Origin::Project, PathBuf::from("b.rs")))
        )
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn speed_search_selects_its_match(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    select(cx, &root, "a.rs");
    cx.simulate_keystrokes("c");
    cx.run_until_parked();
    assert_eq!(selected(cx, &root), [PathBuf::from("c.rs")]);
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().selected.as_deref(),
            Some(Path::new("c.rs"))
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        selected(cx, &root),
        [PathBuf::from("c.rs")],
        "and the selection stays where the search left it"
    );
}

/// A file the selection was on goes away: its directory is the cursor and
/// takes the selection's fill, which the old outline used to mark.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_directory_the_selection_falls_back_to_is_filled(cx: &mut TestAppContext) {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/a.rs"), "").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let focused = colour(theme.file_row(None, crate::theme::Selected::Focused, theme.surface.app));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.tree_mut().unwrap().tree.expand(Path::new("docs"));
            cx.notify();
        })
    });
    select(cx, &root, "docs/a.rs");
    std::fs::remove_file(dir.path().join("docs/a.rs")).unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let state = r.tree_mut().unwrap();
            state.tree.refresh(Path::new("docs"));
            state.settle_selection();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(fills(cx, "docs").contains(&focused));
}

/// A window in the background draws its lists' selection grey, though
/// the list keeps its focus inside the window.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn an_inactive_window_draws_the_selection_grey(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let unfocused =
        colour(theme.file_row(None, crate::theme::Selected::Unfocused, theme.surface.app));
    select(cx, &root, "a.rs");
    cx.deactivate_window();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            root.read(cx)
                .tree()
                .unwrap()
                .focus
                .contains_focused(window, cx)
        )
    });
    assert!(fills(cx, "a.rs").contains(&unfocused));
}

/// A cut row dims what it shows, not its ground: the grey selection is
/// painted whole.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_cut_row_keeps_its_selection_whole(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let unfocused =
        colour(theme.file_row(None, crate::theme::Selected::Unfocused, theme.surface.app));
    select(cx, &root, "a.rs");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.file_clipboard(true, cx);
            window.focus(&r.focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let cut = root.update(cx, |r, cx| r.cut_sources(cx).len());
        assert_eq!(cut, 1, "a.rs is cut");
    });
    assert!(fills(cx, "a.rs").contains(&unfocused));
}

/// A Rename field takes the keys inside the tree's own focus: the row it
/// names is drawn grey while the field has them.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_rename_field_takes_the_keys_from_the_list(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let unfocused =
        colour(theme.file_row(None, crate::theme::Selected::Unfocused, theme.surface.app));
    select(cx, &root, "a.rs");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let naming = crate::root::tree::Naming::rename(Path::new("a.rs")).unwrap();
            r.start_naming(naming, window, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    draw_now(cx);
    cx.run_until_parked();
    cx.update(|window, cx| assert!(crate::text_field::focused(window, cx)));
    assert!(fills(cx, "a.rs").contains(&unfocused));
}

/// A click on a folder ends speed search before it folds, so the query
/// cannot reconcile to a match elsewhere and select it.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_folder_click_ends_speed_search_before_it_folds(cx: &mut TestAppContext) {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/r.rs"), "").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    select(cx, &root, "a.rs");
    cx.simulate_keystrokes("r");
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            assert!(r.tree().unwrap().speed.is_some(), "the query is up");
            r.tree_click(
                Path::new("docs"),
                Path::new("docs"),
                true,
                Default::default(),
                window,
                cx,
            );
            // Before any frame or blur: the fold itself ends the search.
            assert!(r.tree().unwrap().speed.is_none());
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert!(state.speed.is_none());
        assert_eq!(state.selected.as_deref(), Some(Path::new("docs")));
    });
}

/// When the focused file goes away but other files stay selected, the
/// fallback folder is the cursor only, not part of the selection.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn a_fallback_folder_beside_a_surviving_group_is_not_filled(cx: &mut TestAppContext) {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/gone.rs"), "").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let theme = cx.update(|_, cx| root.read(cx).theme);
    let focused = colour(theme.file_row(None, crate::theme::Selected::Focused, theme.surface.app));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.tree_mut().unwrap().tree.expand(Path::new("docs"));
            cx.notify();
        })
    });
    select(cx, &root, "a.rs");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.tree_file_click(
                Origin::Project,
                Path::new("docs/gone.rs"),
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    });
    std::fs::remove_file(dir.path().join("docs/gone.rs")).unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let state = r.tree_mut().unwrap();
            state.tree.refresh(Path::new("docs"));
            state.settle_selection();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(selected(cx, &root), [PathBuf::from("a.rs")]);
    assert!(fills(cx, "a.rs").contains(&focused));
    assert!(!fills(cx, "docs").contains(&focused));
}

/// Command with an arrow moves the selection as the plain arrow does —
/// decision 54: no cursor walks invisibly apart from it.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn command_arrows_move_the_selection(cx: &mut TestAppContext) {
    let dir = fixture();
    let (root, cx) = explorer_fixture(cx, dir.path());
    select(cx, &root, "a.rs");
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    assert_eq!(selected(cx, &root), [PathBuf::from("b.rs")]);
    cx.simulate_keystrokes("cmd-up cmd-up");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert!(
            state.files.selected.is_empty(),
            "the project's row is not a file"
        );
        assert_eq!(state.selected.as_deref(), Some(Path::new("")));
    });
}
