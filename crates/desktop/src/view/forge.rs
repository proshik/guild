//! A worktree's merge request, drawn — GLD-063, the approved Penpot boards
//! *GLD-063 — MR glyph*, *Column rows* and *Hover card*.
//!
//! The glyph takes the branch glyph's 14×18 slot: the MR icon in the
//! verdict's colour, and in its corner the badge on a ring of the row's
//! own ground. The hover card is the path to merge, in the popups' card.
use gpui::prelude::*;
use gpui::{Context, IntoElement, Render, Rgba, Window, div, px, rgb};
use guild_session::merge_requests::status::{
    self, Badge, Check, MergeRequest, State, Verdict, badge, verdict,
};

use crate::theme::{self, Theme};

/// The icon's colour for a verdict.
pub(crate) fn verdict_colour(verdict: Verdict, theme: Theme) -> u32 {
    match verdict {
        Verdict::Mergeable => theme.accent.green,
        Verdict::Waiting => theme.accent.brass,
        Verdict::NeedsChange => theme.accent.rust,
        // Two greys: Closed keeps the plain branch glyph's faint, Draft
        // takes secondary (GLD-063, approved).
        Verdict::Draft => theme.text.secondary,
        Verdict::Merged => theme.accent.violet,
        Verdict::Closed => theme.text.faint,
    }
}

/// The MR icon for a state.
pub(crate) fn icon_of(mr: &MergeRequest) -> &'static str {
    match mr.state {
        State::Merged => "ui/mr-merged.svg",
        State::Closed => "ui/mr-closed.svg",
        State::Open if mr.draft => "ui/mr-draft.svg",
        State::Open => "ui/mr-open.svg",
    }
}

/// A badge's glyph and colour: ✓ green, ✗ rust, the rest brass.
pub(crate) fn badge_look(badge: Badge, theme: Theme) -> (&'static str, u32) {
    match badge {
        Badge::Failed => ("ui/badge-failed.svg", theme.accent.rust),
        Badge::Threads => ("ui/badge-threads.svg", theme.accent.brass),
        Badge::Behind => ("ui/badge-behind.svg", theme.accent.brass),
        Badge::Approval => ("ui/badge-approval.svg", theme.accent.brass),
        Badge::Running => ("ui/badge-running.svg", theme.accent.brass),
        Badge::Fine => ("ui/badge-fine.svg", theme.accent.green),
    }
}

/// The MR glyph in the branch glyph's slot. `ring` is the row's ground,
/// so the badge reads as cut out of the icon's corner.
pub(crate) fn glyph(mr: &MergeRequest, theme: Theme, ring: Rgba) -> gpui::Div {
    let colour = verdict_colour(verdict(mr), theme);
    div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size_full()
        .debug_selector(|| "worktree-mr-glyph".into())
        .child(crate::icons::glyph(icon_of(mr), theme::size::META).text_color(rgb(colour)))
        .when_some(badge(mr), |slot, badge| {
            let (path, colour) = badge_look(badge, theme);
            slot.child(
                div()
                    .absolute()
                    .left(px(7.5))
                    .top(px(9.5))
                    .size(px(8.0))
                    .rounded_full()
                    .bg(ring)
                    .flex()
                    .items_center()
                    .justify_center()
                    .debug_selector(move || format!("worktree-mr-badge-{badge:?}"))
                    .child(
                        gpui::svg()
                            .path(path)
                            .size(px(6.0))
                            .flex_none()
                            .text_color(rgb(colour)),
                    ),
            )
        })
}

/// One line of the card, already worded: ✓, ✗ or neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardLine {
    pub(crate) ok: Option<bool>,
    pub(crate) unpushed: bool,
    pub(crate) text: String,
}

/// The hover card's widest, and its width when it says a failure.
const CARD_WIDTH: f32 = 420.0;

/// The hover card: the path to merge.
pub(crate) struct PathCard {
    pub(crate) iid: u64,
    pub(crate) title: String,
    pub(crate) lines: Vec<CardLine>,
    pub(crate) failure: Option<String>,
    pub(crate) theme: Theme,
}

impl PathCard {
    /// The card for an MR, its unpushed commits and the last refresh's
    /// failure, in the person's language.
    pub(crate) fn of(
        mr: &MergeRequest,
        unpushed: Option<u32>,
        failure: Option<String>,
        ui: crate::i18n::Ui,
        theme: Theme,
    ) -> Self {
        let now = now();
        let lines = status::checklist(mr, unpushed)
            .into_iter()
            .map(|line| CardLine {
                ok: line.ok,
                unpushed: matches!(line.check, Check::Unpushed(_)),
                text: ui.mr_check(&line.check, now),
            })
            .collect();
        Self {
            iid: mr.iid,
            title: mr.title.clone(),
            lines,
            failure,
            theme,
        }
    }
}

/// Seconds since the epoch, for an age.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Render for PathCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::ActiveTheme as _;
        let theme = self.theme;
        crate::view::popup::card_look(theme)
            .debug_selector(|| "worktree-mr-card".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(10.0))
            .max_w(px(CARD_WIDTH))
            // A wrapped line needs a width its element has, and a tooltip
            // is laid out at its minimum size: the failure's line, the one
            // that wraps, gets the card's whole width (docs/traps.md, *a line
            // wraps only against a width its element has*).
            .when(self.failure.is_some(), |card| card.w(px(CARD_WIDTH)))
            .font_family(cx.theme().font_family.clone())
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.primary))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.text.secondary))
                            .child(format!("!{}", self.iid)),
                    )
                    .child(div().min_w_0().truncate().child(self.title.clone())),
            )
            .children(self.lines.iter().enumerate().map(|(at, line)| {
                let (icon, colour, text) = match (line.ok, line.unpushed) {
                    (Some(true), _) => (
                        Some("ui/mr-check.svg"),
                        theme.accent.green,
                        theme.text.primary,
                    ),
                    (Some(false), _) => (
                        Some("ui/mr-cross.svg"),
                        theme.accent.rust,
                        theme.text.primary,
                    ),
                    (None, true) => (
                        Some("ui/arrow-up.svg"),
                        theme.text.secondary,
                        theme.text.secondary,
                    ),
                    (None, false) => (None, theme.text.secondary, theme.text.primary),
                };
                div()
                    .debug_selector(move || format!("worktree-mr-card-line-{at}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(rgb(text))
                    .when_some(icon, |line, icon| {
                        line.child(
                            gpui::svg()
                                .path(icon)
                                .size(px(12.0))
                                .flex_none()
                                .text_color(rgb(colour)),
                        )
                    })
                    // A line that names a long branch ends in an ellipsis
                    // inside the card, never past its edge.
                    .child(
                        div()
                            .debug_selector(move || format!("worktree-mr-card-text-{at}"))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(line.text.clone()),
                    )
            }))
            .when_some(self.failure.clone(), |card, failure| {
                card.child(
                    div()
                        .debug_selector(|| "worktree-mr-card-failure".into())
                        .w_full()
                        .min_w_0()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(failure),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use guild_session::merge_requests::status::State;

    /// The card as a tooltip lays it out: placed, with no width given.
    struct Floating(gpui::Entity<PathCard>);

    impl Render for Floating {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().absolute().top_0().left_0().child(self.0.clone()))
        }
    }

    /// A long failure and a long target branch stay inside the card —
    /// docs/traps.md, *a line wraps only against a width its element has*:
    /// with only a `max_w`, a wrapped line's rest was painted below the
    /// card's edge (GLD-063, seen in the window after code review r1).
    #[gpui::test]
    fn every_line_stays_inside_the_card(cx: &mut TestAppContext) {
        let target = "release/2026-11-very-long-target-branch-name-that-does-not-fit-a-card";
        let mr = MergeRequest {
            iid: 311,
            title: "[OC-311] pipeline required".into(),
            web_url: String::new(),
            state: State::Open,
            target_branch: target.into(),
            draft: false,
            conflict: true,
            mergeable: false,
            needs_rebase: true,
            checking: false,
            block: None,
            approvals: None,
            unresolved_threads: Some(0),
            pipeline: Some(status::Pipeline::Missing),
            updated_at: None,
            finished_at: None,
        };
        let failure = "Read 2m ago · the refresh after it failed: glab: 500 Internal Server \
                       Error (HTTP 500) while reading the merge request's discussions"
            .to_owned();
        let (_, cx): (_, &mut VisualTestContext) = cx.add_window_view(|_, cx| {
            gpui_component::init(cx);
            let card = cx.new(|_| {
                PathCard::of(
                    &mr,
                    None,
                    Some(failure),
                    crate::i18n::Ui(guild_session::settings::Language::English),
                    Theme::dark(),
                )
            });
            Floating(card)
        });
        cx.simulate_resize(gpui::size(px(800.), px(600.)));
        cx.run_until_parked();
        let card = cx.debug_bounds("worktree-mr-card").expect("the card");
        let mut lines = vec![
            cx.debug_bounds("worktree-mr-card-failure")
                .expect("failure"),
        ];
        for at in 0.. {
            let name: &'static str =
                Box::leak(format!("worktree-mr-card-text-{at}").into_boxed_str());
            match cx.debug_bounds(name) {
                Some(line) => lines.push(line),
                None => break,
            }
        }
        assert!(lines.len() > 3, "the lines are drawn: {}", lines.len());
        for line in lines {
            assert!(
                line.bottom() <= card.bottom() && line.right() <= card.right(),
                "{line:?} outside {card:?}"
            );
        }
    }
}
