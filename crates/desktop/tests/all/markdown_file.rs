//! Pane behavior through the real FileTab, input and Markdown presentation.
use gpui::prelude::*;
use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px};
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    markdown::Presentation,
    theme::Theme,
};
use guild_session::markdown::Mode;

fn pane_with_controls(pane: &Presentation) -> gpui::Div {
    gpui::div()
        .size_full()
        .flex()
        .flex_col()
        .child(pane.controls.clone())
        .child(pane.view.clone())
}
struct PaneHost(Presentation);
impl gpui::Render for PaneHost {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        pane_with_controls(&self.0)
    }
}

struct Fixture<'a> {
    dir: tempfile::TempDir,
    file: Entity<FileTab>,
    pane: Presentation,
    cx: &'a mut VisualTestContext,
}
fn mount<'a>(cx: &'a mut TestAppContext, text: &str, autosave: bool) -> Fixture<'a> {
    mount_named(cx, text, autosave, "README.MD")
}
fn mount_named<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    autosave: bool,
    name: &str,
) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), text).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new(name),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        let pane = Presentation::for_file(&file, Mode::Split, cx).unwrap();
        made = Some((file, pane.clone()));
        let host = cx.new(|_| PaneHost(pane));
        gpui_component::Root::new(host, window, cx)
    });
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    let (file, pane) = made.unwrap();
    Fixture {
        dir,
        file,
        pane,
        cx,
    }
}
impl Fixture<'_> {
    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).expect("mode button");
        self.cx
            .simulate_click(bounds.center(), gpui::Modifiers::default());
    }
    fn text(&mut self) -> String {
        self.cx.update(|_, cx| {
            self.file
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .text()
                .to_string()
        })
    }
    fn rendered(&mut self) -> String {
        self.cx
            .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
        let bounds = self.cx.debug_bounds("markdown-reading").unwrap();
        let padding = self.cx.update(|window, _| {
            guild_desktop::theme::space::PANEL_PADDING.to_pixels(window.rem_size())
        });
        let start = bounds.origin + gpui::point(padding + px(1.), padding + px(1.));
        let end = gpui::point(bounds.right() - padding, bounds.bottom() - padding);
        self.cx
            .simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        self.cx
            .simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        self.cx
            .simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        self.cx.simulate_keystrokes("cmd-c");
        self.cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .unwrap_or_default()
    }
}

#[gpui::test]
fn modes_keep_unsaved_text_undo_caret_and_the_same_input(cx: &mut TestAppContext) {
    let original = "# Original heading\n";
    let mut f = mount(cx, original, false);
    assert_eq!(f.pane.mode(), Mode::Split);
    let source = f.cx.debug_bounds("markdown-source").unwrap();
    let preview = f.cx.debug_bounds("markdown-reading").unwrap();
    assert!(
        source.right() <= preview.left(),
        "source is left of preview"
    );
    let editor = f.cx.update(|window, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    f.cx.simulate_input("New ");
    let edited = f.text();
    assert_ne!(edited, original);
    assert_eq!(f.rendered().trim(), edited.trim());
    let caret = f.cx.update(|_, cx| editor.read(cx).cursor_position());
    f.click("markdown-preview");
    assert_eq!(f.pane.mode(), Mode::Preview);
    assert_eq!(
        f.cx.debug_bounds("markdown-reading").unwrap().size.width,
        px(900.)
    );
    f.cx.update(|window, cx| {
        assert!(
            !editor
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        );
        assert!(f.file.read(cx).is_dirty());
        assert!(
            !f.file
                .read(cx)
                .in_gutter(source.origin + gpui::point(px(14.), px(20.)))
        );
    });
    f.cx.simulate_input("must not edit hidden input");
    assert_eq!(f.text(), edited);
    f.click("markdown-editor");
    assert_eq!(
        f.cx.debug_bounds("markdown-source").unwrap().size.width,
        px(900.)
    );
    f.cx.update(|_, cx| {
        assert_eq!(
            f.file.read(cx).editor().unwrap().entity_id(),
            editor.entity_id()
        );
        assert_eq!(editor.read(cx).cursor_position(), caret);
    });
    f.cx.simulate_keystrokes("cmd-z");
    assert_eq!(f.text(), original);
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("README.MD")).unwrap(),
        original
    );
}

#[gpui::test]
fn disk_reload_and_conflict_follow_the_retained_buffer_in_preview(cx: &mut TestAppContext) {
    let mut f = mount(cx, "# Initial\n", false);
    f.click("markdown-preview");
    std::fs::write(f.dir.path().join("README.MD"), "# Disk update\n").unwrap();
    f.cx.update(|window, cx| f.file.update(cx, |file, cx| file.disk_changed(window, cx)));
    f.cx.run_until_parked();
    assert_eq!(f.rendered().trim(), "Disk update");
    f.click("markdown-editor");
    f.cx.simulate_input("unsaved ");
    let edited = f.text();
    f.click("markdown-preview");
    std::fs::write(f.dir.path().join("README.MD"), "# Conflicting disk\n").unwrap();
    f.cx.update(|window, cx| f.file.update(cx, |file, cx| file.disk_changed(window, cx)));
    f.cx.run_until_parked();
    assert_eq!(f.rendered().trim(), edited.trim());
    f.cx.update(|_, cx| {
        assert_eq!(
            f.file.read(cx).mark(),
            Some(guild_desktop::file_tab::Mark::Conflict)
        )
    });
}

#[gpui::test]
fn switching_to_preview_does_not_trigger_blur_autosave(cx: &mut TestAppContext) {
    let mut f = mount(cx, "# Original\n", true);
    f.cx.update(|window, cx| window.focus(&f.file.read(cx).focus_handle(cx), cx));
    f.cx.simulate_input("Unsaved ");
    f.click("markdown-preview");
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("README.MD")).unwrap(),
        "# Original\n"
    );
}

#[gpui::test]
fn presentation_is_independent_even_when_workspaces_share_a_buffer(cx: &mut TestAppContext) {
    let mut f = mount(cx, "# Shared\n", false);
    let other =
        f.cx.update(|_, cx| Presentation::for_file(&f.file, Mode::Split, cx).unwrap());
    f.click("markdown-preview");
    assert_eq!(other.mode(), Mode::Split);
    f.cx.update(|_, cx| other.restore(Mode::Editor, cx));
    assert_eq!(f.pane.mode(), Mode::Preview);
    assert_eq!(other.mode(), Mode::Editor);
}

#[gpui::test]
fn rapid_edits_and_mode_changes_render_only_the_latest_buffer(cx: &mut TestAppContext) {
    let mut f = mount(cx, "# Zero\n", false);
    let editor =
        f.cx.update(|_, cx| f.file.read(cx).editor().unwrap().clone());
    for i in 0..12 {
        f.cx.update(|window, cx| {
            editor.update(cx, |e, cx| {
                e.replace_all(format!("# Revision {i}\n"), window, cx)
            })
        });
        f.click(if i % 2 == 0 {
            "markdown-editor"
        } else {
            "markdown-split"
        });
    }
    f.click("markdown-preview");
    f.cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    f.cx.run_until_parked();
    assert_eq!(f.rendered().trim(), "Revision 11");
    assert_eq!(f.text(), "# Revision 11\n");
}

#[gpui::test]
fn large_preview_falls_back_to_a_visible_editor_and_narrow_controls_remain_reachable(
    cx: &mut TestAppContext,
) {
    let text = "text\n".repeat(guild_desktop::markdown::PREVIEW_LIMIT / 5 + 1);
    let mut f = mount(cx, &text, false);
    f.click("markdown-preview");
    assert!(f.cx.debug_bounds("markdown-source").is_some());
    assert!(f.cx.debug_bounds("markdown-reading").is_none());
    f.cx.simulate_resize(gpui::size(px(180.), px(180.)));
    for selector in ["markdown-editor", "markdown-split", "markdown-preview"] {
        let bounds = f.cx.debug_bounds(selector).unwrap();
        assert!(bounds.left() >= px(0.) && bounds.right() <= px(180.));
    }
    f.cx.update(|window, cx| {
        assert!(
            f.file
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        )
    });
}

#[gpui::test]
fn empty_and_unclosed_markdown_remain_editable(cx: &mut TestAppContext) {
    let mut f = mount(cx, "", false);
    f.click("markdown-preview");
    f.click("markdown-editor");
    f.cx.simulate_input("```rust\nfn unfinished() {\n[incomplete");
    let text = f.text();
    f.click("markdown-split");
    assert_eq!(f.text(), text);
    assert!(f.cx.debug_bounds("markdown-source").is_some());
}

#[gpui::test]
fn latest_rendered_text_survives_unrelated_frames_and_mode_roundtrip(cx: &mut TestAppContext) {
    let mut f = mount(cx, "old paragraph", false);
    f.cx.update(|window, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        for i in 0..8 {
            editor.update(cx, |editor, cx| {
                editor.replace_all(format!("new paragraph {i}"), window, cx)
            });
        }
    });
    f.click("markdown-preview");
    for _ in 0..3 {
        f.cx.update(|_, cx| f.pane.view.update(cx, |_, cx| cx.notify()));
    }
    f.click("markdown-editor");
    f.click("markdown-preview");
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    let padding = f.cx.update(|window, _| {
        guild_desktop::theme::space::PANEL_PADDING.to_pixels(window.rem_size())
    });
    let y = bounds.top() + padding + px(8.);
    f.cx.simulate_mouse_down(
        gpui::point(bounds.left() + padding + px(1.), y),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    f.cx.simulate_mouse_move(
        gpui::point(bounds.right() - px(30.), y),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    f.cx.simulate_mouse_up(
        gpui::point(bounds.right() - px(30.), y),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    f.cx.simulate_keystrokes("cmd-c");
    f.cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("new paragraph 7")
        )
    });
}

#[gpui::test]
fn read_failures_keep_mode_controls_and_the_existing_explanation(cx: &mut TestAppContext) {
    let mut f = mount(cx, "binary\0data", false);
    f.click("markdown-preview");
    assert!(f.cx.debug_bounds("markdown-source").is_some());
    assert!(f.cx.debug_bounds("markdown-reading").is_none());
    f.cx.update(|_, cx| {
        assert_eq!(
            *f.file.read(cx).state(),
            guild_desktop::file_tab::FileState::NotText
        )
    });
    std::fs::remove_file(f.dir.path().join("README.MD")).unwrap();
    f.cx.update(|window, cx| f.file.update(cx, |file, cx| file.disk_changed(window, cx)));
    f.cx.run_until_parked();
    f.click("markdown-editor");
    assert!(f.cx.debug_bounds("markdown-source").is_some());
}

#[gpui::test]
fn ordinary_source_has_no_markdown_presentation(cx: &mut TestAppContext) {
    let f = mount(cx, "# Document", false);
    std::fs::write(f.dir.path().join("main.rs"), "fn main() {}").unwrap();
    f.cx.update(|window, cx| {
        let file = cx.new(|cx| {
            FileTab::open(
                f.dir.path(),
                std::path::Path::new("main.rs"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        assert!(Presentation::for_file(&file, Mode::Split, cx).is_none());
    });
}

struct CommandHost {
    pane: Presentation,
    file: Entity<FileTab>,
    closed: bool,
}
impl gpui::Render for CommandHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div()
            .size_full()
            .key_context("guild")
            .on_action(
                cx.listener(|this, _: &guild_desktop::actions::SaveFile, _, cx| {
                    this.file.update(cx, |file, cx| file.save(cx)).unwrap();
                }),
            )
            .on_action(
                cx.listener(|this, _: &guild_desktop::actions::CloseTab, _, cx| {
                    this.closed = true;
                    cx.notify();
                }),
            )
            .child(self.pane.view.clone())
    }
}
#[gpui::test]
fn preview_focus_keeps_save_and_close_actions_reachable(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "original").unwrap();
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-s", guild_desktop::actions::SaveFile, Some("guild")),
            gpui::KeyBinding::new("cmd-w", guild_desktop::actions::CloseTab, Some("guild")),
        ]);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("README.md"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        let pane = Presentation::for_file(&file, Mode::Split, cx).unwrap();
        let view = cx.new(|_| CommandHost {
            pane,
            file,
            closed: false,
        });
        host = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = host.as_ref().unwrap().read(cx);
        let file = view.file.clone();
        let pane = view.pane.clone();
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |e, cx| e.replace_all("changed", window, cx));
        pane.view
            .update(cx, |p, cx| p.set_mode(Mode::Preview, window, cx));
    });
    cx.simulate_resize(gpui::size(px(600.), px(300.)));
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "changed"
    );
    cx.simulate_keystrokes("cmd-w");
    cx.update(|_, cx| assert!(host.unwrap().read(cx).closed));
}

#[gpui::test]
fn restoring_preview_never_focuses_the_hidden_lazy_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("restored.md"), "restored").unwrap();
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::restore(
                dir.path(),
                std::path::Path::new("restored.md"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                cx,
            )
        });
        let pane = Presentation::for_file(&file, Mode::Split, cx).unwrap();
        pane.restore(Mode::Preview, cx);
        window.focus(&pane.view.read(cx).focus_handle(cx), cx);
        saved = Some((file, pane.clone()));
        let host = cx.new(|_| PaneHost(pane));
        gpui_component::Root::new(host, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(300.)));
    cx.simulate_input("hidden edit");
    cx.update(|window, cx| {
        let (file, pane) = saved.unwrap();
        let editor = file.read(cx).editor().unwrap().read(cx);
        assert_eq!(editor.text().to_string(), "restored");
        assert!(!editor.focus_handle(cx).contains_focused(window, cx));
        assert_eq!(pane.mode(), Mode::Preview);
    });
}

#[gpui::test]
fn preview_fallback_does_not_suppress_a_later_real_blur_save(cx: &mut TestAppContext) {
    let original = format!("Title\n\n```\n{}\n```", "x".repeat(2100));
    let mut f = mount(cx, &original, true);
    f.cx.update(|window, cx| window.focus(&f.file.read(cx).focus_handle(cx), cx));
    f.cx.simulate_input("edited ");
    let edited = f.text();
    f.click("markdown-preview");
    f.cx.update(|window, cx| {
        assert!(
            f.file
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx),
            "fallback leaves source focused"
        );
        window.blur(cx);
    });
    // Deliver the component's public Blur event at FileTab's subscription boundary.
    // The test platform does not dispatch native window-focus transitions.
    f.cx.update(|_, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        editor.update(cx, |_, cx| cx.emit(gpui_component::input::InputEvent::Blur));
    });
    f.cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("README.MD")).unwrap(),
        edited
    );
}

#[gpui::test]
fn ordinary_uneven_and_cyrillic_tables_do_not_hit_the_preview_limit(cx: &mut TestAppContext) {
    let text = format!(
        "| Description | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n| {} | b | c | d | e | f |\n\n| Описание | Б | В |\n| --- | --- | --- |\n| {} | б | в |",
        "x".repeat(340),
        "я".repeat(350)
    );
    let mut f = mount(cx, &text, false);
    f.click("markdown-preview");
    assert!(
        f.cx.debug_bounds("markdown-reading").is_some(),
        "ordinary tables stay readable"
    );
    assert_eq!(
        f.cx.debug_bounds("markdown-reading").unwrap().size.width,
        px(900.)
    );
}

fn copy_preview_band(f: &mut Fixture<'_>) -> String {
    f.cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    let padding = f.cx.update(|window, _| {
        guild_desktop::theme::space::PANEL_PADDING.to_pixels(window.rem_size())
    });
    let start = gpui::point(
        bounds.left() + padding + px(1.),
        bounds.top() + padding + px(80.),
    );
    let end = gpui::point(bounds.right() - padding - px(1.), start.y + px(60.));
    f.cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_keystrokes("cmd-c");
    f.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default()
}
#[gpui::test]
fn editing_keeps_the_preview_at_its_reading_position(cx: &mut TestAppContext) {
    let source = (0..100)
        .map(|i| format!("Paragraph {i:03} remains readable.\n\n"))
        .collect::<String>();
    let mut f = mount(cx, &source, false);
    let initial = copy_preview_band(&mut f);
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    f.cx.simulate_event(gpui::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-700.))),
        ..Default::default()
    });
    let before = copy_preview_band(&mut f);
    assert!(!before.is_empty());
    assert_ne!(
        before, initial,
        "scroll must actually change the visible text"
    );
    f.cx.update(|window, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        editor.update(cx, |e, cx| {
            e.replace_all(format!("{source}New final paragraph."), window, cx)
        });
    });
    f.cx.run_until_parked();
    assert_eq!(
        copy_preview_band(&mut f),
        before,
        "an edit at the end must not jump the reading position"
    );
}

struct TwoPanes {
    first: Presentation,
    second: Presentation,
}
impl gpui::Render for TwoPanes {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div()
            .size_full()
            .flex()
            .child(
                gpui::div()
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .h_full()
                    .child(pane_with_controls(&self.first)),
            )
            .child(
                gpui::div()
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .h_full()
                    .child(pane_with_controls(&self.second)),
            )
    }
}
#[gpui::test]
fn two_visible_files_keep_modes_and_keyboard_targets_independent(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["first.md", "second.MARKDOWN"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let mut files = Vec::new();
        let mut panes = Vec::new();
        for name in ["first.md", "second.MARKDOWN"] {
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    std::path::Path::new(name),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            panes.push(Presentation::for_file(&file, Mode::Split, cx).unwrap());
            files.push(file);
        }
        made = Some((files, panes.clone()));
        let view = cx.new(|_| TwoPanes {
            first: panes[0].clone(),
            second: panes[1].clone(),
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    let left_preview =
        cx.debug_bounds("markdown-preview").unwrap().center() - gpui::point(px(450.), px(0.));
    let right_editor = cx.debug_bounds("markdown-editor").unwrap().center();
    let (files, panes) = made.unwrap();
    cx.simulate_click(left_preview, gpui::Modifiers::default());
    assert_eq!(panes[0].mode(), Mode::Preview);
    assert_eq!(panes[1].mode(), Mode::Split);
    cx.simulate_input("hidden input");
    cx.simulate_click(right_editor, gpui::Modifiers::default());
    cx.simulate_input("edited ");
    assert_eq!(panes[0].mode(), Mode::Preview);
    assert_eq!(panes[1].mode(), Mode::Editor);
    cx.update(|_, cx| {
        assert_eq!(
            files[0]
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .text()
                .to_string(),
            "first.md"
        );
        assert_eq!(
            files[1]
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .text()
                .to_string(),
            "edited second.MARKDOWN"
        );
    });
}

#[gpui::test]
fn missing_images_leave_surrounding_text_and_editor_available(cx: &mut TestAppContext) {
    let mut f = mount(
        cx,
        "Before the image.\n\n![Missing](does-not-exist.png)\n\nAfter the image.",
        false,
    );
    f.click("markdown-preview");
    f.cx.run_until_parked();
    let rendered = f.rendered();
    assert!(rendered.contains("Before the image."));
    assert!(rendered.contains("After the image."));
    f.click("markdown-editor");
    f.cx.simulate_input("Still editable. ");
    assert!(f.text().starts_with("Still editable. "));
}

fn wrapping_selection(f: &mut Fixture<'_>, offset: f32) -> String {
    f.cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    let padding = f.cx.update(|window, _| {
        guild_desktop::theme::space::PANEL_PADDING.to_pixels(window.rem_size())
    });
    let start = bounds.origin + gpui::point(padding + px(1.), padding + px(2. + offset));
    let end = gpui::point(bounds.right() - padding - px(1.), start.y + px(60.));
    f.cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    f.cx.simulate_keystrokes("cmd-c");
    f.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default()
}

fn assert_prose_wraps_independently(cx: &mut TestAppContext, narrow: &str, full: &str) {
    let mut f = mount(cx, narrow, false);
    f.click("markdown-preview");
    f.cx.simulate_resize(gpui::size(px(600.), px(600.)));
    // Below an opening heading, so the selection lands in the prose.
    let offset = if narrow.starts_with('#') { 100. } else { 0. };
    let baseline = wrapping_selection(&mut f, offset);
    let narrow_bounds = f.cx.debug_bounds("markdown-document").unwrap();
    assert!(
        !baseline.is_empty(),
        "the control must select actual rendered prose"
    );
    f.cx.update(|window, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        editor.update(cx, |e, cx| e.replace_all(full.to_owned(), window, cx));
    });
    f.cx.run_until_parked();
    let mixed = wrapping_selection(&mut f, offset);
    let full_bounds = f.cx.debug_bounds("markdown-document").unwrap();
    let viewport = f.cx.debug_bounds("markdown-reading").unwrap();
    eprintln!(
        "viewport={viewport:?}; prose-only={narrow_bounds:?}; full={full_bounds:?}; control selection={baseline:?}; full selection={mixed:?}"
    );
    assert_eq!(viewport.size.width, px(600.));
    assert_eq!(
        mixed, baseline,
        "wide blocks must not change the preceding prose wrapping/selection at the same viewport coordinates"
    );
    assert_eq!(
        full_bounds.size.width, narrow_bounds.size.width,
        "the prose document must remain viewport-width; wide blocks scroll independently"
    );
}

#[gpui::test]
fn mixed_wide_blocks_preserve_prose_wrapping_at_600px(cx: &mut TestAppContext) {
    let full = include_str!("../fixtures/markdown-wrapping.md");
    let (paragraph, _) = full.split_once("\n\n").unwrap();
    assert_prose_wraps_independently(cx, paragraph, full);
}

#[gpui::test]
fn guild_readme_preserves_prose_wrapping_at_600px(cx: &mut TestAppContext) {
    let readme = include_str!("../../../../README.md");
    // The README opens with a centred header of images and badges; its prose
    // starts at the first section, which wide tables and code blocks follow.
    let (_, body) = readme.split_once("\n## What is Guild\n").unwrap();
    let full = format!("## What is Guild\n{body}");
    let (intro, _) = full.split_once("\n## ").unwrap();
    assert_prose_wraps_independently(cx, intro, &full);
}

#[gpui::test]
fn mixed_wide_blocks_keep_both_tails_reachable_in_real_pane(cx: &mut TestAppContext) {
    let full = include_str!("../fixtures/markdown-wrapping.md");
    let mut f = mount(cx, full, false);
    f.click("markdown-preview");
    f.cx.simulate_resize(gpui::size(px(600.), px(900.)));
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    let mut code = false;
    let mut table = false;
    for _ in 0..28 {
        for y in (54..850).step_by(12) {
            f.cx.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()))
            });
            // Send the gesture over each block, not over the surrounding prose.
            f.cx.simulate_event(gpui::ScrollWheelEvent {
                position: gpui::point(bounds.center().x, px(y as f32)),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(-100.), px(0.))),
                ..Default::default()
            });
            let start = gpui::point(px(19.), px(y as f32));
            let end = gpui::point(px(580.), start.y);
            f.cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
            f.cx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::default());
            f.cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
            f.cx.simulate_keystrokes("cmd-c");
            let copied =
                f.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
                    .unwrap_or_default();
            code |= copied.contains("CODETAIL");
            table |= copied.contains("TABLETAIL");
        }
        if code && table {
            break;
        }
        f.cx.simulate_event(gpui::ScrollWheelEvent {
            position: bounds.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-100.), px(0.))),
            ..Default::default()
        });
    }
    eprintln!("mixed real Pane: code tail reachable={code}; table tail reachable={table}");
    assert!(
        code && table,
        "both wide block tails must remain selectable through actual horizontal scrolling"
    );
}

#[gpui::test]
fn code_copy_action_copies_the_block_without_editing_the_file(cx: &mut TestAppContext) {
    let source = "Before.\n\n```rust\nfn main() {}\n```\n\nAfter.";
    let mut f = mount(cx, source, false);
    f.click("markdown-preview");
    f.click("markdown-copy-code");
    f.cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap().trim(),
            "fn main() {}"
        );
        assert!(!f.file.read(cx).is_dirty());
    });
    assert_eq!(f.text(), source);
    assert_eq!(f.pane.mode(), Mode::Preview);
}

#[gpui::test]
fn preview_content_does_not_reserve_a_second_toolbar_row(cx: &mut TestAppContext) {
    let mut f = mount(cx, "A paragraph.", false);
    f.click("markdown-preview");
    // Mount only the actual content view, as the area does beneath its tab strip.
    let pane = f.pane.view.clone();
    f.cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| gpui_component::Root::new(pane, window, cx));
    });
    f.cx.run_until_parked();
    let reading = f.cx.debug_bounds("markdown-reading").unwrap();
    assert_eq!(
        reading.top(),
        px(0.),
        "mode controls belong to the tab strip"
    );
    assert_eq!(reading.size.height, px(600.));
}

#[gpui::test]
fn markdown_editor_wraps_visually_without_changing_buffer_lines(cx: &mut TestAppContext) {
    use gpui_component::input::Position;
    let original = format!(
        "{}\nnext line\n",
        "Readable documentation words. ".repeat(18)
    );
    let mut f = mount(cx, &original, false);
    f.click("markdown-editor");
    f.cx.simulate_resize(gpui::size(px(600.), px(600.)));
    let editor =
        f.cx.update(|_, cx| f.file.read(cx).editor().unwrap().clone());
    f.cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(0, 0), window, cx);
        })
    });
    f.cx.run_until_parked();
    let first =
        f.cx.update(|_, cx| editor.read(cx).cursor_layout().unwrap().0);
    f.cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(0, 100), window, cx);
        })
    });
    f.cx.run_until_parked();
    f.cx.update(|_, cx| {
        let state = editor.read(cx);
        let caret = state.cursor_layout().unwrap().0;
        assert!(
            caret.top() > first.top(),
            "long Markdown prose wraps onto another visual row"
        );
        assert!(caret.right() <= state.input_bounds().right());
        assert_eq!(state.cursor_position(), Position::new(0, 100));
    });
    assert_eq!(f.text(), original);
    f.cx.simulate_keystrokes("cmd-a cmd-c");
    f.cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), original));
}

#[gpui::test]
fn markdown_syntax_reaches_the_component_highlighter(cx: &mut TestAppContext) {
    use gpui_component::{Theme as ComponentTheme, highlighter::SyntaxHighlighter, input::Rope};
    cx.update(|cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::editor::apply_theme(&Theme::default(), 14., cx);
        let text = "# Heading\n\n**bold** and *italic* with `code` and [link](https://example.org).\n\n> quoted words\n";
        let mut highlighter = SyntaxHighlighter::new("markdown");
        highlighter.update(None, &Rope::from(text), None);
        let theme = &ComponentTheme::global(cx).highlight_theme;
        let styles = highlighter.styles(&(0..text.len()), theme.as_ref());
        for word in ["Heading", "bold", "italic", "code", "https://example.org", "quoted words"] {
            let at = text.find(word).unwrap();
            assert!(styles.iter().any(|(range, style)| range.contains(&at) && *style != gpui::HighlightStyle::default()), "{word} has no syntax style: {styles:?}");
        }
        let at = text.find("bold").unwrap();
        assert!(styles.iter().any(|(range, style)| range.contains(&at) && style.font_weight == Some(gpui::FontWeight::BOLD)));
        let at = text.find("italic").unwrap();
        assert!(styles.iter().any(|(range, style)| range.contains(&at) && style.font_style == Some(gpui::FontStyle::Italic)));

    });
}

#[gpui::test]
fn offscreen_code_actions_do_not_participate_in_frame_layout(cx: &mut TestAppContext) {
    let source = format!(
        "{}\n```sh\necho offscreen\n```",
        "A separate paragraph.\n\n".repeat(200)
    );
    let mut f = mount(cx, &source, false);
    f.click("markdown-preview");
    assert!(
        f.cx.debug_bounds("markdown-copy-code").is_none(),
        "offscreen blocks must not be laid out on ordinary frames"
    );
}

#[gpui::test]
#[ignore = "a measurement, run by hand"]
fn preview_frame_work_measurement(cx: &mut TestAppContext) {
    // Diagnostic evidence only: no machine-dependent timing assertion.
    let source = std::env::var("GUILD_MARKDOWN_BENCH_PATH")
        .ok()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .unwrap_or_else(|| include_str!("../../../../README.md").to_owned());
    eprintln!("Measurement source: {} bytes", source.len());
    let mut f = mount(cx, &source, false);
    for mode in ["markdown-preview", "markdown-split"] {
        f.click(mode);
        let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
        let mut samples = Vec::new();
        for _ in 0..30 {
            let start = std::time::Instant::now();
            f.cx.simulate_event(gpui::ScrollWheelEvent {
                position: bounds.center(),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-15.))),
                ..Default::default()
            });
            samples.push(start.elapsed().as_micros());
        }
        samples.sort();
        eprintln!(
            "Markdown real Pane {mode}: 30 scroll dispatch/draw samples, median={}us p95={}us max={}us; test platform, debug_assertions={}",
            samples[15],
            samples[28],
            samples[29],
            cfg!(debug_assertions)
        );
    }
}

#[gpui::test]
fn copy_preserves_newlines_and_offscreen_text_for_each_block(cx: &mut TestAppContext) {
    let first = format!("echo {}TAIL\nprintf 'second line'\n\n", "x".repeat(500));
    let second = "print('other block')\n";
    let source = format!("```sh\n{first}```\n\n```python\n{second}```");
    let mut f = mount(cx, &source, false);
    f.click("markdown-preview");
    f.cx.simulate_resize(gpui::size(px(260.), px(600.)));
    // Every action is scoped by its parent code block. Probe both visible rows
    // using native event dispatch; do not identify a block by its language.
    let mut copied = std::collections::HashSet::new();
    for y in (55..350).step_by(3) {
        f.cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
        f.cx.simulate_click(
            gpui::point(px(220.), px(y as f32)),
            gpui::Modifiers::default(),
        );
        let text =
            f.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
                .unwrap_or_default();
        if !text.is_empty() {
            copied.insert(text);
        }
    }
    // The newline separating code from its closing fence is Markdown syntax;
    // the preceding blank line remains part of the copied code.
    assert!(
        copied.contains(first.strip_suffix('\n').unwrap()),
        "whole first block including trailing newlines: {copied:?}"
    );
    assert!(
        copied.contains(second.strip_suffix('\n').unwrap()),
        "second action must copy its own block: {copied:?}"
    );
    assert_eq!(f.text(), source);
}

#[gpui::test]
fn inline_images_keep_logical_dimensions_at_retina_scale(cx: &mut TestAppContext) {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut f = mount(cx, "Initial", false);
    f.click("markdown-preview");
    for scale in [1., 2.] {
        f.cx.simulate_scale_factor_change(scale);
        for (image, width, height, html) in [
            ("markdown-badge.svg", 100., 20., false),
            ("markdown-badge.svg", 100., 20., true),
            ("markdown-raster.png", 80., 40., false),
            ("markdown-illustration.svg", 564., 141., false),
        ] {
            let uri = gpui::http_client::Url::from_file_path(fixtures.join(image)).unwrap();
            let source = if html {
                format!("# Heading <img src=\"{uri}\" alt=\"\"> tail")
            } else {
                format!("# Heading ![picture]({uri}) tail")
            };
            f.cx.update(|window, cx| {
                let editor = f.file.read(cx).editor().unwrap().clone();
                editor.update(cx, |editor, cx| editor.replace_all(source, window, cx));
            });
            f.cx.run_until_parked();
            f.cx.simulate_resize(gpui::size(px(600.), px(700.)));
            f.cx.run_until_parked();
            let bounds =
                f.cx.debug_bounds("markdown-inline-image")
                    .expect("loaded inline image");
            assert_eq!(
                bounds.size,
                gpui::size(px(width), px(height)),
                "{image}, scale {scale}"
            );
        }
    }
}

#[gpui::test]
fn replacing_tail_and_changing_theme_keep_the_reading_anchor(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::editor::apply_theme(&Theme::default(), 14., cx);
    });
    let source = (0..100)
        .map(|i| format!("Paragraph {i:03} remains readable.\n\n"))
        .collect::<String>();
    let mut f = mount(cx, &source, false);
    f.click("markdown-preview");
    let bounds = f.cx.debug_bounds("markdown-reading").unwrap();
    f.cx.simulate_event(gpui::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-700.))),
        ..Default::default()
    });
    let before = copy_preview_band(&mut f);
    assert!(!before.is_empty());
    f.cx.update(|window, cx| {
        let editor = f.file.read(cx).editor().unwrap().clone();
        editor.update(cx, |e, cx| {
            e.replace_all(
                source.replace(
                    "Paragraph 099 remains readable.",
                    "Replaced tail.\n\nExtra block.",
                ),
                window,
                cx,
            )
        });
    });
    f.cx.run_until_parked();
    assert_eq!(
        copy_preview_band(&mut f),
        before,
        "replacement changing block count must preserve anchor"
    );
    for kind in [
        guild_desktop::theme::Kind::Light,
        guild_desktop::theme::Kind::Dark,
    ] {
        f.cx.update(|_, cx| {
            let theme = Theme::of(kind);
            guild_desktop::editor::apply_theme(&theme, 14., cx);
            f.file.update(cx, |file, cx| {
                file.restyle(
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme,
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    cx,
                )
            });
        });
        f.cx.run_until_parked();
        assert_eq!(
            copy_preview_band(&mut f),
            before,
            "theme must preserve anchor"
        );
        f.cx.update(|_, cx| {
            assert_eq!(
                gpui_component::Theme::global(cx).colors.selection,
                gpui::Hsla::from(gpui::rgb(Theme::of(kind).accent.brass_bg)),
                "preview selection overlay must not change source or sibling selection colors"
            );
        });
    }
}

#[gpui::test]
fn remote_badge_retains_intrinsic_size_after_loading(cx: &mut TestAppContext) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/badge.svg", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 2048];
        let _ = socket.read(&mut request);
        let body = include_bytes!("../fixtures/markdown-badge.svg");
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        socket.write_all(body).unwrap();
    });
    let mut f = mount(cx, &format!("# Heading ![badge]({url}) tail"), false);
    f.click("markdown-preview");
    f.cx.run_until_parked();
    f.cx.update(|window, cx| window.simulate_next_frame(cx));
    f.cx.run_until_parked();
    server.join().unwrap();
    let before =
        f.cx.debug_bounds("markdown-inline-image")
            .expect("remote image loaded");
    assert_eq!(before.size, gpui::size(px(100.), px(20.)));
    // The server is gone: a mode/theme repaint must use the document cache.
    f.click("markdown-editor");
    f.click("markdown-preview");
    f.cx.run_until_parked();
    assert_eq!(
        f.cx.debug_bounds("markdown-inline-image").unwrap().size,
        before.size
    );
}

#[gpui::test]
fn nested_illustrations_fit_the_visible_content_column(cx: &mut TestAppContext) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/markdown-illustration.svg");
    let uri = gpui::http_client::Url::from_file_path(path).unwrap();
    let mut f = mount(cx, "Initial", false);
    f.click("markdown-preview");
    for prefix in ["- ", "> ", "> - ", "- outer\n  - "] {
        f.cx.update(|window, cx| {
            let editor = f.file.read(cx).editor().unwrap().clone();
            editor.update(cx, |e, cx| {
                e.replace_all(format!("{prefix}![image]({uri})"), window, cx)
            });
        });
        f.cx.run_until_parked();
        for width in [600., 360., 900.] {
            f.cx.simulate_resize(gpui::size(px(width), px(700.)));
            f.cx.run_until_parked();
            let reading = f.cx.debug_bounds("markdown-document").unwrap();
            let image = f.cx.debug_bounds("markdown-inline-image").unwrap();
            assert!(
                image.right() <= reading.right(),
                "{prefix:?}: {image:?} outside {reading:?}"
            );
            assert!(
                (image.size.width - image.size.height * 4.).abs() <= px(4.),
                "aspect ratio survives pixel rounding"
            );
            assert!(
                image.size.width > px(width * 0.7),
                "widening must restore the illustration"
            );
        }
    }
}

#[gpui::test]
fn html_image_blocks_keep_intrinsic_size_and_alignment(cx: &mut TestAppContext) {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/markdown-badge.svg");
    let uri = gpui::http_client::Url::from_file_path(path).unwrap();
    let mut f = mount(cx, "Initial", false);
    f.click("markdown-preview");
    for source in [
        format!("<img src=\"{uri}\" alt=\"badge\">"),
        format!("<p align=\"center\"><img src=\"{uri}\" alt=\"badge\"></p>"),
    ] {
        f.cx.update(|window, cx| {
            let editor = f.file.read(cx).editor().unwrap().clone();
            editor.update(cx, |e, cx| e.replace_all(source.clone(), window, cx));
        });
        f.cx.run_until_parked();
        let image =
            f.cx.debug_bounds("markdown-inline-image")
                .expect("HTML block image loaded");
        assert_eq!(image.size, gpui::size(px(100.), px(20.)));
        if source.contains("center") {
            let reading = f.cx.debug_bounds("markdown-document").unwrap();
            assert!((image.center().x - reading.center().x).abs() < px(1.));
        }
    }
}

#[gpui::test]
fn animated_images_keep_requesting_frames(cx: &mut TestAppContext) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/markdown-animation.gif");
    let uri = gpui::http_client::Url::from_file_path(path).unwrap();
    let mut f = mount(cx, &format!("![animation]({uri})"), false);
    f.click("markdown-preview");
    f.cx.run_until_parked();
    f.cx.update(|window, _| window.activate_window());
    let mut frames = 0;
    for _ in 0..5 {
        frames = f.cx.update(|window, cx| window.simulate_next_frame(cx));
        f.cx.run_until_parked();
    }
    assert!(f.cx.debug_bounds("markdown-inline-image").is_some());
    assert!(
        frames > 0,
        "a loaded visible animation must keep requesting frames"
    );
}

#[gpui::test]
fn file_wrap_toggle_preserves_text_and_survives_reload(cx: &mut TestAppContext) {
    use gpui_component::input::Position;
    let original = format!("{}\n", "Readable documentation words. ".repeat(18));
    let mut f = mount(cx, &original, false);
    f.click("markdown-editor");
    f.cx.simulate_resize(gpui::size(px(600.), px(600.)));
    let editor =
        f.cx.update(|_, cx| f.file.read(cx).editor().unwrap().clone());
    for (index, wrapped) in [true, false, true, false].into_iter().enumerate() {
        if index > 0 {
            f.cx.update(|window, cx| {
                f.file
                    .update(cx, |file, cx| file.toggle_soft_wrap(window, cx))
            });
        }
        f.cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                state.set_cursor_position(Position::new(0, 0), window, cx)
            })
        });
        f.cx.run_until_parked();
        let top =
            f.cx.update(|_, cx| editor.read(cx).cursor_layout().unwrap().0.top());
        f.cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                state.set_cursor_position(Position::new(0, 100), window, cx)
            })
        });
        f.cx.run_until_parked();
        f.cx.update(|_, cx| {
            assert_eq!(
                editor.read(cx).cursor_layout().unwrap().0.top() > top,
                wrapped,
                "toggle changes visual wrapping"
            );
            assert!(!f.file.read(cx).is_dirty());
        });
        assert_eq!(f.text(), original);
        assert_eq!(
            std::fs::read(f.dir.path().join("README.MD")).unwrap(),
            original.as_bytes()
        );
        f.cx.update(|window, cx| f.file.update(cx, |file, cx| file.reload(window, cx)));
    }
}

#[gpui::test]
fn standalone_diagrams_keep_one_unsaved_editor_across_all_modes(cx: &mut TestAppContext) {
    for name in ["flow.MMD", "flow.mermaid", "flow.PUML", "flow.plantuml"] {
        let original = "Alice -> Bob: hello\n";
        let mut f = mount_named(cx, original, false, name);
        assert_eq!(f.pane.mode(), Mode::Editor);
        let editor = f.cx.update(|window, cx| {
            let editor = f.file.read(cx).editor().unwrap().clone();
            window.focus(&editor.read(cx).focus_handle(cx), cx);
            editor
        });
        f.cx.simulate_input("unsaved ");
        let changed = f.text();
        for mode in ["markdown-split", "markdown-preview", "markdown-editor"] {
            f.click(mode);
            if mode != "markdown-editor" {
                assert!(f.cx.debug_bounds("diagram-resources-unavailable").is_some());
            }
            assert_eq!(f.text(), changed);
            f.cx.update(|_, cx| {
                assert_eq!(
                    f.file.read(cx).editor().unwrap().entity_id(),
                    editor.entity_id()
                )
            });
        }
        f.cx.simulate_keystrokes("cmd-z");
        assert_eq!(f.text(), original);
        assert_eq!(
            std::fs::read_to_string(f.dir.path().join(name)).unwrap(),
            original
        );
    }
}

#[gpui::test]
#[ignore = "requires explicit prepared bundled resources"]
fn bundled_diagrams_render_unsaved_source_and_fences_recover_without_losing_text(
    cx: &mut TestAppContext,
) {
    use guild_desktop::diagrams::runtime::Manager;
    for (name, text) in [
        ("live.mmd", "flowchart LR\n A[\"Привет\"]-->B\n"),
        (
            "live.md",
            "Before diagram.\n\n```mermaid\nflowchart LR\n A[\"Привет\"]-->B\n```\n\n```plantuml\n@startuml\nAlice -> Bob: Привет\n@enduml\n```\n\n```text\n```mermaid literal\n```\n\nAfter diagram.\n",
        ),
    ] {
        let mut f = mount_named(cx, text, false, name);
        let manager = f.cx.update(|_, cx| {
            cx.new(|cx| Manager::new(Ok(super::diagrams::fixture_resources()), cx))
        });
        f.cx.update(|_, cx| f.pane.attach_renderer(&manager, cx));
        f.click("markdown-split");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(25);
        loop {
            f.cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            f.cx.run_until_parked();
            if f.cx.debug_bounds("diagram-image-mermaid").is_some()
                && (name.ends_with("mmd") || f.cx.debug_bounds("diagram-image-plantuml").is_some())
            {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "installed diagram not painted"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if name.ends_with("mmd") {
            f.cx.update(|window, cx| {
                let editor = f.file.read(cx).editor().unwrap();
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            });
            f.cx.simulate_keystrokes("cmd-a");
            f.cx.simulate_input("flowchart LR\n A[");
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                f.cx.executor()
                    .advance_clock(std::time::Duration::from_millis(20));
                f.cx.run_until_parked();
                if f.cx.debug_bounds("diagram-error").is_some() {
                    break;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // Simulated multiline input records native newline/edit groups.
            for _ in 0..text.lines().count() * 2 + 8 {
                if f.text() == text {
                    break;
                }
                f.cx.update(|window, cx| {
                    let editor = f.file.read(cx).editor().unwrap();
                    window.focus(&editor.read(cx).focus_handle(cx), cx);
                });
                f.cx.simulate_keystrokes("cmd-z");
            }
            assert_eq!(
                f.text(),
                text,
                "native Undo history must retain the original source"
            );
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                f.cx.executor()
                    .advance_clock(std::time::Duration::from_millis(20));
                f.cx.run_until_parked();
                if f.cx.debug_bounds("diagram-image").is_some() {
                    break;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        } else {
            assert!(f.cx.debug_bounds("diagram-pending-mermaid").is_none());
            assert!(f.cx.debug_bounds("diagram-pending-plantuml").is_none());
            assert!(f.cx.debug_bounds("diagram-error").is_none());
            f.cx.update(|window, cx| {
                let editor = f.file.read(cx).editor().unwrap();
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            });
            f.cx.simulate_keystrokes("cmd-a");
            f.cx.simulate_input(&text.replace(" A[\"Привет\"]-->B", " A["));
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                f.cx.executor()
                    .advance_clock(std::time::Duration::from_millis(20));
                f.cx.run_until_parked();
                if f.cx.debug_bounds("diagram-error-mermaid").is_some()
                    && f.cx.debug_bounds("diagram-image-plantuml").is_some()
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "both fenced engine outcomes required"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // Simulated multiline input records native newline/edit groups.
            for _ in 0..text.lines().count() * 2 + 8 {
                if f.text() == text {
                    break;
                }
                f.cx.update(|window, cx| {
                    let editor = f.file.read(cx).editor().unwrap();
                    window.focus(&editor.read(cx).focus_handle(cx), cx);
                });
                f.cx.simulate_keystrokes("cmd-z");
            }
            assert_eq!(
                f.text(),
                text,
                "native Undo history must retain the original source"
            );
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                f.cx.executor()
                    .advance_clock(std::time::Duration::from_millis(20));
                f.cx.run_until_parked();
                if f.cx.debug_bounds("diagram-image-mermaid").is_some()
                    && f.cx.debug_bounds("diagram-image-plantuml").is_some()
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "both fences must recover: mermaid image {:?}, plantuml image {:?}, error {:?}",
                    f.cx.debug_bounds("diagram-image-mermaid"),
                    f.cx.debug_bounds("diagram-image-plantuml"),
                    f.cx.debug_bounds("diagram-error")
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(f.cx.debug_bounds("diagram-error").is_none());
            assert!(f.cx.debug_bounds("diagram-pending-mermaid").is_none());
            assert!(f.cx.debug_bounds("diagram-pending-plantuml").is_none());
            f.click("markdown-preview");
            let copied = f.rendered();
            assert!(
                copied.contains("Before diagram")
                    && copied.contains("After diagram")
                    && copied.contains("Привет")
                    && copied.contains("mermaid literal"),
                "{copied}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(f.dir.path().join(name)).unwrap(),
            text
        );
        f.click("markdown-editor");
        let joined =
            f.cx.update(|_, cx| manager.update(cx, |manager, cx| manager.shutdown(cx)));
        f.cx.run_until_parked();
        drop(joined);
    }
}

#[gpui::test]
fn bundled_diagrams_packaging_errors_are_adjacent_for_standalone_and_fences(
    cx: &mut TestAppContext,
) {
    use guild_desktop::diagrams::{
        Engine, Resources,
        runtime::{Key, Manager, Output},
    };
    let source = "flowchart TD\n A-->B";
    for (name, text) in [
        ("failure.mmd", source.to_owned()),
        ("failure.md", format!("```mermaid\n{source}\n```")),
    ] {
        let state = tempfile::tempdir().unwrap();
        let mut f = mount_named(cx, &text, false, name);
        let manager = f.cx.update(|_, cx| {
            cx.new(|cx| Manager::new(Ok(Resources::from_directory(state.path())), cx))
        });
        f.cx.update(|_, cx| f.pane.attach_renderer(&manager, cx));
        f.click("markdown-preview");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            f.cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            f.cx.run_until_parked();
            if f.cx.debug_bounds("diagram-error").is_some() {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        f.cx.update(|_, cx| {
            manager.update(cx, |m, _| {
                match m.output(&Key::new(Engine::Mermaid, source)) {
                    Output::Error(error) => assert!(error.contains("Reinstall Guild")),
                    _ => panic!("packaging diagnostic required"),
                }
            })
        });
        assert!(!f.cx.did_prompt_for_paths());
        assert_eq!(f.text(), text);
    }
}
#[gpui::test]
#[ignore = "requires explicit prepared bundled resources"]
fn bundled_diagrams_all_suffixes_and_unicode_paths_render_without_source_changes(
    cx: &mut TestAppContext,
) {
    use guild_desktop::diagrams::runtime::Manager;
    for (name, source, selector) in [
        (
            "Схема с пробелом.mmd",
            "flowchart TD\n A[\"Привет\"]-->B",
            "diagram-image-mermaid",
        ),
        (
            "Схема с пробелом.mermaid",
            "sequenceDiagram\n Alice->>Bob: Привет",
            "diagram-image-mermaid",
        ),
        (
            "Схема с пробелом.puml",
            "@startuml\nAlice -> Bob: Привет\n@enduml",
            "diagram-image-plantuml",
        ),
        (
            "Схема с пробелом.plantuml",
            "@startuml\nclass Привет\n@enduml",
            "diagram-image-plantuml",
        ),
    ] {
        let mut f = mount_named(cx, source, false, name);
        let manager = f.cx.update(|_, cx| {
            cx.new(|cx| Manager::new(Ok(super::diagrams::fixture_resources()), cx))
        });
        f.cx.update(|_, cx| f.pane.attach_renderer(&manager, cx));
        f.click("markdown-preview");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            f.cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            f.cx.run_until_parked();
            if f.cx.debug_bounds(selector).is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "{name} must render directly from bundled resources"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!f.cx.did_prompt_for_paths());
        assert_eq!(f.text(), source);
        assert_eq!(
            std::fs::read_to_string(f.dir.path().join(name)).unwrap(),
            source
        );
    }
}
