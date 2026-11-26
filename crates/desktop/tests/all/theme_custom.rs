//! A person's tint over the built-in palettes — `two-themes-customize`,
//! design D4: the OKLab rule, its gamut mapping, and its contrast guards,
//! over a grid of seeds and intensities in both palettes.

use guild_desktop::theme::tint::{self, Lab};
use guild_desktop::theme::*;
use guild_session::settings::{Customization, Tint};

/// The presets, the extremes and the primaries — the seeds a person can
/// reach in one gesture, and the ones most likely to leave sRGB.
const SEEDS: &[u32] = &[
    0x3b82f6, 0x14b8a6, 0x22c55e, 0xf59e0b, 0xf97316, 0xf43f5e, 0x8b5cf6, 0x71717a, 0x000000,
    0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0xffff00,
];
const INTENSITIES: &[u8] = &[0, 1, 25, 50, 75, 100];

fn tinted(color: u32, intensity: u8) -> Option<Tint> {
    Some(Tint { color, intensity })
}

fn surfaces(theme: &Theme) -> [(&'static str, u32); 10] {
    let s = theme.surface;
    [
        ("backdrop", s.backdrop),
        ("app", s.app),
        ("panel", s.panel),
        ("sunken", s.sunken),
        ("inset", s.inset),
        ("card", s.card),
        ("card_muted", s.card_muted),
        ("raised", s.raised),
        ("badge", s.badge),
        ("divider", s.divider),
    ]
}

/// The pairs Background's guard holds: four text ranks on five surfaces.
fn text_pairs(theme: &Theme) -> Vec<(u32, u32)> {
    let t = theme.text;
    let s = theme.surface;
    let mut pairs = Vec::new();
    for ink in [t.primary, t.secondary, t.tertiary, t.muted] {
        for ground in [s.app, s.panel, s.sunken, s.card, s.raised] {
            pairs.push((ink, ground));
        }
    }
    pairs
}

fn min_contrast(pairs: &[(u32, u32)]) -> f32 {
    pairs
        .iter()
        .map(|&(a, b)| contrast(a, b))
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn oklab_round_trips_within_a_channel_step() {
    for colour in [
        0x000000, 0xffffff, 0x191a1c, 0xf8fafc, 0x548af7, 0x286493, 0xff0000,
    ] {
        let back = tint::to_rgb(tint::to_lab(colour));
        for shift in [16, 8, 0] {
            let (a, b) = ((colour >> shift) & 0xff, (back >> shift) & 0xff);
            assert!(a.abs_diff(b) <= 1, "#{colour:06x} came back #{back:06x}");
        }
    }
}

#[test]
fn zero_intensity_is_the_built_in_palette_exactly() {
    for kind in Kind::ALL.iter().copied() {
        for &seed in SEEDS {
            let custom = Customization {
                background: tinted(seed, 0),
                accent: tinted(seed, 0),
                highlight: tinted(seed, 0),
            };
            assert_eq!(
                Theme::customized(kind, &custom),
                Theme::of(kind),
                "{kind:?} #{seed:06x}"
            );
        }
        assert_eq!(
            Theme::customized(kind, &Customization::default()),
            Theme::of(kind)
        );
    }
}

/// Spec "Surfaces keep their order": the tint moves hue, never lightness.
#[test]
fn a_background_tint_keeps_every_surface_s_lightness_and_their_order() {
    for kind in Kind::ALL.iter().copied() {
        let built_in = Theme::of(kind);
        for &seed in SEEDS {
            for &intensity in INTENSITIES {
                let custom = Customization {
                    background: tinted(seed, intensity),
                    ..Default::default()
                };
                let theme = Theme::customized(kind, &custom);
                let (before, after) = (surfaces(&built_in), surfaces(&theme));
                for ((name, was), (_, now)) in before.iter().zip(&after) {
                    let (was, now) = (tint::to_lab(*was).l, tint::to_lab(*now).l);
                    assert!(
                        (was - now).abs() < 0.006,
                        "{kind:?} #{seed:06x}@{intensity}: {name} L {was:.4} → {now:.4}"
                    );
                }
                for (a, b) in before
                    .iter()
                    .zip(&after)
                    .flat_map(|x| before.iter().zip(&after).map(move |y| (x, y)))
                {
                    let (l_a, l_b) = (tint::to_lab(a.0.1).l, tint::to_lab(b.0.1).l);
                    if l_a + 0.006 < l_b {
                        assert!(
                            tint::to_lab(a.1.1).l <= tint::to_lab(b.1.1).l + 0.006,
                            "{kind:?} #{seed:06x}@{intensity}: {} and {} swapped",
                            a.0.0,
                            b.0.0
                        );
                    }
                }
            }
        }
    }
}

/// Spec "Contrast guard", measured on the rounded colours.
#[test]
fn a_background_tint_keeps_text_contrast_at_today_s_minimum() {
    for kind in Kind::ALL.iter().copied() {
        let floor = min_contrast(&text_pairs(&Theme::of(kind)));
        for &seed in SEEDS {
            for &intensity in INTENSITIES {
                let custom = Customization {
                    background: tinted(seed, intensity),
                    ..Default::default()
                };
                let theme = Theme::customized(kind, &custom);
                let worst = min_contrast(&text_pairs(&theme));
                assert!(
                    worst >= floor,
                    "{kind:?} #{seed:06x}@{intensity}: {worst:.3} under {floor:.3}"
                );
            }
        }
    }
}

#[test]
fn a_background_tint_moves_towards_the_hue() {
    let custom = Customization {
        background: tinted(0x3b82f6, 100),
        ..Default::default()
    };
    let theme = Theme::customized(Kind::Dark, &custom);
    let card = tint::to_lab(theme.surface.card);
    let seed = tint::to_lab(0x3b82f6);
    assert!(card.chroma() > tint::to_lab(Theme::dark().surface.card).chroma());
    assert!(
        (card.hue() - seed.hue()).abs() < 0.2,
        "the card's hue {:.2} is the seed's {:.2}",
        card.hue(),
        seed.hue()
    );
    let neutral = Customization {
        background: tinted(0x808080, 100),
        ..Default::default()
    };
    let grey = Theme::customized(Kind::Light, &neutral);
    assert!(
        tint::to_lab(grey.surface.panel).chroma()
            < tint::to_lab(Theme::light().surface.panel).chroma(),
        "a neutral tint desaturates"
    );
}

/// The accent's floor: its built-in contrast on the app and card grounds.
fn accent_floor(theme: &Theme) -> f32 {
    contrast(theme.accent.patina, theme.surface.app)
        .min(contrast(theme.accent.patina, theme.surface.card))
}

fn accent_contrast(theme: &Theme) -> f32 {
    accent_floor(theme)
}

#[test]
fn an_accent_keeps_its_contrast_on_every_grid_point() {
    for kind in Kind::ALL.iter().copied() {
        let floor = accent_floor(&Theme::of(kind));
        for &seed in SEEDS {
            for &intensity in INTENSITIES {
                for background in [None, tinted(seed, intensity)] {
                    let custom = Customization {
                        background,
                        accent: tinted(seed, intensity),
                        highlight: None,
                    };
                    let theme = Theme::customized(kind, &custom);
                    let got = accent_contrast(&theme);
                    assert!(
                        got >= floor,
                        "{kind:?} #{seed:06x}@{intensity}: accent {got:.3} under {floor:.3}"
                    );
                }
            }
        }
    }
}

/// Spec "Saturated blue on Dark" and "Saturated yellow on Light" — the
/// cases spec review r1 named, where lightness alone leaves sRGB.
#[test]
fn saturated_accents_keep_their_hue_family_and_reach_the_floor() {
    for (kind, seed) in [(Kind::Dark, 0x0000ff), (Kind::Light, 0xffff00)] {
        let custom = Customization {
            accent: tinted(seed, 100),
            ..Default::default()
        };
        let theme = Theme::customized(kind, &custom);
        let floor = accent_floor(&Theme::of(kind));
        let got = accent_contrast(&theme);
        assert!(
            got >= floor,
            "{kind:?} #{seed:06x}: {got:.3} under {floor:.3}"
        );
        let (lab, want) = (tint::to_lab(theme.accent.patina), tint::to_lab(seed));
        assert!(
            lab.chroma() > 0.03,
            "{kind:?}: still a colour, not grey: #{:06x}",
            theme.accent.patina
        );
        let turn = (lab.hue() - want.hue()).abs();
        let turn = turn.min(std::f32::consts::TAU - turn);
        assert!(
            turn < 0.35,
            "{kind:?}: #{:06x} turned {turn:.2} from the seed's hue",
            theme.accent.patina
        );
    }
}

#[test]
fn an_accent_at_full_intensity_is_the_seed_when_it_passes() {
    // Midnight's own blue is legible on its card; a nearby legible orange
    // is taken as it is.
    let custom = Customization {
        accent: tinted(0xf97316, 100),
        ..Default::default()
    };
    let theme = Theme::customized(Kind::Dark, &custom);
    assert_eq!(theme.accent.patina, 0xf97316);
}

/// Spec "Selected rows follow the accent".
#[test]
fn selected_rows_follow_the_accent_until_highlight_has_a_colour() {
    let seed = 0xf97316;
    let accent_only = Customization {
        accent: tinted(seed, 100),
        ..Default::default()
    };
    let theme = Theme::customized(Kind::Dark, &accent_only);
    let row = tint::to_lab(theme.accent.patina_bg);
    let built_in = tint::to_lab(Theme::dark().accent.patina_bg);
    assert!(
        (row.l - built_in.l).abs() < 0.006,
        "the row keeps its lightness"
    );
    let turn = (row.hue() - tint::to_lab(theme.accent.patina).hue()).abs();
    assert!(
        turn.min(std::f32::consts::TAU - turn) < 0.2,
        "the row takes the accent's hue"
    );

    let with_highlight = Customization {
        highlight: tinted(0x22c55e, 100),
        ..accent_only
    };
    let theme = Theme::customized(Kind::Dark, &with_highlight);
    let green = tint::to_lab(0x22c55e).hue();
    let away = |colour: u32| {
        let turn = (tint::to_lab(colour).hue() - green).abs();
        turn.min(std::f32::consts::TAU - turn)
    };
    // The guard may take less than all of it — a green ground at Dark's
    // lightness is brighter for WCAG than the blue one — but it is
    // Highlight's hue the row moves to, not the accent's.
    assert!(
        away(theme.accent.patina_bg) < away(Theme::dark().accent.patina_bg) / 2.0,
        "Highlight's colour wins: #{:06x}",
        theme.accent.patina_bg
    );
}

#[test]
fn a_highlight_keeps_primary_text_legible_on_its_grounds() {
    for kind in Kind::ALL.iter().copied() {
        let built_in = Theme::of(kind);
        // WCAG's 4.5, or the built-in contrast where that is lower.
        let floor = contrast(built_in.text.primary, built_in.accent.patina_bg)
            .min(contrast(built_in.text.primary, built_in.accent.highlight))
            .min(LEGIBLE);
        for &seed in SEEDS {
            for &intensity in INTENSITIES {
                let custom = Customization {
                    highlight: tinted(seed, intensity),
                    background: tinted(seed, intensity),
                    accent: None,
                };
                let theme = Theme::customized(kind, &custom);
                let got = contrast(theme.text.primary, theme.accent.patina_bg)
                    .min(contrast(theme.text.primary, theme.accent.highlight));
                assert!(
                    got >= floor,
                    "{kind:?} #{seed:06x}@{intensity}: {got:.3} under {floor:.3}"
                );
            }
        }
    }
}

/// Spec "What customization never changes".
#[test]
fn customization_never_touches_syntax_diff_status_or_brand_colours() {
    for kind in Kind::ALL.iter().copied() {
        let built_in = Theme::of(kind);
        for &seed in SEEDS {
            for &intensity in INTENSITIES {
                let custom = Customization {
                    background: tinted(seed, intensity),
                    accent: tinted(seed, intensity),
                    highlight: tinted(seed, intensity),
                };
                let theme = Theme::customized(kind, &custom);
                assert_eq!(theme.syntax, built_in.syntax);
                assert_eq!(theme.diff, built_in.diff);
                assert_eq!(theme.kind, kind);
                let (a, b) = (theme.accent, built_in.accent);
                assert_eq!(
                    (a.brass, a.brass_bg, a.green, a.rust, a.rust_bg, a.violet),
                    (b.brass, b.brass_bg, b.green, b.rust, b.rust_bg, b.violet)
                );
                // Deterministic: the same choices, the same colours.
                assert_eq!(Theme::customized(kind, &custom), theme);
            }
        }
    }
}

#[test]
fn a_kind_resolves_its_own_customization() {
    let mut custom = guild_session::ThemeCustom::default();
    custom.dark.accent = tinted(0xf97316, 100);
    assert_eq!(Theme::resolve(Kind::Light, &custom), Theme::light());
    assert_eq!(Theme::resolve(Kind::Dark, &custom).accent.patina, 0xf97316);
}

#[test]
fn lab_s_hue_and_chroma_are_its_polar_form() {
    let lab = Lab {
        l: 0.5,
        a: 0.0,
        b: 0.1,
    };
    assert!((lab.chroma() - 0.1).abs() < 1e-6);
    assert!((lab.hue() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
}

/// Spec "Save persists across restart": a saved tint is what the window
/// opens on, read back through the settings store as `main.rs` reads it,
/// and the other palette stays built-in.
#[test]
fn a_saved_customization_is_in_effect_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = guild_session::Settings::default();
    settings.theme_custom.dark.background = tinted(0x3b82f6, 40);
    settings.theme_custom.dark.accent = tinted(0xf97316, 100);
    guild_session::Store::new(path.clone())
        .save(&settings)
        .unwrap();
    guild_session::flush();
    let read = guild_session::Store::<guild_session::Settings>::new(path)
        .load()
        .unwrap();
    let dark = in_effect(&read, true);
    assert_eq!(
        dark,
        Theme::customized(Kind::Dark, &settings.theme_custom.dark)
    );
    assert_ne!(dark, Theme::dark());
    assert_eq!(dark.accent.patina, 0xf97316);
    assert_eq!(in_effect(&read, false), Theme::light(), "Light has no tint");
}
