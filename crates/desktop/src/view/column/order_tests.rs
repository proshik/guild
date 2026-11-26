//! Gestures in the real cached column, with unopened linked checkouts.
use super::*;
use gpui::{AppContext, TestAppContext};
use guild_session::Desk;

#[test]
fn isolated_worktree_order_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::column::order_tests::cached_column_",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_WORKTREE_ORDER_TEST", "1")
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
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

struct Fixture<'a> {
    dir: tempfile::TempDir,
    main: std::path::PathBuf,
    a: std::path::PathBuf,
    b: std::path::PathBuf,
    common: std::path::PathBuf,
    root: gpui::Entity<Root>,
    cx: &'a mut gpui::VisualTestContext,
}
impl Drop for Fixture<'_> {
    fn drop(&mut self) {
        guild_session::store::flush();
    }
}
fn mount(cx: &mut TestAppContext) -> Fixture<'_> {
    assert_eq!(
        std::env::var("GUILD_WORKTREE_ORDER_TEST").as_deref(),
        Ok("1")
    );
    assert!(
        crate::paths::workspace_file()
            .starts_with(std::env::var_os("GUILD_STATE_DIR").expect("isolated child state"))
    );
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("repo");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    for path in [&main, &a, &b] {
        std::fs::create_dir_all(path).unwrap();
    }
    let common = main.join(".git");
    let list = crate::git::worktrees::parse(format!("worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/main\0\0worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/a\0\0worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/b\0\0", main.display(), a.display(), b.display()).as_bytes()).unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, &main);
        desk.set_repository(project, Some(common.clone()));
        desk.select_workspace(workspace);
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            root.worktrees.insert(common.clone(), list.clone());
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    Fixture {
        dir,
        main,
        a,
        b,
        common,
        root,
        cx,
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_drag_reorders_unopened_paths_without_navigation(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let dir = &fixture.dir;
    let main = fixture.main.clone();
    let a = fixture.a.clone();
    let b = fixture.b.clone();
    let common = fixture.common.clone();
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let (workspace, rows, active) = root.read_with(cx, |r, _| {
        (
            r.active_workspace,
            r.column_groups_for(r.active_workspace)[0].rows.clone(),
            r.active_project,
        )
    });
    let before = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[1]).into_boxed_str()))
        .unwrap();
    let source = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[2]).into_boxed_str()))
        .unwrap();
    cx.update(|window, cx| {
        let focus = root.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    let focus_before = cx.update(|window, cx| window.focused(cx));
    let target = gpui::point(before.center().x, before.top() + px(5.));
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus_before);
    root.read_with(cx, |r, _| {
        let rows = r.column_groups_for(workspace)[0].rows.clone();
        assert!(matches!(&rows[1], Row::Unopened {path, ..} if path == &b));
        assert_eq!(r.active_project, active);
        assert_eq!(r.desk.projects().count(), 1);
    });
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    assert_eq!(
        saved
            .workspace(workspace)
            .expect("drop saved this workspace")
            .worktree_order[&common],
        vec![main.clone(), b.clone(), a.clone()]
    );
    assert_eq!(
        saved.projects().count(),
        1,
        "saving an unopened row must not open it"
    );
    root.read_with(cx, |r, _| {
        assert_eq!(
            r.desk.workspace(workspace).unwrap().worktree_order[&common],
            vec![main.clone(), b.clone(), a.clone()]
        );
    });
    root.read_with(cx, |r, _| {
        use crate::root::column::DragWorktree;
        use guild_session::worktree_order::Insertion;
        let drag = DragWorktree {
            workspace,
            repository: common.clone(),
            path: b.clone(),
        };
        let insertion = Insertion::After(a.clone());
        assert!(
            r.worktree_drop_order(
                &drag,
                guild_session::WorkspaceId::new(),
                &common,
                &insertion
            )
            .is_none()
        );
        assert!(
            r.worktree_drop_order(&drag, workspace, &dir.path().join("other/.git"), &insertion)
                .is_none()
        );
        let primary_drag = DragWorktree {
            path: main.clone(),
            ..drag.clone()
        };
        assert!(
            r.worktree_drop_order(&primary_drag, workspace, &common, &insertion)
                .is_none()
        );
    });
    // Restore by dropping below the last row, then cancel a subsequent upward drag.
    let rows = root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows.clone());
    let source = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[1]).into_boxed_str()))
        .unwrap();
    let last = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[2]).into_boxed_str()))
        .unwrap();
    let end = gpui::point(last.center().x, last.bottom() - px(4.));
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| {
        assert_eq!(
            r.column_groups_for(workspace)[0].rows,
            rows.iter()
                .take(1)
                .cloned()
                .chain([rows[2].clone(), rows[1].clone()])
                .collect::<Vec<_>>()
        )
    });
    let source = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[1]).into_boxed_str()))
        .unwrap();
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "Escape ends the native drag"
    );
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| assert!(matches!(&r.column_groups_for(workspace)[0].rows[1], Row::Unopened { path, .. } if path == &a), "Escape cancels without reordering"));

    // A release lost outside the window is cancelled on the next buttonless move.
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_event(gpui::MouseMoveEvent {
        position: target,
        ..Default::default()
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    assert!(!root.read_with(cx, |r, _| r.worktree_dragging));

    // Ordinary row context menus still receive the actual mouse path.
    cx.simulate_mouse_down(target, gpui::MouseButton::Right, Default::default());
    cx.simulate_mouse_up(target, gpui::MouseButton::Right, Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| r.context_menu.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // The row targets do not swallow wheel events. Populate enough rows to
    // overflow a legal window now that its physical height floor is 500 points.
    let saved_worktrees = root.read_with(cx, |r, _| r.worktrees[&common].clone());
    let extra: Vec<_> = (0..8)
        .map(|i| {
            let path = dir.path().join(format!("scroll-{i}"));
            std::fs::create_dir_all(&path).unwrap();
            crate::git::worktrees::Worktree {
                path,
                branch: Some(format!("scroll-{i}")),
                ..Default::default()
            }
        })
        .collect();
    root.update(cx, |r, cx| {
        r.worktrees.get_mut(&common).unwrap().extend(extra);
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    cx.run_until_parked();
    let first = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[0]).into_boxed_str()))
        .unwrap();
    let last_before = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[1]).into_boxed_str()))
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: first.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-120.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    cx.run_until_parked();
    let last_after = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[1]).into_boxed_str()))
        .unwrap();
    assert!(
        last_after.top() < last_before.top(),
        "wheel scrolls rows past permanent drop targets"
    );
    root.update(cx, |r, cx| {
        r.worktrees.insert(common.clone(), saved_worktrees);
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    // Existing project-header drags retain their distinct payload and ordering.
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let other_common = other.join(".git");
    root.update(cx, |r, cx| {
        let id = r.desk.open_in(workspace, &other);
        r.desk.set_repository(id, Some(other_common.clone()));
        cx.notify();
    });
    cx.run_until_parked();
    let header = cx
        .debug_bounds(Box::leak(
            format!("project-header-{}", common.display()).into_boxed_str(),
        ))
        .unwrap();
    let other_header = cx
        .debug_bounds(Box::leak(
            format!("project-header-{}", other_common.display()).into_boxed_str(),
        ))
        .unwrap();
    // Projects move by the lines between them: the one after the other.
    let other_block = cx
        .debug_bounds(Box::leak(
            format!("project-block-{}", other_common.display()).into_boxed_str(),
        ))
        .unwrap();
    let after_other = gpui::point(other_header.center().x, other_block.bottom() - px(2.));
    cx.simulate_mouse_down(header.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(after_other, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(after_other, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| {
        assert_eq!(r.column_groups_for(workspace)[0].key, other_common);
        assert_eq!(
            r.desk.workspace(workspace).unwrap().worktree_order[&common],
            vec![main.clone(), a.clone(), b.clone()]
        );
        assert_eq!(r.active_project, active);
    });
    // Ordinary row clicks still select a different opened root.
    let primary_row = root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows[0].clone());
    let primary_bounds = cx
        .debug_bounds(Box::leak(row_key(workspace, &primary_row).into_boxed_str()))
        .unwrap();
    cx.simulate_click(primary_bounds.center(), Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| assert_ne!(r.active_project, active));
    // Refresh invalidates a payload naming a vanished unopened source or target.
    root.update(cx, |r, cx| {
        use crate::root::column::DragWorktree;
        use guild_session::worktree_order::Insertion;
        let drag = DragWorktree {
            workspace,
            repository: common.clone(),
            path: b.clone(),
        };
        r.worktrees
            .get_mut(&common)
            .unwrap()
            .retain(|row| row.path != b);
        assert!(
            r.worktree_drop_order(&drag, workspace, &common, &Insertion::After(a.clone()))
                .is_none()
        );
        let a_drag = DragWorktree {
            path: a.clone(),
            ..drag
        };
        assert!(
            r.worktree_drop_order(&a_drag, workspace, &common, &Insertion::After(b.clone()))
                .is_none()
        );
        cx.notify();
    });
    cx.run_until_parked();
    root.read_with(cx, |r, _| {
        assert_eq!(
            r.column_groups_for(workspace)
                .into_iter()
                .find(|g| g.key == common)
                .unwrap()
                .rows
                .len(),
            2
        )
    });
    // Deletion before the Git refresh is also rejected at drop time.
    std::fs::remove_dir(&a).unwrap();
    root.read_with(cx, |r, _| {
        use crate::root::column::DragWorktree;
        use guild_session::worktree_order::Insertion;
        let drag = DragWorktree {
            workspace,
            repository: common.clone(),
            path: a.clone(),
        };
        assert!(
            r.worktree_drop_order(&drag, workspace, &common, &Insertion::After(main.clone()))
                .is_none()
        );
    });
    guild_session::store::flush();
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_escape_cancels_the_drag_first(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (workspace, before) = root.read_with(cx, |r, _| {
        (
            r.active_workspace,
            r.column_groups_for(r.active_workspace)[0].rows.clone(),
        )
    });
    let source = cx
        .debug_bounds(Box::leak(row_key(workspace, &before[2]).into_boxed_str()))
        .unwrap();
    let target = cx
        .debug_bounds(Box::leak(row_key(workspace, &before[1]).into_boxed_str()))
        .unwrap();
    let point = gpui::point(target.center().x, target.top() + px(4.));
    let focus = cx.update(|window, cx| window.focused(cx));
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(point, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_keystrokes("escape");
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "Escape cancels the drag first"
    );
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows.clone()),
        before
    );
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_rejects_non_utf8_full_order_without_poisoning_saves(cx: &mut TestAppContext) {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let bad = fixture
        .dir
        .path()
        .join(std::ffi::OsString::from_vec(b"linked-\xff".to_vec()));
    // Git retains raw path bytes. APFS rejects making such a directory, so
    // keep this Git-listed row unopened and drag only real UTF-8 siblings.
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet", "-b", "fixture"])
            .arg(&fixture.main)
            .status()
            .unwrap()
            .success()
    );
    let mut bytes = b"worktree ".to_vec();
    bytes.extend_from_slice(bad.as_os_str().as_bytes());
    bytes.extend_from_slice(b"\0HEAD abc\0branch refs/heads/non-utf8\0\0");
    let extra = crate::git::worktrees::parse(&bytes).unwrap();
    assert_eq!(extra[0].path, bad);
    let common = fixture.common.clone();
    let a = fixture.a.clone();
    let b = fixture.b.clone();
    let cx = &mut *fixture.cx;
    root.update(cx, |r, cx| {
        r.worktrees.get_mut(&common).unwrap().extend(extra);
        cx.notify();
    });
    cx.run_until_parked();
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    let retained_order = vec![fixture.main.clone(), a.clone(), b.clone()];
    root.update(cx, |r, _| {
        assert!(
            r.desk
                .set_worktree_order(workspace, &common, retained_order.clone())
        );
    });
    let before = root.read_with(cx, |r, _| serde_json::to_value(&r.desk).unwrap());
    root.update(cx, |r, cx| {
        let drag = crate::root::column::DragWorktree {
            workspace,
            repository: common.clone(),
            path: b.clone(),
        };
        r.reorder_worktree(
            &drag,
            workspace,
            &common,
            &guild_session::worktree_order::Insertion::Before(a.clone()),
            cx,
        );
    });
    root.read_with(cx, |r, _| {
        assert_eq!(
            serde_json::to_value(&r.desk).expect("rejected order leaves Desk serializable"),
            before
        );
        assert!(
            r.notices
                .visible()
                .any(|(_, notice)| notice.level == crate::notify::Level::Warning),
            "unsupported order is explained to the user"
        );
    });
    // A later ordinary save must remain possible after the refused drop.
    root.update(cx, |r, _| r.save());
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    assert_eq!(
        saved.workspace(workspace).unwrap().worktree_order[&common],
        retained_order
    );
    assert_eq!(saved.projects().count(), 1);
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_waits_for_initial_git_list_without_losing_unopened_order(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let common = fixture.common.clone();
    let a = fixture.a.clone();
    let b = fixture.b.clone();
    let unopened_a = fixture.dir.path().join("unopened-a");
    let unopened_b = fixture.dir.path().join("unopened-b");
    for path in [&unopened_a, &unopened_b] {
        std::fs::create_dir_all(path).unwrap();
    }
    let cx = &mut *fixture.cx;
    let (workspace, saved, mut list) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        for path in [&a, &b] {
            let id = r.desk.open_in(workspace, path);
            r.desk.set_repository(id, Some(common.clone()));
        }
        let saved = vec![
            fixture.main.clone(),
            unopened_b.clone(),
            unopened_a.clone(),
            a.clone(),
            b.clone(),
        ];
        assert!(r.desk.set_worktree_order(workspace, &common, saved.clone()));
        let list = r.worktrees.remove(&common).unwrap();
        cx.notify();
        (workspace, saved, list)
    });
    root.update(cx, |r, cx| {
        let drag = crate::root::column::DragWorktree {
            workspace,
            repository: common.clone(),
            path: b.clone(),
        };
        r.reorder_worktree(
            &drag,
            workspace,
            &common,
            &guild_session::worktree_order::Insertion::Before(a.clone()),
            cx,
        );
    });
    assert_eq!(
        root.read_with(cx, |r, _| r
            .desk
            .workspace(workspace)
            .unwrap()
            .worktree_order[&common]
            .clone()),
        saved,
        "initial-list gap must not truncate saved unopened paths"
    );
    for path in [&unopened_a, &unopened_b] {
        list.push(crate::git::worktrees::Worktree {
            path: path.clone(),
            branch: Some("unopened".into()),
            ..Default::default()
        });
    }
    root.update(cx, |r, cx| {
        r.worktrees.insert(common.clone(), list);
        cx.notify();
    });
    assert_eq!(
        root.read_with(cx, |r, _| r.column_groups_for(workspace)[0]
            .rows
            .iter()
            .map(|row| r.column_row_path(row).unwrap())
            .collect::<Vec<_>>()),
        saved
    );
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_escape_preserves_editor_multiple_cursors(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    std::fs::write(fixture.main.join("note.txt"), "alpha\nbeta\n").unwrap();
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_file(std::path::Path::new("note.txt"), window, cx)
        })
    });
    cx.run_until_parked();
    let editor = root.read_with(cx, |r, cx| match r.active_body().unwrap() {
        crate::Body::File { tab, .. } => tab.read(cx).editor().unwrap().clone(),
        _ => panic!("file editor owns focus"),
    });
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(gpui_component::input::Position::new(0, 0), window, cx)
        })
    });
    cx.run_until_parked();
    let (caret, height) = editor.read_with(cx, |state, _| {
        (
            state.cursor_layout().unwrap().0,
            state.line_height().unwrap(),
        )
    });
    let second = gpui::point(caret.left() + px(1.), caret.center().y + height);
    cx.simulate_click(
        second,
        gpui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    cx.run_until_parked();
    // Positively witness two actual editing carets before testing Escape.
    cx.simulate_keystrokes("q");
    assert_eq!(
        editor.read_with(cx, |state, _| state.value().to_string()),
        "qalpha\nqbeta\n",
        "fixture has two actual editor cursors"
    );
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        editor.read_with(cx, |state, _| state.value().to_string()),
        "alpha\nbeta\n"
    );
    let (workspace, before, tab) = root.read_with(cx, |r, _| {
        (
            r.active_workspace,
            r.column_groups_for(r.active_workspace)[0].rows.clone(),
            r.active_tab,
        )
    });
    let source = cx
        .debug_bounds(Box::leak(row_key(workspace, &before[2]).into_boxed_str()))
        .unwrap();
    let target = cx
        .debug_bounds(Box::leak(row_key(workspace, &before[1]).into_boxed_str()))
        .unwrap();
    let point = gpui::point(target.center().x, target.top() + px(4.));
    let focus = cx.update(|window, cx| window.focused(cx));
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(point, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_keystrokes("escape");
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "Escape cancels before the editor collapses its cursors"
    );
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows.clone()),
        before
    );
    assert_eq!(root.read_with(cx, |r, _| r.active_tab), tab);
    assert_eq!(
        editor.read_with(cx, |state, _| state.value().to_string()),
        "alpha\nbeta\n"
    );
    cx.simulate_keystrokes("q");
    assert_eq!(
        editor.read_with(cx, |state, _| state.value().to_string()),
        "qalpha\nqbeta\n",
        "drag cancellation retains both editor cursors"
    );
    cx.simulate_keystrokes("escape q");
    let value = editor.read_with(cx, |state, _| state.value().to_string());
    assert!(
        ["qqalpha\nqbeta\n", "qalpha\nqqbeta\n"].contains(&value.as_str()),
        "ordinary Escape leaves only the active cursor: {value:?}"
    );
}

/// Real cached-column bounds, including the list's six-point insets. The
/// subprocess wrapper above isolates every saving gesture from the user's desk.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_long_project_headers_keep_controls_inside(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let common = fixture.common.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    root.update(cx, |r, cx| {
        let project = r.active_project.unwrap();
        let session = r.desk.add_session(guild_session::SessionRecord::new(
            "Waiting fixture".into(),
            guild_session::Seat { workspace, project },
            fixture.main.clone(),
            guild_session::Runs::Codex(guild_session::SessionMode::Terminal),
        ));
        r.board.report(
            session,
            Some(guild_session::status::Reported::Blocked(Some(
                guild_session::status::Kind::Permission,
            ))),
        );
        cx.notify();
    });
    let header_key = Box::leak(format!("project-header-{}", common.display()).into_boxed_str());
    for name in [
        "guild-workflow-harness-pilot-20261017",
        "guild",
        "оченьдлинноеимяпроектабезпробеловдляпроверкиграниц",
    ] {
        for width in [180., 232., 420.] {
            root.update(cx, |r, cx| {
                r.settings.column_width = width;
                r.desk.rename_repository(&common, name);
                cx.notify();
            });
            cx.run_until_parked();
            for folded in [true, false] {
                root.update(cx, |r, cx| {
                    if r.desk.is_folded(&common) != folded {
                        r.toggle_group_fold(&common, cx);
                    }
                });
                cx.run_until_parked();
                let head = cx.debug_bounds(header_key).unwrap();
                for selector in [
                    "project-header-tile",
                    "project-header-label",
                    "project-header-chevron",
                    "project-header-menu",
                    "project-header-new",
                ] {
                    let b = cx.debug_bounds(selector).unwrap();
                    assert!(
                        b.left() >= head.left() && b.right() <= head.right(),
                        "{name}, width={width}, folded={folded}, {selector}: {b:?} outside {head:?}"
                    );
                    assert!(
                        b.top() >= head.top() && b.bottom() <= head.bottom(),
                        "single-line header: {selector}"
                    );
                }
                if folded {
                    let activity = cx.debug_bounds("ui/activity-question.svg").unwrap();
                    assert!(activity.left() >= head.left() && activity.right() <= head.right());
                    assert_eq!(activity.size, gpui::size(px(16.), px(16.)));
                }
                let menu = cx.debug_bounds("project-header-menu").unwrap();
                let new = cx.debug_bounds("project-header-new").unwrap();
                assert_eq!(new.right(), head.right() - px(10.));
                assert_eq!(menu.right(), new.left() - px(8.));
                assert_eq!(menu.size, gpui::size(px(18.), px(18.)));
                assert_eq!(new.size, gpui::size(px(18.), px(18.)));
                cx.simulate_click(menu.center(), Default::default());
                cx.run_until_parked();
                root.read_with(cx, |r, _| {
                    assert!(matches!(r.context_menu.as_ref().map(|m| &m.target), Some(crate::view::menu::Target::Repository {key, ..}) if key == &common));
                    assert_eq!(r.desk.is_folded(&common), folded);
                });
                cx.simulate_keystrokes("escape");
                cx.run_until_parked();
                cx.simulate_click(new.center(), Default::default());
                cx.run_until_parked();
                root.read_with(cx, |r, _| {
                    assert_eq!(
                        r.new_worktree.as_ref().unwrap().project,
                        r.active_project.unwrap()
                    );
                    assert_eq!(r.desk.is_folded(&common), folded);
                });
                cx.simulate_keystrokes("escape");
                cx.run_until_parked();
                cx.update(|window, cx| {
                    root.update(cx, |r, cx| {
                        r.start_renaming_project(common.clone(), name.into(), window, cx)
                    })
                });
                cx.run_until_parked();
                let field = cx.debug_bounds("project-header-rename").unwrap();
                let menu = cx.debug_bounds("project-header-menu").unwrap();
                assert!(
                    field.size.width > px(0.)
                        && field.left() >= head.left()
                        && field.right() <= menu.left(),
                    "bounded rename at {width}, folded={folded}: {field:?}"
                );
                let chevron = cx.debug_bounds("project-header-chevron").unwrap();
                let activity_space = if folded {
                    cx.debug_bounds("ui/activity-question.svg")
                        .unwrap()
                        .size
                        .width
                        + px(8.)
                } else {
                    px(0.)
                };
                let available =
                    menu.left() - field.left() - chevron.size.width - px(16.) - activity_space;
                assert_eq!(
                    field.size.width, available,
                    "rename uses all remaining width at {width}, folded={folded}"
                );
                assert!(cx.debug_bounds("project-header-new").unwrap().right() <= head.right());
                cx.simulate_keystrokes("cmd-a");
                cx.simulate_input("cancelled-name");
                cx.simulate_keystrokes("escape");
                cx.run_until_parked();
                root.read_with(cx, |r, _| {
                    assert!(r.renaming_project().is_none());
                    assert_eq!(r.column_groups_for(workspace)[0].name, name);
                });
                cx.update(|window, cx| {
                    root.update(cx, |r, cx| {
                        r.desk.rename_repository(&common, "previous");
                        r.save();
                        assert_ne!(r.desk.repository_name(&common), Some(name));
                        r.start_renaming_project(common.clone(), "previous".into(), window, cx)
                    })
                });
                cx.run_until_parked();
                cx.simulate_keystrokes("cmd-a");
                cx.simulate_input(name);
                cx.simulate_keystrokes("enter");
                cx.run_until_parked();
                guild_session::store::flush();
                let saved = guild_session::persist::load(&crate::paths::workspace_file());
                assert_eq!(
                    saved.repository_name(&common),
                    Some(name),
                    "accepted full name survives reload"
                );
                assert_eq!(
                    root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].name.clone()),
                    name
                );
                // Clicking the tile folds exactly once, independently of the controls.
                let tile = cx.debug_bounds("project-header-tile").unwrap();
                cx.simulate_click(tile.center(), Default::default());
                cx.run_until_parked();
                assert_eq!(
                    root.read_with(cx, |r, _| r.desk.is_folded(&common)),
                    !folded
                );
            }
        }
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_drag_stationary_edges_and_drop(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let paths: Vec<_> = (0..24)
        .map(|i| fixture.dir.path().join(format!("project-{i:02}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    root.update(cx, |r, cx| {
        for path in &paths {
            r.desk.open_in(r.active_workspace, path);
        }
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.focus(&root.read(cx).focus.clone(), cx);
    });
    let before = root.read_with(cx, |r, _| {
        r.column_groups_for(r.active_workspace)
            .iter()
            .map(|g| g.key.clone())
            .collect::<Vec<_>>()
    });
    let key = Box::leak(format!("project-header-{}", paths[0].display()).into_boxed_str());
    let source = cx.debug_bounds(key).unwrap();
    let controls = cx.debug_bounds("workspace-column-controls").unwrap();
    let bottom = gpui::point(source.center().x, controls.center().y);
    let focus = cx.update(|window, cx| window.focused(cx));
    let active = root.read_with(cx, |r, _| r.active_project);
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| cx.has_active_drag()),
        "real project header starts a drag"
    );
    let initial = cx.debug_bounds(key).unwrap().top();
    project_ticks(cx, 8);
    let scrolled = cx.debug_bounds(key).unwrap().top();
    assert!(
        scrolled < initial - px(40.),
        "stationary project drag must scroll the cached production column: {initial:?} -> {scrolled:?}"
    );
    // Keep this single gesture down to the last project, then drop on a newly revealed header.
    project_ticks(cx, 100);
    let last_key =
        Box::leak(format!("project-header-{}", paths.last().unwrap().display()).into_boxed_str());
    let last = cx.debug_bounds(last_key).unwrap();
    assert!(last.center().y < controls.top());
    // Let go on the line after it: the bottom edge of its block.
    let last_block = cx
        .debug_bounds(Box::leak(
            format!("project-block-{}", paths.last().unwrap().display()).into_boxed_str(),
        ))
        .unwrap();
    let after_last = gpui::point(last.center().x, last_block.bottom() - px(2.));
    cx.simulate_mouse_move(after_last, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(after_last, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let after = root.read_with(cx, |r, _| {
        r.column_groups_for(r.active_workspace)
            .iter()
            .map(|g| g.key.clone())
            .collect::<Vec<_>>()
    });
    let mut expected = before.clone();
    let moved = expected.remove(1);
    expected.push(moved);
    assert_eq!(after, expected);
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
    assert_eq!(root.read_with(cx, |r, _| r.active_project), active);
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    let workspace = saved.active_workspace();
    let saved_paths: Vec<_> = saved
        .projects_of(workspace)
        .map(|p| p.path.clone())
        .collect();
    let actual_paths = root.read_with(cx, |r, _| {
        r.desk
            .projects_of(workspace)
            .map(|p| p.path.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(saved_paths, actual_paths);
    // Reverse direction in one new gesture, holding over the workspace header.
    let source = cx.debug_bounds(key).unwrap();
    let top = gpui::point(source.center().x, px(65.));
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(top, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let initial = cx.debug_bounds(key).unwrap().top();
    project_ticks(cx, 8);
    assert!(
        cx.debug_bounds(key).unwrap().top() > initial + px(40.),
        "stationary top hold scrolls upward"
    );
    project_ticks(cx, 100);
    let target_key =
        Box::leak(format!("project-header-{}", fixture.common.display()).into_boxed_str());
    // The line before the first project: the top edge of its head.
    let first = cx.debug_bounds(target_key).unwrap();
    let target = gpui::point(first.center().x, first.top() + px(2.));
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let mut expected = after.clone();
    let moved = expected.pop().unwrap();
    expected.insert(0, moved);
    assert_eq!(
        root.read_with(cx, |r, _| r
            .column_groups_for(workspace)
            .iter()
            .map(|g| g.key.clone())
            .collect::<Vec<_>>()),
        expected
    );
    guild_session::store::flush();
    let reloaded = guild_session::persist::load(&crate::paths::workspace_file());
    assert_eq!(
        reloaded.workspace(workspace).unwrap().projects,
        root.read_with(cx, |r, _| r
            .desk
            .workspace(workspace)
            .unwrap()
            .projects
            .clone())
    );
}

fn project_ticks(cx: &mut gpui::VisualTestContext, count: usize) {
    // Preserve each existing 80 ms observation interval while allowing the
    // cached view to render at realistic frame granularity.
    for _ in 0..count {
        for _ in 0..5 {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(16));
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            cx.run_until_parked();
        }
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_drag_pauses_resumes_and_terminates(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    let paths: Vec<_> = (0..16)
        .map(|i| fixture.dir.path().join(format!("long-{i:02}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    root.update(cx, |r, cx| {
        for path in &paths {
            r.desk.open_in(workspace, path);
        }
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    cx.run_until_parked();
    let key = Box::leak(format!("project-header-{}", paths[0].display()).into_boxed_str());
    let scroll = root.read_with(cx, |r, _| r.project_drag.scroll(workspace));
    let controls = cx.debug_bounds("workspace-column-controls").unwrap();
    let footer = cx.debug_bounds("workspace-column-actions").unwrap();
    let bottom = gpui::point(px(50.), footer.top() + px(2.));
    let outside = gpui::point(px(400.), bottom.y);
    let source = cx.debug_bounds(key).unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    project_ticks(cx, 4);
    assert!(
        scroll.offset().y < px(0.),
        "actions footer activates scrolling"
    );
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let paused = scroll.offset();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), paused, "horizontal departure pauses");
    assert!(root.read_with(cx, |r, _| r.project_drag.active.is_some()));
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    project_ticks(cx, 4);
    assert!(
        scroll.offset().y < paused.y,
        "return resumes the same held drag"
    );
    project_ticks(cx, 100);
    let limit = scroll.offset();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), limit, "bottom limit is bounded");
    assert_eq!(limit.y, -scroll.max_offset().y);
    assert!(
        root.read_with(cx, |r, _| r.project_drag.active.is_some()),
        "a limit retains ownership"
    );
    cx.simulate_mouse_move(
        gpui::point(px(50.), px(65.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    project_ticks(cx, 4);
    assert!(
        scroll.offset().y > limit.y,
        "reversal at limit resumes upward"
    );
    cx.simulate_mouse_move(
        gpui::point(px(50.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let paused = scroll.offset();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), paused, "vertical departure pauses");
    // Returning with no button models a lost outside-window release.
    cx.simulate_event(gpui::MouseMoveEvent {
        position: controls.center(),
        ..Default::default()
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    assert!(root.read_with(cx, |r, _| r.project_drag.active.is_none()));
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), paused);

    // The same production gesture must terminate when the list disappears or workspace changes.
    for hide in [true, false] {
        root.update(cx, |r, cx| {
            scroll.set_offset(Default::default());
            r.settings.column_folded = false;
            r.column_peeking = false;
            cx.notify();
        });
        cx.run_until_parked();
        let source = cx.debug_bounds(key).unwrap().center();
        cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        assert!(root.read_with(cx, |r, _| r.project_drag.active.is_some()));
        root.update(cx, |r, cx| {
            if hide {
                r.settings.column_folded = true;
                r.column_peeking = false;
            } else {
                let next = r.desk.add_workspace("Other", None).unwrap();
                r.select_workspace(next, cx);
            }
            cx.notify();
        });
        project_ticks(cx, 2);
        assert!(root.read_with(cx, |r, _| r.project_drag.active.is_none()));
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
        let offset = scroll.offset();
        project_ticks(cx, 4);
        assert_eq!(
            scroll.offset(),
            offset,
            "stale callback cannot scroll hidden page"
        );
        cx.simulate_mouse_up(bottom, gpui::MouseButton::Left, Default::default());
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_drag_short_peeking_and_panel_escape(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    let key = Box::leak(format!("project-header-{}", fixture.common.display()).into_boxed_str());
    let scroll = root.read_with(cx, |r, _| r.project_drag.scroll(workspace));
    let source = cx.debug_bounds(key).unwrap().center();
    let bottom = cx
        .debug_bounds("workspace-column-controls")
        .unwrap()
        .center();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.focus, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let focus = cx.update(|window, cx| window.focused(cx));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset().y, px(0.), "short list cannot overscroll");
    cx.simulate_keystrokes("escape");
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
    cx.simulate_mouse_up(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    // Populate the same production list, then use its deferred peeking surface.
    let paths: Vec<_> = (0..16)
        .map(|i| fixture.dir.path().join(format!("peek-{i:02}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    root.update(cx, |r, cx| {
        for path in &paths {
            r.desk.open_in(workspace, path);
        }
        r.settings.column_folded = true;
        r.column_peeking = true;
        cx.notify();
    });
    cx.run_until_parked();
    let source = cx.debug_bounds(key).unwrap().center();
    let footer = cx.debug_bounds("workspace-column-actions").unwrap();
    let height = cx.update(|window, _| window.viewport_size().height);
    let bottom = gpui::point(px(50.), footer.top().max(px(0.)) + px(2.));
    assert!(bottom.y < height, "actual peeking footer is visible");
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    project_ticks(cx, 4);
    assert!(
        scroll.offset().y < px(0.),
        "peeking actions footer activates scrolling"
    );
    cx.simulate_mouse_up(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let after = scroll.offset();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), after, "release terminates the timer");
    assert!(root.read_with(cx, |r, _| r.project_drag.active.is_none()));
    // A worktree drag at the same edge cannot acquire the project clock.
    root.update(cx, |r, cx| {
        scroll.set_offset(Default::default());
        cx.notify();
        assert!(r.project_drag.active.is_none());
    });
    cx.run_until_parked();
    let rows = root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows.clone());
    let row = cx
        .debug_bounds(Box::leak(row_key(workspace, &rows[2]).into_boxed_str()))
        .unwrap();
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    project_ticks(cx, 4);
    assert_eq!(
        scroll.offset().y,
        px(0.),
        "unrelated worktree drag does not scroll projects"
    );
    assert!(root.read_with(cx, |r, _| r.project_drag.active.is_none()));
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
}

/// Isolate the production header from the pre-existing full-window rename
/// dismissal overlay. No Root frame is rendered, so its viewport stays empty;
/// render_group still supplies the real header capture and the real TextField.
struct RenameHeader {
    root: gpui::Entity<Root>,
    _changed: gpui::Subscription,
}
impl Render for RenameHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            let workspace = root.active_workspace;
            let group = root.column_groups_for(workspace).remove(0);
            div()
                .w(px(420.))
                .child(root.render_group(workspace, &group, None, false, false, cx))
        })
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_rename_header_preserves_mouse_caret_and_selection(cx: &mut TestAppContext) {
    assert_eq!(
        std::env::var("GUILD_WORKTREE_ORDER_TEST").as_deref(),
        Ok("1")
    );
    assert!(
        crate::paths::workspace_file()
            .starts_with(std::env::var_os("GUILD_STATE_DIR").expect("isolated child state"))
    );
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
        root.update(cx, |root, cx| {
            root.start_renaming_project(dir.path().into(), "alpha beta".into(), window, cx)
        });
        made = Some(root.clone());
        let header = cx.new(|cx| RenameHeader {
            _changed: cx.observe(&root, |_, _, cx| cx.notify()),
            root,
        });
        gpui_component::Root::new(header, window, cx)
    });
    let root = made.unwrap();
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.viewport()),
        Default::default(),
        "full-window overlay is outside this focused fixture"
    );
    let field = cx.debug_bounds("project-header-rename").unwrap();
    let end = gpui::point(field.right() - px(12.), field.center().y);
    cx.simulate_click(end, Default::default());
    cx.simulate_input("!");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.renaming_project().unwrap().1.to_owned()),
        "alpha beta!",
        "a real field click must collapse the initial select-all before typing"
    );
    let start = gpui::point(field.left() + px(6.), field.center().y);
    let shift = gpui::Modifiers {
        shift: true,
        ..Default::default()
    };
    cx.simulate_click(start, shift);
    cx.simulate_input("Q");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r.renaming_project().unwrap().1.to_owned()),
        "Q",
        "Shift-click must extend the caret selection before replacement input"
    );
    guild_session::store::flush();
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_drag_moves_smoothly_at_frame_scale(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    root.update(cx, |r, cx| {
        for i in 0..24 {
            let path = fixture.dir.path().join(format!("motion-{i:02}"));
            std::fs::create_dir_all(&path).unwrap();
            r.desk.open_in(r.active_workspace, &path);
        }
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    cx.run_until_parked();
    let source = cx
        .debug_bounds(Box::leak(
            format!("project-header-{}", fixture.common.display()).into_boxed_str(),
        ))
        .unwrap()
        .center();
    let bottom = cx
        .debug_bounds("workspace-column-controls")
        .unwrap()
        .center();
    let scroll = root.read_with(cx, |r, _| r.project_drag.scroll(r.active_workspace));
    let row =
        cx.update(|window, _| px(crate::theme::space::TREE_ROW.0 * f32::from(window.rem_size())));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    // Observe the real cached column at roughly 120 Hz, independently of its
    // internal timer. Half a second should travel about ten row heights.
    let frame = std::time::Duration::from_micros(8333);
    let mut first_motion = None;
    let mut largest_jump = px(0.);
    let mut previous = scroll.offset().y;
    for index in 1..=60 {
        cx.executor().advance_clock(frame);
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        let position = scroll.offset().y;
        let jump = (position - previous).abs();
        if jump > px(0.) {
            first_motion.get_or_insert(frame * index);
        }
        largest_jump = largest_jump.max(jump);
        previous = position;
    }
    let distance = -scroll.offset().y;
    let old_nominal_distance = row * (frame * 60).as_secs_f32() / 0.060;
    println!(
        "production project motion: first={first_motion:?}, largest_jump={largest_jump:?}, distance={distance:?}, old_nominal_distance={old_nominal_distance:?}"
    );
    assert!(
        first_motion.is_some_and(|elapsed| elapsed <= std::time::Duration::from_millis(34)),
        "held project must start moving within frame-scale latency"
    );
    assert!(
        largest_jump <= row * 0.4,
        "normal updates must be smaller than a row: {largest_jump:?}"
    );
    assert!(
        distance >= old_nominal_distance * 1.12 && distance <= old_nominal_distance * 1.28,
        "motion should be modestly faster than the old nominal speed: {distance:?}"
    );
    // A delayed callback must not repay a long scheduler stall in one jump.
    let before_stall = scroll.offset().y;
    // The pinned scheduler's advance_clock drains intermediate timers. Advance
    // its raw clock instead to delay the actual production callback by two seconds.
    cx.executor()
        .dispatcher()
        .as_test()
        .unwrap()
        .scheduler()
        .clock()
        .advance(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    let catch_up = (scroll.offset().y - before_stall).abs();
    assert!(
        catch_up > px(0.) && catch_up <= row * 0.7,
        "stall catch-up is bounded: {catch_up:?}"
    );
    cx.simulate_mouse_move(
        gpui::point(px(400.), bottom.y),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let paused = scroll.offset();
    project_ticks(cx, 10);
    assert_eq!(scroll.offset(), paused);
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    for _ in 0..2 {
        cx.executor().advance_clock(frame);
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    let resumed = (scroll.offset().y - paused.y).abs();
    assert!(
        resumed > px(0.) && resumed <= row * 0.4,
        "returning must not repay paused time: {resumed:?}"
    );
    println!("production project motion: stall_catch_up={catch_up:?}, resumed_step={resumed:?}");
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_drag_waits_for_presentation_frame(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    root.update(cx, |r, cx| {
        for i in 0..24 {
            let path = fixture.dir.path().join(format!("motion-{i:02}"));
            std::fs::create_dir_all(&path).unwrap();
            r.desk.open_in(r.active_workspace, &path);
        }
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    cx.run_until_parked();
    let source = cx
        .debug_bounds(Box::leak(
            format!("project-header-{}", fixture.common.display()).into_boxed_str(),
        ))
        .unwrap()
        .center();
    let bottom = cx
        .debug_bounds("workspace-column-controls")
        .unwrap()
        .center();
    let scroll = root.read_with(cx, |r, _| r.project_drag.scroll(r.active_workspace));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(bottom, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    let before = scroll.offset();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(48));
    cx.run_until_parked();
    assert_eq!(
        scroll.offset(),
        before,
        "project motion must wait for a presentation frame"
    );
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    assert!(scroll.offset().y < before.y);
    cx.simulate_keystrokes("escape");
    let stopped = scroll.offset();
    project_ticks(cx, 4);
    assert_eq!(scroll.offset(), stopped);
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_hover_copy_pin_and_unpin_preserve_selection(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let path = fixture.a.clone();
    let cx = &mut *fixture.cx;
    let (workspace, row, active) = root.read_with(cx, |r, _| {
        (
            r.active_workspace,
            r.column_groups_for(r.active_workspace)[0].rows[1].clone(),
            r.active_project,
        )
    });
    cx.update(|window, cx| {
        let focus = root.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    let prior_focus = cx.update(|window, cx| window.focused(cx));
    let selector = Box::leak(row_key(workspace, &row).into_boxed_str());
    let bounds = cx.debug_bounds(selector).unwrap();
    let copy = gpui::point(bounds.right() - px(56.), bounds.top() + px(16.));
    cx.simulate_mouse_move(copy, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let copy = cx
        .debug_bounds(Box::leak(
            format!("worktree-copy-{selector}").into_boxed_str(),
        ))
        .expect("hover copy is drawn")
        .center();
    cx.simulate_click(copy, Default::default());
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(path.display().to_string())
    );
    let pin = cx
        .debug_bounds(Box::leak(
            format!("worktree-pin-{selector}").into_boxed_str(),
        ))
        .expect("hover pin is drawn")
        .center();
    cx.simulate_click(pin, Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| {
        assert!(r.worktree_pinned(workspace, &path));
        assert_eq!(r.active_project, active);
        assert_eq!(r.desk.projects().count(), 1);
    });
    let pinned_bounds = cx.debug_bounds(selector).unwrap();
    assert!(
        pinned_bounds.top() < bounds.top(),
        "pin moves the row above Projects"
    );
    let unpin = gpui::point(
        pinned_bounds.right() - px(38.),
        pinned_bounds.top() + px(16.),
    );
    cx.simulate_mouse_move(unpin, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let unpin = cx
        .debug_bounds(Box::leak(
            format!("worktree-pin-{selector}").into_boxed_str(),
        ))
        .unwrap()
        .center();
    cx.simulate_click(unpin, Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| assert!(!r.worktree_pinned(workspace, &path)));
    assert_eq!(cx.debug_bounds(selector).unwrap().top(), bounds.top());
    root.update(cx, |r, cx| {
        r.toggle_worktree_pin(workspace, &fixture.main, cx)
    });
    assert!(!root.read_with(cx, |r, _| r.worktree_pinned(workspace, &fixture.main)));
    for branch in ["main", "master"] {
        root.update(cx, |r, cx| {
            r.worktrees.get_mut(&fixture.common).unwrap()[1].branch = Some(branch.into());
            r.toggle_worktree_pin(workspace, &path, cx);
        });
        assert!(!root.read_with(cx, |r, _| r.worktree_pinned(workspace, &path)));
    }
    root.update(cx, |r, cx| {
        r.worktrees.get_mut(&fixture.common).unwrap()[1].branch = Some("a".into());
        cx.notify();
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_mouse_move(bounds.center(), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let delete = cx
        .debug_bounds(Box::leak(
            format!("worktree-delete-{selector}").into_boxed_str(),
        ))
        .unwrap()
        .center();
    cx.simulate_click(delete, Default::default());
    cx.run_until_parked();
    assert!(
        root.read_with(cx, |r, _| r
            .worktree_removal
            .as_ref()
            .unwrap()
            .delete_branch),
        "reference checkbox defaults on"
    );
    let card = cx.debug_bounds("worktree-removal-card").unwrap();
    assert_eq!(card.size.width, px(400.));
    assert_eq!(card.size.height, px(188.));
    assert_eq!(card.center(), gpui::point(px(500.), px(350.)));
    let checkbox = cx.debug_bounds("worktree-removal-checkbox").unwrap();
    cx.simulate_click(checkbox.center(), Default::default());
    cx.run_until_parked();
    assert!(!root.read_with(cx, |r, _| {
        r.worktree_removal.as_ref().unwrap().delete_branch
    }));
    let cancel = cx.debug_bounds("worktree-removal-cancel").unwrap();
    cx.simulate_click(cancel.center(), Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| r.worktree_removal.is_none()));
    assert!(path.is_dir(), "Cancel leaves the checkout in place");
    assert_eq!(root.read_with(cx, |r, _| r.active_project), active);
    assert_eq!(cx.update(|window, cx| window.focused(cx)), prior_focus);
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_removal_modal_protects_branches_traps_keys_and_rechecks_locks(
    cx: &mut TestAppContext,
) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    for font in [10., 13., 16., 20.] {
        for branch in [Some("main"), Some("master"), None] {
            root.update(cx, |root, _| {
                root.settings.ui_font_size = font;
                root.worktrees.get_mut(&fixture.common).unwrap()[1].branch =
                    branch.map(str::to_owned);
            });
            cx.update(|window, cx| {
                root.update(cx, |root, cx| {
                    root.remove_worktree_asking(fixture.a.clone(), None, false, window, cx)
                })
            });
            cx.run_until_parked();
            let card = cx.debug_bounds("worktree-removal-card").unwrap();
            assert!(
                (f32::from(card.size.width) - 400. * font / 13.).abs() < 0.51,
                "font={font}, card={card:?}, configured={}",
                root.read_with(cx, |r, _| r.settings.ui_font_size)
            );
            let viewport = cx.update(|window, _| window.viewport_size());
            assert!(
                (f32::from(card.center().x - viewport.width / 2.)).abs() < 0.51
                    && (f32::from(card.center().y - viewport.height / 2.)).abs() < 0.51,
                "card={card:?}, viewport={viewport:?}"
            );
            eprintln!("modal ui-font={font}, branch={branch:?}: {card:?}");
            assert!(!root.read_with(cx, |root, _| {
                root.worktree_removal.as_ref().unwrap().delete_branch
            }));
            let toggle = cx.debug_bounds("worktree-removal-checkbox").unwrap();
            cx.simulate_click(toggle.center(), Default::default());
            cx.run_until_parked();
            assert!(!root.read_with(cx, |root, _| {
                root.worktree_removal.as_ref().unwrap().delete_branch
            }));
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            assert!(root.read_with(cx, |root, _| root.worktree_removal.is_none()));
        }
    }
    for head in [
        None,
        Some("0000000000000000000000000000000000000000".to_owned()),
    ] {
        root.update(cx, |root, _| {
            let row = &mut root.worktrees.get_mut(&fixture.common).unwrap()[1];
            row.branch = Some("feature".into());
            row.head = head;
        });
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.remove_worktree_asking(fixture.a.clone(), None, false, window, cx)
            })
        });
        cx.run_until_parked();
        assert!(
            !root.read_with(cx, |root, _| root
                .worktree_removal
                .as_ref()
                .unwrap()
                .delete_branch),
            "an unavailable/unborn commit disables branch deletion"
        );
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    root.update(cx, |root, _| {
        root.settings.ui_font_size = 13.;
        let row = &mut root.worktrees.get_mut(&fixture.common).unwrap()[1];
        row.branch = Some("feature".into());
        row.head = Some("0123456789012345678901234567890123456789".into());
    });
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.remove_worktree_asking(fixture.a.clone(), None, false, window, cx)
        })
    });
    cx.run_until_parked();
    // Initial keyboard focus is Cancel. Shift+Tab reaches the checkbox,
    // Space toggles it, and an outside click cannot select the project behind.
    let folded = root.read_with(cx, |root, _| root.settings.column_folded);
    cx.simulate_keystrokes("cmd-shift-b");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |root, _| root.settings.column_folded),
        folded,
        "modal blocks bound background actions too"
    );
    cx.simulate_keystrokes("shift-tab space");
    cx.run_until_parked();
    assert!(!root.read_with(cx, |root, _| {
        root.worktree_removal.as_ref().unwrap().delete_branch
    }));
    let selected = root.read_with(cx, |root, _| root.active_project);
    cx.simulate_click(gpui::point(px(30.), px(150.)), Default::default());
    cx.run_until_parked();
    assert_eq!(root.read_with(cx, |root, _| root.active_project), selected);
    root.update(cx, |root, _| {
        root.worktrees.get_mut(&fixture.common).unwrap()[1].locked = Some("another owner".into())
    });
    let delete = cx.debug_bounds("worktree-removal-delete").unwrap();
    cx.simulate_click(delete.center(), Default::default());
    cx.run_until_parked();
    assert!(
        root.read_with(cx, |root, _| root.worktree_removal.is_none()),
        "new lock vetoes confirmation before the absent fixture worker can be asked"
    );
    assert!(fixture.a.is_dir());
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_removal_actual_delete_keep_and_partial_failure_reach_the_git_worker(
    cx: &mut TestAppContext,
) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let git = |dir: &std::path::Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    git(&fixture.main, &["init", "-b", "main"]);
    git(&fixture.main, &["commit", "--allow-empty", "-m", "base"]);
    for (index, (delete_branch, unmerged)) in [(true, false), (false, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        let name = format!("modal-feature-{index}");
        let linked = fixture.dir.path().join(&name);
        git(
            &fixture.main,
            &["worktree", "add", "-b", &name, linked.to_str().unwrap()],
        );
        if unmerged {
            git(&linked, &["commit", "--allow-empty", "-m", "unmerged work"]);
        }
        let target = crate::git::worktree_removal::Target::read(&fixture.main, &linked).unwrap();
        let project = root.read_with(cx, |r, _| r.active_project.unwrap());
        if index == 0 {
            root.update(cx, |r, _| {
                r.status_workers.insert(
                    project,
                    crate::root::git::StatusWorker::start(fixture.main.clone()),
                );
            });
        }
        let (cli_send, cli_receive) = std::sync::mpsc::channel();
        root.update(cx, |r, _| {
            r.pending_writes.push((
                crate::git::Write::WorktreeRemove(target.path.clone()),
                cli_send,
            ));
        });
        cx.update(|window, cx| {
            root.update(cx, |r, cx| r.show_worktree_removal(target, window, cx))
        });
        cx.run_until_parked();
        if !delete_branch {
            let checkbox = cx
                .debug_bounds("worktree-removal-checkbox")
                .unwrap()
                .center();
            cx.simulate_click(checkbox, Default::default());
            cx.run_until_parked();
        }
        let before = root.read_with(cx, |r, _| r.consoles.get(&project).map_or(0, |c| c.len()));
        let delete = cx.debug_bounds("worktree-removal-delete").unwrap().center();
        cx.simulate_click(delete, Default::default());
        cx.run_until_parked();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            root.update(cx, |r, cx| r.poll_git(cx));
            cx.run_until_parked();
            if root.read_with(cx, |r, _| {
                r.consoles.get(&project).is_some_and(|c| c.len() > before)
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "GUI removal did not finish on its actual git worker"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            cli_receive.try_recv().is_err(),
            "GUI completion cannot settle the CLI's separate branch-preserving ask"
        );
        root.update(cx, |r, _| {
            r.pending_writes.clear();
        });
        assert!(!linked.exists());
        let branches = git(&fixture.main, &["branch", "--format=%(refname:short)"]);
        assert_eq!(
            branches.lines().any(|b| b == name),
            !delete_branch || unmerged
        );
        root.read_with(cx, |r, _| {
            let line = r.consoles[&project].lines().last().unwrap();
            assert_eq!(line.failed, unmerged, "{}", line.said);
            if unmerged {
                assert!(
                    line.said
                        .contains("Worktree removed; branch deletion was not confirmed"),
                    "{}",
                    line.said
                );
            }
        });
    }
    root.update(cx, |r, _| r.status_workers.clear());
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_review_pins_keep_repository_identity_and_menus(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    root.update(cx, |r, cx| {
        let project = r.desk.open_in(workspace, &fixture.a);
        r.desk.set_repository(project, Some(fixture.common.clone()));
        r.desk.toggle_worktree_pin(workspace, &fixture.a);
        cx.notify();
    });
    cx.run_until_parked();
    let menu = cx.debug_bounds("project-header-menu").unwrap();
    cx.simulate_click(menu.center(), Default::default());
    cx.run_until_parked();
    root.read_with(cx, |r, _| {
        let Some(crate::view::menu::Target::Repository { projects, .. }) =
            r.context_menu.as_ref().map(|m| &m.target)
        else {
            panic!("repository menu")
        };
        assert_eq!(
            projects.len(),
            2,
            "Move to Workspace includes the pinned opened root"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let rows = root.read_with(cx, |r, _| r.column_groups_for(workspace)[0].rows.clone());
    for row in &rows[..2] {
        let bounds = cx
            .debug_bounds(Box::leak(row_key(workspace, row).into_boxed_str()))
            .unwrap();
        cx.simulate_mouse_down(
            bounds.center(),
            gpui::MouseButton::Right,
            Default::default(),
        );
        cx.simulate_mouse_up(
            bounds.center(),
            gpui::MouseButton::Right,
            Default::default(),
        );
        cx.run_until_parked();
        root.read_with(cx, |r, _| {
            assert!(r.context_menu.is_some(), "both pinned and ordinary rows open menus");
            let expected = r.column_row_path(row).unwrap();
            assert!(matches!(r.context_menu.as_ref().map(|m| &m.target), Some(crate::view::menu::Target::Worktree {path, ..}) if path == &expected), "row target retained");
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    root.update(cx, |r, cx| {
        let primary = r
            .desk
            .projects_of(workspace)
            .find(|p| p.path == fixture.main)
            .unwrap()
            .id;
        r.desk.remove_project(primary);
        cx.notify();
    });
    cx.run_until_parked();
    let new = cx
        .debug_bounds("project-header-new")
        .expect("sole opened root is pinned; New Worktree remains available");
    cx.simulate_click(new.center(), Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| r.new_worktree.is_some()));
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_review_modal_cancels_workspace_landing(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let original = root.read_with(cx, |r, _| r.active_workspace);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.desk.add_workspace("Other", None).unwrap();
            r.apply_workspace_motion(
                crate::swipe::Motion::Land {
                    to: Some(crate::swipe::Step::Next),
                    from: -100.,
                },
                cx,
            );
            r.remove_worktree_asking(fixture.a.clone(), None, false, window, cx);
            assert!(r.worktree_removal.is_some());
            assert_eq!(r.workspace_swipe_offset, 0.);
        })
    });
    cx.run_until_parked();
    project_ticks(cx, 16);
    assert_eq!(
        root.read_with(cx, |r, _| r.active_workspace),
        original,
        "opening the modal cancels the pending landing before any wheel event"
    );
    let position = cx.debug_bounds("project-header-menu").unwrap().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-200.), px(0.))),
        touch_phase: gpui::TouchPhase::Started,
        modifiers: Default::default(),
    });
    project_ticks(cx, 16);
    root.read_with(cx, |r, _| {
        assert_eq!(
            r.active_workspace, original,
            "pending landing cannot change the modal background"
        );
        assert_eq!(
            r.workspace_swipe_offset, 0.,
            "capture-phase wheel cannot move the modal background"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-100.), px(0.))),
        touch_phase: gpui::TouchPhase::Started,
        modifiers: Default::default(),
    });
    cx.run_until_parked();
    assert_ne!(
        root.read_with(cx, |r, _| r.workspace_swipe_offset),
        0.,
        "capture-phase swipe resumes after Cancel"
    );
    root.update(cx, |r, cx| {
        assert!(r.worktree_removal.is_none());
        r.apply_workspace_motion(
            crate::swipe::Motion::Land {
                to: Some(crate::swipe::Step::Next),
                from: -100.,
            },
            cx,
        );
    });
    project_ticks(cx, 16);
    assert_ne!(
        root.read_with(cx, |r, _| r.active_workspace),
        original,
        "swipe landing works after Cancel"
    );
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_visual_enabled_unopened_row(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    root.update(fixture.cx, |r, cx| {
        let groups = r.column_groups_for(r.active_workspace);
        let mut opened = r.render_row(r.active_workspace, &groups[0].rows[0], cx);
        let mut unopened = r.render_row(r.active_workspace, &groups[0].rows[1], cx);
        assert_eq!(
            unopened.style().opacity,
            opened.style().opacity,
            "unopened roots and their enabled actions must not look disabled"
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_visual_first_hover_tooltip_is_anchored(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let (workspace, row) = root.read_with(cx, |r, _| {
        (
            r.active_workspace,
            r.column_groups_for(r.active_workspace)[0].rows[1].clone(),
        )
    });
    let selector = row_key(workspace, &row);
    let bounds = cx
        .debug_bounds(Box::leak(selector.clone().into_boxed_str()))
        .unwrap();
    // One move directly from outside into the hidden control; do not give it
    // another MouseMove to start the timer after the reveal frame.
    let target = gpui::point(bounds.right() - px(20.), bounds.top() + px(16.));
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("tooltip-popup").is_none(),
        "tooltip has an initial delay"
    );
    project_ticks(cx, 3);
    assert!(
        cx.debug_bounds("tooltip-popup").is_none(),
        "brief hover does not flash a hint"
    );
    project_ticks(cx, 5);
    let tip = cx
        .debug_bounds("tooltip-popup")
        .expect("stationary first entry must show its tooltip");
    let button = cx
        .debug_bounds(Box::leak(
            format!("worktree-delete-{selector}").into_boxed_str(),
        ))
        .unwrap();
    assert!(
        tip.bottom() <= button.top() + px(0.51),
        "tooltip is above its button, away from pointer: tip={tip:?}, button={button:?}"
    );
    eprintln!("anchored tooltip: {tip:?}, Delete target: {button:?}");
    for action in ["copy", "pin", "delete"] {
        let key = format!("worktree-{action}-{selector}");
        let target = cx
            .debug_bounds(Box::leak(key.clone().into_boxed_str()))
            .unwrap();
        let glyph = cx
            .debug_bounds(Box::leak(format!("{key}-glyph").into_boxed_str()))
            .unwrap();
        assert!(
            glyph.size.width < target.size.width && glyph.size.height < target.size.height,
            "glyph leaves visible air inside the hit target"
        );
        assert!((f32::from(glyph.center().x - target.center().x)).abs() < 0.51);
        eprintln!("{action}: target={target:?}, glyph={glyph:?}");
    }
    let copy = cx
        .debug_bounds(Box::leak(
            format!("worktree-copy-{selector}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_mouse_move(copy.center(), gpui::MouseButton::Left, Default::default());
    project_ticks(cx, 1);
    let switched = cx
        .debug_bounds("tooltip-popup")
        .expect("adjacent hint switches without a second initial delay");
    assert!(
        (f32::from(switched.center().x - copy.center().x)).abs() < 0.51,
        "hint follows Copy, not the previous Delete: {switched:?}"
    );
    project_ticks(cx, 8);
    assert!(
        cx.debug_bounds("tooltip-popup").is_some(),
        "leaving the previous button must not cancel the new button's hint"
    );
    cx.simulate_mouse_move(
        gpui::point(px(700.), px(500.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    project_ticks(cx, 8);
    assert!(
        cx.debug_bounds("tooltip-popup").is_none(),
        "leaving the row clears the tooltip"
    );
    cx.simulate_mouse_move(copy.center(), gpui::MouseButton::Left, Default::default());
    project_ticks(cx, 8);
    assert!(cx.debug_bounds("tooltip-popup").is_some());
    cx.simulate_click(copy.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("tooltip-popup").is_none(),
        "click clears the overlay"
    );
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(fixture.a.display().to_string())
    );
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_composition_reserves_actions_beside_ellipsis(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    for (opened, active, pinned, branch, primary) in [
        (
            false,
            false,
            false,
            "feature-with-a-very-long-name-that-must-ellipsis",
            false,
        ),
        (
            true,
            true,
            false,
            "feature-with-a-very-long-name-that-must-ellipsis",
            false,
        ),
        (
            true,
            false,
            true,
            "feature-with-a-very-long-name-that-must-ellipsis",
            false,
        ),
        (true, true, false, "main", true),
        (false, false, false, "master", false),
    ] {
        let path = if primary { &fixture.main } else { &fixture.b };
        root.update(cx, |r, cx| {
            r.settings.column_width = crate::column::narrowest(r.settings.ui_font_size);
            // Keep each case's input state independent while using the same cache.
            let b = r
                .desk
                .projects_of(workspace)
                .find(|p| p.path == fixture.b)
                .map(|p| p.id);
            if let Some(b) = b {
                r.desk.remove_project(b);
            }
            let pins = r
                .desk
                .workspace(workspace)
                .unwrap()
                .pinned_worktrees
                .clone();
            for pin in pins {
                r.desk.toggle_worktree_pin(workspace, &pin);
            }
            let project = if opened {
                let p = r.desk.open_in(workspace, path);
                r.desk.set_repository(p, Some(fixture.common.clone()));
                Some(p)
            } else {
                None
            };
            r.active_project = if active {
                project
            } else {
                r.desk
                    .projects_of(workspace)
                    .find(|p| p.path == fixture.main)
                    .map(|p| p.id)
            };
            for worktree in r.worktrees.get_mut(&fixture.common).unwrap() {
                if &worktree.path == path {
                    worktree.branch = Some(branch.into());
                    worktree.lines = Some((123, 45));
                }
            }
            if pinned {
                r.desk.toggle_worktree_pin(workspace, path);
            }
            cx.notify();
        });
        cx.simulate_mouse_move(
            gpui::point(px(700.), px(500.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        let row = root.read_with(cx, |r, _| {
            r.column_groups_for(workspace)[0]
                .rows
                .iter()
                .find(|row| r.column_row_path(row).as_ref() == Some(path))
                .unwrap()
                .clone()
        });
        let key = row_key(workspace, &row);
        let selector = Box::leak(key.clone().into_boxed_str());
        let label_selector = Box::leak(format!("worktree-label-{key}").into_boxed_str());
        let diff_selector = Box::leak(format!("worktree-diff-{key}").into_boxed_str());
        let resting_label = cx.debug_bounds(label_selector).unwrap();
        let resting_diff = cx
            .debug_bounds(diff_selector)
            .expect("diff counts are present at rest");
        assert!(
            resting_diff.size.width > px(0.) && resting_label.right() < resting_diff.left(),
            "resting counts have their own allocation after the ellipsis label"
        );
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_mouse_move(
            gpui::point(bounds.right() - px(20.), bounds.top() + px(16.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        let label = cx.debug_bounds(label_selector).unwrap();
        let copy = cx
            .debug_bounds(Box::leak(format!("worktree-copy-{key}").into_boxed_str()))
            .unwrap();
        assert!(
            label.size.width > px(0.) && label.right() < copy.left(),
            "ellipsis allocation must end before controls: label={label:?}, copy={copy:?}, branch={branch}"
        );
        assert!(
            resting_label.size.width > px(0.),
            "resting label also has usable allocated space"
        );
        let hovered_diff = cx.debug_bounds(diff_selector);
        assert!(
            hovered_diff.is_none_or(|bounds| bounds.size.width == px(0.)),
            "counts give their layout space to actions on hover: {hovered_diff:?}, label={label:?}, copy={copy:?}"
        );
        let pin = cx.debug_bounds(Box::leak(format!("worktree-pin-{key}").into_boxed_str()));
        let delete = cx.debug_bounds(Box::leak(format!("worktree-delete-{key}").into_boxed_str()));
        assert_eq!(pin.is_some(), !matches!(branch, "main" | "master"));
        assert_eq!(delete.is_some(), !primary);
        let last = delete.unwrap_or(copy);
        assert!(
            last.right() <= bounds.right(),
            "controls stay inside the narrow row"
        );
        if !opened && !primary && branch.starts_with("feature-") {
            let painted_ground =
                |cx: &mut gpui::VisualTestContext, bounds: gpui::Bounds<gpui::Pixels>| {
                    cx.update(|window, _| {
                        window
                            .painted_quads()
                            .into_iter()
                            .filter(|quad| quad.bounds == bounds.scale(window.scale_factor()))
                            .filter_map(|quad| quad.background.as_solid())
                            .filter(|color| color.a > 0.)
                            .collect::<Vec<_>>()
                    })
                };
            let row_grounds =
                root.read_with(cx, |r, _| [r.theme.surface.panel, r.theme.surface.raised]);
            let mut neutral_contrasts = [0.; 2];
            for action in ["copy", "pin", "delete"] {
                let button = cx
                    .debug_bounds(Box::leak(
                        format!("worktree-{action}-{key}").into_boxed_str(),
                    ))
                    .unwrap();
                cx.simulate_mouse_move(
                    button.center(),
                    gpui::MouseButton::Left,
                    Default::default(),
                );
                cx.run_until_parked();
                let paints = painted_ground(cx, button);
                let ground = paints
                    .first()
                    .expect("action paints a bounded hover ground");
                let color = u32::from(ground.to_rgb()) >> 8;
                for (index, row) in row_grounds.into_iter().enumerate() {
                    let contrast = theme::contrast(color, row);
                    assert!(
                        contrast >= 1.35,
                        "{action} hover must remain visible against normal/selected row"
                    );
                    if action == "copy" {
                        neutral_contrasts[index] = contrast;
                    }
                    if action == "delete" {
                        assert!(
                            contrast >= neutral_contrasts[index] * 0.9
                                && contrast <= neutral_contrasts[index] * 1.3,
                            "red hover must be comparably noticeable: red={contrast}, neutral={}",
                            neutral_contrasts[index]
                        );
                    }
                }
                if action == "delete" {
                    assert!(
                        (color >> 16) > ((color >> 8) & 255) && (color >> 16) > (color & 255),
                        "destructive hover stays softly red"
                    );
                }
            }
            cx.simulate_mouse_move(label.center(), gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            for action in ["copy", "pin", "delete"] {
                let button = cx
                    .debug_bounds(Box::leak(
                        format!("worktree-{action}-{key}").into_boxed_str(),
                    ))
                    .unwrap();
                assert!(
                    painted_ground(cx, button).is_empty(),
                    "{action} clears its ground after pointer leaves while the row remains hovered"
                );
            }
        }
        eprintln!(
            "composition opened={opened} active={active} pinned={pinned} branch={branch}: label={label:?}, copy={copy:?}, last={last:?}"
        );
    }
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_groups_drag_collapse_and_persist(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let paths: Vec<_> = (0..3)
        .map(|i| fixture.dir.path().join(format!("plain-{i}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    let workspace = root.update(cx, |r, cx| {
        for path in &paths {
            r.desk.open_in(r.active_workspace, path);
        }
        cx.notify();
        r.active_workspace
    });
    cx.run_until_parked();
    let header = |path: &std::path::Path| -> &'static str {
        Box::leak(format!("project-header-{}", path.display()).into_boxed_str())
    };
    // No group yet: nothing of the feature is drawn.
    assert!(cx.debug_bounds("column-ungrouped-drop").is_none());

    // *New Group…*'s field, typed into and taken with Enter.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_new_project_group(
                workspace,
                paths[0].clone(),
                gpui::point(px(40.), px(80.)),
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.simulate_input("Work");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let id = root.read_with(cx, |r, _| {
        let holder = r.desk.workspace(workspace).unwrap();
        assert_eq!(holder.project_groups.len(), 1, "Enter made the group");
        assert_eq!(holder.project_groups[0].name, "Work");
        holder.project_groups[0].id
    });
    let head: &'static str = Box::leak(format!("project-group-{id}").into_boxed_str());
    let group_head = cx.debug_bounds(head).expect("the group's header is drawn");
    let first = cx.debug_bounds(header(&paths[0])).unwrap();
    // A group is told from a project at a glance: a folder rather than a
    // letter tile, and how many projects it holds after its name, before
    // its chevron.
    assert!(
        cx.debug_bounds("project-group-tile").is_none(),
        "no letter tile on a group"
    );
    let icon = cx
        .debug_bounds("project-group-icon")
        .expect("the group's folder");
    let name = cx.debug_bounds("project-group-label").unwrap();
    let count = cx
        .debug_bounds("project-group-count")
        .expect("the group's count");
    let chevron = cx.debug_bounds("project-group-chevron").unwrap();
    assert!(group_head.contains(&icon.center()) && icon.right() <= name.left());
    assert!(name.right() <= count.left() && count.right() <= chevron.left());
    assert!(
        group_head.bottom() <= first.top(),
        "the grouped repository is drawn under its header"
    );
    // The group stands where its repository stood: after the one before.
    let main = cx.debug_bounds(header(&fixture.common)).unwrap();
    assert!(
        main.bottom() <= group_head.top(),
        "the group took its project's place"
    );
    // A group's repository is indented a step under its header, which
    // stands at the top level with the ungrouped repositories — and keeps
    // its right edge: indented, not pushed out of the column.
    assert!(
        (f32::from(group_head.left() - main.left())).abs() < 0.5,
        "a group's header stands where an ungrouped repository does"
    );
    let indent = f32::from(first.left() - main.left());
    assert!(
        (indent - f32::from(theme::column::GROUP_INDENT)).abs() < 0.5,
        "a grouped repository is indented under its header, got {indent}"
    );
    assert_eq!(
        first.right(),
        main.right(),
        "the indent narrows, not shifts"
    );

    // Drag a second project onto the group's header.
    let source = cx.debug_bounds(header(&paths[1])).unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        group_head.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_up(
        group_head.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let members = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.desk
                .workspace(workspace)
                .unwrap()
                .project_group(id)
                .map(|group| group.repositories.clone())
                .unwrap_or_default()
        })
    };
    assert_eq!(members(cx), [paths[0].clone(), paths[1].clone()]);

    // Collapse: the header stays, its repositories go.
    let at = cx.debug_bounds(head).unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(header(&paths[0])).is_none());
    assert!(cx.debug_bounds(head).is_some());
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    let group = saved
        .workspace(workspace)
        .unwrap()
        .project_group(id)
        .unwrap();
    assert!(group.collapsed, "collapsed state is saved");
    assert_eq!(group.repositories, [paths[0].clone(), paths[1].clone()]);
    let at = cx.debug_bounds(head).unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();

    // Dropped on the list's empty end, a project leaves its group.
    let source = cx.debug_bounds(header(&paths[0])).unwrap().center();
    let end = cx.debug_bounds("column-ungrouped-drop").unwrap().center();
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(members(cx), [paths[1].clone()]);

    // Let go on the line before an ungrouped repository, the last member
    // leaves too and takes the group with it.
    let source = cx.debug_bounds(header(&paths[1])).unwrap().center();
    let above = cx.debug_bounds(header(&paths[2])).unwrap();
    let target = gpui::point(above.center().x, above.top() + px(2.));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| {
        r.desk
            .workspace(workspace)
            .unwrap()
            .project_groups
            .is_empty()
    }));
    assert!(cx.debug_bounds(head).is_none());
    assert!(cx.debug_bounds("column-ungrouped-drop").is_none());
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_group_menus_create_rename_and_delete(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    let label =
        |text: &str| -> &'static str { Box::leak(format!("popup-label-{text}").into_boxed_str()) };
    let right_click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_mouse_down(at, gpui::MouseButton::Right, Default::default());
        cx.simulate_mouse_up(at, gpui::MouseButton::Right, Default::default());
        cx.run_until_parked();
    };
    let header: &'static str =
        Box::leak(format!("project-header-{}", fixture.common.display()).into_boxed_str());

    // The repository's menu: Move to Group ▸ New Group… — through the
    // drawn menu, as a person reaches it.
    right_click(cx, header);
    let submenu = cx
        .debug_bounds(label("Move to Group"))
        .expect("the repository menu offers Move to Group");
    cx.simulate_mouse_move(submenu.center(), None, Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds(label("Remove from Group")).is_none(),
        "an ungrouped repository is offered no removal"
    );
    let new = cx.debug_bounds(label("New Group…")).expect("New Group…");
    cx.simulate_click(new.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_input("Alpha");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let id = root.read_with(cx, |r, _| {
        let holder = r.desk.workspace(workspace).unwrap();
        let group = holder
            .project_group_of(&fixture.common)
            .expect("New Group… put the repository into the group it made");
        assert_eq!(group.name, "Alpha");
        group.id
    });
    let head: &'static str = Box::leak(format!("project-group-{id}").into_boxed_str());

    // The group's header: Rename Group…, which edits the current name.
    right_click(cx, head);
    let rename = cx
        .debug_bounds(label("Rename Group…"))
        .expect("Rename Group…");
    assert!(cx.debug_bounds(label("Delete Group")).is_some());
    cx.simulate_click(rename.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_input("Beta");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(cx, |r, _| r
            .desk
            .workspace(workspace)
            .unwrap()
            .project_group(id)
            .map(|group| group.name.clone())),
        Some("Beta".to_owned()),
        "the field opens with the name selected, so typing replaces it"
    );

    // Now the repository's submenu names its group and offers removal.
    right_click(cx, header);
    let submenu = cx.debug_bounds(label("Move to Group")).unwrap();
    cx.simulate_mouse_move(submenu.center(), None, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(label("Beta")).is_some());
    assert!(cx.debug_bounds(label("Remove from Group")).is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // Delete Group: the header goes, the repository stays in the workspace.
    let before: Vec<_> = root.read_with(cx, |r, _| {
        r.desk.projects_of(workspace).map(|p| p.id).collect()
    });
    right_click(cx, head);
    let delete = cx.debug_bounds(label("Delete Group")).unwrap();
    cx.simulate_click(delete.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(head).is_none());
    assert!(cx.debug_bounds(header).is_some());
    root.read_with(cx, |r, _| {
        let holder = r.desk.workspace(workspace).unwrap();
        assert!(holder.project_groups.is_empty());
        let after: Vec<_> = r.desk.projects_of(workspace).map(|p| p.id).collect();
        assert_eq!(after, before, "deleting a group closes nothing");
    });
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_worktree_tabs_list_focus_close_and_fold(cx: &mut TestAppContext) {
    use guild_session::{Runs, Seat, SessionMode, SessionRecord};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let main = fixture.main.clone();
    let cx = &mut *fixture.cx;
    let (workspace, agent, shell, page) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        let project = r
            .desk
            .projects_of(workspace)
            .find(|p| p.path == main)
            .unwrap()
            .id;
        let seat = Seat { workspace, project };
        let claude = r.desk.add_session(SessionRecord::new(
            "Agent".into(),
            seat,
            main.clone(),
            Runs::Claude(SessionMode::Terminal),
        ));
        r.desk.rename_session(claude, "Refine API");
        let terminal = r.desk.add_session(SessionRecord::new(
            "Shell".into(),
            seat,
            main.clone(),
            Runs::Shell,
        ));
        // A page holds no session: not listed.
        let page = r.adopt(
            seat,
            project,
            crate::Body::Settings {
                section: crate::view::settings::Section::General,
            },
            cx,
        );
        let broken = |session| crate::Body::Broken {
            session,
            reason: "Fixture without a provider process".into(),
        };
        let agent = r.adopt(seat, project, broken(claude), cx);
        let shell = r.adopt(seat, project, broken(terminal), cx);
        r.active_project = Some(project);
        r.active_tab = Some(page);
        r.board
            .report(claude, Some(guild_session::status::Reported::Working));
        cx.notify();
        (workspace, agent, shell, page)
    });
    cx.run_until_parked();
    let tab = |id: crate::TabId| -> &'static str {
        Box::leak(format!("column-tab-{id}").into_boxed_str())
    };
    let header: &'static str =
        Box::leak(format!("project-header-{}", fixture.common.display()).into_boxed_str());
    // Nobody has touched this list: it is collapsed, and its count shows
    // without the pointer. The chevron expands it.
    let toggle: &'static str =
        Box::leak(format!("worktree-tabs-toggle-{}", main.display()).into_boxed_str());
    assert!(
        cx.debug_bounds(tab(agent)).is_none(),
        "collapsed by default"
    );
    assert!(cx.debug_bounds("worktree-tabs-count").is_some());
    let at = cx.debug_bounds(toggle).expect("the chevron").center();
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(!root.read_with(cx, |r, _| r.desk.worktree_tabs_folded(workspace, &main)));
    assert!(cx.debug_bounds(tab(page)).is_none(), "a page is not listed");
    let first = cx
        .debug_bounds(tab(agent))
        .expect("the agent's tab is listed");
    let second = cx
        .debug_bounds(tab(shell))
        .expect("the shell's tab is listed");
    assert!(first.bottom() <= second.top(), "in the strip's order");
    assert!(cx.debug_bounds(header).unwrap().bottom() < first.top());
    let labels: Vec<_> = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            r.column_tabs(workspace, project, cx)
                .into_iter()
                .map(|tab| tab.label)
                .collect()
        })
    });
    assert_eq!(labels[0], "Refine API", "the strip's own name");
    // The working agent wears the strip's mark in its row.
    let status: &'static str = Box::leak(format!("column-tab-status-{agent}").into_boxed_str());
    assert!(cx.debug_bounds(status).is_some());

    // The geometry: a card about fifty high, three of air under it,
    // and a lit tab row twenty-five high that runs to the card's right
    // edge.
    let card_key = root.read_with(cx, |r, _| {
        crate::column::row_key(workspace, &r.column_groups_for(workspace)[0].rows[0])
    });
    let card = cx
        .debug_bounds(Box::leak(card_key.into_boxed_str()))
        .unwrap();
    let fill: &'static str = Box::leak(format!("column-tab-card-{agent}").into_boxed_str());
    let fill = cx.debug_bounds(fill).unwrap();
    let close = |a: gpui::Pixels, b: f32| (f32::from(a) - b).abs() < 0.5;
    assert!(close(card.size.height, 50.0), "card {card:?}");
    assert!(close(fill.top() - card.bottom(), 3.0), "{card:?} {fill:?}");
    assert!(close(fill.size.height, 25.0), "fill {fill:?}");
    assert_eq!(fill.right(), card.right());

    // A click brings the tab forward.
    cx.simulate_click(second.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(root.read_with(cx, |r, _| r.active_tab), Some(shell));

    // The chevron collapses the list again: the count shows, the rows go,
    // and the state is saved for this workspace.
    let row_key = root.read_with(cx, |r, _| {
        crate::column::row_key(workspace, &r.column_groups_for(workspace)[0].rows[0])
    });
    let row = cx
        .debug_bounds(Box::leak(row_key.into_boxed_str()))
        .unwrap();
    cx.simulate_mouse_move(row.center(), None, Default::default());
    cx.run_until_parked();
    let at = cx.debug_bounds(toggle).expect("the chevron").center();
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(tab(agent)).is_none());
    assert!(cx.debug_bounds("worktree-tabs-count").is_some());
    assert_eq!(
        root.read_with(cx, |r, _| r.active_tab),
        Some(shell),
        "the chevron is not a click on the row"
    );
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    assert!(saved.worktree_tabs_folded(workspace, &main));
    let at = cx.debug_bounds(toggle).unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(tab(agent)).is_some(), "expanded again");

    // Under the pointer the status gives way to the `×`, which closes the
    // tab by the strip's own path.
    let row = cx.debug_bounds(tab(shell)).unwrap();
    cx.simulate_mouse_move(row.center(), None, Default::default());
    cx.run_until_parked();
    let close: &'static str = Box::leak(format!("column-tab-close-{shell}").into_boxed_str());
    let at = cx.debug_bounds(close).expect("the close control").center();
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(
        root.read_with(cx, |r, _| r.tabs.iter().all(|tab| tab.id != shell)),
        "× closed the tab"
    );
    assert!(cx.debug_bounds(tab(shell)).is_none());
    assert!(cx.debug_bounds(tab(agent)).is_some());

    // A pinned worktree lists its tabs under its Pinned row, once.
    let linked = fixture.a.clone();
    let pinned = root.update(cx, |r, cx| {
        let project = r.desk.open_in(workspace, &linked);
        r.desk.set_repository(project, Some(fixture.common.clone()));
        r.desk.toggle_worktree_pin(workspace, &linked);
        r.desk.toggle_worktree_tabs(workspace, &linked);
        let seat = Seat { workspace, project };
        let session = r.desk.add_session(SessionRecord::new(
            "Pinned".into(),
            seat,
            linked.clone(),
            Runs::Codex(SessionMode::Terminal),
        ));
        let id = r.adopt(
            seat,
            project,
            crate::Body::Broken {
                session,
                reason: "Fixture without a provider process".into(),
            },
            cx,
        );
        cx.notify();
        id
    });
    cx.run_until_parked();
    let pinned_row = cx
        .debug_bounds(tab(pinned))
        .expect("listed under the pinned row");
    assert!(
        pinned_row.bottom() <= cx.debug_bounds(header).unwrap().top(),
        "the Pinned section is above the projects"
    );
}

#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_worktree_tabs_chevron_is_one_column_button(cx: &mut TestAppContext) {
    use guild_session::{Runs, Seat, SessionRecord};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let main = fixture.main.clone();
    let cx = &mut *fixture.cx;
    let (workspace, shell) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        let project = r
            .desk
            .projects_of(workspace)
            .find(|p| p.path == main)
            .unwrap()
            .id;
        let seat = Seat { workspace, project };
        let session = r.desk.add_session(SessionRecord::new(
            "Shell".into(),
            seat,
            main.clone(),
            Runs::Shell,
        ));
        let shell = r.adopt(
            seat,
            project,
            crate::Body::Broken {
                session,
                reason: "Fixture without a provider process".into(),
            },
            cx,
        );
        // Expanded by the person: the chevron then shows only under the
        // pointer.
        r.desk.toggle_worktree_tabs(workspace, &main);
        cx.notify();
        (workspace, shell)
    });
    cx.run_until_parked();
    let toggle: &'static str =
        Box::leak(format!("worktree-tabs-toggle-{}", main.display()).into_boxed_str());
    let row_key = root.read_with(cx, |r, _| {
        crate::column::row_key(workspace, &r.column_groups_for(workspace)[0].rows[0])
    });
    let row = cx
        .debug_bounds(Box::leak(row_key.into_boxed_str()))
        .unwrap();
    let lit = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            (
                r.hovered_worktree.as_ref().map(|(_, path)| path.clone()),
                r.hovered_column_tab,
            )
        })
    };

    // Into the row: its chevron shows, with the head's buttons' target.
    cx.simulate_mouse_move(row.center(), None, Default::default());
    cx.run_until_parked();
    let button = cx.debug_bounds(toggle).expect("the chevron");
    let target = f32::from(theme::column::ACTION_TARGET);
    assert!(
        (f32::from(button.size.height) - target).abs() < 0.5
            && f32::from(button.size.width) >= target - 0.5,
        "the chevron is a column button's target: {button:?}"
    );
    assert!(row.contains(&button.center()), "inside its row");

    // Onto it: the row stays the one lit, and no tab row lights.
    cx.simulate_mouse_move(button.center(), None, Default::default());
    cx.run_until_parked();
    assert_eq!(lit(cx), (Some(main.clone()), None));

    // A press folds the list and is not a click on the row.
    let before = root.read_with(cx, |r, _| r.active_tab);
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| r.desk.worktree_tabs_folded(workspace, &main)));
    assert_eq!(root.read_with(cx, |r, _| r.active_tab), before);
    let tab: &'static str = Box::leak(format!("column-tab-{shell}").into_boxed_str());
    assert!(cx.debug_bounds(tab).is_none());

    // Folded, "1 ›" is one button: the count sits inside its target.
    let button = cx.debug_bounds(toggle).unwrap();
    let count = cx.debug_bounds("worktree-tabs-count").unwrap();
    // Its text line is a hair taller than the target; the glyphs are not.
    assert!(
        count.left() >= button.left()
            && count.right() <= button.right()
            && (f32::from(count.center().y - button.center().y)).abs() < 0.5,
        "{button:?} {count:?}"
    );
    assert!((f32::from(button.size.height) - target).abs() < 0.5);
    assert_eq!(
        lit(cx),
        (Some(main.clone()), None),
        "still under the pointer"
    );

    // Out of the column: nothing stays lit, and the folded chevron stays.
    cx.simulate_mouse_move(gpui::point(px(900.), px(300.)), None, Default::default());
    cx.run_until_parked();
    assert_eq!(lit(cx), (None, None));
    assert!(cx.debug_bounds(toggle).is_some());

    // Back and pressed again: open, and the tab row is back.
    cx.simulate_mouse_move(row.center(), None, Default::default());
    cx.run_until_parked();
    let at = cx.debug_bounds(toggle).unwrap().center();
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(tab).is_some());
}

/// A group's header drags the group among the column's top-level items: let
/// go on a project, it takes that project's place; Escape abandons the
/// gesture with nothing moved; and a project let go on a group's header
/// still joins the group.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_group_drag_takes_a_place_cancels_and_keeps_joining(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let paths: Vec<_> = (0..3)
        .map(|i| fixture.dir.path().join(format!("plain-{i}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    let (workspace, group) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        for path in &paths {
            r.desk.open_in(workspace, path);
        }
        let group = r
            .desk
            .create_project_group(workspace, "Grouped", &paths[1])
            .unwrap();
        cx.notify();
        (workspace, group)
    });
    cx.run_until_parked();
    let tops = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            crate::column::tops(&r.column_sections_for(workspace))
        })
    };
    let repo = crate::column::Top::Repository(fixture.common.clone());
    let plain = |i: usize| crate::column::Top::Repository(paths[i].clone());
    let grouped = crate::column::Top::Group(group);
    assert_eq!(
        tops(cx),
        [repo.clone(), plain(0), grouped.clone(), plain(2)],
        "a new group stands where its project was"
    );
    let head: &'static str = Box::leak(format!("project-group-{group}").into_boxed_str());
    let header = |path: &std::path::Path| -> &'static str {
        Box::leak(format!("project-header-{}", path.display()).into_boxed_str())
    };

    // Onto the repository at the top: the group takes its place.
    let source = cx.debug_bounds(head).expect("the group's header");
    let target = cx.debug_bounds(header(&fixture.common)).unwrap().center();
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.has_active_drag()), "the header drags");
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [grouped.clone(), repo.clone(), plain(0), plain(2)]
    );
    assert!(
        !root.read_with(cx, |r, _| r
            .desk
            .workspace(workspace)
            .unwrap()
            .project_groups[0]
            .collapsed),
        "a drag is not a click on the header"
    );
    guild_session::store::flush();
    let saved = guild_session::persist::load(&crate::paths::workspace_file());
    assert_eq!(
        saved.workspace(workspace).unwrap().projects,
        root.read_with(cx, |r, _| r
            .desk
            .workspace(workspace)
            .unwrap()
            .projects
            .clone()),
        "the new place is saved"
    );

    // Escape abandons a drag: let go on a project afterwards, nothing moves.
    let source = cx.debug_bounds(head).unwrap();
    let target = cx.debug_bounds(header(&paths[2])).unwrap().center();
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "Escape ends the drag"
    );
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [grouped.clone(), repo.clone(), plain(0), plain(2)]
    );

    // A project let go on the group's header joins it, and the group stays.
    let source = cx.debug_bounds(header(&paths[0])).unwrap();
    let target = cx.debug_bounds(head).unwrap().center();
    cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(tops(cx), [grouped.clone(), repo.clone(), plain(2)]);
    let members = root.read_with(cx, |r, _| {
        r.desk.workspace(workspace).unwrap().project_groups[0]
            .repositories
            .clone()
    });
    assert_eq!(members, [paths[1].clone(), paths[0].clone()]);
}

/// A project dragged down to the line after a project of a group below it
/// joins that group right after it — rather than landing above it because
/// joining moved its roots first.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_downward_drop_into_another_group_lands_after_its_target(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let paths: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|name| fixture.dir.path().join(format!("plain-{name}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    let (workspace, lower) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        for path in &paths {
            r.desk.open_in(workspace, path);
        }
        r.desk
            .create_project_group(workspace, "Upper", &paths[0])
            .unwrap();
        let lower = r
            .desk
            .create_project_group(workspace, "Lower", &paths[1])
            .unwrap();
        r.desk
            .move_to_project_group(workspace, &paths[2], Some(lower));
        cx.notify();
        (workspace, lower)
    });
    cx.run_until_parked();
    let header = |path: &std::path::Path| -> &'static str {
        Box::leak(format!("project-header-{}", path.display()).into_boxed_str())
    };
    let source = cx.debug_bounds(header(&paths[0])).unwrap().center();
    // The line after b — its block's bottom edge — inside Lower.
    let b_block = cx
        .debug_bounds(Box::leak(
            format!("project-block-{}", paths[1].display()).into_boxed_str(),
        ))
        .unwrap();
    let target = gpui::point(b_block.center().x, b_block.bottom() - px(2.));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        gpui::point(source.x, source.y + px(4.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let shown: Vec<_> = root.read_with(cx, |r, _| {
        r.column_sections_for(workspace)
            .into_iter()
            .find(|section| section.head.as_ref().is_some_and(|head| head.id == lower))
            .unwrap()
            .repositories
            .into_iter()
            .map(|group| group.key)
            .collect()
    });
    assert_eq!(
        shown,
        [paths[1].clone(), paths[0].clone(), paths[2].clone()]
    );
}

/// A project goes between two groups by a row's edge: the edge shows a
/// line, the middle of a group's header takes it into the group, and the
/// line follows the pointer from one to the other. A group goes by the same
/// lines; Escape abandons a drag with its line; and a drag under a sort
/// mode leaves the column in Manual, as before.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_edges_place_projects_and_groups_between_items(cx: &mut TestAppContext) {
    use crate::column::{Listed, Top, Zone};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let paths: Vec<_> = (0..4)
        .map(|i| fixture.dir.path().join(format!("plain-{i}")))
        .collect();
    for path in &paths {
        std::fs::create_dir_all(path).unwrap();
    }
    // [repo] [First: plain-0, plain-1] [Second: plain-2] [plain-3]
    let (workspace, first, second) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        for path in &paths {
            r.desk.open_in(workspace, path);
        }
        let first = r
            .desk
            .create_project_group(workspace, "First", &paths[0])
            .unwrap();
        r.desk
            .move_to_project_group(workspace, &paths[1], Some(first));
        let second = r
            .desk
            .create_project_group(workspace, "Second", &paths[2])
            .unwrap();
        r.desk
            .set_project_sort(workspace, guild_session::project_sort::Mode::MostActivity);
        cx.notify();
        (workspace, first, second)
    });
    cx.run_until_parked();
    let tops = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            crate::column::tops(&r.column_sections_for(workspace))
        })
    };
    let hint = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.column_drop_hint.clone().map(|h| (h.row, h.zone))
        })
    };
    let repo = Top::Repository(fixture.common.clone());
    let plain = |i: usize| Top::Repository(paths[i].clone());
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(first),
            Top::Group(second),
            plain(3)
        ]
    );
    let header = |path: &std::path::Path| -> &'static str {
        Box::leak(format!("project-header-{}", path.display()).into_boxed_str())
    };
    let head = |id| -> &'static str { Box::leak(format!("project-group-{id}").into_boxed_str()) };
    let block = |path: &std::path::Path| -> &'static str {
        Box::leak(format!("project-block-{}", path.display()).into_boxed_str())
    };
    // The first move after a press starts the drag; rows hear the moves
    // after it, as they do under a hand.
    let press = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            gpui::point(at.x, at.y + px(4.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
    };
    let top_edge = |b: gpui::Bounds<gpui::Pixels>| gpui::point(b.center().x, b.top() + px(2.));
    let bottom_edge =
        |b: gpui::Bounds<gpui::Pixels>| gpui::point(b.center().x, b.bottom() - px(2.));

    // plain-3 onto the top edge of Second's header: a line, which turns into
    // the header's own light as the pointer goes to its middle and back.
    let source = cx.debug_bounds(header(&paths[3])).unwrap().center();
    let second_head = cx.debug_bounds(head(second)).unwrap();
    press(cx, source);
    cx.simulate_mouse_move(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(hint(cx), Some((Listed::Group(second), Zone::Before)));
    assert!(
        cx.debug_bounds("column-drop-line").is_some(),
        "the line is drawn"
    );
    cx.simulate_mouse_move(
        second_head.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(hint(cx), Some((Listed::Group(second), Zone::Middle)));
    assert!(
        cx.debug_bounds("column-drop-line").is_none(),
        "no line over the middle"
    );
    cx.simulate_mouse_move(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(hint(cx), Some((Listed::Group(second), Zone::Before)));
    // Into the gap the line is drawn in: it stays, and letting go there
    // places the project as the line says.
    let gap = gpui::point(second_head.center().x, second_head.top() - px(6.));
    cx.simulate_mouse_move(gap, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        hint(cx),
        Some((Listed::Group(second), Zone::Before)),
        "the line does not blink out between rows"
    );
    cx.simulate_mouse_up(gap, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(first),
            plain(3),
            Top::Group(second)
        ],
        "between the two groups"
    );
    assert_eq!(hint(cx), None, "the line goes with the drag");
    assert_eq!(
        root.read_with(cx, |r, _| r.desk.workspace(workspace).unwrap().project_sort),
        guild_session::project_sort::Mode::Manual,
        "a drag to a place switches to Manual"
    );

    // plain-0 out of First, to the line before Second: it leaves its group.
    let source = cx.debug_bounds(header(&paths[0])).unwrap().center();
    let second_head = cx.debug_bounds(head(second)).unwrap();
    press(cx, source);
    cx.simulate_mouse_move(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_up(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(first),
            plain(3),
            plain(0),
            Top::Group(second)
        ]
    );

    // The middle of First's header still takes a project into the group.
    let source = cx.debug_bounds(header(&paths[3])).unwrap().center();
    let first_head = cx.debug_bounds(head(first)).unwrap();
    press(cx, source);
    cx.simulate_mouse_move(
        first_head.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_up(
        first_head.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(first),
            plain(0),
            Top::Group(second)
        ]
    );
    assert!(root.read_with(cx, |r, _| {
        r.desk
            .workspace(workspace)
            .unwrap()
            .project_group(first)
            .unwrap()
            .repositories
            .contains(&paths[3])
    }));

    // A group by a line: Second to the bottom edge of the repository's
    // block, below its worktrees — right after it.
    let source = cx.debug_bounds(head(second)).unwrap().center();
    let repo_block = cx.debug_bounds(block(&fixture.common)).unwrap();
    press(cx, source);
    cx.simulate_mouse_move(
        bottom_edge(repo_block),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        hint(cx),
        Some((Listed::Repository(fixture.common.clone()), Zone::After))
    );
    cx.simulate_mouse_up(
        bottom_edge(repo_block),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(second),
            Top::Group(first),
            plain(0)
        ]
    );

    // Escape takes the drag and its line away; letting go moves nothing.
    let source = cx.debug_bounds(header(&paths[0])).unwrap().center();
    let second_head = cx.debug_bounds(head(second)).unwrap();
    press(cx, source);
    cx.simulate_mouse_move(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert!(hint(cx).is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(hint(cx), None, "Escape takes the line away");
    assert!(cx.debug_bounds("column-drop-line").is_none());
    cx.simulate_mouse_up(
        top_edge(second_head),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        tops(cx),
        [
            repo.clone(),
            Top::Group(second),
            Top::Group(first),
            plain(0)
        ]
    );
}

/// The four things a person's recorded drags showed, each as its gesture:
/// the place before the first item can be reached and seen — also from the
/// heading above the list — for a group and for a project; a drop on a
/// project of the dragged project's own group keeps it in the group; a
/// place that would change nothing draws no line; and the dragged pill does
/// not cover the row under the pointer.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_recorded_drag_problems(cx: &mut TestAppContext) {
    use crate::column::{Listed, Top, Zone};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let names = ["ember", "atlas", "beacon", "delta", "omega", "cedar"];
    let path = |name: &str| fixture.dir.path().join(name);
    for name in names {
        std::fs::create_dir_all(path(name)).unwrap();
    }
    // [ember] [тест: atlas, beacon, delta, omega] [cedar] — and the
    // fixture's repository, closed away at the end.
    let (workspace, group) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        let main = r.desk.projects_of(workspace).next().unwrap().id;
        for name in names {
            r.desk.open_in(workspace, &path(name));
        }
        r.desk.leave(workspace, main);
        let group = r
            .desk
            .create_project_group(workspace, "тест", &path("atlas"))
            .unwrap();
        for name in ["beacon", "delta", "omega"] {
            r.desk
                .move_to_project_group(workspace, &path(name), Some(group));
        }
        cx.notify();
        (workspace, group)
    });
    cx.run_until_parked();
    let tops = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            crate::column::tops(&r.column_sections_for(workspace))
        })
    };
    let hint = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.column_drop_hint.clone().map(|h| (h.row, h.zone))
        })
    };
    let members = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.column_sections_for(workspace)
                .into_iter()
                .find(|s| s.head.as_ref().is_some_and(|h| h.id == group))
                .map(|s| {
                    s.repositories
                        .into_iter()
                        .map(|g| g.key)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let repo = |name: &str| Top::Repository(path(name));
    assert_eq!(tops(cx), [repo("ember"), Top::Group(group), repo("cedar")]);
    let header = |name: &str| -> &'static str {
        Box::leak(format!("project-header-{}", path(name).display()).into_boxed_str())
    };
    let block = |name: &str| -> &'static str {
        Box::leak(format!("project-block-{}", path(name).display()).into_boxed_str())
    };
    let head: &'static str = Box::leak(format!("project-group-{group}").into_boxed_str());
    let press = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            gpui::point(at.x, at.y + px(4.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
    };
    let to = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_move(at, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
    };
    let release = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_up(at, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
    };

    // 4. The pill sits right of and below the pointer: the name of the row
    // under it stays readable.
    let source = cx.debug_bounds(header("cedar")).unwrap().center();
    press(cx, source);
    let over = cx.debug_bounds(header("ember")).unwrap().center();
    to(cx, over);
    let pill = cx.debug_bounds("column-drag-preview").expect("the pill");
    assert!(
        pill.left() > over.x && pill.top() > over.y,
        "the pill {pill:?} is clear of the pointer {over:?}"
    );
    cx.simulate_keystrokes("escape");
    release(cx, over);

    // 1. The group to the place before the first item: a line that can be
    // seen inside the list, and the group first after letting go.
    let source = cx.debug_bounds(head).unwrap().center();
    let first = cx.debug_bounds(block("ember")).unwrap();
    press(cx, source);
    let edge = gpui::point(first.center().x, first.top() + px(2.));
    to(cx, edge);
    assert_eq!(
        hint(cx),
        Some((Listed::Repository(path("ember")), Zone::Before))
    );
    let line = cx.debug_bounds("column-drop-line").expect("a line");
    let list = cx.debug_bounds("column-list").unwrap();
    assert!(
        line.top() >= list.top(),
        "the first line is not cut off: {line:?} in {list:?}"
    );
    release(cx, edge);
    assert_eq!(tops(cx), [Top::Group(group), repo("ember"), repo("cedar")]);

    // 1. A project to the place before the first item, from the heading
    // above the list.
    let source = cx.debug_bounds(header("cedar")).unwrap().center();
    let heading = cx.debug_bounds("column-projects-heading").unwrap().center();
    press(cx, source);
    to(cx, heading);
    assert_eq!(hint(cx), Some((Listed::Group(group), Zone::Before)));
    release(cx, heading);
    assert_eq!(tops(cx), [repo("cedar"), Top::Group(group), repo("ember")]);

    // 3. A place that changes nothing draws no line: the group just below
    // cedar, over cedar's bottom edge, is already there.
    let source = cx.debug_bounds(head).unwrap().center();
    let above = cx.debug_bounds(block("cedar")).unwrap();
    press(cx, source);
    to(cx, gpui::point(above.center().x, above.bottom() - px(2.)));
    assert_eq!(hint(cx), None, "no line on the group's own place");
    assert!(cx.debug_bounds("column-drop-line").is_none());
    // Letting go there changes nothing either — not even the sort mode.
    root.update(cx, |r, _| {
        r.desk
            .set_project_sort(workspace, guild_session::project_sort::Mode::MostActivity);
    });
    release(cx, gpui::point(above.center().x, above.bottom() - px(2.)));
    assert_eq!(tops(cx), [repo("cedar"), Top::Group(group), repo("ember")]);
    assert_eq!(
        root.read_with(cx, |r, _| r.desk.workspace(workspace).unwrap().project_sort),
        guild_session::project_sort::Mode::MostActivity,
        "a drop on its own place is no drag to a place"
    );
    root.update(cx, |r, _| {
        r.desk
            .set_project_sort(workspace, guild_session::project_sort::Mode::Manual);
    });

    // 2. A project of the group let go of on another project of the same
    // group joins beside it — it never leaves the group.
    assert_eq!(members(cx), ["atlas", "beacon", "delta", "omega"].map(path));
    let source = cx.debug_bounds(header("atlas")).unwrap().center();
    press(cx, source);
    let between = cx.debug_bounds(header("beacon")).unwrap().center();
    to(cx, between);
    let delta = cx.debug_bounds(block("delta")).unwrap();
    let target = gpui::point(delta.center().x, delta.bottom() - px(2.));
    to(cx, target);
    assert_eq!(
        hint(cx),
        Some((Listed::Repository(path("delta")), Zone::After))
    );
    release(cx, target);
    assert_eq!(
        members(cx),
        ["beacon", "delta", "atlas", "omega"].map(path),
        "beside delta, in the group"
    );
    assert_eq!(tops(cx), [repo("cedar"), Top::Group(group), repo("ember")]);
}

/// A project let go of on the middle of an ungrouped project asks to merge
/// the two into a group, naming both, with the group's name to type: Enter
/// or Merge makes the group at the target's place, Escape or Cancel changes
/// nothing — the sort mode included — and a blank name is not taken. A
/// group's header still takes a project without asking, and inside a group
/// the lines between its projects place one in it.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_project_on_project_offers_to_merge(cx: &mut TestAppContext) {
    use crate::column::{Listed, Top, Zone};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let names = ["ember", "atlas", "cedar", "beacon", "delta"];
    let path = |name: &str| fixture.dir.path().join(name);
    for name in names {
        std::fs::create_dir_all(path(name)).unwrap();
    }
    // [ember] [atlas] [cedar] [G: beacon, delta]
    let (workspace, group) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        let main = r.desk.projects_of(workspace).next().unwrap().id;
        for name in names {
            r.desk.open_in(workspace, &path(name));
        }
        r.desk.leave(workspace, main);
        let group = r
            .desk
            .create_project_group(workspace, "G", &path("beacon"))
            .unwrap();
        r.desk
            .move_to_project_group(workspace, &path("delta"), Some(group));
        r.desk
            .set_project_sort(workspace, guild_session::project_sort::Mode::MostActivity);
        cx.notify();
        (workspace, group)
    });
    cx.run_until_parked();
    let tops = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            crate::column::tops(&r.column_sections_for(workspace))
        })
    };
    let hint = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.column_drop_hint.clone().map(|h| (h.row, h.zone))
        })
    };
    let sort = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| r.desk.workspace(workspace).unwrap().project_sort)
    };
    let groups = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.desk
                .workspace(workspace)
                .unwrap()
                .project_groups
                .iter()
                .map(|g| (g.name.clone(), g.repositories.clone()))
                .collect::<Vec<_>>()
        })
    };
    let repo = |name: &str| Top::Repository(path(name));
    let header = |name: &str| -> &'static str {
        Box::leak(format!("project-header-{}", path(name).display()).into_boxed_str())
    };
    let press = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            gpui::point(at.x, at.y + px(4.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
    };
    let drop_at = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        cx.simulate_mouse_move(at, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.simulate_mouse_up(at, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
    };
    let before = tops(cx);
    assert_eq!(
        before,
        [
            repo("ember"),
            repo("atlas"),
            repo("cedar"),
            Top::Group(group)
        ]
    );

    // atlas on cedar's middle: lit, then the question — and nothing moved.
    let source = cx.debug_bounds(header("atlas")).unwrap().center();
    press(cx, source);
    let target = cx.debug_bounds(header("cedar")).unwrap().center();
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        hint(cx),
        Some((Listed::Repository(path("cedar")), Zone::Middle))
    );
    drop_at(cx, target);
    assert!(
        cx.debug_bounds("merge-projects").is_some(),
        "the question is asked"
    );
    assert_eq!(tops(cx), before, "nothing moved yet");
    assert_eq!(sort(cx), guild_session::project_sort::Mode::MostActivity);
    // The name field reads as one: its hint is shown although it has the
    // keys from the start, and it is drawn in a field's outlined box.
    cx.update(|window, cx| {
        use gpui_component::{WindowExt, input::AnyInputState};
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("the name field has the keys")
        };
        assert_eq!(
            input.read(cx).presentation().placeholder().as_str(),
            "Group name",
            "the focused, empty name field shows its hint"
        );
    });
    let frame = cx
        .debug_bounds("merge-projects-field")
        .expect("the name field's box");
    let outline = root.read_with(cx, |r, _| {
        gpui::Hsla::from(gpui::rgb(r.theme.text.tertiary))
    });
    assert!(
        cx.update(|window, _| {
            window.painted_quads().into_iter().any(|quad| {
                quad.bounds == frame.scale(window.scale_factor())
                    && quad.border_widths.top.0 > 0.
                    && quad.border_color == outline
            })
        }),
        "the name field is outlined as Settings' fields are"
    );
    // A blank name is not taken.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-projects").is_some(), "still asking");
    // Escape: nothing.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-projects").is_none());
    assert_eq!(tops(cx), before);
    assert_eq!(groups(cx).len(), 1);

    // Again, and Cancel: nothing.
    let source = cx.debug_bounds(header("atlas")).unwrap().center();
    press(cx, source);
    let cedar = cx.debug_bounds(header("cedar")).unwrap().center();
    drop_at(cx, cedar);
    let cancel = cx.debug_bounds("merge-projects-cancel").expect("Cancel");
    cx.simulate_click(cancel.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-projects").is_none());
    assert_eq!(tops(cx), before);
    assert_eq!(sort(cx), guild_session::project_sort::Mode::MostActivity);

    // Again, a name and Enter: a group of the two at cedar's place.
    let source = cx.debug_bounds(header("atlas")).unwrap().center();
    press(cx, source);
    let cedar = cx.debug_bounds(header("cedar")).unwrap().center();
    drop_at(cx, cedar);
    cx.simulate_input("pair");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-projects").is_none());
    let made = groups(cx)
        .into_iter()
        .find(|(name, _)| name == "pair")
        .expect("the group is made");
    assert_eq!(made.1, [path("cedar"), path("atlas")]);
    let pair = root.read_with(cx, |r, _| {
        r.desk
            .workspace(workspace)
            .unwrap()
            .project_groups
            .iter()
            .find(|g| g.name == "pair")
            .unwrap()
            .id
    });
    assert_eq!(
        tops(cx),
        [repo("ember"), Top::Group(pair), Top::Group(group)]
    );

    // The middle of a group's header: in without a question.
    let source = cx.debug_bounds(header("ember")).unwrap().center();
    let head: &'static str = Box::leak(format!("project-group-{group}").into_boxed_str());
    press(cx, source);
    let middle = cx.debug_bounds(head).unwrap().center();
    drop_at(cx, middle);
    assert!(cx.debug_bounds("merge-projects").is_none(), "no question");
    assert!(
        groups(cx)
            .iter()
            .any(|(name, members)| name == "G" && members.contains(&path("ember")))
    );

    // Inside a group: the line above delta puts atlas into G before it; a
    // project's middle there is no target.
    let source = cx.debug_bounds(header("atlas")).unwrap().center();
    press(cx, source);
    let delta = cx.debug_bounds(header("delta")).unwrap();
    cx.simulate_mouse_move(delta.center(), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(hint(cx), None, "a grouped project's middle is no target");
    drop_at(cx, gpui::point(delta.center().x, delta.top() + px(2.)));
    let g = groups(cx)
        .into_iter()
        .find(|(name, _)| name == "G")
        .unwrap();
    let at = |name: &str| g.1.iter().position(|p| *p == path(name));
    assert!(at("atlas").is_some(), "atlas joined G");
    let shown: Vec<_> = root.read_with(cx, |r, _| {
        r.column_sections_for(workspace)
            .into_iter()
            .find(|s| s.head.as_ref().is_some_and(|h| h.id == group))
            .unwrap()
            .repositories
            .into_iter()
            .map(|g| g.key)
            .collect()
    });
    let a = shown.iter().position(|p| *p == path("atlas")).unwrap();
    let d = shown.iter().position(|p| *p == path("delta")).unwrap();
    assert_eq!(a + 1, d, "atlas right before delta: {shown:?}");
}

/// Under a sort mode a drag places by what is shown: the shown order — the
/// top level and inside each group — becomes the manual one first, then the
/// item goes where its line was, and nothing else moves.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_sorted_drop_keeps_the_shown_order(cx: &mut TestAppContext) {
    use guild_session::project_sort::Mode;
    use guild_session::{Runs, Seat, SessionMode, SessionRecord};
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let cx = &mut *fixture.cx;
    let names = ["ember", "atlas", "beacon", "cedar", "delta"];
    let path = |name: &str| fixture.dir.path().join(name);
    for name in names {
        std::fs::create_dir_all(path(name)).unwrap();
    }
    let (workspace, hello, pair) = root.update(cx, |r, cx| {
        let workspace = r.active_workspace;
        let main = r.desk.projects_of(workspace).next().unwrap().id;
        let mut ids = std::collections::HashMap::new();
        for name in names {
            ids.insert(name, r.desk.open_in(workspace, &path(name)));
        }
        r.desk.leave(workspace, main);
        let hello = r
            .desk
            .create_project_group(workspace, "hello", &path("beacon"))
            .unwrap();
        for name in ["cedar", "delta"] {
            r.desk
                .move_to_project_group(workspace, &path(name), Some(hello));
        }
        let pair = r
            .desk
            .create_project_group(workspace, "pair", &path("atlas"))
            .unwrap();
        // Manual: pair, hello (beacon, cedar, delta), ember.
        r.desk
            .reorder_projects_at(workspace, &[ids["ember"]], ids["delta"], true);
        // Recent activity: cedar, delta, beacon, atlas, ember.
        let reported = guild_session::status::ReportOutcome {
            changed: true,
            began: true,
            ended: false,
        };
        for (name, at) in [
            ("cedar", 50),
            ("delta", 40),
            ("beacon", 30),
            ("atlas", 20),
            ("ember", 10),
        ] {
            let session = r.desk.add_session(SessionRecord::new(
                name.into(),
                Seat {
                    workspace,
                    project: ids[name],
                },
                path(name),
                Runs::Claude(SessionMode::Terminal),
            ));
            r.desk.observe_reported_activity(session, reported, at);
        }
        r.desk.set_project_sort(workspace, Mode::RecentActivity);
        cx.notify();
        (workspace, hello, pair)
    });
    cx.run_until_parked();
    // The whole column: each top-level item, a group with its projects.
    let shown = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |r, _| {
            r.column_sections_for(workspace)
                .into_iter()
                .map(|s| {
                    let name = s.head.as_ref().map(|h| h.name.clone());
                    let keys: Vec<String> = s
                        .repositories
                        .iter()
                        .map(|g| g.key.file_name().unwrap().to_string_lossy().into_owned())
                        .collect();
                    match name {
                        Some(name) => format!("{name}({})", keys.join(",")),
                        None => keys.join(","),
                    }
                })
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        shown(cx),
        ["hello(cedar,delta,beacon)", "pair(atlas)", "ember"]
    );
    let header = |name: &str| -> &'static str {
        Box::leak(format!("project-header-{}", path(name).display()).into_boxed_str())
    };
    let head: &'static str = Box::leak(format!("project-group-{hello}").into_boxed_str());
    let drag_to_top = |cx: &mut gpui::VisualTestContext, name: &str| {
        let source = cx.debug_bounds(header(name)).unwrap().center();
        cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            gpui::point(source.x, source.y + px(4.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.run_until_parked();
        let first = cx.debug_bounds(head).unwrap();
        let line = gpui::point(first.center().x, first.top() + px(2.));
        cx.simulate_mouse_move(line, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
        cx.simulate_mouse_up(line, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
    };

    // ember, to the line above hello: first, and nothing else moves.
    drag_to_top(cx, "ember");
    assert_eq!(
        root.read_with(cx, |r, _| r.desk.workspace(workspace).unwrap().project_sort),
        Mode::Manual
    );
    assert_eq!(
        shown(cx),
        ["ember", "hello(cedar,delta,beacon)", "pair(atlas)"]
    );

    // Sorted again, atlas out of pair to the top: pair goes with its last
    // project, the rest stays as shown.
    root.update(cx, |r, cx| {
        r.desk.set_project_sort(workspace, Mode::RecentActivity);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        shown(cx),
        ["hello(cedar,delta,beacon)", "pair(atlas)", "ember"]
    );
    drag_to_top(cx, "atlas");
    assert_eq!(shown(cx), ["atlas", "hello(cedar,delta,beacon)", "ember"]);
    assert!(root.read_with(cx, |r, _| {
        r.desk
            .workspace(workspace)
            .unwrap()
            .project_group(pair)
            .is_none()
    }));
}

/// GLD-055 decision 29: the group header's `+` with nothing to add is drawn
/// disabled and does nothing; a project outside the group enables it again.
#[gpui::test]
#[ignore = "run through isolated_worktree_order_regressions"]
fn cached_column_group_plus_is_disabled_with_nothing_to_add(cx: &mut TestAppContext) {
    let fixture = mount(cx);
    let root = fixture.root.clone();
    let common = fixture.common.clone();
    let cx = &mut *fixture.cx;
    let workspace = root.read_with(cx, |r, _| r.active_workspace);
    // Every project of the workspace is in the group.
    root.update(cx, |r, cx| {
        r.desk
            .create_project_group(workspace, "all", &common)
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("project-group-add").is_none());
    let disabled = cx
        .debug_bounds("project-group-add-disabled")
        .expect("the + is drawn, disabled");
    cx.simulate_click(disabled.center(), Default::default());
    cx.run_until_parked();
    assert!(
        root.read_with(cx, |r, _| r.context_menu.is_none()),
        "a disabled + opens nothing"
    );

    // A project outside the group: the + is live and offers it.
    let other = fixture.dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    root.update(cx, |r, cx| {
        r.desk.open_in(workspace, &other);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("project-group-add-disabled").is_none());
    let live = cx
        .debug_bounds("project-group-add")
        .expect("the + is live again");
    cx.simulate_click(live.center(), Default::default());
    cx.run_until_parked();
    assert!(root.read_with(cx, |r, _| r.context_menu.is_some()));
}
