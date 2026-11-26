//! The settings, drawn inside their own tab.
//!
//! A module of its own because `main.rs` had grown past three thousand
//! lines by adding one screen at a time — this is the first of them to move
//! out, and it moved because it was the one being changed.
//!
//! The methods are `Root`'s: Rust lets an `impl` block live in any module
//! of the crate that declares the type, and a window's screens are the
//! window's however the files are arranged.

mod action_bar;
pub(crate) use action_bar::OrderScroll;
pub(crate) use catalog::Setting;
#[cfg(test)]
mod action_bar_tests;
mod catalog;
pub(crate) mod choices;
mod customizer;
mod keymap;
mod network;
mod open_in;
pub(crate) use open_in::AppPicker;
mod scopes;
mod sections;
pub(crate) mod updates;

use gpui::prelude::*;
use gpui::{AnimationExt, Context, div, px, rgb};

use crate::i18n::Text;
use crate::{Root, TabId, theme};

/// What a Settings page is drawn in, and so what putting it away means.
///
/// A tab of the project in front, or — with no project in front — the
/// window's own body, where there is no tab to hold it. M5.11's
/// decision 15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SettingsHost {
    Tab(TabId),
    Window,
}

/// A section of the settings.
///
/// Settings as one long list is a list one reads through. Sections are how
/// the environments people come from arrange them, and the grouping is
/// the same: what is about the terminal, what is about sessions, what is
/// about keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    General,
    Appearance,
    Projects,
    Terminal,
    Editor,
    Sessions,
    Network,
    Keymap,
    Experimental,
    Updates,
}

impl Section {
    /// Every section, in the order they are listed.
    const ALL: &'static [Section] = &[
        Section::General,
        Section::Appearance,
        Section::Terminal,
        Section::Sessions,
        Section::Network,
        Section::Projects,
        Section::Editor,
        Section::Keymap,
        Section::Experimental,
        Section::Updates,
    ];

    pub(crate) fn title(self) -> Text {
        match self {
            Section::General => Text::SectionGeneral,
            Section::Appearance => Text::AppearanceSection,
            Section::Projects => Text::ProjectsSection,
            Section::Terminal => Text::SectionTerminal,
            Section::Editor => Text::SectionEditor,
            Section::Sessions => Text::SectionSessions,
            Section::Network => Text::NetworkSection,
            Section::Keymap => Text::SectionKeymap,
            Section::Experimental => Text::SectionExperimental,
            Section::Updates => Text::UpdatesSection,
        }
    }

    /// A stable identifier for the element, so that switching sections
    /// does not confuse GPUI about which row is which.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Section::General => "section-general",
            Section::Appearance => "section-appearance",
            Section::Projects => "section-projects",
            Section::Terminal => "section-terminal",
            Section::Editor => "section-editor",
            Section::Sessions => "section-sessions",
            Section::Network => "section-network",
            Section::Keymap => "section-keymap",
            Section::Experimental => "section-experimental",
            Section::Updates => "section-updates",
        }
    }
}

impl Section {
    fn intro(self) -> Text {
        match self {
            Self::General => Text::SettingsGeneralIntro,
            Self::Appearance => Text::SettingsAppearanceIntro,
            Self::Terminal => Text::SettingsTerminalIntro,
            Self::Sessions => Text::SettingsSessionsIntro,
            Self::Network => Text::NetworkIntro,
            Self::Projects => Text::SettingsProjectsIntro,
            Self::Editor => Text::SettingsEditorIntro,
            Self::Keymap => Text::SettingsKeymapIntro,
            Self::Experimental => Text::SettingsExperimentalIntro,
            Self::Updates => Text::UpdatesIntro,
        }
    }
}
impl Root {
    pub(crate) fn render_settings(
        &self,
        chosen: Section,
        host: SettingsHost,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let searching = !self.settings_query.trim().is_empty();
        let heading = (!searching).then(|| self.scoped_heading(host)).flatten();
        let rows = if searching {
            self.section_rows(chosen, cx)
        } else {
            self.scoped_rows(host, cx)
                .unwrap_or_else(|| self.section_rows(chosen, cx))
        };
        let empty = rows.is_empty();
        let width = self.settings_width.get();
        let compact = width > 0.0 && width < 600.0;
        // A definite width keeps nested wrapped rows out of repeated intrinsic
        // flex sizing on every scroll frame. Reuse the measured host width and
        // scaled design tokens, including when the navigation stacks above it.
        let rem = crate::scale::rem_size(self.settings.ui_font_size);
        let content_width = (width
            - if compact {
                0.0
            } else {
                f32::from(theme::settings::NAV.to_pixels(rem))
            }
            - 2.0 * f32::from(theme::settings::PADDING.to_pixels(rem)))
        .max(0.0);
        let measure = self.settings_width.clone();
        let owner = cx.entity_id();
        div()
            .id("settings-page")
            .debug_selector(|| "settings-page".into())
            .tab_group()
            // The window's own page holds a focus boundary of its own, not a
            // tab stop: with no tab in front, Root's fallback spares what is
            // focused inside it (code review r2, arch+quality-1; traps.md,
            // "Native update controls need a focus boundary").
            .when(host == SettingsHost::Window, |page| {
                page.track_focus(&self.settings_page_focus)
            })
            .capture_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && root.settings_escape(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(|event, window, cx| {
                if crate::view::controls::tab_navigation(event, window, cx) { cx.stop_propagation(); }
            })
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(rgb(theme.surface.sunken))
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        let width: f32 = bounds.size.width.into();
                        if measure.replace(width) != width {
                            window.refresh();
                            cx.defer(move |cx| cx.notify(owner));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .flex()
                    .when(compact, |body| body.flex_col())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .min_h_0()
                            .when(!compact, |nav| nav.w(theme::settings::NAV))
                            .gap(theme::space::CARD_GAP)
                            .p(theme::space::CARD_GAP)
                            .debug_selector(|| "settings-nav-region".into())
                            .bg(theme::settings::navigation(theme))
                            .when(!compact, |nav| nav.border_r_1())
                            .when(compact, |nav| nav.border_b_1())
                            .border_color(theme::settings::outline(theme))
                            .child(
                                sections::navigation_button(
                                    theme,
                                    "settings-done",
                                    div().flex().items_center().gap(theme::space::CARD_GAP)
                                        .child(crate::icons::toolbar("ui/arrow-left.svg")
                                            .text_color(rgb(theme.text.secondary)))
                                        .child(ui.text(Text::MenuBack)),
                                    false,
                                )
                                .debug_selector(|| "settings-done".into())
                                .on_click(cx.listener(move |root, _, _, cx| root.close_settings(host, cx))),
                            )
                            .child(
                                div()
                                    .id("settings-search")
                                    .debug_selector(|| "settings-search".into())
                                    .track_focus(&self.settings_search_focus)
                                    .key_context("settings-search")
                                    .min_h(theme::settings::CONTROL)
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .px(theme::space::CARD_GAP)
                                    .rounded(theme::radius::ROW)
                                    .border_1()
                                    .border_color(rgb(theme.surface.divider))
                                    .focus(|style| style.border_color(rgb(theme.accent.patina)))
                                    .bg(rgb(theme.surface.inset))
                                    .child(
                                        crate::text_field::TextField::new(
                                            "settings-query-input",
                                            self.settings_query.clone(),
                                            cx.listener(|root, value: &String, _, cx| {
                                                root.settings_query = value.clone();
                                                root.stop_recording();
                                                root.settings_scroll.set_offset(Default::default());
                                                cx.notify();
                                            }),
                                        )
                                        .vertical_scroll()
                                        .placeholder(ui.text(Text::SettingsSearch))
                                        .focus(&self.settings_search_focus)
                                        .on_key(
                                            cx.listener(
                                                |root, event: &gpui::KeyDownEvent, window, cx| {
                                                    if event.keystroke.key == "escape" {
                                                        if root.settings_query.is_empty() {
                                                            window.focus(&root.focus, cx);
                                                        } else {
                                                            root.settings_query.clear();
                                                        }
                                                        cx.notify();
                                                    } else if event.keystroke.key == "tab" {
                                                        if event.keystroke.modifiers.shift {
                                                            window.focus_prev(cx);
                                                        } else {
                                                            window.focus_next(cx);
                                                        }
                                                    }
                                                },
                                            ),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("settings-navigation")
                                    .flex_1()
                                    .flex()
                                    .when(!compact, |nav| nav.flex_col())
                                    .when(compact, |nav| nav.flex_wrap())
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .gap(px(2.0))
                                    .children(Section::ALL.iter().copied().map(|section| {
                                        let active =
                                            !searching && section == chosen && heading.is_none();
                                        let mut groups = Vec::new();
                                        if active && !compact {
                                            for row in catalog::Setting::ALL
                                                .iter()
                                                .filter(|row| row.info().0 == section)
                                            {
                                                let group = row.info().1;
                                                if !groups.contains(&group) {
                                                    groups.push(group);
                                                }
                                            }
                                        }
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                sections::navigation_button(
                                                    theme,
                                                    section.id(),
                                                    div().flex().items_center().gap(theme::space::CARD_GAP)
                                                        .child(crate::icons::toolbar(match section {
                                                            Section::General => "ui/title-settings.svg",
                                                            Section::Appearance => "ui/workspace-palette.svg",
                                                            Section::Terminal => "ui/workspace-terminal.svg",
                                                            Section::Sessions => "ui/workspace-brain.svg",
                                                            Section::Network => "ui/tool-refresh.svg",
                                                            Section::Projects => "ui/workspace-folder.svg",
                                                            Section::Editor => "ui/workspace-book.svg",
                                                            Section::Keymap => "ui/workspace-keyboard.svg",
                                                            Section::Experimental => "ui/title-settings.svg",
                                                            Section::Updates => "ui/tool-refresh.svg",
                                                        }).text_color(rgb(theme.text.secondary)))
                                                        .child(ui.text(section.title()).to_owned()),
                                                    active,
                                                )
                                                .on_click(cx.listener(move |root, _, _, cx| {
                                                    root.settings_query.clear();
                                                    root.choose_section(host, section, cx);
                                                })),
                                            )
                                            .children(groups.into_iter().map(|group| {
                                                div()
                                                    .id(gpui::SharedString::from(format!(
                                                        "settings-group-{:?}",
                                                        group
                                                    )))
                                                    .tab_index(0)
                                                    .pl(theme::space::CARD_PADDING)
                                                    .py(px(4.0))
                                                    .text_size(theme::size::META)
                                                    .text_color(rgb(theme.text.tertiary))
                                                    .cursor_pointer()
                                                    .focus(|style| {
                                                        style.text_color(rgb(theme.accent.patina))
                                                    })
                                                    .hover(|style| {
                                                        style.text_color(rgb(theme.text.primary))
                                                    })
                                                    .child(ui.text(group))
                                                    .on_click(cx.listener(move |root, _, _, cx| {
                                                        if let Some((_, y)) = root
                                                            .settings_group_positions
                                                            .borrow()
                                                            .iter()
                                                            .find(|(title, _)| *title == group)
                                                        {
                                                            root.settings_scroll.set_offset(
                                                                gpui::point(px(0.0), -*y),
                                                            );
                                                            cx.notify();
                                                        }
                                                    }))
                                            }))
                                    }))
                                    .children(self.scoped_navigation(host, cx)),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .child(
                                div()
                                    .id("settings-rows")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.settings_scroll)
                                    .p(theme::settings::PADDING)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .w_full()
                                            .debug_selector(|| "settings-content".into())
                                            .max_w(theme::settings::CONTENT)
                                            .when(width > 0.0, |content| content.w(px(content_width)))
                                            .gap(theme::settings::GROUP_GAP)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(theme::space::CARD_GAP)
                                                    .child(
                                                        div()
                                                            .text_size(theme::size::SECTION)
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .child(if searching {
                                                                ui.text(Text::SettingsResults)
                                                                    .to_owned()
                                                            } else {
                                                                heading.as_ref().map_or_else(
                                                                    || {
                                                                        ui.text(chosen.title())
                                                                            .to_owned()
                                                                    },
                                                                    |(title, _)| title.clone(),
                                                                )
                                                            }),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(theme.text.tertiary))
                                                            .child(
                                                                heading
                                                                    .as_ref()
                                                                    .map(|h| h.1.clone())
                                                                    .unwrap_or_else(|| {
                                                                        ui.text(if searching {
                                                                        Text::SettingsResultsIntro
                                                                    } else {
                                                                        chosen.intro()
                                                                    })
                                                                    .into()
                                                                    }),
                                                            ),
                                                    ),
                                            )
                                            .children(rows)
                                            .when(empty, |content| {
                                                content.child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(theme::space::CARD_GAP)
                                                        .child(ui.text(Text::SettingsNoResults))
                                                        .child(ui.text(Text::SettingsTrySearch))
                                                        .child(
                                                            sections::setting_button(
                                                                theme,
                                                                "clear-settings-search",
                                                                ui.text(Text::SettingsClear)
                                                                    .to_owned(),
                                                                false,
                                                            )
                                                            .on_click(cx.listener(
                                                                |root, _, window, cx| {
                                                                    root.settings_query.clear();
                                                                    window.focus(
                                                                        &root.settings_search_focus,
                                                                        cx,
                                                                    );
                                                                    cx.notify();
                                                                },
                                                            )),
                                                        ),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .text_size(theme::size::META)
                                                    .text_color(rgb(theme.text.tertiary))
                                                    .child(ui.text(if heading.is_some() {Text::ScopedSaveHint} else {Text::SettingsImmediate})),
                                            )
                                            .map(|content| {
                                                if crate::system::reduce_motion() {
                                                    content.into_any_element()
                                                } else {
                                                    content.with_animation(
                                                        gpui::ElementId::Name(format!("settings-{host:?}-{chosen:?}-{:?}",self.scoped_settings_target(host)).into()),
                                                        gpui::Animation::new(theme::motion::TAB_SETTLE),
                                                        |content, delta| content.opacity(delta),
                                                    ).into_any_element()
                                                }
                                            }),
                                    ),
                            )
                            .when_some(
                                crate::view::thumb(&self.settings_scroll),
                                |area, (top, length)| {
                                    area.child(
                                        div()
                                            .absolute()
                                            .right(px(2.0))
                                            .top(px(top))
                                            .w(theme::space::SCROLLBAR)
                                            .h(px(length))
                                            .rounded_full()
                                            .bg(rgb(theme.text.faint)),
                                    )
                                },
                            ),
                    ),
            )
            .into_any_element()
    }
}
