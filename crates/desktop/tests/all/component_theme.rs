//! Shared component chrome must follow Guild across startup and live theme changes.
use gpui_component::Theme as ComponentTheme;
use guild_desktop::{editor, theme::Theme};

fn assert_palette(palette: &Theme, cx: &gpui::App) {
    let component = ComponentTheme::global(cx);
    assert_eq!(
        component.mode.is_dark(),
        !palette.kind.is_light(),
        "{} mode",
        palette.kind.name()
    );
    for (name, color, token, expected) in [
        (
            "Find/Replace and tooltip",
            component.colors.popover,
            component.tokens.popover,
            palette.surface.card,
        ),
        (
            "completion",
            component.colors.list,
            component.tokens.list,
            palette.surface.card,
        ),
        (
            "ghost button hover",
            component.colors.secondary_hover,
            component.tokens.secondary_hover,
            u32::from(guild_desktop::theme::control_hover(
                *palette,
                palette.surface.raised,
            )) >> 8,
        ),
        (
            "button",
            component.colors.secondary,
            component.tokens.secondary,
            palette.surface.raised,
        ),
        (
            "document",
            component.colors.background,
            component.tokens.background,
            palette.surface.app,
        ),
        (
            "input border",
            component.colors.input,
            component.tokens.input,
            palette.surface.divider,
        ),
        (
            "foreground",
            component.colors.foreground,
            component.tokens.foreground,
            palette.text.primary,
        ),
        (
            "selection",
            component.colors.selection,
            component.tokens.selection,
            palette.accent.highlight,
        ),
        (
            "slider bar",
            component.colors.slider_bar,
            component.tokens.slider_bar,
            palette.accent.patina,
        ),
        (
            "slider thumb",
            component.colors.slider_thumb,
            component.tokens.slider_thumb,
            palette.text.primary,
        ),
    ] {
        let expected: gpui::Hsla = gpui::rgb(expected).into();
        assert_eq!(color, expected, "{} {name} color", palette.kind.name());
        assert_eq!(
            token,
            expected.into(),
            "{} {name} token",
            palette.kind.name()
        );
    }
    assert_eq!(component.font_family.as_ref(), "Inter");
    assert_eq!(component.font_size, gpui::px(17.));
    let base = gpui_kit::base::Theme::global(cx);
    assert_eq!(
        base.tokens.colors.background,
        gpui::rgb(palette.surface.app).into()
    );
    assert_eq!(
        base.tokens.colors.surface,
        gpui::rgb(palette.surface.card).into()
    );
    assert_eq!(base.tokens.typography.sans.as_ref(), "Inter");
    assert_eq!(component.tokens.button.color, component.colors.button);
    assert_ne!(component.colors.button_foreground, component.colors.button);
    let syntax = serde_json::to_value(&component.highlight_theme).unwrap();
    assert_eq!(
        syntax["appearance"],
        if palette.kind.is_light() {
            "light"
        } else {
            "dark"
        }
    );
}

#[gpui::test]
fn component_theme_startup_for_every_palette(cx: &mut gpui::TestAppContext) {
    for palette in Theme::all() {
        cx.update(|cx| {
            gpui_component::init(cx);
            editor::apply_ui_font(Some("Inter"), cx);
            editor::apply_theme(&palette, 17., cx);
            assert_palette(&palette, cx);
        });
    }
}

#[gpui::test]
fn component_theme_every_transition_and_repeat(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        editor::apply_ui_font(Some("Inter"), cx);
        for source in Theme::all() {
            for target in Theme::all() {
                editor::apply_theme(&source, 17., cx);
                editor::apply_theme(&target, 17., cx);
                assert_palette(&target, cx);
                editor::apply_theme(&target, 17., cx);
                assert_palette(&target, cx);
                editor::apply_theme(&source, 17., cx);
                assert_palette(&source, cx);
            }
        }
    });
}

#[gpui::test]
fn component_theme_repairs_an_equal_color_with_a_stale_token(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        editor::apply_ui_font(Some("Inter"), cx);
        let palette = Theme::default();
        editor::apply_theme(&palette, 17., cx);
        // A previous direct write can leave the solid color correct but the
        // renderable token stale. Reapplying the same palette must repair it.
        ComponentTheme::global_mut(cx).tokens.popover = gpui::Hsla::white().into();
        editor::apply_theme(&palette, 17., cx);
        assert_palette(&palette, cx);
    });
}

#[gpui::test]
fn component_theme_cloud_uses_light_defaults(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        editor::apply_theme(&Theme::of(guild_desktop::theme::Kind::Light), 17., cx);
        assert!(
            !ComponentTheme::global(cx).mode.is_dark(),
            "Cloud is a light palette"
        );
    });
}

struct SearchSource(gpui::Entity<gpui_component::input::EditorState>);
impl gpui::Render for SearchSource {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled};
        gpui::div()
            .size_full()
            .child(gpui_component::input::Editor::new(&self.0).h_full())
    }
}

#[gpui::test]
fn component_theme_open_find_replace_keeps_query_selection_focus_and_undo(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::{AppContext, px, size};
    use gpui_component::input::EditorState;
    let mut editor_state = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        editor::init(cx);
        editor::apply_theme(&Theme::default(), 17., cx);
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .searchable(true)
                .default_value("foo bar foo")
        });
        state.update(cx, |state, cx| {
            state.set_selected_range(0..3, cx);
            state.open_search(true, cx);
        });
        editor_state = Some(state.clone());
        let view = cx.new(|_| SearchSource(state));
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(size(px(800.), px(400.)));
    cx.run_until_parked();
    let state = editor_state.unwrap();
    cx.simulate_keystrokes("cmd-a b a r");
    cx.simulate_keystrokes("cmd-a");
    let focus = cx.update(|window, cx| window.focused(cx).expect("Find field focused"));
    for palette in Theme::all() {
        cx.update(|_, cx| editor::apply_theme(&palette, 17., cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(
                window.focused(cx),
                Some(focus.clone()),
                "{} focus",
                palette.kind.name()
            );
            assert_eq!(state.read(cx).search_session().query.as_str(), "bar");
            assert!(state.read(cx).search_session().replace_mode);
            assert_eq!(state.read(cx).value().as_ref(), "foo bar foo");
        });
        cx.simulate_keystrokes("cmd-c");
        cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "bar"));
    }
    cx.simulate_keystrokes("z cmd-z");
    cx.update(|_, cx| assert_eq!(state.read(cx).search_session().query.as_str(), "bar"));
    cx.simulate_keystrokes("tab");
    cx.simulate_input("replacement");
    cx.simulate_keystrokes("cmd-a");
    let replace_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for palette in Theme::all() {
        cx.update(|_, cx| editor::apply_theme(&palette, 17., cx));
        cx.run_until_parked();
        cx.update(|window, cx| assert_eq!(window.focused(cx), Some(replace_focus.clone())));
        cx.simulate_keystrokes("cmd-c");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "replacement"
            )
        });
    }
    cx.simulate_keystrokes("z cmd-z cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "replacement"
        )
    });
}
