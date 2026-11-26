//! These exercise platform callbacks against the real component; they do not
//! claim native macOS input-source/candidate-window automation.
use super::*;
use gpui::{Context, Render, TestAppContext, px};

struct Host {
    session: Entity<State>,
    focus: FocusHandle,
    query: Option<String>,
    opened: Vec<usize>,
}
impl Host {
    fn matched(&self) -> Option<usize> {
        crate::speed_search::Search::new(self.query.as_deref().unwrap_or(""), None)
            .matched(&[crate::speed_search::Candidate::new(0, "に.rs", 0)])
            .map(|row| row.key)
    }
    fn change(cx: &Context<Self>) -> Change {
        Rc::new(cx.listener(|host, value: &String, _, cx| {
            host.query = Some(value.clone());
            cx.notify();
        }))
    }
    fn activation(&self, cx: &Context<Self>) -> Activation {
        let input = self.session.read(cx).input.clone();
        Activation {
            inner: ElementInputHandler::new(Bounds::default(), input.clone()),
            input,
            focus: self.focus.clone(),
            change: Self::change(cx),
        }
    }
}
impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).w(gpui::px(350.)).child(
            Surface {
                query: self.query.clone(),
                focus: self.focus.clone(),
                no_match: false,
                theme: crate::theme::Theme::default(),
                change: Self::change(cx),
                key: Rc::new(cx.listener(|host, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter"
                        && let Some(key) = host.matched()
                    {
                        host.opened.push(key);
                    }
                    if matches!(event.keystroke.key.as_str(), "escape" | "enter") {
                        host.query = None;
                        window.focus(&host.focus, cx);
                        cx.notify();
                    }
                })),
                blur: Rc::new(|_, _, _| {}),
            }
            .render_session(self.session.clone(), window, cx),
        )
    }
}

#[gpui::test]
fn first_composition_survives_field_mount_commit_cancel_and_undo(cx: &mut TestAppContext) {
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| {
            let input = cx.new(|cx| InputState::new(window, cx));
            Host {
                session: cx.new(|_| State {
                    input,
                    active: false,
                }),
                focus: cx.focus_handle(),
                query: None,
                opened: Vec::new(),
            }
        });
        entity.read(cx).focus.clone().focus(window, cx);
        host = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(gpui::size(px(400.), px(200.)));
    let host = host.unwrap();
    let mut activation = cx.update(|_, cx| host.update(cx, |h, cx| h.activation(cx)));
    let input = activation.input.clone();
    cx.update(|window, cx| {
        activation.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
        assert_eq!(activation.marked_text_range(window, cx), Some(0..1));
        assert_eq!(host.read(cx).query.as_deref(), Some("に"));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("speed-search").is_some());
    cx.update(|window, cx| {
        assert_eq!(
            host.read(cx).session.read(cx).input,
            input,
            "mount must continue the same component, not reseed its value"
        );
        assert!(input.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(activation.marked_text_range(window, cx), Some(0..1));
        assert_eq!(
            activation
                .selected_text_range(false, window, cx)
                .unwrap()
                .range,
            1..1
        );
        // After mounting, the component's own platform handler receives the
        // replacement/commit; this is not a sequence of synthetic key_chars.
        let mut platform = ElementInputHandler::new(Bounds::default(), input.clone());
        platform.replace_and_mark_text_in_range(None, "日本", Some(2..2), window, cx);
        assert_eq!(platform.marked_text_range(window, cx), Some(0..2));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(host.read(cx).query.as_deref(), Some("日本"));
        let mut platform = ElementInputHandler::new(Bounds::default(), input.clone());
        assert_eq!(platform.marked_text_range(window, cx), Some(0..2));
        platform.replace_text_in_range(None, "日本語", window, cx);
        assert_eq!(platform.marked_text_range(window, cx), None);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(host.read(cx).query.as_deref(), Some("日本語")));
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some(""),
            "undo includes the first composition"
        )
    });
    cx.simulate_keystrokes("cmd-shift-z escape");
    cx.update(|_, cx| {
        assert!(host.read(cx).query.is_none());
        assert_ne!(
            host.read(cx).session.read(cx).input,
            input,
            "dismissal discards composition and undo"
        );
    });
    let mut activation = cx.update(|_, cx| host.update(cx, |h, cx| h.activation(cx)));
    let mounted_input = activation.input.clone();
    cx.update(|window, cx| {
        activation.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("speed-search").is_some());
    cx.update(|window, cx| {
        assert_eq!(
            host.read(cx).matched(),
            Some(0),
            "preedit is initially a match"
        );
        assert!(mounted_input.read(cx).focus_handle(cx).is_focused(window));
        // The mounted component owns platform dispatch now; do not call the
        // dormant Activation or insertText to simulate setMarkedText("").
        let mut platform = ElementInputHandler::new(Bounds::default(), mounted_input.clone());
        platform.replace_and_mark_text_in_range(None, "", Some(0..0), window, cx);
        assert!(mounted_input.read(cx).value().is_empty());
        assert_eq!(platform.marked_text_range(window, cx), None);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some(""),
            "cancel after mount must clear the model"
        );
        assert_eq!(host.read(cx).session.read(cx).input, mounted_input);
        assert!(
            crate::speed_search::ranges("に.rs", host.read(cx).query.as_deref().unwrap())
                .is_empty(),
            "canceled preedit has no highlight"
        );
        assert_eq!(host.read(cx).matched(), None);
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(
            host.read(cx).opened.is_empty(),
            "Enter must not open a canceled match"
        )
    });
    let mut activation = cx.update(|_, cx| host.update(cx, |h, cx| h.activation(cx)));
    let unmarked_input = activation.input.clone();
    cx.update(|window, cx| {
        activation.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let mut platform = ElementInputHandler::new(Bounds::default(), unmarked_input.clone());
        platform.replace_and_mark_text_in_range(None, "日本", Some(2..2), window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(host.read(cx).query.as_deref(), Some("日本"));
        let mut platform = ElementInputHandler::new(Bounds::default(), unmarked_input.clone());
        platform.unmark_text(window, cx);
        assert_eq!(platform.marked_text_range(window, cx), None);
        assert_eq!(unmarked_input.read(cx).value().as_str(), "日本");
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(host.read(cx).session.read(cx).input, unmarked_input);
        assert_eq!(host.read(cx).query.as_deref(), Some("日本"));
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some(""),
            "unmark preserves the composition undo transaction"
        )
    });
    cx.simulate_keystrokes("escape");
    let mut activation = cx.update(|_, cx| host.update(cx, |h, cx| h.activation(cx)));
    cx.update(|window, cx| {
        activation.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
        activation.replace_text_in_range(None, "", window, cx);
        activation.unmark_text(window, cx);
        assert_eq!(activation.marked_text_range(window, cx), None);
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some(""),
            "cancel before mount must not leave a stale query"
        );
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape s");
    cx.update(|_, cx| {
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some("s"),
            "a new Latin input arrives once"
        )
    });
    cx.simulate_keystrokes("cmd-z escape");
    let mut stale = cx.update(|_, cx| host.update(cx, |h, cx| h.activation(cx)));
    cx.update(|window, cx| {
        let other = cx.focus_handle();
        other.focus(window, cx);
        assert!(!stale.accepts_text_input(window, cx));
        stale.replace_text_in_range(None, "wrong owner", window, cx);
        assert!(host.read(cx).query.is_none());
        assert!(stale.input.read(cx).value().is_empty());
        host.read(cx).focus.clone().focus(window, cx);
    });
    cx.simulate_input("ы");
    cx.update(|_, cx| {
        assert_eq!(
            host.read(cx).query.as_deref(),
            Some("ы"),
            "a new Cyrillic input arrives once"
        )
    });
}
