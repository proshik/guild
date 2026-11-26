//! Private fixture: the full Root and UI/CLI dispatch are not a public API.
use crate::Root;
use gpui::{AppContext, TestAppContext, px};
use guild_session::{Desk, Settings, Tool};

#[test]
fn isolated_fixed_footer_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::render::footer_tests::fixed_",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_FOOTER_TEST", "1")
        .env("GUILD_STATE_DIR", state.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_footer_window(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let project_dir = tempfile::tempdir().unwrap();
    let git = std::process::Command::new("git")
        .args(["init", "--quiet", "-b", "fixture"])
        .arg(project_dir.path())
        .status()
        .unwrap();
    assert!(git.success());
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), project_dir.path());
        for name in ["Side", "Third"] {
            desk.add_workspace(name, None).unwrap();
        }
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                Settings {
                    column_folded: false,
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.isolated_render = true;
            root.active_project = Some(project);
            root.branch = Some("fixture".into());
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    let closed = cx.debug_bounds("window-footer-bar").unwrap();
    let centered = cx.debug_bounds("footer-centered-tools").unwrap();
    let commit = cx.debug_bounds("footer-tool-Commit").unwrap();
    assert!(
        commit.right() <= centered.left(),
        "Commit assigned left must precede the centered bottom tools"
    );
    assert_eq!(
        cx.debug_bounds("work-surface").unwrap().left(),
        closed.left(),
        "pinned column must not reserve an extra gutter"
    );
    cx.update(|_, cx| root.update(cx, |r, cx| r.show_tool(Tool::Git, cx)));
    cx.run_until_parked();
    let opened = cx.debug_bounds("window-footer-bar").unwrap();
    assert_eq!(
        opened.bottom(),
        closed.bottom(),
        "opening bottom tool must not raise footer"
    );
    assert_eq!(opened.bottom(), px(800.));
    let edge = cx.debug_bounds("column-edge-grip").unwrap();
    let border = cx.update(|_, cx| root.read(cx).column_width());
    assert_eq!(
        edge.center().x,
        border,
        "resize target must straddle the visible column boundary"
    );
    let line = cx.debug_bounds("column-edge-line").unwrap();
    assert_eq!(line.center().x, border);
    let corner = px(cx.update(|_, cx| root.read(cx).rem_px(crate::theme::radius::WORK)));
    assert_eq!(
        line.top(),
        edge.top() + corner,
        "highlight starts below the card corner"
    );
    assert_eq!(
        line.bottom(),
        edge.bottom() - corner,
        "highlight ends above the card corner"
    );
    // The work-facing half must win hit testing over the surface behind it.
    let right_half = gpui::point(border + px(2.), edge.center().y);
    cx.simulate_mouse_down(right_half, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(root.read(cx).column_edge_drag));
    cx.simulate_mouse_up(right_half, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!root.read(cx).column_edge_drag));
    root.update(cx, |root, cx| {
        root.with_panels(|panels| panels.left.open = true);
        cx.notify();
    });
    cx.run_until_parked();
    let grip = cx.debug_bounds("grip-left").unwrap();
    let before = cx.update(|_, cx| root.read(cx).panels().left.size);
    cx.simulate_mouse_down(grip.center(), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let moved = grip.center() + gpui::point(px(2.), px(0.));
    cx.simulate_mouse_move(moved, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).panels().left.size,
            before + 2.,
            "slow dock drag must follow through its occluding grip"
        )
    });
    cx.simulate_mouse_up(moved, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    // Coalesced events need not paint a frame between press and release.
    // The release position is the final size, even without an intervening move.
    for delta in [2., -2., 40., -40.] {
        let edge = cx.debug_bounds("column-edge-grip").unwrap();
        let start = edge.center();
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
        let end = gpui::point(start.x + px(delta), start.y);
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).column_width(),
                end.x,
                "release must settle width"
            );
            assert!(
                !root.read(cx).column_edge_drag,
                "release must relinquish ownership"
            );
        });
    }
    for (x, width) in [
        (
            -2000.,
            crate::column::narrowest(guild_session::Settings::default().ui_font_size),
        ),
        (2000., crate::column::WIDEST),
        (232., 232.),
    ] {
        let start = cx.debug_bounds("column-edge-grip").unwrap().center();
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_up(
            gpui::point(px(x), start.y),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).column_width(),
                px(width),
                "release clamps column width"
            );
            assert!(!root.read(cx).column_edge_drag);
        });
    }
    assert!(
        edge.bottom() <= opened.top(),
        "column grip cannot occlude footer"
    );
    let id = cx.update(|_, cx| root.read(cx).active_workspace);
    let selector: &'static str = Box::leak(format!("workspace-switch-{id}").into_boxed_str());
    let switch = cx.debug_bounds(selector).unwrap();
    let controls = cx.debug_bounds("workspace-column-controls").unwrap();
    assert!(
        (f32::from(switch.center().y - controls.center().y)).abs() < 4.,
        "switcher stays in the column bottom row"
    );
    let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
    let update = cx.debug_bounds("update-branch").unwrap();
    assert!(
        explorer.top() > update.bottom(),
        "tools and branch occupy distinct frame rows"
    );
    assert!(
        cx.debug_bounds("update-source").is_none(),
        "there is no source chip beside Update branch"
    );
    // Exercise supported sizes; undersized resize clamping has its own fixture.
    for (width, height) in [(1200., 800.), (900., 500.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        cx.run_until_parked();
        let update = cx.debug_bounds("update-branch").unwrap();
        cx.simulate_mouse_move(update.center(), gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("update-source").is_none(),
            "hovering Update branch reveals no source chip"
        );
        let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
        assert!(explorer.top() > update.bottom());
        assert!(explorer.right() <= px(width));
        cx.simulate_mouse_move(
            gpui::point(px(width / 2.), px(50.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            let focus = root.read(cx).update_branch_focus.clone();
            window.focus(&focus, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(
                root.read(cx).update_branch_focus.is_focused(window),
                "keyboard fixture must focus update button before dispatch"
            )
        });
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).branch_face(),
                Some(crate::root::branches::Face::Sources)
            );
            let anchor = root.read(cx).update_branch_at.get();
            assert!(
                anchor.x > px(0.) && anchor.y < px(50.),
                "keyboard popup follows the titlebar trigger"
            );
        });
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_move(
            gpui::point(px(width / 2.), px(50.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
    }
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    let switch = cx.debug_bounds(selector).unwrap();

    cx.simulate_mouse_down(
        switch.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.simulate_mouse_up(
        switch.center(),
        gpui::MouseButton::Right,
        Default::default(),
    );
    cx.run_until_parked();
    let rename = cx.debug_bounds("menu-item-0").unwrap();
    cx.simulate_click(rename.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            matches!(
                root.read(cx).workspace_field,
                Some(crate::view::workspaces::WorkspaceField::Rename { .. })
            ),
            "workspace menu must take its action"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let grip = cx.debug_bounds("grip-bottom").unwrap();
    let before = cx.update(|_, cx| root.read(cx).panels().bottom.size);
    cx.simulate_mouse_down(grip.center(), gpui::MouseButton::Left, Default::default());
    let moved = grip.center() - gpui::point(px(0.), px(60.));
    cx.simulate_mouse_move(moved, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(moved, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("window-footer-bar").unwrap().bottom(),
        px(800.)
    );
    cx.update(|_, cx| {
        assert!(root.read(cx).dragging.is_none());
        assert!(
            root.read(cx).panels().bottom.size > before,
            "press/move/release must actually resize the dock"
        );
    });
    // A restored oversized dock must respond to the very first gesture.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.panels.as_mut().unwrap().bottom.size = 900.;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let before = cx.update(|_, cx| root.read(cx).bottom_dock_height());
    let grip = cx.debug_bounds("grip-bottom").unwrap();
    cx.simulate_mouse_down(grip.center(), gpui::MouseButton::Left, Default::default());
    let moved = grip.center() + gpui::point(px(0.), px(60.));
    cx.simulate_mouse_move(moved, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(moved, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let after = cx.update(|_, cx| {
        assert!(root.read(cx).dragging.is_none());
        root.read(cx).bottom_dock_height()
    });
    assert!(
        (f32::from(before - after) - 60.).abs() < 0.5,
        "first restored-dock gesture must shrink displayed height by 60: {before:?} -> {after:?}"
    );
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.panels.as_mut().unwrap().bottom.size = 900.;
            cx.notify();
        })
    });
    for _ in 0..3 {
        cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    }
    let upper = cx.debug_bounds("upper-work-surface").unwrap();
    assert!(
        f32::from(upper.size.height) >= guild_session::Panel::MIN_WORK,
        "maximum dock must leave MIN_WORK in actual upper layout: {upper:?}"
    );
    // Each keyboard route starts at the visible height on its first decrease.
    for saved in [900., 260.] {
        for stretch in [true, false] {
            cx.update(|window, cx| {
                root.update(cx, |r, cx| {
                    r.settings.panels.as_mut().unwrap().bottom.size = saved;
                    r.last_panel = crate::view::panels::Side::Bottom;
                    window.focus(r.dock_focus(crate::view::panels::Side::Bottom), cx);
                    cx.notify();
                })
            });
            for _ in 0..3 {
                cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
                cx.run_until_parked();
            }
            let before = cx.update(|_, cx| root.read(cx).bottom_dock_height());
            let upper_before = cx.debug_bounds("upper-work-surface").unwrap();
            if stretch {
                cx.simulate_keystrokes("ctrl-alt-down");
            } else {
                cx.simulate_keystrokes("cmd-shift-right");
            }
            cx.run_until_parked();
            let after = cx.update(|_, cx| root.read(cx).bottom_dock_height());
            assert!(
                after < before,
                "first keyboard decrease must change displayed height: saved={saved}, stretch={stretch}, {before:?} -> {after:?}"
            );
            let upper_after = cx.debug_bounds("upper-work-surface").unwrap();
            assert!(
                upper_after.size.height > upper_before.size.height,
                "keyboard decrease must enlarge actual upper work geometry"
            );
            assert!(
                (f32::from(upper_after.size.height - upper_before.size.height)
                    - f32::from(before - after))
                .abs()
                    <= 0.5
            );
            if !stretch {
                assert!((f32::from(before - after) - 24.).abs() < 0.5);
            }
            cx.update(|_, cx| {
                assert!((root.read(cx).panels().bottom.size - f32::from(after)).abs() < 0.5)
            });
        }
    }
    // Moving a tool changes its destination, never its footer reachability.
    for dock in guild_session::Dock::ALL {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.move_tool(Tool::Explorer, *dock, cx);
                r.move_tool(Tool::Git, *dock, cx);
                r.move_tool(Tool::Commit, *dock, cx);
            })
        });
        cx.run_until_parked();
        let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
        let selector = match dock {
            guild_session::Dock::Left => "footer-left-tools",
            guild_session::Dock::Bottom => "footer-centered-tools",
            guild_session::Dock::Right => "footer-right-tools",
        };
        let assigned = cx.debug_bounds(selector).unwrap();
        for tool in ["footer-tool-Git", "footer-tool-Commit"] {
            let button = cx.debug_bounds(tool).unwrap();
            assert!(
                button.left() >= assigned.left() && button.right() <= assigned.right(),
                "footer group must follow saved dock assignment"
            );
        }
        cx.simulate_click(explorer.center(), Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).panels().docks().dock_of(Tool::Explorer),
                *dock
            );
            assert_eq!(root.read(cx).tool_shown(*dock), Some(Tool::Explorer));
        });
    }
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.show_tool(Tool::Git, cx);
            r.stop_every_process(cx);
        })
    });
    for (width, height, font) in [(900., 500., 10.), (900., 500., 16.), (1200., 800., 12.)] {
        for (folded, peeking) in [(false, false), (true, false), (true, true)] {
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.settings.column_folded = folded;
                    r.column_peeking = peeking;
                    r.settings.ui_font_size = font;
                    r.settings.panels.as_mut().unwrap().bottom.size = 900.;
                    cx.notify();
                })
            });
            cx.simulate_resize(gpui::size(px(width), px(height)));
            cx.run_until_parked();
            let footer = cx.debug_bounds("window-footer-bar").unwrap();
            let viewport = cx.update(|window, _| window.viewport_size());
            assert_eq!(
                footer.bottom(),
                viewport.height,
                "footer stays inside the actual clamped window"
            );
            for _ in 0..3 {
                cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
                cx.run_until_parked();
            }
            let upper = cx.debug_bounds("upper-work-surface").unwrap();
            assert!(
                f32::from(upper.size.height) >= guild_session::Panel::MIN_WORK,
                "restored dock leaves actual MIN_WORK: font={font}, folded={folded}, {upper:?}"
            );
            let grip = cx.debug_bounds("grip-bottom").unwrap();
            assert!(
                grip.top() > px(34.),
                "oversized saved dock must leave room for upper work and footer"
            );
            assert!(
                cx.debug_bounds(selector).is_some() == (!folded || peeking),
                "workspace marks follow pinned or peeking column"
            );
            let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
            assert!(explorer.right() <= px(width));
        }
    }
    // Long labels must leave every control within the work-surface width.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.column_folded = false;
            r.column_peeking = false;
            r.settings.column_width = 420.;
            r.settings.ui_font_size = 20.;
            r.branch = Some("feature/very-long-current-branch-name-for-footer".into());
            let project = r.active_project.unwrap();
            let path = project_dir.path().to_path_buf();
            r.update_sources.insert(
                project,
                Ok(crate::git::branch_update::Snapshot {
                    checkout: None,
                    root: path,
                    head: None,
                    sources: vec![],
                    default: Some("refs/heads/very-long-update-source-name-for-footer".into()),
                }),
            );
            cx.notify();
        })
    });
    cx.simulate_resize(gpui::size(px(640.), px(800.)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    }
    let update = cx.debug_bounds("update-branch").unwrap();
    cx.simulate_mouse_move(update.center(), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let tools = cx.update(|_, cx| {
        Tool::ALL
            .iter()
            .copied()
            .filter(|tool| !tool.on_demand() || root.read(cx).tool_is_up(*tool))
            .collect::<Vec<_>>()
    });
    let mut names = vec!["branch-button", "update-branch"];
    names.extend(
        tools
            .into_iter()
            .map(|tool| &*Box::leak(format!("footer-tool-{tool:?}").into_boxed_str())),
    );
    for name in names {
        let bounds = cx
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("required control {name} missing"));
        assert!(
            bounds.left() >= cx.debug_bounds("upper-work-surface").unwrap().left()
                && bounds.right() <= cx.update(|window, _| window.viewport_size().width)
                && bounds.size.width > px(0.),
            "long-label control {name} must be reachable: {bounds:?}"
        );
    }
    let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
    assert!(explorer.top() > cx.debug_bounds("update-branch").unwrap().bottom());
    cx.simulate_mouse_move(
        gpui::point(px(600.), px(50.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.ui_font_size = 13.;
            cx.notify();
        })
    });
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = root.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_mouse_move(
        gpui::point(px(900.), px(50.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    }
    let before = cx.debug_bounds("window-footer-bar").unwrap();
    let branch_before = cx.debug_bounds("branch-button").unwrap();
    let height_before = cx.update(|_, cx| root.read(cx).footer_height.get());
    let update = cx.debug_bounds("update-branch").unwrap();
    cx.simulate_mouse_move(update.center(), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    // Measurements in prepaint schedule the following native frame.
    cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert!(cx.debug_bounds("update-source").is_none());
    assert_eq!(
        cx.debug_bounds("branch-button").unwrap().top(),
        branch_before.top(),
        "hovering Update branch must not move the branch to a different row"
    );
    assert_eq!(cx.debug_bounds("window-footer-bar").unwrap(), before);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).footer_height.get()),
        height_before
    );
    cx.simulate_mouse_move(
        gpui::point(px(900.), px(50.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| root.read(cx).footer_height.get()),
        height_before
    );
    // A background refresh can change the parent under a hovered button.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.update_sources
                .get_mut(&r.active_project.unwrap())
                .unwrap()
                .as_mut()
                .unwrap()
                .default = Some("refs/heads/main".into());
            cx.notify();
        })
    });
    for (old, new) in [
        ("feature/very-long-current-branch-name-for-footer", "main"),
        ("main", "feature/very-long-current-branch-name-for-footer"),
    ] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.branch = Some(old.into());
                cx.notify();
            })
        });
        for _ in 0..3 {
            cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
            cx.run_until_parked();
        }
        let old_position = cx.debug_bounds("update-branch").unwrap();
        cx.simulate_mouse_move(
            old_position.center(),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        // Match refresh_branch: mutate the model and notify without a pointer exit.
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.branch = Some(new.into());
                cx.notify();
            })
        });
        for _ in 0..3 {
            cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
            cx.run_until_parked();
        }
        let new_position = cx.debug_bounds("update-branch").unwrap();
        assert_eq!(
            old_position.top(),
            new_position.top(),
            "background label refresh must keep one footer row for {old} -> {new}"
        );
        cx.simulate_mouse_move(
            gpui::point(px(900.), px(50.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
    }
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_creation_dispatch(cx: &mut TestAppContext) {
    use crate::view::workspaces::WorkspaceField;
    use guild_session::command::{Answer, Command};
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let project_dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), project_dir.path());
        for name in ["Second", "Third", "Fourth"] {
            desk.add_workspace(name, None).unwrap();
        }
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                Settings {
                    column_folded: false,
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.active_project = Some(project);
            r.isolated_render = true;
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("workspace-new").is_some());
    for project in [
        Some(cx.update(|_, cx| root.read(cx).active_project.unwrap())),
        None,
    ] {
        // Exercise supported sizes; undersized resize clamping has its own fixture.
        for (width, height) in [(1200., 800.), (900., 500.)] {
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.active_project = project;
                    r.branch_read = None;
                    cx.notify();
                })
            });
            cx.simulate_resize(gpui::size(px(width), px(height)));
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("update-branch").is_none(),
                "non-Git workspace has no update control"
            );
            let left = cx.debug_bounds("work-surface").unwrap().left();
            let inset = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::WORK_INSET));
            let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
            assert!(
                (f32::from(explorer.left() - left) - inset).abs() < 0.1,
                "without Git Explorer starts at normal work inset"
            );
        }
    }
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_new_workspace(gpui::point(px(90.), px(770.)), window, cx);
            r.workspace_field = Some(WorkspaceField::Create {
                typed: "Deferred".into(),
                at: gpui::point(px(90.), px(770.)),
            });
            let (tx, rx) = std::sync::mpsc::channel();
            r.take_command(
                crate::hooks::server::Asked {
                    command: Command::WorkspaceCreate {
                        name: "Fifth".into(),
                        icon: None,
                        select: false,
                    },
                    answer: tx,
                },
                cx,
            );
            assert!(matches!(
                rx.recv().unwrap(),
                Answer::WorkspaceCreated { .. }
            ));
        })
    });
    // An already-open dialog must recheck capacity when Enter is submitted.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(root.read(cx).desk.workspaces().len(), 5));
    assert!(cx.debug_bounds("workspace-new").is_none());
    for font in [10., 13., 16., guild_session::MAX_UI_FONT_SIZE] {
        for folded in [false, true] {
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.settings.column_width = crate::column::narrowest(font);
                    r.settings.column_folded = folded;
                    r.column_peeking = folded;
                    r.settings.ui_font_size = font;
                    cx.notify();
                })
            });
            cx.run_until_parked();
            let controls = cx.debug_bounds("workspace-column-controls").unwrap();
            let inset = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::WORK_INSET));
            let edge = controls.right() - px(inset);
            let ids = cx.update(|_, cx| {
                root.read(cx)
                    .desk
                    .workspaces()
                    .iter()
                    .map(|w| w.id)
                    .collect::<Vec<_>>()
            });
            let mut previous = controls.left() + px(inset);
            for id in ids {
                let name: &'static str =
                    Box::leak(format!("workspace-switch-{id}").into_boxed_str());
                let bounds = cx.debug_bounds(name).unwrap();
                assert!(
                    bounds.left() + px(0.5) >= previous && bounds.right() <= edge + px(0.5),
                    "all five workspace hitboxes must fit inside the column without scrolling, font={font}, folded={folded}: {bounds:?}, previous={previous:?}, edge={edge:?}"
                );
                previous = bounds.right();
            }
        }
    }

    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            for _ in 0..2 {
                let (tx, rx) = std::sync::mpsc::channel();
                r.take_command(
                    crate::hooks::server::Asked {
                        command: Command::WorkspaceCreate {
                            name: "Racing CLI".into(),
                            icon: None,
                            select: true,
                        },
                        answer: tx,
                    },
                    cx,
                );
                let Answer::Refused { because } = rx.recv().unwrap() else {
                    panic!("CLI accepted at capacity")
                };
                assert_eq!(because, r.ui().text(crate::i18n::Text::WorkspaceCapacity));
            }
            let id = r.desk.workspaces()[4].id;
            r.desk.delete_workspace(id).unwrap();
            r.workspace_field = None;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("workspace-new").is_some());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_new_workspace(gpui::point(px(90.), px(770.)), window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_input("Recreated");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(root.read(cx).desk.workspaces().len(), 5);
        assert_eq!(root.read(cx).desk.workspaces()[4].name, "Recreated");
    });
    assert!(cx.debug_bounds("workspace-new").is_none());
}

// These fixtures exercise the complete Root, including its cached pinned column.
fn workspace_controls_fixture(cx: &mut TestAppContext, scenario: &str) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let project_dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), project_dir.path());
        desk.add_workspace("Second", None).unwrap();
        if scenario == "foot" {
            for name in ["Third", "Fourth", "Fifth"] {
                desk.add_workspace(name, None).unwrap();
            }
        }
        if scenario == "legacy" {
            let mut saved = serde_json::to_value(&desk).unwrap();
            let workspaces = saved["workspaces"].as_array_mut().unwrap();
            // Enough that the marks overflow even the column's floor.
            for at in 2..12 {
                workspaces.push(serde_json::json!({
                    "id": guild_session::WorkspaceId::new(),
                    "name": format!("Legacy {at}"), "projects": []
                }));
            }
            desk = serde_json::from_value(saved).unwrap();
        }
        let entity = cx.new(|cx| {
            let saved = Settings {
                column_folded: scenario == "folded" || scenario == "peek" || scenario == "footer",
                panels: Some(Default::default()),
                ..Default::default()
            };
            let restored = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            let mut r = Root::from_state(restored, desk, cx);
            r.isolated_render = true;
            r.active_project = Some(project);
            r.branch = Some("fixture".into());
            r.branch_read = Some(std::time::Instant::now());
            r.column_peeking = scenario == "peek";
            if scenario == "glyph" {
                // One agent, so the footer has its sessions control: with
                // none open there is nothing to list and no control.
                let seat = guild_session::Seat {
                    workspace: r.active_workspace,
                    project,
                };
                let session = r.desk.add_session(guild_session::SessionRecord::new(
                    "Agent".into(),
                    seat,
                    project_dir.path().into(),
                    guild_session::Runs::Claude(guild_session::SessionMode::Terminal),
                ));
                r.adopt(
                    seat,
                    project,
                    crate::Body::Broken {
                        session,
                        reason: String::new(),
                    },
                    cx,
                );
            }
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    let ids = cx.update(|_, cx| {
        root.read(cx)
            .desk
            .workspaces()
            .iter()
            .map(|w| w.id)
            .collect::<Vec<_>>()
    });
    let selectors: Vec<&'static str> = ids
        .iter()
        .map(|id| &*Box::leak(format!("workspace-switch-{id}").into_boxed_str()))
        .collect();
    let inset = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::WORK_INSET));
    match scenario {
        "measurements" => {
            for right in [true, false] {
                let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
                let counted = notifications.clone();
                let _subscription = cx
                    .update(|_, cx| cx.observe(&root, move |_, _| counted.set(counted.get() + 1)));
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        if right {
                            r.status_right_width.set(px(-1.));
                        } else {
                            r.status_tools_width.set(px(0.));
                        }
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                assert!(
                    notifications.get() >= 2,
                    "prepaint width measurement must notify Root after the triggering frame: right={right}"
                );
                assert!(cx.debug_bounds("footer-centered-tools").is_some());
            }
        }
        "hover-reset" => {
            let update = cx.debug_bounds("update-branch").unwrap();
            cx.simulate_mouse_move(update.center(), gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            // Workspace switching by keyboard removes the group without a pointer exit.
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    assert!(r.select_workspace(ids[1], cx));
                    r.branch = None;
                    cx.notify();
                })
            });
            cx.run_until_parked();
            assert!(cx.debug_bounds("update-branch").is_none());
            cx.simulate_mouse_move(
                gpui::point(px(900.), px(50.)),
                gpui::MouseButton::Left,
                Default::default(),
            );
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    assert!(r.select_workspace(ids[0], cx));
                    r.branch = Some("fixture".into());
                    r.branch_read = Some(std::time::Instant::now());
                    cx.notify();
                })
            });
            cx.run_until_parked();
            assert!(cx.debug_bounds("update-branch").is_some());
            assert!(
                cx.debug_bounds("update-source").is_none(),
                "returning brings back Update branch alone, with no source chip"
            );
        }
        "dock-budget" => {
            cx.simulate_resize(gpui::size(px(900.), px(500.)));
            cx.run_until_parked();
            for left_open in [true, false] {
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        let panels = r.settings.panels.as_mut().unwrap();
                        panels.left.open = left_open;
                        panels.right.open = !left_open;
                        panels.left.size = if left_open { 500. } else { 280. };
                        panels.right.size = if left_open { 280. } else { 500. };
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                cx.update(|_, cx| {
                    let panels = root.read(cx).panels();
                    assert_eq!(
                        if left_open {
                            panels.left.size
                        } else {
                            panels.right.size
                        },
                        500.,
                        "a closed dock must not shrink the open dock"
                    );
                    assert_eq!(
                        if left_open {
                            panels.right.size
                        } else {
                            panels.left.size
                        },
                        280.,
                        "closed dock size is retained independently"
                    );
                });
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        let panels = r.settings.panels.as_mut().unwrap();
                        panels.left.open = true;
                        panels.right.open = true;
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                cx.update(|_, cx| {
                    let r = root.read(cx);
                    let panels = r.panels();
                    assert!(
                        panels.left.size + panels.right.size <= 578.1,
                        "opening the second dock applies the joint work-area limit"
                    );
                    assert!(panels.left.size >= guild_session::Panel::MIN_WIDTH);
                    assert!(panels.right.size >= guild_session::Panel::MIN_WIDTH);
                });
            }
        }
        "baseline" => {
            for folded in [false, true] {
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        r.settings.column_folded = folded;
                        r.column_peeking = folded;
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                let mark = cx.debug_bounds(selectors[0]).unwrap();
                let tools = cx.debug_bounds("footer-tool-Commit").unwrap();
                assert!(
                    mark.bottom() <= tools.top(),
                    "workspace marks stay inside the column card above the footer"
                );
                for selector in ["column-open", "column-get-from-version-control"] {
                    let action = cx.debug_bounds(selector).unwrap();
                    assert!(
                        f32::from(action.center().y - tools.center().y).abs() < 0.6,
                        "only project actions share the window footer baseline"
                    );
                }
                assert!(
                    mark.bottom() <= cx.debug_bounds("column-edge-grip").unwrap().bottom(),
                    "marks belong to the column card"
                );
            }
        }
        "single-row" => {
            // A control-size trial must not persist a narrower saved column.
            // The saved width is UI20's floor: a narrower one would be raised.
            let wide = crate::column::narrowest(20.);
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.settings.ui_font_size = 20.;
                    r.settings.column_width = wide;
                    r.store_settings();
                    cx.notify();
                })
            });
            cx.simulate_resize(gpui::size(px(1100.), px(800.)));
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    root.read(cx).settings.column_width,
                    wide,
                    "initial constrained UI20 render preserves the saved column"
                )
            });
            let saved: guild_session::Settings =
                guild_session::Store::new(crate::paths::settings_file())
                    .load()
                    .unwrap();
            assert_eq!(
                saved.column_width, wide,
                "render must not persist a narrower column"
            );
            for font in [10., 13., 16., guild_session::MAX_UI_FONT_SIZE] {
                for extra in [0., 400.] {
                    cx.update(|_, cx| {
                        root.update(cx, |r, cx| {
                            r.settings.ui_font_size = font;
                            r.settings.column_width = crate::column::narrowest(font);
                            r.branch = Some("feature/long-current-branch-for-minimum".into());
                            cx.notify();
                        })
                    });
                    let least = crate::frame::minimum(font, 0., 0.);
                    cx.simulate_resize(gpui::size(least.width + px(extra), least.height));
                    cx.run_until_parked();
                    let update = cx.debug_bounds("update-branch").unwrap();
                    cx.simulate_mouse_move(
                        update.center(),
                        gpui::MouseButton::Left,
                        Default::default(),
                    );
                    cx.run_until_parked();
                    let footer = cx.debug_bounds("window-footer-bar").unwrap();
                    let tools = cx.debug_bounds("footer-centered-tools").unwrap();
                    let branch = cx.debug_bounds("branch-button").unwrap();
                    let explorer = cx.debug_bounds("footer-tool-Explorer").unwrap();
                    assert!(
                        branch.bottom() < footer.top() && tools.top() >= footer.top(),
                        "branch is in the header, tools retain one footer row"
                    );
                    assert!(
                        explorer.right() <= tools.left(),
                        "source/Explorer must clear centered icons"
                    );
                    assert!(
                        (f32::from(tools.size.height) - 24. * font / 13.).abs() < 0.6,
                        "tools must never wrap"
                    );
                    let mut previous = None;
                    for tool in Tool::ALL
                        .iter()
                        .filter(|t| **t != Tool::Explorer && !t.on_demand())
                    {
                        let selector: &'static str =
                            Box::leak(format!("footer-tool-{tool:?}").into_boxed_str());
                        let bounds = cx.debug_bounds(selector).unwrap();
                        if let Some(right) = previous {
                            assert!(bounds.left() >= right, "footer icons never overlap");
                        }
                        previous = Some(bounds.right());
                    }
                }
            }
        }
        "minimum" => {
            cx.simulate_resize(gpui::size(px(100.), px(100.)));
            cx.run_until_parked();
            cx.update(|window, _| {
                assert!(
                    window.viewport_size().width >= px(900.),
                    "minimum width must stop shrinking"
                );
                assert!(
                    window.viewport_size().height >= px(500.),
                    "minimum height preserves docks and work"
                );
            });
        }
        "scale-growth" => {
            cx.simulate_resize(gpui::size(px(900.), px(500.)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                assert_eq!(window.viewport_size().width, px(900.));
                root.update(cx, |r, cx| {
                    assert_eq!(r.settings.ui_font_size, 13.);
                    r.settings.ui_font_size = guild_session::MAX_UI_FONT_SIZE;
                    cx.notify();
                });
            });
            cx.run_until_parked();
            cx.update(|window, _| {
                assert!(
                    window.viewport_size().width >= px(1024.),
                    "font growth must enlarge the existing window without a manual resize"
                );
                assert!(window.viewport_size().height >= px(500.));
            });
            assert!(cx.debug_bounds("footer-tool-Commit").is_some());
        }
        "glyph" => {
            for fullscreen in [false, true] {
                if fullscreen {
                    cx.update(|window, _| window.toggle_fullscreen());
                }
                for font in [10., 13., 16., guild_session::MAX_UI_FONT_SIZE] {
                    // A width saved before the floor rose — 150, the old
                    // 180 — is raised to it; the floor itself holds.
                    let floor = crate::column::narrowest(font);
                    for column_width in [150., 180., floor, 420.] {
                        let mut fixed = None;
                        for folded in [false, true] {
                            cx.update(|_, cx| {
                                root.update(cx, |r, cx| {
                                    r.settings.column_width = column_width;
                                    r.settings.column_folded = folded;
                                    r.settings.ui_font_size = font;
                                    cx.notify();
                                })
                            });
                            cx.run_until_parked();
                            let toggle = cx.debug_bounds("column-button").unwrap();
                            let back = cx.debug_bounds("go-back").unwrap();
                            let forward = cx.debug_bounds("go-forward").unwrap();
                            let branch = cx.debug_bounds("branch-button").unwrap();
                            let update = cx.debug_bounds("update-branch").unwrap();
                            assert!(
                                toggle.right() <= back.left() && back.right() <= forward.left()
                            );
                            if let Some(back_left) = fixed {
                                assert_eq!(
                                    back.left(),
                                    back_left,
                                    "history cannot follow the column"
                                );
                            }
                            fixed = Some(back.left());
                            let gap = cx.update(|_, cx| {
                                root.read(cx).rem_px(crate::theme::space::CARD_GAP)
                            });
                            let anchor = cx.update(|_, cx| root.read(cx).panel_geometry().column);
                            let saved = cx.update(|_, cx| root.read(cx).settings.column_width);
                            assert!(
                                saved >= floor - 0.01,
                                "a saved {column_width} is raised to the UI{font} floor {floor}: {saved}"
                            );
                            // The mode button stands in the column's corner —
                            // one gap between areas short of its edge — at
                            // every width down to the floor.
                            let between = cx.update(|_, cx| {
                                root.read(cx).rem_px(crate::theme::space::WORK_BETWEEN)
                            });
                            match cx.debug_bounds("column-mode") {
                                Some(mode) => {
                                    assert!(!folded && forward.right() <= mode.left());
                                    assert!(
                                        (f32::from(mode.right()) - (anchor - between)).abs() < 0.6,
                                        "UI{font} column {anchor}: the mode button {mode:?} in the column's corner"
                                    );
                                }
                                None => assert!(folded, "the mode button shows with the column"),
                            }
                            // A folded column brought out by the pointer: the
                            // button in the overlay's corner, the branch past it.
                            if folded {
                                cx.update(|_, cx| {
                                    root.update(cx, |r, cx| {
                                        r.column_peeking = true;
                                        cx.notify();
                                    })
                                });
                                cx.run_until_parked();
                                let mode = cx
                                    .debug_bounds("column-mode")
                                    .expect("shown while it peeks");
                                let edge = crate::column::width(saved, font);
                                assert!(
                                    (f32::from(mode.right()) - (edge - between)).abs() < 0.6,
                                    "UI{font} peeking column {edge}: the mode button {mode:?} in its corner"
                                );
                                assert!(forward.right() <= mode.left());
                                let over = cx.debug_bounds("branch-button").unwrap();
                                assert!(
                                    over.left() >= mode.right(),
                                    "the branch clears the overlay's button"
                                );
                                cx.update(|_, cx| {
                                    root.update(cx, |r, cx| {
                                        r.column_peeking = false;
                                        cx.notify();
                                    })
                                });
                                cx.run_until_parked();
                            }
                            let expected = anchor.max(f32::from(forward.right()) + gap);
                            assert!(
                                (f32::from(branch.left()) - expected).abs() < 0.6,
                                "branch follows content boundary without reserving titlebar Sessions"
                            );
                            let summary = cx.debug_bounds("session-list");
                            if folded {
                                assert!(summary.is_none(), "hidden workspaces must hide Sessions");
                            } else {
                                assert_sessions_on_bottom_footer(&root, cx);
                                assert!(
                                    cx.debug_bounds("workspace-new").is_some(),
                                    "+ remains available"
                                );
                            }
                            assert!(branch.right() <= update.left());
                            assert!(
                                update.right()
                                    <= cx.debug_bounds("titlebar-open-in").unwrap().left()
                            );
                        }
                    }
                }
            }
        }
        "foot" => {
            let saved_desk = cx.update(|_, cx| serde_json::to_value(&root.read(cx).desk).unwrap());
            for count in 1..=5 {
                for font in [10., 13., guild_session::MAX_UI_FONT_SIZE] {
                    for width in [crate::column::narrowest(font), 420.] {
                        for peeking in [false, true] {
                            cx.update(|_, cx| {
                                root.update(cx, |r, cx| {
                                    let mut desk = saved_desk.clone();
                                    desk["workspaces"].as_array_mut().unwrap().truncate(count);
                                    r.desk = serde_json::from_value(desk).unwrap();
                                    r.settings.ui_font_size = font;
                                    r.settings.column_width = width;
                                    r.settings.column_folded = peeking;
                                    r.column_peeking = peeking;
                                    cx.notify();
                                })
                            });
                            cx.run_until_parked();
                            let foot = cx.debug_bounds("workspace-column-controls").unwrap();
                            let open = cx.debug_bounds("column-open").unwrap();
                            let clone = cx.debug_bounds("column-get-from-version-control").unwrap();
                            assert!(
                                open.top()
                                    >= cx.debug_bounds("workspace-column-actions").unwrap().top()
                                    && clone.bottom()
                                        <= cx
                                            .debug_bounds("workspace-column-actions")
                                            .unwrap()
                                            .bottom(),
                                "actions belong in the foot"
                            );
                            assert!(
                                cx.debug_bounds("column-hide").is_none(),
                                "identity head has no hide/pin button"
                            );
                            let first = cx.debug_bounds(selectors[0]).unwrap();
                            let last = cx
                                .debug_bounds(if count < 5 {
                                    "workspace-new"
                                } else {
                                    selectors[count - 1]
                                })
                                .unwrap();
                            assert!(
                                last.right() <= foot.right(),
                                "five marks must fit without scrolling"
                            );
                            assert!(f32::from(first.size.width) > 0.);
                            let centered = (f32::from(first.left() + last.right()) / 2.
                                - f32::from(foot.center().x))
                            .abs()
                                < 0.6;
                            assert!(
                                centered,
                                "marks center inside the column independently of footer actions"
                            );
                            if peeking {
                                let toggle = cx.debug_bounds("column-button").unwrap();
                                cx.simulate_click(toggle.center(), Default::default());
                                cx.run_until_parked();
                                cx.update(|_, cx| {
                                    assert!(!root.read(cx).settings.column_folded);
                                    assert!(!root.read(cx).column_peeking);
                                });
                            }
                        }
                    }
                }
            }
            let clone = cx.debug_bounds("column-get-from-version-control").unwrap();
            let open = cx.debug_bounds("column-open").unwrap();
            cx.simulate_click(open.center(), Default::default());
            cx.run_until_parked();
            assert!(
                cx.did_prompt_for_paths(),
                "Open retains its folder picker action"
            );
            cx.simulate_path_prompt_response(|_| None);
            cx.run_until_parked();
            cx.simulate_click(clone.center(), Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).clone_project.is_some()));
        }
        "peek-pin" => {
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.settings.column_folded = true;
                    r.column_peeking = true;
                    cx.notify();
                })
            });
            cx.run_until_parked();
            let toggle = cx.debug_bounds("column-button").unwrap();
            cx.simulate_click(toggle.center(), Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert!(!root.read(cx).settings.column_folded);
                assert!(!root.read(cx).column_peeking);
            });
        }
        "legacy" => {
            for folded in [false, true] {
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        r.settings.column_width = crate::column::narrowest(r.settings.ui_font_size);
                        r.settings.column_folded = folded;
                        r.column_peeking = folded;
                        r.workspace_swipe = crate::swipe::Detector::default();
                        r.workspace_swipe_offset = 0.;
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                let controls = cx.debug_bounds("workspace-column-controls").unwrap();
                // The same footer slot retains its scroll offset across pin/peek.
                cx.simulate_mouse_move(
                    controls.center(),
                    gpui::MouseButton::Left,
                    Default::default(),
                );
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: controls.center(),
                    delta: gpui::ScrollDelta::Pixels(gpui::point(px(1000.), px(0.))),
                    touch_phase: gpui::TouchPhase::Moved,
                    modifiers: Default::default(),
                });
                cx.run_until_parked();
                let first = cx.debug_bounds(selectors[0]).unwrap();
                let last = cx.debug_bounds(selectors[selectors.len() - 1]).unwrap();
                assert!(
                    last.right() > controls.right(),
                    "legacy fixture must overflow"
                );
                let position = controls.center();
                cx.simulate_mouse_move(position, gpui::MouseButton::Left, Default::default());
                for (phase, dx) in [
                    (gpui::TouchPhase::Started, 0.),
                    (gpui::TouchPhase::Moved, -240.),
                ] {
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position,
                        delta: gpui::ScrollDelta::Pixels(gpui::point(px(dx), px(0.))),
                        touch_phase: phase,
                        modifiers: Default::default(),
                    });
                    cx.run_until_parked();
                }
                cx.update(|_, cx| {
                    assert_eq!(
                        root.read(cx).workspace_swipe_offset,
                        0.,
                        "marks scroll must not move carousel pages"
                    );
                    assert_eq!(root.read(cx).active_workspace, ids[0]);
                });
                assert!(
                    cx.debug_bounds(selectors[0]).unwrap().left() < first.left(),
                    "horizontal wheel must scroll legacy marks"
                );
                assert!(
                    cx.debug_bounds(selectors[selectors.len() - 1])
                        .unwrap()
                        .right()
                        <= controls.right()
                );
                // The project pages retain their precise horizontal carousel gesture.
                let position = gpui::point(px(90.), px(150.));
                cx.simulate_mouse_move(position, gpui::MouseButton::Left, Default::default());
                for (phase, dx) in [
                    (gpui::TouchPhase::Started, 0.),
                    (gpui::TouchPhase::Moved, -40.),
                ] {
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position,
                        delta: gpui::ScrollDelta::Pixels(gpui::point(px(dx), px(0.))),
                        touch_phase: phase,
                        modifiers: Default::default(),
                    });
                    cx.run_until_parked();
                }
                cx.update(|_, cx| {
                    assert_ne!(
                        root.read(cx).workspace_swipe_offset,
                        0.,
                        "pages still accept carousel swipes"
                    )
                });
            }
        }
        "pinned" | "peek" => {
            let edge = cx.debug_bounds("column-edge-grip").unwrap();
            let border = cx.update(|_, cx| root.read(cx).column_width());
            assert_eq!(
                edge.center().x,
                border,
                "pinned and peeking grips share the visible boundary"
            );
            for selector in &selectors {
                let mark = cx.debug_bounds(selector).unwrap();
                assert!(
                    mark.bottom() <= edge.bottom() - px(inset),
                    "column-owned marks stay above the window footer: {mark:?}, {edge:?}"
                );
                assert!(mark.left() >= px(inset) && mark.right() <= edge.right() - px(inset));
            }
            assert!(cx.debug_bounds("workspace-footer-controls").is_none());
            if scenario == "peek" {
                let at = gpui::point(border + px(2.), edge.center().y);
                cx.simulate_mouse_move(at, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| {
                    assert!(
                        root.read(cx).column_peeking,
                        "the right half of the grip belongs to the peeking column"
                    )
                });
                cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                let moved = gpui::point(border + px(30.), edge.center().y);
                cx.simulate_mouse_move(moved, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| {
                    assert_eq!(root.read(cx).column_width(), moved.x);
                    assert!(root.read(cx).column_peeking);
                });
                let shrunk = gpui::point(border - px(30.), edge.center().y);
                cx.simulate_mouse_move(shrunk, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| {
                    let floor = crate::column::narrowest(root.read(cx).settings.ui_font_size);
                    assert_eq!(root.read(cx).column_width(), shrunk.x.max(px(floor)))
                });
                cx.simulate_mouse_up(shrunk, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| assert!(!root.read(cx).column_edge_drag));
            }
            let second = cx.debug_bounds(selectors[1]).unwrap();
            cx.simulate_click(second.center(), Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    root.read(cx).active_workspace,
                    ids[1],
                    "mark click switches without reaching the surface beneath"
                )
            });
            if scenario == "peek" {
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        r.column_peeking = false;
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                assert!(cx.debug_bounds(selectors[1]).is_none());
            }
        }
        "folded" | "footer" => {
            if scenario == "folded" {
                assert!(
                    cx.debug_bounds(selectors[0]).is_none(),
                    "folding must hide workspace marks"
                );
                assert!(cx.debug_bounds("workspace-new").is_none());
            }
            for branch in [Some("fixture"), None] {
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        r.branch = branch.map(str::to_owned);
                        cx.notify();
                    })
                });
                cx.run_until_parked();
                let card = cx.debug_bounds("upper-work-surface").unwrap();
                let first = cx
                    .debug_bounds(if branch.is_some() {
                        "branch-button"
                    } else {
                        "footer-tool-Explorer"
                    })
                    .unwrap();
                if branch.is_none() {
                    assert!(
                        (f32::from(first.left() - card.left()) - inset).abs() < 0.5,
                        "Explorer without Git starts at the work card inset"
                    );
                } else {
                    assert!(
                        first.left() >= card.left(),
                        "branch follows title-bar clamp"
                    );
                }
            }
            cx.update(|window, cx| {
                let focus = root.read(cx).focus.clone();
                window.focus(&focus, cx);
            });
            cx.simulate_keystrokes("cmd-shift-alt-right");
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    root.read(cx).active_workspace,
                    ids[1],
                    "folded keyboard workspace navigation remains available"
                )
            });
        }
        "titlebar-drag" => {
            // Observe the native move request at its call boundary: TestWindow
            // cannot run an OS drag loop. Dispatch still uses production hitboxes.
            let background = gpui::point(px(750.), px(12.));
            cx.simulate_mouse_down(background, gpui::MouseButton::Left, Default::default());
            cx.simulate_mouse_move(
                background + gpui::point(px(20.), px(0.)),
                gpui::MouseButton::Left,
                Default::default(),
            );
            cx.simulate_mouse_up(background, gpui::MouseButton::Left, Default::default());
            cx.update(|_, cx| {
                assert_eq!(
                    root.read(cx).titlebar_drag_starts,
                    1,
                    "empty background must start the OS window move"
                )
            });
            for selector in [
                "column-button",
                "go-back",
                "go-forward",
                "branch-button",
                "update-branch",
                "titlebar-usage",
                "titlebar-open-selected",
                "titlebar-open-selector",
                "terminal-jobs-trigger",
            ] {
                let bounds = cx
                    .debug_bounds(selector)
                    .unwrap_or_else(|| panic!("missing {selector}"));
                let at = bounds.center();
                cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
                cx.simulate_mouse_move(background, gpui::MouseButton::Left, Default::default());
                cx.simulate_mouse_up(background, gpui::MouseButton::Left, Default::default());
                cx.run_until_parked();
                cx.update(|_, cx| {
                    assert_eq!(
                        root.read(cx).titlebar_drag_starts,
                        1,
                        "{selector} must own its press even after leaving the control"
                    )
                });
            }
            let toggle = cx.debug_bounds("column-button").unwrap();
            cx.simulate_click(toggle.center(), Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert!(
                    root.read(cx).settings.column_folded,
                    "control clicks remain functional"
                )
            });
            for position in [toggle.center(), background] {
                cx.simulate_event(gpui::MouseDownEvent {
                    position,
                    button: gpui::MouseButton::Left,
                    click_count: 2,
                    ..Default::default()
                });
                cx.simulate_event(gpui::MouseUpEvent {
                    position,
                    button: gpui::MouseButton::Left,
                    click_count: 2,
                    ..Default::default()
                });
            }
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    root.read(cx).titlebar_double_clicks,
                    1,
                    "only background double clicks reach the title bar"
                );
                assert_eq!(
                    root.read(cx).titlebar_drag_starts,
                    1,
                    "double clicks must not start a move"
                );
            });
        }
        "titlebar" => {
            for fullscreen in [false, true] {
                if fullscreen {
                    cx.update(|window, _| window.toggle_fullscreen());
                }
                for font in [10., 13., guild_session::MAX_UI_FONT_SIZE] {
                    for folded in [false, true] {
                        cx.update(|_, cx| {
                            root.update(cx, |r, cx| {
                                r.settings.column_folded = folded;
                                r.settings.ui_font_size = font;
                                cx.notify();
                            })
                        });
                        cx.run_until_parked();
                        let toggle = cx.debug_bounds("column-button").unwrap();
                        let back = cx.debug_bounds("go-back").unwrap();
                        let traffic = cx.update(|_, cx| {
                            if fullscreen {
                                root.read(cx).rem_px(crate::theme::space::WORK_INSET)
                            } else {
                                root.read(cx)
                                    .rem_px(crate::theme::space::TRAFFIC_LIGHTS)
                                    .max(82.)
                            }
                        });
                        assert!(
                            (f32::from(toggle.left()) - traffic).abs() < 0.5,
                            "toggle follows traffic lights, full={fullscreen}, folded={folded}: {toggle:?}"
                        );
                        assert!(toggle.bottom() < px(50.) && toggle.right() < back.left());
                        cx.simulate_click(toggle.center(), Default::default());
                        cx.run_until_parked();
                        cx.update(|_, cx| {
                            assert_eq!(root.read(cx).settings.column_folded, !folded)
                        });
                        // Observe delivery independently of AppleActionOnDoubleClick.
                        cx.simulate_event(gpui::MouseDownEvent {
                            position: toggle.center(),
                            button: gpui::MouseButton::Left,
                            click_count: 2,
                            ..Default::default()
                        });
                        cx.simulate_event(gpui::MouseUpEvent {
                            position: toggle.center(),
                            button: gpui::MouseButton::Left,
                            click_count: 2,
                            ..Default::default()
                        });
                        cx.run_until_parked();
                        cx.update(|_, cx| assert_eq!(root.read(cx).settings.column_folded, folded));
                        cx.update(|_, cx| {
                            assert_eq!(
                                root.read(cx).titlebar_double_clicks,
                                0,
                                "the title bar must not receive the toggle's double click"
                            )
                        });
                    }
                }
            }
        }
        _ => unreachable!(),
    }
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_marks_pinned(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "pinned");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_marks_folded(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "folded");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_marks_peeking(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "peek");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_toggle_titlebar(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "titlebar");
}

#[test]
fn workspaces_naming_preserves_the_toggle_action() {
    use crate::i18n::{Language, Text, Ui};
    for (language, name, action) in [
        (Language::English, "Workspaces", "Toggle Workspaces"),
        (
            Language::Russian,
            "Рабочие пространства",
            "Показать или скрыть рабочие пространства",
        ),
    ] {
        let ui = Ui(language);
        assert_eq!(ui.text(Text::MenuToggleColumn), name);
        assert_eq!(ui.text(Text::ActionToggleSidebar), action);
    }
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_workspace_folded_footer_inset(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "footer");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_legacy_marks_scroll_without_swiping_pages(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "legacy");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_footer_glyph_corner_alignment(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "glyph");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_column_foot_actions(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "foot");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_titlebar_pins_peek(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "peek-pin");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_column_window_footer_baseline(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "baseline");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_single_line_footer(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "single-row");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_native_minimum_size(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "minimum");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_footer_measurements_notify_after_prepaint(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "measurements");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_absent_branch_clears_hover(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "hover-reset");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_only_open_docks_share_width_budget(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "dock-budget");
}
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_font_growth_enlarges_minimum_window(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "scale-growth");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_session_navigator_dispatch(cx: &mut TestAppContext) {
    use guild_session::{Panes, Runs, Seat, SessionMode, SessionRecord, layout::Axis};
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let mut sessions = Vec::new();
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let other = desk.add_workspace("Other", None).unwrap();
        desk.open_in(other, dir.path());
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.isolated_render = true;
            r.active_project = Some(project);
            r.active_workspace = workspace;
            // A started window has restored the workspace in front; coming
            // back to it from the other must not restore it again.
            r.restored_workspaces.insert(workspace);
            r.restored_workspaces.insert(other);
            let mut ids = Vec::new();
            for (i, harness) in [
                Runs::Codex(SessionMode::Terminal),
                Runs::Claude(SessionMode::Terminal),
                Runs::OpenCode(SessionMode::Terminal),
            ]
            .into_iter()
            .enumerate()
            {
                let seat = Seat {
                    workspace: if i == 2 { other } else { workspace },
                    project,
                };
                let id = r.desk.add_session(SessionRecord::new(
                    format!("Navigator agent {i}"),
                    seat,
                    dir.path().into(),
                    harness,
                ));
                ids.push(id);
            }
            let first = crate::Body::Broken {
                session: ids[0],
                reason: "Owned fixture".into(),
            };
            let mut panes = Panes::one(first);
            let first_pane = panes.focused();
            let second_pane = panes.split(first_pane, Axis::Horizontal).unwrap();
            assert!(
                panes
                    .fill(
                        second_pane,
                        crate::Body::Broken {
                            session: ids[1],
                            reason: "Owned fixture".into()
                        }
                    )
                    .is_ok()
            );
            panes.focus(first_pane);
            let tab = r.adopt_panes(Seat { workspace, project }, project, panes, cx);
            r.adopt(
                Seat {
                    workspace: other,
                    project,
                },
                project,
                crate::Body::Broken {
                    session: ids[2],
                    reason: "Owned fixture".into(),
                },
                cx,
            );
            r.active_tab = Some(tab);
            r.board
                .report(ids[0], Some(guild_session::status::Reported::Working));
            r.board.report(
                ids[1],
                Some(guild_session::status::Reported::Blocked(Some(
                    guild_session::status::Kind::Permission,
                ))),
            );
            r.board.report(
                ids[2],
                Some(guild_session::status::Reported::Blocked(Some(
                    guild_session::status::Kind::Question,
                ))),
            );
            sessions = ids;
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    // Counters for every agent together: one working, two waiting, no
    // failure and so no red mark, no word.
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).session_counts(),
            crate::session_list::Counts {
                working: 1,
                waiting: 2,
                ..Default::default()
            }
        );
    });
    assert!(cx.debug_bounds("session-count-working").is_some());
    assert!(cx.debug_bounds("session-count-waiting").is_some());
    assert!(cx.debug_bounds("session-count-failed").is_none());
    assert!(cx.debug_bounds("session-count-done").is_none());
    assert!(cx.debug_bounds("session-summary-label").is_none());
    let trigger = cx.debug_bounds("session-list").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    let popup = cx.debug_bounds("session-navigator").unwrap();
    assert_sessions_on_bottom_footer(&root, cx);
    assert!(
        popup.bottom() <= trigger.top(),
        "navigator opens upward from its footer trigger"
    );
    assert!(
        (f32::from(popup.left() - trigger.left())).abs() < 0.6,
        "the popup lines up with its control's left edge: {popup:?} vs {trigger:?}"
    );
    assert!(popup.left() >= px(0.) && popup.top() >= px(0.) && popup.right() <= px(1200.));
    assert!(
        cx.debug_bounds("session-group-Running").is_none(),
        "one list, no section buttons"
    );
    // Waiting first, then the working one; every open agent is listed.
    let order = cx.update(|_, cx| root.read(cx).popup_sessions());
    assert_eq!(order.len(), 3);
    assert_eq!(
        order[2], sessions[0],
        "the working one comes after both waiting"
    );
    assert!(sessions.iter().all(|id| order.contains(id)));
    for id in &order {
        let selector = format!("session-row-state-{id}");
        assert!(
            cx.debug_bounds(Box::leak(selector.into_boxed_str()))
                .is_some(),
            "each row says its state"
        );
    }
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(root.read(cx).context_menu.is_none());
        assert_eq!(root.read(cx).front_session(), Some(order[0]));
    });
    let trigger = cx.debug_bounds("session-list").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).front_session(),
            Some(order[1]),
            "selection follows a session wherever it is"
        );
        assert!(root.read(cx).context_menu.is_none());
    });
    let trigger = cx.debug_bounds("session-list").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(root.read(cx).context_menu.is_none()));
    let trigger = cx.debug_bounds("session-list").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_click(gpui::point(px(1100.), px(500.)), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            root.read(cx).context_menu.is_none(),
            "outside click closes navigator"
        )
    });
    // A folded peek may show the column's workspace marks, but never Sessions.
    for peeking in [false, true] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.settings.column_folded = true;
                r.column_peeking = peeking;
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("session-list").is_none(),
            "Sessions must be absent when folded, even in peek"
        );
        assert_eq!(
            cx.debug_bounds("workspace-column-controls").is_some(),
            peeking,
            "fixture must actually expose the column controls in hover-peek"
        );
        if peeking {
            assert!(
                cx.debug_bounds("workspace-new").is_some(),
                "peek keeps the workspace + available"
            );
        }
    }
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.column_folded = false;
            r.column_peeking = false;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let trigger = cx.debug_bounds("session-list").unwrap();
    cx.simulate_click(trigger.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(900.), px(500.)));
    cx.run_until_parked();
    let popup = cx.debug_bounds("session-navigator").unwrap();
    let trigger = cx.debug_bounds("session-list").unwrap();
    assert!(popup.top() >= px(0.) && popup.right() <= px(900.) && popup.bottom() <= trigger.top());
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.toggle_column(cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("session-list").is_none());
    cx.update(|_, cx| {
        assert!(
            root.read(cx).context_menu.is_none(),
            "hiding the column closes its popup"
        )
    });
    // A failure is counted, and the hover names each agent's share; with
    // nothing to count the control says its name.
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            for id in &sessions {
                r.board.forget(*id);
            }
            // Every agent idle is still something to count: the idle
            // count, not the word.
            let idle = r.session_counts();
            assert!(!idle.is_empty());
            assert_eq!(idle.idle, sessions.len());
            r.board
                .report(sessions[0], Some(guild_session::status::Reported::Working));
            r.board
                .report(sessions[2], Some(guild_session::status::Reported::Error));
            assert_eq!(r.session_counts().failed, 1);
            assert_eq!(r.session_counts().idle, sessions.len() - 2);
            let breakdown = r.session_breakdown();
            assert!(
                breakdown.contains("OpenCode: 1 failed") && breakdown.contains("Codex: 1 working"),
                "{breakdown}"
            );
            assert!(
                breakdown.contains("idle"),
                "idle agents are in the hover: {breakdown}"
            );
            r.settings.language = guild_session::Language::Russian;
            let breakdown = r.session_breakdown();
            assert!(
                breakdown.contains("OpenCode: с ошибкой: 1") && breakdown.contains("простаивают"),
                "{breakdown}"
            );
            r.settings.language = guild_session::Language::English;
            for id in &sessions {
                r.board.forget(*id);
            }
        })
    });
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.column_folded = false;
            cx.notify();
        })
    });
    cx.run_until_parked();
    // Every agent idle: the idle count, last and quiet, and no word.
    assert!(cx.debug_bounds("session-count-idle").is_some());
    assert!(cx.debug_bounds("session-count-working").is_none());
    assert!(
        cx.debug_bounds("session-summary-label").is_none(),
        "idle agents are counted, not named"
    );
    // No agent open at all: the control says Sessions.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let tabs: Vec<_> = r.tabs.iter().map(|tab| tab.id).collect();
            for tab in tabs {
                r.close_tab(tab, cx);
            }
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| root.read(cx).session_counts().is_empty()));
    assert!(
        cx.debug_bounds("session-list").is_none(),
        "no agent open: nothing to list, so no control at all"
    );
    assert!(cx.debug_bounds("session-summary-label").is_none());
    // A full navigator must fit above its measured footer trigger even at UI20.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let seat = Seat {
                workspace: r.active_workspace,
                project: r.active_project.unwrap(),
            };
            for i in 0..30 {
                let id = r.desk.add_session(SessionRecord::new(
                    format!("Overflow agent {i}"),
                    seat,
                    dir.path().into(),
                    Runs::Codex(SessionMode::Terminal),
                ));
                r.adopt(
                    seat,
                    seat.project,
                    crate::Body::Broken {
                        session: id,
                        reason: "Owned fixture".into(),
                    },
                    cx,
                );
            }
            cx.notify();
        })
    });
    for font in [20., 13.] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.settings.ui_font_size = font;
                cx.notify();
            })
        });
        cx.simulate_resize(gpui::size(px(900.), px(500.)));
        cx.run_until_parked();
        let trigger = cx.debug_bounds("session-list").unwrap();
        cx.simulate_click(trigger.center(), Default::default());
        cx.run_until_parked();
        let popup = cx.debug_bounds("session-navigator").unwrap();
        let trigger = cx.debug_bounds("session-list").unwrap();
        // The window keeps its own minimum, which a large size raises.
        let window = cx.update(|window, _| window.viewport_size().width);
        assert!(
            popup.top() >= px(0.)
                && popup.bottom() <= trigger.top()
                && popup.left() >= px(0.)
                && popup.right() <= window,
            "complete UI{font} card must fit above trigger: {popup:?}, {trigger:?}"
        );
        let keys = cx.debug_bounds("session-navigator-keys").unwrap();
        assert!(keys.bottom() <= popup.bottom() && keys.top() >= popup.top());
        cx.update(|_, cx| {
            assert!(
                root.read(cx)
                    .context_menu
                    .as_ref()
                    .unwrap()
                    .scroll
                    .max_offset()
                    .y
                    > px(0.),
                "full list scrolls"
            )
        });
        for _ in 0..25 {
            cx.simulate_keystrokes("down");
            cx.run_until_parked();
        }
        cx.update(|_, cx| {
            assert!(
                root.read(cx)
                    .context_menu
                    .as_ref()
                    .unwrap()
                    .scroll
                    .offset()
                    .y
                    < px(0.),
                "keyboard selection scrolls to later rows"
            )
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(
                root.read(cx).context_menu.is_none(),
                "Enter still opens the selected session"
            )
        });
    }
}

fn assert_sessions_on_bottom_footer(root: &gpui::Entity<Root>, cx: &mut gpui::VisualTestContext) {
    let sessions = cx
        .debug_bounds("session-list")
        .expect("Sessions in pinned workspace footer");
    let segment = cx.debug_bounds("workspace-column-actions").unwrap();
    let open = cx.debug_bounds("column-open").unwrap();
    let down = cx.debug_bounds("column-get-from-version-control").unwrap();
    let footer = cx.debug_bounds("window-footer-bar").unwrap();
    let inset = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::WORK_INSET));
    assert!(
        (f32::from(sessions.center().y - open.center().y)).abs() < 0.6,
        "Sessions must share the folder-plus y center: {sessions:?} vs {open:?}"
    );
    assert!(
        (f32::from(sessions.center().y - down.center().y)).abs() < 0.6,
        "Sessions must share the down-arrow y center"
    );
    assert!(
        (f32::from(segment.right() - sessions.right()) - inset).abs() < 0.6,
        "Sessions must be right-aligned inside the workspace footer segment"
    );
    assert!(
        sessions.left() >= down.right() && sessions.size.width > px(0.),
        "Sessions must not collide with fixed left actions at narrow widths"
    );
    assert!(sessions.top() >= segment.top() && sessions.bottom() <= segment.bottom());
    assert_eq!(
        segment.bottom(),
        footer.bottom(),
        "workspace segment uses the global bottom footer"
    );
    assert!(
        cx.debug_bounds("workspace-column-controls")
            .unwrap()
            .bottom()
            <= segment.top(),
        "colored workspace selectors/+ stay above the footer"
    );
    assert!(
        cx.debug_bounds("workspace-sessions-footer").is_none(),
        "there must be no extra Sessions row in column content"
    );
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_titlebar_drag_ownership(cx: &mut TestAppContext) {
    workspace_controls_fixture(cx, "titlebar-drag");
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_resize_docks_and_layouts(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    use guild_session::layout::Axis;
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let directory = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), directory.path());
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.isolated_render = true;
            r.active_project = Some(project);
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1600.), px(1100.)));
    cx.run_until_parked();
    for (side, selector) in [
        (crate::view::panels::Side::Left, "grip-left"),
        (crate::view::panels::Side::Right, "grip-right"),
        (crate::view::panels::Side::Bottom, "grip-bottom"),
    ] {
        root.update(cx, |r, cx| {
            r.with_panels(|p| {
                p.left.open = side == crate::view::panels::Side::Left;
                p.right.open = side == crate::view::panels::Side::Right;
                p.bottom.open = side == crate::view::panels::Side::Bottom;
            });
            cx.notify();
        });
        cx.run_until_parked();
        let value = |r: &Root| {
            let p = r.panels();
            match side {
                crate::view::panels::Side::Left => p.left.size,
                crate::view::panels::Side::Right => p.right.size,
                crate::view::panels::Side::Bottom => p.bottom.size,
            }
        };
        for delta in [2., -2., 60., -60.] {
            let start = cx.debug_bounds(selector).unwrap().center();
            let before = cx.update(|_, cx| value(root.read(cx)));
            let end = start
                + if side == crate::view::panels::Side::Bottom {
                    gpui::point(px(0.), px(delta))
                } else {
                    gpui::point(px(delta), px(0.))
                };
            cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    value(root.read(cx)),
                    before + delta * side.towards(),
                    "{selector} must follow move"
                )
            });
            cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).dragging.is_none()));
            cx.simulate_mouse_move(start, None, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    value(root.read(cx)),
                    before + delta * side.towards(),
                    "released dock must stay still"
                )
            });
        }
        // A missing release ends on the next move without the left button.
        let start = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.simulate_mouse_move(start, None, Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert!(root.read(cx).dragging.is_none()));
    }
    root.update(cx, |r, cx| {
        r.with_panels(|p| {
            p.left.open = false;
            p.right.open = false;
            p.bottom.open = true;
            p.bottom.size = 360.;
        });
        r.move_tool(Tool::Git, guild_session::Dock::Bottom, cx);
        r.ensure_log(cx);
        let state = r.logs.get_mut(&r.active_project.unwrap()).unwrap();
        state.commits.push(crate::git::log::Commit {
            sha: "1234567890".into(),
            parents: vec![],
            author: "Fixture".into(),
            email: "fixture@example.invalid".into(),
            time: 0,
            refs: vec![],
            summary: "Fixture commit".into(),
            body: "Details".into(),
        });
        state.opened = Some("1234567890".into());
        cx.notify();
    });
    cx.run_until_parked();
    for (column, selector) in [
        (
            crate::view::panels::Column::LogBranches,
            "grip-log-branches",
        ),
        (crate::view::panels::Column::LogDetails, "grip-log-details"),
        (crate::view::panels::Column::LogMessage, "grip-log-message"),
    ] {
        let value = |r: &Root| {
            let columns = r.panels().columns;
            match column {
                crate::view::panels::Column::LogBranches => columns.branches,
                crate::view::panels::Column::LogDetails => columns.details,
                crate::view::panels::Column::LogMessage => columns.message,
            }
        };
        for delta in [2., -2., 40., -40.] {
            let start = cx.debug_bounds(selector).expect("Log grip drawn").center();
            let before = cx.update(|_, cx| value(root.read(cx)));
            let end = start
                + if column == crate::view::panels::Column::LogMessage {
                    gpui::point(px(0.), px(delta))
                } else {
                    gpui::point(px(delta), px(0.))
                };
            cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    value(root.read(cx)),
                    before + delta * column.towards(),
                    "{selector} must follow move"
                )
            });
            cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).column_drag.is_none()));
        }
    }
    root.update(cx, |r, cx| {
        r.with_panels(|p| {
            p.left.open = false;
            p.right.open = false;
            p.bottom.open = false;
        });
        let panes = guild_session::Panes::one(crate::Body::Settings {
            section: crate::view::settings::Section::General,
        })
        .emptied();
        let project = r.active_project.unwrap();
        let tab = r.adopt_panes(r.active_seat().unwrap(), project, panes, cx);
        r.show_tab(tab, cx);
        cx.notify();
    });
    cx.run_until_parked();
    // Both tree axes, through the actual full-window grips and layout model.
    for (axis, name) in [
        (Axis::Horizontal, "horizontal"),
        (Axis::Vertical, "vertical"),
    ] {
        cx.update(|window, cx| root.update(cx, |r, cx| r.split_pane(axis, window, cx)));
        cx.run_until_parked();
        let selector: &'static str = Box::leak(format!("pane-grip-{name}").into_boxed_str());
        for delta in [2., -2., 40., -40.] {
            let start = cx
                .debug_bounds(selector)
                .expect("pane split drawn")
                .center();
            let end = start
                + if axis == Axis::Horizontal {
                    gpui::point(px(delta), px(0.))
                } else {
                    gpui::point(px(0.), px(delta))
                };
            cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            let actual = cx.debug_bounds(selector).unwrap().center();
            let error = if axis == Axis::Horizontal {
                actual.x - end.x
            } else {
                actual.y - end.y
            };
            assert!(
                f32::from(error).abs() <= 8.,
                "pane grip must follow pointer: {actual:?}, {end:?}"
            );
            cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).pane_drag.is_none()));
        }
    }
    for (axis, selector) in [
        (Axis::Horizontal, "area-grip-horizontal"),
        (Axis::Vertical, "area-grip-vertical"),
    ] {
        cx.update(|window, cx| root.update(cx, |r, cx| r.split_area(axis, window, cx)));
        cx.run_until_parked();
        for delta in [2., -2., 40., -40.] {
            let start = cx
                .debug_bounds(selector)
                .expect("area split drawn")
                .center();
            let end = start
                + if axis == Axis::Horizontal {
                    gpui::point(px(delta), px(0.))
                } else {
                    gpui::point(px(0.), px(delta))
                };
            cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            let actual = cx.debug_bounds(selector).unwrap().center();
            let error = if axis == Axis::Horizontal {
                actual.x - end.x
            } else {
                actual.y - end.y
            };
            assert!(
                f32::from(error).abs() <= 8.,
                "area grip must follow pointer: {actual:?}, {end:?}"
            );
            cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).area_drag.is_none()));
        }
    }
}

#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_file_wrap_control(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let xml = "<document>unchanged</document>\r\n";
    std::fs::write(dir.path().join("a.xml"), xml).unwrap();
    std::fs::write(dir.path().join("b.xml"), xml).unwrap();
    std::fs::write(dir.path().join("README.md"), "Markdown prose\n").unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(Settings::default(), desk, cx);
            root.isolated_render = true;
            root.active_project = Some(project);
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("file-soft-wrap").is_none());
    let mut opened = Vec::new();
    for (name, default) in [("a.xml", false), ("b.xml", false), ("README.md", true)] {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.open_file(std::path::Path::new(name), window, cx)
            })
        });
        cx.run_until_parked();
        let file = cx.update(|_, cx| {
            let Some(crate::Body::File { tab, .. }) = root.read(cx).active_body() else {
                panic!("file")
            };
            assert_eq!(tab.read(cx).soft_wrap(), default);
            tab.clone()
        });
        let bounds = cx.debug_bounds("file-soft-wrap").expect("file wrap target");
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            file.update(cx, |f, cx| {
                assert_eq!(f.soft_wrap(), !default);
                assert!(!f.is_dirty());
                let look = crate::file_tab::Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: crate::theme::Theme::default(),
                    autosave: false,
                };
                let ui = crate::i18n::Ui(guild_session::Language::Russian);
                f.restyle(look, ui, cx);
                f.reload(window, cx);
                assert_eq!(f.soft_wrap(), !default);
            })
        });
        opened.push(file);
    }
    cx.update(|window, cx| {
        for file in &opened {
            assert_eq!(
                file.read(cx).soft_wrap(),
                file.read(cx).language().unwrap().name != "markdown"
            );
        }
        // An extension change reconfigures syntax but retains this editor's choice.
        std::fs::rename(dir.path().join("a.xml"), dir.path().join("a.rs")).unwrap();
        opened[0].update(cx, |file, cx| {
            file.relocate(std::path::Path::new("a.rs"), window, cx);
            assert!(file.soft_wrap());
            // Exercise recreation after a non-text reload, too.
            std::fs::remove_file(file.path()).unwrap();
            file.reload(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        opened[0].update(cx, |file, cx| {
            assert!(file.editor().is_none());
            std::fs::write(file.path(), xml).unwrap();
            file.reload(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        opened[0].update(cx, |file, _| {
            assert!(file.editor().is_some());
            assert!(file.soft_wrap());
        });
        let look = crate::file_tab::Look {
            family: "monospace".into(),
            font_size: 14.,
            theme: crate::theme::Theme::default(),
            autosave: false,
        };
        let ui = crate::i18n::Ui(guild_session::Language::English);
        let fresh = cx.new(|cx| {
            crate::file_tab::FileTab::open(
                dir.path(),
                std::path::Path::new("b.xml"),
                look,
                ui,
                window,
                cx,
            )
        });
        assert!(
            !fresh.read(cx).soft_wrap(),
            "new editor gets the language default"
        );
    });
    assert_eq!(
        std::fs::read(dir.path().join("b.xml")).unwrap(),
        xml.as_bytes()
    );
}

/// A server's line is the one part of the status bar that comes and goes
/// on its own. It once counted in the window's minimum: a long line —
/// indexing a deep path, or a refusal in the server's own words — grew the
/// window past the screen and took the growth from the column, for good.
/// The line now takes the room the rest leaves it and gives way.
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_server_line_keeps_the_window(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let git = std::process::Command::new("git")
        .args(["init", "--quiet", "-b", "fixture"])
        .arg(dir.path())
        .status()
        .unwrap();
    assert!(git.success());
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut root = None;
    let mut project = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let opened = desk.open_in(desk.active_workspace(), dir.path());
        project = Some(opened);
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                Settings {
                    column_folded: false,
                    column_width: 232.,
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.isolated_render = true;
            root.active_project = Some(opened);
            root.scoped_settings.identities.insert(
                opened,
                crate::repository_settings::resolve_repository(dir.path()),
            );
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let (root, project) = (root.unwrap(), project.unwrap());
    let floor = crate::column::narrowest(guild_session::Settings::default().ui_font_size);
    for (width, column) in [(1512., 232.), (1180., 232.), (900., floor)] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.settings.column_width = column;
                r.serverless.remove(&(project, "rust"));
                cx.notify();
            })
        });
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.open_file(std::path::Path::new("main.rs"), window, cx)
            })
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("server-state").is_none(), "a quiet server");
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.serverless.insert(
                    (project, "rust"),
                    crate::lsp::progress::Fault::Refused(
                        // A server's own words keep their line breaks; the
                        // bar measures and draws one line.
                        "the workspace at /a/very/deep/path/that/goes/on/and/on/for/a/while\n\
                         could not be loaded because its manifest names a toolchain this \
                         machine does not have installed"
                            .into(),
                    ),
                );
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(
                window.viewport_size().width,
                px(width),
                "a server's line must not grow the window at {width}"
            );
            assert_eq!(
                root.read(cx).settings.column_width,
                column,
                "nor take the column at {width}"
            );
        });
        let server = cx
            .debug_bounds("server-state")
            .expect("the failure is said");
        let tools = cx.debug_bounds("footer-centered-tools").unwrap();
        let facts = cx.debug_bounds("file-facts").expect("the file's facts");
        assert!(
            server.left() >= tools.right(),
            "the line clears the centred tools at {width}: {server:?} {tools:?}"
        );
        assert!(
            server.right() <= facts.left(),
            "and stops before the file's facts at {width}"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.serverless.remove(&(project, "rust"));
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds("file-facts").unwrap(),
            facts,
            "the file's facts are never cut or moved by the server at {width}"
        );
        assert_eq!(
            cx.debug_bounds("footer-centered-tools").unwrap(),
            tools,
            "nor are the centred tools at {width}"
        );
    }
    // The stable side is what the minimum keeps; where it alone fills the
    // right, the server's line gives way to nothing and pushes nothing off
    // the window: the largest scale, tools on the right, at the minimum.
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.ui_font_size = guild_session::MAX_UI_FONT_SIZE;
            // Every tool on the right: the stable side, not the left,
            // is then what the minimum is made of.
            for tool in Tool::ALL.iter().filter(|t| !Tool::RETIRED.contains(t)) {
                r.move_tool(*tool, guild_session::Dock::Right, cx);
            }
            cx.notify();
        })
    });
    cx.simulate_resize(gpui::size(px(100.), px(800.)));
    cx.run_until_parked();
    // The minimum is enforced by a resize from inside a frame: draw once
    // more at the size it settled on.
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| root.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    };
    settle(cx);
    let selectors = cx.update(|_, cx| {
        root.read(cx)
            .footer_tools(guild_session::Dock::Right)
            .into_iter()
            .map(|tool| &*Box::leak(format!("footer-tool-{tool:?}").into_boxed_str()))
            .chain(["file-facts"])
            .collect::<Vec<&'static str>>()
    });
    assert!(
        selectors.len() > 5,
        "the right holds the tools: {selectors:?}"
    );
    let bounds_of = |cx: &mut gpui::VisualTestContext| {
        selectors
            .iter()
            .map(|selector| cx.debug_bounds(selector).expect(selector))
            .collect::<Vec<_>>()
    };
    let idle = bounds_of(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.serverless.insert(
                (project, "rust"),
                crate::lsp::progress::Fault::Refused("refused\nin two lines".into()),
            );
            cx.notify();
        })
    });
    cx.run_until_parked();
    settle(cx);
    let width = cx.update(|window, _| window.viewport_size().width);
    let busy = bounds_of(cx);
    assert_eq!(busy, idle, "the stable side does not move for the server");
    for bounds in busy {
        assert!(
            bounds.right() <= width,
            "{bounds:?} past the window's {width:?}"
        );
    }
}

/// Update branch's hint is one short line — the reason and the source
/// belong to the popup a click opens, not to the hover.
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_update_branch_hint_is_one_line(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let (_project_dir, _root, cx) = update_branch_fixture(cx);
    let update = cx.debug_bounds("update-branch").unwrap();
    cx.simulate_mouse_move(update.center(), None, Default::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(crate::theme::tooltip::SHOW_DELAY);
    cx.run_until_parked();
    let tip = cx
        .debug_bounds("guild-control-tooltip")
        .expect("Update branch explains itself on hover");
    assert!(
        tip.size.height <= px(28.),
        "the hint is one line, not a title over a reason: {tip:?}"
    );
}

/// A window with one project on `fixture`, whose sources — `fixture` and
/// `other`, both local and on one commit — are already read; the
/// project's git worker reads the same ones again.
fn update_branch_fixture(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    gpui::Entity<Root>,
    &mut gpui::VisualTestContext,
) {
    let project_dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "--quiet", "-b", "fixture"][..],
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "init",
        ],
        &["branch", "other"],
    ] {
        let git = std::process::Command::new("git")
            .args(args)
            .current_dir(project_dir.path())
            .status()
            .unwrap();
        assert!(git.success(), "{args:?}");
    }
    let path = project_dir.path().canonicalize().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), project_dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.isolated_render = true;
            root.active_project = Some(project);
            root.branch = Some("fixture".into());
            let source = |name: &str| crate::git::branch_update::Source {
                reference: format!("refs/heads/{name}"),
                oid: "0".repeat(40),
                upstream: String::new(),
                remote: None,
            };
            root.update_sources.insert(
                project,
                Ok(crate::git::branch_update::Snapshot {
                    checkout: None,
                    root: path.clone(),
                    head: Some(("refs/heads/fixture".into(), "0".repeat(40))),
                    sources: vec![source("fixture"), source("other")],
                    default: None,
                }),
            );
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    (project_dir, root.unwrap(), cx)
}

/// A click on ↓ opens the compact source popup under it and updates
/// nothing; a source taken there is confirmed in a sheet, and only
/// **Update** remembers it — Cancel leaves the choice as it was.
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_update_branch_click_opens_the_sources(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let (project_dir, root, cx) = update_branch_fixture(cx);
    let remembered = |cx: &mut gpui::VisualTestContext| {
        let path = project_dir.path().canonicalize().unwrap();
        cx.update(|_, cx| {
            root.read(cx)
                .settings
                .branch_update_sources
                .get(&path)
                .cloned()
        })
    };
    for (answer, kept) in [("Cancel", None), ("Update", Some("refs/heads/other"))] {
        let update = cx.debug_bounds("update-branch").unwrap();
        cx.simulate_click(update.center(), Default::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            let root = root.read(cx);
            assert_eq!(
                root.branch_face(),
                Some(crate::root::branches::Face::Sources)
            );
            assert!(root.updating_branches.is_empty(), "a click updates nothing");
            assert_eq!(root.update_branch_at.get().x, update.left());
        });
        assert!(cx.debug_bounds("update-source").is_none());
        let row = cx.debug_bounds("update-source-1").expect("the other row");
        let width = f32::from(row.size.width);
        assert!(
            width < 300. && width > 250.,
            "rows sit on the narrow card: {row:?}"
        );
        cx.simulate_mouse_move(row.center(), None, Default::default());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| root.read(cx).picker_lit_row()), Some(1));
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();
        assert!(
            cx.has_pending_prompt(),
            "a chosen source is confirmed first"
        );
        assert_eq!(cx.update(|_, cx| root.read(cx).branch_face()), None);
        cx.simulate_prompt_answer(answer);
        cx.run_until_parked();
        assert_eq!(remembered(cx).as_deref(), kept, "after {answer}");
    }
    // The update Update ran is the one the button always ran: let it land.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while cx.update(|_, cx| !root.read(cx).updating_branches.is_empty()) {
        assert!(std::time::Instant::now() < until, "the update never landed");
        cx.update(|_, cx| root.update(cx, |root, cx| root.poll_git(cx)));
        std::thread::sleep(std::time::Duration::from_millis(20));
        cx.run_until_parked();
    }
    // Reopened, the remembered source is the lit row.
    let update = cx.debug_bounds("update-branch").unwrap();
    cx.simulate_click(update.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| root.read(cx).picker_lit_row()), Some(1));
}

/// The title bar's mode button: in the column's corner, switching
/// Projects and Sessions and back, pressed in Sessions, gone with the
/// column — and `Toggle Session List` switching the same mode.
#[gpui::test]
#[ignore = "run through isolated_fixed_footer_regressions"]
fn fixed_column_mode_button(cx: &mut TestAppContext) {
    use guild_session::settings::ColumnMode;
    assert_eq!(std::env::var("GUILD_FOOTER_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.isolated_render = true;
            r.active_project = Some(project);
            r.active_workspace = workspace;
            r.restored_workspaces.insert(workspace);
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    let mode = |r: &Root| r.settings.column_mode;
    let button = cx.debug_bounds("column-mode").expect("the mode button");
    let column = cx.update(|_, cx| f32::from(root.read(cx).column_width()));
    let between = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::WORK_BETWEEN));
    assert!(
        (f32::from(button.right()) - (column - between)).abs() < 0.6,
        "the button stands in the column's corner, against the work's card: {button:?}"
    );
    let lights = cx.debug_bounds("go-back").unwrap();
    assert!(
        (f32::from(button.center().y - lights.center().y)).abs() < 0.6,
        "in the row of the history arrows"
    );
    assert!(cx.debug_bounds("column-mode-on").is_none());
    cx.update(|_, cx| {
        assert_eq!(mode(root.read(cx)), ColumnMode::Projects);
        assert_eq!(
            root.read(cx).column_heading(root.read(cx).active_workspace),
            crate::i18n::Text::SidebarProjects
        );
    });
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(mode(root.read(cx)), ColumnMode::Sessions);
        assert_eq!(
            root.read(cx).column_heading(root.read(cx).active_workspace),
            crate::i18n::Text::SidebarSessions
        );
    });
    assert!(
        cx.debug_bounds("column-mode-on").is_some(),
        "pressed in Sessions"
    );
    let button = cx.debug_bounds("column-mode").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(mode(root.read(cx)), ColumnMode::Projects));
    // The keymap's action switches the same mode, both ways.
    for expected in [ColumnMode::Sessions, ColumnMode::Projects] {
        cx.dispatch_action(crate::actions::ToggleSessions);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(mode(root.read(cx)), expected));
    }
    // Hidden with the column, back with it.
    cx.update(|_, cx| root.update(cx, |r, cx| r.toggle_column(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("column-mode").is_none());
    cx.update(|_, cx| root.update(cx, |r, cx| r.toggle_column(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("column-mode").is_some());
}
