//! Private Pane state exposes the actual list anchor without a production test API.
use super::*;
use gpui::{
    ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase::{self, *},
    VisualTestContext, point, px, size,
};

struct Fixture<'a> {
    _dir: tempfile::TempDir,
    state: Entity<TextViewState>,
    cx: &'a mut VisualTestContext,
}
fn mount<'a>(cx: &'a mut TestAppContext, block: &str) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    let text = format!("{block}\n\n{}", "Paragraph.\n\n".repeat(100));
    std::fs::write(dir.path().join("README.md"), text).unwrap();
    let mut pane = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("README.md"),
                crate::file_tab::Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                crate::i18n::Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        let presentation = Presentation::for_file(&file, Mode::Split, cx).unwrap();
        presentation
            .view
            .update(cx, |p, cx| p.set_mode(Mode::Preview, window, cx));
        pane = Some(presentation.view.clone());
        gpui_component::Root::new(presentation.view, window, cx)
    });
    cx.simulate_resize(size(px(600.), px(300.)));
    cx.run_until_parked();
    let state = cx.update(|_, cx| pane.unwrap().read(cx).state.clone().unwrap());
    Fixture {
        _dir: dir,
        state,
        cx,
    }
}
impl Fixture<'_> {
    fn top(&mut self) -> (usize, gpui::Pixels) {
        self.cx.update(|_, cx| {
            let top = self.state.read(cx).list_state().logical_scroll_top();
            (top.item_ix, top.offset_in_item)
        })
    }
    fn gesture(&mut self, y: f32, phase: TouchPhase, dx: f32, dy: f32) {
        self.cx.simulate_event(ScrollWheelEvent {
            position: point(px(300.), px(y)),
            delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
            touch_phase: phase,
            ..Default::default()
        });
        self.cx.run_until_parked();
    }
    fn select(&mut self, y: f32) -> String {
        let start = point(px(55.), px(y));
        let end = point(px(200.), px(y));
        self.cx
            .simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
        self.cx
            .simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
        self.cx
            .simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
        self.cx.update(|_, cx| self.state.read(cx).selected_text())
    }
}
fn check_block(cx: &mut TestAppContext, block: &str) {
    let mut f = mount(cx, block);
    let (y, baseline) = (25..90)
        .step_by(5)
        .find_map(|y| {
            let selected = f.select(y as f32);
            (!selected.is_empty() && !selected.contains("Paragraph"))
                .then_some((y as f32, selected))
        })
        .expect("select actual visible block text");
    eprintln!("block baseline y={y}: {baseline:?}");
    let before = f.top();
    for (phase, x, dy) in [
        (Started, -150., -3.),
        (Moved, -5., -60.),
        (Ended, -2., -40.),
        (Moved, -4., -50.),
    ] {
        f.gesture(y, phase, x, dy);
        assert_eq!(
            f.top(),
            before,
            "a horizontal block gesture must not scroll the document, even during drift and momentum"
        );
    }
    let shifted = f.select(y);
    assert!(!shifted.is_empty());
    assert_ne!(
        shifted, baseline,
        "the block must actually scroll horizontally"
    );
    // A new vertical gesture has tiny vertical movement to keep the same row
    // visible. Much larger sideways drift must leave its selected text intact.
    for (phase, x, dy) in [
        (Started, -1., -3.),
        (Moved, -150., -1.),
        (Ended, 0., 0.),
        (Moved, -100., -1.),
    ] {
        f.gesture(y, phase, x, dy);
    }
    assert_eq!(
        f.top(),
        (0, px(5.)),
        "vertical gesture moves the page over an overflowing block"
    );
    assert_eq!(
        f.select(y - 5.),
        shifted,
        "vertical gesture must not change horizontal block position"
    );
    f.gesture(y - 5., Started, 10000., 1.);
    assert_eq!(
        f.top(),
        (0, px(5.)),
        "horizontal gesture at an edge must not scroll the page"
    );
    assert_eq!(
        f.select(y - 5.),
        baseline,
        "a later horizontal gesture can reach the block's start"
    );
}
#[gpui::test]
fn markdown_scroll_axis_keeps_code_and_page_independent(cx: &mut TestAppContext) {
    let code = (0..150).map(|i| format!("word{i:03} ")).collect::<String>();
    check_block(cx, &format!("```text\n{code}\n```"));
}
#[gpui::test]
fn markdown_scroll_axis_keeps_table_and_page_independent(cx: &mut TestAppContext) {
    let row = (0..150).map(|i| format!("word{i:03} ")).collect::<String>();
    check_block(cx, &format!("| {row} |\n| --- |\n| value |"));
}
