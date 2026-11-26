//! Scoped activity marks; native animation is mounted only for visible work.
use crate::theme;
use gpui::{div, prelude::*, rgb};
use guild_session::status::Status;

mod clock;

#[cfg(test)]
thread_local! {
    /// How many rings were painted turning, in view, on this thread: each
    /// is one that asks the shared clock for its next tick.
    pub(crate) static TURNING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn mark(
    status: Status,
    palette: theme::Theme,
    animate: bool,
    workspace: bool,
) -> gpui::AnyElement {
    if status == Status::Working {
        return ring(rgb(palette.text.secondary), animate);
    }
    if !workspace {
        let asset = match status {
            Status::Waiting(_) => Some("ui/activity-question.svg"),
            Status::Failed => Some("ui/activity-failure.svg"),
            _ => None,
        };
        if let Some(asset) = asset {
            return crate::icons::toolbar(asset)
                .debug_selector(move || asset.into())
                .size(theme::activity::GLYPH)
                .text_color(rgb(palette.accent.rust))
                .into_any_element();
        }
    }
    div()
        .debug_selector(|| "activity-dot".into())
        .size(theme::activity::DOT)
        .rounded_full()
        .bg(rgb(color(status, palette)))
        .when(!workspace && status == Status::Idle, |dot| dot.invisible())
        .into_any_element()
}

/// A state's colour, decision 32's: rust while it waits on the person or
/// failed, green for an unread answer. The dot and a row's secondary counts
/// both paint with it, so a count reads like its mark.
pub(crate) fn color(status: Status, palette: theme::Theme) -> u32 {
    match status {
        Status::Waiting(_) | Status::Failed => palette.accent.rust,
        Status::Done => palette.accent.green,
        _ => palette.text.muted,
    }
}

/// Shared visibility-aware ring, including the review progress colour.
pub(crate) fn ring(color: gpui::Rgba, animate: bool) -> gpui::AnyElement {
    turning(
        "ui/activity-ring.svg",
        theme::activity::RING,
        color,
        animate,
    )
}

/// A glyph that turns while it is seen: the activity ring, a repository's
/// merge requests being checked. One clock and one rule for both — no
/// frame is asked for out of view or under reduced motion.
pub(crate) fn turning(
    asset: &'static str,
    size: impl Into<gpui::Length> + Clone,
    color: gpui::Rgba,
    animate: bool,
) -> gpui::AnyElement {
    // AnimationElement schedules during layout, before scroll clipping is known.
    // Native canvas requests frames only after intersecting the actual content mask.
    gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width <= gpui::px(0.) || visible.size.height <= gpui::px(0.) {
                return;
            }
            let rotating = animate && !cx.reduce_motion();
            static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let angle = if rotating {
                EPOCH
                    .get_or_init(std::time::Instant::now)
                    .elapsed()
                    .as_secs_f64()
                    .rem_euclid(f64::from(theme::activity::ROTATION_SECONDS)) as f32
                    / theme::activity::ROTATION_SECONDS
                    * std::f32::consts::TAU
            } else {
                0.
            };
            let center = bounds.center().scale(window.scale_factor());
            let transform = gpui::TransformationMatrix::unit()
                .translate(center)
                .rotate(gpui::Radians(angle))
                .translate(bounds.center().scale(-window.scale_factor()));
            if let Err(error) =
                window.paint_svg(bounds, asset.into(), None, transform, color.into(), cx)
            {
                tracing::error!("Activity spinner: {error}");
            }
            if rotating {
                #[cfg(test)]
                TURNING.with(|rings| rings.set(rings.get() + 1));
                clock::request(window, cx);
            }
        },
    )
    .size(size)
    .into_any_element()
}

impl crate::Root {
    pub(crate) fn activity_entries(
        &self,
        seat: guild_session::Seat,
    ) -> Vec<crate::view::menu::Entry> {
        use crate::view::menu::{Choice, Deed, Entry, Label, Mark};
        let observed: std::collections::HashMap<_, _> = self
            .open_agents()
            .into_iter()
            .flat_map(|group| group.rows)
            .map(|agent| (agent.session, agent))
            .collect();
        let sessions: Vec<_> = self.desk.sessions_of(seat).collect();
        let activity = self.board.summary(sessions.iter().map(|record| record.id));
        let mut entries = vec![Entry::Heading(Label::Named(
            self.ui().activity_counts("", activity),
        ))];
        for record in sessions {
            let showing = self
                .find_showing(seat, |body| body.session() == Some(record.id))
                .is_some();
            let known = observed.get(&record.id);
            let kind = known.map(|agent| agent.kind).or_else(|| {
                record
                    .runs
                    .harness()
                    .map(crate::agents::Agent::from_harness)
            });
            entries.push(Entry::Choice(Choice {
                label: Label::Named(
                    record
                        .name
                        .as_ref()
                        .or(record.topic.as_ref())
                        .unwrap_or(&record.title)
                        .clone(),
                ),
                mark: kind.map_or(Mark::None, |kind| Mark::Harness(kind.mark())),
                deed: Deed::ShowSession(record.id),
                check: None,
                enabled: showing,
                detail: Some(Label::Named(
                    self.ui()
                        .activity_state(self.board.status(record.id))
                        .into(),
                )),
                trailing: None,
            }));
        }
        if entries.len() == 1 {
            entries.push(Entry::Note(Label::Text(crate::i18n::Text::AgentsNone)));
        }
        entries
    }

    pub(crate) fn open_activity(
        &mut self,
        seat: guild_session::Seat,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let return_focus = window.focused(cx);
        self.open_menu(
            crate::view::menu::Target::WorktreeActivity { seat, return_focus },
            crate::view::menu::Anchor::Pointer(at),
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::menu::{Deed, Entry, Mark};
    use gpui::{AppContext, TestAppContext, px};
    use guild_session::status::Why;
    use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

    pub(super) struct MarkPreview;

    impl gpui::Render for MarkPreview {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().flex().items_start().children(
                [
                    ("activity-size-ring", Status::Working),
                    ("activity-size-question", Status::Waiting(Why::Question)),
                    ("activity-size-failure", Status::Failed),
                    ("activity-size-dot", Status::Done),
                ]
                .into_iter()
                .map(|(selector, status)| {
                    div()
                        .flex()
                        .flex_none()
                        .debug_selector(move || selector.into())
                        .child(mark(status, theme::Theme::dark(), false, false))
                }),
            )
        }
    }

    #[gpui::test]
    fn activity_mark_sizes_keep_the_small_increase(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let preview = cx.new(|_| MarkPreview);
            gpui_component::Root::new(preview, window, cx)
        });
        cx.run_until_parked();
        for (selector, points) in [
            ("activity-size-ring", 14.),
            ("activity-size-question", 16.),
            ("activity-size-failure", 16.),
            ("activity-size-dot", 8.),
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert_eq!(
                bounds.size,
                gpui::size(px(points), px(points)),
                "{selector}"
            );
        }
    }

    #[gpui::test]
    fn activity_resolves_hidden_panes_and_keeps_workspace_scopes_independent(
        cx: &mut TestAppContext,
    ) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::activity_resolves_hidden_panes_and_keeps_workspace_scopes_independent"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let other = desk.add_workspace("Other", None).unwrap();
            let project = desk.open_in(workspace, dir.path());
            desk.open_in(other, dir.path());
            let seat = Seat { workspace, project };
            let other_seat = Seat {
                workspace: other,
                project,
            };
            let mut ids = Vec::new();
            for (seat, runs) in [
                (seat, Runs::Claude(SessionMode::Terminal)),
                (seat, Runs::Codex(SessionMode::Terminal)),
                (seat, Runs::OpenCode(SessionMode::Terminal)),
                (other_seat, Runs::Codex(SessionMode::Terminal)),
            ] {
                ids.push(desk.add_session(SessionRecord::new(
                    format!("Agent {}", ids.len()),
                    seat,
                    dir.path().into(),
                    runs,
                )));
            }
            let root = cx.new(|cx| {
                let mut root = crate::Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_project = Some(project);
                root.active_workspace = workspace;
                let bodies: Vec<_> = ids
                    .iter()
                    .map(|id| crate::Body::Broken {
                        session: *id,
                        reason: "fixture".into(),
                    })
                    .collect();
                let mut bodies = bodies.into_iter();
                let mut panes = guild_session::Panes::one(bodies.next().unwrap());
                let first = panes.focused();
                let second = panes
                    .split(first, guild_session::layout::Axis::Horizontal)
                    .unwrap();
                assert!(panes.fill(second, bodies.next().unwrap()).is_ok());
                panes.focus(first);
                let tab = root.adopt_panes(seat, project, panes, cx);
                root.adopt(seat, project, bodies.next().unwrap(), cx);
                root.adopt(other_seat, project, bodies.next().unwrap(), cx);
                root.active_tab = Some(tab);
                root.board
                    .report(ids[0], Some(guild_session::status::Reported::Done));
                root.board.report(
                    ids[1],
                    Some(guild_session::status::Reported::Blocked(Some(
                        guild_session::status::Kind::Question,
                    ))),
                );
                root.board
                    .report(ids[2], Some(guild_session::status::Reported::Done));
                root.board
                    .report(ids[3], Some(guild_session::status::Reported::Working));
                root
            });
            root.update(cx, |root, cx| {
                let a = root.workspace_activity(workspace);
                assert_eq!((a.waiting, a.working, a.unread), (1, 0, 2));
                assert_eq!(root.workspace_activity(other).working, 1);
                assert!(
                    root.worktree_busy(project),
                    "another workspace still owns working work"
                );
                // No Waiting: a current Done/Failed must not conceal another workspace's work.
                root.board
                    .report(ids[1], Some(guild_session::status::Reported::Error));
                assert!(root.worktree_busy(project));
                root.board
                    .report(ids[3], Some(guild_session::status::Reported::Done));
                assert!(!root.worktree_busy(project));
                root.board
                    .report(ids[3], Some(guild_session::status::Reported::Working));
                root.board.report(
                    ids[1],
                    Some(guild_session::status::Reported::Blocked(Some(
                        guild_session::status::Kind::Question,
                    ))),
                );
                let entries = root.activity_entries(seat);
                let choices: Vec<_> = entries
                    .iter()
                    .filter_map(|entry| {
                        if let Entry::Choice(choice) = entry {
                            Some(choice)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(choices.len(), 3);
                assert!(
                    choices
                        .iter()
                        .all(|choice| matches!(choice.mark, Mark::Harness(_)))
                );
                assert!(
                    choices
                        .iter()
                        .any(|choice| choice.deed == Deed::ShowSession(ids[1]) && choice.enabled)
                );
                assert!(root.go_to_session_in_workspace(ids[1], cx));
                assert_eq!(root.active_session(), Some(ids[1]));
                assert_eq!(root.board.status(ids[1]), Status::Waiting(Why::Question));
                root.window_focused = true;
                root.poll_status(cx);
                assert_eq!(
                    root.board.status(ids[0]),
                    Status::Idle,
                    "visible other pane is seen"
                );
                assert_eq!(
                    root.board.status(ids[2]),
                    Status::Done,
                    "hidden result is unread"
                );
                root.tabs.retain(|tab| tab.seat.workspace != other);
                root.restored_workspaces.remove(&other);
                assert!(
                    root.go_to_session_in_workspace(ids[3], cx),
                    "an unvisited workspace restores before resolving the exact session"
                );
                root.select_workspace(other, cx);
                assert_eq!(
                    root.board.status(ids[2]),
                    Status::Done,
                    "workspace selection does not bulk read results"
                );
                let before = root.active_tab;
                root.desk.remove_session(ids[1]);
                assert!(!root.go_to_session_in_workspace(ids[1], cx));
                assert_eq!(
                    root.active_tab, before,
                    "missing target cannot select another tab"
                );
            });
        });
    }
}

#[cfg(test)]
mod clipping_tests {
    use super::*;
    use gpui::{TestAppContext, px};
    struct ClipPreview {
        offset: f32,
        animate: bool,
        renders: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl gpui::Render for ClipPreview {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div()
                .id("clip")
                .flex()
                .flex_col()
                .w(px(30.))
                .h(px(20.))
                .overflow_hidden()
                .child(div().flex_none().h(px(self.offset)))
                .child(
                    div()
                        .flex_none()
                        .debug_selector(|| "clipped-spinner".into())
                        .child(mark(
                            Status::Working,
                            theme::Theme::dark(),
                            self.animate,
                            false,
                        )),
                )
        }
    }
    #[gpui::test]
    fn activity_spinner_requests_frames_only_inside_actual_clip(cx: &mut TestAppContext) {
        let mut preview = None;
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let view = cx.new(|_| ClipPreview {
                offset: 40.,
                animate: true,
                renders: renders.clone(),
            });
            preview = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        cx.run_until_parked();
        let preview = preview.unwrap();
        for (offset, animate, expected) in [
            (40., true, 0),
            (0., true, 1),
            (40., true, 0),
            (0., false, 0),
        ] {
            preview.update(cx, |view, cx| {
                view.offset = offset;
                view.animate = animate;
                cx.notify();
            });
            cx.run_until_parked();
            // A previously visible mark may have one delayed tick pending.
            // Settle it, then check that hidden marks schedule no successor.
            budget_tests::redraws_in_tick(cx, &renders);
            let frames = budget_tests::redraws_in_tick(cx, &renders);
            assert_eq!(
                frames, expected,
                "actual clip: offset={offset} animate={animate}"
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod budget_tests;
