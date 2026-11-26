//! The keymap section: one row per action, editable in place.
//!
//! A stroke being recorded must not also fire — pressing `cmd-w` to bind it
//! would close the tab — so every binding carries a `!recording` predicate
//! and the window's key context gains `recording` while a stroke is read.
//! The stroke is an answer to a question, not a command.

use gpui::prelude::*;
use gpui::{Context, SharedString, div, px, rgb};

use crate::i18n::Text;
use crate::{Root, actions, theme};

impl Root {
    /// One row per action: what it is, and the keys that do it.
    ///
    /// Every action, not only the ones with a default — an action bound to
    /// nothing is exactly the one a person comes here to bind.
    pub(crate) fn keymap_rows(&self, cx: &mut Context<Root>) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        use actions::keymap::Named;

        let ui = self.ui();
        let bound: std::collections::BTreeMap<Named, String> = actions::effective()
            .into_iter()
            .map(|(stroke, action)| (action, stroke))
            .collect();
        let overrides = actions::keymap::overrides();

        Named::ALL
            .iter()
            .copied()
            .filter(|action| {
                (!self.settings_query.trim().is_empty()
                    || !self.settings_modified
                    || overrides.contains_key(action))
                    && super::catalog::matches_query(
                        &self.settings_query,
                        &[
                            action.name(),
                            bound.get(action).map(String::as_str).unwrap_or(""),
                            self.ui().text(Text::SectionKeymap),
                            "keymap shortcuts keyboard",
                        ],
                    )
            })
            .map(|action| {
                let recording = self.recording == Some(action);
                // **Search Everywhere's default is a double Shift**, which
                // the window detects rather than binds (M8.9, decision 8):
                // `effective` has no stroke for it, and the row shows the
                // default until a recorded stroke replaces it — Reset
                // brings it back. Spelled as its neighbours are, `shift
                // shift` beside `cmd-shift-o`, not printed as `⇧⇧`.
                let stroke = bound.get(&action).cloned().or_else(|| {
                    (action == Named::SearchEverywhere)
                        .then(|| crate::double_tap::DOUBLE_SHIFT.to_owned())
                });
                let changed = overrides.contains_key(&action);

                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .px(theme::space::CARD_PADDING)
                    .py(px(5.0))
                    .rounded(theme::radius::ROW)
                    .hover(|row| row.bg(rgb(theme.surface.card)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            // The name the keymap file uses, not a prose
                            // label: someone editing the file by hand needs
                            // the same word the file expects, and two names
                            // for one action is how a shortcut comes to
                            // "not work".
                            .child(action.name()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("stroke-{}", action.name())))
                            .tab_index(0)
                            .border_1()
                            .border_color(rgb(theme.surface.divider))
                            .focus(|style| style.border_color(rgb(theme.accent.patina)))
                            .min_w(px(140.0))
                            .px(theme::space::CARD_GAP)
                            .py(px(3.0))
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .font_family(self.code_font.clone())
                            .text_size(theme::size::META)
                            .bg(rgb(if recording {
                                theme.surface.raised
                            } else {
                                theme.surface.inset
                            }))
                            .text_color(rgb(
                                if recording && (self.conflict.is_some() || self.stroke_refused) {
                                    theme.accent.rust
                                } else if recording {
                                    theme.accent.brass
                                } else if changed {
                                    theme.accent.patina
                                } else {
                                    theme.text.secondary
                                },
                            ))
                            .hover(|slot| {
                                slot.bg(crate::theme::control_hover(theme, theme.surface.card))
                            })
                            .child(match (recording, self.conflict) {
                                (true, Some(owner)) => {
                                    format!("{} {}", ui.text(Text::KeymapTaken), owner.name())
                                }
                                (true, None) if self.stroke_refused => {
                                    ui.text(Text::KeymapNeedsModifier).to_owned()
                                }
                                (true, None) => ui.text(Text::KeymapRecording).to_owned(),
                                (false, _) => stroke.clone().unwrap_or_else(|| "—".to_owned()),
                            })
                            .on_click(cx.listener(move |root, _event, window, cx| {
                                // Clicking the one being recorded gives up
                                // — otherwise the only way out of
                                // recording would be to bind something.
                                // Nothing in the keymap changed, so nothing
                                // is bound again.
                                root.stop_recording();
                                root.recording = (!recording).then_some(action);
                                // The keys that follow have to reach the
                                // recording, which reads them only while
                                // the root's own handle is focused.
                                if root.recording.is_some() {
                                    window.focus(&root.focus, cx);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("reset-{}", action.name())))
                            .when(changed, |button| {
                                button
                                    .tab_index(0)
                                    .focus(|style| style.bg(rgb(theme.surface.raised)))
                            })
                            .px(px(6.0))
                            .py(px(3.0))
                            .rounded(theme::radius::ROW)
                            .text_size(theme::size::META)
                            .when(changed, |button| {
                                button
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text.tertiary))
                                    .hover(|button| {
                                        button
                                            .bg(crate::theme::control_hover(
                                                theme,
                                                theme.surface.card,
                                            ))
                                            .text_color(rgb(theme.accent.rust))
                                    })
                                    .on_click(cx.listener(move |root, _event, _window, cx| {
                                        root.reset_stroke(action, cx)
                                    }))
                            })
                            // The slot keeps its width whether or not
                            // anything was changed: a row that shifts when
                            // its neighbour is edited is a row that is hard
                            // to click.
                            .when(!changed, |button| {
                                button.text_color(rgb(theme.surface.panel))
                            })
                            .child(ui.text(Text::KeymapReset)),
                    )
                    .into_any_element()
            })
            .collect()
    }
}
