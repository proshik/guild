//! The theme customizer's card, under Settings → Appearance → Colour theme,
//! in Guild's own parts: settings buttons for the two switches, a `TextField`
//! for hex, gpui-component's `Slider` for the three rows, round swatches for
//! the presets.
//!
//! The card is quiet on purpose: the window behind it is the preview.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};
use gpui_component::slider::Slider;

use super::sections::setting_button;
use crate::customizer::Tab;
use crate::i18n::Text;
use crate::root::customizer::Slider as Which;
use crate::view::tooltip::Hint as _;
use crate::{Root, theme};

impl Root {
    /// The *Customize theme* row's button: it opens the card, and closes it.
    pub(crate) fn customize_theme_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        setting_button(
            self.theme,
            "customize-theme",
            self.ui().text(Text::CustomizeThemeTitle).to_owned(),
            self.customizing(),
        )
        .debug_selector(|| "customize-theme".into())
        // A focus that outlives a render, so closing the card can hand it back.
        .track_focus(&self.customize_button)
        .on_click(cx.listener(|root, _, window, cx| root.toggle_customizer(window, cx)))
        .into_any_element()
    }

    /// The card, or nothing while it is closed.
    pub(crate) fn render_customizer(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let customizer = self.customizer.as_ref()?;
        let theme = self.theme;
        let ui = self.ui();
        let draft = &customizer.draft;
        let kind = draft.editing();
        let current = draft.current();
        let shown = current.map(|t| t.color);

        let kinds = div()
            .flex()
            .gap(px(2.0))
            .children(theme::Kind::ALL.iter().copied().map(|k| {
                let label = ui.text(if k.is_light() {
                    Text::ThemeLight
                } else {
                    Text::ThemeDark
                });
                setting_button(
                    theme,
                    ("customizer-kind", k as usize),
                    label.to_owned(),
                    k == kind,
                )
                .on_click(cx.listener(move |root, _, _, cx| root.customizer_edit(k, cx)))
            }));
        let head = div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .child(
                div()
                    .flex_1()
                    .text_size(theme::size::CARD_TITLE)
                    .text_color(rgb(theme.text.primary))
                    .child(ui.text(Text::CustomizeThemeTitle)),
            )
            .child(kinds)
            .child(
                div()
                    .id("customizer-close")
                    .debug_selector(|| "customizer-close".into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(theme::space::HEAD_BUTTON)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::control_hover(theme, theme.surface.sunken)))
                    .hint(ui.text(Text::CustomizeThemeClose), theme)
                    .on_click(
                        cx.listener(|root, _, window, cx| root.dismiss_customizer(window, cx)),
                    )
                    .child(
                        crate::icons::toolbar("icons/close.svg")
                            .text_color(rgb(theme.text.tertiary)),
                    ),
            );

        let tabs = div()
            .flex()
            .gap(px(2.0))
            .children(Tab::ALL.into_iter().map(|tab| {
                let label = ui.text(match tab {
                    Tab::Background => Text::ThemeBackground,
                    Tab::Accent => Text::ThemeAccent,
                    Tab::Highlight => Text::ThemeHighlight,
                });
                setting_button(
                    theme,
                    ("customizer-tab", tab as usize),
                    label.to_owned(),
                    tab == draft.tab(),
                )
                .on_click(cx.listener(move |root, _, _, cx| root.customizer_tab(tab, cx)))
            }));

        let hex_text = customizer
            .hex
            .clone()
            .unwrap_or_else(|| shown.map(guild_session::Tint::hex).unwrap_or_default());
        let colour = div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .debug_selector(|| "customizer-swatch".into())
                    .size(theme::settings::CONTROL)
                    .flex_none()
                    .rounded(theme::radius::ROW)
                    .border_1()
                    .border_color(theme::settings::outline(theme))
                    .when_some(shown, |swatch, color| swatch.bg(rgb(color))),
            )
            .child(
                div()
                    .debug_selector(|| "customizer-hex".into())
                    .w(theme::settings::HEX_FIELD)
                    .h(theme::settings::CONTROL)
                    .rounded(theme::radius::ROW)
                    .border_1()
                    .border_color(if customizer.invalid {
                        rgb(theme.accent.rust)
                    } else {
                        theme::settings::outline(theme)
                    })
                    .bg(rgb(theme.surface.inset))
                    .font_family(theme::font::MONO)
                    .when(customizer.invalid, |field| {
                        field.hint(ui.text(Text::ThemeHexInvalid), theme)
                    })
                    .child(
                        crate::text_field::TextField::new(
                            (
                                "customizer-hex-field",
                                kind as usize * 3 + draft.tab() as usize,
                            ),
                            hex_text,
                            cx.listener(|root, text: &String, _, cx| {
                                root.customizer_hex_typed(text, cx)
                            }),
                        )
                        .placeholder("#3b82f6")
                        .padded()
                        .on_key({
                            let root = cx.entity().downgrade();
                            move |event, window, cx| match event.keystroke.key.as_str() {
                                "enter" => {
                                    let _ = root
                                        .update(cx, |root, cx| root.customizer_hex_confirmed(cx));
                                }
                                "escape" => {
                                    cx.stop_propagation();
                                    let _ = root
                                        .update(cx, |root, cx| root.customizer_hex_restored(cx));
                                }
                                "tab" => {
                                    if event.keystroke.modifiers.shift {
                                        window.focus_prev(cx);
                                    } else {
                                        window.focus_next(cx);
                                    }
                                }
                                _ => {}
                            }
                        })
                        .on_focus({
                            let root = cx.entity().downgrade();
                            move |_, _, cx| {
                                let _ = root.update(cx, |root, _| root.customizer_hex_entered());
                            }
                        })
                        .on_blur({
                            let root = cx.entity().downgrade();
                            move |_, _, cx| {
                                let _ = root.update(cx, |root, cx| root.customizer_hex_left(cx));
                            }
                        }),
                    ),
            )
            .child(
                setting_button(theme, "customizer-eyedropper", String::new(), false)
                    .debug_selector(|| "customizer-eyedropper".into())
                    .hint(ui.text(Text::ThemeEyedropper), theme)
                    .on_click(cx.listener(|root, _, _, cx| root.customizer_sample(cx)))
                    .child(
                        crate::icons::toolbar("ui/tool-pipette.svg")
                            .text_color(rgb(theme.text.secondary)),
                    ),
            );

        let (hue, saturation) = draft.hue_saturation().unwrap_or((0.0, 0.0));
        let intensity = current.map_or(0, |t| t.intensity);
        let row = |label: Text, which: Which, value: String, disabled: bool| {
            div()
                .flex()
                .items_center()
                .gap(theme::space::CARD_GAP)
                .child(
                    div()
                        .w(theme::settings::CUSTOMIZER_LABEL)
                        .flex_none()
                        .text_color(rgb(theme.text.secondary))
                        .child(ui.text(label)),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Slider::new(match which {
                            Which::Hue => &customizer.hue,
                            Which::Saturation => &customizer.saturation,
                            Which::Intensity => &customizer.intensity,
                        })
                        .horizontal()
                        .disabled(disabled),
                    ),
                )
                .child(
                    div()
                        .w(theme::settings::VALUE_WIDTH)
                        .flex_none()
                        .text_right()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.tertiary))
                        .child(value),
                )
        };

        let preset = |id: usize, color: Option<u32>, name: String, selected: bool| {
            div()
                .id(("customizer-preset", id))
                .debug_selector(move || format!("customizer-preset-{id}"))
                .tab_index(0)
                .size(theme::settings::PRESET)
                .flex_none()
                .rounded_full()
                .border_1()
                .border_color(theme::settings::outline(theme))
                .cursor_pointer()
                .map(|dot| match color {
                    Some(color) => dot.bg(rgb(color)),
                    // The default swatch is the palette's own two grounds.
                    None => dot.overflow_hidden().child(
                        div()
                            .size_full()
                            .flex()
                            .child(div().flex_1().h_full().bg(rgb(theme.surface.card)))
                            .child(div().flex_1().h_full().bg(rgb(theme.surface.panel))),
                    ),
                })
                // The ink, not the accent: a patina ring vanished on the
                // blue preset in the dev window.
                .when(selected, |dot| {
                    dot.border_2().border_color(rgb(theme.text.primary))
                })
                .focus(|style| style.border_color(rgb(theme.accent.patina)))
                .hint(name, theme)
        };
        let presets = div()
            .flex()
            .flex_wrap()
            .gap(theme::space::CARD_GAP)
            .child(
                preset(
                    0,
                    None,
                    ui.text(Text::ThemeDefaultSwatch).to_owned(),
                    current.is_none(),
                )
                .on_click(cx.listener(|root, _, _, cx| root.customize(cx, |d| d.clear_tab()))),
            )
            .children(theme::PRESETS.iter().enumerate().map(|(i, &(_, color))| {
                preset(
                    i + 1,
                    Some(color),
                    guild_session::Tint::hex(color),
                    shown == Some(color),
                )
                .on_click(cx.listener(move |root, _, _, cx| root.customizer_color(color, cx)))
            }));

        let other = kind.other();
        let save = if kind.is_light() {
            Text::ThemeSaveLight
        } else {
            Text::ThemeSaveDark
        };
        let dirty = draft.dirty(kind);
        let foot = div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                setting_button(
                    theme,
                    "customizer-copy",
                    ui.text(if other.is_light() {
                        Text::ThemeCopyFromLight
                    } else {
                        Text::ThemeCopyFromDark
                    })
                    .to_owned(),
                    false,
                )
                .on_click(
                    cx.listener(|root, _, _, cx| root.customize(cx, |d| d.copy_from_other())),
                ),
            )
            .child(div().flex_1())
            .child(
                setting_button(
                    theme,
                    "customizer-reset",
                    ui.text(Text::ThemeReset).to_owned(),
                    false,
                )
                .on_click(cx.listener(|root, _, _, cx| root.customize(cx, |d| d.reset()))),
            )
            .child(
                setting_button(theme, "customizer-save", ui.text(save).to_owned(), false)
                    .debug_selector(|| "customizer-save".into())
                    .map(|button| {
                        if dirty {
                            // The primary button: one ink on brass in every
                            // palette, section 6.7.
                            button
                                .bg(rgb(theme.accent.brass))
                                .border_color(rgb(theme.accent.brass))
                                .text_color(rgb(theme::ON_BRASS))
                                .on_click(cx.listener(|root, _, _, cx| root.save_customizer(cx)))
                        } else {
                            button.opacity(0.5).cursor_default()
                        }
                    }),
            );

        Some(
            div()
                .id("customizer")
                .debug_selector(|| "customizer".into())
                .key_context("guild-customizer")
                // Escape is the window's (`root/render.rs`), so it reaches
                // the card from wherever the focus is.
                .track_focus(&customizer.focus)
                .flex()
                .flex_col()
                .gap(theme::space::CARD_INNER_GAP)
                .p(theme::space::CARD_PADDING)
                .border_t_1()
                .border_color(theme::settings::rule(theme))
                .child(head)
                .child(tabs)
                .child(colour)
                .child(row(Text::ThemeHue, Which::Hue, format!("{hue:.0}°"), false))
                .child(row(
                    Text::ThemeSaturation,
                    Which::Saturation,
                    format!("{saturation:.0}%"),
                    false,
                ))
                .child(presets)
                .child(row(
                    Text::ThemeIntensity,
                    Which::Intensity,
                    format!("{intensity}%"),
                    current.is_none(),
                ))
                .child(foot)
                .into_any_element(),
        )
    }
}
