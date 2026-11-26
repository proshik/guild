//! A seat's areas — M12.8. The screen of one seat divides into areas,
//! each with its own strip of tabs; this is the division and the area
//! that has the keys. Invariant 1 is kept here: the last area stays.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::layout::{Axis, Tree};

/// An area's identity, for as long as its seat keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AreaId(Uuid);

impl AreaId {
    /// A fresh one.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AreaId {
    fn default() -> Self {
        Self::new()
    }
}

/// Closing the last area is refused — invariant 1. An id that is not the
/// seat's is refused the same way: there is nothing to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastArea;

/// How a seat's screen is divided, and which area has the keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Areas {
    tree: Tree<AreaId>,
    focused: AreaId,
}

impl Areas {
    /// One area, the whole screen: every seat until it is divided.
    pub fn one() -> Self {
        let id = AreaId::new();
        Self {
            tree: Tree::leaf(id),
            focused: id,
        }
    }

    /// Every area, in draw order.
    pub fn ids(&self) -> Vec<AreaId> {
        self.tree.leaves()
    }

    /// The area with the keys.
    pub fn focused(&self) -> AreaId {
        self.focused
    }

    /// Whether `area` is one of this seat's.
    pub fn contains(&self, area: AreaId) -> bool {
        self.tree.contains(&area)
    }

    /// Give `area` the keys. Answers whether it is one of this seat's.
    pub fn focus(&mut self, area: AreaId) -> bool {
        let known = self.contains(area);
        if known {
            self.focused = area;
        }
        known
    }

    /// Divide `target`, putting a new, empty area on its second side and
    /// the keys in it.
    pub fn split(&mut self, target: AreaId, axis: Axis) -> Option<AreaId> {
        let new = AreaId::new();
        if !self.tree.split_leaf(&target, axis, new) {
            return None;
        }
        self.focused = new;
        Some(new)
    }

    /// Close `target`, answering its heir — the area that takes its room
    /// and its tabs (decision 9), and the keys when `target` had them.
    pub fn close(&mut self, target: AreaId) -> Result<AreaId, LastArea> {
        let heir = self.tree.heir(&target).ok_or(LastArea)?;
        self.tree.remove_leaf(&target);
        if self.focused == target {
            self.focused = heir;
        }
        Ok(heir)
    }

    /// Put right what a hand-edited file may say: a focus that names no
    /// area goes to the first.
    pub fn normalised(mut self) -> Self {
        if !self.contains(self.focused) {
            self.focused = self.ids()[0];
        }
        self
    }

    /// The division, for measuring.
    pub fn tree(&self) -> &Tree<AreaId> {
        &self.tree
    }

    /// The division, for a dragged border.
    pub fn tree_mut(&mut self) -> &mut Tree<AreaId> {
        &mut self.tree
    }
}

/// One area's strip as the state file keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AreaState {
    /// Its tabs, in strip order.
    #[serde(default)]
    pub tabs: Vec<crate::project_state::OpenTab>,
    /// The tab in front, by position.
    #[serde(default)]
    pub active: Option<usize>,
}

/// A divided seat as the state file keeps it: the division, and each
/// area's strip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedAreas {
    /// The division and the area with the keys.
    pub areas: Areas,
    /// Each area's strip; an area with none is empty.
    #[serde(default)]
    pub strips: std::collections::BTreeMap<AreaId, AreaState>,
}

impl SavedAreas {
    /// One area holding `tabs`: every seat until it is divided.
    pub fn one(tabs: Vec<crate::project_state::OpenTab>, active: Option<usize>) -> Self {
        let areas = Areas::one();
        let mut strips = std::collections::BTreeMap::new();
        strips.insert(areas.focused(), AreaState { tabs, active });
        Self { areas, strips }
    }

    /// `area`'s strip, if it is one of this seat's and has one.
    pub fn strip(&self, area: AreaId) -> Option<&AreaState> {
        self.strips.get(&area)
    }

    /// `area`'s strip, made empty when it has none.
    pub fn strip_mut(&mut self, area: AreaId) -> &mut AreaState {
        self.strips.entry(area).or_default()
    }

    /// What the old fields keep: the focused area's strip when it has
    /// tabs, else the first in draw order that has, else an empty one.
    pub fn mirror(&self) -> AreaState {
        let has = |id: &AreaId| self.strips.get(id).is_some_and(|s| !s.tabs.is_empty());
        let chosen = if has(&self.areas.focused()) {
            Some(self.areas.focused())
        } else {
            self.areas.ids().into_iter().find(has)
        };
        chosen
            .and_then(|id| self.strips.get(&id).cloned())
            .unwrap_or_default()
    }

    /// Put right what a hand-edited file may say: the division
    /// normalised, a strip for every area, and none for an area that is
    /// not there.
    pub fn normalised(mut self) -> Self {
        self.areas = self.areas.normalised();
        let areas = &self.areas;
        self.strips.retain(|id, _| areas.contains(*id));
        for id in self.areas.ids() {
            self.strips.entry(id).or_default();
        }
        self
    }
}
