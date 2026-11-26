//! Where the Recent Projects popup sits and how big it is.
//!
//! Geometry, and nothing else — no window, no drawing. The questions a
//! draggable box raises are all arithmetic ("does the left edge stop when
//! the width does") and arithmetic is the part that goes wrong, so it is
//! here where a test can ask without opening anything.
//!
//! It lives beside [`crate::settings`] because that is where it is kept:
//! a size a person chose once and would have to choose again on every
//! launch is a size not worth choosing.

use serde::{Deserialize, Serialize};

/// A point in the window, from its top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    /// Offset from the window's left edge.
    pub x: f32,
    /// Offset from its top.
    pub y: f32,
}

/// What a hand has hold of.
///
/// The corners are named rather than derived from two booleans: a corner
/// is a thing a person aims at, and the two edges it joins are how it
/// behaves, not what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    /// The header — the whole popup follows.
    Move,
    /// The left edge: the popup's left side moves and its right stays.
    Left,
    /// The right edge.
    Right,
    /// The bottom edge.
    Bottom,
    /// Both at once.
    BottomLeft,
    /// Both at once.
    BottomRight,
}

/// The popup's place, as it is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickerPlace {
    /// Where it was dragged to, if it ever was.
    ///
    /// `None` rather than a pair of numbers until it is moved: the default
    /// is *centred*, which is a rule about the window rather than a
    /// position in it. Written down as a number it would stop following a
    /// window that changed width — centred yesterday, off-centre today.
    pub at: Option<Spot>,
    /// How wide, between [`Self::MIN_WIDTH`] and [`Self::MAX_WIDTH`].
    pub width: f32,
    /// How tall, between [`Self::MIN_HEIGHT`] and [`Self::MAX_HEIGHT`].
    pub height: f32,
}

impl Default for PickerPlace {
    fn default() -> Self {
        Self {
            at: None,
            // The widest thing in the popup is a path, and that is the
            // reason to be wider than the session panel's 330:
            // `~/work/projects/acme/backend` is nearly thirty characters
            // and it is a short one.
            width: 460.0,
            // Seven rows and the line being typed into. Ten is the whole
            // recent list, and a popup that is always taller than its list
            // is a box with a hole at the bottom.
            height: 380.0,
        }
    }
}

impl PickerPlace {
    /// Narrower than this and a path is unreadable at any length.
    pub const MIN_WIDTH: f32 = 320.0;
    /// Wider than this and it stops being a popup.
    pub const MAX_WIDTH: f32 = 900.0;
    /// The line being typed into, plus a row, plus the padding.
    pub const MIN_HEIGHT: f32 = 160.0;
    /// Taller than the whole recent list has nothing left to show.
    pub const MAX_HEIGHT: f32 = 760.0;

    /// The top left corner, in a window of this size.
    ///
    /// `top` is where the popup starts when it has not been moved — below
    /// the title bar, because the eye is already up there reading the
    /// project's name and this popup answers a question about that name.
    pub fn place(&self, window: (f32, f32), top: f32) -> (f32, f32) {
        let spot = match self.at {
            Some(spot) => (spot.x, spot.y),
            None => (((window.0 - self.width) / 2.0).max(0.0), top),
        };

        on_screen(spot, (self.width, self.height), window)
    }

    /// Follow a hand that has hold of one part of the popup.
    pub fn drag(&mut self, grab: Grab, by: (f32, f32), window: (f32, f32), top: f32) {
        let (x, y) = self.place(window, top);

        match grab {
            Grab::Move => {
                self.at = Some(spot(on_screen(
                    (x + by.0, y + by.1),
                    (self.width, self.height),
                    window,
                )));
            }
            Grab::Right => self.width = size(self.width + by.0, Self::MIN_WIDTH, Self::MAX_WIDTH),
            Grab::Bottom => {
                self.height = size(self.height + by.1, Self::MIN_HEIGHT, Self::MAX_HEIGHT)
            }
            Grab::Left => self.pull_left(by.0, (x, y)),
            Grab::BottomLeft => {
                self.pull_left(by.0, (x, y));
                self.drag(Grab::Bottom, (0.0, by.1), window, top);
            }
            Grab::BottomRight => {
                self.drag(Grab::Right, (by.0, 0.0), window, top);
                self.drag(Grab::Bottom, (0.0, by.1), window, top);
            }
        }
    }

    /// Drag the left edge, leaving the right one where it is.
    ///
    /// **The trap.** At the narrowest width the edge has to stop *with*
    /// the width. Move it by the hand's full delta and the popup walks
    /// right while its right edge stays put, which reads as the popup
    /// shrinking on its own. So the width decides, and the edge moves by
    /// what the width actually gave up.
    fn pull_left(&mut self, by: f32, at: (f32, f32)) {
        let width = size(self.width - by, Self::MIN_WIDTH, Self::MAX_WIDTH);
        let moved = self.width - width;

        self.width = width;
        self.at = Some(Spot {
            x: at.0 + moved,
            y: at.1,
        });
    }

    /// Bring the popup back into a window that has changed under it.
    ///
    /// A remembered place comes from whatever window it was made in — an
    /// external monitor, a full screen — and a popup restored outside the
    /// window looks like a shortcut that does nothing.
    pub fn settle(&mut self, window: (f32, f32), top: f32) {
        let default = Self::default();

        // A file written by hand, or by a version that wrote something
        // else here. A `NaN` survives every clamp and lays out as nothing.
        if !usable(self.width) {
            self.width = default.width;
        }
        if !usable(self.height) {
            self.height = default.height;
        }

        self.width = size(self.width, Self::MIN_WIDTH, Self::MAX_WIDTH).min(window.0);
        self.height = size(self.height, Self::MIN_HEIGHT, Self::MAX_HEIGHT).min(window.1);

        if self.at.is_some() {
            self.at = Some(spot(self.place(window, top)));
        }
    }
}

/// Is this a number a layout can use?
fn usable(measure: f32) -> bool {
    measure.is_finite() && measure > 0.0
}

/// A size, held between its two limits.
fn size(measure: f32, least: f32, most: f32) -> f32 {
    if !usable(measure) {
        return least;
    }
    measure.clamp(least, most)
}

/// A corner, held inside the window.
///
/// Both ends matter: dragged past the top left there is nothing to take
/// hold of, and dragged past the bottom right the popup is gone with no
/// way to bring it back — a scroll bar is not something a popup has.
fn on_screen(at: (f32, f32), of: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    (
        at.0.clamp(0.0, (window.0 - of.0).max(0.0)),
        at.1.clamp(0.0, (window.1 - of.1).max(0.0)),
    )
}

/// A pair as the spot it is.
fn spot(at: (f32, f32)) -> Spot {
    Spot { x: at.0, y: at.1 }
}
