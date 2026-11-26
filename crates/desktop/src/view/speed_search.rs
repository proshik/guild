//! List-local platform input continues in the shared compact TextField.
use crate::{text_field::TextField, theme};
use gpui::{
    App, Bounds, ElementInputHandler, Entity, FocusHandle, Focusable, InputHandler, KeyDownEvent,
    Pixels, Point, UTF16Selection, Window, canvas, div, prelude::*, rgb,
};
use gpui_component::input::InputState;
use std::{ops::Range, rc::Rc};

type Change = Rc<dyn Fn(&String, &mut Window, &mut App)>;
type Key = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type Blur = Rc<dyn Fn(&(), &mut Window, &mut App)>;

struct State {
    input: Entity<InputState>,
    active: bool,
}

#[derive(IntoElement)]
struct Surface {
    query: Option<String>,
    focus: FocusHandle,
    no_match: bool,
    theme: theme::Theme,
    change: Change,
    key: Key,
    blur: Blur,
}

pub(crate) fn field(
    query: Option<&str>,
    focus: &FocusHandle,
    no_match: bool,
    theme: theme::Theme,
    change: impl Fn(&String, &mut Window, &mut App) + 'static,
    key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    blur: impl Fn(&(), &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    Surface {
        query: query.map(str::to_owned),
        focus: focus.clone(),
        no_match,
        theme,
        change: Rc::new(change),
        key: Rc::new(key),
        blur: Rc::new(blur),
    }
    .into_any_element()
}

impl RenderOnce for Surface {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Two eligible lists can share one view (Push commits and files).
        // Their platform sessions follow the actual focus owner, not a common label.
        let state = window.use_keyed_state(
            (gpui::ElementId::from(&self.focus), "speed-search-session"),
            cx,
            |window, cx| State {
                input: cx.new(|cx| InputState::new(window, cx)),
                active: false,
            },
        );
        self.render_session(state, window, cx)
    }
}
impl Surface {
    fn render_session(
        self,
        state: Entity<State>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::AnyElement {
        let active = self.query.is_some();
        let input = state.update(cx, |state, cx| {
            if state.active && !active {
                // A new session must not inherit composition, selection or undo.
                state.input = cx.new(|cx| InputState::new(window, cx));
            }
            state.active = active;
            state.input.clone()
        });
        let Some(query) = self.query else {
            // No synthetic field: register a platform handler only while the
            // actual list owns focus, before the very first native IME event.
            return div()
                .h_0()
                .w_full()
                .flex_none()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &self.focus,
                                Activation {
                                    inner: ElementInputHandler::new(bounds, input.clone()),
                                    input: input.clone(),
                                    focus: self.focus.clone(),
                                    change: self.change.clone(),
                                },
                                cx,
                            );
                        },
                    )
                    .absolute()
                    .w_full()
                    .h(theme::space::TREE_ROW),
                )
                .into_any_element();
        };
        let editing = input.clone();
        div()
            .id("speed-search")
            .debug_selector(|| "speed-search".to_owned())
            .flex_none()
            .min_w_0()
            .w_full()
            .rounded(theme::radius::ROW)
            .border_1()
            .border_color(rgb(if self.no_match {
                self.theme.accent.rust
            } else {
                self.theme.accent.patina
            }))
            .bg(rgb(self.theme.surface.inset))
            .px(theme::space::TREE_PADDING)
            .child(
                TextField::new(
                    ("speed-search-input", input.entity_id()),
                    query,
                    move |value, window, cx| {
                        // Delayed Change events from a dismissed field must not start
                        // another session after focus has moved to a different host.
                        if editing.read(cx).focus_handle(cx).is_focused(window) {
                            (self.change)(value, window, cx);
                        }
                    },
                )
                .input(input)
                .autofocus()
                .list()
                .on_key(move |event, window, cx| (self.key)(event, window, cx))
                .on_blur(move |event, window, cx| (self.blur)(event, window, cx)),
            )
            .into_any_element()
    }
}

/// Forward native edits, including the first marked range, to the very same
/// InputState the visible TextField will use. Raw key_char is never committed.
struct Activation {
    inner: ElementInputHandler<InputState>,
    input: Entity<InputState>,
    focus: FocusHandle,
    change: Change,
}
impl Activation {
    fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.input.read(cx).focus_handle(cx).is_focused(window)
    }
    fn changed(&self, previously_nonempty: bool, window: &mut Window, cx: &mut App) {
        let value = self.input.read(cx).value().to_string();
        if previously_nonempty
            || (!value.is_empty()
                && value.chars().all(|c| !c.is_control())
                && !value.chars().all(char::is_whitespace))
        {
            (self.change)(&value, window, cx);
        }
    }
}
impl InputHandler for Activation {
    fn selected_text_range(
        &mut self,
        ignore: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.inner.selected_text_range(ignore, window, cx)
    }
    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.inner.marked_text_range(window, cx)
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.inner.text_for_range(range, adjusted, window, cx)
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.owns_focus(window, cx) {
            let previously_nonempty = !self.input.read(cx).value().is_empty();
            if !previously_nonempty && text.chars().all(char::is_whitespace) {
                return;
            }
            self.inner.replace_text_in_range(range, text, window, cx);
            self.changed(previously_nonempty, window, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.owns_focus(window, cx) {
            let previously_nonempty = !self.input.read(cx).value().is_empty();
            self.inner
                .replace_and_mark_text_in_range(range, text, selected, window, cx);
            self.changed(previously_nonempty, window, cx);
        }
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        if self.owns_focus(window, cx) {
            self.inner.unmark_text(window, cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.inner
            .bounds_for_range(range, window, cx)
            .or_else(|| self.inner.element_bounds(window, cx))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.inner.character_index_for_point(point, window, cx)
    }
    fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.owns_focus(window, cx)
    }
    // Keep the default prefers_ime_for_printable_keys=false while dormant:
    // Commit's first Space is a checkbox action. Unhandled printable keys go
    // to inputContext; an existing marked range always takes IME priority.
}

pub(crate) fn label(text: String, query: &str, theme: theme::Theme) -> gpui::StyledText {
    let marks = crate::speed_search::ranges(&text, query)
        .into_iter()
        .map(|r| {
            (
                r,
                gpui::HighlightStyle {
                    color: Some(rgb(theme.text.primary).into()),
                    background_color: Some(rgb(theme.accent.highlight).into()),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    gpui::StyledText::new(text).with_highlights(marks)
}

#[cfg(test)]
mod tests;
