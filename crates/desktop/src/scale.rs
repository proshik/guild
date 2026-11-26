//! How large the interface is drawn.
//!
//! One number moves all of it. GPUI multiplies every `Rems` value by the
//! window's rem size, so a setting that changes that size changes the text
//! and the space around it together — which is the only way it can work.
//! Text grown inside boxes that did not grow is text that overflows them,
//! and this window sets a text size in 27 places and a distance in 66.
//!
//! The terminal is deliberately outside this. It measures a grid of cells
//! against a font chosen separately, and a rem there would make it move
//! whenever the interface did — which is what having two settings exists to
//! prevent.

use gpui::{Pixels, px};

/// The rem size the design system's numbers were written against.
///
/// `theme.rs` says `1.0` rem where the document says 16 pixels, so at this
/// size the interface is drawn exactly as the document describes it.
pub const BASE: f32 = 16.0;

/// The interface font size the design system assumes.
///
/// Not a number of its own: it is the document's body size, and it is what
/// a person's setting is measured against.
pub const BASE_UI_FONT_SIZE: f32 = 13.0;

/// The rem size for a chosen interface font size.
pub fn rem_size(ui_font_size: f32) -> Pixels {
    px(BASE * (ui_font_size / BASE_UI_FONT_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use guild_session::{MAX_UI_FONT_SIZE, MIN_UI_FONT_SIZE};

    /// At the size the document was written against, the interface is drawn
    /// exactly as the document describes it. This is the acceptance
    /// criterion for the whole change: a default that looks different is a
    /// design system that has been broken rather than scaled.
    #[test]
    fn the_default_size_leaves_the_design_system_alone() {
        assert_eq!(rem_size(BASE_UI_FONT_SIZE), px(BASE));
    }

    /// The scale is proportional, so every distance moves with every size
    /// and the document's ratios survive.
    #[test]
    fn the_scale_is_proportional() {
        assert_eq!(rem_size(BASE_UI_FONT_SIZE * 2.0), px(BASE * 2.0));
        assert_eq!(rem_size(BASE_UI_FONT_SIZE / 2.0), px(BASE / 2.0));
    }

    /// Both ends of the range must produce something drawable: a rem size
    /// of nothing, or of half a screen, is a window nobody can use.
    #[test]
    fn both_ends_of_the_range_are_usable() {
        let smallest = rem_size(MIN_UI_FONT_SIZE);
        let largest = rem_size(MAX_UI_FONT_SIZE);

        assert!(smallest > px(0.0), "{smallest:?}");
        assert!(largest < px(100.0), "{largest:?}");
        assert!(smallest < largest);
    }
}
