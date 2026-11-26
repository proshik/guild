//! A split that is dragged — M8.3.6, decision 8. Pure: a fraction, what
//! it gives each side of a length, and what a drag makes of it.

use serde::{Deserialize, Serialize};

/// Where a split stands, as the share of the length its first side takes.
///
/// **Written as its fraction and read through [`Split::new`]** — M12.8,
/// decision 12 — so a hand-edited file is clamped as a split made in
/// code is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "f32", into = "f32")]
pub struct Split {
    fraction: f32,
}

impl Default for Split {
    /// Halfway: a new division gives each side the same.
    fn default() -> Self {
        Self::new(0.5)
    }
}

impl From<f32> for Split {
    fn from(fraction: f32) -> Self {
        Self::new(fraction)
    }
}

impl From<Split> for f32 {
    fn from(split: Split) -> Self {
        split.fraction
    }
}

impl Split {
    /// A split at `fraction`, held between nothing and everything.
    pub fn new(fraction: f32) -> Self {
        Self {
            fraction: if fraction.is_finite() {
                fraction.clamp(0.0, 1.0)
            } else {
                0.5
            },
        }
    }

    /// The share its first side takes.
    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    /// The two sides of `total`, neither under `min` while both fit; when
    /// they do not, `total` is shared in the fraction's proportion.
    pub fn sizes(&self, total: f32, min: f32) -> (f32, f32) {
        self.sizes_between(total, min, min)
    }

    /// As [`Split::sizes`], with a floor of its own for each side — a side
    /// holding a division needs the floors of everything in it (M12.8).
    pub fn sizes_between(&self, total: f32, min_first: f32, min_second: f32) -> (f32, f32) {
        if total < min_first + min_second {
            let first = total * self.fraction;
            return (first, total - first);
        }
        let first = (total * self.fraction).clamp(min_first, total - min_second);
        (first, total - first)
    }

    /// Move the split by `delta` along `total`, stopping where a side
    /// would fall under `min`. Rounded to a hundredth, so that a drag
    /// that comes back lands where it began.
    pub fn drag(&mut self, delta: f32, total: f32, min: f32) {
        self.drag_between(delta, total, min, min);
    }

    /// As [`Split::drag`], with a floor of its own for each side.
    pub fn drag_between(&mut self, delta: f32, total: f32, min_first: f32, min_second: f32) {
        if total <= 0.0 {
            return;
        }
        let (first, _) = self.sizes_between(total, min_first, min_second);
        let moved = if total < min_first + min_second {
            first + delta
        } else {
            (first + delta).clamp(min_first, total - min_second)
        };
        self.fraction = ((moved / total) * 100.0).round() / 100.0;
    }
}
