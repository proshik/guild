//! A palette with the person's tints over it — `two-themes-customize`, D4.
//!
//! Only the choices are stored; the colours are computed here, from the
//! built-in palette, every time. A fix to a built-in palette therefore
//! reaches every customized theme, and Reset is forgetting the choices.
//!
//! Background, then Accent, then Highlight, each guarded so that what is
//! read on the result is never harder to read than on the built-in
//! palette: a choice that would be is taken at the strongest intensity
//! that is not. The numbers are design system §2.9's.

use guild_session::settings::{Customization, ThemeCustom, Tint};

use super::tint::{self, legible, strongest, worst_contrast};
use super::{Kind, Theme};

/// The most colour a tinted surface takes — OKLab chroma.
pub const SURFACE_CAP: f32 = 0.045;
/// The most colour tinted text takes, before its share.
pub const TEXT_CAP: f32 = 0.020;
/// Text moves this share of the way the surfaces do: a slighter tint.
pub const TEXT_SHARE: f32 = 0.5;
/// The most colour a selected row takes when it follows the accent.
pub const FOLLOW_CAP: f32 = 0.06;
/// The most colour a Highlight ground takes.
pub const HIGHLIGHT_CAP: f32 = 0.08;

/// One tab's preset swatches, after its *Default* swatch, which clears the
/// tab. Data, not widget literals.
pub const PRESETS: &[(&str, u32)] = &[
    ("Blue", 0x3b82f6),
    ("Teal", 0x14b8a6),
    ("Green", 0x22c55e),
    ("Amber", 0xf59e0b),
    ("Orange", 0xf97316),
    ("Rose", 0xf43f5e),
    ("Violet", 0x8b5cf6),
    ("Graphite", 0x71717a),
];

fn share(tint: Tint) -> f32 {
    f32::from(tint.intensity) / f32::from(guild_session::settings::MAX_INTENSITY)
}

impl Theme {
    /// The built-in palette of `kind`, with `custom`'s tints over it.
    pub fn resolve(kind: Kind, custom: &ThemeCustom) -> Theme {
        Theme::customized(kind, custom.of(kind))
    }

    /// The built-in palette of `kind` with `custom` over it, by the rule in
    /// the module's documentation. Empty, or every intensity 0: the
    /// built-in palette exactly.
    pub fn customized(kind: Kind, custom: &Customization) -> Theme {
        let base = Theme::of(kind);
        let mut theme = base;
        if let Some(tint) = custom.background {
            theme = background(&base, tint.color, share(tint));
        }
        if let Some(tint) = custom.accent {
            theme = accent(
                &base,
                theme,
                tint.color,
                share(tint),
                custom.highlight.is_none(),
            );
        }
        if let Some(tint) = custom.highlight {
            theme = highlight(&base, theme, tint.color, share(tint));
        }
        theme
    }
}

/// Four text ranks on the five surfaces text is read on.
fn text_floor_pairs(theme: &Theme) -> f32 {
    let (t, s) = (theme.text, theme.surface);
    [t.primary, t.secondary, t.tertiary, t.muted]
        .into_iter()
        .map(|ink| worst_contrast(ink, &[s.app, s.panel, s.sunken, s.card, s.raised]))
        .fold(f32::INFINITY, f32::min)
}

/// Every surface towards `seed`'s hue, text a little; lightness kept.
fn background(base: &Theme, seed: u32, intensity: f32) -> Theme {
    let at = |i: f32| {
        let mut theme = *base;
        let surface = |c| tint::tint(c, seed, SURFACE_CAP, 1.0, i);
        let text = |c| tint::tint(c, seed, TEXT_CAP, TEXT_SHARE, i);
        let s = &mut theme.surface;
        for c in [
            &mut s.backdrop,
            &mut s.app,
            &mut s.panel,
            &mut s.sunken,
            &mut s.inset,
            &mut s.card,
            &mut s.card_muted,
            &mut s.raised,
            &mut s.badge,
            &mut s.divider,
        ] {
            *c = surface(*c);
        }
        let t = &mut theme.text;
        for c in [
            &mut t.primary,
            &mut t.secondary,
            &mut t.tertiary,
            &mut t.muted,
            &mut t.faint,
            &mut t.gutter,
        ] {
            *c = text(*c);
        }
        theme
    };
    // Text on the selection grounds too: the tint moves `text.primary`,
    // and Highlight's own guard starts from what this one leaves.
    let (floor, rows) = (text_floor_pairs(base), ground_floor(base));
    at(strongest(intensity, |i| {
        let theme = at(i);
        text_floor_pairs(&theme) >= floor && ground_contrast(&theme) >= rows
    }))
}

/// How well primary text reads on the selection grounds — the worse of
/// the two.
fn ground_contrast(theme: &Theme) -> f32 {
    let ink = theme.text.primary;
    super::contrast(ink, theme.accent.patina_bg).min(super::contrast(ink, theme.accent.highlight))
}

/// What a tinted selection ground must keep: WCAG's 4.5, or the built-in
/// contrast where that is lower. Not the built-in contrast itself: at one
/// OKLab lightness a green is brighter for WCAG than a brown, so that
/// floor would refuse most hues on Dark while primary text still read at
/// six to one (recorded in the plan during task 5).
pub fn ground_floor(base: &Theme) -> f32 {
    ground_contrast(base).min(super::LEGIBLE)
}

/// `accent.patina` towards `seed`, then made legible on the app and the
/// card as the built-in accent is; selected rows follow when Highlight
/// has no colour of its own.
fn accent(base: &Theme, mut theme: Theme, seed: u32, intensity: f32, rows_follow: bool) -> Theme {
    let floor = worst_contrast(base.accent.patina, &[base.surface.app, base.surface.card]);
    let mixed = tint::mix(base.accent.patina, seed, intensity);
    let grounds = [theme.surface.app, theme.surface.card];
    theme.accent.patina = legible(mixed, &grounds, floor, !theme.kind.is_light());
    if rows_follow {
        let rows = ground_floor(base);
        let follow = |i: f32| {
            let mut t = theme;
            t.accent.patina_bg = tint::tint(
                base.accent.patina_bg,
                theme.accent.patina,
                FOLLOW_CAP,
                1.0,
                i,
            );
            t
        };
        theme = follow(strongest(intensity, |i| {
            ground_contrast(&follow(i)) >= rows
        }));
    }
    theme
}

/// Selection and match grounds towards `seed`, lightness kept.
fn highlight(base: &Theme, theme: Theme, seed: u32, intensity: f32) -> Theme {
    let at = |i: f32| {
        let mut t = theme;
        t.accent.patina_bg = tint::tint(base.accent.patina_bg, seed, HIGHLIGHT_CAP, 1.0, i);
        t.accent.highlight = tint::tint(base.accent.highlight, seed, HIGHLIGHT_CAP, 1.0, i);
        t
    };
    let floor = ground_floor(base);
    at(strongest(intensity, |i| ground_contrast(&at(i)) >= floor))
}
