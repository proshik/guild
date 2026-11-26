//! Which tab was in front of each seat — a project, or a worktree, as one
//! workspace holds it. Pure: the window says which tab is in front and
//! what the strip is, this remembers and answers. 2026-09-26.
//!
//! **Coming back to a seat brings forward the tab the person left.** Every
//! way into a project — the column, `ctrl-tab`, `cmd-1`, Recent Projects,
//! a workspace switch — picked the strip's last, so four tabs with the
//! second in front came back with the fourth. The window had one
//! `active_tab` and nothing per seat; `SeatState.active` on disk had the
//! place for it, and `store_tabs` wrote `None` there for every project but
//! the one in front.
//!
//! Since M12.8 it is keyed by whatever the caller keys it by: the stage
//! keys it by an area, so each area of a seat has its own tab in front.

use std::collections::HashMap;
use std::hash::Hash;

/// The tab in front of each seat, as it stood when the person was last
/// there.
#[derive(Debug)]
pub struct Fronts<S, T> {
    by_seat: HashMap<S, T>,
}

impl<S, T> Default for Fronts<S, T> {
    fn default() -> Self {
        Self {
            by_seat: HashMap::new(),
        }
    }
}

impl<S: Hash + Eq + Copy, T: Copy + Eq> Fronts<S, T> {
    /// `tab` is in front of `seat`.
    pub fn remember(&mut self, seat: S, tab: T) {
        self.by_seat.insert(seat, tab);
    }

    /// The tab that was in front of `seat`, if it is still on `strip`.
    pub fn of(&self, seat: S, strip: &[T]) -> Option<T> {
        self.by_seat
            .get(&seat)
            .copied()
            .filter(|tab| strip.contains(tab))
    }

    /// The tab that comes forward when the person comes back to `seat`:
    /// the one they left, else the strip's last — a seat never looked at
    /// has nothing better to offer.
    pub fn returning(&self, seat: S, strip: &[T]) -> Option<T> {
        self.of(seat, strip).or_else(|| strip.last().copied())
    }

    /// `closing` leaves `seat`'s `strip`, read before it goes. When it was
    /// the remembered one, its neighbour is remembered instead — the rule
    /// `cmd-w` follows for the tab in front, so a tab closed while the
    /// person was elsewhere comes back as the one closed in front would.
    pub fn close(&mut self, seat: S, strip: &[T], closing: T) {
        if self.by_seat.get(&seat) != Some(&closing) {
            return;
        }
        match crate::closing::neighbour(strip, closing) {
            Some(tab) => self.remember(seat, tab),
            None => self.forget(seat),
        }
    }

    /// The tab remembered in front of `seat`, whether or not it is still
    /// on a strip — what the stage lends away when a tab is carried in.
    pub fn held(&self, seat: S) -> Option<T> {
        self.by_seat.get(&seat).copied()
    }

    /// The seat moved — a root carried to another workspace — and its
    /// front goes with it.
    pub fn moved(&mut self, from: S, to: S) {
        if let Some(tab) = self.by_seat.remove(&from) {
            self.by_seat.insert(to, tab);
        }
    }

    /// The seat is gone.
    pub fn forget(&mut self, seat: S) {
        self.by_seat.remove(&seat);
    }
}
