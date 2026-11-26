//! The two palettes that stay, pinned to the values they had before the
//! other six were removed (`two-themes-customize`, task 1.1).
//!
//! **Literals, read off `theme/palettes.rs` on the day, not off the code
//! under test.** The design-system tables in `palettes.rs` move with the
//! document; this table does not move at all, which is what makes it the
//! evidence that Light is still Cloud and Dark is still Midnight, bit for
//! bit, through the rename and every customization that is computed from
//! them. A change here is a change to a palette, and should be one.

use guild_desktop::theme::*;

/// Cloud, as `Theme::cloud()` drew it before the change.
const CLOUD: &[(&str, u32, bool)] = &[
    ("surface.backdrop", 0xdbe3ec, false),
    ("surface.app", 0xf8fafc, false),
    ("surface.panel", 0xe7edf3, false),
    ("surface.sunken", 0xf0f4f8, false),
    ("surface.inset", 0xe2eaf2, false),
    ("surface.card", 0xffffff, false),
    ("surface.card_muted", 0xf0f4f8, false),
    ("surface.raised", 0xdbe3ec, false),
    ("surface.badge", 0xe2eaf2, false),
    ("surface.divider", 0xcbd5e1, false),
    ("text.primary", 0x172337, false),
    ("text.secondary", 0x33465d, false),
    ("text.tertiary", 0x52657c, false),
    ("text.muted", 0x63768b, false),
    ("text.faint", 0x98a8ba, false),
    ("text.gutter", 0xaebdcd, false),
    ("accent.brass", 0x805900, false),
    ("accent.brass_bg", 0xe7e0ce, false),
    ("accent.patina", 0x286493, false),
    ("accent.patina_bg", 0xd7e8f5, false),
    ("accent.green", 0x2f7a3d, false),
    ("accent.rust", 0xc4452a, false),
    ("accent.rust_bg", 0xf6ebe9, false),
    ("accent.violet", 0x7a3fa8, false),
    // Added with the token itself, equal to `accent.brass_bg` (task 4.1).
    ("accent.highlight", 0xe7e0ce, false),
    ("diff.added", 0xe3ece5, false),
    ("diff.removed", 0xf5e7e7, false),
    ("syntax.keyword", 0x7a3fa8, false),
    ("syntax.ty", 0x0f6c7c, false),
    ("syntax.string", 0x805900, false),
    ("syntax.number", 0x2f7a3d, false),
    ("syntax.comment", 0x63768b, true),
    ("syntax.doc", 0x63768b, true),
    ("syntax.macro", 0x286493, false),
    ("syntax.function", 0x286493, false),
    ("syntax.constant", 0x2f7a3d, false),
    ("syntax.tag", 0x0f6c7c, false),
    ("syntax.type_ref", 0x0f6c7c, false),
    ("syntax.namespace", 0x286493, false),
    ("syntax.builtin", 0x7a3fa8, false),
];

/// Midnight, as `Theme::islands()` drew it before the change — with one
/// deliberate departure since: `surface.raised` went from `0x27292d` to
/// `0x313337` on 2026-11-21, the person's decision, because a raised fill
/// on a `0x26282c` card was 1.01:1 and showed nothing.
const MIDNIGHT: &[(&str, u32, bool)] = &[
    ("surface.backdrop", 0x131416, false),
    ("surface.app", 0x191a1c, false),
    ("surface.panel", 0x212326, false),
    ("surface.sunken", 0x1d1e21, false),
    ("surface.inset", 0x1f2024, false),
    ("surface.card", 0x26282c, false),
    ("surface.card_muted", 0x212326, false),
    ("surface.raised", 0x313337, false),
    ("surface.badge", 0x33353b, false),
    ("surface.divider", 0x33353b, false),
    ("text.primary", 0xbcbec4, false),
    ("text.secondary", 0x9fa2a8, false),
    ("text.tertiary", 0x8b8e94, false),
    ("text.muted", 0x73767c, false),
    ("text.faint", 0x6f737a, false),
    ("text.gutter", 0x4b5059, false),
    ("accent.brass", 0xc29e4a, false),
    ("accent.brass_bg", 0x3e3726, false),
    ("accent.patina", 0x548af7, false),
    ("accent.patina_bg", 0x26334c, false),
    ("accent.green", 0x549159, false),
    ("accent.rust", 0xf75464, false),
    ("accent.rust_bg", 0x45262a, false),
    ("accent.violet", 0xc77dbb, false),
    // Added with the token itself, equal to `accent.brass_bg` (task 4.1).
    ("accent.highlight", 0x3e3726, false),
    ("diff.added", 0x242f27, false),
    ("diff.removed", 0x382226, false),
    ("syntax.keyword", 0xcf8e6d, false),
    ("syntax.ty", 0xbcbec4, false),
    ("syntax.string", 0x6aab73, false),
    ("syntax.number", 0x2aacb8, false),
    ("syntax.comment", 0x7a7e85, false),
    ("syntax.doc", 0x5f826b, true),
    ("syntax.macro", 0xb3ae60, false),
    ("syntax.function", 0x56a8f5, false),
    ("syntax.constant", 0xc77dbb, true),
    ("syntax.tag", 0xd5b778, false),
    ("syntax.type_ref", 0x6fafbd, false),
    ("syntax.namespace", 0xafbf7e, false),
    ("syntax.builtin", 0xcc7832, false),
];

/// Every token of a palette, named as the design system names it.
///
/// Destructured without `..`: a field added to any of these structs stops
/// this from compiling, so the table cannot silently miss a token.
fn tokens(theme: &Theme) -> Vec<(&'static str, u32, bool)> {
    let Theme {
        kind: _,
        surface,
        text,
        accent,
        syntax,
        diff,
    } = *theme;
    let Surface {
        backdrop,
        app,
        panel,
        sunken,
        inset,
        card,
        card_muted,
        raised,
        badge,
        divider,
    } = surface;
    let Text {
        primary,
        secondary,
        tertiary,
        muted,
        faint,
        gutter,
    } = text;
    let Accent {
        brass,
        brass_bg,
        patina,
        patina_bg,
        green,
        rust,
        rust_bg,
        violet,
        highlight,
    } = accent;
    let Diff { added, removed } = diff;
    let Syntax {
        keyword,
        ty,
        string,
        number,
        comment,
        doc,
        r#macro,
        function,
        constant,
        tag,
        type_ref,
        namespace,
        builtin,
    } = syntax;
    let mut all = vec![
        ("surface.backdrop", backdrop, false),
        ("surface.app", app, false),
        ("surface.panel", panel, false),
        ("surface.sunken", sunken, false),
        ("surface.inset", inset, false),
        ("surface.card", card, false),
        ("surface.card_muted", card_muted, false),
        ("surface.raised", raised, false),
        ("surface.badge", badge, false),
        ("surface.divider", divider, false),
        ("text.primary", primary, false),
        ("text.secondary", secondary, false),
        ("text.tertiary", tertiary, false),
        ("text.muted", muted, false),
        ("text.faint", faint, false),
        ("text.gutter", gutter, false),
        ("accent.brass", brass, false),
        ("accent.brass_bg", brass_bg, false),
        ("accent.patina", patina, false),
        ("accent.patina_bg", patina_bg, false),
        ("accent.green", green, false),
        ("accent.rust", rust, false),
        ("accent.rust_bg", rust_bg, false),
        ("accent.violet", violet, false),
        ("accent.highlight", highlight, false),
        ("diff.added", added, false),
        ("diff.removed", removed, false),
    ];
    for (name, ink) in [
        ("syntax.keyword", keyword),
        ("syntax.ty", ty),
        ("syntax.string", string),
        ("syntax.number", number),
        ("syntax.comment", comment),
        ("syntax.doc", doc),
        ("syntax.macro", r#macro),
        ("syntax.function", function),
        ("syntax.constant", constant),
        ("syntax.tag", tag),
        ("syntax.type_ref", type_ref),
        ("syntax.namespace", namespace),
        ("syntax.builtin", builtin),
    ] {
        all.push((name, ink.colour, ink.italic));
    }
    all
}

fn assert_pinned(theme: &Theme, pinned: &[(&str, u32, bool)], name: &str) {
    let actual = tokens(theme);
    assert_eq!(actual.len(), pinned.len(), "{name}: the token count moved");
    for (got, want) in actual.iter().zip(pinned) {
        assert_eq!(got.0, want.0, "{name}: token order");
        assert_eq!(
            (got.1, got.2),
            (want.1, want.2),
            "{name} {}: #{:06x} italic {}, pinned #{:06x} italic {}",
            got.0,
            got.1,
            got.2,
            want.1,
            want.2
        );
    }
}

#[test]
fn light_is_cloud_bit_for_bit() {
    assert_pinned(&Theme::light(), CLOUD, "Light");
    assert_pinned(&Theme::of(Kind::Light), CLOUD, "Light");
}

#[test]
fn dark_is_midnight_bit_for_bit() {
    assert_pinned(&Theme::dark(), MIDNIGHT, "Dark");
    assert_pinned(&Theme::of(Kind::Dark), MIDNIGHT, "Dark");
}
