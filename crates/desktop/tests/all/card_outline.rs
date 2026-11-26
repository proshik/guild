//! Floating card edges remain visible without competing with faint text.
use gpui::{Rgba, rgb};
use guild_desktop::theme::Theme;

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
fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn floating_card_outline_meets_every_palette_band() {
    for theme in Theme::all() {
        let card = rgb(theme.surface.card);
        let outline = guild_desktop::theme::card_outline(theme);
        let actual = contrast(card, outline);
        let floor = 1.5_f64.max(contrast(card, rgb(theme.surface.divider)) + 0.05);
        let ceiling = contrast(card, rgb(theme.text.faint));
        assert!(
            actual >= floor - 0.00001 && actual < ceiling,
            "{:?}: outline {actual:.4}, expected [{floor:.4}, {ceiling:.4})",
            theme.kind
        );
        assert_eq!(
            luminance(outline) > luminance(card),
            luminance(rgb(theme.text.primary)) > luminance(card)
        );
        assert_eq!(outline.a, 1.0);
    }
}

/// Architectural guard: independently framed overlays must not bypass the token.
#[test]
fn floating_frames_use_the_shared_outline() {
    let frames = [
        include_str!("../../src/view/popup.rs"),
        include_str!("../../src/view/overlay.rs"),
        include_str!("../../src/view/goto_line.rs"),
        include_str!("../../src/view/push_dialog.rs")
            .split_once(".id(\"push-popup\")")
            .expect("push popup")
            .1,
        include_str!("../../src/terminal/view/find_bar.rs")
            .split_once(".id(\"find-bar\")")
            .expect("terminal find card")
            .1,
    ];
    for frame in frames {
        let outer = frame.split(".border_1()").nth(1).expect("floating frame");
        let color = outer.split(".border_color(").nth(1).expect("outer border");
        assert!(color.starts_with("theme::card_outline(theme)"));
    }
}
