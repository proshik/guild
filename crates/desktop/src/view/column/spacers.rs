//! The column's list draws only what is near its viewport: an item far out
//! of it is a spacer of its height — measured in the last frame.
//!
//! **Why.** Scrolling a `div` notifies the view it is drawn in, and GPUI
//! marks every view above it dirty, so a frame of scrolling was the whole
//! list built, laid out by taffy and painted again — linear in every row,
//! not in the rows on screen. On 2026-11-23 with forty repositories (two
//! hundred rows) a scroll frame was ~110 ms on the test platform and a
//! notify of the root ~56 ms, three quarters of it taffy's flexbox.
//!
//! **Spacers, not GPUI's `list()`.** An item stays a child of the scroller
//! at its own index, so `ScrollHandle::bounds_for_item` and
//! `scroll_to_top_of_item` keep meaning what the speed search and the drags
//! count along (`sidebar_speed_rows`), and the list's height —
//! `max_offset` — is what it would be with every item drawn.
//!
//! **A height is only trusted while nothing that decides it changed.** Each
//! item carries a [`Shape`]: what its height depends on, and nothing that
//! only colours it. The heights come from the scroll handle, which holds
//! the last frame's bounds *by index*; they are used only for the run of
//! items whose shapes are the same, at the same index, as the last frame's.
//! From the first item that differs — one added, removed, folded, given a
//! tab — everything is drawn, because every position after it may have
//! moved. A [`Frame`] that changed — the column's width, the interface's
//! size, font or language — draws everything too, and so does a list never
//! painted.
//!
//! **And the frame checks itself.** After the list's prepaint, a measure
//! beside it ([`Spacers::verify`]) looks at what was laid out: a spacer the
//! final scroll offset brought into view — a jump further than the margin
//! in one frame — and a drawn item whose height moved while its shape said
//! it could not each ask for one more frame, drawn whole. The second is a
//! [`Shape`] missing a factor; a test build panics on it.
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{Bounds, Pixels, ScrollHandle, Styled as _, px};
use guild_session::WorkspaceId;

use crate::Root;
use crate::column::{Item, Row};

#[cfg(test)]
thread_local! {
    /// Frames painted with a spacer in view — a blank stretch of the list
    /// a person would see for one frame — on this thread.
    pub(crate) static BLANK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What decides the height of one item of the column's list — and only
/// that: a status ring, a hover, a lit row or a diff's numbers do not move
/// anything, and a change in them must not cost a full list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A project group's header, with its folder's row when it has one.
    Head {
        id: guild_session::project_group::ProjectGroupId,
        collapsed: bool,
        folder: Option<RowShape>,
    },
    /// A repository: its head, and its worktrees unless it is folded.
    Repository {
        key: std::path::PathBuf,
        grouped: bool,
        last: bool,
        folded: bool,
        /// Its name being typed: never a spacer, the field holds the keys.
        renaming: bool,
        rows: Vec<RowShape>,
    },
}

impl Shape {
    /// The worktree rows the item draws.
    fn rows(&self) -> impl Iterator<Item = &RowShape> {
        match self {
            Shape::Head { folder, .. } => folder.as_slice().iter(),
            Shape::Repository { rows, .. } => rows.as_slice().iter(),
        }
    }

    /// An item that must be drawn wherever it is.
    fn pinned(&self) -> bool {
        matches!(self, Shape::Repository { renaming: true, .. })
    }
}

/// One worktree row as far as height goes: which row, and how many tab
/// rows are listed under it — `None` while the list is collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowShape {
    pub key: String,
    tabs: Option<usize>,
    /// Its folder and its opened root, what a hover is kept by.
    pub path: Option<std::path::PathBuf>,
    pub project: Option<guild_session::ProjectId>,
}

/// How many tabs each seat's row lists — `Root::column_tab_counts`.
pub(crate) type TabCounts = HashMap<guild_session::Seat, usize>;

/// What every item's height depends on at once — and the window's height,
/// which the margin is measured against: a window grown past it shows rows
/// the last frame never planned for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub width: f32,
    pub font_size: f32,
    pub font: Option<String>,
    pub code_font: gpui::SharedString,
    pub language: guild_session::Language,
    /// The display's backing scale, which layout snaps to: a fractional
    /// line is a different height at another one.
    pub scale: f32,
    pub window_height: f32,
}

/// What a frame's list looked like, kept for the next one.
struct Page {
    frame: Frame,
    shapes: Vec<Shape>,
    /// Which of them were spacers.
    spaced: Vec<bool>,
    /// A `scroll_to_top_of_item` asked for since: the offset the list's
    /// prepaint is about to take, so the item and what follows it are
    /// drawn in the very frame they come into view.
    reveal: Option<usize>,
}

/// The column's spacers, by workspace page. A clone shares them: the
/// check after a prepaint holds one.
#[derive(Default, Clone)]
pub(crate) struct Spacers {
    pages: Rc<RefCell<HashMap<WorkspaceId, Page>>>,
}

/// What one frame decided: an item's spacer height, or `None` to draw it.
pub(crate) struct Plan {
    pub heights: Vec<Option<Pixels>>,
    /// The heights the last frame laid out the leading items that kept
    /// their shape at: the ones trusted, and so the ones
    /// [`Spacers::verify`] checks.
    previous: Vec<Pixels>,
    /// The items drawn last frame and spacers in this one: rows that never
    /// hear the pointer leave them.
    discarded: Vec<usize>,
    /// Those items' rows — what a hover or a tooltip they owned is cleared
    /// by.
    pub discarded_rows: Vec<RowShape>,
}

#[cfg(test)]
impl Plan {
    /// Whether any item is a spacer this frame.
    pub fn any(&self) -> bool {
        self.heights.iter().any(Option::is_some)
    }
}

/// The viewport and the items as the last frame laid them out.
pub(crate) struct Laid<'a> {
    /// The scroller's box, in window coordinates.
    pub view: Bounds<Pixels>,
    /// The scroll offset now: negative as the list scrolls down.
    pub offset: Pixels,
    pub max_offset: Pixels,
    /// Each item's bounds before the offset, by index.
    pub bounds: &'a dyn Fn(usize) -> Option<Bounds<Pixels>>,
}

/// Which items to draw as spacers — pure, for the rules above.
///
/// `previous` is the last frame's frame, shapes and spacers; `None` for a
/// list never drawn. The margin is one viewport above and one below the
/// tallest the window lets the viewport grow to, past every offset the
/// prepaint can settle at.
pub(crate) fn plan(
    previous: Option<(&Frame, &[Shape], &[bool])>,
    frame: &Frame,
    shapes: &[Shape],
    laid: &Laid<'_>,
    reveal: Option<usize>,
) -> Plan {
    let all = || Plan {
        heights: vec![None; shapes.len()],
        previous: Vec::new(),
        discarded: Vec::new(),
        discarded_rows: Vec::new(),
    };
    let Some((last_frame, last_shapes, last_spaced)) = previous else {
        return all();
    };
    let height = laid.view.size.height;
    if last_frame != frame || height <= px(0.) {
        return all();
    }
    let stable = shapes
        .iter()
        .zip(last_shapes)
        .take_while(|(now, then)| now == then)
        .count();
    let previous: Vec<Pixels> = (0..stable)
        .map_while(|index| (laid.bounds)(index).map(|bounds| bounds.size.height))
        .collect();
    let stable = previous.len();
    // A reveal into what changed has no last bounds to aim by: the list's
    // destination cannot be bounded, so everything is drawn.
    if reveal.is_some_and(|index| index >= stable) {
        return all();
    }
    // **Every offset the prepaint can leave the list at, every viewport it
    // can lay it out in.** The plan reads last frame's offset and bounds;
    // in this frame's prepaint a reveal moves the offset (`Top` over the
    // kept bounds), GPUI clamps it into the list — a wheel that overshot
    // either end, a list that became shorter — and the viewport itself
    // grows when what is above it goes, though never past the window.
    // The plan draws whatever any of those can bring into view.
    let start = reveal
        .and_then(|index| (laid.bounds)(index))
        .map_or(laid.offset, |bounds| laid.view.top() - bounds.top())
        .min(px(0.));
    let tallest = height.max(px(frame.window_height));
    // The least the list's end can reach: the kept items' if what follows
    // them changed, else where it was.
    let changed = stable < shapes.len() || stable < last_shapes.len();
    let end = if changed {
        stable
            .checked_sub(1)
            .and_then(|last| (laid.bounds)(last))
            .map_or(laid.view.top(), |last| last.bottom())
    } else {
        laid.view.bottom() + laid.max_offset
    };
    let least_max = (end - laid.view.top() - tallest).max(px(0.));
    // The most the list can come down (a clamp to a shorter end) and the
    // most it can go up (where it was asked to be).
    let down = start.max(-least_max);
    let up = start;
    let above = laid.view.top() - height;
    let below = laid.view.top() + tallest + height;
    let heights = shapes
        .iter()
        .enumerate()
        .map(|(index, shape)| {
            if index >= stable || shape.pinned() {
                return None;
            }
            let bounds = (laid.bounds)(index)?;
            (bounds.bottom() + down < above || bounds.top() + up > below)
                .then_some(bounds.size.height)
        })
        .collect::<Vec<_>>();
    let discarded = heights
        .iter()
        .enumerate()
        .filter(|(index, spacer)| {
            spacer.is_some() && !last_spaced.get(*index).copied().unwrap_or(false)
        })
        .map(|(index, _)| index)
        .collect();
    Plan {
        heights,
        previous,
        discarded,
        discarded_rows: Vec::new(),
    }
}

impl Spacers {
    /// Forget the pages of workspaces that are gone.
    pub fn retain(&self, workspaces: &[guild_session::Workspace]) {
        self.pages
            .borrow_mut()
            .retain(|id, _| workspaces.iter().any(|w| w.id == *id));
    }

    /// The scroll handle is about to bring `index` to the top: draw where
    /// it lands rather than where it is.
    pub fn reveal(&self, workspace: WorkspaceId, index: usize) {
        if let Some(page) = self.pages.borrow_mut().get_mut(&workspace) {
            page.reveal = Some(index);
        }
    }

    /// This frame's spacers for one page, kept as the next frame's past.
    pub fn plan(
        &self,
        workspace: WorkspaceId,
        handle: &ScrollHandle,
        frame: Frame,
        shapes: Vec<Shape>,
    ) -> Plan {
        let mut pages = self.pages.borrow_mut();
        let page = pages.remove(&workspace);
        let bounds = |index: usize| handle.bounds_for_item(index);
        let laid = Laid {
            view: handle.bounds(),
            offset: handle.offset().y,
            max_offset: handle.max_offset().y,
            bounds: &bounds,
        };
        let mut plan = plan(
            page.as_ref()
                .map(|page| (&page.frame, page.shapes.as_slice(), page.spaced.as_slice())),
            &frame,
            &shapes,
            &laid,
            page.as_ref().and_then(|page| page.reveal),
        );
        plan.discarded_rows = plan
            .discarded
            .iter()
            .flat_map(|index| shapes[*index].rows().cloned())
            .collect();
        pages.insert(
            workspace,
            Page {
                frame,
                shapes,
                spaced: plan.heights.iter().map(Option::is_some).collect(),
                reveal: None,
            },
        );
        plan
    }

    /// Drop every page's past: the next frames draw everything.
    #[cfg(test)]
    pub fn clear(&self) {
        self.pages.borrow_mut().clear();
    }

    /// Drop a page's past, so that its next frame draws everything.
    fn forget(&self, workspace: WorkspaceId) {
        self.pages.borrow_mut().remove(&workspace);
    }

    /// The check after the list's prepaint, as a measure beside it: a
    /// spacer in view, or a height that moved under an unchanged shape,
    /// asks for another frame — deferred, since a notify inside a draw is
    /// one the same draw consumes.
    pub fn verify(
        &self,
        workspace: WorkspaceId,
        handle: ScrollHandle,
        plan: Plan,
    ) -> impl gpui::IntoElement {
        let this = self.clone();
        gpui::canvas(
            move |_, window, cx| {
                let view = handle.bounds();
                let offset = handle.offset().y;
                let seen = plan.heights.iter().enumerate().any(|(index, spacer)| {
                    spacer.is_some()
                        && handle.bounds_for_item(index).is_some_and(|bounds| {
                            bounds.bottom() + offset > view.top()
                                && bounds.top() + offset < view.bottom()
                        })
                });
                let moved = plan.previous.iter().enumerate().find(|(index, height)| {
                    plan.heights[*index].is_none()
                        && handle
                            .bounds_for_item(*index)
                            .is_some_and(|now| (now.size.height - **height).abs() > px(0.5))
                });
                if let Some((index, height)) = moved {
                    let now = handle.bounds_for_item(index).map(|b| b.size.height);
                    // A shape that leaves out what moved this item: its
                    // spacer would have been the wrong height.
                    if cfg!(test) {
                        panic!(
                            "column item {index} moved from {height:?} to {now:?} under one shape"
                        );
                    }
                    tracing::warn!(index, ?height, ?now, "a column item moved under one shape");
                    this.forget(workspace);
                }
                #[cfg(test)]
                if seen {
                    BLANK.with(|frames| frames.set(frames.get() + 1));
                }
                if seen || moved.is_some() {
                    let view = window.current_view();
                    cx.defer(move |cx| cx.notify(view));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
    }
}

impl Root {
    /// The rows a repository draws under its head: all but those pinned
    /// above the list, and a main checkout always.
    pub(super) fn column_rows_shown<'a>(
        &'a self,
        workspace: WorkspaceId,
        group: &'a crate::column::Group,
    ) -> impl Iterator<Item = &'a Row> + 'a {
        let pinned = self
            .desk
            .workspace(workspace)
            .map(|saved| saved.pinned_worktrees.as_slice())
            .unwrap_or_default();
        group.rows.iter().filter(move |row| {
            let title = match row {
                Row::Opened { title, .. } | Row::Unopened { title, .. } => title,
            };
            matches!(title.name.as_str(), "main" | "master")
                || !self
                    .column_row_path(row)
                    .is_some_and(|path| pinned.contains(&path))
        })
    }

    /// Everything the column's items are measured under.
    pub(super) fn column_frame(&self) -> Frame {
        Frame {
            width: self.settings.column_width,
            font_size: self.settings.ui_font_size,
            font: self.settings.ui_font.clone(),
            code_font: self.code_font.clone(),
            language: self.settings.language,
            scale: self.scale_factor,
            window_height: self.viewport.height.into(),
        }
    }

    fn row_shape(&self, workspace: WorkspaceId, row: &Row, counts: &TabCounts) -> RowShape {
        let folded = self
            .column_row_path(row)
            .is_none_or(|path| self.desk.worktree_tabs_folded(workspace, &path));
        // An unopened worktree has no tabs; an opened one lists what
        // `column_tabs` would, counted once for every row — `column_tab_counts`.
        let listed = match row {
            Row::Opened { project, .. } => counts
                .get(&guild_session::Seat {
                    workspace,
                    project: *project,
                })
                .copied()
                .unwrap_or(0),
            Row::Unopened { .. } => 0,
        };
        RowShape {
            key: crate::column::row_key(workspace, row),
            tabs: (!folded).then_some(listed),
            path: self.column_row_path(row),
            project: match row {
                Row::Opened { project, .. } => Some(*project),
                Row::Unopened { .. } => None,
            },
        }
    }

    /// One item's [`Shape`], from what its drawing reads.
    pub(super) fn column_item_shape(
        &self,
        workspace: WorkspaceId,
        item: &Item,
        last: bool,
        counts: &TabCounts,
    ) -> Shape {
        match item {
            Item::Head(head) => Shape::Head {
                id: head.id,
                collapsed: head.collapsed,
                folder: self
                    .group_folder_row(workspace, head)
                    .map(|row| self.row_shape(workspace, &row, counts)),
            },
            Item::Repository { group, section } => {
                let folded = self.desk.is_folded(&group.key);
                Shape::Repository {
                    key: group.key.clone(),
                    grouped: section.is_some(),
                    last,
                    folded,
                    renaming: self
                        .renaming_project()
                        .is_some_and(|(key, _)| key == group.key.as_path()),
                    rows: if folded {
                        Vec::new()
                    } else {
                        self.column_rows_shown(workspace, group)
                            .map(|row| self.row_shape(workspace, row, counts))
                            .collect()
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    fn frame() -> Frame {
        Frame {
            width: 232.,
            font_size: 13.,
            font: None,
            code_font: "Mono".into(),
            language: Default::default(),
            scale: 2.,
            window_height: 400.,
        }
    }

    fn shape(name: &str) -> Shape {
        Shape::Repository {
            key: name.into(),
            grouped: false,
            last: false,
            folded: false,
            renaming: false,
            rows: vec![RowShape {
                key: name.into(),
                tabs: None,
                path: None,
                project: None,
            }],
        }
    }

    /// Forty items of 100 points, 12 apart, in a 400-point viewport at the
    /// window's top, scrolled down `scrolled` points.
    fn laid(scrolled: f32, run: impl FnOnce(&Laid<'_>)) {
        let bounds = |index: usize| {
            (index < 40).then(|| {
                Bounds::new(
                    point(px(0.), px(index as f32 * 112.)),
                    size(px(200.), px(100.)),
                )
            })
        };
        run(&Laid {
            view: Bounds::new(point(px(0.), px(0.)), size(px(200.), px(400.))),
            offset: px(-scrolled),
            max_offset: px(40. * 112. - 12. - 400.),
            bounds: &bounds,
        });
    }

    fn shapes() -> Vec<Shape> {
        (0..40).map(|i| shape(&format!("r{i}"))).collect()
    }

    fn drawn(plan: &Plan) -> Vec<usize> {
        (0..plan.heights.len())
            .filter(|index| plan.heights[*index].is_none())
            .collect()
    }

    #[test]
    fn a_list_never_drawn_draws_everything() {
        laid(0., |laid| {
            let plan = plan(None, &frame(), &shapes(), laid, None);
            assert!(!plan.any());
        });
    }

    #[test]
    fn items_beyond_a_viewport_of_margin_are_spacers_of_their_height() {
        let shapes = shapes();
        laid(1000., |laid| {
            let plan = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                None,
            );
            // Visible: 1000..1400; with a viewport above and below, 600..1800.
            assert_eq!(drawn(&plan), (5..=16).collect::<Vec<_>>());
            assert_eq!(plan.heights[0], Some(px(100.)));
            assert_eq!(plan.heights[39], Some(px(100.)));
        });
    }

    #[test]
    fn everything_from_the_first_changed_shape_is_drawn() {
        let before = shapes();
        let mut after = before.clone();
        after[20] = shape("another");
        laid(0., |laid| {
            let plan = plan(Some((&frame(), &before, &[])), &frame(), &after, laid, None);
            let drawn = drawn(&plan);
            // The viewport, 0..400, and its margin below to 800.
            assert_eq!(drawn[..8], (0..8).collect::<Vec<_>>()[..]);
            assert_eq!(
                drawn[8..],
                (20..40).collect::<Vec<_>>()[..],
                "and all after"
            );
        });
        let mut fewer = before.clone();
        fewer.remove(10);
        laid(0., |laid| {
            let plan = plan(Some((&frame(), &before, &[])), &frame(), &fewer, laid, None);
            assert!(plan.heights[10..].iter().all(Option::is_none));
        });
    }

    #[test]
    fn another_width_font_or_language_draws_everything() {
        let shapes = shapes();
        for changed in [
            Frame {
                width: 300.,
                ..frame()
            },
            Frame {
                font_size: 15.,
                ..frame()
            },
            Frame {
                font: Some("Inter".into()),
                ..frame()
            },
            Frame {
                scale: 1.,
                ..frame()
            },
            Frame {
                window_height: 1300.,
                ..frame()
            },
        ] {
            laid(2000., |laid| {
                let plan = plan(
                    Some((&frame(), &shapes, &[])),
                    &changed,
                    &shapes,
                    laid,
                    None,
                );
                assert!(!plan.any(), "{changed:?}");
            });
        }
    }

    #[test]
    fn a_renamed_item_is_drawn_wherever_it_is() {
        let mut shapes = shapes();
        if let Shape::Repository { renaming, .. } = &mut shapes[30] {
            *renaming = true;
        }
        laid(0., |laid| {
            let plan = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                None,
            );
            assert!(plan.heights[30].is_none());
            assert!(plan.heights[29].is_some());
        });
    }

    #[test]
    fn a_reveal_draws_where_the_list_lands() {
        let shapes = shapes();
        laid(0., |laid| {
            let plan = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                Some(30),
            );
            // Item 30 at the top: 3360..3760, margin 2960..4160.
            let drawn = drawn(&plan);
            assert!(drawn.contains(&30) && drawn.contains(&33) && drawn.contains(&27));
            assert!(!drawn.contains(&0), "the old viewport is left behind");
        });
    }

    #[test]
    fn a_shorter_list_draws_what_its_clamped_offset_brings_down() {
        // Scrolled to 3000 in a list of forty; the last ten change and may
        // be shorter: the first thirty end at 3348, so the offset may come
        // back to 2948 and bring 52 more points above the viewport.
        let before = shapes();
        let mut after = before.clone();
        after.truncate(30);
        laid(3000., |laid| {
            let kept = plan(
                Some((&frame(), &before, &[])),
                &frame(),
                &before,
                laid,
                None,
            );
            let shorter = plan(Some((&frame(), &before, &[])), &frame(), &after, laid, None);
            let first = |plan: &Plan| drawn(plan)[0];
            assert!(
                first(&shorter) < first(&kept),
                "{} {}",
                first(&shorter),
                first(&kept)
            );
            // Drawn from a bottom at 3000 - 400 - 52 = 2548: item 22's.
            assert_eq!(first(&shorter), 22);
        });
    }

    #[test]
    fn an_item_newly_spaced_is_discarded_once() {
        let shapes = shapes();
        laid(1000., |laid| {
            let first = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                None,
            );
            assert!(!first.discarded.is_empty(), "drawn before, spaced now");
            let spaced: Vec<bool> = first.heights.iter().map(Option::is_some).collect();
            let again = plan(
                Some((&frame(), &shapes, &spaced)),
                &frame(),
                &shapes,
                laid,
                None,
            );
            assert!(again.discarded.is_empty(), "already a spacer");
        });
    }

    #[test]
    fn a_wheel_past_either_end_draws_where_the_clamp_puts_the_list() {
        let shapes = shapes();
        // Past the top: the list stays at its top.
        laid(-8000., |laid| {
            let plan = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                None,
            );
            assert_eq!(drawn(&plan)[0], 0);
        });
        // Past the end: the list ends at the viewport's bottom, 4068.
        laid(60000., |laid| {
            let plan = plan(
                Some((&frame(), &shapes, &[])),
                &frame(),
                &shapes,
                laid,
                None,
            );
            assert!(drawn(&plan).contains(&39) && drawn(&plan).contains(&36));
        });
    }

    #[test]
    fn a_viewport_that_may_grow_to_the_window_is_drawn_to_its_bottom() {
        let shapes = shapes();
        let tall = Frame {
            window_height: 1000.,
            ..frame()
        };
        laid(0., |laid| {
            let plan = plan(Some((&tall, &shapes, &[])), &tall, &shapes, laid, None);
            // A viewport of up to 1000, and a 400-point margin below it.
            assert_eq!(*drawn(&plan).last().unwrap(), 12);
        });
    }

    #[test]
    fn a_reveal_into_what_changed_draws_everything() {
        let before = shapes();
        let mut after = before.clone();
        after[35] = shape("opened");
        laid(0., |laid| {
            let plan = plan(
                Some((&frame(), &before, &[])),
                &frame(),
                &after,
                laid,
                Some(36),
            );
            assert!(!plan.any());
        });
    }
}
