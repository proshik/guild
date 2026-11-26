//! The theme customizer's window half — `openspec/specs/theme-customizer`.
//!
//! [`crate::customizer::Draft`] decides what a choice means; this owns the
//! card's live parts — the three sliders, the hex text being typed, the
//! eyedropper's wait — and hands every change to the one apply path,
//! which previews the draft while the card is open.
//!
//! **Every drag step reaches every surface.** The plan allowed leaving
//! file, diff and Annotate tabs for a slider's release if restyling them
//! per step proved slow; measured in task 9.2 at about 90 µs an apply with
//! twenty file tabs in a release build (under 0.5 ms in debug), it did not,
//! so every step flushes. The apply path still tracks what the tabs last
//! received, so a release carrying the dragged value is never skipped
//! (spec review r1, finding 2).

use gpui::{AppContext as _, Context, Entity, FocusHandle, Subscription, Task, Window};
use gpui_component::slider::{SliderEvent, SliderState};

use crate::customizer::{Draft, Tab};
use crate::{Root, theme};

/// Which of the card's sliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slider {
    Hue,
    Saturation,
    Intensity,
}

/// The open customizer.
pub(crate) struct Customizer {
    pub(crate) draft: Draft,
    /// What the hex field holds while a person is typing in it; `None`
    /// shows the edited tab's colour.
    pub(crate) hex: Option<String>,
    /// The typed hex was confirmed and is not a colour.
    pub(crate) invalid: bool,
    pub(crate) hue: Entity<SliderState>,
    pub(crate) saturation: Entity<SliderState>,
    pub(crate) intensity: Entity<SliderState>,
    /// The slider under the pointer, between its press and its release:
    /// it is not rebuilt from the draft while a person drags it.
    dragging: Option<Slider>,
    subscriptions: Vec<Subscription>,
    /// The eyedropper's answer, awaited.
    sampling: Option<Task<()>>,
    /// The card's own focus, taken when it opens, so that Escape is the
    /// card's from the first stroke (code review r1, bugs+impl-2).
    pub(crate) focus: FocusHandle,
    /// The hex field holds the keys: its Escape restores the field rather
    /// than closing the card.
    pub(crate) hex_focused: bool,
}

impl Customizer {
    /// The values the sliders show for the draft as it is.
    fn wanted(&self) -> [(Slider, f32); 3] {
        let (hue, saturation) = self.draft.hue_saturation().unwrap_or((0.0, 0.0));
        let intensity = self.draft.current().map_or(0.0, |t| f32::from(t.intensity));
        [
            (Slider::Hue, hue),
            (Slider::Saturation, saturation),
            (Slider::Intensity, intensity),
        ]
    }

    fn state(&self, which: Slider) -> &Entity<SliderState> {
        match which {
            Slider::Hue => &self.hue,
            Slider::Saturation => &self.saturation,
            Slider::Intensity => &self.intensity,
        }
    }
}

fn slider_state(which: Slider, value: f32, cx: &mut Context<Root>) -> Entity<SliderState> {
    let max = if which == Slider::Hue { 359.0 } else { 100.0 };
    cx.new(|_| {
        SliderState::new()
            .min(0.0)
            .max(max)
            .step(1.0)
            .default_value(value.clamp(0.0, max))
    })
}

impl Root {
    /// Whether the customizer card is open.
    pub(crate) fn customizing(&self) -> bool {
        self.customizer.is_some()
    }

    /// *Customize theme*: open on the palette in effect with the focus in
    /// the card, or close and give the focus back to the button.
    pub(crate) fn toggle_customizer(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        if self.customizing() {
            self.dismiss_customizer(window, cx);
        } else {
            self.open_customizer(cx);
            if let Some(customizer) = &self.customizer {
                window.focus(&customizer.focus, cx);
            }
        }
    }

    /// ×, or Escape anywhere on the Settings page: close, and hand the
    /// focus to the button that opens the card — a focused element that
    /// goes away would leave the keys to the window's fallback.
    pub(crate) fn dismiss_customizer(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        self.close_customizer(cx);
        window.focus(&self.customize_button, cx);
    }

    /// The one rule for whether the card is on the screen: a Settings page
    /// in front that draws its section rows — not a scoped page — and, under
    /// a search or not, the *Customize theme* row among them. The card is
    /// drawn under that row, and its preview lives exactly as long
    /// (code review r1, bugs+impl-1 and arch+quality-1).
    pub(crate) fn customizer_drawn(&self) -> bool {
        // Popups and overlays are drawn over the page, not instead of it.
        let Some(section) = self.settings_in_front() else {
            return false;
        };
        let host = match self.active_body() {
            Some(crate::Body::Settings { .. }) => self
                .active_tab
                .map(crate::view::settings::SettingsHost::Tab),
            Some(_) => None,
            None => self
                .settings_page
                .map(|_| crate::view::settings::SettingsHost::Window),
        };
        let searching = !self.settings_query.trim().is_empty();
        let scoped = !searching
            && host
                .and_then(|host| self.scoped_settings_target(host))
                .is_some_and(|target| {
                    !matches!(target, crate::root::repository_settings::Target::Global(_))
                });
        !scoped
            && self
                .visible_settings(section)
                .contains(&crate::view::settings::Setting::CustomizeTheme)
    }

    pub(crate) fn open_customizer(&mut self, cx: &mut Context<Root>) {
        let kind = self.settings.theme_kind(self.system_dark);
        let draft = Draft::open(self.settings.theme_custom, kind);
        let mut customizer = Customizer {
            draft,
            hex: None,
            invalid: false,
            hue: slider_state(Slider::Hue, 0.0, cx),
            saturation: slider_state(Slider::Saturation, 0.0, cx),
            intensity: slider_state(Slider::Intensity, 0.0, cx),
            dragging: None,
            subscriptions: Vec::new(),
            sampling: None,
            focus: cx.focus_handle(),
            hex_focused: false,
        };
        Self::rebuild_sliders(&mut customizer, None, cx);
        self.customizer = Some(customizer);
        self.apply_theme_in_effect(true, cx);
        cx.notify();
    }

    /// Close the card: both palettes' unsaved choices go, and the window
    /// draws the saved theme in effect. Nothing is written.
    pub(crate) fn close_customizer(&mut self, cx: &mut Context<Root>) {
        if self.customizer.take().is_some() {
            self.apply_theme_in_effect(true, cx);
            cx.notify();
        }
    }

    /// Change the draft, then show it: the sliders follow, and the window.
    pub(crate) fn customize(&mut self, cx: &mut Context<Root>, change: impl FnOnce(&mut Draft)) {
        let Some(customizer) = self.customizer.as_mut() else {
            return;
        };
        change(&mut customizer.draft);
        customizer.hex = None;
        customizer.invalid = false;
        Self::rebuild_sliders(customizer, None, cx);
        self.apply_theme_in_effect(true, cx);
        cx.notify();
    }

    pub(crate) fn customizer_edit(&mut self, kind: theme::Kind, cx: &mut Context<Root>) {
        self.customize(cx, |draft| draft.edit(kind));
    }

    pub(crate) fn customizer_tab(&mut self, tab: Tab, cx: &mut Context<Root>) {
        self.customize(cx, |draft| draft.select_tab(tab));
    }

    pub(crate) fn customizer_color(&mut self, color: u32, cx: &mut Context<Root>) {
        self.customize(cx, |draft| draft.set_color(color));
    }

    /// *Save Light Theme* / *Save Dark Theme*.
    pub(crate) fn save_customizer(&mut self, cx: &mut Context<Root>) {
        let Some(customizer) = self.customizer.as_mut() else {
            return;
        };
        customizer.draft.save(&mut self.settings.theme_custom);
        self.store_settings();
        self.apply_theme_in_effect(true, cx);
        cx.notify();
    }

    /// The hex field's text changed. A full six-digit colour previews at
    /// once; anything shorter waits for Enter, so `#3b8` on its way to
    /// `#3b82f6` is not taken as `#33bb88` under the caret.
    pub(crate) fn customizer_hex_typed(&mut self, text: &str, cx: &mut Context<Root>) {
        let Some(customizer) = self.customizer.as_mut() else {
            return;
        };
        customizer.hex = Some(text.to_owned());
        customizer.invalid = false;
        let digits = text.trim().trim_start_matches('#');
        if digits.len() == 6
            && let Some(color) = crate::customizer::parse_hex(text)
        {
            customizer.draft.set_color(color);
            Self::rebuild_sliders(customizer, None, cx);
            self.apply_theme_in_effect(true, cx);
        }
        cx.notify();
    }

    /// Enter in the hex field: take it, or say it is not a colour.
    pub(crate) fn customizer_hex_confirmed(&mut self, cx: &mut Context<Root>) {
        let Some(customizer) = self.customizer.as_mut() else {
            return;
        };
        let Some(text) = customizer.hex.clone() else {
            return;
        };
        match crate::customizer::parse_hex(&text) {
            Some(color) => self.customizer_color(color, cx),
            None => {
                customizer.invalid = true;
                cx.notify();
            }
        }
    }

    /// Leaving the hex field: a valid colour, `#rgb` included, is taken;
    /// anything else goes back to the tab's colour (code review r1,
    /// bugs+impl-3 — D7 confirms on Enter *and* on focus loss).
    pub(crate) fn customizer_hex_left(&mut self, cx: &mut Context<Root>) {
        if let Some(customizer) = self.customizer.as_mut() {
            customizer.hex_focused = false;
        }
        let typed = self.customizer.as_ref().and_then(|c| c.hex.clone());
        match typed.as_deref().and_then(crate::customizer::parse_hex) {
            Some(color) => self.customizer_color(color, cx),
            None => self.customizer_hex_restored(cx),
        }
    }

    /// The hex field took the keys.
    pub(crate) fn customizer_hex_entered(&mut self) {
        if let Some(customizer) = self.customizer.as_mut() {
            customizer.hex_focused = true;
        }
    }

    /// Escape on the Settings page closes the card — unless the hex field
    /// has it, whose Escape restores the field. Read in the page's capture
    /// phase: a Guild field stops every Escape it handles, so the bubble
    /// phase never sees one from the search or any other field (code
    /// review r2, bugs+impl-2).
    pub(crate) fn settings_escape(&mut self, window: &mut Window, cx: &mut Context<Root>) -> bool {
        let hex = self.customizer.as_ref().is_some_and(|c| c.hex_focused);
        if hex || !self.customizing() || !self.customizer_drawn() {
            return false;
        }
        self.dismiss_customizer(window, cx);
        true
    }

    /// Escape in the hex field, or an invalid value left: back to the
    /// tab's colour.
    pub(crate) fn customizer_hex_restored(&mut self, cx: &mut Context<Root>) {
        if let Some(customizer) = self.customizer.as_mut() {
            customizer.hex = None;
            customizer.invalid = false;
            cx.notify();
        }
    }

    /// The eyedropper: the system's sampler, and its colour into the tab.
    pub(crate) fn customizer_sample(&mut self, cx: &mut Context<Root>) {
        let sampled = match crate::system::color_sampler::sample() {
            Ok(sampled) => sampled,
            Err(reason) => {
                self.warn(reason);
                cx.notify();
                return;
            }
        };
        let task = cx.spawn(async move |root, cx| {
            if let Ok(Some(color)) = sampled.await {
                let _ = root.update(cx, |root, cx| root.customizer_color(color, cx));
            }
        });
        if let Some(customizer) = self.customizer.as_mut() {
            customizer.sampling = Some(task);
        }
    }

    fn customizer_slider(&mut self, which: Slider, event: &SliderEvent, cx: &mut Context<Root>) {
        let Some(customizer) = self.customizer.as_mut() else {
            return;
        };
        let (value, released) = match event {
            SliderEvent::Change(value) => (value.start(), false),
            SliderEvent::Release(value) => (value.start(), true),
        };
        customizer.dragging = (!released).then_some(which);
        match which {
            Slider::Hue => customizer.draft.set_hue(value),
            Slider::Saturation => customizer.draft.set_saturation(value),
            Slider::Intensity => customizer.draft.set_intensity(value.round() as u8),
        }
        customizer.hex = None;
        customizer.invalid = false;
        let held = customizer.dragging;
        Self::rebuild_sliders(customizer, held, cx);
        self.apply_theme_in_effect(true, cx);
        cx.notify();
    }

    /// Give every slider but `held` the draft's value. A slider's state
    /// takes a value only through a window, so one that must move is made
    /// again at the new value — never the one under the pointer.
    fn rebuild_sliders(customizer: &mut Customizer, held: Option<Slider>, cx: &mut Context<Root>) {
        let mut moved = false;
        for (which, value) in customizer.wanted() {
            if Some(which) == held
                || customizer.subscriptions.len() == 3 && {
                    let now = customizer.state(which).read(cx).value().start();
                    (now - value).abs() < 0.5
                }
            {
                continue;
            }
            let state = slider_state(which, value, cx);
            match which {
                Slider::Hue => customizer.hue = state,
                Slider::Saturation => customizer.saturation = state,
                Slider::Intensity => customizer.intensity = state,
            }
            moved = true;
        }
        if moved || customizer.subscriptions.len() != 3 {
            customizer.subscriptions = [Slider::Hue, Slider::Saturation, Slider::Intensity]
                .into_iter()
                .map(|which| {
                    let state = customizer.state(which).clone();
                    cx.subscribe(&state, move |root, _, event: &SliderEvent, cx| {
                        root.customizer_slider(which, event, cx)
                    })
                })
                .collect();
        }
    }

    /// The card goes when it is not drawn ([`Self::customizer_drawn`]): a
    /// draft nobody can see would go on painting the window. Deferred —
    /// this is asked from a render.
    pub(crate) fn settle_customizer(&mut self, cx: &mut Context<Root>) {
        if self.customizing() && !self.customizer_drawn() {
            let root = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = root.update(cx, |root, cx| root.close_customizer(cx));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, Entity, TestAppContext};
    use gpui_component::slider::{SliderEvent, SliderValue};

    use super::Slider;
    use crate::Root;
    use crate::customizer::Tab;
    use crate::theme::{Kind, Mode, Theme};
    use crate::view::settings::Section;

    /// A diagnostic Root on Settings → Appearance, owning its settings
    /// writes in `dir/state`, with a project holding `a.txt`.
    fn root(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Root>,
        tempfile::TempDir,
        &mut gpui::VisualTestContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("a.txt"), "text\n").unwrap();
        let state = dir.path().join("state");
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|_, cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), &project_dir);
            let root = cx.new(|cx| Root::diagnostic(desk, state, cx));
            root.update(cx, |root, cx| {
                root.active_project = Some(project);
                root.settings_page = Some(Section::Appearance);
                root.set_theme_mode(Mode::Dark, cx);
            });
            owner = Some(root);
            gpui::Empty
        });
        (owner.unwrap(), dir, cx)
    }

    fn saved(dir: &tempfile::TempDir) -> serde_json::Value {
        guild_session::flush();
        let file = dir.path().join("state").join("settings.json");
        std::fs::read_to_string(file)
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap_or(serde_json::Value::Null)
    }

    #[gpui::test]
    fn a_preset_previews_and_closing_reverts_without_a_write(cx: &mut TestAppContext) {
        let (root, dir, cx) = root(cx);
        let before = saved(&dir)["theme_custom"].clone();
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.toggle_customizer(window, cx);
                assert!(root.customizing());
                assert_eq!(
                    root.customizer.as_ref().unwrap().draft.editing(),
                    Kind::Dark
                );
                root.customizer_tab(Tab::Accent, cx);
                root.customizer_color(0xf97316, cx);
                assert_eq!(root.theme.accent.patina, 0xf97316, "the window previews");
                root.toggle_customizer(window, cx);
                assert!(!root.customizing());
                assert_eq!(root.theme, Theme::dark(), "closing reverts");
            })
        });
        assert_eq!(saved(&dir)["theme_custom"], before, "nothing was written");
    }

    #[gpui::test]
    fn editing_the_other_palette_previews_it(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_edit(Kind::Light, cx);
            assert_eq!(root.theme, Theme::light());
            root.close_customizer(cx);
            assert_eq!(root.theme, Theme::dark());
        });
    }

    #[gpui::test]
    fn save_writes_the_edited_palette_only(cx: &mut TestAppContext) {
        let (root, dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_color(0x3b82f6, cx);
            root.customizer_edit(Kind::Light, cx);
            root.customizer_color(0x14b8a6, cx);
            root.save_customizer(cx);
        });
        let file = saved(&dir);
        assert_eq!(
            file["theme_custom"]["light"]["background"],
            serde_json::json!({"color": "#14b8a6", "intensity": 40})
        );
        assert!(
            file["theme_custom"]["dark"].get("background").is_none(),
            "{file}"
        );
        root.update(cx, |root, cx| {
            // Closing drops Dark's unsaved choice and keeps Light's saved one.
            root.close_customizer(cx);
            assert_eq!(root.theme, Theme::dark());
            assert_eq!(
                root.settings.theme_custom.light.background.unwrap().color,
                0x14b8a6
            );
        });
    }

    #[gpui::test]
    fn reset_and_save_leave_no_customization(cx: &mut TestAppContext) {
        let (root, dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_color(0x3b82f6, cx);
            root.save_customizer(cx);
            root.customize(cx, |draft| draft.reset());
            root.save_customizer(cx);
            assert_eq!(root.theme, Theme::dark(), "built-in Dark exactly");
        });
        assert_eq!(saved(&dir)["theme_custom"]["dark"], serde_json::json!({}));
    }

    #[gpui::test]
    fn copy_from_takes_the_other_saved_palette(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.settings.theme_custom.light.accent = Some(guild_session::Tint {
                color: 0x22c55e,
                intensity: 100,
            });
            root.open_customizer(cx);
            root.customize(cx, |draft| draft.copy_from_other());
            let dark = *root
                .customizer
                .as_ref()
                .unwrap()
                .draft
                .customization(Kind::Dark);
            assert_eq!(dark.accent.unwrap().color, 0x22c55e);
        });
    }

    #[gpui::test]
    fn the_hex_field_previews_a_full_colour_and_marks_an_invalid_one(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_tab(Tab::Accent, cx);
            root.customizer_hex_typed("#f97", cx);
            assert_eq!(root.theme, Theme::dark(), "a short form waits for Enter");
            root.customizer_hex_confirmed(cx);
            assert_eq!(root.theme.accent.patina, 0xff9977);
            root.customizer_hex_typed("#f97316", cx);
            assert_eq!(
                root.theme.accent.patina, 0xf97316,
                "six digits preview at once"
            );
            root.customizer_hex_typed("blue", cx);
            root.customizer_hex_confirmed(cx);
            assert!(root.customizer.as_ref().unwrap().invalid);
            assert_eq!(
                root.theme.accent.patina, 0xf97316,
                "an invalid value changes nothing"
            );
            root.customizer_hex_restored(cx);
            let customizer = root.customizer.as_ref().unwrap();
            assert!(!customizer.invalid && customizer.hex.is_none());
        });
    }

    fn emit(
        root: &Entity<Root>,
        which: Slider,
        event: SliderEvent,
        cx: &mut gpui::VisualTestContext,
    ) {
        let state = root.read_with(cx, |root, _| {
            let c = root.customizer.as_ref().unwrap();
            match which {
                Slider::Hue => c.hue.clone(),
                Slider::Saturation => c.saturation.clone(),
                Slider::Intensity => c.intensity.clone(),
            }
        });
        state.update(cx, |_, cx| cx.emit(event));
        cx.run_until_parked();
    }

    /// Spec "Slider release reaches every surface": every drag step and
    /// its release, carrying the same value, reach the tabs, and so does
    /// Save after a change.
    #[gpui::test]
    fn a_slider_drag_previews_and_its_release_reaches_the_tabs(cx: &mut TestAppContext) {
        use crate::root::appearance::tab_looks::{assert_all, open_all};
        let (root, _dir, cx) = root(cx);
        open_all(&root, "a.txt", cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_color(0x3b82f6, cx);
        });
        cx.run_until_parked();
        let at_40 = root.read_with(cx, |root, _| root.theme);
        assert_ne!(at_40, Theme::dark(), "a tint, same kind");
        assert_all(&root, at_40, "a preset", cx);
        emit(
            &root,
            Slider::Intensity,
            SliderEvent::Change(SliderValue::Single(90.0)),
            cx,
        );
        let at_90 = root.read_with(cx, |root, _| root.theme);
        assert_ne!(at_90, at_40, "the window previews the drag");
        assert_all(&root, at_90, "a drag step", cx);
        emit(
            &root,
            Slider::Intensity,
            SliderEvent::Release(SliderValue::Single(90.0)),
            cx,
        );
        assert_all(&root, at_90, "the release", cx);
        // Save after a change.
        emit(
            &root,
            Slider::Intensity,
            SliderEvent::Change(SliderValue::Single(20.0)),
            cx,
        );
        root.update(cx, |root, cx| root.save_customizer(cx));
        let at_20 = root.read_with(cx, |root, _| root.theme);
        assert_all(&root, at_20, "Save", cx);
        assert_eq!(
            root.read_with(cx, |root, _| root
                .settings
                .theme_custom
                .dark
                .background
                .unwrap()
                .intensity),
            20
        );
    }

    /// Code review r1 (bugs+impl-1, arch+quality-1): the card lives while
    /// it is drawn — under a search from any section too — and goes, with
    /// its preview, when it is not.
    #[gpui::test]
    fn the_card_lives_exactly_while_it_is_drawn(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_color(0x3b82f6, cx);
            // Found by search while General is the section: drawn.
            root.settings_page = Some(Section::General);
            root.settings_query = "customize".into();
            root.settle_customizer(cx);
        });
        cx.run_until_parked();
        assert!(
            root.read_with(cx, |root, _| root.customizing()),
            "drawn under a search"
        );
        root.update(cx, |root, cx| {
            // A search in Appearance that hides the row: not drawn.
            root.settings_page = Some(Section::Appearance);
            root.settings_query = "interface font".into();
            root.settle_customizer(cx);
        });
        cx.run_until_parked();
        root.read_with(cx, |root, _| {
            assert!(!root.customizing(), "hidden by a search");
            assert_eq!(root.theme, Theme::dark(), "its preview went with it");
        });
    }

    /// A Settings page on Appearance, drawn as the window draws it, owning
    /// its writes in `dir`. Run inside `isolated_test`.
    fn drawn(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Root>,
        tempfile::TempDir,
        &mut gpui::VisualTestContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let mut entity = None;
        let state = dir.path().to_path_buf();
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let root = cx.new(|cx| {
                let mut root = Root::diagnostic(Default::default(), state, cx);
                root.isolated_render = true;
                root.settings_page = Some(Section::Appearance);
                root.settings.theme_mode = Some(Mode::Dark);
                root
            });
            entity = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        let root = entity.unwrap();
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(1400.)));
        // An active window, as on the screen: GPUI tells blur listeners
        // nothing in an inactive one, and leaving a field is a blur.
        cx.update(|window, _| window.activate_window());
        root.update(cx, |root, cx| root.apply_theme_in_effect(true, cx));
        cx.run_until_parked();
        (root, dir, cx)
    }

    fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let at = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
            .center();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
    }

    /// The card as drawn, by clicks and single keys: the row's button opens
    /// it, a preset repaints the window, × closes it; Escape closes it right
    /// after opening and with the focus on a control outside the card (code
    /// review r1, bugs+impl-2); the Theme row is three buttons.
    #[gpui::test]
    fn the_drawn_card_answers_clicks(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::the_drawn_card_answers_clicks"
        )) {
            return;
        }
        let (root, _dir, cx) = drawn(cx);
        let theme = |cx: &mut gpui::VisualTestContext| root.read_with(cx, |root, _| root.theme);
        let open =
            |cx: &mut gpui::VisualTestContext| root.read_with(cx, |root, _| root.customizing());

        // Theme: System / Light / Dark, three buttons.
        click(cx, "theme-mode-light");
        assert_eq!(theme(cx), Theme::light());
        click(cx, "theme-mode-system");
        assert_eq!(
            root.read_with(cx, |root, _| root.settings.theme_mode()),
            Mode::System
        );
        click(cx, "theme-mode-dark");
        assert_eq!(theme(cx), Theme::dark());

        click(cx, "customize-theme");
        assert!(open(cx));
        assert!(cx.debug_bounds("customizer").is_some());
        click(cx, "customizer-preset-1");
        assert_ne!(
            theme(cx),
            Theme::dark(),
            "the first preset repaints the window"
        );
        click(cx, "customizer-close");
        assert!(!open(cx));
        assert_eq!(theme(cx), Theme::dark());
        assert!(cx.debug_bounds("customizer").is_none());

        // Escape right after the click that opened it.
        click(cx, "customize-theme");
        assert!(open(cx));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!open(cx), "Escape right after opening closes the card");

        // Escape with the focus on a Settings control outside the card.
        click(cx, "customize-theme");
        click(cx, "customizer-preset-2");
        assert_ne!(theme(cx), Theme::dark());
        click(cx, "theme-mode-dark");
        assert!(open(cx));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!open(cx), "Escape from outside the card closes it");
        assert_eq!(theme(cx), Theme::dark(), "and its preview goes");
    }

    /// Code review r1 (bugs+impl-3): a valid short hex is taken when the
    /// field is left, through the real field's blur; an invalid one is not.
    #[gpui::test]
    fn a_short_hex_is_taken_when_the_field_is_left(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::a_short_hex_is_taken_when_the_field_is_left"
        )) {
            return;
        }
        let (root, _dir, cx) = drawn(cx);
        let current = |cx: &mut gpui::VisualTestContext| {
            root.read_with(cx, |root, _| {
                root.customizer
                    .as_ref()
                    .unwrap()
                    .draft
                    .current()
                    .map(|t| t.color)
            })
        };
        click(cx, "customize-theme");
        click(cx, "customizer-hex");
        cx.simulate_input("#abc");
        cx.run_until_parked();
        assert_eq!(current(cx), None, "a short form waits");
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        assert_eq!(current(cx), Some(0xaabbcc), "leaving the field takes it");
        assert_ne!(root.read_with(cx, |root, _| root.theme), Theme::dark());

        click(cx, "customizer-hex");
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("nope");
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        assert_eq!(
            current(cx),
            Some(0xaabbcc),
            "an invalid value changes nothing"
        );
        root.read_with(cx, |root, _| {
            let customizer = root.customizer.as_ref().unwrap();
            assert!(
                customizer.hex.is_none() && !customizer.invalid,
                "the field shows the colour again"
            );
        });
    }

    /// Code review r2 (bugs+impl-2): Escape in a Settings field that is not
    /// the hex field — the search — closes the card; Escape in the hex
    /// field restores the field and leaves the card open.
    #[gpui::test]
    fn escape_in_a_settings_field_closes_the_card(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::escape_in_a_settings_field_closes_the_card"
        )) {
            return;
        }
        let (root, _dir, cx) = drawn(cx);
        let open =
            |cx: &mut gpui::VisualTestContext| root.read_with(cx, |root, _| root.customizing());
        click(cx, "customize-theme");
        click(cx, "customizer-preset-1");
        click(cx, "customizer-hex");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(open(cx), "the hex field's Escape is its own");
        click(cx, "settings-search");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!open(cx), "Escape in the search closes the card");
        assert_eq!(root.read_with(cx, |root, _| root.theme), Theme::dark());
    }

    /// Code review r2 (arch+quality-1): with no project open, a Settings
    /// control keeps the focus the keyboard gave it — Shift+Tab reaches the
    /// Dark button after a frame, and Enter presses it.
    #[gpui::test]
    fn settings_controls_keep_the_keyboard_without_a_project(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::settings_controls_keep_the_keyboard_without_a_project"
        )) {
            return;
        }
        let (root, _dir, cx) = drawn(cx);
        let mode = |cx: &mut gpui::VisualTestContext| {
            root.read_with(cx, |root, _| root.settings.theme_mode())
        };
        click(cx, "theme-mode-system");
        assert_eq!(mode(cx), Mode::System);
        // The keyboard starts on Customize theme, the stop after the Theme
        // buttons; a frame between the strokes, as on the screen.
        cx.update(|window, cx| {
            let button = root.read(cx).customize_button.clone();
            window.focus(&button, cx);
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.simulate_keystrokes("shift-tab");
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        // A key's click is on its release; simulated strokes are presses.
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        cx.run_until_parked();
        assert_eq!(mode(cx), Mode::Dark, "Shift+Tab then Enter pressed Dark");
    }

    #[gpui::test]
    fn leaving_appearance_closes_the_card(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.open_customizer(cx);
            root.customizer_color(0x3b82f6, cx);
            root.settings_page = Some(Section::General);
            root.settle_customizer(cx);
        });
        cx.run_until_parked();
        root.read_with(cx, |root, _| {
            assert!(!root.customizing());
            assert_eq!(root.theme, Theme::dark());
        });
    }
}
