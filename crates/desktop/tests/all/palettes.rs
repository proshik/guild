//! The two palettes, against the document that defines them.
//!
//! Out here rather than beside the code: every one goes in through the
//! public door, which makes them a check on that door as well as on what
//! is behind it.

use gpui::{Rgba, rgb};
use guild_desktop::theme::*;

/// The design system is a document, and this module is its copy. The
/// copy has to be checked, otherwise it drifts from the original
/// silently: the interface keeps building and working, it just stops
/// matching what everyone looks at when discussing it.
///
/// So the token table is read from the markdown and compared row by
/// row.
fn design_system() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design-system.md");
    std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "could not read the design system from {}: {err}",
            path.display()
        )
    })
}

/// Pull a token's value out of a row like
/// `| `token` | `#08080a` | … |`.
fn documented(document: &str, token: &str) -> u32 {
    let needle = format!("`{token}`");
    let row = document
        .lines()
        .find(|line| line.starts_with("| ") && line.contains(&needle))
        .unwrap_or_else(|| panic!("token {token} not found in the design system"));

    let hex = row
        .split('|')
        .filter_map(|cell| cell.trim().strip_prefix("`#"))
        .find_map(|cell| cell.strip_suffix('`'))
        .unwrap_or_else(|| panic!("token {token} has no `#rrggbb` value in the document"));

    u32::from_str_radix(hex, 16)
        .unwrap_or_else(|err| panic!("could not parse the value of {token}: {err}"))
}

#[test]
fn surface_tokens_match_the_document() {
    let doc = design_system();
    let pairs = [
        ("surface.backdrop", Theme::dark().surface.backdrop),
        ("surface.app", Theme::dark().surface.app),
        ("surface.panel", Theme::dark().surface.panel),
        ("surface.sunken", Theme::dark().surface.sunken),
        ("surface.inset", Theme::dark().surface.inset),
        ("surface.card", Theme::dark().surface.card),
        ("surface.raised", Theme::dark().surface.raised),
        ("surface.badge", Theme::dark().surface.badge),
        ("divider", Theme::dark().surface.divider),
    ];

    for (token, value) in pairs {
        assert_eq!(
            documented(&doc, token),
            value,
            "{token} has drifted from the design system"
        );
    }
}

/// A brand's colour is not a palette's: one value in every theme, and the
/// document says which. Claude's mark on a tab — 2026-09-16.
#[test]
fn brand_tokens_match_the_document() {
    let doc = design_system();
    assert_eq!(
        documented(&doc, "brand.claude"),
        brand::CLAUDE,
        "brand.claude has drifted from the design system"
    );
}

#[test]
fn text_tokens_match_the_document() {
    let doc = design_system();
    let pairs = [
        ("text.primary", Theme::dark().text.primary),
        ("text.secondary", Theme::dark().text.secondary),
        ("text.tertiary", Theme::dark().text.tertiary),
        ("text.muted", Theme::dark().text.muted),
        ("text.faint", Theme::dark().text.faint),
        ("text.gutter", Theme::dark().text.gutter),
    ];

    for (token, value) in pairs {
        assert_eq!(
            documented(&doc, token),
            value,
            "{token} has drifted from the design system"
        );
    }
}

#[test]
fn accent_tokens_match_the_document() {
    let doc = design_system();
    let pairs = [
        ("accent.brass", Theme::dark().accent.brass),
        ("accent.patina", Theme::dark().accent.patina),
        ("accent.green", Theme::dark().accent.green),
        ("accent.rust", Theme::dark().accent.rust),
        ("accent.violet", Theme::dark().accent.violet),
        ("accent.highlight", Theme::dark().accent.highlight),
    ];

    for (token, value) in pairs {
        assert_eq!(
            documented(&doc, token),
            value,
            "{token} has drifted from the design system"
        );
    }
}

/// `DIM` must dim rather than change the colour. Checked on red:
/// mixing around the HSL circle would drag it towards yellow, and a
/// dim error would stop looking like an error.
#[test]
fn dim_keeps_the_hue_and_only_moves_toward_the_background() {
    let background = gpui::Hsla::from(rgb(Theme::dark().surface.app));
    let red = gpui::Hsla::from(rgb(Theme::dark().accent.rust));

    let dimmed = Rgba::from(blend(red, background, DIM_KEEP));
    let full = Rgba::from(red);
    let bg = Rgba::from(background);

    assert!(
        dimmed.r < full.r && dimmed.r > bg.r,
        "the dim colour must sit between the colour and the background: {} against {} and {}",
        dimmed.r,
        full.r,
        bg.r
    );
    assert!(
        dimmed.r > dimmed.g && dimmed.r > dimmed.b,
        "a dim red must stay red, got r={} g={} b={}",
        dimmed.r,
        dimmed.g,
        dimmed.b
    );
}

#[test]
fn fully_dimmed_is_the_background_and_undimmed_is_the_colour() {
    let background = gpui::Hsla::from(rgb(Theme::dark().surface.app));
    let colour = gpui::Hsla::from(rgb(Theme::dark().text.primary));

    let none = Rgba::from(blend(colour, background, 0.0));
    let all = Rgba::from(blend(colour, background, 1.0));

    assert!((none.r - Rgba::from(background).r).abs() < 1e-6);
    assert!((all.r - Rgba::from(colour).r).abs() < 1e-6);
}

/// Relative luminance, for the checks that must hold of every palette
/// whatever its source.
fn luma(colour: u32) -> f32 {
    let (r, g, b) = ((colour >> 16) & 0xff, (colour >> 8) & 0xff, colour & 0xff);
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

/// The gap between two colours, whichever is lighter.
fn gap(a: u32, b: u32) -> f32 {
    (luma(a) - luma(b)).abs()
}

/// Every palette must be legible. A palette copied from a terminal and
/// a palette derived from one background can both fail this, and the
/// failure is a screen nobody can read rather than a compile error.
#[test]
fn every_palette_separates_its_text_from_its_background() {
    for theme in Theme::all() {
        let against_app = gap(theme.text.primary, theme.surface.app);
        let against_panel = gap(theme.text.primary, theme.surface.panel);

        assert!(
            against_app > 120.0,
            "{}: primary text against the canvas is {against_app:.0}",
            theme.kind.name()
        );
        assert!(
            against_panel > 120.0,
            "{}: primary text against a panel is {against_panel:.0}",
            theme.kind.name()
        );
    }
}

/// The ranks must stay ranks: secondary quieter than primary, and so on
/// down. A derived palette can invert them by accident, and then the
/// interface's hierarchy is upside down while every colour is
/// individually fine.
#[test]
fn every_palette_keeps_its_text_ranks_in_order() {
    for theme in Theme::all() {
        let app = theme.surface.app;
        let ranks = [
            ("primary", theme.text.primary),
            ("secondary", theme.text.secondary),
            ("tertiary", theme.text.tertiary),
            ("muted", theme.text.muted),
            ("faint", theme.text.faint),
        ];

        for pair in ranks.windows(2) {
            let (louder, quieter) = (pair[0], pair[1]);
            assert!(
                gap(louder.1, app) > gap(quieter.1, app),
                "{}: {} is not louder than {}",
                theme.kind.name(),
                louder.0,
                quieter.0
            );
        }
    }
}

/// The one rule that separates the light palette from the four dark
/// ones, and the one a derivation can get backwards.
#[test]
fn the_light_palette_is_actually_light() {
    let light = Theme::light();
    assert!(
        luma(light.surface.app) > luma(light.text.primary),
        "the canvas must be lighter than the text on it"
    );

    for theme in Theme::all().into_iter().filter(|t| !t.kind.is_light()) {
        assert!(
            luma(theme.surface.app) < luma(theme.text.primary),
            "{} is a dark palette",
            theme.kind.name()
        );
    }
}

/// A surface has to be distinguishable from the one it sits on, or the
/// step-in-background rule from section 1 draws nothing.
#[test]
fn every_palette_separates_a_raised_surface_from_its_canvas() {
    for theme in Theme::all() {
        let step = gap(theme.surface.raised, theme.surface.app);

        assert!(
            step > 3.0,
            "{}: a raised row is {step:.1} from the canvas",
            theme.kind.name()
        );
    }
}

/// A lit row, a secondary button and a disc are `surface.raised` drawn
/// on a card, so the step has to show there too, not only against the
/// canvas. Dark's #27292d on its #26282c card was 1.01:1 — nothing; the
/// floor is the step a person sees, a little under `surface.badge`'s.
#[test]
fn every_palette_separates_a_raised_surface_from_a_card() {
    for theme in Theme::all() {
        let step = contrast(theme.surface.raised, theme.surface.card);

        assert!(
            step >= 1.15,
            "{}: a raised row on a card is {step:.2}:1",
            theme.kind.name()
        );
    }
}

/// The × on a Terminal or Run tab lights over the ground it is on: the
/// selected tab's own `raised` — not `badge`, which Dark's raised now all
/// but equals (code review r1, bugs+impl-1) — and, on a tab at rest, the
/// tab the pointer is lighting too.
#[test]
fn a_close_button_lights_on_its_tab() {
    for theme in Theme::all() {
        let hover = u32::from(close_hover(theme, true)) >> 8;
        let step = contrast(hover, theme.surface.raised);
        assert!(
            step >= 1.15,
            "{}: the × hover on a selected tab is {step:.2}:1",
            theme.kind.name()
        );
        let tab = u32::from(control_hover(theme, theme.surface.panel)) >> 8;
        let hover = u32::from(close_hover(theme, false)) >> 8;
        let step = contrast(hover, tab);
        assert!(
            step >= 1.15,
            "{}: the × hover on a lit tab at rest is {step:.2}:1",
            theme.kind.name()
        );
        assert_eq!(close_hover(theme, false), control_hover_lit(theme));
    }
}

/// Light is in section 2.6 of the document, one table. The same rule as
/// Dark's above: the document is the source and this is its copy, and a
/// copy nobody compares drifts.
#[test]
fn the_other_palettes_match_the_document() {
    let document = design_system();

    for theme in Theme::all().into_iter().filter(|t| t.kind != Kind::Dark) {
        let table = palette_table(&document, theme.kind.name());

        for (token, value) in [
            ("surface.app", theme.surface.app),
            ("surface.panel", theme.surface.panel),
            ("surface.raised", theme.surface.raised),
            ("divider", theme.surface.divider),
            ("text.primary", theme.text.primary),
            ("text.secondary", theme.text.secondary),
            ("text.gutter", theme.text.gutter),
            ("accent.brass", theme.accent.brass),
            ("accent.patina", theme.accent.patina),
            ("accent.rust", theme.accent.rust),
            ("accent.highlight", theme.accent.highlight),
        ] {
            assert_eq!(
                documented(&table, token),
                value,
                "{} disagrees with the document about {token}",
                theme.kind.name()
            );
        }
    }
}

/// One palette's table out of section 2.6.
fn palette_table(document: &str, name: &str) -> String {
    let heading = format!("#### {name}\n");
    let start = document
        .find(&heading)
        .unwrap_or_else(|| panic!("no table for {name} in the design system"));
    let rest = &document[start + heading.len()..];
    let end = rest.find("\n#### ").unwrap_or(rest.len());
    rest[..end].to_owned()
}

/// One row out of section 2.4's table: `| keyword | `#c39bf5` |`. The
/// first cell is a word, not a token in backticks, so `documented` cannot
/// find it; this looks for the word at the start of the row.
fn syntax_row(document: &str, name: &str) -> u32 {
    let start = document
        .find("### 2.4 Syntax highlighting")
        .expect("section 2.4 is in the design system");
    let section = &document[start..];
    let row = section
        .lines()
        .find(|line| {
            line.starts_with("| ")
                && line[2..]
                    .split('|')
                    .next()
                    .map(|cell| cell.trim().split([',', ' ']).next() == Some(name))
                    .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("no syntax row for {name} in section 2.4"));
    let hex = row
        .split('|')
        .filter_map(|cell| cell.trim().strip_prefix("`#"))
        .find_map(|cell| cell.split('`').next())
        .unwrap_or_else(|| panic!("the syntax row for {name} has no `#rrggbb` value"));
    u32::from_str_radix(hex, 16).expect("a hex colour")
}

/// The thirteen roles every palette colours, by name and by value — seven
/// until M8.10, which added the three the Dark palette draws apart and the
/// three a grammar may ask for instead.
fn syntax_roles(theme: &Theme) -> [(&'static str, u32); 13] {
    [
        ("keyword", theme.syntax.keyword.colour),
        ("type", theme.syntax.ty.colour),
        // The three a language may ask for instead of the plain ones —
        // M8.10's layer.
        ("type.reference", theme.syntax.type_ref.colour),
        ("namespace", theme.syntax.namespace.colour),
        ("builtin", theme.syntax.builtin.colour),
        ("string", theme.syntax.string.colour),
        ("number", theme.syntax.number.colour),
        ("comment", theme.syntax.comment.colour),
        ("doc", theme.syntax.doc.colour),
        ("macro", theme.syntax.r#macro.colour),
        ("function", theme.syntax.function.colour),
        ("constant", theme.syntax.constant.colour),
        ("tag", theme.syntax.tag.colour),
    ]
}

/// Section 2.4 names thirteen colours; the theme carries exactly those.
#[test]
fn syntax_tokens_match_the_document() {
    let document = design_system();
    for (name, value) in syntax_roles(&Theme::dark()) {
        assert_eq!(value, syntax_row(&document, name), "syntax.{name}");
    }
}

/// The four other palettes have their syntax rows in their own tables in
/// section 2.6 — two transcribed from a token file, two derived from a
/// terminal palette by the rule written beside them — and the code
/// matches each, token by token, the way it matches their surfaces.
#[test]
fn the_other_palettes_syntax_matches_the_document() {
    let document = design_system();

    for theme in Theme::all().into_iter().filter(|t| t.kind != Kind::Dark) {
        let table = palette_table(&document, theme.kind.name());
        for (name, value) in syntax_roles(&theme) {
            assert_eq!(
                documented(&table, &format!("syntax.{name}")),
                value,
                "{} disagrees with the document about syntax.{name}",
                theme.kind.name()
            );
        }
    }
}

/// A comment must still be readable. It is the one role that is
/// deliberately quiet, and a derived palette can quieten it into the
/// background: the tightest of the five sits at 53 against its canvas,
/// so the floor is below that and above what nobody can read.
#[test]
fn every_palette_keeps_its_comments_above_the_floor() {
    for theme in Theme::all() {
        let against_app = gap(theme.syntax.comment.colour, theme.surface.app);
        assert!(
            against_app > 45.0,
            "{}: a comment against the canvas is {against_app:.0}",
            theme.kind.name()
        );
    }
}

/// A background's distance from the canvas, summed over the three
/// channels: the measure section 2.7 sets its floor in. Luma alone would
/// say a green tint and a red tint of the same brightness are the same.
fn channel_gap(a: u32, b: u32) -> u32 {
    let ch = |c: u32, shift: u32| (c >> shift) & 0xff;
    [16, 8, 0]
        .iter()
        .map(|&shift| ch(a, shift).abs_diff(ch(b, shift)))
        .sum()
}

/// Section 2.7 names two backgrounds for Guild's palette; the theme
/// carries exactly those.
#[test]
fn diff_tokens_match_the_document() {
    let document = design_system();
    let start = document
        .find("### 2.7 Diff")
        .expect("section 2.7 is in the design system");
    let section = &document[start..];
    let theme = Theme::dark();
    assert_eq!(theme.diff.added, documented(section, "diff.added"));
    assert_eq!(theme.diff.removed, documented(section, "diff.removed"));
}

/// The four other palettes have their diff rows in their own tables in
/// section 2.6 — two composited from rgba tokens, two derived by
/// the rule beside them — and the code matches each.
#[test]
fn the_other_palettes_diff_matches_the_document() {
    let document = design_system();
    for theme in Theme::all().into_iter().filter(|t| t.kind != Kind::Dark) {
        let table = palette_table(&document, theme.kind.name());
        assert_eq!(
            documented(&table, "diff.added"),
            theme.diff.added,
            "{} disagrees with the document about diff.added",
            theme.kind.name()
        );
        assert_eq!(
            documented(&table, "diff.removed"),
            theme.diff.removed,
            "{} disagrees with the document about diff.removed",
            theme.kind.name()
        );
    }
}

/// A changed line must be visibly changed: section 2.7's floor, 16 over
/// the three channels against the canvas, the tightest of the five being
/// Guild's own addition at 19.
#[test]
fn every_palette_keeps_its_diff_backgrounds_off_the_canvas() {
    for theme in Theme::all() {
        for (name, value) in [("added", theme.diff.added), ("removed", theme.diff.removed)] {
            let gap = channel_gap(value, theme.surface.app);
            assert!(
                gap >= 16,
                "{}: diff.{name} sits {gap} from the canvas",
                theme.kind.name()
            );
        }
    }
}

/// The marks are letters in accents the theme already has, section 2.7:
/// `M` brass, `A` green, `D` rust, `?` faint — in every palette.
#[test]
fn the_marks_are_the_accents_the_document_names() {
    use guild_desktop::theme::Mark;
    for theme in Theme::all() {
        assert_eq!(theme.mark(Mark::Modified), theme.accent.brass);
        assert_eq!(theme.mark(Mark::Added), theme.accent.green);
        assert_eq!(theme.mark(Mark::Deleted), theme.accent.rust);
        assert_eq!(theme.mark(Mark::Untracked), theme.text.faint);
    }
    assert_eq!(Mark::Modified.letter(), "M");
    assert_eq!(Mark::Untracked.letter(), "?");
}

/// The actual Update titlebar control must remain legible in every palette.
#[test]
fn the_update_control_word_reads_in_every_palette() {
    for theme in Theme::all() {
        let ink = update_ink(&theme);
        assert!(ink == theme.accent.brass || ink == theme.text.primary);
        for fill in [theme.surface.card, theme.surface.raised] {
            assert!(
                contrast(ink, fill) >= LEGIBLE,
                "{:?}: Update text fails contrast",
                theme.kind
            );
        }
    }
}

/// WCAG's ratio, checked on the two ends and a pair everyone knows.
#[test]
fn contrast_is_wcag_s() {
    assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 0.01);
    assert!((contrast(0xffffff, 0xffffff) - 1.0).abs() < 0.001);
    assert!((contrast(0x777777, 0xffffff) - 4.48).abs() < 0.01);
}

/// A control in the column lights over the ground it is actually drawn
/// on. Under the pointer, a worktree row or a repository's head already
/// wears `control_hover` over the panel; a control in it that lit with
/// the same blend over the panel painted exactly the row's own fill and
/// showed nothing — the tab list's chevron, the head's `⋯` and `+`.
#[test]
fn a_column_control_lights_over_its_hovered_row() {
    for theme in Theme::all() {
        let row = control_hover(theme, theme.surface.panel);
        let lit = control_hover_over(theme, row);
        let (row, lit) = (u32::from(row) >> 8, u32::from(lit) >> 8);
        assert!(
            contrast(lit, row) >= 1.15,
            "{:?}: a control lit over its hovered row reads {:.2} against it",
            theme.kind,
            contrast(lit, row)
        );
        // And over the selected row's ground.
        let selected = column_selected(theme);
        let lit = control_hover_over(theme, selected);
        assert!(contrast(u32::from(lit) >> 8, u32::from(selected) >> 8) >= 1.15);
    }
}

/// The column's selection: one faint fill
/// shared by the selected worktree's card and the tab in front under it
/// (both drawn with `column_selected`), barely above the panel, the card's
/// outline carrying it; the hover a louder neutral fill, apart from it;
/// and a tab's name legible on the panel and on the selection.
#[test]
fn the_column_s_selection_is_quiet_and_its_hover_louder() {
    for theme in Theme::all() {
        let rgb24 = |colour: Rgba| u32::from(colour) >> 8;
        let panel = theme.surface.panel;
        let selected = rgb24(column_selected(theme));
        let hover = rgb24(control_hover(theme, panel));
        let quiet = contrast(selected, panel);
        assert!(
            (1.02..=1.10).contains(&quiet),
            "{:?}: the selection is barely above the panel, got {quiet:.3}",
            theme.kind
        );
        assert!(
            contrast(hover, panel) > quiet * 1.15 && contrast(hover, selected) >= 1.2,
            "{:?}: the hover is louder than the selection and apart from it",
            theme.kind
        );
        assert!(
            contrast(theme.text.gutter, selected) >= 1.25,
            "{:?}: the card's outline shows on its fill",
            theme.kind
        );
        for ground in [panel, selected] {
            assert!(
                contrast(theme.text.secondary, ground) >= LEGIBLE,
                "{:?}: a tab's name on {ground:06x} reads {:.2}",
                theme.kind,
                contrast(theme.text.secondary, ground)
            );
        }
    }
}

/// Every code line of `src/` that names `token`, by file — comments left out.
fn files_naming(token: &str) -> std::collections::BTreeSet<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    files
        .into_iter()
        // Asked of the path under `src/`: a checkout's own folder may be
        // named `…-tests`, and every file would be left out.
        .filter(|path| {
            let name = path.strip_prefix(&src).unwrap().to_string_lossy();
            !name.contains("tests") && !name.starts_with("theme/")
        })
        .filter(|path| {
            std::fs::read_to_string(path).unwrap().lines().any(|line| {
                let code = line.trim_start();
                !code.starts_with("//") && code.contains(token)
            })
        })
        .map(|path| {
            path.strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// `two-themes-customize`, task 4.1: the ground of selected text and of a
/// search match is `accent.highlight`, which Highlight tints; `brass_bg`
/// stays a warning's ground, a drop target, a lit toggle or chip — which
/// Highlight must not recolour. A new use of either is a decision about
/// which it means, and this list is where it is made.
#[test]
fn selection_and_match_grounds_use_the_highlight_token() {
    let highlight: Vec<_> = files_naming("accent.highlight").into_iter().collect();
    assert_eq!(
        highlight,
        [
            "editor.rs",            // the component's text selection
            "local_search.rs",      // a document's Find matches
            "terminal/palette.rs",  // a terminal's Find matches
            "view/speed_search.rs", // speed search's matched letters
        ],
        "selection and match grounds"
    );
    let brass_bg: Vec<_> = files_naming("accent.brass_bg").into_iter().collect();
    assert_eq!(
        brass_bg,
        [
            "local_search.rs",               // a lit option toggle
            "terminal/view/find_bar.rs",     // a lit option toggle
            "view/areas.rs",                 // a tab dragged over an area
            "view/changes.rs",               // the lit commit chip
            "view/column.rs",                // a drop target
            "view/column/project_groups.rs", // a drop target
            "view/diff.rs",                  // an accepting answer's ground
            "view/drag.rs",                  // a drop target
            "view/find_in_files.rs",         // a lit scope chip and toggle
            "view/notices.rs",               // a warning's ground
            "view/panels.rs",                // a drop target
            "view/process.rs",               // attention
            "view/push_dialog.rs",           // a lit control
            "view/release_notes.rs",         // the Update pill
            "view/search.rs",                // a lit option toggle
            "view/tree_drag.rs",             // a drop target, decision 54
        ],
        "warning, drop and lit grounds"
    );
}
