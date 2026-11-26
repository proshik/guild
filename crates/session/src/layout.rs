//! The division of a surface — M12.8 decision 1. One tree serves the
//! seat's areas and a tab's panes: a node is an axis, a dragged fraction
//! and two children; a leaf is an **id**, and what it stands for lives in
//! a map beside the tree (decision 3). A border is named by its path from
//! the root (decision 12).

use serde::{Deserialize, Serialize};

use crate::split::Split;

/// Which way a node divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Side by side: the first child is the left one.
    Horizontal,
    /// Stacked: the first child is the upper one.
    Vertical,
}

/// A rectangle in pixels, its origin at the top left of what is divided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Its left edge.
    pub x: f32,
    /// Its top edge.
    pub y: f32,
    /// How wide it is.
    pub width: f32,
    /// How tall it is.
    pub height: f32,
}

impl Frame {
    /// A frame from its corner and its size.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// What a place at this frame, one of `room`'s, draws in: the frame
    /// less a `line` wide hairline at each border it has on its left or
    /// its top — M12.8's second window check. A border's line is drawn at
    /// [`Border::at`], which is the second side's first point, so without
    /// this the second side's outline stood a point nearer its line than
    /// the first side's; an edge of the room has no line.
    pub fn inside_lines(&self, room: Frame, line: f32) -> Frame {
        let mut inside = *self;
        if self.x > room.x {
            inside.x += line;
            inside.width = (inside.width - line).max(0.0);
        }
        if self.y > room.y {
            inside.y += line;
            inside.height = (inside.height - line).max(0.0);
        }
        inside
    }

    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// The least a side is made while both sides of a division fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floor {
    /// Across, for a side-by-side division.
    pub width: f32,
    /// Down, for a stacked one.
    pub height: f32,
}

/// Which child of a node, on the way from the root to a border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The left or upper child.
    First,
    /// The right or lower child.
    Second,
}

/// The border between a node's two children.
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    /// The node's place from the root — the border's name for a drag.
    pub path: Vec<Side>,
    /// The node's axis: `Horizontal` is a vertical line dragged sideways.
    pub axis: Axis,
    /// Where the line is, along the axis.
    pub at: f32,
    /// The node's frame, which the line runs across.
    pub span: Frame,
}

/// A way to move from one place to its neighbour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards the left edge.
    Left,
    /// Towards the right edge.
    Right,
    /// Towards the top.
    Up,
    /// Towards the bottom.
    Down,
}

/// A division of a surface, or a leaf standing for what fills it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tree<L> {
    /// What fills a part, by its id.
    Leaf(L),
    /// A part divided in two.
    Split {
        /// Which way it is divided.
        axis: Axis,
        /// The share the first side takes.
        #[serde(default)]
        split: Split,
        /// The left or upper side.
        first: Box<Tree<L>>,
        /// The right or lower side.
        second: Box<Tree<L>>,
    },
}

impl<L: Clone + PartialEq> Tree<L> {
    /// One leaf: the whole surface.
    pub fn leaf(leaf: L) -> Self {
        Tree::Leaf(leaf)
    }

    /// Every leaf, in the order the window draws them: first side before
    /// second, depth first.
    pub fn leaves(&self) -> Vec<L> {
        let mut out = Vec::new();
        self.walk(&mut out);
        out
    }

    fn walk(&self, out: &mut Vec<L>) {
        match self {
            Tree::Leaf(leaf) => out.push(leaf.clone()),
            Tree::Split { first, second, .. } => {
                first.walk(out);
                second.walk(out);
            }
        }
    }

    /// Whether `leaf` is one of this tree's.
    pub fn contains(&self, leaf: &L) -> bool {
        match self {
            Tree::Leaf(own) => own == leaf,
            Tree::Split { first, second, .. } => first.contains(leaf) || second.contains(leaf),
        }
    }

    /// Divide `target` along `axis`, putting `new` on the second side.
    /// Answers whether the target was there.
    pub fn split_leaf(&mut self, target: &L, axis: Axis, new: L) -> bool {
        match self {
            Tree::Leaf(leaf) if leaf == target => {
                let first = Box::new(Tree::Leaf(leaf.clone()));
                *self = Tree::Split {
                    axis,
                    split: Split::default(),
                    first,
                    second: Box::new(Tree::Leaf(new)),
                };
                true
            }
            Tree::Leaf(_) => false,
            Tree::Split { first, second, .. } => {
                first.split_leaf(target, axis, new.clone()) || second.split_leaf(target, axis, new)
            }
        }
    }

    /// The leaf that takes `target`'s room when it goes: the nearest leaf
    /// of its sibling. `None` for the only leaf, or one not here.
    pub fn heir(&self, target: &L) -> Option<L> {
        match self {
            Tree::Leaf(_) => None,
            Tree::Split { first, second, .. } => {
                if matches!(&**first, Tree::Leaf(leaf) if leaf == target) {
                    return second.leaves().first().cloned();
                }
                if matches!(&**second, Tree::Leaf(leaf) if leaf == target) {
                    return first.leaves().last().cloned();
                }
                first.heir(target).or_else(|| second.heir(target))
            }
        }
    }

    /// Take `target` out, and collapse the node it was half of into its
    /// surviving side. The last leaf is **kept**: invariants 1 and 3.
    pub fn remove_leaf(&mut self, target: &L) -> bool {
        if matches!(self, Tree::Leaf(_)) {
            return false;
        }
        self.remove_within(target)
    }

    fn remove_within(&mut self, target: &L) -> bool {
        let surviving = match self {
            Tree::Leaf(_) => return false,
            Tree::Split { first, second, .. } => {
                if matches!(&**first, Tree::Leaf(leaf) if leaf == target) {
                    Some((**second).clone())
                } else if matches!(&**second, Tree::Leaf(leaf) if leaf == target) {
                    Some((**first).clone())
                } else {
                    None
                }
            }
        };
        if let Some(surviving) = surviving {
            *self = surviving;
            return true;
        }
        match self {
            Tree::Leaf(_) => false,
            Tree::Split { first, second, .. } => {
                first.remove_within(target) || second.remove_within(target)
            }
        }
    }

    /// Where each leaf lands inside `total`, in draw order, no side under
    /// `floor` while both fit — `Split::sizes`' rule, down the tree.
    pub fn frames(&self, total: Frame, floor: Floor) -> Vec<(L, Frame)> {
        let mut out = Vec::new();
        self.measure(total, floor, &mut out);
        out
    }

    fn measure(&self, total: Frame, floor: Floor, out: &mut Vec<(L, Frame)>) {
        match self {
            Tree::Leaf(leaf) => out.push((leaf.clone(), total)),
            Tree::Split {
                axis,
                split,
                first,
                second,
            } => {
                let (one, two) = halves(*axis, *split, first, second, total, floor);
                first.measure(one, floor, out);
                second.measure(two, floor, out);
            }
        }
    }

    /// Every border inside `total`, outermost first.
    pub fn borders(&self, total: Frame, floor: Floor) -> Vec<Border> {
        let mut out = Vec::new();
        self.collect(total, floor, &mut Vec::new(), &mut out);
        out
    }

    fn collect(&self, total: Frame, floor: Floor, path: &mut Vec<Side>, out: &mut Vec<Border>) {
        if let Tree::Split {
            axis,
            split,
            first,
            second,
        } = self
        {
            let (one, two) = halves(*axis, *split, first, second, total, floor);
            let at = match axis {
                Axis::Horizontal => two.x,
                Axis::Vertical => two.y,
            };
            out.push(Border {
                path: path.clone(),
                axis: *axis,
                at,
                span: total,
            });
            path.push(Side::First);
            first.collect(one, floor, path, out);
            path.pop();
            path.push(Side::Second);
            second.collect(two, floor, path, out);
            path.pop();
        }
    }

    /// Move the border at `path` by `delta` pixels along its axis, no
    /// side under `floor`. Answers whether `path` named a border.
    pub fn drag(&mut self, path: &[Side], delta: f32, total: Frame, floor: Floor) -> bool {
        let Tree::Split {
            axis,
            split,
            first,
            second,
        } = self
        else {
            return false;
        };
        match path.split_first() {
            None => {
                let (length, _) = along(*axis, total, floor);
                let (least_first, least_second) = floors(*axis, first, second, floor);
                split.drag_between(delta, length, least_first, least_second);
                true
            }
            Some((side, rest)) => {
                let (one, two) = halves(*axis, *split, first, second, total, floor);
                match side {
                    Side::First => first.drag(rest, delta, one, floor),
                    Side::Second => second.drag(rest, delta, two, floor),
                }
            }
        }
    }

    /// The split of the border at `path` — what a hand takes hold of, so
    /// that the drag is measured from it ([`Tree::drag_from`]). `None`
    /// where `path` names no border.
    pub fn split_at(&self, path: &[Side]) -> Option<Split> {
        let Tree::Split {
            split,
            first,
            second,
            ..
        } = self
        else {
            return None;
        };
        match path.split_first() {
            None => Some(*split),
            Some((Side::First, rest)) => first.split_at(rest),
            Some((Side::Second, rest)) => second.split_at(rest),
        }
    }

    /// Put the border at `path` where it was taken hold of, `held`, moved
    /// by `delta` — the hand's whole way from the press, not its last
    /// step. Followed step by step, each step's few pixels were rounded
    /// away with the split's hundredth and then forgotten, and a slow hand
    /// never moved the border at all (M12.8's third window check). Answers
    /// whether `path` named a border.
    pub fn drag_from(
        &mut self,
        path: &[Side],
        held: Split,
        delta: f32,
        total: Frame,
        floor: Floor,
    ) -> bool {
        if !self.set_split(path, held) {
            return false;
        }
        self.drag(path, delta, total, floor)
    }

    fn set_split(&mut self, path: &[Side], to: Split) -> bool {
        let Tree::Split {
            split,
            first,
            second,
            ..
        } = self
        else {
            return false;
        };
        match path.split_first() {
            None => {
                *split = to;
                true
            }
            Some((Side::First, rest)) => first.set_split(rest, to),
            Some((Side::Second, rest)) => second.set_split(rest, to),
        }
    }

    /// Put the border at `path` back at a half — a double click on it.
    pub fn reset(&mut self, path: &[Side]) -> bool {
        let Tree::Split {
            split,
            first,
            second,
            ..
        } = self
        else {
            return false;
        };
        match path.split_first() {
            None => {
                *split = Split::default();
                true
            }
            Some((Side::First, rest)) => first.reset(rest),
            Some((Side::Second, rest)) => second.reset(rest),
        }
    }
}

fn along(axis: Axis, total: Frame, floor: Floor) -> (f32, f32) {
    match axis {
        Axis::Horizontal => (total.width, floor.width),
        Axis::Vertical => (total.height, floor.height),
    }
}

/// What a subtree needs, as (width, height): a leaf needs one floor; a
/// division needs the sum of its sides' along its axis and the larger of
/// them across it. A node's floor is its subtree's.
fn need<L>(tree: &Tree<L>, floor: Floor) -> (f32, f32) {
    match tree {
        Tree::Leaf(_) => (floor.width, floor.height),
        Tree::Split {
            axis,
            first,
            second,
            ..
        } => {
            let (aw, ah) = need(first, floor);
            let (bw, bh) = need(second, floor);
            match axis {
                Axis::Horizontal => (aw + bw, ah.max(bh)),
                Axis::Vertical => (aw.max(bw), ah + bh),
            }
        }
    }
}

/// What each side of a node needs along the node's axis.
fn floors<L>(axis: Axis, first: &Tree<L>, second: &Tree<L>, floor: Floor) -> (f32, f32) {
    let (aw, ah) = need(first, floor);
    let (bw, bh) = need(second, floor);
    match axis {
        Axis::Horizontal => (aw, bw),
        Axis::Vertical => (ah, bh),
    }
}

fn halves<L>(
    axis: Axis,
    split: Split,
    first: &Tree<L>,
    second: &Tree<L>,
    total: Frame,
    floor: Floor,
) -> (Frame, Frame) {
    let (length, _) = along(axis, total, floor);
    let (least_first, least_second) = floors(axis, first, second, floor);
    let (a, b) = split.sizes_between(length, least_first, least_second);
    match axis {
        Axis::Horizontal => (
            Frame::new(total.x, total.y, a, total.height),
            Frame::new(total.x + a, total.y, b, total.height),
        ),
        Axis::Vertical => (
            Frame::new(total.x, total.y, total.width, a),
            Frame::new(total.x, total.y + a, total.width, b),
        ),
    }
}

/// Whether `frame` can be divided along `axis` with both sides at least
/// `floor` — decision 17: a split that cannot is refused, not squeezed.
pub fn room_to_split(frame: Frame, axis: Axis, floor: Floor) -> bool {
    let (length, least) = along(axis, frame, floor);
    length >= 2.0 * least
}

/// The place an arrow reaches from `from` — decision 14. A candidate lies
/// wholly on that side and shares some of `from`'s span across the arrow;
/// the nearest wins, then the one sharing more, then the first drawn. An
/// arrow with nothing on its side goes nowhere: the walk does not wrap.
pub fn toward<K: Clone + PartialEq>(
    places: &[(K, Frame)],
    from: &K,
    direction: Direction,
) -> Option<K> {
    const SLACK: f32 = 0.5;
    let (_, here) = places.iter().find(|(key, _)| key == from)?;
    let shared = |a0: f32, a1: f32, b0: f32, b1: f32| (a1.min(b1) - a0.max(b0)).max(0.0);
    let mut best: Option<(&K, f32, f32)> = None;
    for (key, frame) in places {
        if key == from {
            continue;
        }
        let (gap, overlap) = match direction {
            Direction::Right => (
                frame.x - here.right(),
                shared(here.y, here.bottom(), frame.y, frame.bottom()),
            ),
            Direction::Left => (
                here.x - frame.right(),
                shared(here.y, here.bottom(), frame.y, frame.bottom()),
            ),
            Direction::Down => (
                frame.y - here.bottom(),
                shared(here.x, here.right(), frame.x, frame.right()),
            ),
            Direction::Up => (
                here.y - frame.bottom(),
                shared(here.x, here.right(), frame.x, frame.right()),
            ),
        };
        if gap < -SLACK || overlap <= 0.0 {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, nearest, most)) => {
                gap < nearest - SLACK || ((gap - nearest).abs() <= SLACK && overlap > most + SLACK)
            }
        };
        if better {
            best = Some((key, gap, overlap));
        }
    }
    best.map(|(key, ..)| key.clone())
}
