//! Pointer regressions through the production strip, including its row drag.
use super::*;
use gpui::{AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point};

struct Strip(Entity<Root>);
impl Render for Strip {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |root, cx| {
            div()
                .w(px(600.))
                .child(root.render_tabs(root.focused_area(), cx))
        })
    }
}

struct Fixture {
    root: Entity<Root>,
    strip: Entity<Strip>,
    target: TabId,
    front: TabId,
    _dir: tempfile::TempDir,
}

fn fixture(cx: &mut TestAppContext, compact: bool) -> (Fixture, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let seat = guild_session::Seat { workspace, project };
        let mut ids = Vec::new();
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            root.active_workspace = workspace;
            root.active_project = Some(project);
            for index in 0..2 {
                let body = if compact && index == 0 {
                    Body::Process {
                        run: guild_session::workflow::RunId::new(),
                    }
                } else {
                    Body::Settings {
                        section: crate::view::settings::Section::General,
                    }
                };
                ids.push(root.adopt(seat, project, body, cx));
            }
            root.active_tab = Some(ids[1]);
            root
        });
        let strip = cx.new(|_| Strip(root.clone()));
        made = Some((root, strip.clone(), ids[0], ids[1]));
        gpui_component::Root::new(strip, window, cx)
    });
    cx.run_until_parked();
    let (root, strip, target, front) = made.unwrap();
    (
        Fixture {
            root,
            strip,
            target,
            front,
            _dir: dir,
        },
        cx,
    )
}

fn hover(f: &Fixture, cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::default());
    // TestWindow resets the platform mouse position at frame start. Keep the
    // observed hover while requesting a redraw of this production strip host.
    cx.update(|_, cx| f.strip.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
}

fn mark_center(cx: &mut VisualTestContext) -> gpui::Point<gpui::Pixels> {
    let label = first_label(cx);
    point(label.left() - px(9. + TAB_MARK / 2.), label.center().y)
}

fn first_label(cx: &mut VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
    // Repeated selectors retain the last tab. These two Settings tabs have
    // identical geometry, so step left by one tab to target the background one.
    let mut label = cx.debug_bounds("activity-tab-label").unwrap();
    label.origin.x -= cx.debug_bounds("activity-tab").unwrap().size.width;
    label
}

fn closed(f: &Fixture, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let root = f.root.read(cx);
        assert!(
            !root.tabs.iter().any(|t| t.id == f.target),
            "close must remove the target"
        );
        assert_eq!(
            root.active_tab,
            Some(f.front),
            "close must not select its parent tab"
        );
        assert!(!cx.has_active_drag(), "close must not start a tab drag");
    });
}

#[gpui::test]
fn close_center_after_hover_entry(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let label = first_label(cx);
    let tab = cx.debug_bounds("activity-tab").unwrap();
    let at = mark_center(cx);
    hover(&f, cx, at);
    cx.update(|_, cx| assert_eq!(f.root.read(cx).hovered_tab, Some(f.target)));
    assert_eq!(
        first_label(cx),
        label,
        "hover must not shift or resize the name"
    );
    assert_eq!(cx.debug_bounds("activity-tab").unwrap(), tab);
    cx.simulate_click(at, Modifiers::default());
    closed(&f, cx);
}

#[gpui::test]
fn close_press_before_hover_redraw(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let at = mark_center(cx);
    cx.update(|_, cx| assert_eq!(f.root.read(cx).hovered_tab, None));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    closed(&f, cx);
}

#[gpui::test]
fn close_horizontal_slot_edge(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let at = mark_center(cx) + point(px(12.), px(0.));
    hover(&f, cx, at);
    cx.simulate_click(at, Modifiers::default());
    closed(&f, cx);
}

#[gpui::test]
fn close_release_outside_slot_cancels_without_selecting_or_dragging(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let at = mark_center(cx);
    hover(&f, cx, at);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    let outside = at + point(px(40.), px(40.));
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| {
        assert!(f.root.read(cx).tabs.iter().any(|t| t.id == f.target));
        assert_eq!(f.root.read(cx).active_tab, Some(f.front));
        assert!(!cx.has_active_drag());
    });
}

#[gpui::test]
fn close_reserved_slot_outside_old_circle(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let at = mark_center(cx) + point(px(0.), px(12.));
    hover(&f, cx, at);
    cx.simulate_click(at, Modifiers::default());
    closed(&f, cx);
}

#[gpui::test]
fn close_survives_modest_pointer_motion(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let at = mark_center(cx);
    hover(&f, cx, at);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    let end = at + point(px(5.), px(0.));
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| {
        assert!(
            !cx.has_active_drag(),
            "close press must not arm ancestor drag"
        )
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    closed(&f, cx);
}

#[gpui::test]
fn tab_label_still_selects_and_deliberately_drags(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, false);
    let label = first_label(cx);
    let at = label.center();
    hover(&f, cx, at);
    cx.simulate_click(at, Modifiers::default());
    cx.update(|_, cx| assert_eq!(f.root.read(cx).active_tab, Some(f.target)));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        at + point(px(25.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|_, cx| assert!(cx.has_active_drag(), "label remains a tab drag source"));
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn compact_close_survives_motion_and_logo_still_selects(cx: &mut TestAppContext) {
    let (f, cx) = fixture(cx, true);
    // The first tab starts at CARD_GAP; compact padding is CARD_GAP / 2.
    let logo = point(px(20.), px(18.));
    hover(&f, cx, logo);
    cx.simulate_click(logo, Modifiers::default());
    cx.update(|_, cx| assert_eq!(f.root.read(cx).active_tab, Some(f.target)));
    cx.update(|_, cx| f.root.update(cx, |root, _| root.active_tab = Some(f.front)));
    let close = cx.debug_bounds("compact-process-close").unwrap().center();
    cx.simulate_mouse_down(close, MouseButton::Left, Modifiers::default());
    let end = close + point(px(4.), px(0.));
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| assert!(!cx.has_active_drag(), "compact close must not arm row drag"));
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    closed(&f, cx);
}
