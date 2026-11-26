//! Native layout at the actual minimum column and every supported font size.
use super::*;
use gpui::{AppContext, Entity, TestAppContext};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

struct ActivityRow {
    root: Entity<Root>,
    row: Row,
    workspace: guild_session::WorkspaceId,
    font: f32,
    input: gpui::Entity<gpui_component::input::InputState>,
}
impl Render for ActivityRow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(crate::scale::rem_size(self.font));
        let input = self.input.clone();
        self.root.update(cx, |root, cx| {
            root.settings.ui_font_size = self.font;
            div()
                .track_focus(&root.focus)
                .w(px(232.))
                .on_key_down(cx.listener(|host, event, window, cx| {
                    if !host.context_menu_key(event, window, cx) {
                        crate::view::controls::tab_navigation(event, window, cx);
                    }
                }))
                .child(root.render_row(self.workspace, &self.row, cx))
                .child(gpui_component::input::Input::new(&input))
                .children(root.render_context_menu(cx))
        })
    }
}

#[gpui::test]
fn activity_two_lines_fit_232_points_at_every_supported_interface_size(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::activity_two_lines_fit_232_points_at_every_supported_interface_size"
    )) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let seat = Seat { workspace, project };
        let ids: Vec<_> = (0..40)
            .map(|i| {
                desk.add_session(SessionRecord::new(
                    format!("Agent {i}"),
                    seat,
                    dir.path().into(),
                    Runs::Claude(SessionMode::Terminal),
                ))
            })
            .collect();
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            for (i, id) in ids.iter().enumerate() {
                root.board.report(
                    *id,
                    [
                        Some(guild_session::status::Reported::Blocked(Some(
                            guild_session::status::Kind::Permission,
                        ))),
                        Some(guild_session::status::Reported::Error),
                        Some(guild_session::status::Reported::Working),
                        Some(guild_session::status::Reported::Done),
                    ][i % 4],
                );
            }
            root
        });
        let row = Row::Opened {
            project,
            title: crate::column::Title {
                name: "a-very-long-worktree-branch-name".into(),
                primary: true,
            },
            branch: None,
            committed: Some(1),
            lines: Some((u32::MAX, u32::MAX)),
        };
        let input = cx.new(|cx| gpui_component::input::InputState::new(window, cx));
        let view = cx.new(|_| ActivityRow {
            input,
            root,
            row,
            workspace,
            font: 13.,
        });
        made = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(232.), px(180.)));
    let host = made.unwrap();
    cx.run_until_parked();
    let input = host.read_with(cx, |host, _| host.input.clone());
    cx.update(|window, cx| {
        use gpui::Focusable;
        window.focus(&input.read(cx).focus_handle(cx), cx);
    });
    let control = cx.debug_bounds("activity-control").unwrap();
    cx.simulate_click(control.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    host.read_with(cx, |host, cx| {
        host.root.read_with(cx, |root, _| {
            assert!(
                root.context_menu.is_some(),
                "indicator click opens disclosure"
            );
        });
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("a");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "a",
        "mouse dismissal returns typing to prior input"
    );
    cx.simulate_keystrokes("tab enter");
    cx.run_until_parked();
    assert!(
        host.read_with(cx, |host, cx| host.root.read(cx).context_menu.is_some()),
        "keyboard trigger opens disclosure"
    );
    let trigger = cx.update(|_window, cx| {
        host.read(cx)
            .root
            .read(cx)
            .context_menu
            .as_ref()
            .and_then(|open| {
                if let crate::view::menu::Target::WorktreeActivity { return_focus, .. } =
                    &open.target
                {
                    return_focus.clone()
                } else {
                    None
                }
            })
            .expect("keyboard trigger focus")
    });
    cx.simulate_keystrokes("escape");
    assert!(
        cx.update(|window, _| trigger.is_focused(window)),
        "keyboard dismissal returns to trigger"
    );
    for signals in [
        [
            Some(guild_session::status::Reported::Blocked(Some(
                guild_session::status::Kind::Permission,
            ))),
            Some(guild_session::status::Reported::Error),
            Some(guild_session::status::Reported::Working),
            Some(guild_session::status::Reported::Done),
        ],
        [
            Some(guild_session::status::Reported::Error),
            Some(guild_session::status::Reported::Working),
            Some(guild_session::status::Reported::Done),
            None,
        ],
        [
            Some(guild_session::status::Reported::Working),
            Some(guild_session::status::Reported::Done),
            None,
            Some(guild_session::status::Reported::Working),
        ],
        [
            Some(guild_session::status::Reported::Done),
            None,
            Some(guild_session::status::Reported::Done),
            None,
        ],
        [None; 4],
    ] {
        host.update(cx, |host, cx| {
            host.root.update(cx, |root, cx| {
                root.board = Default::default();
                let ids: Vec<_> = root.desk.sessions().map(|record| record.id).collect();
                for (i, id) in ids.into_iter().enumerate() {
                    root.board.report(id, signals[i % signals.len()]);
                }
                cx.notify();
            });
            cx.notify();
        });
        for font in 10..=20 {
            host.update(cx, |host, cx| {
                host.font = font as f32;
                cx.notify();
            });
            cx.run_until_parked();
            let slot = cx.debug_bounds("activity-control").unwrap();
            assert_eq!(
                slot.size,
                gpui::size(px(14.), px(18.)),
                "fixed slot at {font}"
            );
            let branch = cx.debug_bounds("activity-branch").unwrap();
            let age = cx.debug_bounds("activity-age").unwrap();
            let counts = cx.debug_bounds("activity-secondary").unwrap();
            assert!(
                branch.size.width > px(0.),
                "branch remains present at {font}"
            );
            assert!(
                age.top() >= branch.bottom(),
                "second line below the branch at {font}"
            );
            assert!(
                age.right() <= counts.left(),
                "age never overlaps counts at {font}"
            );
            assert!(
                counts.right() <= px(232.),
                "bounded counts stay inside the column at {font}"
            );
            let line = cx.debug_bounds("activity-meta-line").unwrap();
            assert!(
                age.top() >= line.top() && age.bottom() <= line.bottom(),
                "age remains inside the single meta line at {font}: {age:?}, {line:?}"
            );
            assert!(
                counts.top() >= line.top() && counts.bottom() <= line.bottom(),
                "counts never create a third line at {font}"
            );
        }
    }
    // Work in progress asks nothing of the person: beside a failure, a row
    // working everywhere else but one unread answer counts that answer alone,
    // and no `↻` — with Working primary it would be excluded as the primary.
    host.update(cx, |host, cx| {
        host.font = 13.;
        host.root.update(cx, |root, cx| {
            root.board = Default::default();
            let ids: Vec<_> = root.desk.sessions().map(|record| record.id).collect();
            for (i, id) in ids.into_iter().enumerate() {
                root.board.report(
                    id,
                    Some(match i {
                        0 => guild_session::status::Reported::Error,
                        1 => guild_session::status::Reported::Done,
                        _ => guild_session::status::Reported::Working,
                    }),
                );
            }
            cx.notify();
        });
        cx.notify();
    });
    cx.run_until_parked();
    let counts = cx.debug_bounds("activity-secondary").unwrap();
    let unread = cx
        .debug_bounds("activity-count-●1")
        .expect("the unread answer is counted beside the failure");
    assert!(cx.debug_bounds("activity-count-↻9+").is_none());
    assert_eq!(
        counts.size.width, unread.size.width,
        "nothing else is counted"
    );
}

// Draw each production caller separately so a different working mark cannot mask
// a stalled one. Root's normal first render installs its activation observer.
#[derive(Clone, Copy, Debug)]
enum BackgroundSurface {
    Row,
    Group,
    Workspace,
    Tabs(Runs),
    Sessions(Runs),
}
struct BackgroundActivity {
    root: Entity<Root>,
    group: Group,
    workspace: guild_session::WorkspaceId,
    surface: BackgroundSurface,
    clipped: bool,
    renders: std::rc::Rc<std::cell::Cell<usize>>,
}
impl Render for BackgroundActivity {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        self.root.update(cx, |root, cx| {
            if root.activation.is_none() {
                let _ = root.render(window, cx);
            }
            let child = match self.surface {
                BackgroundSurface::Tabs(_) => root.render_tabs(root.focused_area(), cx),
                BackgroundSurface::Sessions(_) => root.render_session_list(self.workspace, cx),
                BackgroundSurface::Row => root
                    .render_row(self.workspace, &self.group.rows[0], cx)
                    .into_any_element(),
                BackgroundSurface::Group => root
                    .render_group(self.workspace, &self.group, None, false, false, cx)
                    .into_any_element(),
                BackgroundSurface::Workspace => {
                    root.render_workspace_switcher(cx).into_any_element()
                }
            };
            div()
                .flex()
                .flex_col()
                .w(px(if matches!(self.surface, BackgroundSurface::Tabs(_)) {
                    600.
                } else {
                    232.
                }))
                .h(px(180.))
                .overflow_hidden()
                .child(
                    div()
                        .flex_none()
                        .h(px(if self.clipped { 200. } else { 0. })),
                )
                .child(div().flex_none().child(child))
        })
    }
}
#[gpui::test]
fn background_activity_column_callers_keep_requesting_frames(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::background_activity_column_callers_keep_requesting_frames"
    )) {
        return;
    }
    for surface in [
        BackgroundSurface::Row,
        BackgroundSurface::Group,
        BackgroundSurface::Workspace,
        BackgroundSurface::Tabs(Runs::Claude(SessionMode::Terminal)),
        BackgroundSurface::Tabs(Runs::Codex(SessionMode::Terminal)),
        BackgroundSurface::Tabs(Runs::OpenCode(SessionMode::Terminal)),
        BackgroundSurface::Sessions(Runs::Claude(SessionMode::Terminal)),
        BackgroundSurface::Sessions(Runs::Codex(SessionMode::Terminal)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut made = None;
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let session = desk.add_session(SessionRecord::new(
                "Working".into(),
                Seat { workspace, project },
                dir.path().into(),
                match surface {
                    BackgroundSurface::Tabs(runs) | BackgroundSurface::Sessions(runs) => runs,
                    _ => Runs::Codex(SessionMode::Terminal),
                },
            ));
            desk.rename_session(session, &"A very long agent task title that must truncate without compressing the activity indicator".repeat(3));
            desk.toggle_fold(dir.path());
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                if matches!(surface, BackgroundSurface::Sessions(_)) {
                    root.settings.column_mode = guild_session::settings::ColumnMode::Sessions;
                }
                if matches!(surface, BackgroundSurface::Tabs(_) | BackgroundSurface::Sessions(_)) {
                    // No provider process: strip identity comes from the session record.
                    root.adopt(
                        Seat { workspace, project },
                        project,
                        crate::Body::Broken {
                            session,
                            reason: "Fixture without a provider process".into(),
                        },
                     cx,);
                }
                root.board.report(session, Some(guild_session::status::Reported::Working));
                root
            });
            let group = Group {
                name: "Repository".into(),
                key: dir.path().into(),
                repository: None,
                plain: false,
                rows: vec![Row::Opened {
                    project,
                    title: crate::column::Title {
                        name: "main".into(),
                        primary: true,
                    },
                    branch: None,
                    committed: None,
                    lines: None,
                }],
            };
            let preview = cx.new(|_| BackgroundActivity {
                root: root.clone(),
                group,
                workspace,
                surface,
                clipped: false,
                renders: renders.clone(),
            });
            made = Some((root, preview.clone(), session));
            gpui_component::Root::new(preview, window, cx)
        });
        let (root, preview, session) = made.unwrap();
        cx.run_until_parked();
        let selector = match surface {
            BackgroundSurface::Tabs(_) => Some("tab-activity-mark"),
            _ => None,
        };
        if let Some(selector) = selector {
            let mark = cx.debug_bounds(selector).expect("working mark is present");
            assert_eq!(
                mark.size,
                gpui::size(px(14.), px(14.)),
                "working {surface:?}: long title or narrow row must not squeeze the ring"
            );
            if matches!(surface, BackgroundSurface::Tabs(_)) {
                let tab = cx.debug_bounds("activity-tab").unwrap();
                let label = cx.debug_bounds("activity-tab-label").unwrap();
                assert!(tab.size.width <= px(270.), "the long tab remains bounded");
                assert!(
                    label.size.width > px(0.) && label.right() < mark.left(),
                    "the truncated title leaves room for the mark"
                );
                assert!(mark.right() <= tab.right(), "the mark stays inside the tab");
            }
        }
        for active in [true, false, true] {
            if active {
                cx.update(|window, _| window.activate_window());
            } else {
                cx.deactivate_window();
            }
            // A production watch can notify the cached region; this fixture draws
            // one caller directly and explicitly invalidates its host after activation.
            preview.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
            assert_eq!(root.read_with(cx, |root, _| root.window_focused), active);
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
            cx.run_until_parked();
            for _ in 0..3 {
                assert!(
                    (1..=2).contains(&crate::view::activity::budget_tests::redraws_in_tick(
                        cx, &renders
                    )),
                    "{surface:?}, active={active}"
                );
                cx.run_until_parked();
            }
        }
        preview.update(cx, |view, cx| {
            view.clipped = true;
            cx.notify();
        });
        cx.run_until_parked();
        crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
        cx.run_until_parked();
        assert_eq!(
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders),
            0,
            "clipped {surface:?}"
        );
        preview.update(cx, |view, cx| {
            view.clipped = false;
            cx.notify();
        });
        if !matches!(
            surface,
            BackgroundSurface::Workspace | BackgroundSurface::Sessions(_)
        ) {
            let shown = root.read_with(cx, |root, _| root.active_workspace);
            root.update(cx, |root, _| {
                root.active_workspace = root.desk.add_workspace("Other", None).unwrap();
            });
            preview.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
            cx.run_until_parked();
            assert_eq!(
                crate::view::activity::budget_tests::redraws_in_tick(cx, &renders),
                0,
                "unshown workspace {surface:?}"
            );
            root.update(cx, |root, _| {
                root.active_workspace = shown;
            });
        }
        cx.update(|_, cx| cx.set_reduce_motion(true));
        preview.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
        cx.run_until_parked();
        assert_eq!(
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders),
            0,
            "reduced motion {surface:?}"
        );
        cx.update(|_, cx| cx.set_reduce_motion(false));
        for signal in [
            None,
            Some(guild_session::status::Reported::Done),
            Some(guild_session::status::Reported::Blocked(Some(
                guild_session::status::Kind::Permission,
            ))),
            Some(guild_session::status::Reported::Error),
        ] {
            root.update(cx, |root, _| {
                root.board = Default::default();
                root.board.report(session, signal);
            });
            preview.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
            cx.run_until_parked();
            assert_eq!(
                crate::view::activity::budget_tests::redraws_in_tick(cx, &renders),
                0,
                "nonworking {surface:?}, {signal:?}"
            );
            if let Some(selector) = selector {
                let expected = match signal {
                    None => None,
                    Some(guild_session::status::Reported::Done) => Some(("activity-dot", 8.)),
                    Some(guild_session::status::Reported::Blocked(_)) => {
                        Some(("ui/activity-question.svg", 16.))
                    }
                    Some(guild_session::status::Reported::Error) => {
                        Some(("ui/activity-failure.svg", 16.))
                    }
                    _ => unreachable!(),
                };
                assert_eq!(
                    cx.debug_bounds(selector).map(|bounds| bounds.size),
                    expected.map(|(_, size)| gpui::size(px(size), px(size))),
                    "{surface:?}, {signal:?}: slot is absent for Idle and fixed otherwise"
                );
                for name in [
                    "activity-dot",
                    "ui/activity-question.svg",
                    "ui/activity-failure.svg",
                ] {
                    let size = expected
                        .filter(|(wanted, _)| *wanted == name)
                        .map(|(_, size)| gpui::size(px(size), px(size)));
                    assert_eq!(
                        cx.debug_bounds(name).map(|bounds| bounds.size),
                        size,
                        "{surface:?}, {signal:?}: actual mark type and size with a long title"
                    );
                }
            }
        }
    }
}
