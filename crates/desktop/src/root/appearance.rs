//! Which palette is in effect, and the one path that hands it to
//! everything that draws — `openspec/specs/appearance-themes`.
//!
//! The palette is not read off the settings file: System follows the
//! macOS appearance, which only the window can answer. So the window
//! resolves it — mode, appearance, and the person's tints — and every
//! consumer is brought up to that answer here, and nowhere else.
//!
//! **Each group remembers what it last received.** Chrome, the component
//! theme, terminals and satellites move with `Root::theme`; file, diff and
//! Annotate tabs keep a `Look` of their own and move with
//! `Root::tabs_theme`, because a slider's drag may leave their restyle for
//! its release. A release carries the value the last drag step already
//! previewed, so a guard comparing with `Root::theme` alone would return
//! before the tabs were told (spec review r1, finding 2).

use gpui::Context;

use crate::{Root, theme};

impl Root {
    /// The palette these settings mean now, with the system as it is.
    /// While the customizer is open, the theme it edits with its unsaved
    /// choices — the preview.
    pub(crate) fn resolved_theme(&self) -> theme::Theme {
        if let Some(customizer) = &self.customizer {
            let (kind, custom) = customizer.draft.preview();
            return theme::Theme::customized(kind, &custom);
        }
        theme::in_effect(&self.settings, self.system_dark)
    }

    /// Bring every consumer that is behind up to [`Self::resolved_theme`].
    ///
    /// `flush` also restyles the file, diff and Annotate tabs when only
    /// they are behind — what a slider's release, Save, Reset, closing the
    /// customizer and every discrete choice ask for. A drag step may pass
    /// `false` and leave the tabs for the release. Storing the settings is
    /// the caller's business: a preview is never written.
    pub(crate) fn apply_theme_in_effect(&mut self, flush: bool, cx: &mut Context<Root>) {
        let theme = self.resolved_theme();
        if theme != self.theme {
            self.theme = theme;
            crate::editor::apply_theme(&self.theme, self.settings.ui_font_size, cx);
            // Every terminal, the Output panel's run included: a run left
            // on the old palette is the most visible way a theme can be
            // half applied, and a walk over tabs alone would miss it.
            for view in self.terminals() {
                view.update(cx, |view, cx| view.set_theme(self.theme, cx));
            }
            self.restyle_satellites(cx);
            cx.notify();
        }
        if flush && self.tabs_theme != self.theme {
            self.tabs_theme = self.theme;
            self.restyle_file_tabs(cx);
            self.restyle_diff_tabs(cx);
            self.restyle_annotate_tabs(cx);
            cx.notify();
        }
    }

    /// Theme: System, Light or Dark — Settings → Appearance.
    pub(crate) fn set_theme_mode(&mut self, mode: theme::Mode, cx: &mut Context<Root>) {
        self.settings.theme_mode = Some(mode);
        self.hand_mode_to_appkit(cx);
        self.apply_theme_in_effect(true, cx);
        self.store_settings();
        cx.notify();
    }

    /// AppKit draws in the mode: Light or Dark forced application-wide,
    /// System handed back — and then the system read again, since while
    /// a mode was forced every appearance AppKit reported was that mode.
    fn hand_mode_to_appkit(&mut self, cx: &mut Context<Root>) {
        let mode = self.settings.theme_mode();
        let app: &gpui::App = cx;
        if let Some(dark) = theme::follow_mode(
            mode,
            |forced| app.set_window_appearance(forced),
            || app.window_appearance(),
        ) {
            self.system_dark = dark;
        }
    }

    /// macOS changed its appearance. Only a System mode moves with it,
    /// and only when the answer changed. Under Light or Dark the
    /// appearance reported is the one Guild forced, not the system's, so
    /// there is nothing to learn from it.
    pub(crate) fn appearance_changed(
        &mut self,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Root>,
    ) {
        if self.settings.theme_mode() != theme::Mode::System {
            return;
        }
        self.system_dark = theme::system_dark(appearance);
        self.apply_theme_in_effect(true, cx);
    }

    /// Hear the appearance change — once per main window, from its first
    /// frame, like the activation is.
    pub(crate) fn watch_appearance(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        if theme::system_dark(window.appearance()) != self.system_dark {
            self.appearance_changed(window.appearance(), cx);
        }
        self.appearance = Some(cx.observe_window_appearance(window, |root, window, cx| {
            root.appearance_changed(window.appearance(), cx);
        }));
    }

    /// Push Commits and the diff window, when open, draw with the theme in
    /// effect rather than the one they were opened with.
    fn restyle_satellites(&mut self, cx: &mut Context<Root>) {
        let theme = self.theme;
        let look = self.file_look();
        let ui = self.ui();
        let push = self.push_dialog.borrow().as_ref().and_then(|w| w.upgrade());
        if let Some(dialog) = push {
            let look = look.clone();
            dialog.update(cx, |dialog, cx| dialog.set_theme(theme, look, ui, cx));
        }
        let diff = self.diff_window.borrow().as_ref().and_then(|w| w.upgrade());
        if let Some(window) = diff {
            window.update(cx, |window, cx| window.set_theme(theme, look, ui, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, Entity, TestAppContext, WindowAppearance};

    use crate::Root;
    use crate::theme::{Kind, Mode, Theme};

    /// A diagnostic Root, which owns its settings writes in `dir`, with
    /// one project at `dir/project` holding `a.txt`.
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
            root.update(cx, |root, _| root.active_project = Some(project));
            owner = Some(root);
            gpui::Empty
        });
        (owner.unwrap(), dir, cx)
    }

    #[gpui::test]
    fn system_follows_the_appearance_and_an_explicit_mode_does_not(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.set_theme_mode(Mode::System, cx);
            root.appearance_changed(WindowAppearance::Dark, cx);
            assert_eq!(root.theme, Theme::dark());
            root.appearance_changed(WindowAppearance::Light, cx);
            assert_eq!(root.theme, Theme::light());
            root.appearance_changed(WindowAppearance::VibrantDark, cx);
            assert_eq!(root.theme, Theme::dark());
            root.set_theme_mode(Mode::Light, cx);
            assert_eq!(root.theme.kind, Kind::Light);
            root.appearance_changed(WindowAppearance::Dark, cx);
            assert_eq!(
                root.theme.kind,
                Kind::Light,
                "an explicit mode ignores the system"
            );
            assert_eq!(root.settings.theme_mode, Some(Mode::Light));
        });
    }

    /// Back to System after an explicit mode: the window reads the system
    /// again instead of keeping what it last knew (code review r1,
    /// bugs+impl-2). The test platform's system is Light, so a cached
    /// Dark left over from before is what a skipped re-read would draw.
    /// Under the explicit mode, the appearance AppKit reports is Guild's
    /// own and teaches the window nothing about the system.
    #[gpui::test]
    fn back_to_system_reads_the_system_again(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.set_theme_mode(Mode::Dark, cx);
            root.system_dark = true;
            root.appearance_changed(WindowAppearance::Light, cx);
            assert!(
                root.system_dark,
                "a forced mode's appearance is not the system's"
            );
            root.set_theme_mode(Mode::System, cx);
            assert!(!root.system_dark, "the system was read again");
            assert_eq!(root.theme, Theme::light());
        });
    }

    /// A saved tint is what the window draws once applied — the mode's
    /// palette with it, the other palette untouched.
    #[gpui::test]
    fn a_saved_customization_is_what_the_window_draws(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| {
            root.settings.theme_custom.dark.accent = Some(guild_session::Tint {
                color: 0xf97316,
                intensity: 100,
            });
            root.set_theme_mode(Mode::Dark, cx);
            assert_eq!(root.theme.accent.patina, 0xf97316);
            assert_eq!(
                root.theme,
                Theme::resolve(Kind::Dark, &root.settings.theme_custom)
            );
            root.set_theme_mode(Mode::Light, cx);
            assert_eq!(root.theme, Theme::light());
        });
    }

    /// Settings → Appearance → Theme: each choice is written down, in the
    /// diagnostic Root's own state directory.
    #[gpui::test]
    fn each_theme_choice_is_stored(cx: &mut TestAppContext) {
        let (root, dir, cx) = root(cx);
        for (mode, wire) in [
            (Mode::Light, "light"),
            (Mode::Dark, "dark"),
            (Mode::System, "system"),
        ] {
            root.update(cx, |root, cx| root.set_theme_mode(mode, cx));
            guild_session::flush();
            let file = dir.path().join("state").join("settings.json");
            let saved: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            assert_eq!(saved["theme_mode"], wire);
            assert_eq!(
                root.read_with(cx, |root, _| root.settings.theme_mode()),
                mode
            );
        }
    }

    /// Spec "Slider release reaches every surface": a drag step that left
    /// the tabs behind is caught up by a release carrying the same value.
    #[gpui::test]
    fn a_flush_with_the_same_theme_restyles_tabs_left_behind(cx: &mut TestAppContext) {
        use super::tab_looks::{assert_all, open_all};
        let (root, _dir, cx) = root(cx);
        root.update(cx, |root, cx| root.set_theme_mode(Mode::Dark, cx));
        open_all(&root, "a.txt", cx);
        assert_all(&root, Theme::dark(), "opened", cx);
        root.update(cx, |root, cx| {
            // A drag step: the window previews, the tabs wait.
            root.settings.theme_mode = Some(Mode::Light);
            root.apply_theme_in_effect(false, cx);
            assert_eq!(root.theme.kind, Kind::Light);
        });
        assert_all(&root, Theme::dark(), "deferred", cx);
        root.update(cx, |root, cx| root.apply_theme_in_effect(true, cx));
        assert_all(&root, Theme::light(), "the release caught the tabs up", cx);
        // Nothing behind: a repeat flush restyles nothing.
        root.update(cx, |root, cx| {
            let before = root.tabs_theme;
            root.apply_theme_in_effect(true, cx);
            assert_eq!(root.tabs_theme, before);
        });
    }

    /// Spec "Open Push Commits window follows a change", and the diff
    /// window with it.
    #[gpui::test]
    fn open_satellites_follow_a_change(cx: &mut TestAppContext) {
        let (root, _dir, cx) = root(cx);
        let project = root.read_with(cx, |root, _| root.active_project.unwrap());
        let weak = root.downgrade();
        let dialog = cx.new(|cx| {
            crate::view::push_dialog::PushDialog::new(weak.clone(), project, Theme::dark(), cx)
        });
        let folder = root.read_with(cx, |root, _| {
            root.desk.project(project).unwrap().path.clone()
        });
        let showing = crate::view::diff_window::Showing {
            project,
            folder,
            sha: "HEAD".into(),
            files: vec!["a.txt".into()],
            index: 0,
        };
        let diff = cx.new(|cx| {
            crate::view::diff_window::DiffWindow::new(weak.clone(), showing, Theme::dark(), cx)
        });
        // Push Commits with a file's diff open in its preview (code review
        // r2, docs+tests-1): the nested diffs are what a person reads.
        let preview = cx.new(|cx| {
            let (look, ui) = root.read_with(cx, |root, _| (root.file_look(), root.ui()));
            let folder = root.read(cx).desk.project(project).unwrap().path.clone();
            crate::diff_tab::DiffTab::open_inspection(
                &folder,
                "a.txt".into(),
                "HEAD".into(),
                Default::default(),
                look,
                ui,
                cx,
            )
        });
        dialog.update(cx, |dialog, _| {
            dialog.show_preview_for_test(preview.clone())
        });
        let nested = |cx: &mut gpui::VisualTestContext| {
            (
                preview.read_with(cx, |tab, _| tab.look().theme),
                diff.read_with(cx, |window, cx| window.tab_for_test().read(cx).look().theme),
            )
        };
        root.update(cx, |root, cx| {
            *root.push_dialog.borrow_mut() = Some(dialog.downgrade());
            *root.diff_window.borrow_mut() = Some(diff.downgrade());
            root.set_theme_mode(Mode::Dark, cx);
            root.set_theme_mode(Mode::Light, cx);
        });
        assert_eq!(dialog.read_with(cx, |d, _| d.theme().kind), Kind::Light);
        assert_eq!(diff.read_with(cx, |d, _| d.theme().kind), Kind::Light);
        assert_eq!(
            nested(cx),
            (Theme::light(), Theme::light()),
            "the nested diffs"
        );
        // A tint, the kind unchanged.
        let tinted = root.update(cx, |root, cx| {
            root.settings.theme_custom.light.background = Some(guild_session::Tint {
                color: 0x3b82f6,
                intensity: 60,
            });
            root.apply_theme_in_effect(true, cx);
            root.theme
        });
        assert_ne!(tinted, Theme::light());
        assert_eq!(dialog.read_with(cx, |d, _| d.theme()), tinted);
        assert_eq!(diff.read_with(cx, |d, _| d.theme()), tinted);
        assert_eq!(nested(cx), (tinted, tinted), "the nested diffs take a tint");
    }
}

/// The three tab kinds that keep a `Look` of their own — File, Diff and
/// Annotate — opened on one file, and the palette each is drawn with.
/// Code review r1 (docs+tests-3): a restyle test that opens only a file
/// cannot see a dropped `restyle_diff_tabs` or `restyle_annotate_tabs`.
#[cfg(test)]
pub(crate) mod tab_looks {
    use gpui::{Entity, VisualTestContext};

    use crate::theme::Theme;
    use crate::{Body, Root};

    /// Open `file` as a file tab, its diff against HEAD, and its annotation.
    pub(crate) fn open_all(root: &Entity<Root>, file: &str, cx: &mut VisualTestContext) {
        let path = std::path::PathBuf::from(file);
        // A diff and a blame read git on threads of their own.
        cx.executor().allow_parking();
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                let project = root.active_project.expect("a project");
                root.open_file(&path, window, cx);
                root.open_diff(Some(path.clone()), guild_session::Base::Head, window, cx);
                root.restore_annotate_tab(project, path.clone(), cx);
            })
        });
        cx.run_until_parked();
        let kinds = looks(root, cx);
        assert_eq!(
            kinds.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            ["file", "diff", "annotate"],
            "one tab of each kind is open"
        );
    }

    /// The palette each open File, Diff and Annotate tab is drawn with.
    pub(crate) fn looks(
        root: &Entity<Root>,
        cx: &mut VisualTestContext,
    ) -> Vec<(&'static str, Theme)> {
        root.read_with(cx, |root, cx| {
            let mut found = Vec::new();
            for body in root.tabs.iter().flat_map(crate::Tab::bodies) {
                match body {
                    Body::File { tab, .. } => found.push(("file", tab.read(cx).look().theme)),
                    Body::Diff { tab, .. } => found.push(("diff", tab.read(cx).look().theme)),
                    Body::Annotate { tab, .. } => {
                        found.push(("annotate", tab.read(cx).look().theme))
                    }
                    _ => {}
                }
            }
            found.sort_by_key(|(kind, _)| match *kind {
                "file" => 0,
                "diff" => 1,
                _ => 2,
            });
            found
        })
    }

    /// Every one of the three is drawn with `theme`.
    pub(crate) fn assert_all(
        root: &Entity<Root>,
        theme: Theme,
        why: &str,
        cx: &mut VisualTestContext,
    ) {
        for (kind, drawn) in looks(root, cx) {
            assert_eq!(drawn, theme, "{why}: the {kind} tab");
        }
    }
}
