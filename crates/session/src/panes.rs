//! A tab's panes — M12.8. A tab divides into panes; each shows one body or
//! nothing (decision 13), and one has the keys. The same model serves the
//! state file (`B` is `Shows`) and the window (`B` is `Body`), so no bound
//! is put on `B` beyond what a single method needs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::layout::{Axis, Tree};

/// A pane's identity, for as long as its tab keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaneId(Uuid);

impl PaneId {
    /// A fresh one.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PaneId {
    fn default() -> Self {
        Self::new()
    }
}

/// What closing a pane came to.
#[derive(Debug, PartialEq)]
pub enum Closed<B> {
    /// The pane went, and this is what it showed.
    Pane(Option<B>),
    /// It was the tab's last pane that showed anything: the tab closes,
    /// and nothing here has changed — invariant 3.
    Tab,
    /// `pane` is not one of this tab's — a stale id. Nothing has changed,
    /// and the tab is not to be closed on its account.
    Missing,
}

/// How a tab is divided, what each pane shows, and which has the keys.
///
/// **A pane with no body is empty** — decision 13 — and shows the start
/// screen. `B` is `Shows` in the state file and `Body` in the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panes<B> {
    tree: Tree<PaneId>,
    showing: BTreeMap<PaneId, B>,
    focused: PaneId,
}

impl<B> Panes<B> {
    /// One pane showing `body`, focused: every tab until it is divided.
    pub fn one(body: B) -> Self {
        let id = PaneId::new();
        let mut showing = BTreeMap::new();
        showing.insert(id, body);
        Self {
            tree: Tree::leaf(id),
            showing,
            focused: id,
        }
    }

    /// Every pane, in draw order.
    pub fn ids(&self) -> Vec<PaneId> {
        self.tree.leaves()
    }

    /// The pane with the keys.
    pub fn focused(&self) -> PaneId {
        self.focused
    }

    /// Give `pane` the keys. Answers whether it is one of this tab's.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        let known = self.tree.contains(&pane);
        if known {
            self.focused = pane;
        }
        known
    }

    /// What the focused pane shows, if anything.
    pub fn body(&self) -> Option<&B> {
        self.showing.get(&self.focused)
    }

    /// What `pane` shows, if it is here and shows anything.
    pub fn body_of(&self, pane: PaneId) -> Option<&B> {
        self.showing.get(&pane)
    }

    /// What the focused pane shows, to change it in place.
    pub fn body_mut(&mut self) -> Option<&mut B> {
        self.showing.get_mut(&self.focused)
    }

    /// Every body with its pane, in draw order.
    pub fn bodies(&self) -> Vec<(PaneId, &B)> {
        self.ids()
            .into_iter()
            .filter_map(|id| self.showing.get(&id).map(|body| (id, body)))
            .collect()
    }

    /// How many panes show something.
    pub fn showing(&self) -> usize {
        self.showing.len()
    }

    /// Divide `target`, putting a new, **empty** pane on its second side
    /// and the keys in it — decision 13.
    pub fn split(&mut self, target: PaneId, axis: Axis) -> Option<PaneId> {
        let new = PaneId::new();
        if !self.tree.split_leaf(&target, axis, new) {
            return None;
        }
        self.focused = new;
        Some(new)
    }

    /// Show `body` in `pane`. `Err(body)` when `pane` is not here or
    /// already shows something: a pane that shows something is kept.
    pub fn fill(&mut self, pane: PaneId, body: B) -> Result<(), B> {
        if !self.tree.contains(&pane) || self.showing.contains_key(&pane) {
            return Err(body);
        }
        self.showing.insert(pane, body);
        Ok(())
    }

    /// Take what `pane` shows out, leaving it empty.
    pub fn take(&mut self, pane: PaneId) -> Option<B> {
        self.showing.remove(&pane)
    }

    /// Close `pane`. `Closed::Tab` when it is the only pane or the only
    /// one showing anything — the model is then left as it was, because
    /// closing the tab is the window's. Otherwise the pane goes with what
    /// it showed, and its heir takes the keys if it had them. A pane that
    /// is not here answers `Closed::Missing` and changes nothing. `showing`
    /// is trusted to hold only ids of the tree, which `normalised`
    /// guarantees for a deserialised value.
    pub fn close(&mut self, pane: PaneId) -> Closed<B> {
        if !self.tree.contains(&pane) {
            return Closed::Missing;
        }
        let Some(heir) = self.tree.heir(&pane) else {
            return Closed::Tab;
        };
        if self.showing.len() == 1 && self.showing.contains_key(&pane) {
            return Closed::Tab;
        }
        self.tree.remove_leaf(&pane);
        let body = self.showing.remove(&pane);
        if self.focused == pane {
            self.focused = heir;
        }
        Closed::Pane(body)
    }

    /// Transfer selected occupied panes, preserving identities and bodies. The
    /// original must retain at least one body; moving its whole tab is the
    /// caller's existing operation. Empty panes stay with the original.
    pub fn extract(&mut self, selected: &[PaneId]) -> Option<Self> {
        let ids: Vec<_> = self
            .ids()
            .into_iter()
            .filter(|id| selected.contains(id) && self.showing.contains_key(id))
            .collect();
        if ids.is_empty() || ids.len() == self.showing.len() {
            return None;
        }
        let mut tree = self.tree.clone();
        for id in self.ids() {
            if !ids.contains(&id) {
                tree.remove_leaf(&id);
            }
        }
        let focused = if ids.contains(&self.focused) {
            self.focused
        } else {
            ids[0]
        };
        let mut showing = BTreeMap::new();
        for id in ids {
            showing.insert(
                id,
                self.showing.remove(&id).expect("selected occupied pane"),
            );
            let heir = self
                .tree
                .heir(&id)
                .expect("original retains an occupied pane");
            self.tree.remove_leaf(&id);
            if self.focused == id {
                self.focused = heir;
            }
        }
        Some(Self {
            tree,
            showing,
            focused,
        })
    }

    /// The first pane, in draw order, whose body satisfies `pred`.
    pub fn find(&self, pred: impl Fn(&B) -> bool) -> Option<PaneId> {
        self.bodies()
            .into_iter()
            .find(|(_, body)| pred(body))
            .map(|(id, _)| id)
    }

    /// Every body through `f`. A pane whose answer is `None` is removed;
    /// empty panes stay. `None` when nothing is left showing. The focus
    /// goes to the first pane when its own was removed.
    pub fn map<C>(&self, mut f: impl FnMut(&B) -> Option<C>) -> Option<Panes<C>> {
        let mut tree = self.tree.clone();
        let mut showing = BTreeMap::new();
        for id in self.ids() {
            if let Some(body) = self.showing.get(&id) {
                match f(body) {
                    Some(mapped) => {
                        showing.insert(id, mapped);
                    }
                    None => {
                        tree.remove_leaf(&id);
                    }
                }
            }
        }
        if showing.is_empty() {
            return None;
        }
        let focused = if tree.contains(&self.focused) {
            self.focused
        } else {
            tree.leaves()[0]
        };
        Some(Panes {
            tree,
            showing,
            focused,
        })
    }

    /// The same division and focus, nothing shown — the shape to pour
    /// bodies into on a restore.
    pub fn emptied<C>(&self) -> Panes<C> {
        Panes {
            tree: self.tree.clone(),
            showing: BTreeMap::new(),
            focused: self.focused,
        }
    }

    /// Put right what a hand-edited file may say: bodies of panes that
    /// are not in the tree are dropped, and a focus that names none goes
    /// to the first pane. It does **not** restore invariant 3: a
    /// deserialised value may show nothing at all, and the caller must
    /// drop such a tab.
    pub fn normalised(mut self) -> Self {
        let tree = &self.tree;
        self.showing.retain(|id, _| tree.contains(id));
        if !self.tree.contains(&self.focused) {
            self.focused = self.tree.leaves()[0];
        }
        self
    }

    /// The division, for measuring.
    pub fn tree(&self) -> &Tree<PaneId> {
        &self.tree
    }

    /// The division, for a dragged border.
    pub fn tree_mut(&mut self) -> &mut Tree<PaneId> {
        &mut self.tree
    }
}
