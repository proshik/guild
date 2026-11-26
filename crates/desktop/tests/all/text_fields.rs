use gpui::{Context, Entity, FocusHandle, Render, TestAppContext, Window, div, prelude::*, px};
use guild_desktop::text_field::TextField;

struct Form {
    value: String,
    focus: FocusHandle,
    keys: Vec<String>,
    context: &'static str,
    changes: usize,
    select: bool,
}
impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().key_context("guild").child(
            div().key_context(self.context).w(px(340.)).child(
                TextField::new(
                    "field",
                    self.value.clone(),
                    cx.listener(|form, value: &String, _, cx| {
                        form.value = value.clone();
                        form.changes += 1;
                        cx.notify();
                    }),
                )
                .placeholder("Search fixture")
                .focus(&self.focus)
                .selected(self.select)
                .list()
                .on_key(cx.listener(|form, event: &gpui::KeyDownEvent, _, cx| {
                    form.keys.push(event.keystroke.key.clone());
                    cx.notify();
                })),
            ),
        )
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    context: &'static str,
) -> (Entity<Form>, &'a mut gpui::VisualTestContext) {
    let mut form = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let view = cx.new(|cx| Form {
            value: "https://old.test/team/old.git".into(),
            focus: cx.focus_handle(),
            keys: vec![],
            context,
            changes: 0,
            select: false,
        });
        view.read(cx).focus.clone().focus(window, cx);
        form = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(500.), px(180.)));
    (form.unwrap(), cx)
}

#[gpui::test]
fn selecting_all_replaces_the_clone_url(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "clone-project");
    cx.simulate_keystrokes("cmd-a x");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "x"));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, ""));
}

#[gpui::test]
fn movement_selection_clipboard_and_model_refresh_keep_the_caret(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "settings-search");
    cx.simulate_keystrokes("cmd-a a b c cmd-left right shift-right cmd-x");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "ac"));
    cx.simulate_keystrokes("cmd-v");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "abc"));
    cx.simulate_keystrokes("cmd-right shift-cmd-left z");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "z"));
    cx.simulate_keystrokes("cmd-z");
    // The upgraded component undoes the last selection replacement separately.
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "abc"));
    cx.simulate_keystrokes("cmd-shift-z");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "z"));
}

#[gpui::test]
fn unicode_paste_readline_deletion_and_host_keys(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "picker");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("проект/тест\nимя".into()))
    });
    cx.simulate_keystrokes("cmd-a cmd-v");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "проект/тест имя"));
    cx.simulate_keystrokes("cmd-left alt-right alt-backspace");
    cx.update(|_, cx| assert!(!form.read(cx).value.starts_with("проект")));
    cx.simulate_keystrokes("cmd-right ctrl-w");
    cx.update(|_, cx| assert!(!form.read(cx).value.ends_with("имя")));
    cx.simulate_keystrokes("ctrl-u enter escape tab shift-tab down alt-up");
    cx.update(|_, cx| {
        assert_eq!(form.read(cx).value, "");
        assert_eq!(
            form.read(cx).keys,
            ["enter", "escape", "tab", "tab", "down", "up"]
        );
    });
}

#[gpui::test]
fn standard_edits_survive_every_migrated_host_context(cx: &mut TestAppContext) {
    for context in [
        "clone-project",
        "picker",
        "search",
        "log",
        "settings-search",
        "tree",
        "terminal",
        "stream",
        "guild",
        "diff-window",
    ] {
        let (form, visual) = open(cx, context);
        visual.simulate_keystrokes("cmd-a a b cmd-left right shift-right x cmd-right backspace");
        visual.update(|_, cx| assert_eq!(form.read(cx).value, "a", "{context}"));
        visual.simulate_keystrokes("cmd-a cmd-c cmd-right cmd-v");
        visual.update(|_, cx| assert_eq!(form.read(cx).value, "aa", "{context}"));
    }
}

struct CloneFields {
    form: guild_desktop::clone_project::Form,
    directory: bool,
    focus: FocusHandle,
}
impl Render for CloneFields {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("guild")
            .child(
                div()
                    .key_context("clone-project")
                    .w(px(400.))
                    .children([false, true].map(|directory| {
                        let value = if directory {
                            self.form.directory.clone()
                        } else {
                            self.form.url.clone()
                        };
                        TextField::new(
                            if directory { "directory" } else { "url" },
                            value,
                            cx.listener(move |form, value: &String, _, cx| {
                                if directory {
                                    form.form.set_directory(value.clone());
                                } else {
                                    form.form.set_url(value.clone());
                                }
                                cx.notify();
                            }),
                        )
                        .when(self.directory == directory, |input| {
                            input.focus(&self.focus)
                        })
                        .selected(true)
                        .on_focus(cx.listener(move |form, _, _, cx| {
                            form.directory = directory;
                            cx.notify();
                        }))
                        .on_key(cx.listener(|form, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "tab" {
                                form.directory = !form.directory;
                                form.focus.focus(window, cx);
                                cx.notify();
                            }
                        }))
                        .map(|input| {
                            div()
                                .debug_selector(move || {
                                    if directory {
                                        "directory".into()
                                    } else {
                                        "url".into()
                                    }
                                })
                                .child(input)
                        })
                    })),
            )
    }
}

#[gpui::test]
fn clone_fields_tab_click_selection_and_external_suggestions(cx: &mut TestAppContext) {
    let mut form = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let view = cx.new(|cx| {
            let mut form = guild_desktop::clone_project::Form::new(std::path::Path::new("/tmp"));
            form.set_url("https://host/old.git".into());
            CloneFields {
                form,
                directory: false,
                focus: cx.focus_handle(),
            }
        });
        view.read(cx).focus.clone().focus(window, cx);
        form = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    let form = form.unwrap();
    cx.simulate_resize(gpui::size(px(500.), px(200.)));
    // The opening prefill is a real selection. Native clipboard paste replaces it.
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            "https://host/new.git".into(),
        ))
    });
    cx.simulate_keystrokes("cmd-v");
    cx.update(|_, cx| assert_eq!(form.read(cx).form.directory, "/tmp/new"));
    cx.simulate_keystrokes("tab cmd-right backspace x");
    cx.update(|_, cx| assert_eq!(form.read(cx).form.directory, "/tmp/nex"));
    cx.simulate_keystrokes("shift-tab cmd-right left shift-left z");
    cx.update(|_, cx| {
        assert_eq!(form.read(cx).form.url, "https://host/new.gzt");
        assert_eq!(form.read(cx).form.directory, "/tmp/nex");
    });
    let bounds = cx.debug_bounds("directory").expect("second field drawn");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    cx.simulate_keystrokes("cmd-a q");
    cx.update(|_, cx| {
        assert_eq!(form.read(cx).form.directory, "q");
        assert_eq!(form.read(cx).form.url, "https://host/new.gzt");
    });
}

#[gpui::test]
fn list_backspace_deletes_text_but_delegates_an_empty_query(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "picker");
    cx.simulate_keystrokes("cmd-a x backspace backspace");
    cx.update(|_, cx| {
        assert_eq!(form.read(cx).value, "");
        assert_eq!(form.read(cx).keys, ["backspace"]);
    });
}

#[gpui::test]
fn text_input_and_deletion_respect_unicode_scalars(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "clone-project");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("a🙂é/путь");
    cx.simulate_keystrokes("cmd-left right delete");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "aé/путь"));
    cx.simulate_keystrokes("right backspace");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "a/путь"));
    cx.simulate_keystrokes("cmd-right ctrl-w");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "a/"));
}

#[gpui::test]
fn paste_action_and_empty_clipboard_match_keyboard_paste(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "picker");
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("one\ntwo".into())));
    cx.dispatch_action(gpui_component::input::Paste);
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "one two"));
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
    cx.dispatch_action(gpui_component::input::Paste);
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "one two"));
}

#[gpui::test]
fn an_explicit_selection_request_reselects_an_existing_field(cx: &mut TestAppContext) {
    let (form, cx) = open(cx, "picker");
    cx.simulate_keystrokes("cmd-a a b c left");
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select = true;
            cx.notify();
        })
    });
    cx.simulate_keystrokes("x");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, "x"));
}

struct WheelHost {
    vertical: bool,
    received: usize,
}
impl Render for WheelHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(340.))
            .h(px(100.))
            .on_scroll_wheel(cx.listener(|host, _, _, _| host.received += 1))
            .child(
                div().debug_selector(|| "wheel-input".into()).child(
                    TextField::new("wheel-field", "A long input remains editable", |_, _, _| {})
                        .when(self.vertical, |field| field.vertical_scroll()),
                ),
            )
    }
}
#[gpui::test]
fn scrolling_hosts_opt_in_without_leaking_other_fields_or_horizontal_gestures(
    cx: &mut TestAppContext,
) {
    for vertical in [false, true] {
        let mut host = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            guild_desktop::editor::init(cx);
            let view = cx.new(|_| WheelHost {
                vertical,
                received: 0,
            });
            host = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        visual.simulate_resize(gpui::size(px(500.), px(180.)));
        visual.run_until_parked();
        let position = visual.debug_bounds("wheel-input").unwrap().center();
        let host = host.unwrap();
        for delta in [
            gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-15.))),
            gpui::ScrollDelta::Lines(gpui::point(0., -1.)),
        ] {
            visual.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta,
                ..Default::default()
            });
            visual.run_until_parked();
        }
        assert_eq!(
            visual.update(|_, cx| host.read(cx).received),
            if vertical { 2 } else { 0 }
        );
        for delta in [
            gpui::ScrollDelta::Pixels(gpui::point(px(-15.), px(-1.))),
            gpui::ScrollDelta::Lines(gpui::point(-2., -0.1)),
        ] {
            visual.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta,
                ..Default::default()
            });
            visual.run_until_parked();
        }
        assert_eq!(
            visual.update(|_, cx| host.read(cx).received),
            if vertical { 2 } else { 0 },
            "horizontal gesture escaped field ownership"
        );
    }
}

#[gpui::test]
fn placeholder_hides_on_focus_without_changing_text_or_undo(cx: &mut TestAppContext) {
    use gpui::Focusable;
    use gpui_component::{WindowExt, input::AnyInputState};
    let (form, cx) = open(cx, "settings-search");
    cx.simulate_keystrokes("cmd-a backspace");
    let input = cx.update(|window, cx| {
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("input")
        };
        assert!(
            input.read(cx).presentation().placeholder().is_empty(),
            "focused hint"
        );
        assert_eq!(form.read(cx).value, "");
        input
    });
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(
            input.read(cx).presentation().placeholder().as_str(),
            "Search fixture"
        );
        assert_eq!(form.read(cx).value, "");
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.simulate_input("actual suggestion");
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(form.read(cx).value, "actual suggestion");
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| assert_eq!(form.read(cx).value, ""));
}

struct MountedField {
    shown: bool,
    proxy: FocusHandle,
    automatic: bool,
}
impl Render for MountedField {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("guild")
            .size_full()
            .when(self.shown, |body| {
                body.child(
                    TextField::new("mounted", "", |_, _, _| {})
                        .placeholder("Display name")
                        .when(self.automatic, |field| field.autofocus())
                        .when(!self.automatic, |field| field.focus(&self.proxy)),
                )
            })
    }
}
#[gpui::test]
fn first_mount_focus_paints_a_real_blinking_caret(cx: &mut TestAppContext) {
    use gpui_component::{WindowExt, input::AnyInputState};
    for automatic in [true, false] {
        let (host, cx) = cx.add_window_view(|window, cx| {
            guild_desktop::editor::init(cx);
            let host = cx.new(|cx| MountedField {
                shown: false,
                proxy: cx.focus_handle(),
                automatic,
            });
            gpui_component::Root::new(host, window, cx)
        });
        cx.update(|window, _| window.activate_window());
        // Add a field to an already active host, just like Add app / opening a dialog.
        cx.update(|window, cx| {
            host.update(cx, |root, cx| {
                let field = root.view().clone().downcast::<MountedField>().unwrap();
                field.update(cx, |field, cx| {
                    field.shown = true;
                    if !automatic {
                        field.proxy.focus(window, cx);
                    }
                    cx.notify();
                });
            })
        });
        cx.run_until_parked();
        let input = cx.update(|window, cx| {
            assert!(window.is_window_active());
            let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
                panic!("real input must own focus")
            };
            assert!(input.read(cx).presentation().placeholder().is_empty());
            input
        });
        let mut phases = Vec::new();
        for _ in 0..8 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
            phases.push(cx.update(|window, cx| {
                let (bounds, _) = input.read(cx).cursor_layout().expect("caret layout");
                window
                    .painted_quads()
                    .iter()
                    .any(|quad| quad.bounds == bounds.scale(window.scale_factor()))
            }));
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(250));
            cx.run_until_parked();
        }
        assert!(
            phases.contains(&true),
            "no visible initial caret: automatic={automatic}, phases={phases:?}"
        );
        assert!(phases.contains(&false), "caret must blink: {phases:?}");
    }
}

struct PromptField;
impl Render for PromptField {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().key_context("guild").size_full().child(
            TextField::new("prompt", "", |_, _, _| {})
                .placeholder("Group name")
                .keep_hint()
                .autofocus(),
        )
    }
}
/// A field that is the whole of its popup has the keys from the moment it
/// opens, so the rule that hides a hint while focused would hide it for good:
/// such a field keeps its hint while it is empty, focused or not.
#[gpui::test]
fn a_prompt_field_keeps_its_hint_while_focused(cx: &mut TestAppContext) {
    use gpui::Focusable;
    use gpui_component::{WindowExt, input::AnyInputState};
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let view = cx.new(|_| PromptField);
        gpui_component::Root::new(view, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let input = cx.update(|window, cx| {
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("the field has the keys")
        };
        assert_eq!(
            input.read(cx).presentation().placeholder().as_str(),
            "Group name"
        );
        input
    });
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(
            input.read(cx).presentation().placeholder().as_str(),
            "Group name"
        );
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            input.read(cx).presentation().placeholder().as_str(),
            "Group name",
            "focus regained keeps the hint"
        );
    });
}
