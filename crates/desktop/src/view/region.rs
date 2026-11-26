//! A region of the window drawn as a view of its own, so that GPUI can
//! reuse its last frame.
//!
//! **The root is not cached, and cannot be**: it is the window. A view's
//! notify marks it and every view above it dirty, so a terminal printing
//! in the tab in front is `Root::render` again on each of its frames —
//! and while every region was an inline method of `Root`, that was the
//! column, the docks, their layout and their paint rebuilt for a picture
//! in which only the terminal moved. Measured on 2026-09-24 with twelve
//! projects and two docks open: 2.7 ms a frame, of which the terminal was
//! 0.24 ms and the column 0.7.
//!
//! GPUI's remedy for panes and panels: a child view handed
//! over as `AnyView::cached`, whose layout, prepaint and paint are reused
//! from the last frame unless the view itself was notified since, or its
//! bounds moved, or the window refreshed. A region here is that view and
//! nothing more — it draws with the same `Root` method as before, through
//! `root.update`, so the drawing code and its listeners are `Root`'s as
//! they were.
//!
//! **Freshness is the observer.** A region re-renders on every notify of
//! the root, because the root is what it draws, and not on a terminal's
//! notify, because a terminal is not what it draws. Two things follow:
//! a `Root` handler that changes what a region shows and forgets
//! `cx.notify()` now shows up as a stale region instead of being covered
//! by the next terminal frame; and a region must not read an entity that
//! is neither `Root` nor a view drawn inside it — such a notify would
//! reach nothing here. A view drawn inside, an input or a list, marks the
//! region dirty itself, since GPUI dirties every view above the one that
//! notified.

use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, WeakEntity, Window, div};

use crate::Root;

#[cfg(test)]
thread_local! {
    /// How many times a region rendered on this thread — a test's count of
    /// the frames a clock or a scroll asked for.
    pub(crate) static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Which region a view draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    /// The column of projects and worktrees, pinned beside the work.
    Column,
    /// A dock's panel: its head and the tool it shows.
    Dock(guild_session::Dock),
}

/// A region's view: the root it draws from, and the observation that keeps
/// it fresh.
pub(crate) struct Region {
    root: WeakEntity<Root>,
    part: Part,
    _root_changed: Subscription,
}

impl Region {
    pub(crate) fn new(root: &Entity<Root>, part: Part, cx: &mut Context<Self>) -> Self {
        Self {
            root: root.downgrade(),
            part,
            _root_changed: cx.observe(root, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for Region {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "native-preview")]
        crate::native_preview::adaptive_refresh_probe::REGION_RENDERS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(test)]
        RENDERS.with(|renders| renders.set(renders.get() + 1));
        let part = self.part;
        self.root
            .update(cx, |root, cx| match part {
                Part::Column => root.render_column(cx),
                Part::Dock(guild_session::Dock::Bottom) => root.render_bottom_dock(window, cx),
                Part::Dock(guild_session::Dock::Left) => root.render_left_panel(window, cx),
                Part::Dock(guild_session::Dock::Right) => root.render_right_panel(window, cx),
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}
