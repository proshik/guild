//! The Find in Files card — section 6.34 of the design system.
//!
//! A floating picker card a person moves and resizes (section 6.32): a head
//! with the title, what the search came to, the pin and the cross; under it
//! the query with the three toggles; then the hits, a long list, each row
//! the line in the code face with its matches in brass and the file and
//! the line at its right.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, MouseButton, SharedString, div, px, rgb};

use crate::Root;
use crate::find_in_files::Field;
use crate::i18n::Text;
use crate::search::Toggle;
use crate::theme;
use crate::view::menu::{Anchor, Target};
use crate::view::picker::{Card, Place};

impl Root {
    /// What the Find in Files popup draws.
    pub(crate) fn find_in_files_card(&self, cx: &mut Context<Root>) -> Card {
        let pinned = self.find_in_files_popup().is_some_and(|popup| popup.pinned);
        Card {
            id: "find-in-files",
            place: Place::Floating {
                remembered: |settings| &mut settings.find_in_files,
                top: crate::root::recent::picker_top(),
                size: None,
            },
            // **A pinned card has no backdrop** (decision 14): a click in the
            // editor behind it lands in the editor.
            backdrop: !pinned,
            head: self.render_find_in_files_head(cx),
            rows: self.render_find_in_files_rows(cx),
            // Under the rows while there are rows: a preview of the last hit
            // under *Nothing matches* would be a preview of nothing found.
            preview: self
                .find_in_files_popup()
                .filter(|popup| !popup.walk.hits().is_empty())
                .and_then(|_| self.render_preview(cx)),
            foot: Some(self.render_find_in_files_foot(cx)),
        }
    }

    /// The line under everything: *Open in Find Window* and its stroke, at
    /// the right.
    fn render_find_in_files_foot(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .px(theme::space::CARD_PADDING)
            .py(px(6.0))
            .border_t_1()
            .border_color(rgb(theme.surface.divider))
            .text_size(theme::size::META)
            .child(
                div()
                    .id("find-in-files-open-in-window")
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .px(theme::space::CARD_GAP)
                    .h(theme::space::HEAD_TAB)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .hover(|button| {
                        button.bg(crate::theme::control_hover(theme, theme.surface.card))
                    })
                    .text_color(rgb(theme.text.secondary))
                    .child(ui.text(Text::FindInFilesOpenInWindow).to_owned())
                    .child(div().text_color(rgb(theme.text.faint)).child("⌘↩"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _event, window, cx| {
                            cx.stop_propagation();
                            root.find_in_files_to_window(window, cx);
                        }),
                    ),
            )
            .when(
                self.find_in_files_popup()
                    .is_some_and(|popup| popup.replace_mode),
                |this| this.child(self.render_project_replace_actions(cx)),
            )
            .into_any_element()
    }

    /// The title line and the query line.
    fn render_find_in_files_head(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .capture_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(toggle) = crate::find_in_files::toggle(&event.keystroke) {
                    root.find_in_files_toggle(toggle, cx);
                    root.find_in_files_focus(window, cx);
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .bg(rgb(theme.surface.sunken))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_INNER_GAP)
                    .px(theme::space::CARD_PADDING)
                    .pt(theme::space::CARD_GAP)
                    .text_size(theme::size::META)
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.text.secondary))
                            .child(
                                if self
                                    .find_in_files_popup()
                                    .is_some_and(|popup| popup.replace_mode)
                                {
                                    ui.text(Text::ReplaceInFilesTitle).to_owned()
                                } else {
                                    ui.text(Text::FindInFilesTitle).to_owned()
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_color(rgb(theme.text.faint))
                            .child(self.find_in_files_count()),
                    )
                    .child(self.render_find_in_files_use_mask(cx))
                    .child(ui.text(Text::SearchMask).to_owned())
                    .child(self.render_find_in_files_field(Field::Mask, cx))
                    .child(self.render_find_in_files_pin(cx))
                    .child(self.render_find_in_files_close(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .px(theme::space::CARD_PADDING)
                    .py(theme::space::CARD_GAP)
                    .text_size(theme::size::BODY)
                    .child(self.render_find_in_files_field(Field::Text, cx)),
            )
            .when(
                self.find_in_files_popup()
                    .is_some_and(|popup| popup.replace_mode),
                |this| this.child(self.render_project_replacement(cx)),
            )
            .child(self.render_find_in_files_scope(cx))
            .into_any_element()
    }

    /// Where the search looks: *In Project*, *Directory* with the directory
    /// beside it in `text.faint`, and `…` for a folder from the chooser.
    fn render_find_in_files_scope(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let scope = self
            .find_in_files_popup()
            .and_then(|popup| popup.query.scope.clone());

        div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .px(theme::space::CARD_PADDING)
            .pb(theme::space::CARD_GAP)
            .text_size(theme::size::META)
            .child(
                self.find_in_files_chip(
                    "find-in-files-in-project",
                    ui.text(Text::FindInFilesInProject),
                    scope.is_none(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _event, window, cx| {
                        cx.stop_propagation();
                        root.find_in_files_focus(window, cx);
                        root.find_in_files_scope(crate::find_in_files::Scope::Project, cx);
                    }),
                ),
            )
            .child(
                self.find_in_files_chip(
                    "find-in-files-directory",
                    ui.text(Text::FindInFilesDirectory),
                    scope.is_some(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _event, window, cx| {
                        cx.stop_propagation();
                        root.find_in_files_focus(window, cx);
                        root.find_in_files_directory(window, cx);
                    }),
                ),
            )
            .children(scope.map(|scope| {
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_color(rgb(theme.text.faint))
                    .child(scope.display().to_string())
            }))
            .child(
                self.find_in_files_button(
                    "find-in-files-choose",
                    false,
                    ui.text(Text::FindInFilesChooseDirectory).to_owned(),
                )
                .child("…")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _event, window, cx| {
                        cx.stop_propagation();
                        root.find_in_files_focus(window, cx);
                        root.find_in_files_choose(window, cx);
                    }),
                ),
            )
            .into_any_element()
    }

    /// A scope chip: lit `accent.brass` on `accent.brass_bg` while it is
    /// where the search looks.
    fn find_in_files_chip(
        &self,
        id: &'static str,
        label: &str,
        lit: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .flex()
            .items_center()
            .h(theme::space::HEAD_TAB)
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .bg(rgb(if lit {
                theme.accent.brass_bg
            } else {
                theme.surface.inset
            }))
            .text_color(rgb(if lit {
                theme.accent.brass
            } else {
                theme.text.secondary
            }))
            .child(label.to_owned())
    }

    /// One of the two fields: what has been typed, the caret while the
    /// letters go to it, the placeholder while nothing has been, and the
    /// chevron that opens a menu of what was used before.
    ///
    /// **Drawn here rather than with `picker_field`**, which is one field
    /// with its caret always on: this card has two, and a caret blinking in
    /// the field the letters do not go to is the trap *Guild has three
    /// fields that take the keyboard* drawn inside one card.
    fn render_find_in_files_field(&self, which: Field, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let Some(popup) = self.find_in_files_popup() else {
            return div().into_any_element();
        };
        let (text, placeholder) = match which {
            Field::Text => (self.picker_query().to_owned(), Text::SearchPlaceholder),
            Field::Mask => (popup.query.mask.clone(), Text::SearchMask),
        };
        let lit = popup.typing == which && !popup.replacement_focus;
        let off = which == Field::Mask && popup.mask_off;
        // The field that carries a refusal says so, as the tool's does.
        let wrong = popup.walk.refused().is_some()
            && match which {
                Field::Text => popup.query.regular_expression,
                Field::Mask => !popup.query.mask.is_empty() && !popup.mask_off,
            };
        let open = self.find_in_files_history_open(which);
        let at = self.find_history_at.clone();
        let slot = which as usize;

        let field = div()
            .id(match which {
                Field::Text => "find-in-files-query",
                Field::Mask => "find-in-files-mask",
            })
            .flex()
            .items_center()
            .min_w_0()
            .map(|this| {
                if which == Field::Text && popup.multiline.is_some() {
                    this.h(theme::space::HEAD_TAB * 3.)
                } else {
                    this.h(theme::space::HEAD_TAB)
                }
            })
            .pl(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.inset))
            .border_1()
            .border_color(rgb(if wrong {
                theme.accent.rust
            } else if lit {
                theme.accent.brass
            } else {
                theme.surface.inset
            }))
            .cursor_text()
            .text_size(match which {
                Field::Text => theme::size::BODY,
                Field::Mask => theme::size::META,
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_color(rgb(if off {
                        theme.text.faint
                    } else {
                        theme.text.primary
                    }))
                    .child(
                        if which == Field::Text
                            && let Some(input) = &popup.multiline
                        {
                            div()
                                .key_context("guild-search-multiline")
                                .size_full()
                                .child(
                                    gpui_component::input::Textarea::new(input)
                                        .appearance(false)
                                        .h_full(),
                                )
                                .into_any_element()
                        } else {
                            crate::text_field::TextField::new(
                                match which {
                                    Field::Text => "find-text-input",
                                    Field::Mask => "find-mask-input",
                                },
                                text,
                                cx.listener(move |root, value: &String, window, cx| {
                                    match which {
                                        Field::Text => {
                                            if let Some(open) = root.picker.as_mut() {
                                                open.picker.set_query(value.clone());
                                            }
                                            root.find_in_files_edited(window, cx);
                                        }
                                        Field::Mask => {
                                            let active = if let Some(popup) =
                                                root.find_in_files_popup_mut()
                                            {
                                                popup.query.mask = value.clone();
                                                popup.recall = Default::default();
                                                !popup.mask_off
                                            } else {
                                                false
                                            };
                                            if active {
                                                root.find_in_files_restart(cx);
                                            }
                                        }
                                    }
                                    root.settle_picker();
                                    cx.notify();
                                }),
                            )
                            .placeholder(ui.text(placeholder))
                            .list()
                            .when(lit, |input| {
                                input.focus(&self.picker.as_ref().expect("find popup").focus)
                            })
                            .selected(
                                which == Field::Text
                                    && self
                                        .picker
                                        .as_ref()
                                        .is_some_and(|open| open.picker.is_prefilled()),
                            )
                            .on_focus(cx.listener(move |root, _, _, cx| {
                                root.find_in_files_type_into(which, cx)
                            }))
                            .on_key(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                                root.picker_key(event, window, cx);
                            }))
                            .into_any_element()
                        },
                    ),
            )
            .child(
                div()
                    .id(match which {
                        Field::Text => "find-in-files-query-list",
                        Field::Mask => "find-in-files-mask-list",
                    })
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(theme::space::HEAD_TAB)
                    .cursor_pointer()
                    .child(
                        crate::icons::toolbar("ui/chevron-down.svg").text_color(rgb(if open {
                            theme.accent.brass
                        } else {
                            theme.text.faint
                        })),
                    )
                    // Where it ended up, for the menu that hangs under it
                    // — recorded only while its own menu is not up, the
                    // tab strip's own trigger rule.
                    .child(
                        gpui::canvas(
                            move |bounds, window, _cx| {
                                let mut places = at.get();
                                if !open && places[slot] != Some(bounds) {
                                    places[slot] = Some(bounds);
                                    at.set(places);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |root, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            root.find_in_files_focus(window, cx);
                            let anchor = match root.find_history_at.get()[slot] {
                                Some(bounds) => Anchor::Below(bounds),
                                None => Anchor::Pointer(event.position),
                            };
                            let items = root
                                .find_in_files_popup()
                                .map(|popup| match which {
                                    Field::Text => popup.history.clone(),
                                    Field::Mask => popup.masks.clone(),
                                })
                                .unwrap_or_default();
                            root.open_menu(
                                Target::FindHistory {
                                    field: which,
                                    items,
                                },
                                anchor,
                                window,
                                cx,
                            );
                        }),
                    ),
            )
            .when(which == Field::Text, |this| {
                this.child(
                    self.find_in_files_button(
                        "project-multiline",
                        popup.multiline.is_some(),
                        "Multiline query".into(),
                    )
                    .child("↵")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _, window, cx| {
                            cx.stop_propagation();
                            root.find_in_files_multiline(window, cx);
                        }),
                    ),
                )
                .child(self.render_find_in_files_toggle(
                    Toggle::MatchCase,
                    "Aa",
                    Text::SearchMatchCase,
                    cx,
                ))
                .child(self.render_find_in_files_toggle(
                    Toggle::WholeWord,
                    "W",
                    Text::SearchWholeWord,
                    cx,
                ))
                .child(self.render_find_in_files_toggle(
                    Toggle::Regex,
                    ".*",
                    Text::SearchRegex,
                    cx,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |root, _event, window, cx| {
                    cx.stop_propagation();
                    root.find_in_files_focus(window, cx);
                    root.find_in_files_type_into(which, cx);
                }),
            );

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .map(|column| match which {
                Field::Text => column.flex_1(),
                Field::Mask => column.flex_none().w(theme::space::SEARCH_MASK_FIELD),
            })
            .child(field)
            .into_any_element()
    }

    /// Whether `which` field's chevron is the menu the engine has open —
    /// M12.7 task 7.
    fn find_in_files_history_open(&self, which: Field) -> bool {
        matches!(
            self.context_menu.as_ref().map(|open| &open.target),
            Some(Target::FindHistory { field, .. }) if *field == which
        )
    }

    /// The checkbox before the mask: ticked while the mask is applied.
    fn render_find_in_files_use_mask(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let ticked = self
            .find_in_files_popup()
            .is_some_and(|popup| !popup.mask_off);
        let hint = ui.text(Text::FindInFilesUseMask).to_owned();
        div()
            .id("find-in-files-use-mask")
            .flex_none()
            .size(theme::space::CHECKBOX)
            .rounded(theme::radius::CHECKBOX)
            .border_1()
            .border_color(rgb(if ticked {
                theme.accent.brass
            } else {
                theme.text.faint
            }))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when(ticked, |this| {
                this.bg(rgb(theme.accent.brass))
                    .child(crate::icons::check().text_color(rgb(theme.surface.panel)))
            })
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| crate::view::tooltip::Tooltip::text(&hint, theme))
                    .into()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|root, _event, window, cx| {
                    cx.stop_propagation();
                    root.find_in_files_focus(window, cx);
                    root.find_in_files_use_mask(cx);
                }),
            )
            .into_any_element()
    }

    /// *N matches in M files*, *and more* when capped, *searching…* while
    /// it walks — and the pattern's own refusal when there is one, whose
    /// last line is the sentence a person acts on.
    fn find_in_files_count(&self) -> String {
        let ui = self.ui();
        let Some(popup) = self.find_in_files_popup() else {
            return String::new();
        };
        let walk = &popup.walk;
        if let Some(said) = walk.refused() {
            return said
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .unwrap_or(said)
                .to_owned();
        }
        if popup.query.is_empty() {
            return String::new();
        }
        let mut said = if walk.hits().is_empty() && walk.running() {
            String::new()
        } else {
            ui.matches_in_files(
                if walk.documents().is_empty() {
                    walk.hits().len()
                } else {
                    walk.documents()
                        .iter()
                        .map(|document| document.match_count)
                        .sum()
                },
                walk.files(),
            )
        };
        if walk.capped() {
            said.push(' ');
            said.push_str(ui.text(Text::SearchCapped));
        }
        if walk.running() {
            if !said.is_empty() {
                said.push_str(" · ");
            }
            said.push_str(ui.text(Text::SearchRunning));
        }
        said
    }

    /// A small square button of the head: pressed on the way down, so the
    /// press is not also the head's grab.
    fn find_in_files_button(
        &self,
        id: &'static str,
        on: bool,
        hint: String,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .capture_any_mouse_down(|_, window, _| window.prevent_default())
            .when(on, |button| button.bg(rgb(theme.accent.brass_bg)))
            .when(!on, |button| {
                button.hover(|button| {
                    button.bg(crate::theme::control_hover(theme, theme.surface.sunken))
                })
            })
            .text_size(theme::size::META)
            .text_color(rgb(if on {
                theme.accent.brass
            } else {
                theme.text.tertiary
            }))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| crate::view::tooltip::Tooltip::text(&hint, theme))
                    .into()
            })
    }

    /// One of the three toggles, drawn as the Search tool draws them.
    fn render_find_in_files_toggle(
        &self,
        which: Toggle,
        label: &'static str,
        hint: Text,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let ui = self.ui();
        let on = self
            .find_in_files_popup()
            .is_some_and(|popup| which.is_on(&popup.query));
        let id = match which {
            Toggle::MatchCase => "find-in-files-case",
            Toggle::WholeWord => "find-in-files-word",
            Toggle::Regex => "find-in-files-regex",
        };
        let content = if matches!(which, Toggle::MatchCase) {
            crate::icons::toolbar("icons/case-sensitive.svg")
                .text_color(rgb(self.theme.text.secondary))
                .flex_none()
                .into_any_element()
        } else {
            label.into_any_element()
        };
        self.find_in_files_button(id, on, ui.text(hint).to_owned())
            .child(content)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |root, _event, window, cx| {
                    cx.stop_propagation();
                    root.find_in_files_focus(window, cx);
                    root.find_in_files_toggle(which, cx);
                }),
            )
            .into_any_element()
    }

    /// The pin: lit while Enter leaves the popup up.
    fn render_find_in_files_pin(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let ui = self.ui();
        let pinned = self.find_in_files_popup().is_some_and(|popup| popup.pinned);
        let hint = ui.text(if pinned {
            Text::FindInFilesUnpin
        } else {
            Text::FindInFilesPin
        });
        self.find_in_files_button("find-in-files-pin", pinned, hint.to_owned())
            .child(
                crate::icons::toolbar("ui/head-pin.svg").text_color(rgb(if pinned {
                    self.theme.accent.brass
                } else {
                    self.theme.text.tertiary
                })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|root, _event, window, cx| {
                    cx.stop_propagation();
                    root.find_in_files_focus(window, cx);
                    root.find_in_files_pin(cx);
                }),
            )
            .into_any_element()
    }

    /// The cross, which closes the popup from anywhere — a pinned one
    /// included, whose Escape belongs to whatever holds the focus.
    fn render_find_in_files_close(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let ui = self.ui();
        self.find_in_files_button(
            "find-in-files-close",
            false,
            ui.text(Text::FindInFilesClose).to_owned(),
        )
        .child(
            crate::icons::glyph("icons/close.svg", theme::size::META)
                .text_color(rgb(self.theme.text.tertiary)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|root, _event, window, cx| {
                cx.stop_propagation();
                root.close_picker(crate::root::picker::Focus::Back, window, cx);
            }),
        )
        .into_any_element()
    }

    /// The hits, or the sentence that says why there are none.
    fn render_find_in_files_rows(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(popup) = self.find_in_files_popup() else {
            return div().into_any_element();
        };
        let count = popup.walk.hits().len();
        if count == 0 {
            // Three emptinesses: nothing typed, nothing found, and a walk
            // or a refusal the head is already saying.
            let note = if popup.walk.refused().is_some() || popup.walk.running() {
                None
            } else if popup.query.is_empty() {
                Some(Text::PanelSearchEmpty)
            } else {
                Some(Text::SearchNothingFound)
            };
            return div()
                .flex_1()
                .min_h(theme::space::PICKER_ROW * 3.0)
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme::size::UI)
                .text_color(rgb(theme.text.faint))
                .children(note.map(|note| self.ui().text(note).to_owned()))
                .into_any_element();
        }
        self.picker_rows(
            "find-in-files-list",
            count,
            theme::space::PICKER_ROW,
            |root, index, lit, _window, _cx| root.render_find_in_files_row(index, lit),
            cx,
        )
    }

    /// One hit: the line in the code face with its matches in brass, and
    /// the file's name and the line at the right.
    fn render_find_in_files_row(&self, index: usize, lit: bool) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(hit) = self
            .find_in_files_popup()
            .and_then(|popup| popup.walk.hits().get(index))
        else {
            return div().into_any_element();
        };
        let name = hit
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| hit.path.to_string_lossy().into_owned());
        let family: SharedString = self.code_font.clone();

        div()
            .when(
                self.find_in_files_popup().is_some_and(|popup| {
                    self.find_in_files_identity(&hit.path)
                        .is_some_and(|identity| popup.excluded.contains(identity))
                }),
                |this| this.opacity(0.4),
            )
            .h_full()
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .text_size(theme::size::META)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .font_family(family)
                    .text_color(rgb(theme.text.secondary))
                    .child(crate::view::search::painted(
                        hit,
                        crate::view::search::result_identity(
                            self.picker.as_ref().and_then(|open| open.project),
                            crate::Origin::Project,
                            hit,
                            self.find_in_files_popup()
                                .map_or(0, |popup| popup.walk.answered()),
                        ),
                        theme,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(260.0))
                    .overflow_hidden()
                    .text_color(rgb(theme.text.faint))
                    .child(format!("{name} {}", hit.line)),
            )
            .into_any_element()
    }
}

impl Root {
    fn search_label(&self, en: &'static str, ru: &'static str) -> &'static str {
        if self.ui().0 == guild_session::Language::Russian {
            ru
        } else {
            en
        }
    }
    fn render_project_replacement(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let Some(popup) = self.find_in_files_popup() else {
            return div().into_any_element();
        };
        div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .px(theme::space::CARD_PADDING)
            .pb(theme::space::CARD_GAP)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(theme::space::HEAD_TAB)
                    .rounded(theme::radius::ROW)
                    .border_1()
                    .border_color(rgb(self.theme.surface.divider))
                    .child(
                        crate::text_field::TextField::new(
                            "project-replacement",
                            popup.replacement.clone(),
                            cx.listener(|root, value: &String, _, cx| {
                                if let Some(popup) = root.find_in_files_popup_mut() {
                                    popup.replacement = value.clone();
                                }
                                cx.notify();
                            }),
                        )
                        .padded()
                        .placeholder(self.search_label("Replace with", "Заменить на"))
                        .when(popup.replacement_focus, |input| {
                            input.focus(&self.picker.as_ref().expect("find popup").focus)
                        })
                        .on_focus(cx.listener(|root, _, _, cx| {
                            if let Some(popup) = root.find_in_files_popup_mut() {
                                popup.replacement_focus = true;
                            }
                            cx.notify();
                        }))
                        .on_key(cx.listener(
                            |root, event: &gpui::KeyDownEvent, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "enter" => root.find_in_files_replace(false, window, cx),
                                    "tab" => {
                                        root.find_in_files_type_into(
                                            if event.keystroke.modifiers.shift {
                                                Field::Text
                                            } else {
                                                Field::Mask
                                            },
                                            cx,
                                        );
                                        root.find_in_files_focus(window, cx);
                                    }
                                    _ => {
                                        root.picker_key(event, window, cx);
                                    }
                                }
                            },
                        )),
                    ),
            )
            .child(
                self.find_in_files_button(
                    "project-preserve",
                    popup.preserve_case && !popup.query.regular_expression,
                    "Preserve case (literal search)".into(),
                )
                .child("aA")
                .when(popup.query.regular_expression, |this| this.opacity(0.4))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(popup) = root.find_in_files_popup_mut()
                            && !popup.query.regular_expression
                        {
                            popup.preserve_case = !popup.preserve_case;
                        }
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }
    fn render_project_replace_actions(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let Some(popup) = self.find_in_files_popup() else {
            return div().into_any_element();
        };
        let ready = !popup.applying
            && !popup.walk.running()
            && !popup.walk.capped()
            && popup.walk.refused().is_none()
            && popup.walk.generation() == popup.walk.answered();
        let mut row = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .id("project-replacement-status")
                    .max_h(theme::space::PREVIEW)
                    .overflow_y_scroll()
                    .text_color(rgb(self.theme.text.faint))
                    .child(if popup.walk.capped() {
                        self.search_label(
                            "Narrow the search before replacing: result limit reached",
                            "Сузьте поиск перед заменой: достигнут лимит результатов",
                        )
                        .to_owned()
                    } else {
                        popup.replace_status.clone()
                    }),
            );
        let buttons = div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                self.find_in_files_chip(
                    "project-replace-one",
                    self.search_label("Replace", "Заменить"),
                    false,
                )
                .when(!ready, |this| this.opacity(0.4))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _, window, cx| {
                        cx.stop_propagation();
                        root.find_in_files_replace(false, window, cx);
                    }),
                ),
            )
            .child(
                self.find_in_files_chip(
                    "project-replace-all",
                    self.search_label("Replace All", "Заменить все"),
                    false,
                )
                .when(!ready, |this| this.opacity(0.4))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _, window, cx| {
                        cx.stop_propagation();
                        root.find_in_files_replace(true, window, cx);
                    }),
                ),
            )
            .child(
                self.find_in_files_chip(
                    "project-exclude",
                    self.search_label("Exclude / Include File", "Исключить / включить файл"),
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|root, _, _, cx| {
                        cx.stop_propagation();
                        let path = root
                            .picker
                            .as_ref()
                            .and_then(|open| open.picker.lit())
                            .and_then(|row| {
                                root.find_in_files_popup()
                                    .and_then(|popup| popup.walk.hits().get(row))
                            })
                            .and_then(|hit| root.find_in_files_identity(&hit.path).cloned());
                        if let Some(path) = path
                            && let Some(popup) = root.find_in_files_popup_mut()
                            && !popup.excluded.remove(&path)
                        {
                            popup.excluded.insert(path);
                        }
                        cx.notify();
                    }),
                ),
            )
            .when(popup.applying, |this| {
                this.child(
                    self.find_in_files_chip(
                        "project-cancel-replace",
                        self.search_label("Cancel", "Отмена"),
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(popup) = root.find_in_files_popup_mut() {
                                popup.apply_task = None;
                                popup.applying = false;
                                popup.replace_status.push_str(&format!(
                                    " — cancelled; {} unprocessed files unchanged",
                                    popup.apply_remaining
                                ));
                            }
                            root.find_in_files_restart(cx);
                            cx.notify();
                        }),
                    ),
                )
            });
        row = row.child(buttons).child(div().text_color(rgb(self.theme.text.faint)).child(self.search_label("Edits use editor Undo and normal save policy. Regex uses Rust syntax; no lookarounds/backreferences.", "Замены используют Undo редактора и обычное сохранение. Regex: Rust, без lookaround/обратных ссылок.")));
        row.into_any_element()
    }
}
