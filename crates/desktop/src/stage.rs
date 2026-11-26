//! What is in front of each seat: its areas, the one with the keys, and
//! the tab in front of each — M12.8, decisions 7, 11 and 15. Pure, in
//! `fronts.rs`'s manner: the window says what it has, this answers.
//!
//! **The window's one list of tabs stays the order.** A tab names its
//! area; an area's strip is the tabs that name it, in the list's order
//! (`strip`), so every walk of `Root.tabs` that was right before is right
//! now, and a drag is a permutation of that one list (`reorder`).

use std::collections::HashMap;
use std::hash::Hash;

use guild_session::layout::{Axis, Direction, Frame, toward};
use guild_session::{AreaId, Areas, LastArea, PaneId};

use crate::fronts::Fronts;

/// Each seat's areas, and the tab in front of each area.
#[derive(Debug)]
pub struct Stage<S, T> {
    seats: HashMap<S, Areas>,
    fronts: Fronts<AreaId, T>,
    /// The front an area had before a tab was carried in over it, and
    /// that tab — while it is still the area's front. A live drag
    /// carries the tab into every area it passes over (`carry`); the
    /// area it merely passed through gets its own front back when the
    /// tab goes on (the final review's Minor 2).
    lent: HashMap<AreaId, (T, Option<T>)>,
}

impl<S, T> Default for Stage<S, T> {
    fn default() -> Self {
        Self {
            seats: HashMap::new(),
            fronts: Fronts::default(),
            lent: HashMap::new(),
        }
    }
}

impl<S: Hash + Eq + Copy, T: Copy + Eq> Stage<S, T> {
    /// `seat`'s areas, when it has been divided or asked about.
    pub fn areas(&self, seat: S) -> Option<&Areas> {
        self.seats.get(&seat)
    }

    /// `seat`'s areas, one made on the first ask.
    pub fn areas_mut(&mut self, seat: S) -> &mut Areas {
        self.seats.entry(seat).or_insert_with(Areas::one)
    }

    /// Put `seat`'s areas as a restore read them.
    pub fn set_areas(&mut self, seat: S, areas: Areas) {
        self.seats.insert(seat, areas);
    }

    /// The area with the keys, as the stage last heard.
    pub fn focused(&self, seat: S) -> Option<AreaId> {
        self.seats.get(&seat).map(Areas::focused)
    }

    /// The area with the keys, read from the tab in front when that tab is
    /// this seat's — decision 7 — and from the stage when there is none.
    pub fn focused_with(&self, seat: S, front: Option<(S, AreaId)>) -> Option<AreaId> {
        match front {
            Some((its, area)) if its == seat => Some(area),
            _ => self.focused(seat),
        }
    }

    /// Where a new tab of `seat` goes — decision 15.
    pub fn landing(&mut self, seat: S) -> AreaId {
        self.areas_mut(seat).focused()
    }

    /// Give `area` the keys — an arrow reaching an empty area.
    pub fn focus(&mut self, seat: S, area: AreaId) -> bool {
        self.areas_mut(seat).focus(area)
    }

    /// `tab` is in front of `area`, and `area` has the keys.
    pub fn bring_forward(&mut self, seat: S, area: AreaId, tab: T) {
        self.areas_mut(seat).focus(area);
        self.fronts.remember(area, tab);
        // Another tab chosen there is the area's own front now.
        if self.lent.get(&area).is_some_and(|(lent, _)| *lent != tab) {
            self.lent.remove(&area);
        }
    }

    /// The tab that comes forward in `area`: the one left there, else the
    /// strip's last — `Root::come_back`'s rule (traps.md, 2026-09-26).
    pub fn front(&self, area: AreaId, strip: &[T]) -> Option<T> {
        self.fronts.returning(area, strip)
    }

    /// The tab remembered in front of `area`, if it is still on `strip` —
    /// what `store_tabs` writes as the strip's `active`.
    pub fn remembered(&self, area: AreaId, strip: &[T]) -> Option<T> {
        self.fronts.of(area, strip)
    }

    /// `closing` leaves `area`'s `strip`, read before it goes.
    pub fn closing(&mut self, area: AreaId, strip: &[T], closing: T) {
        self.fronts.close(area, strip, closing);
    }

    /// `tab` leaves `from` — whose strip, read before it went, is
    /// `strip` — for `to`, where it comes forward with the keys: a tab
    /// moved to another area (decision 8). The area it left brings its
    /// neighbour forward when the tab was its front, as a close would —
    /// **unless the tab was only carried in over it**, and nothing else
    /// was brought forward there since: then the area gets back the front
    /// it had, so a drag from A to C across B leaves B as it was.
    pub fn carry(&mut self, seat: S, tab: T, from: AreaId, strip: &[T], to: AreaId) {
        let had = match self.lent.remove(&from) {
            Some((lent, had)) if lent == tab => {
                had.filter(|had| *had != tab && strip.contains(had))
            }
            _ => None,
        };
        match had {
            Some(had) => self.fronts.remember(from, had),
            None => self.closing(from, strip, tab),
        }
        let had = self.fronts.held(to);
        self.bring_forward(seat, to, tab);
        self.lent.insert(to, (tab, had));
    }

    /// Divide `area`; the new one has the keys.
    pub fn split(&mut self, seat: S, area: AreaId, axis: Axis) -> Option<AreaId> {
        self.areas_mut(seat).split(area, axis)
    }

    /// Close `area`, answering the heir its tabs go to (decision 9).
    pub fn close_area(&mut self, seat: S, area: AreaId) -> Result<AreaId, LastArea> {
        let heir = self.areas_mut(seat).close(area)?;
        self.fronts.forget(area);
        self.lent.remove(&area);
        Ok(heir)
    }

    /// Where a reopened tab goes: the area it was closed from while the
    /// seat has it, else the focused one.
    pub fn reopening(&mut self, seat: S, from: Option<AreaId>) -> AreaId {
        let areas = self.areas_mut(seat);
        match from {
            Some(area) if areas.contains(area) => area,
            _ => areas.focused(),
        }
    }

    /// The seat moved — a root carried to another workspace.
    pub fn moved(&mut self, from: S, to: S) {
        if let Some(areas) = self.seats.remove(&from) {
            self.seats.insert(to, areas);
        }
    }

    /// The seat is gone, and every area's front with it.
    pub fn forget(&mut self, seat: S) {
        if let Some(areas) = self.seats.remove(&seat) {
            for area in areas.ids() {
                self.fronts.forget(area);
                self.lent.remove(&area);
            }
        }
    }
}

/// Which of a divided tab's panes the strip draws its mark for — M12.8's
/// second window check, O7: the most urgent by
/// [`Status::urgency`](guild_session::status::Status::urgency), the order
/// a project's tab and the Dock already fold statuses by, and **the first
/// listed among equals** — the window lists the focused pane first, so a
/// tab with nothing to say wears its focused pane's mark, and an agent
/// working or waiting beside a focused shell keeps its pulse and its dot
/// in the strip. The name stays the focused pane's.
pub fn loudest<T>(
    panes: impl IntoIterator<Item = (T, guild_session::status::Status)>,
) -> Option<T> {
    let mut best: Option<(T, u8)> = None;
    for (pane, status) in panes {
        let urgency = status.urgency();
        if best.as_ref().is_none_or(|(_, held)| urgency > *held) {
            best = Some((pane, urgency));
        }
    }
    best.map(|(pane, _)| pane)
}

/// `area`'s strip: the tabs that name it, in the window's order.
pub fn strip<T: Copy, A: Copy + Eq>(order: &[(T, A)], area: A) -> Vec<T> {
    order
        .iter()
        .filter(|(_, own)| *own == area)
        .map(|(tab, _)| *tab)
        .collect()
}

/// Where `tab` sits in its own strip, counting from nought — what a tab
/// with no name of its own is numbered by (*Session 2*), in the strip, the
/// close sheet and the agents list alike. `None` for a tab not in `order`.
///
/// **Its own strip, not its seat**: since M12.8 a seat's tabs are drawn in
/// several strips, each numbering from one, and a sheet that counted the
/// seat's tabs named a strip's *Session 1* as *Session 5*.
pub fn place<T: Copy + Eq, A: Copy + Eq>(order: &[(T, A)], tab: T) -> Option<usize> {
    let (_, area) = order.iter().find(|(own, _)| *own == tab)?;
    strip(order, *area).iter().position(|own| *own == tab)
}

/// Move `dragged` before or after `target`, into `target`'s area — M14's
/// drag, across areas since M12.8. Answers whether anything moved.
pub fn reorder<T: Copy + Eq, A: Copy + Eq>(
    order: &mut Vec<(T, A)>,
    dragged: T,
    target: T,
    before: bool,
) -> bool {
    if dragged == target {
        return false;
    }
    let (Some(from), Some(to)) = (
        order.iter().position(|(tab, _)| *tab == dragged),
        order.iter().position(|(tab, _)| *tab == target),
    ) else {
        return false;
    };
    let (area, was) = (order[to].1, order[from].1);
    let target_after = if from < to { to - 1 } else { to };
    let at = if before {
        target_after
    } else {
        target_after + 1
    };
    if at == from && was == area {
        return false;
    }
    order.remove(from);
    order.insert(at.min(order.len()), (dragged, area));
    true
}

/// Move `tab` to the end of `area`'s strip — a drop on an empty area.
pub fn move_to_end<T: Copy + Eq, A: Copy + Eq>(order: &mut Vec<(T, A)>, tab: T, area: A) -> bool {
    let Some(from) = order.iter().position(|(own, _)| *own == tab) else {
        return false;
    };
    let last = order.iter().rposition(|(_, own)| *own == area);
    if order[from].1 == area && last == Some(from) {
        return false;
    }
    order.remove(from);
    order.push((tab, area));
    true
}

/// The area an arrow from `from` reaches in each direction — left,
/// right, up, down, `Direction`'s order — over the areas' own frames:
/// what *Move to Area ▸* offers (decision 21). An area names no area of
/// its own; where it is is its name.
pub fn neighbours(areas: &[(AreaId, Frame)], from: AreaId) -> [Option<AreaId>; 4] {
    [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ]
    .map(|direction| toward(areas, &from, direction))
}

/// Every tab of `from` names `to` instead — a closed area's tabs going to
/// its heir (decision 9) — and they move to the end of the order,
/// together and in their own order, so the heir's strip reads its own tabs
/// and then the ones that arrived. A stable partition: the window's one
/// list is history, and keeping each moved tab's place there interleaved
/// the two strips by nothing the person had arranged (the final review's
/// ruling). Every other tab keeps its place.
pub fn rehome<T, A: Copy + Eq>(order: &mut Vec<(T, A)>, from: A, to: A) {
    let (moving, staying): (Vec<_>, Vec<_>) = std::mem::take(order)
        .into_iter()
        .partition(|(_, area)| *area == from);
    order.extend(staying);
    order.extend(moving.into_iter().map(|(tab, _)| (tab, to)));
}

/// A place the arrows reach: a pane of an area's tab in front, or an
/// empty area — decision 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Place {
    /// The area.
    pub area: AreaId,
    /// The pane of its tab in front; none for an empty area.
    pub pane: Option<PaneId>,
}

/// Every place of a seat in one geometry: each area's body under its strip
/// of `strip` pixels, divided into the panes `panes` gives for it, or
/// whole when it gives none.
pub fn places(
    areas: &[(AreaId, Frame)],
    strip: f32,
    panes: impl Fn(AreaId, Frame) -> Vec<(PaneId, Frame)>,
) -> Vec<(Place, Frame)> {
    let mut out = Vec::new();
    for (area, frame) in areas {
        let body = Frame::new(
            frame.x,
            frame.y + strip,
            frame.width,
            (frame.height - strip).max(0.0),
        );
        let inside = panes(*area, body);
        if inside.is_empty() {
            out.push((
                Place {
                    area: *area,
                    pane: None,
                },
                body,
            ));
        } else {
            out.extend(inside.into_iter().map(|(pane, frame)| {
                (
                    Place {
                        area: *area,
                        pane: Some(pane),
                    },
                    frame,
                )
            }));
        }
    }
    out
}
