//! Where a tab lands and which area has the keys — M12.8. The rules are
//! `desktop::stage`'s and pure; this is the window asking them.

use std::time::Instant;

use gpui::{Context, Window};
use guild_session::layout::{Axis, Direction, Floor, Frame, Side, room_to_split, toward};
use guild_session::{AreaId, ProjectId, Seat};

use crate::i18n::Text;
use crate::stage::{self, Place};
use crate::{Body, Root, Tab, TabId, theme};
use guild_session::PaneId;

impl Root {
    /// Put a new tab in its seat's focused area — decision 15. **The one
    /// place a `Tab` is made**: `guild session new`, a harness tab, the
    /// IDE's `openDiff`, a file from the diff window, a restore and a
    /// reopen all come here, so an opening that forgot the area cannot
    /// be written.
    ///
    /// For the seat in front the area is `focused_area`'s — read from the
    /// tab in front (decision 7), which the stage hears of only at the
    /// next save; for any other seat it is the stage's.
    pub(crate) fn adopt(
        &mut self,
        seat: Seat,
        project: ProjectId,
        body: Body,
        cx: &gpui::App,
    ) -> TabId {
        self.adopt_panes(seat, project, guild_session::Panes::one(body), cx)
    }

    /// [`Root::adopt`] for a tab that already has its panes — a restore
    /// or a reopen of a divided tab. `adopt` comes here with one pane, so
    /// this stays the one place a `Tab` is made.
    pub(crate) fn adopt_panes(
        &mut self,
        seat: Seat,
        project: ProjectId,
        panes: guild_session::Panes<Body>,
        cx: &gpui::App,
    ) -> TabId {
        self.note_place(cx);
        let id = TabId::new();
        let area = match self.focused_area() {
            Some(area) if self.active_seat() == Some(seat) => area,
            _ => self.stage.landing(seat),
        };
        self.tabs.push(Tab {
            id,
            seat,
            project,
            area,
            panes,
            last_probe: Instant::now(),
        });
        id
    }

    /// The area with the keys in the seat in front — decision 7: the tab
    /// in front's, else the stage's. `None` with no project in front, or a
    /// seat the stage has never heard of, which has no tabs either.
    pub(crate) fn focused_area(&self) -> Option<AreaId> {
        let seat = self.active_seat()?;
        let front = self
            .active_tab
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .map(|tab| (tab.seat, tab.area));
        self.stage.focused_with(seat, front)
    }

    /// Every tab of the seat in front, whatever its area — for a lookup
    /// that must not open a second copy of what the other half of the
    /// screen shows (decision 15), and for counting the seat's tabs.
    pub(crate) fn seat_tabs(&self) -> impl Iterator<Item = &Tab> {
        let seat = self.active_seat();
        self.tabs.iter().filter(move |tab| Some(tab.seat) == seat)
    }

    /// The tabs of one area of the seat in front, in strip order. `None`
    /// is the one card drawn before the seat has areas, which has none.
    pub(crate) fn area_tabs(&self, area: Option<AreaId>) -> impl Iterator<Item = &Tab> {
        self.seat_tabs().filter(move |tab| Some(tab.area) == area)
    }

    /// The tab and pane of `seat` showing what `wanted` accepts — every
    /// area, every pane (decision 15): a reuse lookup that looked only at
    /// the strip in front, or only at each tab's focused pane, would open
    /// a second copy of what the other half of the screen shows.
    pub(crate) fn find_showing(
        &self,
        seat: Seat,
        wanted: impl Fn(&Body) -> bool,
    ) -> Option<(TabId, guild_session::PaneId)> {
        self.tabs
            .iter()
            .filter(|tab| tab.seat == seat)
            .find_map(|tab| tab.panes.find(&wanted).map(|pane| (tab.id, pane)))
    }

    /// Give `pane` of `tab` the keys within its tab — what a reuse lookup
    /// does besides bringing the tab forward, so the thing it found is
    /// the one in front. Answers the pane's body, for the focus handle.
    ///
    /// **The window's keys are the caller's to move**, because only the
    /// caller knows where they should end: the Changes panel and the tree
    /// reuse a tab and then take the keys back themselves (traps.md,
    /// *Opening a tab focuses it*), and a restore must not take them from
    /// a dock. A gesture that wants them in the pane focuses its body at
    /// once, or sets `focusing_front` for the next frame — the pane beside
    /// it is drawn, and never loses the focus by itself (M12.8's second
    /// window check, 12.2).
    pub(crate) fn focus_pane(
        &mut self,
        tab: TabId,
        pane: guild_session::PaneId,
        cx: &gpui::App,
    ) -> Option<&Body> {
        self.note_place(cx);
        let tab = self.tabs.iter_mut().find(|open| open.id == tab)?;
        tab.panes.focus(pane);
        tab.panes.body_of(pane)
    }

    /// The number a tab with no name of its own is called by — *Session 2*
    /// — which is its place in its own strip, whatever seat it is in:
    /// the close sheet and the agents list say what the strip says
    /// (`stage::place`).
    pub(crate) fn tab_number(&self, tab: &Tab) -> usize {
        let order: Vec<(TabId, (Seat, AreaId))> = self
            .tabs
            .iter()
            .map(|other| (other.id, (other.seat, other.area)))
            .collect();
        stage::place(&order, tab.id).unwrap_or(0) + 1
    }

    /// The tab in front of `area`: the tab in front of the window for the
    /// focused area — which is what makes it focused (decision 7) — and
    /// the one its area remembers for every other.
    pub(crate) fn area_front(&self, area: Option<AreaId>) -> Option<TabId> {
        if area == self.focused_area() {
            return self.active_tab;
        }
        let area = area?;
        let strip: Vec<TabId> = self.area_tabs(Some(area)).map(|tab| tab.id).collect();
        self.stage.front(area, &strip)
    }

    /// The tab in front of every area of the seat in front — what is on
    /// the screen, and so what a focused window has seen (the final
    /// review's Minor 1). The tab in front of the window alone for a seat
    /// that was never divided.
    pub(crate) fn fronts_on_screen(&self) -> Vec<TabId> {
        let areas = self
            .active_seat()
            .and_then(|seat| self.stage.areas(seat))
            .map(|areas| areas.ids())
            .unwrap_or_default();
        if areas.is_empty() {
            return self.active_tab.into_iter().collect();
        }
        areas
            .into_iter()
            .filter_map(|area| self.area_front(Some(area)))
            .collect()
    }

    /// A design-system length in the window's pixels, at the rem size
    /// `render` sets — what the areas' geometry, which is pixels, is cut
    /// with.
    pub(crate) fn rem_px(&self, rems: gpui::Rems) -> f32 {
        rems.0 * f32::from(crate::scale::rem_size(self.settings.ui_font_size))
    }

    /// The least an area is made — decision 17, the design system's two
    /// numbers at the window's scale.
    pub(crate) fn area_floor(&self) -> Floor {
        Floor {
            width: self.rem_px(theme::space::AREA_MIN_WIDTH),
            height: self.rem_px(theme::space::AREA_MIN_HEIGHT),
        }
    }

    /// The room the areas divide, as the last frame measured it, with its
    /// corner at the origin — what the frames and the borders are cut
    /// from. `None` before the first measurement.
    pub(crate) fn work_room(&self) -> Option<Frame> {
        let bounds = self.work_at.get()?;
        Some(Frame::new(
            0.0,
            0.0,
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ))
    }

    /// Where each area of the seat in front lands in the room, in draw
    /// order — decision 17: from the room's **measured** size, no area
    /// under the floor while both sides fit. Empty before the first
    /// measurement, and for a seat that has no areas yet; the window then
    /// draws the focused area alone, filling the room.
    pub(crate) fn area_frames(&self) -> Vec<(AreaId, Frame)> {
        let (Some(areas), Some(room)) = (
            self.active_seat().and_then(|seat| self.stage.areas(seat)),
            self.work_room(),
        ) else {
            return Vec::new();
        };
        areas.tree().frames(room, self.area_floor())
    }

    /// Give `area` the keys — a press anywhere in it (decision 16: a
    /// press on a start screen that is not the focused one focuses its
    /// place first). Its front tab comes forward; an empty area keeps the
    /// focus itself, on its start screen, which the render then focuses.
    pub(crate) fn focus_area(&mut self, area: AreaId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.focus_area_without_history(area, cx);
        self.history.suspended -= 1;
    }

    fn focus_area_without_history(&mut self, area: AreaId, cx: &mut Context<Root>) {
        let Some(seat) = self.active_seat() else {
            return;
        };
        if self.focused_area() == Some(area) {
            return;
        }
        let front = self.area_front(Some(area));
        self.stage.focus(seat, area);
        self.active_tab = front;
        // The sessions list is the focused area's; it does not follow.
        // And the keys with it: whatever had them was in the other area.
        // A press on a terminal focuses it anyway; a press on a strip or
        // a start screen focuses nothing by itself.
        self.focusing_front = true;
        self.save();
        cx.notify();
    }

    /// How many areas the seat in front has — one when it has never been
    /// divided, or there is no seat: what the `⋮` asks before it offers
    /// *Close Area* (decision 9).
    pub(crate) fn area_count(&self) -> usize {
        self.active_seat()
            .and_then(|seat| self.stage.areas(seat))
            .map_or(1, |areas| areas.ids().len())
    }

    /// Divide the focused area along `axis` — `SplitAreaRight` and
    /// `SplitAreaDown`, decision 14. The new area is empty and has the
    /// keys, on its start screen (decision 16), which is focused here
    /// rather than left to a later frame: nothing takes the keys by itself.
    ///
    /// **Refused, with a notice, when a side would be under the floor** —
    /// decision 17 — measured on the area's frame as the last frame cut
    /// it; an undivided seat not yet measured is the room itself.
    pub(crate) fn split_area(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.split_area_without_history(axis, window, cx);
        self.history.suspended -= 1;
    }

    fn split_area_without_history(
        &mut self,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(seat) = self.active_seat() else {
            return;
        };
        // What is in front now is its area's front when the keys come back.
        self.note_front();
        let area = self
            .focused_area()
            .unwrap_or_else(|| self.stage.landing(seat));
        if self.divide(seat, area, axis, cx).is_none() {
            return;
        }
        self.active_tab = None;
        self.save();
        self.refocus(window, cx);
    }

    /// Divide `area` of `seat` along `axis` and answer the new area, which
    /// has the keys on the stage — or refuse, with the notice, when a side
    /// would be under the floor (decision 17), and answer `None`. What is
    /// in front, and where the keys go, is the caller's.
    fn divide(
        &mut self,
        seat: Seat,
        area: AreaId,
        axis: Axis,
        cx: &mut Context<Root>,
    ) -> Option<AreaId> {
        let frame = self
            .area_frames()
            .into_iter()
            .find(|(id, _)| *id == area)
            .map(|(_, frame)| frame)
            .or_else(|| self.work_room().filter(|_| self.area_count() == 1));
        let frame = frame?;
        if !room_to_split(frame, axis, self.area_floor()) {
            let text = self.ui().text(Text::NoticeNoRoomToSplit).to_owned();
            self.inform(text);
            cx.notify();
            return None;
        }
        self.stage.split(seat, area, axis)
    }

    /// The strip `tab` is drawn in: the tabs of its own seat and area, in
    /// the window's order. What a tab's menu acts on — *Close Others*,
    /// *Close All*, the two sides — which is that tab's strip and not the
    /// focused one's.
    pub(crate) fn strip_of(&self, tab: TabId) -> Vec<TabId> {
        let Some((seat, area)) = self
            .tabs
            .iter()
            .find(|own| own.id == tab)
            .map(|own| (own.seat, own.area))
        else {
            return Vec::new();
        };
        self.tabs
            .iter()
            .filter(|own| own.seat == seat && own.area == area)
            .map(|own| own.id)
            .collect()
    }

    /// Which areas an arrow from `tab`'s own area reaches — left, right,
    /// up, down — for its menu's *Move to Area ▸* (decision 21). Nothing
    /// before the room is measured, and nothing for a tab of a seat not in
    /// front, whose areas are not drawn.
    pub(crate) fn tab_neighbours(&self, tab: TabId) -> [bool; 4] {
        let Some(area) = self
            .tabs
            .iter()
            .find(|own| own.id == tab && Some(own.seat) == self.active_seat())
            .map(|own| own.area)
        else {
            return [false; 4];
        };
        stage::neighbours(&self.area_frames(), area).map(|there| there.is_some())
    }

    /// Move `tab` to the area an arrow from its own reaches — *Move to
    /// Area ▸*'s *Right Area* and its three.
    pub(crate) fn move_tab_toward(
        &mut self,
        tab: TabId,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(from) = self
            .tabs
            .iter()
            .find(|own| own.id == tab && Some(own.seat) == self.active_seat())
            .map(|own| own.area)
        else {
            return;
        };
        if let Some(to) = toward(&self.area_frames(), &from, direction) {
            self.move_tab_to_area(tab, to, None, window, cx);
        }
    }

    /// Divide `tab`'s own area and move the tab into the new one — *New
    /// Area to the Right* and *New Area Below*, one gesture (decision 21).
    /// Refused with the split's own notice when there is no room.
    pub(crate) fn move_tab_to_new_area(
        &mut self,
        tab: TabId,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some((seat, from)) = self
            .tabs
            .iter()
            .find(|own| own.id == tab && Some(own.seat) == self.active_seat())
            .map(|own| (own.seat, own.area))
        else {
            return;
        };
        if let Err(text) = self.process_move_group(tab) {
            self.warn(self.ui().text(text).to_owned());
            return;
        }
        self.note_front();
        let Some(new) = self.divide(seat, from, axis, cx) else {
            return;
        };
        self.move_tab_to_area(tab, new, None, window, cx);
    }

    /// Move `tab` into `area` of its own seat — decision 8. Dropped on a
    /// tab, `at` names that tab and whether it went before it
    /// (`stage::reorder`); `None` puts it at the end of the area's strip
    /// (`stage::move_to_end`) — a drop on an empty area, or the menu.
    ///
    /// **Nothing is restarted.** The `Tab` is the same value — its body,
    /// its process, its terminal and their state — moved in the window's
    /// one list with its area rewritten, so a tab is never in two areas
    /// and never made again. The area it left brings its neighbour
    /// forward; the tab comes forward where it lands, and that area takes
    /// the keys.
    pub(crate) fn move_tab_to_area(
        &mut self,
        tab: TabId,
        area: AreaId,
        at: Option<(TabId, bool)>,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.move_tab_to_area_without_history(tab, area, at, window, cx);
        self.history.suspended -= 1;
    }

    fn move_tab_to_area_without_history(
        &mut self,
        tab: TabId,
        area: AreaId,
        at: Option<(TabId, bool)>,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some((seat, own)) = self
            .tabs
            .iter()
            .find(|each| each.id == tab)
            .map(|each| (each.seat, each.area))
        else {
            return;
        };
        // Only into an area of the tab's own seat: another seat's area is
        // another project's screen. Its own area is always its seat's,
        // whatever the stage has heard — a reorder within a strip.
        if area != own
            && !self
                .stage
                .areas(seat)
                .is_some_and(|areas| areas.contains(area))
        {
            return;
        }
        // Validate the anchor before any mixed body transfer.
        if let Some((target, _)) = at
            && self
                .tabs
                .iter()
                .find(|t| t.id == target)
                .is_none_or(|t| t.seat != seat || t.area != area)
        {
            return;
        }
        let group = match self.process_move_group(tab) {
            Ok(group) => group,
            Err(text) => {
                self.warn(self.ui().text(text).to_owned());
                return;
            }
        };
        if let Some((owner, group)) = group {
            if at.is_some_and(|(target, _)| group.contains_key(&target)) {
                return;
            }
            let previous = self.tab_order();
            let (active, mut members) = self.detach_process_group(&group, tab, cx);
            members.sort_by_key(|id| {
                !self
                    .tabs
                    .iter()
                    .find(|t| t.id == *id)
                    .unwrap()
                    .sessions()
                    .any(|s| s == owner)
            });
            let mut order = self.tab_order();
            if at.is_some_and(|(_, before)| !before) {
                members.reverse();
            }
            for member in members {
                match at {
                    Some((target, before)) => stage::reorder(&mut order, member, target, before),
                    None => stage::move_to_end(&mut order, member, area),
                };
            }
            if order != previous {
                self.take_order(active, order, window, cx);
                if active != tab && self.active_tab != Some(active) {
                    self.active_tab = Some(active);
                    self.stage.bring_forward(seat, area, active);
                    self.save();
                    self.refocus(window, cx);
                }
            }
            return;
        }
        let mut order = self.tab_order();
        let moved = match at {
            Some((target, before)) => stage::reorder(&mut order, tab, target, before),
            None => stage::move_to_end(&mut order, tab, area),
        };
        if moved {
            self.take_order(tab, order, window, cx);
        }
    }

    /// The window's one list of tabs as the stage reads it: each tab and
    /// the area it names.
    fn tab_order(&self) -> Vec<(TabId, AreaId)> {
        self.tabs.iter().map(|tab| (tab.id, tab.area)).collect()
    }

    /// Put the window's tabs in `order`, each in the area `order` names —
    /// a permutation of the same values, so nothing is made again — after
    /// `moved` has been moved. When it changed area it is carried: the
    /// stage re-chooses the front of the area it left, and the tab comes
    /// forward, with the keys, in the one it went to.
    fn take_order(
        &mut self,
        moved: TabId,
        order: Vec<(TabId, AreaId)>,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some((seat, from)) = self
            .tabs
            .iter()
            .find(|tab| tab.id == moved)
            .map(|tab| (tab.seat, tab.area))
        else {
            return;
        };
        let mut previous = self.tab_order();
        let mut carried: Vec<_> = previous
            .iter()
            .filter_map(|(id, from)| {
                order
                    .iter()
                    .find(|(other, _)| other == id)
                    .filter(|(_, to)| to != from)
                    .map(|(_, to)| (*id, *from, *to))
            })
            .collect();
        // The dragged member takes the keys last. Companions still apply the
        // existing carry/loan rule to every source area's surviving front.
        carried.sort_by_key(|(id, _, _)| *id == moved);

        // What is in front now is its own area's front, whatever moves.
        self.note_front();
        let place: std::collections::HashMap<TabId, (usize, AreaId)> = order
            .iter()
            .enumerate()
            .map(|(at, (id, area))| (*id, (at, *area)))
            .collect();
        self.tabs
            .sort_by_key(|tab| place.get(&tab.id).map_or(usize::MAX, |(at, _)| *at));
        for tab in &mut self.tabs {
            if let Some((_, area)) = place.get(&tab.id) {
                tab.area = *area;
            }
        }
        let to = place.get(&moved).map_or(from, |(_, area)| *area);
        if !carried.is_empty() {
            for (id, from, to) in carried {
                let strip = stage::strip(&previous, from);
                self.stage.carry(seat, id, from, &strip, to);
                if let Some(row) = previous.iter_mut().find(|(old, _)| *old == id) {
                    row.1 = to;
                }
            }
            self.stage.bring_forward(seat, to, moved);
            self.active_tab = Some(moved);
            self.save();
            self.refocus(window, cx);
        } else {
            self.save();
            cx.notify();
        }
    }

    /// Close the focused area — the `⋮`'s row and the View menu's, never a
    /// stroke (decision 14). **No tab closes** (decision 9): the area's
    /// tabs go to its heir, the area that takes its room, after the heir's
    /// own and in their own order (`stage::rehome`), and the heir's front
    /// comes forward with the keys. The last area stays, and the View menu — which cannot
    /// draw a row faint with its reason — is answered with the reason.
    pub(crate) fn close_area(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.close_area_without_history(window, cx);
        self.history.suspended -= 1;
    }

    fn close_area_without_history(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        let Some(seat) = self.active_seat() else {
            return;
        };
        self.note_front();
        let Some(area) = self.focused_area() else {
            return;
        };
        let Ok(heir) = self.stage.close_area(seat, area) else {
            let text = self.ui().text(Text::NoticeLastAreaStays).to_owned();
            self.inform(text);
            cx.notify();
            return;
        };
        let mut order = self.tab_order();
        stage::rehome(&mut order, area, heir);
        // The same values in the order `rehome` answers: the closed area's
        // tabs after the heir's own.
        let place: std::collections::HashMap<TabId, usize> = order
            .iter()
            .enumerate()
            .map(|(at, (id, _))| (*id, at))
            .collect();
        self.tabs
            .sort_by_key(|tab| place.get(&tab.id).copied().unwrap_or(usize::MAX));
        for tab in &mut self.tabs {
            if tab.area == area {
                tab.area = heir;
            }
        }
        let strip: Vec<TabId> = self.area_tabs(Some(heir)).map(|tab| tab.id).collect();
        self.active_tab = self.stage.front(heir, &strip);
        self.stage.focus(seat, heir);
        self.save();
        self.refocus(window, cx);
    }

    /// Give the keys to the place an arrow reaches from the focused one —
    /// `FocusLeft` and its three, decision 14. **One geometry**: every
    /// area's front tab's panes, and every empty area as one place, so the
    /// same arrows cross between panes and into the next area; an arrow
    /// with nothing on its side does nothing, and the walk does not wrap.
    /// A pane reached takes the keys with its tab and its area. The keys
    /// are put on what is now in front explicitly — the pane's body, or a
    /// start screen (traps.md: *…the focus-lost rule does not cover a
    /// closed tab*; nothing is closed here, but the element that had the
    /// keys is still drawn, so nothing else would move them).
    pub(crate) fn focus_toward(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.focus_toward_without_history(direction, window, cx);
        self.history.suspended -= 1;
    }

    fn focus_toward_without_history(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        // What is in front now is its area's front when the keys come
        // back — as `split_area` and `close_area` do before they move them.
        self.note_front();
        let Some(from) = self.focused_area() else {
            return;
        };
        let strip = self.rem_px(theme::space::TABS);
        let floor = self.area_floor();
        let frames = self.area_frames();
        let fronts: Vec<(AreaId, Option<TabId>)> = frames
            .iter()
            .map(|(area, _)| (*area, self.area_front(Some(*area))))
            .collect();
        let places = stage::places(&frames, strip, |area, body| {
            fronts
                .iter()
                .find(|(id, _)| *id == area)
                .and_then(|(_, tab)| *tab)
                .and_then(|tab| self.tabs.iter().find(|open| open.id == tab))
                .map(|tab| tab.panes.tree().frames(body, floor))
                .unwrap_or_default()
        });
        let here = Place {
            area: from,
            pane: self
                .active_tab
                .and_then(|tab| self.tabs.iter().find(|open| open.id == tab))
                .filter(|tab| tab.area == from)
                .map(|tab| tab.panes.focused()),
        };
        let Some(to) = toward(&places, &here, direction) else {
            return;
        };
        self.focus_area(to.area, cx);
        if let (Some(pane), Some(tab)) = (to.pane, self.active_tab) {
            self.press_pane(tab, pane, cx);
        }
        self.refocus(window, cx);
    }

    /// A scroll handle for every strip about to be drawn, and none for an
    /// area that is gone — the correction to task 6: one handle per strip.
    pub(crate) fn keep_strip_scrolls(&mut self) {
        let drawn: Vec<Option<AreaId>> =
            match self.active_seat().and_then(|seat| self.stage.areas(seat)) {
                Some(areas) => areas.ids().into_iter().map(Some).collect(),
                None => vec![self.focused_area()],
            };
        // The focused area drawn alone before the room is measured is one
        // of the seat's; a tab in front whose area the seat does not know
        // is drawn in that one card.
        let mut drawn = drawn;
        if !drawn.contains(&self.focused_area()) {
            drawn.push(self.focused_area());
        }
        self.tab_drag.retain(&drawn);
        self.tab_scroll.retain(|area, _| drawn.contains(area));
        for area in &drawn {
            self.tab_scroll.entry(*area).or_default();
        }
        self.strip_at
            .borrow_mut()
            .retain(|area, _| drawn.contains(area));
        self.body_at
            .borrow_mut()
            .retain(|area, _| drawn.contains(area));
    }

    /// Take hold of the border between two areas — or, on a second click,
    /// put it back at a half, and do not also begin a drag: the hand that
    /// double-clicks moves a little, and the reset would be undone at once.
    pub(crate) fn grab_area_border(
        &mut self,
        seat: Seat,
        path: Vec<Side>,
        event: &gpui::MouseDownEvent,
        cx: &mut Context<Root>,
    ) {
        if event.click_count >= 2 {
            self.area_drag = None;
            if self.stage.areas_mut(seat).tree_mut().reset(&path) {
                self.save();
            }
            cx.notify();
            return;
        }
        let Some(held) = self
            .stage
            .areas(seat)
            .and_then(|areas| areas.tree().split_at(&path))
        else {
            return;
        };
        self.area_drag = Some((seat, path, held));
        self.drag_from = Some(event.position);
        cx.notify();
    }

    /// Follow the pointer while an area's border is dragged — **measured
    /// from where the hand pressed**, not from its last step
    /// (`Tree::drag_from`: a slow hand's steps were rounded away), along
    /// the border's own axis, and no area under its floor.
    pub(crate) fn drag_area_border(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Root>,
    ) {
        let (Some((seat, path, held)), Some(from), Some(room)) =
            (self.area_drag.clone(), self.drag_from, self.work_room())
        else {
            return;
        };
        let floor = self.area_floor();
        let Some(axis) = self.stage.areas(seat).and_then(|areas| {
            areas
                .tree()
                .borders(room, floor)
                .into_iter()
                .find(|border| border.path == path)
                .map(|border| border.axis)
        }) else {
            // The border went while it was held — the seat changed, or the
            // area closed. Nothing is left to follow.
            self.area_drag = None;
            return;
        };
        let moved = match axis {
            guild_session::layout::Axis::Horizontal => f32::from(at.x - from.x),
            guild_session::layout::Axis::Vertical => f32::from(at.y - from.y),
        };
        self.stage
            .areas_mut(seat)
            .tree_mut()
            .drag_from(&path, held, moved, room, floor);
        cx.notify();
    }
}

/// The panes — M12.8, decisions 13, 17, 19 and 20: a tab divided, its
/// borders dragged, and an empty pane filled from its start screen.
impl Root {
    /// The room an area's body gives its front tab's panes, as the last
    /// frame measured it, its corner at the origin — decision 17's rule
    /// for the areas, one level in. `None` before the area's first frame.
    pub(crate) fn body_room(&self, area: Option<AreaId>) -> Option<Frame> {
        let bounds = *self.body_at.borrow().get(&area)?;
        Some(Frame::new(
            0.0,
            0.0,
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ))
        .filter(|room| room.width > 0.0 && room.height > 0.0)
    }

    /// Where each pane of `tab` lands in its area's body, in draw order,
    /// none under the floor an area has (decision 17). Empty before the
    /// area is measured.
    pub(crate) fn pane_frames(&self, tab: TabId) -> Vec<(PaneId, Frame)> {
        let Some(open) = self.tabs.iter().find(|open| open.id == tab) else {
            return Vec::new();
        };
        let Some(room) = self.body_room(Some(open.area)) else {
            return Vec::new();
        };
        open.panes.tree().frames(room, self.area_floor())
    }

    /// What a place `width` by `height` pixels shows of the start screen
    /// — decision 20, its thresholds read at the window's scale: a larger
    /// interface needs a larger place for the same screen.
    pub(crate) fn start_fits(&self, width: f32, height: f32) -> crate::start::Fits {
        let points = crate::scale::BASE / self.rem_px(gpui::Rems(1.0));
        crate::start::fits(width * points, height * points)
    }

    /// Divide the focused pane of the tab in front along `axis` —
    /// `SplitPaneRight` and `SplitPaneDown`, decision 14. The new pane is
    /// empty (decision 13) and has the keys, on its start screen, which
    /// `refocus` puts them on: nothing takes the keys by itself.
    ///
    /// **Refused, with a notice, when a side would be under the floor** —
    /// decision 17, measured on the pane's frame as the last frame cut it.
    pub(crate) fn split_pane(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.split_pane_without_history(axis, window, cx);
        self.history.suspended -= 1;
    }

    fn split_pane_without_history(
        &mut self,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(tab) = self.active_tab else {
            return;
        };
        let Some(pane) = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| open.panes.focused())
        else {
            return;
        };
        let Some(frame) = self
            .pane_frames(tab)
            .into_iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, frame)| frame)
        else {
            return;
        };
        if !room_to_split(frame, axis, self.area_floor()) {
            // The tab is what a person asked to divide, not its area.
            let text = self.ui().text(Text::NoticeNoRoomToSplitTab).to_owned();
            self.inform(text);
            cx.notify();
            return;
        }
        if let Some(open) = self.tabs.iter_mut().find(|open| open.id == tab) {
            open.panes.split(pane, axis);
        }
        // The panes are drawn where the body is; the sessions list is not
        // what the person asked to see divided.
        self.save();
        self.refocus(window, cx);
    }

    /// Divide `tab`'s focused pane — the tab menu's *Split Tab Right* and
    /// *Split Tab Down*, decision 21. A tab behind another comes forward
    /// first, with its area: the new pane has the keys, and keys on a
    /// start screen nobody draws are a dispatch path with nothing on it.
    pub(crate) fn split_tab(
        &mut self,
        tab: TabId,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.split_tab_without_history(tab, axis, window, cx);
        self.history.suspended -= 1;
    }

    fn split_tab_without_history(
        &mut self,
        tab: TabId,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        if !self.tabs.iter().any(|open| open.id == tab) {
            return;
        }
        if self.active_tab != Some(tab) {
            // What was in front is its own area's front when it comes back.
            self.note_front();
            self.active_tab = Some(tab);
        }
        self.split_pane(axis, window, cx);
    }

    /// The tab in front and its focused pane, when that pane is empty —
    /// the place a start screen's deed is poured into (decision 13).
    pub(crate) fn empty_pane_in_front(&self) -> Option<(TabId, PaneId)> {
        let tab = self
            .tabs
            .iter()
            .find(|open| Some(open.id) == self.active_tab)?;
        let pane = tab.panes.focused();
        tab.panes.body_of(pane).is_none().then_some((tab.id, pane))
    }

    /// Move the one body of `from` — a tab just opened by the start screen
    /// — into the empty pane `into`, and drop `from` (decision 13). The
    /// body is moved, not made again: its process goes on.
    ///
    /// Nothing happens unless `from` is a tab of one pane in the same seat
    /// and `into` names an empty pane of another tab: a pane that shows
    /// something is kept, and the opened tab stays a tab of its own.
    pub(crate) fn pour(&mut self, from: TabId, into: (TabId, PaneId), cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.pour_without_history(from, into, cx);
        self.history.suspended -= 1;
    }

    fn pour_without_history(&mut self, from: TabId, into: (TabId, PaneId), cx: &mut Context<Root>) {
        let (to, pane) = into;
        let seat_of = |id: TabId| {
            self.tabs
                .iter()
                .find(|open| open.id == id)
                .map(|open| open.seat)
        };
        if from == to || seat_of(from).is_none() || seat_of(from) != seat_of(to) {
            return;
        }
        let empty = self.tabs.iter().any(|open| {
            open.id == to && open.panes.ids().contains(&pane) && open.panes.body_of(pane).is_none()
        });
        let Some(at) = self
            .tabs
            .iter()
            .position(|open| open.id == from && open.panes.ids().len() == 1)
        else {
            return;
        };
        if !empty {
            return;
        }
        let mut opened = self.tabs.remove(at);
        let shown = opened.panes.focused();
        let Some(body) = opened.panes.take(shown) else {
            self.tabs.insert(at, opened);
            return;
        };
        let Some(target) = self.tabs.iter_mut().find(|open| open.id == to) else {
            // Checked above; a body is never dropped on the way.
            if opened.panes.fill(shown, body).is_ok() {
                self.tabs.insert(at, opened);
            }
            return;
        };
        if let Err(body) = target.panes.fill(pane, body) {
            if opened.panes.fill(shown, body).is_ok() {
                self.tabs.insert(at, opened);
            }
            return;
        }
        target.panes.focus(pane);
        self.active_tab = Some(to);
        self.save();
        cx.notify();
    }

    /// A press in a pane of `tab` that has not the keys: that pane takes
    /// them before whatever was pressed acts — the carried finding of
    /// task 9, a settings click or a tile acting on the pane it is in.
    /// The keys follow at the next frame, as a press in another area's.
    pub(crate) fn press_pane(&mut self, tab: TabId, pane: PaneId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.press_pane_without_history(tab, pane, cx);
        self.history.suspended -= 1;
    }

    fn press_pane_without_history(&mut self, tab: TabId, pane: PaneId, cx: &mut Context<Root>) {
        let Some(open) = self.tabs.iter_mut().find(|open| open.id == tab) else {
            return;
        };
        if open.panes.focused() == pane || !open.panes.focus(pane) {
            return;
        }
        self.focusing_front = true;
        self.save();
        cx.notify();
    }

    /// Take hold of the border between two panes — or, on a second click,
    /// put it back at a half, and do not also begin a drag:
    /// `grab_area_border`'s reason.
    pub(crate) fn grab_pane_border(
        &mut self,
        tab: TabId,
        path: Vec<Side>,
        event: &gpui::MouseDownEvent,
        cx: &mut Context<Root>,
    ) {
        if event.click_count >= 2 {
            self.pane_drag = None;
            let reset = self
                .tabs
                .iter_mut()
                .find(|open| open.id == tab)
                .is_some_and(|open| open.panes.tree_mut().reset(&path));
            if reset {
                self.save();
            }
            cx.notify();
            return;
        }
        let Some(held) = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .and_then(|open| open.panes.tree().split_at(&path))
        else {
            return;
        };
        self.pane_drag = Some((tab, path, held));
        self.drag_from = Some(event.position);
        cx.notify();
    }

    /// Follow the pointer while a pane's border is dragged — measured from
    /// where the hand pressed, as an area's is, along the border's own
    /// axis, and no pane under the floor.
    pub(crate) fn drag_pane_border(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Root>,
    ) {
        let (Some((tab, path, taken)), Some(from)) = (self.pane_drag.clone(), self.drag_from)
        else {
            return;
        };
        let floor = self.area_floor();
        let held = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .and_then(|open| {
                let room = self.body_room(Some(open.area))?;
                let border = open
                    .panes
                    .tree()
                    .borders(room, floor)
                    .into_iter()
                    .find(|border| border.path == path)?;
                Some((room, border.axis))
            });
        let Some((room, axis)) = held else {
            // The border went while it was held — the tab closed or went
            // behind another. Nothing is left to follow.
            self.pane_drag = None;
            return;
        };
        let moved = match axis {
            Axis::Horizontal => f32::from(at.x - from.x),
            Axis::Vertical => f32::from(at.y - from.y),
        };
        if let Some(open) = self.tabs.iter_mut().find(|open| open.id == tab) {
            open.panes
                .tree_mut()
                .drag_from(&path, taken, moved, room, floor);
        }
        cx.notify();
    }
}
