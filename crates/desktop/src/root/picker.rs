//! The one popup slot: which list is up, and everything every list does.
//!
//! Decision 1 of M8.9. `Root.picker` is an `Option<Open>`, and **only one
//! popup can be up because there is one field for it** — the rule
//! `put_fields_away` used to hold by naming every list, which is how a
//! list got left out of it (`R7`).
//!
//! **A popup is a [`Holds`] variant that answers seven questions**, each a
//! method on `Root` in the popup's own files: its card, its lit rows, its
//! own keys, what an edit of the query does, what taking a row does, what
//! its thread has said, and what a watcher batch means to it. This file
//! matches on `Holds` once per question and does the rest itself — the
//! focus it takes and gives back, the caret's blink, the drag and the
//! resize written through the card's own [`Place`], the arrows over the
//! lit rows, the pointer, the scroll — so the seven rules each popup used
//! to relearn are written once:
//!
//! 1. *A list with rows keeps one lit* — `picker::Picker::settle`.
//! 2. *The pointer moves the one highlight* — [`Root::picker_point`].
//! 3. *Occlude the card* — `view::picker`.
//! 4. *A drag's release can go missing* — `follow_pointer` drops a grab on
//!    a move with no button held.
//! 5. *The keyboard scrolls the list* — [`Scroll::show`].
//! 6. *The popup takes the focus and gives it back* — [`Root::open_picker`]
//!    and [`Root::close_picker`].
//! 7. *A popup outranks its place in the tree* — `view::picker` draws a
//!    hanging card `deferred`.

use gpui::{
    Context, FocusHandle, KeyDownEvent, Pixels, Point, ScrollHandle, ScrollStrategy,
    UniformListScrollHandle, Window,
};
use guild_session::{Grab, PickerPlace, ProjectId};

use crate::Root;
use crate::picker::{Jump, Key, Lit, Picker, Take};
use crate::view::picker::Place;

/// The popup that is up.
pub(crate) struct Open {
    /// Which popup, and whatever it keeps of its own.
    pub holds: Holds,
    /// The query and the lit row.
    pub picker: Picker,
    /// The card's own focus — decision 2: a popup reads its own keys, in
    /// the key context `picker`, so a key meant for the window behind it
    /// cannot reach it and Tab is ours to read.
    pub focus: FocusHandle,
    /// What held the focus before the popup took it, given back when it
    /// closes — rather than `refocus`'s guess at the body in front, which
    /// would act on nothing for a stroke aimed at the panel that had it.
    pub previous: Option<FocusHandle>,
    /// The project the popup is about, when it is about one; closing that
    /// project drops the popup (`close_project_now`).
    pub project: Option<ProjectId>,
    /// The list's scrolling, in the shape its length asks for.
    pub scroll: Scroll,
    /// What the hand has hold of on a floating card, and where it was last
    /// seen.
    pub grab: Option<(Grab, Point<Pixels>)>,
    /// Where the card was last drawn — read by the drag, which has no card
    /// to ask.
    pub place: Option<Place>,
}

/// Which popup is up.
pub(crate) enum Holds {
    /// The directories opened before — M5.8.
    RecentProjects,
    /// The files and the places lately — `cmd-e` and `cmd-shift-e`, M8.8.2.
    RecentFiles(crate::root::recent_files::Popup),
    /// The commands a project can run, under the Output panel's command —
    /// M8.4.
    Run,
    /// The branches and the stash, under the branch in the title bar —
    /// M8.3.3. The face is the tab showing.
    Branches(crate::root::branches::Face),
    /// The faces a setting is chosen from — M5.9: which setting, and the
    /// families this machine has, read **once, when the list opens**,
    /// because `all_font_names` is a CoreText walk and a sort.
    Fonts(crate::fonts::Which, Vec<String>),
    /// Files, symbols, actions and text in one popup — M8.9's group A.
    /// Read through `self.picker` by its own methods; no arm here
    /// destructures it. **Boxed**, like `FindInFiles` below: once both
    /// groups met, whichever of the two is not boxed sizes every `Holds`.
    Everywhere(Box<crate::root::everywhere::Everywhere>),
    /// Text across the project, with a preview — M8.9's group C. **Boxed**:
    /// a walk, a query and two histories are ten times the next largest
    /// kind, and clippy's `large_enum_variant` would size every `Holds` by
    /// it.
    FindInFiles(Box<crate::root::find_in_files::FindPopup>),
}

/// Decision 4's two shapes of list.
pub(crate) enum Scroll {
    /// A long list: one `uniform_list`, every row one height.
    #[allow(
        dead_code,
        reason = "the first long list is Search Everywhere's or Find in Files', after the seams"
    )]
    Uniform(UniformListScrollHandle),
    /// A short list: a scrolling `div`, a heading one of its children.
    Children(ScrollHandle),
}

/// Where the focus goes when a popup closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    /// Back to what held it before the popup opened.
    Back,
    /// Where the deed that closed it put it — **a row that opens a tab
    /// keeps the focus the tab took**; focusing back after it would take
    /// the keyboard away from what was just opened.
    Kept,
}

impl Scroll {
    /// A fresh handle for a short list.
    pub(crate) fn children() -> Self {
        Self::Children(ScrollHandle::new())
    }

    /// A fresh handle for a long list.
    #[allow(
        dead_code,
        reason = "the first long list is Search Everywhere's or Find in Files', after the seams"
    )]
    pub(crate) fn uniform() -> Self {
        Self::Uniform(UniformListScrollHandle::new())
    }

    /// Bring a row into view. **A list that scrolls needs the keyboard to
    /// scroll it too**: a highlight below the fold is a row taken blind.
    ///
    /// A uniform list moves only when the row is not wholly in view — its
    /// non-strict form; a scrolling `div` does the same by itself. **The
    /// strategy follows the direction the highlight went**: a non-strict
    /// scroll to a row below the fold still applies its strategy
    /// (`uniform_list.rs:414-416`), so `Top` alone would throw every row
    /// the down arrow reaches to the top of the view, a page at a time.
    fn show(&self, row: usize, from: Option<usize>) {
        match self {
            Self::Uniform(handle) => {
                let strategy = if from.is_some_and(|from| row > from) {
                    ScrollStrategy::Bottom
                } else {
                    ScrollStrategy::Top
                };
                handle.scroll_to_item(row, strategy)
            }
            Self::Children(handle) => handle.scroll_to_item(row),
        }
    }

    /// How many rows a page key moves, measured from the list on screen.
    fn page(&self, count: usize) -> isize {
        match self {
            Self::Uniform(handle) => {
                let state = handle.0.borrow();
                let (view, row) = state.last_item_size.map_or((0.0, 0.0), |size| {
                    (
                        f32::from(size.item.height),
                        f32::from(size.contents.height) / count.max(1) as f32,
                    )
                });
                crate::picker::page(view, row)
            }
            Self::Children(handle) => crate::picker::page(
                f32::from(handle.bounds().size.height),
                handle
                    .bounds_for_item(0)
                    .map(|bounds| f32::from(bounds.size.height))
                    .unwrap_or_default(),
            ),
        }
    }

    /// The scrollbar's thumb, from whichever handle holds the offset — a
    /// uniform list keeps its offset on its base handle, as a `div` does.
    pub(crate) fn thumb(&self) -> Option<(f32, f32)> {
        match self {
            Self::Uniform(handle) => crate::view::thumb(&handle.0.borrow().base_handle),
            Self::Children(handle) => crate::view::thumb(handle),
        }
    }
}

/// Work on a floating card's remembered place at the size it is drawn at.
///
/// `size` is a card's fixed size in pixels, or `None` for a card resized
/// by hand, whose remembered size is its size.
///
/// **A fixed card's size is the design system's, not the settings'**: it
/// is worked on in a copy, and only where it is — `at` — is written back.
/// Written through, every frame put section 6.25's pixels at the scale of
/// the moment into `settings.recent_files`, and the next settings write
/// saved them.
fn at_size<T>(
    place: &mut PickerPlace,
    size: Option<(f32, f32)>,
    work: impl FnOnce(&mut PickerPlace) -> T,
) -> T {
    let Some((width, height)) = size else {
        return work(place);
    };
    let mut drawn = PickerPlace {
        width,
        height,
        ..*place
    };
    let answer = work(&mut drawn);
    place.at = drawn.at;
    answer
}

impl Root {
    /// Put a popup up: every other field away, the focus taken, the caret
    /// solid, a row lit.
    ///
    /// **The focus it gives back is the one before any popup**: opening
    /// one popup over another inherits the first one's `previous`, or
    /// closing the second would focus a card nobody draws.
    pub(crate) fn open_picker(
        &mut self,
        holds: Holds,
        project: Option<ProjectId>,
        scroll: Scroll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = match self.picker.take() {
            Some(open) => open.previous,
            None => window.focused(cx),
        };
        self.put_fields_away();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.picker = Some(Open {
            holds,
            picker: Picker::default(),
            focus,
            previous,
            project,
            scroll,
            grab: None,
            place: None,
        });
        self.settle_picker();
        cx.notify();
    }

    /// Put the popup away.
    ///
    /// A place the hand was still dragging is written now — the release
    /// never came.
    pub(crate) fn close_picker(
        &mut self,
        focus: Focus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.picker.take() else {
            return;
        };
        if open.grab.is_some() {
            self.store_settings();
        }
        if focus == Focus::Back {
            match open.previous {
                Some(previous) => window.focus(&previous, cx),
                None => self.refocus(window, cx),
            }
        }
        cx.notify();
    }

    /// A key pressed while the card holds the focus. Answers whether the
    /// popup took it.
    ///
    /// The popup's own keys first — Recent Files' Backspace, a face's Tab
    /// — then what every popup does with a key.
    pub(crate) fn picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.picker.is_none() {
            return false;
        }
        let key = crate::picker::key(&event.keystroke);
        if self.holds_key(&key, &event.keystroke, window, cx) {
            return true;
        }
        match key {
            Key::Close => self.close_picker(Focus::Back, window, cx),
            Key::Take(take) => {
                if let Some(row) = self.picker.as_ref().and_then(|open| open.picker.lit()) {
                    self.picker_take(row, take, window, cx);
                }
            }
            Key::Step(by) => self.step_picker(by, cx),
            Key::Page(pages) => {
                let count = self.picker_lit().len();
                let page = self
                    .picker
                    .as_ref()
                    .map_or(1, |open| open.scroll.page(count));
                self.step_picker(pages * page, cx);
            }
            Key::Jump(to) => self.jump_picker(to, cx),
            Key::Edit(_) => return false,
            // A face or a history the popup does not have: still the
            // popup's key, and nobody else's.
            Key::NextTab | Key::PreviousTab | Key::History(_) => {}
            Key::Other => return false,
        }
        true
    }

    /// The pointer moved over a row: that row is lit.
    ///
    /// On a move and not on hover — hover is recomputed when the arrows
    /// scroll the list under a still pointer, and would drag the highlight
    /// straight back. No scroll: the row is under the pointer, so in view.
    pub(crate) fn picker_point(&mut self, row: usize, cx: &mut Context<Self>) {
        let rows = self.picker_lit();
        if let Some(open) = self.picker.as_mut()
            && open.picker.point_at(row, &rows)
        {
            cx.notify();
        }
    }

    /// Take a row — Return, `cmd-enter` or a click. A heading takes
    /// nothing, and the row taken is lit first so a click and Return
    /// agree about which row it was.
    pub(crate) fn picker_take(
        &mut self,
        row: usize,
        take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.picker_lit();
        if rows.get(row) != Some(&Lit::Row) {
            return;
        }
        if let Some(open) = self.picker.as_mut() {
            open.picker.point_at(row, &rows);
        }
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentProjects) => self.recent_projects_take(row, take, window, cx),
            Some(Holds::RecentFiles(_)) => self.recent_files_take(row, take, window, cx),
            Some(Holds::Run) => self.run_take(row, take, window, cx),
            Some(Holds::Branches(_)) => self.branches_take(row, take, window, cx),
            Some(Holds::Fonts(..)) => self.fonts_take(row, take, window, cx),
            Some(Holds::Everywhere(_)) => self.everywhere_take(row, take, window, cx),
            Some(Holds::FindInFiles(_)) => self.find_in_files_take(row, take, window, cx),
            None => {}
        }
    }

    /// What the popup's thread has said, on the watch loop's clock.
    pub(crate) fn poll_picker(&mut self, cx: &mut Context<Self>) {
        let changed = match self.picker.as_ref().map(|open| &open.holds) {
            // None of these has a thread of its own: the branch lists land
            // through `poll_git`, which settles the popup itself.
            Some(Holds::RecentProjects | Holds::Run | Holds::Branches(_) | Holds::Fonts(..))
            | None => false,
            Some(Holds::RecentFiles(_)) => self.recent_files_poll(),
            Some(Holds::Everywhere(_)) => self.everywhere_poll(cx),
            Some(Holds::FindInFiles(_)) => self.find_in_files_poll(cx),
        };
        if changed {
            self.settle_picker();
            cx.notify();
        }
    }

    /// A watcher batch, for the popup that is up.
    pub(crate) fn picker_heard(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        match self.picker.as_ref().map(|open| &open.holds) {
            // The recent projects are the workspace's, not the disk's; a
            // recent file deleted under the popup is a row that opens
            // nothing, which `open_file` already says.
            // The run list's declared commands are asked again by
            // `rediscover`, and the branches by the watcher's `.git` rule.
            Some(
                Holds::RecentProjects
                | Holds::RecentFiles(_)
                | Holds::Run
                | Holds::Branches(_)
                | Holds::Fonts(..),
            )
            | None => {}
            Some(Holds::Everywhere(_)) => self.everywhere_heard(project, batch),
            Some(Holds::FindInFiles(_)) => self.find_in_files_heard(project, batch),
        }
    }

    /// Whether the popup's own card holds the focus — so the frame after
    /// it opened does not take the focus back to the root.
    pub(crate) fn picker_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.picker
            .as_ref()
            .is_some_and(|open| open.focus.contains_focused(window, cx))
    }

    /// The popup's rows, as far as lighting them goes.
    pub(crate) fn picker_lit(&self) -> Vec<Lit> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentProjects) => self.recent_projects_lit(),
            Some(Holds::RecentFiles(_)) => self.recent_files_lit(),
            Some(Holds::Run) => vec![Lit::Row; self.run_rows().len()],
            Some(Holds::Branches(_)) => self.branches_lit(),
            Some(Holds::Fonts(..)) => vec![Lit::Row; self.font_rows().len()],
            Some(Holds::Everywhere(_)) => self.everywhere_lit(),
            Some(Holds::FindInFiles(_)) => self.find_in_files_lit(),
            None => Vec::new(),
        }
    }

    /// Whether the popup's card still has what it hangs from.
    ///
    /// **A hanging card is drawn by the picker, not by its button**, so a
    /// list whose button has gone — the Output panel closed, Settings left
    /// for another section, a project with no branch — would stand on at
    /// the last place its button was, holding the focus. It is closed
    /// instead (`root::render`), which is what not drawing it used to do
    /// by accident, minus the focus nobody drew.
    pub(crate) fn picker_anchored(&self) -> bool {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::Run) => self.tool_is_up(guild_session::Tool::Output),
            Some(Holds::Branches(_)) => self.branch.is_some(),
            Some(Holds::Fonts(..)) => self.font_list_drawn(),
            // The two of M8.9 float; they hang from nothing.
            Some(
                Holds::RecentProjects
                | Holds::RecentFiles(_)
                | Holds::Everywhere(_)
                | Holds::FindInFiles(_),
            )
            | None => true,
        }
    }

    /// Do a deed that may close the popup through a door with no window —
    /// `switch_branch` and its kind close it with `cx` alone — and give the
    /// focus back to what the popup was opened over if it did.
    pub(crate) fn picker_deed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        deed: impl FnOnce(&mut Self, &mut Context<Self>),
    ) {
        let previous = self.picker.as_ref().and_then(|open| open.previous.clone());
        deed(self, cx);
        if self.picker.is_none() {
            match previous {
                Some(previous) => window.focus(&previous, cx),
                None => self.refocus(window, cx),
            }
        }
    }

    /// The lit row, if a popup is up and has one.
    pub(crate) fn picker_lit_row(&self) -> Option<usize> {
        self.picker.as_ref().and_then(|open| open.picker.lit())
    }

    /// What has been typed into the popup that is up.
    pub(crate) fn picker_query(&self) -> &str {
        self.picker.as_ref().map_or("", |open| open.picker.query())
    }

    /// The popup's own keys.
    ///
    /// The stroke rides beside what it means for the popups whose key is a
    /// key and not a meaning: Recent Files closes a file on the Backspace
    /// key itself, and `ctrl-w` means the same edit as `alt-backspace`
    /// without meaning *close*; Find in Files turns a toggle on `alt-c`,
    /// which means `ç` to the line.
    fn holds_key(
        &mut self,
        key: &Key,
        stroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentProjects) => self.recent_projects_key(key, window, cx),
            Some(Holds::RecentFiles(_)) => self.recent_files_key(stroke, window, cx),
            Some(Holds::Branches(_)) => self.branches_key(key, window, cx),
            // The run list's typed first row is a row of `run::rows`, not
            // a key; the font list has none either.
            Some(Holds::Run | Holds::Fonts(..)) => false,
            Some(Holds::Everywhere(_)) => self.everywhere_key(key, window, cx),
            Some(Holds::FindInFiles(_)) => self.find_in_files_key(key, stroke, window, cx),
            None => false,
        }
    }

    /// What an edit of the query does beyond narrowing.
    pub(crate) fn holds_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentProjects) => self.recent_projects_edited(window, cx),
            Some(Holds::RecentFiles(_)) => self.recent_files_edited(window, cx),
            Some(Holds::Branches(_)) => self.branches_edited(window, cx),
            Some(Holds::Everywhere(_)) => self.everywhere_edited(window, cx),
            Some(Holds::FindInFiles(_)) => self.find_in_files_edited(window, cx),
            Some(Holds::Run | Holds::Fonts(..)) | None => {}
        }
    }

    /// The row a popup opens on — its own rule, or the first row.
    fn picker_opening(&self) -> usize {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentFiles(_)) => self.recent_files_opening(),
            Some(Holds::Branches(crate::root::branches::Face::Sources)) => {
                self.update_sources_opening()
            }
            Some(
                Holds::RecentProjects
                | Holds::Run
                | Holds::Branches(_)
                | Holds::Fonts(..)
                | Holds::Everywhere(_)
                | Holds::FindInFiles(_),
            )
            | None => 0,
        }
    }

    /// Keep a row lit in the list as it now is, and in view.
    pub(crate) fn settle_picker(&mut self) {
        let rows = self.picker_lit();
        let opening = self.picker_opening();
        if let Some(open) = self.picker.as_mut() {
            open.picker.settle(&rows, opening);
            if let Some(row) = open.picker.lit() {
                open.scroll.show(row, None);
            }
        }
    }

    fn step_picker(&mut self, by: isize, cx: &mut Context<Self>) {
        let rows = self.picker_lit();
        if let Some(open) = self.picker.as_mut() {
            let from = open.picker.lit();
            open.picker.step(by, &rows);
            if let Some(row) = open.picker.lit() {
                open.scroll.show(row, from);
            }
        }
        cx.notify();
    }

    fn jump_picker(&mut self, to: Jump, cx: &mut Context<Self>) {
        let rows = self.picker_lit();
        if let Some(open) = self.picker.as_mut() {
            let from = open.picker.lit();
            open.picker.jump(to, &rows);
            if let Some(row) = open.picker.lit() {
                open.scroll.show(row, from);
            }
        }
        cx.notify();
    }

    /// Is a floating card being dragged right now?
    pub(crate) fn picker_held(&self) -> bool {
        self.picker.as_ref().is_some_and(|open| open.grab.is_some())
    }

    /// Take hold of the card — its head, an edge, or a corner.
    pub(crate) fn grab_picker(&mut self, grab: Grab, at: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(open) = self.picker.as_mut() {
            open.grab = Some((grab, at));
            cx.notify();
        }
    }

    /// Follow the hand, measured against where the pointer was last seen
    /// rather than accumulated from where the drag began: a dropped frame
    /// would otherwise leave the card somewhere the hand is not.
    ///
    /// **Written through the card's own settings field**, so no drag has
    /// to know which popup it is moving.
    pub(crate) fn drag_picker(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(open) = self.picker.as_mut() else {
            return;
        };
        let (
            Some((grab, from)),
            Some(Place::Floating {
                remembered,
                top,
                size,
            }),
        ) = (open.grab, open.place)
        else {
            return;
        };
        let by = (f32::from(at.x - from.x), f32::from(at.y - from.y));
        if by.0 == 0.0 && by.1 == 0.0 {
            return;
        }
        open.grab = Some((grab, at));
        let window = (
            f32::from(self.viewport.width),
            f32::from(self.viewport.height),
        );
        let rem = f32::from(crate::scale::rem_size(self.settings.ui_font_size));
        let size = size.map(|(width, height)| (width.0 * rem, height.0 * rem));
        at_size(remembered(&mut self.settings), size, |place| {
            place.drag(grab, by, window, top)
        });
        cx.notify();
    }

    /// The hand let go: the place is written once, here, rather than on
    /// every pointer event — a drag is a hundred events.
    pub(crate) fn drop_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.picker.as_mut()
            && open.grab.take().is_some()
        {
            self.store_settings();
            cx.notify();
        }
    }

    /// Where a floating card is in this window, brought back inside it
    /// first: left, top, width and height in pixels.
    pub(crate) fn picker_frame(&mut self, place: Place) -> Option<(f32, f32, f32, f32)> {
        let Place::Floating {
            remembered,
            top,
            size,
        } = place
        else {
            return None;
        };
        let window = (
            f32::from(self.viewport.width),
            f32::from(self.viewport.height),
        );
        let rem = f32::from(crate::scale::rem_size(self.settings.ui_font_size));
        let size = size.map(|(width, height)| (width.0 * rem, height.0 * rem));
        Some(at_size(remembered(&mut self.settings), size, |place| {
            place.settle(window, top);
            let (x, y) = place.place(window, top);
            (x, y, place.width, place.height)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A card of a fixed size remembers where it is, not how big.**
    /// Recent Files is drawn at section 6.25's width and greatest height,
    /// and its drag and its settle work at that size — which until the
    /// review were written into `settings.recent_files` on every frame and
    /// saved with the next settings write, at whatever scale was last used.
    /// A card resized by hand keeps what the hand did.
    #[test]
    fn a_fixed_card_remembers_where_it_is_not_how_big() {
        let window = (1600.0, 1000.0);
        let mut place = PickerPlace::default();
        let (width, height) = (place.width, place.height);

        at_size(&mut place, Some((560.0, 520.0)), |place| {
            place.drag(Grab::Move, (40.0, 30.0), window, 120.0)
        });
        let framed = at_size(&mut place, Some((560.0, 520.0)), |place| {
            place.settle(window, 120.0);
            (place.width, place.height)
        });

        assert_eq!(
            framed,
            (560.0, 520.0),
            "worked on at the size it is drawn at"
        );
        assert_eq!(
            place.at.map(|spot| (spot.x, spot.y)),
            Some(((1600.0 - 560.0) / 2.0 + 40.0, 150.0)),
            "the move is remembered, from where the drawn card was"
        );
        assert_eq!(
            (place.width, place.height),
            (width, height),
            "the size is not"
        );

        let mut resized = PickerPlace::default();
        at_size(&mut resized, None, |place| {
            place.drag(Grab::Right, (50.0, 0.0), window, 64.0)
        });
        assert_eq!(resized.width, width + 50.0, "a hand's resize is kept");
    }
}
