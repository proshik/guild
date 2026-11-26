//! Floating-card contrast, defined in design-system section 6.17.
use super::Theme;
use gpui::{Rgba, rgb};

/// A quiet contour above each palette's divider, independent of card geometry.
pub fn card_outline(theme: Theme) -> Rgba {
    let card = rgb(theme.surface.card);
    let ink = rgb(theme.text.primary);
    let target = 1.5_f64.max(contrast(card, rgb(theme.surface.divider)) + 0.05);
    // A fixed sRGB blend is weaker than the existing divider on light themes.
    // Solve the fraction against relative luminance instead.
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let fraction = (low + high) / 2.0;
        if contrast(card, card.blend(ink.opacity(fraction))) < target {
            low = fraction;
        } else {
            high = fraction;
        }
    }
    card.blend(ink.opacity(high))
}

fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(color: Rgba) -> f64 {
    let linear = |c: f32| {
        let c = f64::from(c);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}
