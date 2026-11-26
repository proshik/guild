//! Disabled navigation does not hide a live status; paint clipping owns visibility.
use super::*;
use gpui::{AppContext, Entity, Render, TestAppContext, prelude::*, px};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

#[derive(Clone, Copy, Debug)]
enum Surface {
    StatusMark,
    Disclosure,
}
struct Preview {
    root: Entity<crate::Root>,
    session: SessionId,
    surface: Surface,
    disabled: bool,
    renders: std::rc::Rc<std::cell::Cell<usize>>,
}
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        self.root.update(cx, |root, cx| {
            if root.activation.is_none() {
                let _ = root.render(window, cx);
            }
            let child =
                match self.surface {
                    Surface::StatusMark => root
                        .menu_mark(&Mark::Status(self.session), false, self.disabled)
                        .unwrap(),
                    Surface::Disclosure => {
                        let record = root.desk.session(self.session).unwrap();
                        let seat = Seat {
                            workspace: record.workspace,
                            project: record.project,
                        };
                        if root.context_menu.is_none() {
                            root.open_menu(
                                Target::WorktreeActivity {
                                    seat,
                                    return_focus: None,
                                },
                                Anchor::Pointer(gpui::point(px(10.), px(10.))),
                                window,
                                cx,
                            );
                        }
                        // The record has no showing pane, so this real disclosure
                        // entry remains disabled while its working status is visible.
                        let entries = root.activity_entries(seat);
                        assert!(entries.iter().any(
                            |entry| matches!(entry, Entry::Choice(choice) if !choice.enabled)
                        ));
                        root.render_context_menu(cx).unwrap()
                    }
                };
            gpui::div().size_full().child(child)
        })
    }
}
#[gpui::test]
fn background_activity_menu_callers_animate_visible_disabled_work(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::background_activity_menu_callers_animate_visible_disabled_work"
    )) {
        return;
    }
    for (surface, disabled) in [
        (Surface::StatusMark, false),
        (Surface::StatusMark, true),
        (Surface::Disclosure, true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut made = None;
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let seat = Seat {
                workspace: desk.active_workspace(),
                project: desk.open_in(desk.active_workspace(), dir.path()),
            };
            let session = desk.add_session(SessionRecord::new(
                "Working".into(),
                seat,
                dir.path().into(),
                Runs::Codex(SessionMode::Terminal),
            ));
            let root = cx.new(|cx| {
                let mut root = crate::Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.board
                    .report(session, Some(guild_session::status::Reported::Working));
                root
            });
            let preview = cx.new(|_| Preview {
                root: root.clone(),
                session,
                surface,
                disabled,
                renders: renders.clone(),
            });
            made = Some((root, preview.clone()));
            gpui_component::Root::new(preview, window, cx)
        });
        let (root, preview) = made.unwrap();
        cx.run_until_parked();
        for active in [true, false, true] {
            if active {
                cx.update(|window, _| window.activate_window());
            } else {
                cx.deactivate_window();
            }
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
                    "{surface:?}, disabled={disabled}, active={active}"
                );
                cx.run_until_parked();
            }
        }
        cx.update(|_, cx| cx.set_reduce_motion(true));
        preview.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        crate::view::activity::budget_tests::redraws_in_tick(cx, &renders);
        cx.run_until_parked();
        assert_eq!(
            crate::view::activity::budget_tests::redraws_in_tick(cx, &renders),
            0
        );
        cx.update(|_, cx| cx.set_reduce_motion(false));
        let session = preview.read_with(cx, |view, _| view.session);
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
        }
    }
}
