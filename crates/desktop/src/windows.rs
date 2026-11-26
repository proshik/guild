//! Windows beside Guild's own — M8.3.6, decision 1.
//!
//! Guild had exactly one window until Push Commits was asked
//! for as a window of its own, with its files' diff in another. A second
//! window is a **satellite**: opened by the main one, at most one of each
//! kind, closed with it, and never what decides whether the application
//! lives — the main window alone does.

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{AnyView, AnyWindowHandle, App, Bounds, Pixels, Window, WindowBounds, WindowOptions};

/// The kinds of window beside the main one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Satellite {
    /// Push Commits.
    Push,
    /// A commit's file as a diff, stepping through the commit.
    Diff,
}

/// Which satellite is open, by kind. Generic over the handle so that the
/// rule can be tested without a window.
#[derive(Debug, Clone)]
pub struct Registry<H> {
    open: HashMap<Satellite, H>,
}

impl<H> Default for Registry<H> {
    fn default() -> Self {
        Self {
            open: HashMap::new(),
        }
    }
}

impl<H: Copy> Registry<H> {
    /// The window of a kind, if one was opened and not found gone since.
    pub fn handle(&self, kind: Satellite) -> Option<H> {
        self.open.get(&kind).copied()
    }

    /// A window of a kind was opened.
    pub fn opened(&mut self, kind: Satellite, handle: H) {
        self.open.insert(kind, handle);
    }

    /// A window of a kind was found gone — closed by its traffic light, by
    /// `cmd-w`, by Escape.
    pub fn gone(&mut self, kind: Satellite) {
        self.open.remove(&kind);
    }

    /// Every window still thought open.
    pub fn all(&self) -> Vec<H> {
        self.open.values().copied().collect()
    }
}

/// Whether the application quits, the main window being `main` and the
/// windows left being `remaining`: **when the main window is gone**, and
/// only then. A satellite left behind is closed by the quit.
pub fn quits<W: PartialEq>(main: W, remaining: &[W]) -> bool {
    !remaining.contains(&main)
}

/// Where a window of `size` opens over the main window standing at
/// `main`: centred on it — 2026-09-24. Both in the main window's display's
/// coordinates, which is what `open`'s `display` makes them.
pub fn over(main: Bounds<Pixels>, size: gpui::Size<Pixels>) -> Bounds<Pixels> {
    Bounds::centered_at(main.center(), size)
}

/// Open a satellite: a window with the system's title bar and `title`,
/// at `bounds` on `display`, whose root view is the component's `Root` over what `view`
/// builds — `gpui_component::Root::read` expects exactly that of every
/// window. Our satellite frame reapplies Guild's rem conversion after the
/// component root's raw font size. The error is what the
/// platform said, as text: this crate keeps `anyhow` to its binary.
pub fn open(
    title: String,
    bounds: Bounds<Pixels>,
    display: Option<gpui::DisplayId>,
    view: impl FnOnce(&mut Window, &mut App) -> AnyView + 'static,
    cx: &mut App,
) -> Result<AnyWindowHandle, String> {
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // **The main window's display** — 2026-09-24. A window's
                // bounds are its own display's coordinates, and with no
                // display GPUI lays them on the primary one: with two
                // monitors Push Commits opened on the other screen.
                display_id: display,
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                ..Default::default()
            },
            move |window, cx| {
                let view = view(window, cx);
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            },
        )
        .map_err(|err| err.to_string())?;
    Ok(handle.into())
}
