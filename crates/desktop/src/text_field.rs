//! Single-line Guild fields: the component owns text, selection, IME and caret.
//! Hosts own submit, dismiss and list navigation; unchanged model values never
//! reset an in-progress selection. Keyed state lives exactly as long as its view.
use gpui::{
    App, ElementId, Entity, EntityInputHandler, FocusHandle, Focusable, KeyDownEvent, Window, div,
    prelude::*,
};
use gpui_component::input::{Input, InputEvent, InputState};
use std::rc::Rc;

type Change = Rc<dyn Fn(&String, &mut Window, &mut App)>;
type Key = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type Focus = Rc<dyn Fn(&(), &mut Window, &mut App)>;

/// The outlined box a field is drawn in where it must read as a field —
/// Settings' path fields, and the merge question's name: a hairline in
/// `text.tertiary` round a well of `surface.inset`.
pub(crate) fn frame(palette: crate::theme::Theme) -> gpui::Div {
    div()
        .cursor_text()
        .p(crate::theme::space::CARD_GAP)
        .rounded(crate::theme::radius::ROW)
        .border_1()
        .border_color(gpui::rgb(palette.text.tertiary))
        .bg(gpui::rgb(palette.surface.inset))
}

#[derive(Default)]
struct Fields(Vec<gpui::WeakEntity<InputState>>);
impl gpui::Global for Fields {}

/// Only our migrated fields bypass legacy host capture. File editors and
/// review forms keep their existing ancestor handlers.
pub(crate) fn focused(window: &Window, cx: &App) -> bool {
    cx.try_global::<Fields>().is_some_and(|fields| {
        fields
            .0
            .iter()
            .filter_map(|input| input.upgrade())
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    })
}

struct State {
    input: Entity<InputState>,
    value: String,
    selected: bool,
    change: Change,
    focus: Option<Focus>,
    blur: Option<Focus>,
    _subscription: gpui::Subscription,
    _observation: gpui::Subscription,
}

impl State {
    fn sync_value(&mut self, window: &mut Window, cx: &mut App) {
        let value = self.input.read(cx).value().to_string();
        if value != self.value {
            self.value = value.clone();
            (self.change)(&value, window, cx);
        }
    }
}

#[derive(IntoElement)]
pub struct TextField {
    id: ElementId,
    value: String,
    placeholder: String,
    keep_hint: bool,
    change: Change,
    key: Option<Key>,
    focus_event: Option<Focus>,
    blur_event: Option<Focus>,
    proxy: Option<FocusHandle>,
    autofocus: bool,
    input: Option<Entity<InputState>>,
    select: bool,
    initial_selection: Option<std::ops::Range<usize>>,
    disabled: bool,
    list: bool,
    vertical_scroll: bool,
    padded: bool,
}
impl TextField {
    pub fn new(
        id: impl Into<ElementId>,
        value: impl Into<String>,
        change: impl Fn(&String, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            placeholder: String::new(),
            keep_hint: false,
            change: Rc::new(change),
            key: None,
            focus_event: None,
            blur_event: None,
            proxy: None,
            autofocus: false,
            input: None,
            select: false,
            initial_selection: None,
            disabled: false,
            list: false,
            vertical_scroll: false,
            padded: false,
        }
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
    /// Keep the hint while the field is focused, as long as it is empty.
    /// For a field that is the whole of a prompt — a popup asking for a
    /// name — which has the keys from the moment it opens: the rule that
    /// hides a focused field's hint would hide that one for good.
    pub fn keep_hint(mut self) -> Self {
        self.keep_hint = true;
        self
    }
    pub fn focus(mut self, proxy: &FocusHandle) -> Self {
        self.proxy = Some(proxy.clone());
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Continue the same platform input session when a list reveals its field.
    /// The supplied component already owns its marked text and undo history.
    pub(crate) fn input(mut self, input: Entity<InputState>) -> Self {
        self.input = Some(input);
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.select = selected;
        self
    }
    /// Select a UTF-8 byte range on first mount only. Renders, model updates and
    /// focus restoration must not reset the person's caret or undo history.
    pub fn initial_selection(mut self, range: std::ops::Range<usize>) -> Self {
        self.initial_selection = Some(range);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn list(mut self) -> Self {
        self.list = true;
        self
    }
    /// Make the whole styled field interior focusable, including its padding.
    pub fn padded(mut self) -> Self {
        self.padded = true;
        self
    }
    /// Pass vertical wheel gestures to a scrolling host; keep horizontal gestures.
    pub fn vertical_scroll(mut self) -> Self {
        self.vertical_scroll = true;
        self
    }
    pub fn on_key(mut self, key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static) -> Self {
        self.key = Some(Rc::new(key));
        self
    }
    pub fn on_blur(mut self, blur: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.blur_event = Some(Rc::new(blur));
        self
    }
    pub fn on_focus(mut self, focus: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.focus_event = Some(Rc::new(focus));
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut fresh = false;
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            fresh = true;
            let input = self.input.clone().unwrap_or_else(|| {
                cx.new(|cx| {
                    let mut input = InputState::new(window, cx);
                    input.set_value(self.value.clone(), window, cx);
                    input
                })
            });
            if !cx.has_global::<Fields>() {
                cx.set_global(Fields::default());
            }
            cx.update_global::<Fields, _>(|fields, _| {
                fields.0.retain(|input| input.upgrade().is_some());
                fields.0.push(input.downgrade());
            });
            let subscription = cx.subscribe_in(
                &input,
                window,
                |state: &mut State, input, event, window, cx| match event {
                    InputEvent::Change => state.sync_value(window, cx),
                    InputEvent::Focus => {
                        if let Some(focus) = &state.focus {
                            focus(&(), window, cx);
                        }
                    }
                    InputEvent::Blur => {
                        // App deactivation also emits Blur, but does not move the
                        // window's focus. Disposing that still-focused field
                        // would send subsequent typing to the host's fallback.
                        if !input.read(cx).focus_handle(cx).is_focused(window)
                            && let Some(blur) = &state.blur
                        {
                            blur(&(), window, cx);
                        }
                    }
                    _ => {}
                },
            );
            // Marked-text updates/cancellation notify without InputEvent::Change.
            // Observe the actual value too; caret/focus notifications and model
            // writes (cached before set_value below) must not become user edits.
            let observation = cx.observe_in(&input, window, |state: &mut State, _, window, cx| {
                state.sync_value(window, cx);
            });
            State {
                input,
                value: self.value.clone(),
                selected: false,
                change: self.change.clone(),
                focus: self.focus_event.clone(),
                blur: self.blur_event.clone(),
                _subscription: subscription,
                _observation: observation,
            }
        });
        let input = state.read(cx).input.clone();
        let changed = state.read(cx).value != self.value;
        let selection_requested = self.select && !state.read(cx).selected;
        state.update(cx, |state, _| {
            state.selected = self.select;
            state.change = self.change;
            state.focus = self.focus_event;
            state.blur = self.blur_event;
            state.value = self.value.clone();
        });
        if changed {
            input.update(cx, |input, cx| input.set_value(self.value, window, cx));
        }
        let take_focus = !self.disabled
            && ((fresh && (self.autofocus || input.read(cx).focus_handle(cx).is_focused(window)))
                || self
                    .proxy
                    .as_ref()
                    .is_some_and(|focus| focus.is_focused(window)));
        if take_focus {
            // The public input operation starts its caret as well as moving
            // focus, including first mount before focus ancestry is painted.
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        let hint_label = self.placeholder.clone();
        input_placeholder(&input, self.placeholder, self.keep_hint, window, cx);
        if self.select && (fresh || changed || take_focus || selection_requested) {
            let input = input.downgrade();
            window.defer(cx, move |window, cx| {
                if let Some(input) = input.upgrade()
                    && input.read(cx).focus_handle(cx).is_focused(window)
                {
                    window.dispatch_action(Box::new(gpui_component::input::SelectAll), cx);
                }
            });
        }
        if fresh && let Some(range) = self.initial_selection {
            let input = input.downgrade();
            window.defer(cx, move |_, cx| {
                if let Some(input) = input.upgrade() {
                    input.update(cx, |input, cx| input.set_selected_range(range, cx));
                }
            });
        }
        let keys = self.key;
        let target = input.clone();
        let clipboard_target = input.clone();
        let mouse_target = input.clone();
        div()
            .id(self.id)
            .key_context(if self.list {
                "guild-field guild-list-field"
            } else {
                "guild-field"
            })
            .w_full()
            .min_w_0()
            .when(self.padded, |field| {
                field
                    .h_full()
                    .px(crate::theme::space::CARD_GAP)
                    .flex()
                    .items_center()
            })
            .capture_action(move |_: &gpui_component::input::Paste, window, cx| {
                paste(&clipboard_target, window, cx);
                cx.stop_propagation();
            })
            .capture_key_down(move |event, window, cx| {
                if self.list
                    && event.keystroke.key == "backspace"
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                {
                    if target.read(cx).value().is_empty() {
                        if let Some(key) = &keys {
                            key(event, window, cx);
                        }
                    } else {
                        window.dispatch_action(Box::new(gpui_component::input::Backspace), cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                if special_edit(&target, event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if host_key(event, self.list)
                    && target.update(cx, |input, cx| {
                        input.marked_text_range(window, cx).is_some()
                    })
                {
                    return;
                }
                if host_key(event, self.list) {
                    if let Some(key) = &keys {
                        key(event, window, cx);
                    } else if event.keystroke.key == "tab" {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                    }
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                if !self.disabled {
                    mouse_target.read(cx).focus_handle(cx).focus(window, cx);
                }
                cx.stop_propagation();
            })
            .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(move |event, _, cx| {
                let horizontal = match event.delta {
                    gpui::ScrollDelta::Pixels(delta) => delta.x.abs() > delta.y.abs(),
                    gpui::ScrollDelta::Lines(delta) => delta.x.abs() > delta.y.abs(),
                };
                if !self.vertical_scroll || horizontal {
                    cx.stop_propagation();
                }
            })
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                Input::new(&input)
                    .when(!hint_label.is_empty(), |input| input.aria_label(hint_label))
                    .appearance(false)
                    .disabled(self.disabled)
                    .w_full()
                    .px_0()
                    .py_0()
                    .text_size(crate::theme::size::UI)
                    .map(|input| gpui::Styled::h(input, crate::theme::space::HEAD_TAB)),
            )
    }
}

fn host_key(event: &KeyDownEvent, list: bool) -> bool {
    let key = &event.keystroke;
    matches!(key.key.as_str(), "enter" | "escape" | "tab")
        || (list
            && !key.modifiers.platform
            && !key.modifiers.control
            && matches!(key.key.as_str(), "up" | "down" | "pageup" | "pagedown"))
}

/// Binding overrides are scoped below the component input, never in an editor.
pub(crate) fn bindings() -> Vec<gpui::KeyBinding> {
    let mut keys: Vec<_> = [
        "enter",
        "cmd-enter",
        "escape",
        "tab",
        "shift-tab",
        "ctrl-w",
        "ctrl-u",
        "ctrl-backspace",
        "alt-backspace",
        "cmd-backspace",
        "cmd-v",
    ]
    .into_iter()
    .map(|key| gpui::KeyBinding::new(key, gpui::NoAction {}, Some("guild-field > Input")))
    .collect();
    keys.extend(
        [
            "backspace",
            "up",
            "down",
            "alt-up",
            "alt-down",
            "pageup",
            "pagedown",
        ]
        .into_iter()
        .map(|key| gpui::KeyBinding::new(key, gpui::NoAction {}, Some("guild-list-field > Input"))),
    );
    keys
}

fn paste(input: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    let Some(text) = cx
        .read_from_clipboard()
        .and_then(|clipboard| clipboard.text())
    else {
        return;
    };
    let text = text.lines().collect::<Vec<_>>().join(" ").trim().to_owned();
    if !text.is_empty() {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, &text, window, cx)
        });
    }
}

fn special_edit(
    input: &Entity<InputState>,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let key = &event.keystroke;
    let kind = match key.key.as_str() {
        "v" if key.modifiers.platform => 0,
        "w" if key.modifiers.control => 1,
        "backspace" if key.modifiers.platform || key.modifiers.control => 2,
        "backspace" if key.modifiers.alt => 1,
        "u" if key.modifiers.control => 2,
        _ => return false,
    };
    if kind == 0 {
        paste(input, window, cx);
        return true;
    }
    input.update(cx, |input, cx| {
        let Some(selection) = input
            .selected_text_range(false, window, cx)
            .map(|range| range.range)
        else {
            return;
        };
        let range = if selection.is_empty() {
            let text = input.value();
            let prefix: String = text
                .chars()
                .scan(0, |offset, c| {
                    *offset += c.len_utf16();
                    Some((*offset, c))
                })
                .take_while(|(offset, _)| *offset <= selection.start)
                .map(|(_, c)| c)
                .collect();
            let start = if kind == 2 {
                0
            } else {
                let mut before = prefix.clone();
                crate::edit::apply(&mut before, crate::edit::Edit::RubWord);
                before.encode_utf16().count()
            };
            start..selection.end
        } else {
            selection
        };
        input.replace_text_in_range(Some(range), "", window, cx);
    });
    true
}

// The hint is presentation only. Keep it separate from value/set_value so focus
// cannot edit a suggestion, create undo entries or interrupt marked text.
struct Hint {
    text: gpui::SharedString,
    /// Shown while focused too — `TextField::keep_hint`.
    keep: bool,
    _subscription: gpui::Subscription,
}
macro_rules! placeholder_policy {
    ($name:ident, $input:ty) => {
        pub(crate) fn $name(
            input: &Entity<$input>,
            text: impl Into<gpui::SharedString>,
            keep: bool,
            window: &mut Window,
            cx: &mut App,
        ) {
            let hint = window.use_keyed_state(
                ("field-placeholder", input.entity_id()),
                cx,
                |window, cx| {
                    let subscription = cx.subscribe_in(
                        input,
                        window,
                        |hint: &mut Hint, input, event, window, cx| {
                            if matches!(event, InputEvent::Focus | InputEvent::Blur) {
                                let text = if !hint.keep
                                    && input.read(cx).focus_handle(cx).is_focused(window)
                                {
                                    gpui::SharedString::default()
                                } else {
                                    hint.text.clone()
                                };
                                if input.read(cx).presentation().placeholder() != &text {
                                    input.update(cx, |input, cx| {
                                        input.set_placeholder(text, window, cx)
                                    });
                                }
                            }
                        },
                    );
                    Hint {
                        text: gpui::SharedString::default(),
                        keep,
                        _subscription: subscription,
                    }
                },
            );
            let text = text.into();
            hint.update(cx, |hint, _| {
                hint.text = text.clone();
                hint.keep = keep;
            });
            let shown = if !keep && input.read(cx).focus_handle(cx).is_focused(window) {
                gpui::SharedString::default()
            } else {
                text
            };
            if input.read(cx).presentation().placeholder() != &shown {
                input.update(cx, |input, cx| input.set_placeholder(shown, window, cx));
            }
        }
    };
}
placeholder_policy!(input_placeholder, InputState);
placeholder_policy!(textarea_placeholder, gpui_component::input::TextareaState);

/// Render the common hint policy with the control, never while its state is
/// constructed by a command or background-result handler outside layout.
#[derive(IntoElement)]
pub(crate) struct HintedTextarea {
    input: Entity<gpui_component::input::TextareaState>,
    hint: gpui::SharedString,
    appearance: bool,
}
impl HintedTextarea {
    pub(crate) fn new(
        input: &Entity<gpui_component::input::TextareaState>,
        hint: impl Into<gpui::SharedString>,
    ) -> Self {
        Self {
            input: input.clone(),
            hint: hint.into(),
            appearance: true,
        }
    }
    pub(crate) fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }
}
impl RenderOnce for HintedTextarea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        textarea_placeholder(&self.input, self.hint.clone(), false, window, cx);
        gpui_component::input::Textarea::new(&self.input)
            .aria_label(self.hint)
            .appearance(self.appearance)
    }
}

#[cfg(test)]
mod hint_tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext};
    use gpui_component::input::TextareaState;
    struct Multiline(Entity<TextareaState>);
    impl Render for Multiline {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            HintedTextarea::new(&self.0, "Commit message")
        }
    }
    #[gpui::test]
    fn multiline_hint_is_presentation_only_across_focus_and_undo(cx: &mut TestAppContext) {
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let input = cx.new(|cx| TextareaState::new(window, cx));
            made = Some(input.clone());
            let view = cx.new(|_| Multiline(input));
            gpui_component::Root::new(view, window, cx)
        });
        let input = made.unwrap();
        cx.update(|window, cx| {
            assert_eq!(
                input.read(cx).presentation().placeholder().as_str(),
                "Commit message"
            );
            input.read(cx).focus_handle(cx).focus(window, cx);
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert!(input.read(cx).presentation().placeholder().is_empty()));
        cx.simulate_input("actual\nmessage");
        cx.update(|window, cx| window.blur(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(input.read(cx).text().to_string(), "actual\nmessage");
            input.read(cx).focus_handle(cx).focus(window, cx);
        });
        cx.simulate_keystrokes("cmd-z");
        // The component records the line break separately; focusing the hint
        // must not insert another history entry between these real edits.
        cx.update(|_, cx| assert_eq!(input.read(cx).text().to_string(), "actual\n"));
        cx.simulate_keystrokes("cmd-z cmd-z");
        cx.update(|_, cx| assert_eq!(input.read(cx).text().to_string(), ""));
        cx.update(|window, cx| window.blur(cx));
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                input.read(cx).presentation().placeholder().as_str(),
                "Commit message"
            )
        });
    }
}
