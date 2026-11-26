//! Changing a setting, and what the window does about it.

use gpui::Context;

use crate::Body;
use crate::Root;
use crate::TabId;
use crate::actions;
use crate::i18n::Text;
use crate::view::settings::Section;
use crate::view::settings::SettingsHost;
use guild_session::MAX_UI_FONT_SIZE;
use guild_session::MIN_UI_FONT_SIZE;
use guild_session::Settings;
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

impl Root {
    #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
    pub(crate) fn apply_terminal_refresh(
        &mut self,
        enabled: bool,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.adaptive_terminal_refresh = enabled;
        #[cfg(feature = "adaptive-terminal-refresh")]
        {
            for view in self.terminals() {
                view.update(cx, |tab, cx| tab.set_refresh_enabled(enabled, _window, cx));
            }
            self.sync_terminal_visibility(_window, cx);
        }
        self.store_settings();
        cx.notify();
    }

    /// Open the settings.
    ///
    /// One settings tab per project, reused rather than stacked: a second
    /// one would show the same thing and give two places to change one
    /// value. Opening it again brings the existing tab forward, which is
    /// what every environment does with a page there can only be one of.
    /// A settings tab brought back after a restart: pushed where it
    /// was, not brought to the front.
    pub(crate) fn restore_settings_tab(
        &mut self,
        project: guild_session::ProjectId,
        cx: &gpui::App,
    ) -> TabId {
        self.adopt(
            self.seat_for_project(project),
            project,
            Body::Settings {
                section: Section::General,
            },
            cx,
        )
    }

    /// With no project in front the window draws the page itself —
    /// decision 15 of M5.11. Until then the gear, the menu item and
    /// `cmd-,` did nothing in an empty window, which is exactly where a
    /// person first looks for the language or the theme.
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Root>) {
        self.ensure_open_in_inventory(cx);
        self.note_place(cx);
        self.history.suspended += 1;
        self.open_settings_without_history(cx);
        self.history.suspended -= 1;
    }

    fn open_settings_without_history(&mut self, cx: &mut Context<Root>) {
        let Some(project) = self.active_project else {
            self.settings_page.get_or_insert(Section::General);
            cx.notify();
            return;
        };

        // In every area and every pane of the seat (decision 15).
        let seat = self.seat_for_project(project);
        let existing = self.find_showing(seat, |body| matches!(body, Body::Settings { .. }));

        self.active_tab = Some(match existing {
            Some((tab, pane)) => {
                self.focus_pane(tab, pane, cx);
                // The keys leave the pane beside it (12.2).
                self.focusing_front = true;
                tab
            }
            None => self.adopt(
                self.seat_for_project(project),
                project,
                Body::Settings {
                    section: Section::General,
                },
                cx,
            ),
        });

        self.save();
        cx.notify();
    }

    /// Open the settings at `section` — the start screen's *Settings*,
    /// which opens at *Sessions*, where the default tab and mode it
    /// offers are chosen. M14.5.
    ///
    /// `open_settings` and then `choose_section`, on whichever page it
    /// brought up: the project's tab, or the window's own page with no
    /// project in front.
    pub(crate) fn open_settings_at(&mut self, section: Section, cx: &mut Context<Root>) {
        self.open_settings(cx);
        match self.active_body() {
            Some(Body::Settings { .. }) => {
                if let Some(tab) = self.active_tab {
                    self.choose_section(SettingsHost::Tab(tab), section, cx);
                }
            }
            Some(_) => {}
            None => self.settings_page = Some(section),
        }
    }

    /// Move a settings page to another section.
    ///
    /// The column goes back to its top, so a section opens at its first
    /// row rather than wherever the last one was left.
    pub(crate) fn choose_section(
        &mut self,
        host: SettingsHost,
        section: Section,
        cx: &mut Context<Root>,
    ) {
        self.network_check.invalidate();
        let current = match host {
            SettingsHost::Tab(tab) => match self
                .tabs
                .iter_mut()
                .find(|open| open.id == tab)
                .and_then(|open| open.panes.body_mut())
            {
                Some(Body::Settings { section: current }) => current,
                _ => return,
            },
            SettingsHost::Window => match self.settings_page.as_mut() {
                Some(current) => current,
                None => return,
            },
        };
        *current = section;
        self.scoped_settings
            .targets
            .insert(host, super::repository_settings::Target::Global(section));
        self.stop_recording();
        self.put_fields_away();
        self.settings_scroll.set_offset(gpui::Point::default());
        cx.notify();
    }

    /// Put a settings page away: its tab is closed, or the window's own page
    /// gives the body back to the hints.
    pub(crate) fn close_settings(&mut self, host: SettingsHost, cx: &mut Context<Root>) {
        self.network_check.invalidate();
        self.scoped_settings.targets.remove(&host);
        let prefix = format!("{host:?}:");
        self.scoped_settings
            .drafts
            .retain(|key, _| !key.starts_with(&prefix));
        match host {
            // The pane showing Settings, and its tab only when that pane
            // is all of it — M12.8: what is beside it stays.
            SettingsHost::Tab(tab) => {
                let pane = self
                    .tabs
                    .iter()
                    .find(|open| open.id == tab)
                    .and_then(|open| {
                        open.panes
                            .find(|body| matches!(body, crate::Body::Settings { .. }))
                    });
                match pane {
                    Some(pane) => self.close_pane(tab, pane, cx),
                    None => self.close_tab(tab, cx),
                }
            }
            SettingsHost::Window => {
                self.settings_page = None;
                cx.notify();
            }
        }
    }

    /// The window's own page goes when a project comes to the front, where
    /// Settings is a tab again. Called by every render; nothing on the page
    /// is lost, because every change on it is saved as it is made.
    pub(crate) fn settle_settings_page(&mut self) {
        if self.active_project.is_some() {
            self.settings_page = None;
        }
    }

    /// The Settings column goes back to its top when the page in front is
    /// not the page it was last drawn for. Called by every render.
    pub(crate) fn settle_settings_scroll(&mut self) {
        let now = match self.active_body() {
            Some(Body::Settings { .. }) => self.active_tab.map(SettingsHost::Tab),
            Some(_) => None,
            None => self.settings_page.map(|_| SettingsHost::Window),
        };
        if now != self.settings_scroll_host {
            self.open_in_picker = None;
            self.open_in_new_custom = None;
        }
        if settings_scroll_resets(self.settings_scroll_host, now) {
            self.settings_query.clear();
            self.settings_modified = false;
            self.settings_scroll.set_offset(gpui::Point::default());
        }
        self.settings_scroll_host = now;
    }

    /// The section of the Settings page in front, whether a tab or the
    /// window draws it.
    pub(crate) fn settings_in_front(&self) -> Option<Section> {
        match self.active_body() {
            Some(Body::Settings { section }) => Some(*section),
            Some(_) => None,
            None => self.settings_page,
        }
    }

    /// Change the language.
    ///
    /// The menu is rebuilt right away: it is drawn by the system, which
    /// will not learn about our choice on its own — it would stay half in
    /// the previous language.
    /// Move to the next palette.
    ///
    /// Every open tab is told, because a terminal left on the old colours
    /// while the interface moved is the most visible way a theme can be
    /// half applied — and the terminal is most of the window.
    /// Draw the interface larger or smaller.
    ///
    /// Half-point steps, because the number drives a scale rather than one
    /// font: 13 to 13.5 is a real change to every distance in the window,
    /// and a whole point is a coarser notch than a person chasing a
    /// comfortable size wants.
    pub(crate) fn change_ui_font_size(&mut self, by: f32, cx: &mut Context<Root>) {
        let size = (self.settings.ui_font_size + by).clamp(MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE);
        if (size - self.settings.ui_font_size).abs() < f32::EPSILON {
            return;
        }
        self.settings.ui_font_size = size;
        crate::editor::apply_theme(&self.theme, size, cx);
        cx.refresh_windows();
        self.store_settings();
        cx.notify();
    }

    /// Draw the interface in this family, or in the window's own choice
    /// when there is none. M5.9: chosen from a list, not cycled.
    pub(crate) fn set_ui_font(&mut self, family: Option<String>, cx: &mut Context<Root>) {
        self.settings.ui_font = family;
        crate::editor::apply_ui_font(self.settings.ui_font.as_deref(), cx);
        cx.refresh_windows();
        self.store_settings();
        cx.notify();
    }

    /// Draw code in this family — the editor, a diff, Annotate. Its own
    /// setting since M12.11; the terminal's is [`Self::set_terminal_font`].
    pub(crate) fn set_code_font(&mut self, family: Option<String>, cx: &mut Context<Root>) {
        self.settings.code_font = family;
        // The editor's face is resolved from the setting at the next
        // render — `refresh_code_font` notices the change.
        self.restyle_file_tabs(cx);
        self.store_settings();
        cx.notify();
    }

    /// Draw every terminal in this family.
    pub(crate) fn set_terminal_font(&mut self, family: Option<String>, cx: &mut Context<Root>) {
        self.settings.terminal_font = family;
        let chosen = self.settings.terminal_font.clone();
        for view in self.terminals() {
            let chosen = chosen.clone();
            view.update(cx, |view, cx| view.set_font(chosen, cx));
        }
        self.store_settings();
        cx.notify();
    }

    /// Set a face setting, whichever it is — a button's click and a row
    /// of the list both end here.
    pub(crate) fn set_font(
        &mut self,
        which: crate::fonts::Which,
        family: Option<String>,
        cx: &mut Context<Root>,
    ) {
        match which {
            crate::fonts::Which::Ui => self.set_ui_font(family, cx),
            crate::fonts::Which::Code => self.set_code_font(family, cx),
            crate::fonts::Which::Terminal => self.set_terminal_font(family, cx),
        }
    }

    pub(crate) fn cycle_language(&mut self, cx: &mut Context<Root>) {
        self.settings.language = self.settings.language.next();
        // Every terminal, the Output panel's included: an open find bar
        // speaks the language it was told, and nothing else tells it.
        let ui = self.ui();
        for view in self.terminals() {
            view.update(cx, |view, cx| view.set_ui(ui, cx));
        }
        self.store_settings();
        self.rebuild_menu(cx);
        cx.notify();
    }

    /// Change the terminal font size.
    ///
    /// It changes for every tab at once and is saved: the size is a
    /// property of eyes and screens, not of a single tab.
    pub(crate) fn change_font_size(&mut self, delta: f32, cx: &mut Context<Root>) {
        let size = (self.settings.terminal_font_size + delta)
            .clamp(guild_session::MIN_FONT_SIZE, guild_session::MAX_FONT_SIZE);
        self.apply_font_size(size, cx);
    }

    pub(crate) fn apply_font_size(&mut self, size: f32, cx: &mut Context<Root>) {
        self.settings.terminal_font_size = size;
        self.store_settings();

        for view in self.terminals() {
            view.update(cx, |tab, cx| tab.set_font_size(size, cx));
        }
        self.restyle_file_tabs(cx);
        cx.notify();
    }

    /// Return to the original font size.
    pub(crate) fn reset_font_size(&mut self, cx: &mut Context<Root>) {
        self.apply_font_size(Settings::default().terminal_font_size, cx);
    }

    /// Write the settings.
    pub(crate) fn store_settings(&mut self) {
        if crate::network::snapshot() != self.settings.network {
            self.network_check.invalidate();
        }
        crate::network::publish(&self.settings.network);
        self.store_settings_to(&self.integrations.state_dir().join("settings.json"));
    }

    /// Write the keymap and put it into force at once.
    ///
    /// A shortcut that only takes effect after a restart is a shortcut
    /// people report as broken.
    pub(crate) fn store_keymap(
        &mut self,
        overrides: std::collections::BTreeMap<actions::keymap::Named, String>,
        cx: &mut Context<Root>,
    ) {
        if let Err(err) = actions::keymap::save(&overrides) {
            tracing::error!(%err, "the keymap was not saved");
            let text = self.ui().text(Text::NoticeKeymapNotSaved).to_owned();
            self.fail(text);
        }

        self.stop_recording();
        actions::rebind(cx);
        cx.notify();
    }

    /// Forget the recording, its conflict and its refusal together: three
    /// fields that describe one question.
    pub(crate) fn stop_recording(&mut self) {
        self.recording = None;
        self.conflict = None;
        self.stroke_refused = false;
    }

    /// End a recording whose question is no longer on the screen.
    ///
    /// Called by every render. A recording belongs to the Keymap page in
    /// front, while the window's own handle holds the focus and the window
    /// is active; until M5.11 it outlived all three, so a person who left
    /// Settings with a slot lit had every shortcut of the window standing
    /// down under `!recording`, and the next key that bubbled up was taken
    /// as a stroke. M5.11's decision 14.
    ///
    /// **Not while the sessions panel is up**, either: it is drawn over the
    /// page and holds the root's focus, and a key pressed there is not an
    /// answer to the slot underneath.
    pub(crate) fn settle_recording(&mut self, window: &gpui::Window, cx: &mut Context<Root>) {
        if self.recording.is_none() {
            self.stroke_refused = false;
            return;
        }
        if !recording_holds(
            self.settings_in_front().map(|section| {
                if !self.settings_query.trim().is_empty() {
                    Section::Keymap
                } else {
                    section
                }
            }),
            self.focus.is_focused(window),
            window.is_window_active(),
        ) {
            self.stop_recording();
            cx.notify();
        }
    }

    /// Whether the page in front draws the font list's buttons, so the
    /// list still has what it hangs from — M5.11's task 16's rule. Since
    /// M8.9 the focus half of it is the picker's (`picker_holds_focus`),
    /// and a list whose page is gone is closed by `picker_anchored`.
    pub(crate) fn font_list_drawn(&self) -> bool {
        font_list_hangs(self.settings_in_front().map(|section| {
            if !self.settings_query.trim().is_empty() {
                Section::Appearance
            } else {
                section
            }
        }))
    }

    /// Take the keystroke a person just pressed as a shortcut.
    ///
    /// Only while recording, and the window's own shortcuts are standing
    /// down meanwhile — otherwise pressing cmd-w to bind it would close the
    /// tab instead. **And only while the root's own handle is focused**: a
    /// key that bubbled up from a terminal or a panel was typed into that
    /// thing, and is never a stroke. M5.11's decision 14.
    pub(crate) fn record_stroke(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let Some(action) = self.recording else {
            return;
        };
        if !self.focus.is_focused(window) {
            return;
        }

        let stroke = &event.keystroke;
        // Escape gives up. Nothing in the keymap changed, so there is
        // nothing to bind again.
        if stroke.key == "escape" {
            self.stop_recording();
            cx.notify();
            return;
        }
        let text = match actions::recordable(stroke) {
            // Ignored rather than refused: it is what a hand rests on
            // while reaching for the rest of the combination.
            actions::Recordable::Modifier => return,
            // Refused in the slot, and the question stays open.
            actions::Recordable::NeedsModifier => {
                self.stroke_refused = true;
                self.conflict = None;
                cx.notify();
                return;
            }
            actions::Recordable::Stroke(text) => text,
        };
        self.stroke_refused = false;

        // A stroke GPUI cannot parse back would panic when it is bound.
        // It comes from GPUI itself, so this should not happen — and if it
        // ever does, refusing beats bringing the window down.
        if gpui::Keystroke::parse(&text).is_err() {
            tracing::warn!(stroke = %text, "the pressed combination cannot be bound");
            let notice = self.ui().text(Text::NoticeKeymapNotSaved).to_owned();
            self.fail(notice);
            self.stop_recording();
            cx.notify();
            return;
        }

        // One stroke, one action. Taking it from its current owner would
        // leave that row with nothing, and the file — a map of stroke to
        // action — has no way to say "bound to nothing". So the conflict
        // is refused and named, and the person chooses.
        if let Some(owner) = actions::effective()
            .into_iter()
            .find(|(bound, owner)| bound == &text && *owner != action)
            .map(|(_, owner)| owner)
        {
            self.conflict = Some(owner);
            cx.notify();
            return;
        }

        let mut overrides = actions::keymap::overrides();
        overrides.insert(action, text);
        self.store_keymap(overrides, cx);
    }

    /// Put an action back on its default.
    pub(crate) fn reset_stroke(&mut self, action: actions::keymap::Named, cx: &mut Context<Root>) {
        let mut overrides = actions::keymap::overrides();
        overrides.remove(&action);
        self.store_keymap(overrides, cx);
    }
}

impl Root {
    /// The handlers for the window itself: its settings, its font size, its full screen.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_window(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &About, window, cx| root.about(window, cx)))
            .on_action(
                cx.listener(|root, _: &crate::actions::UpdateGuild, window, cx| {
                    root.apply_update(window, cx)
                }),
            )
            .on_action(cx.listener(|root, _: &OpenSettings, _window, cx| root.open_settings(cx)))
            .on_action(cx.listener(|_root, _: &ToggleFullScreen, window, _cx| {
                // The system's full screen, which is what ctrl-cmd-f means
                // everywhere: the window takes a
                // space of its own and covers the menu bar. macOS hides
                // the traffic lights there and slides them back at the top
                // edge — that is the platform's behaviour, not ours to
                // keep. Filling the screen while staying a window is what
                // the green button does.
                window.toggle_fullscreen();
            }))
            .on_action(
                cx.listener(|root, _: &IncreaseFontSize, _window, cx| {
                    root.change_font_size(0.5, cx)
                }),
            )
            .on_action(cx.listener(|root, _: &DecreaseFontSize, _window, cx| {
                root.change_font_size(-0.5, cx)
            }))
            .on_action(cx.listener(|root, _: &ResetFontSize, _window, cx| root.reset_font_size(cx)))
    }
}

/// Whether a recording's question is still being asked: the Keymap page
/// in front — a tab's or the window's — the root's own handle focused, the
/// window active.
fn recording_holds(in_front: Option<Section>, root_focused: bool, window_active: bool) -> bool {
    in_front == Some(Section::Keymap) && root_focused && window_active
}

/// Whether the Settings column goes back to its top because the page in
/// front is another page than the one last drawn.
fn settings_scroll_resets(last: Option<SettingsHost>, now: Option<SettingsHost>) -> bool {
    now.is_some() && now != last
}

/// Whether the font list still has the buttons it hangs from: the
/// Appearance or Terminal section in front.
///
/// Until M8.9 this also asked whether the list held the focus; that half
/// is `picker_holds_focus` now, and a list asked about here is one the
/// picker has open — so a list whose page is gone is closed by
/// `picker_anchored` rather than left holding a focus nothing draws.
fn font_list_hangs(in_front: Option<Section>) -> bool {
    matches!(in_front, Some(Section::Appearance | Section::Terminal))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A recording belongs to the Keymap page in front, while the
    /// window's own handle holds the focus and the window is active.** Any
    /// one of the three gone and the recording ends: until M5.11 it
    /// outlived the page, and the next letter typed into a terminal was
    /// taken as a shortcut — or, with every binding standing down under
    /// `!recording`, no shortcut in the window worked at all.
    /// **The font list hangs only where the page that draws its buttons
    /// is in front.** A list left open on a page that no longer draws them
    /// — another section — would stand at the
    /// last place its button was, holding a focus nobody draws, and every
    /// shortcut would go nowhere; `picker_anchored` closes it instead.
    /// M5.11's task 16 met the first half of this as a list that lost
    /// every key to the root the frame after it opened; that half is
    /// `picker_holds_focus` since M8.9, and this test asks only what is
    /// left.
    #[test]
    fn the_font_list_hangs_only_where_it_is_drawn() {
        for in_front in [None, Some(Section::General), Some(Section::Keymap)] {
            assert!(!font_list_hangs(in_front), "{in_front:?}");
        }
        assert!(font_list_hangs(Some(Section::Terminal)));
        assert!(font_list_hangs(Some(Section::Appearance)));
    }

    /// **The Settings column is one scroll handle, and a page it did not
    /// scroll opens at its top.** Every Settings tab and the window's own
    /// page share the handle; before this, a second project's Settings tab
    /// — or the window's page after a tab — opened wherever the last one
    /// had been left. The same page drawn again keeps its place, or the
    /// column could never be scrolled at all.
    #[test]
    fn a_settings_page_brought_to_the_front_opens_at_its_top() {
        let one = SettingsHost::Tab(TabId::new());
        let two = SettingsHost::Tab(TabId::new());
        assert!(!settings_scroll_resets(Some(one), Some(one)));
        assert!(!settings_scroll_resets(
            Some(SettingsHost::Window),
            Some(SettingsHost::Window)
        ));
        assert!(settings_scroll_resets(Some(one), Some(two)));
        assert!(settings_scroll_resets(
            Some(one),
            Some(SettingsHost::Window)
        ));
        assert!(settings_scroll_resets(None, Some(one)));
        assert!(!settings_scroll_resets(Some(one), None));
    }

    #[test]
    fn a_recording_ends_when_its_page_or_the_focus_goes() {
        let pages = [
            None,
            Some(Section::General),
            Some(Section::Appearance),
            Some(Section::Keymap),
        ];
        for in_front in pages {
            for root_focused in [false, true] {
                for window_active in [false, true] {
                    let asking = in_front == Some(Section::Keymap) && root_focused && window_active;
                    assert_eq!(
                        recording_holds(in_front, root_focused, window_active),
                        asking,
                        "{in_front:?}, focus {root_focused}, active {window_active}"
                    );
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "macos", feature = "adaptive-terminal-refresh"))]
mod adaptive_setting_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext, px};
    #[test]
    fn isolated_adaptive_settings_toggle() {
        let state = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::settings::adaptive_setting_tests::real_toggle",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GIT_CEILING_DIRECTORIES", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_ADAPTIVE_SETTINGS_TEST", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[gpui::test]
    #[ignore = "run through isolated_adaptive_settings_toggle"]
    fn real_toggle_updates_existing_hidden_terminal_and_persists(cx: &mut TestAppContext) {
        assert_eq!(
            std::env::var("GUILD_ADAPTIVE_SETTINGS_TEST").as_deref(),
            Ok("1")
        );
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let mut root = None;
        let mut terminal = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let r = cx.new(|cx| {
                let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
                root.isolated_render = true;
                root.settings_page = Some(Section::Terminal);
                let project = root.desk.open_in(root.desk.active_workspace(), dir.path());
                let mut tabs = crate::terminal_panel::TerminalTabs::default();
                let mut cmd = portable_pty::CommandBuilder::new("/bin/sleep");
                cmd.arg("30");
                let tab = crate::terminal::view::open_job(
                    cmd,
                    dir.path(),
                    crate::terminal::view::Look::of(&root.settings, root.theme),
                    cx,
                )
                .unwrap();
                terminal = Some(tab.clone());
                tabs.add(tab);
                root.panel_terminals.insert(project, tabs);
                root
            });
            root = Some(r.clone());
            gpui_component::Root::new(r, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1100.), px(900.)));
        cx.run_until_parked();
        let root = root.unwrap();
        let terminal = terminal.unwrap();
        assert_eq!(
            terminal.read_with(cx, |tab, _| tab.refresh_ownership()),
            (false, false, 0, false)
        );
        let bounds = cx
            .debug_bounds("adaptive-refresh-toggle")
            .expect("real Terminal settings toggle");
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        assert!(root.read_with(cx, |root, _| root.settings.adaptive_terminal_refresh));
        guild_session::store::flush();
        let saved_on = guild_session::Store::<Settings>::new(dir.path().join("settings.json"))
            .load()
            .unwrap();
        assert!(saved_on.adaptive_terminal_refresh);
        cx.update(|_, cx| {
            let mut cmd = portable_pty::CommandBuilder::new("/bin/sleep");
            cmd.arg("30");
            let early = crate::terminal::view::open_job(
                cmd,
                dir.path(),
                crate::terminal::view::Look::of(&saved_on, Default::default()),
                cx,
            )
            .unwrap();
            assert_eq!(
                early.read(cx).refresh_ownership(),
                (true, false, 0, false),
                "saved value reaches construction before the first host paint"
            );
            early.update(cx, |tab, _| tab.cancel_job());
        });
        let enabled = terminal.read_with(cx, |tab, _| tab.refresh_ownership());
        assert!(enabled.0 && enabled.1 && enabled.2 > 0 && enabled.3);
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            terminal.read_with(cx, |tab, _| tab.refresh_ownership()),
            (false, false, 0, false)
        );
        assert!(!root.read_with(cx, |root, _| root.settings.adaptive_terminal_refresh));
        guild_session::store::flush();
        let saved = guild_session::Store::<Settings>::new(dir.path().join("settings.json"))
            .load()
            .unwrap();
        assert!(!saved.adaptive_terminal_refresh);
        cx.update(|_, cx| terminal.update(cx, |tab, _| tab.cancel_job()));
    }
}
