//! The production column with far more projects than fit: what is far from
//! the viewport is a spacer, and every gesture still reaches what it aims at.
use super::*;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord, WorkspaceId};

const REPOS: usize = 30;

struct Fixture<'a> {
    _dir: tempfile::TempDir,
    root: Entity<Root>,
    workspace: WorkspaceId,
    /// Each repository's common directory, the key its group is kept by.
    commons: Vec<std::path::PathBuf>,
    /// Each repository's main checkout, opened.
    mains: Vec<guild_session::ProjectId>,
    cx: &'a mut VisualTestContext,
}

impl Drop for Fixture<'_> {
    fn drop(&mut self) {
        guild_session::store::flush();
    }
}

/// Thirty repositories of a main checkout and two unopened worktrees, the
/// last named *zebra*; `working` repositories have an agent at work.
fn mount<'a>(cx: &'a mut TestAppContext, working: &[usize]) -> Fixture<'a> {
    mount_with(cx, working, |_, _, _| {})
}

/// [`mount`], with `setup` given the desk and each repository's common
/// directory before the window is made.
fn mount_with<'a>(
    cx: &'a mut TestAppContext,
    working: &[usize],
    setup: impl FnOnce(&mut Desk, WorkspaceId, &[std::path::PathBuf]),
) -> Fixture<'a> {
    assert!(
        crate::paths::workspace_file()
            .starts_with(std::env::var_os("GUILD_STATE_DIR").expect("isolated state"))
    );
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let mut commons = Vec::new();
    let mut mains = Vec::new();
    let mut workspace = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let shown = desk.active_workspace();
        workspace = Some(shown);
        let mut lists = Vec::new();
        let mut sessions = Vec::new();
        for r in 0..REPOS {
            let main = dir.path().join(format!("repository-{r:02}"));
            let common = main.join(".git");
            let mut porcelain = String::new();
            for (w, branch) in ["main", "feature-a", "feature-b"].iter().enumerate() {
                let path = if w == 0 {
                    main.clone()
                } else {
                    dir.path().join(format!("repository-{r:02}-{branch}"))
                };
                std::fs::create_dir_all(&path).unwrap();
                porcelain.push_str(&format!(
                    "worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/{branch}\0\0",
                    path.display()
                ));
            }
            let project = desk.open_in(shown, &main);
            desk.set_repository(project, Some(common.clone()));
            if r == REPOS - 1 {
                desk.rename_repository(&common, "zebra");
            }
            if working.contains(&r) {
                sessions.push(desk.add_session(SessionRecord::new(
                    "Agent".into(),
                    Seat {
                        workspace: shown,
                        project,
                    },
                    main.clone(),
                    Runs::Claude(SessionMode::Terminal),
                )));
            }
            lists.push((
                common.clone(),
                crate::git::worktrees::parse(porcelain.as_bytes()).unwrap(),
            ));
            commons.push(common);
            mains.push(project);
        }
        setup(&mut desk, shown, &commons);
        desk.select_workspace(shown);
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            for (common, list) in lists {
                root.worktrees.insert(common, list);
            }
            for session in sessions {
                root.board
                    .report(session, Some(guild_session::status::Reported::Working));
            }
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    cx.run_until_parked();
    // Two more frames: the first one drawn at this size has no past.
    for _ in 0..2 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    Fixture {
        _dir: dir,
        root: made.unwrap(),
        workspace: workspace.unwrap(),
        commons,
        mains,
        cx,
    }
}

fn header(common: &std::path::Path) -> &'static str {
    Box::leak(format!("project-header-{}", common.display()).into_boxed_str())
}

/// Every item's top and height and the list's extent, as the scroll handle
/// has them after the last frame. A spacer is not painted, so how far in
/// and how wide it is does not matter; where it puts what follows does.
fn layout(f: &mut Fixture<'_>) -> (Vec<(gpui::Pixels, gpui::Pixels)>, gpui::Point<gpui::Pixels>) {
    f.root.read_with(f.cx, |r, _| {
        let handle = r.project_drag.scroll(r.active_workspace);
        (
            (0..handle.children_count())
                .filter_map(|index| handle.bounds_for_item(index))
                .map(|bounds| (bounds.top(), bounds.size.height))
                .collect(),
            handle.max_offset(),
        )
    })
}

fn redraw(f: &mut Fixture<'_>) {
    f.root.update(f.cx, |_, cx| cx.notify());
    f.cx.run_until_parked();
}

fn spacers_shown(f: &mut Fixture<'_>) -> bool {
    f.cx.debug_bounds("column-spacer").is_some()
}

fn list(f: &mut Fixture<'_>) -> gpui::Bounds<gpui::Pixels> {
    f.cx.debug_bounds("column-list").unwrap()
}

/// Scroll the list with the wheel until `selector` is drawn inside it.
fn wheel_to(f: &mut Fixture<'_>, selector: &'static str) -> gpui::Bounds<gpui::Pixels> {
    let at = list(f).center();
    for _ in 0..200 {
        if let Some(bounds) = f.cx.debug_bounds(selector)
            && list(f).contains(&bounds.center())
        {
            return bounds;
        }
        f.cx.simulate_event(gpui::ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-90.))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        f.cx.run_until_parked();
    }
    panic!("{selector} never came into the list");
}

#[gpui::test]
fn column_spacers_keep_the_list_whole(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_keep_the_list_whole"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    assert!(spacers_shown(&mut f), "far items are spacers");
    let first = header(&f.commons[0]);
    let last = header(&f.commons[REPOS - 1]);
    assert!(f.cx.debug_bounds(first).is_some(), "the viewport is drawn");
    assert!(f.cx.debug_bounds(last).is_none(), "the far end is not");
    let spaced = layout(&mut f);

    // A list with no past — the first frame, or one cleared — draws
    // everything, and lays every item out exactly where its spacer stood.
    f.root.read_with(f.cx, |r, _| r.column_spacers.clear());
    redraw(&mut f);
    assert!(
        !spacers_shown(&mut f),
        "a list never drawn draws everything"
    );
    assert!(f.cx.debug_bounds(last).is_some());
    assert_eq!(layout(&mut f), spaced, "same bounds, same max_offset");
    redraw(&mut f);
    assert!(
        spacers_shown(&mut f),
        "and spacers come back the frame after"
    );

    // The column's width and the interface's size decide every height:
    // after either, the spacers stand where everything drawn would.
    for change in [
        (|r: &mut Root| r.settings.column_width += 40.) as fn(&mut Root),
        |r: &mut Root| r.settings.ui_font_size += 2.,
    ] {
        f.root.update(f.cx, |r, cx| {
            change(r);
            cx.notify();
        });
        f.cx.run_until_parked();
        redraw(&mut f);
        assert!(spacers_shown(&mut f));
        let spaced = layout(&mut f);
        f.root.read_with(f.cx, |r, _| r.column_spacers.clear());
        redraw(&mut f);
        assert!(!spacers_shown(&mut f));
        assert_eq!(layout(&mut f), spaced);
    }
}

#[gpui::test]
fn column_spacers_a_far_group_is_drawn_once_scrolled_to_and_clicks(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_far_group_is_drawn_once_scrolled_to_and_clicks"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let last = header(&f.commons[REPOS - 1]);
    wheel_to(&mut f, last);
    let row = f.root.read_with(f.cx, |r, _| {
        r.column_groups_for(r.active_workspace).last().unwrap().rows[0].clone()
    });
    let key = Box::leak(row_key(f.workspace, &row).into_boxed_str());
    let bounds = f.cx.debug_bounds(key).expect("its rows are drawn");
    f.cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    f.cx.run_until_parked();
    assert_eq!(
        f.root.read_with(f.cx, |r, _| r.active_project),
        Some(f.mains[REPOS - 1]),
        "a click on a row drawn after scrolling chooses it"
    );
    assert!(
        f.cx.debug_bounds(header(&f.commons[0])).is_none(),
        "and the top of the list is a spacer now"
    );
}

#[gpui::test]
fn column_spacers_a_jump_past_the_margin_is_drawn_whole(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_jump_past_the_margin_is_drawn_whole"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let last = header(&f.commons[REPOS - 1]);
    // A scroll the list's own prepaint makes, which no plan foresaw: the
    // frame after it is asked for, and draws where the list landed.
    let blank = super::spacers::BLANK.with(|frames| frames.get());
    f.root.update(f.cx, |r, cx| {
        let items = r.column_items_for(r.active_workspace).len();
        r.project_drag
            .scroll(r.active_workspace)
            .scroll_to_top_of_item(items - 1);
        cx.notify();
    });
    f.cx.run_until_parked();
    let bounds = f.cx.debug_bounds(last).expect("the far end is drawn");
    assert!(list(&mut f).contains(&bounds.center()));
    assert_eq!(
        super::spacers::BLANK.with(|frames| frames.get()),
        blank + 1,
        "one frame blank, and the one it asked for drawn whole"
    );
}

#[gpui::test]
fn column_spacers_speed_search_reveals_and_lights_a_far_project(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_speed_search_reveals_and_lights_a_far_project"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let last = header(&f.commons[REPOS - 1]);
    assert!(f.cx.debug_bounds(last).is_none());
    let root = f.root.clone();
    f.cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    f.cx.run_until_parked();
    let blank = super::spacers::BLANK.with(|frames| frames.get());
    f.cx.simulate_keystrokes("z e b r a");
    let bounds = f.cx.debug_bounds(last).expect("the match is drawn");
    assert!(list(&mut f).contains(&bounds.center()), "in view");
    assert_eq!(
        f.root.read_with(f.cx, |r, _| r
            .sidebar_speed
            .as_ref()
            .and_then(|s| s.current.clone())),
        Some(crate::column::Listed::Repository(
            f.commons[REPOS - 1].clone()
        )),
        "and lit as the search's match"
    );
    // And never a blank frame first: the frame that scrolls draws where
    // the list lands.
    assert_eq!(super::spacers::BLANK.with(|frames| frames.get()), blank);
}

#[gpui::test]
fn column_spacers_a_far_worktree_drags_into_place(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_far_worktree_drags_into_place"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let last = header(&f.commons[REPOS - 1]);
    wheel_to(&mut f, last);
    let (rows, common) = f.root.read_with(f.cx, |r, _| {
        (
            r.column_groups_for(r.active_workspace)
                .last()
                .unwrap()
                .rows
                .clone(),
            f.commons[REPOS - 1].clone(),
        )
    });
    let bounds = |f: &mut Fixture<'_>, row: &Row| {
        f.cx.debug_bounds(Box::leak(row_key(f.workspace, row).into_boxed_str()))
            .unwrap()
    };
    // Down to the last row, which is the one held.
    let held = Box::leak(row_key(f.workspace, &rows[2]).into_boxed_str());
    wheel_to(&mut f, held);
    let before = bounds(&mut f, &rows[1]);
    let source = bounds(&mut f, &rows[2]);
    let target = gpui::point(before.center().x, before.top() + px(5.));
    f.cx.simulate_mouse_down(source.center(), gpui::MouseButton::Left, Default::default());
    f.cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    f.cx.run_until_parked();
    f.cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    f.cx.run_until_parked();
    let order = f.root.read_with(f.cx, |r, _| {
        r.desk.workspace(f.workspace).unwrap().worktree_order[&common].clone()
    });
    let path = |row: &Row| match row {
        Row::Unopened { path, .. } => path.clone(),
        Row::Opened { .. } => f
            .root
            .read_with(f.cx, |r, _| r.column_row_path(row).unwrap()),
    };
    assert_eq!(order, vec![path(&rows[0]), path(&rows[2]), path(&rows[1])]);
}

#[gpui::test]
fn column_spacers_a_working_ring_turns_only_where_it_is_drawn(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_working_ring_turns_only_where_it_is_drawn"
    )) {
        return;
    }
    // An agent at work in the last repository only. The workspace's own
    // mark under the list turns for it wherever its row is; the row's ring
    // is in a spacer and is not painted, so it asks the clock for nothing.
    let mut f = mount(cx, &[REPOS - 1]);
    let last = header(&f.commons[REPOS - 1]);
    assert!(f.cx.debug_bounds(last).is_none());
    let turning = |f: &mut Fixture<'_>| {
        let before = crate::view::activity::TURNING.with(|rings| rings.get());
        redraw(f);
        crate::view::activity::TURNING.with(|rings| rings.get()) - before
    };
    let far = turning(&mut f);
    // Scrolled into view, its ring turns beside the workspace's.
    wheel_to(&mut f, last);
    assert_eq!(
        turning(&mut f),
        far + 1,
        "the row's ring is painted, turning"
    );
    // And the clock it asks keeps the column moving.
    let before = crate::view::region::RENDERS.with(|r| r.get());
    for _ in 0..20 {
        f.cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        f.cx.run_until_parked();
    }
    assert!(crate::view::region::RENDERS.with(|r| r.get()) - before >= 5);
}

/// Ten groups of three repositories, expanded and bound to no folder: each
/// header overlaps the first repository under it by its negative margin.
fn grouped(desk: &mut Desk, workspace: WorkspaceId, commons: &[std::path::PathBuf]) {
    for (n, members) in commons.chunks(3).enumerate() {
        let id = desk
            .create_project_group(workspace, &format!("Group {n}"), &members[0])
            .unwrap();
        for key in &members[1..] {
            assert!(desk.move_to_project_group(workspace, key, Some(id)));
        }
    }
}

/// The spaced layout against everything drawn, at the same offset.
fn assert_whole(f: &mut Fixture<'_>) {
    assert!(spacers_shown(f), "the fixture has spacers to compare");
    let spaced = layout(f);
    f.root.read_with(f.cx, |r, _| r.column_spacers.clear());
    redraw(f);
    assert!(!spacers_shown(f));
    assert_eq!(layout(f), spaced, "every child where its spacer stood");
}

#[gpui::test]
fn column_spacers_keep_a_grouped_list_whole(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_keep_a_grouped_list_whole"
    )) {
        return;
    }
    let mut f = mount_with(cx, &[], grouped);
    // In the middle: spaced headers above the viewport and below it.
    let middle = header(&f.commons[15]);
    wheel_to(&mut f, middle);
    assert!(f.cx.debug_bounds(header(&f.commons[0])).is_none());
    assert!(f.cx.debug_bounds(header(&f.commons[REPOS - 1])).is_none());
    assert_whole(&mut f);
}

#[gpui::test]
fn column_spacers_a_taller_window_shows_no_blank(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_taller_window_shows_no_blank"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let blank = super::spacers::BLANK.with(|frames| frames.get());
    // More than twice the height, the column as wide as it was.
    f.cx.simulate_resize(gpui::size(px(1000.), px(1500.)));
    f.cx.run_until_parked();
    assert_eq!(super::spacers::BLANK.with(|frames| frames.get()), blank);
    redraw(&mut f);
    assert_whole(&mut f);
}

#[gpui::test]
fn column_spacers_another_display_scale_keeps_the_list_whole(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_another_display_scale_keeps_the_list_whole"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    // A size whose lines are fractional points, snapped by the scale.
    f.root.update(f.cx, |r, cx| {
        r.settings.ui_font_size = 14.;
        cx.notify();
    });
    f.cx.run_until_parked();
    redraw(&mut f);
    f.cx.simulate_scale_factor_change(1.);
    f.cx.run_until_parked();
    redraw(&mut f);
    assert_whole(&mut f);
}

#[gpui::test]
fn column_spacers_leaving_a_search_deep_in_a_group_shows_no_blank(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_leaving_a_search_deep_in_a_group_shows_no_blank"
    )) {
        return;
    }
    // The last twelve repositories in a collapsed group, all found by one
    // query: the search opens the group, Escape closes it again.
    let mut f = mount_with(cx, &[], |desk, workspace, commons| {
        let tail = &commons[REPOS - 12..];
        let id = desk
            .create_project_group(workspace, "Tail", &tail[0])
            .unwrap();
        for (n, key) in tail.iter().enumerate() {
            desk.rename_repository(key, &format!("zebra {n:02}"));
            if n > 0 {
                assert!(desk.move_to_project_group(workspace, key, Some(id)));
            }
        }
        assert!(desk.toggle_project_group(workspace, id));
    });
    let root = f.root.clone();
    f.cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    f.cx.run_until_parked();
    f.cx.simulate_keystrokes("z e b r a");
    let last = header(&f.commons[REPOS - 1]);
    wheel_to(&mut f, last);
    // Down to the list's end, deep inside the opened group.
    let at = list(&mut f).center();
    for _ in 0..20 {
        f.cx.simulate_event(gpui::ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-200.))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        f.cx.run_until_parked();
    }
    let blank = super::spacers::BLANK.with(|frames| frames.get());
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    assert!(f.root.read_with(f.cx, |r, _| r.sidebar_speed.is_none()));
    assert_eq!(
        super::spacers::BLANK.with(|frames| frames.get()),
        blank,
        "the shorter list's clamped offset was drawn in the frame it took"
    );
}

#[gpui::test]
fn column_spacers_a_row_scrolled_away_takes_its_tooltip(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_row_scrolled_away_takes_its_tooltip"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let row = f.root.read_with(f.cx, |r, _| {
        r.column_groups_for(r.active_workspace)[0].rows[1].clone()
    });
    let bounds =
        f.cx.debug_bounds(Box::leak(row_key(f.workspace, &row).into_boxed_str()))
            .unwrap();
    let ticks = |f: &mut Fixture<'_>, n: usize| {
        for _ in 0..n {
            f.cx.executor()
                .advance_clock(std::time::Duration::from_millis(80));
            f.cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            f.cx.run_until_parked();
        }
    };
    // A worktree action's tooltip, the pointer still on it.
    let on = gpui::point(bounds.right() - px(20.), bounds.top() + px(16.));
    f.cx.simulate_mouse_move(on, None, Default::default());
    f.cx.run_until_parked();
    ticks(&mut f, 10);
    assert!(
        f.cx.debug_bounds("tooltip-popup").is_some(),
        "the tooltip shows"
    );
    // One step far past the margin, the pointer where it was.
    f.cx.simulate_event(gpui::ScrollWheelEvent {
        position: on,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-2500.))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    f.cx.run_until_parked();
    ticks(&mut f, 10);
    assert!(
        f.cx.debug_bounds(Box::leak(row_key(f.workspace, &row).into_boxed_str()))
            .is_none(),
        "its row is a spacer now"
    );
    assert!(
        f.cx.debug_bounds("tooltip-popup").is_none(),
        "and its tooltip went with it"
    );
}

#[gpui::test]
fn column_spacers_count_the_tabs_a_row_lists(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_count_the_tabs_a_row_lists"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let (workspace, mains) = (f.workspace, f.mains.clone());
    let cx = &mut *f.cx;
    f.root.update(cx, |r, cx| {
        for (n, project) in mains.iter().enumerate() {
            let seat = Seat {
                workspace,
                project: *project,
            };
            // A session tab or more on every third repository.
            for t in 0..(n % 3) {
                let session = r.desk.add_session(SessionRecord::new(
                    format!("Agent {t}"),
                    seat,
                    "/".into(),
                    Runs::Claude(SessionMode::Terminal),
                ));
                r.adopt(
                    seat,
                    *project,
                    crate::Body::Broken {
                        session,
                        reason: "Fixture".into(),
                    },
                    cx,
                );
            }
            // A page with no session, and a session with no record: the
            // column lists neither.
            if n % 4 == 0 {
                r.adopt(
                    seat,
                    *project,
                    crate::Body::Settings {
                        section: crate::view::settings::Section::General,
                    },
                    cx,
                );
                r.adopt(
                    seat,
                    *project,
                    crate::Body::Broken {
                        session: guild_session::SessionId::new(),
                        reason: "Forgotten".into(),
                    },
                    cx,
                );
            }
            let path = r.desk.project(*project).unwrap().path.clone();
            r.desk.toggle_worktree_tabs(workspace, &path);
        }
        cx.notify();
    });
    f.cx.run_until_parked();
    f.root.update(f.cx, |r, cx| {
        let counts = r.column_tab_counts();
        for project in &mains {
            let seat = Seat {
                workspace,
                project: *project,
            };
            assert_eq!(
                counts.get(&seat).copied().unwrap_or(0),
                r.column_tabs(workspace, *project, cx).len(),
                "one walk counts what each row lists"
            );
        }
    });
    // The lists drawn open: their heights, counted, keep spacers whole.
    redraw(&mut f);
    assert_whole(&mut f);
}

fn wheel(f: &mut Fixture<'_>, at: gpui::Point<gpui::Pixels>, dy: f32) {
    f.cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    f.cx.run_until_parked();
}

fn blank() -> usize {
    super::spacers::BLANK.with(|frames| frames.get())
}

#[gpui::test]
fn column_spacers_a_search_that_opens_a_far_group_shows_no_blank(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_search_that_opens_a_far_group_shows_no_blank"
    )) {
        return;
    }
    // A collapsed group of one, *zebra*, at the end of a long list.
    let mut f = mount_with(cx, &[], |desk, workspace, commons| {
        let tail = &commons[REPOS - 1];
        let id = desk.create_project_group(workspace, "Tail", tail).unwrap();
        assert!(desk.toggle_project_group(workspace, id));
    });
    let root = f.root.clone();
    f.cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    f.cx.run_until_parked();
    let before = blank();
    // The first key opens the group and brings it to the list's end.
    f.cx.simulate_keystrokes("z");
    let found =
        f.cx.debug_bounds(header(&f.commons[REPOS - 1]))
            .expect("the found project is drawn");
    assert!(list(&mut f).contains(&found.center()));
    f.cx.simulate_keystrokes("e b r a");
    assert_eq!(
        blank(),
        before,
        "the frame that opens and scrolls draws it all"
    );
}

#[gpui::test]
fn column_spacers_a_wheel_past_either_end_shows_no_blank(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_wheel_past_either_end_shows_no_blank"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let at = list(&mut f).center();
    let before = blank();
    // Far past the top, then far past the end, in single steps.
    wheel(&mut f, at, 8000.);
    assert_eq!(blank(), before, "past the top");
    wheel(&mut f, at, -60000.);
    assert_eq!(blank(), before, "past the end");
    assert!(f.cx.debug_bounds(header(&f.commons[REPOS - 1])).is_some());
    wheel(&mut f, at, -8000.);
    assert_eq!(blank(), before, "further past the end");
    wheel(&mut f, at, 60000.);
    assert_eq!(blank(), before, "and back past the top");
    assert!(f.cx.debug_bounds(header(&f.commons[0])).is_some());
}

#[gpui::test]
fn column_spacers_unpinning_shows_no_blank(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_unpinning_shows_no_blank"
    )) {
        return;
    }
    // Twenty late worktrees pinned: Pinned takes its 40% of the column.
    // Every repository folded, so that items are short beside the growth.
    let mut pinned = Vec::new();
    let mut f = mount_with(cx, &[], |desk, workspace, commons| {
        for common in commons {
            desk.toggle_fold(common);
        }
        for common in &commons[REPOS - 10..] {
            let main = common.parent().unwrap();
            let name = main.file_name().unwrap().to_string_lossy().into_owned();
            for branch in ["feature-a", "feature-b"] {
                let path = main.with_file_name(format!("{name}-{branch}"));
                desk.toggle_worktree_pin(workspace, &path);
                pinned.push(path);
            }
        }
    });
    // A short window at the largest interface size, the search open: the
    // list's own viewport is small beside Pinned.
    f.cx.simulate_resize(gpui::size(px(1000.), px(500.)));
    f.root.update(f.cx, |r, cx| {
        r.settings.ui_font_size = guild_session::MAX_UI_FONT_SIZE;
        cx.notify();
    });
    f.cx.run_until_parked();
    let root = f.root.clone();
    f.cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    f.cx.simulate_keystrokes("r");
    redraw(&mut f);
    redraw(&mut f);
    assert!(spacers_shown(&mut f));
    let before = blank();
    let workspace = f.workspace;
    f.root.update(f.cx, |r, cx| {
        for path in &pinned {
            r.desk.toggle_worktree_pin(workspace, path);
        }
        cx.notify();
    });
    f.cx.run_until_parked();
    assert_eq!(
        blank(),
        before,
        "the taller list was drawn in the frame it grew"
    );
}

#[gpui::test]
fn column_spacers_a_spaced_row_forgets_its_hover(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_a_spaced_row_forgets_its_hover"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let workspace = f.workspace;
    // A session tab listed under the first repository's main row.
    let (tab, row) = f.root.update(f.cx, |r, cx| {
        let project = r.desk.projects_of(workspace).next().unwrap().id;
        let seat = Seat { workspace, project };
        let session = r.desk.add_session(SessionRecord::new(
            "Agent".into(),
            seat,
            "/".into(),
            Runs::Claude(SessionMode::Terminal),
        ));
        let tab = r.adopt(
            seat,
            project,
            crate::Body::Broken {
                session,
                reason: "Fixture".into(),
            },
            cx,
        );
        let path = r.desk.project(project).unwrap().path.clone();
        r.desk.toggle_worktree_tabs(workspace, &path);
        cx.notify();
        (tab, r.column_groups_for(workspace)[0].rows[1].clone())
    });
    f.cx.run_until_parked();
    let at_row =
        f.cx.debug_bounds(Box::leak(row_key(workspace, &row).into_boxed_str()))
            .unwrap()
            .center();
    let at_tab =
        f.cx.debug_bounds(Box::leak(format!("column-tab-{tab}").into_boxed_str()))
            .unwrap()
            .center();
    for at in [at_row, at_tab] {
        f.cx.simulate_mouse_move(at, None, Default::default());
        f.cx.run_until_parked();
        let held = f.root.read_with(f.cx, |r, _| {
            (r.hovered_worktree.clone(), r.hovered_column_tab)
        });
        assert!(held.0.is_some() || held.1.is_some(), "hovered: {held:?}");
        // One step far past the margin, landing the pointer on the head of
        // the twentieth repository — no row of its own takes the hover:
        // the hovered row is a spacer, and never heard the pointer leave.
        let delta = f.root.read_with(f.cx, |r, _| {
            let handle = r.project_drag.scroll(workspace);
            let item = handle.bounds_for_item(20).unwrap();
            at.y - px(10.) - item.top() - handle.offset().y
        });
        wheel(&mut f, at, delta.into());
        let now = f.root.read_with(f.cx, |r, _| {
            (r.hovered_worktree.clone(), r.hovered_column_tab)
        });
        if held.0.is_some() {
            assert_ne!(now.0, held.0, "the worktree's hover went with its row");
        }
        if held.1.is_some() {
            assert_ne!(now.1, held.1, "the tab's hover went with its row");
        }
        wheel(&mut f, at, -f32::from(delta));
    }
}

#[gpui::test]
fn column_spacers_another_rows_spacer_keeps_the_tooltip(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::column_spacers_another_rows_spacer_keeps_the_tooltip"
    )) {
        return;
    }
    let mut f = mount(cx, &[]);
    let workspace = f.workspace;
    // Scrolled so that the fourth repository sits two points inside the
    // margin above the viewport: a four-point step makes it a spacer.
    f.root.update(f.cx, |r, cx| {
        let handle = r.project_drag.scroll(workspace);
        let (view, item) = (handle.bounds(), handle.bounds_for_item(3).unwrap());
        let offset = view.top() - view.size.height + px(2.) - item.bottom();
        handle.set_offset(gpui::point(px(0.), offset));
        cx.notify();
    });
    f.cx.run_until_parked();
    redraw(&mut f);
    // A worktree action's tooltip on a row in view.
    let list = list(&mut f);
    let row = f.root.read_with(f.cx, |r, _| {
        r.column_groups_for(workspace)
            .into_iter()
            .flat_map(|group| group.rows)
            .filter(|row| matches!(row, Row::Unopened { .. }))
            .collect::<Vec<_>>()
    });
    let (row, bounds) = row
        .iter()
        .find_map(|row| {
            let bounds =
                f.cx.debug_bounds(Box::leak(row_key(workspace, row).into_boxed_str()))?;
            list.contains(&bounds.center())
                .then(|| (row.clone(), bounds))
        })
        .expect("an unopened row in view");
    let on = gpui::point(bounds.right() - px(20.), bounds.top() + px(16.));
    f.cx.simulate_mouse_move(on, None, Default::default());
    f.cx.run_until_parked();
    let ticks = |f: &mut Fixture<'_>| {
        for _ in 0..10 {
            f.cx.executor()
                .advance_clock(std::time::Duration::from_millis(80));
            f.cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            f.cx.run_until_parked();
        }
    };
    ticks(&mut f);
    assert!(
        f.cx.debug_bounds("tooltip-popup").is_some(),
        "the tooltip shows"
    );
    wheel(&mut f, on, -4.);
    assert!(
        f.cx.debug_bounds(Box::leak(row_key(workspace, &row).into_boxed_str()))
            .is_some(),
        "its row is still drawn"
    );
    ticks(&mut f);
    assert!(
        f.cx.debug_bounds("tooltip-popup").is_some(),
        "another row's spacer leaves this one's tooltip"
    );
}
