//! What the window draws.
//!
//! Apart from what it *does*, which is [`crate::root`]. The two grew
//! together in one file until it reached three thousand lines, and the
//! drawing is the half that keeps growing: a side panel, a file tree, a
//! diff pane are each another screen. Beside the behaviour
//! each would have made a bigger file; here each is a new one.
//!
//! Every screen is an `impl Root` block. Rust lets an impl live in any
//! module of the crate that declares the type, and a window's screens are
//! the window's however the files are arranged.

use gpui::{HighlightStyle, IntoElement, StyledText, rgb};

use crate::syntax::Span;

/// A line of code, coloured by its spans — the one place every surface
/// we draw ourselves turns a paint into a run.
///
/// `None` draws it plain, which is what a caller answers when the spans
/// have not arrived, when nothing claims the file, or when the line is
/// not the one the spans were taken from.
///
/// That refusal is not a fallback, it is the point. The rows come from
/// `git diff` and the spans from parsing the blobs, and colouring a row
/// they disagree about would paint a keyword-red `if` on a line with no
/// `if` in it — no gap, no artefact, nothing in any log. It is also what
/// makes a late answer safe: a buffer that has moved ahead of its spans
/// draws plain until they catch up.
pub(crate) fn code(
    text: String,
    spans: Option<&[Span]>,
    theme: &crate::theme::Theme,
) -> gpui::AnyElement {
    let Some(spans) = spans else {
        return text.into_any_element();
    };
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = spans
        .iter()
        .filter_map(|span| {
            let (colour, italic, bold) = theme.of_paint(span.paint?);
            Some((
                span.at.clone(),
                HighlightStyle {
                    color: Some(rgb(colour).into()),
                    font_style: italic.then_some(gpui::FontStyle::Italic),
                    font_weight: bold.then_some(gpui::FontWeight::BOLD),
                    ..Default::default()
                },
            ))
        })
        .collect();
    StyledText::new(text)
        .with_highlights(highlights)
        .into_any_element()
}

/// Where a scrolled list's thumb goes, as its top and its length, if there
/// is anything to scroll.
///
/// GPUI scrolls and draws no bar, so every list of ours that scrolls draws
/// this one: the Recent Projects popup and the Settings column. `None`
/// when the list fits — a bar that is always there takes a column for
/// something only interesting while there is more list than box, the rule
/// the terminal's scrollbar follows — and before the first frame, when
/// nothing has been measured yet.
pub(crate) fn thumb(scroll: &gpui::ScrollHandle) -> Option<(f32, f32)> {
    thumb_along(
        f32::from(scroll.bounds().size.height),
        f32::from(scroll.max_offset().y),
        f32::from(scroll.offset().y),
    )
}

/// [`thumb`] from the three numbers a `ScrollHandle` holds: the view's
/// height, what is left over below it, and the offset — which runs
/// **negative** as the list goes down. Pure, so the arithmetic is checked
/// without a frame.
fn thumb_along(view: f32, over: f32, offset: f32) -> Option<(f32, f32)> {
    /// Shorter than this and there is nothing left to take hold of.
    const LEAST: f32 = 24.0;

    if view <= 0.0 || over <= 0.0 {
        return None;
    }
    let length = (view * (view / (view + over))).max(LEAST).min(view);
    let progress = (-offset / over).clamp(0.0, 1.0);
    Some(((view - length) * progress, length))
}

pub(crate) mod activity;
pub mod agents;
pub mod annotate;
pub mod areas;
pub mod body;
pub mod branches;
pub mod changes;
pub mod column;
pub mod console;
pub mod diff;
pub mod diff_window;
pub mod dock_menu;
pub mod drag;
pub mod everywhere;
pub mod find_in_files;
pub mod fonts;
pub(crate) mod forge;
pub mod goto_line;
pub mod kind;
pub mod log;
pub mod log_filters;
pub mod menu;
pub mod new_worktree;
pub mod notices;
pub mod panels;
pub mod panes;
pub mod picker;
pub mod popup;
pub mod preview;
pub mod problems;
pub(crate) mod process;
pub mod push_dialog;
pub mod recent;
pub mod recent_files;
pub mod region;
pub mod release_notes;
pub(crate) mod resize;
pub mod run;
pub mod satellite;
pub mod search;
pub mod servers;
pub mod settings;
pub mod start;
pub mod status;
pub mod tab_list;
pub mod tabs;
pub mod terminal_panel;
pub mod titlebar;
pub mod tooltip;
pub mod tree;
pub(crate) mod tree_drag;
pub mod usage;
pub mod usages;
pub mod workspaces;

#[cfg(test)]
mod tests {
    use super::*;

    /// **One scrollbar, drawn by one rule.** The Recent Projects list and
    /// the Settings column each carried a copy of this arithmetic, and a
    /// copy is where a fix lands once. No bar while the rows fit or before
    /// anything has been measured; a thumb as long as the view's share of
    /// the whole, never shorter than a hand can take; and the offset
    /// running negative as the list goes down.
    #[test]
    fn a_thumb_is_the_views_share_of_the_list_and_runs_its_track() {
        assert_eq!(thumb_along(0.0, 0.0, 0.0), None, "nothing measured yet");
        assert_eq!(thumb_along(300.0, 0.0, 0.0), None, "the rows fit");

        assert_eq!(thumb_along(300.0, 300.0, 0.0), Some((0.0, 150.0)));
        assert_eq!(thumb_along(300.0, 300.0, -150.0), Some((75.0, 150.0)));
        assert_eq!(thumb_along(300.0, 300.0, -300.0), Some((150.0, 150.0)));
        assert_eq!(
            thumb_along(300.0, 300.0, -900.0),
            Some((150.0, 150.0)),
            "an overshoot stays at the end of the track"
        );

        assert_eq!(
            thumb_along(100.0, 9_900.0, 0.0),
            Some((0.0, 24.0)),
            "a long list keeps a thumb a hand can take"
        );
    }
}

pub mod overlay;
mod overlay_viewport;

mod controls;
pub(crate) use controls::{is_tab_navigation, tab_navigation as process_tab_navigation};

pub(crate) mod clone_project;

pub(crate) mod terminal_jobs;

pub(crate) mod speed_search;

pub(crate) mod worktree_removal;

#[cfg(test)]
mod scale_tests;
