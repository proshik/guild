//! The windows beside the main one — M8.3.6, decision 1.
//!
//! [`crate::windows`] is the rule; this is the half that needs the window:
//! opening a satellite or raising the one already open, closing them all,
//! and remembering where each was left.

use gpui::AppContext as _;
use gpui::{AnyView, AnyWindowHandle, App, Bounds, Context, Window, px, size};
use guild_session::WindowPlace;

use crate::Root;
use crate::windows::{self, Satellite};

impl Root {
    /// Bring the satellite of `kind` to the front when it is open; answer
    /// whether it was. A window closed since — by its traffic light, by
    /// `cmd-w` — is found gone here and forgotten.
    pub(crate) fn raise_satellite(&mut self, kind: Satellite, cx: &mut App) -> bool {
        let Some(handle) = self.satellites.handle(kind) else {
            return false;
        };
        match handle.update(cx, |_, window, _| window.activate_window()) {
            Ok(()) => true,
            Err(_) => {
                self.satellites.gone(kind);
                false
            }
        }
    }

    /// The satellite of `kind`, if one is open.
    pub(crate) fn satellite(&mut self, kind: Satellite, cx: &mut App) -> Option<AnyWindowHandle> {
        let handle = self.satellites.handle(kind)?;
        if handle.update(cx, |_, _, _| ()).is_err() {
            self.satellites.gone(kind);
            return None;
        }
        Some(handle)
    }

    /// Open a satellite of `kind` — or raise the one already open, which
    /// is what a second `cmd-shift-k` means. Where it opens is where the
    /// last one of its kind was left, or `default` centred.
    ///
    /// **Deferred.** `open_window` draws the new window's first frame at
    /// once, and a frame clears the element arena every window shares: a
    /// window opened while the main one is mid-draw frees the main window's
    /// elements from under it, and GPUI segfaults dropping one. Measured on
    /// the first try, 2026-09-16.
    pub(crate) fn open_satellite(
        &mut self,
        kind: Satellite,
        title: String,
        default: (f32, f32),
        view: impl FnOnce(&mut Window, &mut App) -> AnyView + 'static,
        cx: &mut Context<Root>,
    ) {
        if self.raise_satellite(kind, cx) {
            return;
        }
        // Held as opening, so a second ask before the defer runs does not
        // open a second window.
        if self.satellites_opening.contains(&kind) {
            return;
        }
        self.satellites_opening.insert(kind);
        let bounds = self.satellite_bounds(kind, default, cx);
        let display = self.main_display.map(|(display, _)| display);
        // **Outside this entity's update**: the new window's view is built
        // and drawn by `open_window`, and it reads `Root` — which, inside
        // an update of `Root`, is a double lease and a panic.
        cx.spawn(async move |root, cx| {
            let opened = cx
                .update(|cx| windows::open(title, bounds, display, view, cx))
                .map_err(|err| err.to_string());
            let _ = root.update(cx, |root, cx| {
                root.satellites_opening.remove(&kind);
                match opened {
                    Ok(handle) => {
                        root.satellites.opened(kind, handle);
                        let _ = handle.update(cx, |_, window, _| window.activate_window());
                    }
                    Err(err) => {
                        tracing::warn!(err = %err, ?kind, "a window beside the main one did not open");
                    }
                }
            });
        })
        .detach();
    }

    /// Where a satellite of `kind` opens: where the last one was left, or
    /// `default` centred.
    fn satellite_bounds(
        &self,
        kind: Satellite,
        default: (f32, f32),
        cx: &App,
    ) -> Bounds<gpui::Pixels> {
        self.satellite_place(kind)
            .filter(WindowPlace::is_usable)
            .map(|place| Bounds {
                origin: gpui::point(px(place.x), px(place.y)),
                size: size(px(place.width), px(place.height)),
            })
            .unwrap_or_else(|| {
                let size = size(px(default.0), px(default.1));
                match self.main_display {
                    Some((_, main)) => windows::over(main, size),
                    None => Bounds::centered(None, size, cx),
                }
            })
    }

    /// Where a satellite of `kind` was last left.
    fn satellite_place(&self, kind: Satellite) -> Option<WindowPlace> {
        match kind {
            Satellite::Push => self.settings.push_window,
            Satellite::Diff => self.settings.diff_window,
        }
    }

    /// Remember where a satellite is — called from its render, so a move
    /// or a resize is kept. Queued: a drag changes it on every frame.
    pub(crate) fn remember_satellite(&mut self, kind: Satellite, window: &Window) {
        if window.is_fullscreen() {
            return;
        }
        let gpui::WindowBounds::Windowed(bounds) = window.window_bounds() else {
            return;
        };
        // **The content's size, not the frame's**: a satellite has the
        // system's title bar, `open_window` takes the size as the content's,
        // and the frame's height reopened a window 32 taller every time.
        let content = window.viewport_size();
        let place = Some(WindowPlace {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(content.width),
            height: f32::from(content.height),
        });
        let slot = match kind {
            Satellite::Push => &mut self.settings.push_window,
            Satellite::Diff => &mut self.settings.diff_window,
        };
        if *slot == place {
            return;
        }
        *slot = place;
        self.store_settings();
    }
}

impl Root {
    /// Remember Push Commits' two splits — queued, like the window's place.
    pub(crate) fn remember_push_splits(&mut self, splits: (f32, f32)) {
        if self.settings.push_splits == Some(splits) {
            return;
        }
        self.settings.push_splits = Some(splits);
        self.store_settings();
    }

    /// A commit's file in the diff window — opened, or shown in the one
    /// already open. M8.3.6, decision 7.
    pub(crate) fn open_diff_window(
        &mut self,
        project: guild_session::ProjectId,
        sha: String,
        files: Vec<std::path::PathBuf>,
        index: usize,
        cx: &mut Context<Root>,
    ) {
        let Some(folder) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let showing = crate::view::diff_window::Showing {
            project,
            folder,
            sha,
            files,
            index,
        };
        let open = self.diff_window.borrow().as_ref().and_then(|w| w.upgrade());
        if let Some(view) = open
            && self.raise_satellite(Satellite::Diff, cx)
        {
            // Reopening reads Root for the diff's look. Release this
            // Root update first, just as opening a new satellite does.
            cx.defer(move |cx| view.update(cx, |view, cx| view.show(showing, cx)));
            return;
        }
        let root = cx.entity().downgrade();
        let theme = self.theme;
        let slot = self.diff_window.clone();
        self.open_satellite(
            Satellite::Diff,
            String::new(),
            (1100.0, 760.0),
            move |_window, cx| {
                let view = cx.new(|cx| {
                    crate::view::diff_window::DiffWindow::new(root.clone(), showing, theme, cx)
                });
                *slot.borrow_mut() = Some(view.downgrade());
                view.into()
            },
            cx,
        );
    }

    /// The diff window's pencil: the file in the main window, at `line`,
    /// in `project`, and the main window in front.
    ///
    /// **Deferred**: the ask comes from inside an update of this entity,
    /// and the main window's update would be a second one.
    pub(crate) fn open_in_main(
        &mut self,
        project: guild_session::ProjectId,
        path: std::path::PathBuf,
        line: usize,
        cx: &mut Context<Root>,
    ) {
        let Some(handle) = self.main_window else {
            return;
        };
        let me = cx.entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                window.activate_window();
                me.update(cx, |root, cx| {
                    if root.active_project != Some(project) {
                        root.select_project(project, cx);
                    }
                    root.open_file_at(path, line, 0, window, cx);
                });
            });
        });
    }
}
