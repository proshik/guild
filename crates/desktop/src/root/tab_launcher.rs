//! Momentary new-tab menu. It never takes the input focus or binds a shortcut.
use crate::{
    Root,
    view::menu::{Anchor, Target},
};
use gpui::{Context, Window};

#[derive(Default)]
pub(crate) struct Launcher {
    held: bool,
    used: bool,
    pub visible: bool,
    ear: Option<gpui::Subscription>,
    focus: Option<gpui::FocusHandle>,
    blur: Option<gpui::Subscription>,
    navigation: std::rc::Rc<std::cell::Cell<bool>>,
}

impl Launcher {
    fn modifiers(&mut self, held: bool) -> bool {
        let entered = held && !self.held;
        if !held {
            self.used = false;
            self.navigation.set(false);
        }
        self.held = held;
        entered
    }
    fn digit(&mut self, key: &str, repeated: bool) -> Option<usize> {
        if !self.held || self.used {
            return None;
        }
        self.used = true;
        if repeated {
            return None;
        }
        match key {
            "1" => Some(0),
            "2" => Some(1),
            "3" => Some(2),
            "4" => Some(3),
            _ => None,
        }
    }
}

impl Root {
    pub(crate) fn hear_tab_launcher(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.tab_launcher.ear.is_some() {
            return;
        }
        // Which agents are installed is `root::availability`'s, asked on
        // the watch loop's clock; the menu below reads its answer.
        let owner = window.window_handle();
        let root = cx.weak_entity();
        let navigation = self.tab_launcher.navigation.clone();
        // Bound shortcuts dispatch before raw capture. Dismiss the transient card
        // after their dispatch, leaving the original focus and action untouched.
        self.tab_launcher.ear = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != owner
                || matches!(
                    event.keystroke.key.as_str(),
                    "shift" | "platform" | "control" | "alt" | "function"
                )
            {
                return;
            }
            let modifiers = event.keystroke.modifiers;
            if navigation.get()
                && modifiers.platform
                && modifiers.control
                && !modifiers.shift
                && !modifiers.alt
                && !modifiers.function
                && matches!(
                    event.keystroke.key.as_str(),
                    "up" | "down" | "enter" | "escape"
                )
            {
                // These four keys belong to this held gesture, even when the
                // focused editor has a binding for them. Raw capture runs next.
                cx.stop_propagation();
                return;
            }
            let (bindings, partial) = cx
                .key_bindings()
                .borrow()
                .bindings_for_input(std::slice::from_ref(&event.keystroke), &event.context_stack);
            if bindings.is_empty() && !partial {
                return;
            }
            navigation.set(false);
            let root = root.clone();
            cx.defer(move |cx| {
                let _ = owner.update(cx, |_, window, cx| {
                    let _ = root.update(cx, |root, cx| {
                        if root.tab_launcher.held && root.tab_launcher.visible {
                            root.tab_launcher.used = true;
                            root.dismiss_tab_launcher(window, cx);
                        }
                    });
                });
            });
        }));
    }

    pub(crate) fn repair_tab_launcher_selection(&mut self) {
        if !self.tab_launcher.visible {
            return;
        }
        let Some(open) = self.context_menu.as_mut() else {
            return;
        };
        let Target::NewTab { available } = &open.target else {
            return;
        };
        let rows = crate::view::menu::strip::ordered_new_tab(
            available,
            &self.settings.tab_launcher_order(),
        );
        if !open.lit.is_some_and(|index| matches!(rows.get(index), Some(crate::view::menu::Entry::Choice(choice)) if choice.enabled)) {
            open.lit = crate::view::menu::step(&rows, None, true);
        }
    }

    pub(crate) fn dismiss_tab_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tab_launcher.blur = None;
        self.tab_launcher.focus = None;
        if self.tab_launcher.visible {
            if self
                .context_menu
                .as_ref()
                .is_some_and(|open| matches!(open.target, Target::NewTab { .. }))
            {
                self.close_context_menu(window, cx);
            }
            self.tab_launcher.visible = false;
            cx.notify();
        }
    }

    /// Programmatic reset is cancellation, never a modifier-release gesture.
    pub(crate) fn cancel_tab_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tab_launcher.modifiers(false);
        self.tab_launcher.used = true;
        self.dismiss_tab_launcher(window, cx);
    }

    pub(crate) fn tab_launcher_modifiers(
        &mut self,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = modifiers.platform
            && modifiers.control
            && !modifiers.shift
            && !modifiers.alt
            && !modifiers.function;
        let entered = self.tab_launcher.modifiers(held);
        if !held {
            // Adding a modifier hands the chord to another shortcut. Do not
            // rearm if that modifier is released while Cmd+Ctrl remain down.
            if modifiers.platform && modifiers.control {
                self.tab_launcher.used = true;
            }
            self.dismiss_tab_launcher(window, cx);
            return;
        }
        if !entered
            || self.tab_launcher.used
            || self.recording.is_some()
            || !window.is_window_active()
            || self.active_project.is_none()
            || self.context_menu.is_some()
            || self.picker.is_some()
        {
            return;
        }
        let area = self.focused_area();
        let Some(bounds) = self
            .strip_at
            .borrow()
            .get(&area)
            .and_then(|places| places[0])
        else {
            return;
        };
        self.strip_menu_area = area;
        self.tab_launcher.visible = true;
        self.tab_launcher.navigation.set(true);
        // The existing availability probe runs in the background; never probe
        // executable paths synchronously on every modifier gesture.
        self.open_menu(
            Target::NewTab {
                available: self.available.shown().to_vec(),
            },
            Anchor::Below(bounds),
            window,
            cx,
        );
        self.repair_tab_launcher_selection();
        self.tab_launcher.focus = window.focused(cx);
        self.tab_launcher.blur = self.tab_launcher.focus.as_ref().map(|focus| {
            cx.on_blur(focus, window, |root, window, cx| {
                root.tab_launcher.used = true;
                root.dismiss_tab_launcher(window, cx);
            })
        });
    }

    pub(crate) fn tab_launcher_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if !self.tab_launcher.held
            || !modifiers.platform
            || !modifiers.control
            || modifiers.shift
            || modifiers.alt
            || modifiers.function
        {
            return false;
        }
        let digit = matches!(event.keystroke.key.as_str(), "1" | "2" | "3" | "4");
        let navigation = matches!(
            event.keystroke.key.as_str(),
            "up" | "down" | "enter" | "escape"
        );
        if !self.tab_launcher.visible {
            return self.tab_launcher.used && (digit || navigation);
        }
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                self.repair_tab_launcher_selection();
                return self.context_menu_key(event, window, cx);
            }
            "escape" => {
                self.tab_launcher.used = true;
                self.dismiss_tab_launcher(window, cx);
                return true;
            }
            "enter" => {
                if !event.is_held {
                    self.tab_launcher.used = true;
                    self.tab_launcher.blur = None;
                    self.tab_launcher.focus = None;
                    self.repair_tab_launcher_selection();
                    self.context_menu_key(event, window, cx);
                    self.tab_launcher.visible = false;
                }
                return true;
            }
            _ => {}
        }
        if !digit {
            self.tab_launcher.navigation.set(false);
        }
        let index = self.tab_launcher.digit(&event.keystroke.key, event.is_held);
        self.dismiss_tab_launcher(window, cx);
        let Some(index) = index else {
            return digit;
        };
        let tab = self.settings.tab_launcher_order()[index];
        if let Some(project) = self.active_project {
            let kind = crate::start::harness_of(tab);
            if kind.is_none_or(|kind| self.available.shown().contains(&kind)) {
                let runs = kind.map_or(guild_session::Runs::Shell, |kind| self.runs_for_pick(kind));
                self.open_harness_tab(project, runs, window, cx);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chord_release_rearms_digit_alternatives() {
        let mut launcher = Launcher::default();
        assert!(launcher.modifiers(true));
        assert!(!launcher.modifiers(true));
        assert!(!launcher.modifiers(false));
        assert_eq!(launcher.digit("2", false), None);
        assert!(launcher.modifiers(true));
        assert_eq!(launcher.digit("2", false), Some(1));
        assert_eq!(launcher.digit("3", false), None);
        launcher.modifiers(false);
        launcher.modifiers(true);
        assert_eq!(launcher.digit("4", false), Some(3));
    }
    #[test]
    fn repeat_and_other_shortcuts_never_launch() {
        let mut launcher = Launcher::default();
        launcher.modifiers(true);
        assert_eq!(launcher.digit("2", true), None);
        assert_eq!(launcher.digit("2", false), None);
        launcher.modifiers(false);
        launcher.modifiers(true);
        assert_eq!(launcher.digit("p", false), None);
        assert_eq!(launcher.digit("1", false), None);
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext, px};

    #[test]
    fn isolated_launcher_routing() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::tab_launcher::routing_tests::editor_routing",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_LAUNCHER_TEST", "1")
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "run through isolated_launcher_routing"]
    fn editor_routing(cx: &mut TestAppContext) {
        assert_eq!(std::env::var("GUILD_LAUNCHER_TEST").as_deref(), Ok("1"));
        cx.executor().allow_parking();
        let fixture = tempfile::tempdir().unwrap();
        let project_path = fixture.path().join("project");
        std::fs::create_dir(&project_path).unwrap();
        std::fs::write(project_path.join("sample.rs"), "fn main() {}\n").unwrap();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let root = cx.new(|cx| {
                let mut desk = guild_session::Desk::default();
                let project = desk.open_in(desk.active_workspace(), &project_path);
                desk.look_at(Some(project), None);
                let mut root = Root::diagnostic(desk, fixture.path().join("state"), cx);
                root.active_project = Some(project);
                root.isolated_render = true;
                root.available.set(Vec::new()); // disabled harness digits must never launch.
                root.open_file(std::path::Path::new("sample.rs"), window, cx);
                root
            });
            made = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        let root = made.unwrap();
        cx.simulate_resize(gpui::size(px(1200.), px(850.)));
        cx.update(|window, cx| {
            window.activate_window();
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        let focus = cx.update(|window, cx| window.focused(cx));
        let chord = gpui::Modifiers {
            platform: true,
            control: true,
            ..Default::default()
        };
        // The rejected Cmd+Shift chord must leave system shortcuts alone.
        cx.simulate_modifiers_change(gpui::Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        });
        assert!(!root.read_with(cx, |root, _| root.tab_launcher.visible));
        cx.simulate_modifiers_change(Default::default());
        cx.simulate_modifiers_change(chord);
        assert!(root.read_with(cx, |root, _| root.tab_launcher.visible));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let number = cx
            .debug_bounds("popup-number-1")
            .expect("launcher leading number");
        let icon = cx
            .debug_bounds("popup-icon-Claude Code")
            .expect("existing provider icon");
        let label = cx.debug_bounds("popup-label-Claude Code").unwrap();
        assert!(number.right() <= icon.left() && icon.right() <= label.left());
        for selector in ["popup-number-2", "popup-number-3", "popup-number-4"] {
            let other = cx.debug_bounds(selector).unwrap();
            assert_eq!(other.left(), number.left());
            assert_eq!(other.size.width, number.size.width);
        }
        assert!(cx.debug_bounds("popup-shortcut-1").is_none());
        assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
        cx.simulate_keystrokes("ctrl-cmd-escape");
        cx.simulate_modifiers_change(Default::default());
        assert!(root.read_with(cx, |root, _| root.context_menu.is_none()));
        assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
        let tabs = root.read_with(cx, |root, _| root.tabs.len());
        cx.simulate_modifiers_change(chord);
        cx.simulate_keystrokes("ctrl-cmd-2 ctrl-cmd-2");
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs);
        assert!(root.read_with(cx, |root, _| root.context_menu.is_none()));
        cx.simulate_modifiers_change(Default::default());
        cx.simulate_modifiers_change(chord);
        cx.simulate_modifiers_change(gpui::Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        });
        cx.simulate_keystrokes("cmd-shift-f");
        assert!(root.read_with(cx, |root, _| root.picker.is_some()));
        assert!(!root.read_with(cx, |root, _| root.tab_launcher.visible));
        cx.simulate_keystrokes("escape");
        cx.simulate_modifiers_change(Default::default());
        // Native flagsChanged arrives once per released modifier. Neither edge
        // confirms the highlighted row, even after navigating to another row.
        let release_control = gpui::Modifiers {
            platform: true,
            ..Default::default()
        };
        let before = root.read_with(cx, |root, _| root.tabs.len());
        let sessions = root.read_with(cx, |root, _| root.desk.sessions().count());
        root.update(cx, |root, _| {
            use guild_session::DefaultTab::*;
            root.settings.tab_launcher_order = vec![Terminal, Claude, OpenCode, Codex];
        });
        let focus = cx.update(|window, cx| window.focused(cx));
        cx.simulate_modifiers_change(chord);
        cx.simulate_modifiers_change(release_control);
        assert_eq!(
            root.read_with(cx, |root, _| root.tabs.len()),
            before,
            "release must not launch the first enabled row"
        );
        assert!(!root.read_with(cx, |root, _| root.tab_launcher.visible));
        assert!(root.read_with(cx, |root, _| root.context_menu.is_none()));
        assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), before);
        assert_eq!(
            root.read_with(cx, |root, _| root.desk.sessions().count()),
            sessions
        );
        root.update(cx, |root, _| {
            root.settings.tab_launcher_order.clear();
            root.available.set(vec![guild_session::HarnessKind::Claude]);
        });
        cx.simulate_modifiers_change(chord);
        assert_eq!(
            root.read_with(cx, |root, _| root.context_menu.as_ref().unwrap().lit),
            Some(0)
        );
        cx.simulate_keystrokes("ctrl-cmd-down");
        assert_eq!(
            root.read_with(cx, |root, _| root.context_menu.as_ref().unwrap().lit),
            Some(3)
        );
        cx.simulate_modifiers_change(gpui::Modifiers {
            control: true,
            ..Default::default()
        });
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(
            root.read_with(cx, |root, _| root.tabs.len()),
            before,
            "release must not launch the row selected with Down"
        );
        assert!(root.read_with(cx, |root, _| root.context_menu.is_none()));
        assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
        assert_eq!(
            root.read_with(cx, |root, _| root.desk.sessions().count()),
            sessions
        );
        let tabs = before;
        for cancel in 0..4 {
            cx.simulate_modifiers_change(chord);
            cx.update(|window, cx| {
                root.update(cx, |root, cx| match cancel {
                    0 => root.cancel_tab_launcher(window, cx),
                    1 => root.close_context_menu(window, cx),
                    2 => root.focus.focus(window, cx),
                    _ => root.dismiss_tab_launcher(window, cx),
                })
            });
            cx.simulate_modifiers_change(chord);
            cx.simulate_modifiers_change(release_control);
            cx.simulate_modifiers_change(Default::default());
            assert_eq!(
                root.read_with(cx, |root, _| root.tabs.len()),
                tabs,
                "cancel path {cancel}"
            );
        }
        cx.simulate_modifiers_change(chord);
        cx.simulate_modifiers_change(gpui::Modifiers {
            shift: true,
            ..chord
        });
        cx.simulate_modifiers_change(chord);
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(
            root.read_with(cx, |root, _| root.tabs.len()),
            tabs,
            "shortcut takeover cannot rearm on modifier return"
        );
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.open_file(std::path::Path::new("sample.rs"), window, cx)
            })
        });
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let focus = cx.update(|window, cx| window.focused(cx));
        // Configured order and disabled rows use the existing menu stepping rule.
        root.update(cx, |root, _| {
            use guild_session::{DefaultTab::*, HarnessKind};
            root.settings.tab_launcher_order = vec![Codex, Terminal, OpenCode, Claude];
            root.available.set(vec![HarnessKind::Claude]);
        });
        cx.simulate_modifiers_change(chord);
        assert_eq!(
            root.read_with(cx, |root, _| root.context_menu.as_ref().unwrap().lit),
            Some(1)
        );
        cx.simulate_keystrokes("ctrl-cmd-down");
        assert_eq!(
            root.read_with(cx, |root, _| root.context_menu.as_ref().unwrap().lit),
            Some(3)
        );
        cx.simulate_keystrokes("ctrl-cmd-up");
        assert_eq!(
            root.read_with(cx, |root, _| root.context_menu.as_ref().unwrap().lit),
            Some(1)
        );
        cx.simulate_keystrokes("ctrl-cmd-escape");
        assert!(!root.read_with(cx, |root, _| root.tab_launcher.visible));
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs);
        assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
        cx.simulate_modifiers_change(Default::default());
        cx.simulate_modifiers_change(chord);
        cx.simulate_keystrokes("ctrl-cmd-enter");
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        assert_eq!(
            root.read_with(cx, |root, _| root.desk.sessions().count()),
            sessions + 1
        );
        assert!(
            root.read_with(cx, |root, _| {
                let session = root.active_body().unwrap().session().unwrap();
                root.desk.session(session).unwrap().runs == guild_session::Runs::Shell
            }),
            "Enter must open the highlighted Terminal, not the first available harness"
        );
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-cmd-enter").unwrap(),
            is_held: true,
            prefer_character_input: false,
        });
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        cx.simulate_modifiers_change(release_control);
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        root.update(cx, |root, _| {
            root.settings.tab_launcher_order.clear();
            root.available.set(Vec::new());
        });
        let tabs = root.read_with(cx, |root, _| root.tabs.len());
        // The shell is the real existing launch path, in the disposable project.
        cx.simulate_modifiers_change(chord);
        cx.simulate_keystrokes("ctrl-cmd-4");
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-cmd-4").unwrap(),
            is_held: true,
            prefer_character_input: false,
        });
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        cx.simulate_modifiers_change(release_control);
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let terminal_focus = cx.update(|window, cx| window.focused(cx));
        let expected = root.read_with(cx, |root, cx| root.active_body().unwrap().focus_handle(cx));
        assert_eq!(terminal_focus, expected);
        cx.simulate_modifiers_change(chord);
        assert!(root.read_with(cx, |root, _| root.tab_launcher.visible));
        assert_eq!(cx.update(|window, cx| window.focused(cx)), terminal_focus);
        cx.simulate_keystrokes("ctrl-cmd-escape");
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(cx.update(|window, cx| window.focused(cx)), terminal_focus);

        // Mouse selection remains explicit confirmation while the chord is held.
        let tabs = root.read_with(cx, |root, _| root.tabs.len());
        cx.simulate_modifiers_change(chord);
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let terminal = cx.debug_bounds("popup-label-terminal").unwrap().center();
        cx.simulate_click(terminal, chord);
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);
        assert!(root.read_with(cx, |root, _| root.context_menu.is_none()));
        cx.simulate_modifiers_change(release_control);
        cx.simulate_modifiers_change(Default::default());
        assert_eq!(root.read_with(cx, |root, _| root.tabs.len()), tabs + 1);

        root.update(cx, |root, cx| {
            root.open_settings_at(crate::view::settings::Section::Sessions, cx)
        });
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let down = cx
            .debug_bounds("tab-order-0-true")
            .expect("Sessions draws reorder controls")
            .center();
        cx.simulate_click(down, Default::default());
        let order = root.read_with(cx, |root, _| root.settings.tab_launcher_order());
        assert_eq!(order[0], guild_session::DefaultTab::OpenCode);
        let saved = guild_session::Store::<guild_session::Settings>::new(
            fixture.path().join("state/settings.json"),
        )
        .load()
        .unwrap();
        assert_eq!(saved.tab_launcher_order(), order);
        cx.update(|_, cx| root.update(cx, |root, cx| root.stop_every_process(cx)));
    }
}
