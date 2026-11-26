//! What marks a tab's kind, and what colours it.
//!
//! A shape rather than a shade, and that distinction is the point. Colour
//! alone carried the kind until M5.4 and stopped carrying it exactly when
//! it mattered: a tab waiting for an answer turns green, and with several
//! open the one signal a person scans for disappeared.
//!
//! So shape says agent or shell or settings, colour identifies the harness,
//! and the dot is left with the one thing it does well.

#[cfg(test)]
use guild_session::SessionMode;
use guild_session::{DefaultTab, Runs};

use crate::i18n::{Text, Ui};
use crate::{Kind, theme};

/// The colour of a tab's kind mark.
///
/// The shape says agent or shell; the colour identifies the harness.
/// Claude uses the theme's brass accent.
pub(crate) fn runs_colour(runs: Runs, theme: theme::Theme) -> u32 {
    match runs {
        Runs::Claude(_) => theme.accent.brass,
        Runs::OpenCode(_) => theme::brand::OPENCODE,
        Runs::Codex(_) => theme::brand::CODEX,
        // A shell is not an agent's work, hence not an accent colour: it
        // must not compete for attention with the sessions.
        Runs::Shell => theme.text.muted,
    }
}

/// What marks a tab's kind.
///
/// A shape rather than a shade. Colour alone said the kind until now, and
/// it stopped saying it exactly when it mattered: a tab waiting for an
/// answer turned green, and with several tabs open the one signal a person
/// was scanning for disappeared.
///
/// `claude` writes `✳` in its own title, so the mark is the program's
/// rather than ours — and `title::of` strips it precisely because it
/// belongs here instead. A shell gets the prompt character every modern
/// shell theme draws.
///
/// The variation selector forces the text presentation: `U+2733` has an
/// emoji form, and a colour glyph would sit in the strip shouting.
pub(crate) fn runs_glyph(runs: Runs) -> &'static str {
    match runs {
        // Every harness session wears the same mark; the colour says which.
        Runs::Claude(_) | Runs::OpenCode(_) | Runs::Codex(_) => "\u{2733}\u{fe0e}",
        Runs::Shell => "\u{276f}",
    }
}

/// The colour of a tab's kind mark.
pub(crate) fn kind_colour(kind: Kind, theme: theme::Theme) -> u32 {
    match kind {
        Kind::Session(runs) => runs_colour(runs, theme),
        // Not an accent: a page about Guild is not the work, and must not
        // compete with the sessions for attention.
        Kind::Settings => theme.text.muted,
        Kind::Process => theme.text.muted,
        // The work, not the agent's: no accent. The strip draws the
        // file's icon where a glyph would be, in its own colours.
        Kind::File => theme.text.muted,
        // Neither the work nor the agent: what changed. Quiet, like the
        // settings, and told by its glyph.
        Kind::Diff => theme.text.tertiary,
        // The same claim as a diff: about the work, not the work.
        Kind::Annotate => theme.text.tertiary,
    }
}

/// What marks a tab's kind in the strip.
pub(crate) fn kind_glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::Session(runs) => runs_glyph(runs),
        // A cog, because settings are settings everywhere. Text
        // presentation for the same reason as the others.
        Kind::Settings => "\u{2699}\u{fe0e}",
        Kind::Process => "\u{25c7}\u{fe0e}",
        // Never drawn: a file tab shows its icon instead. A glyph is
        // still answered so that nothing has to special-case the kind.
        Kind::File => "\u{25a4}\u{fe0e}",
        // Plus-minus: the two markers of a diff in one sign.
        Kind::Diff => "\u{00b1}\u{fe0e}",
        // A pilcrow: the mark of who wrote the paragraph.
        Kind::Annotate => "\u{00b6}\u{fe0e}",
    }
}

/// How the **+**'s default tab is labelled in the interface — M14.
///
/// A terminal is a row of the choice, so it is a row of the setting too.
pub(crate) fn default_tab_label(ui: Ui, tab: DefaultTab) -> &'static str {
    ui.text(default_tab_text(tab))
}

/// The word a kind of tab is called by, before it is translated — one
/// table for the add button's tooltip and the chevron's rows. M12.7.
pub(crate) fn default_tab_text(tab: DefaultTab) -> Text {
    match tab {
        DefaultTab::Claude => Text::AgentClaudeCode,
        DefaultTab::OpenCode => Text::AgentOpenCode,
        DefaultTab::Codex => Text::AgentCodex,
        DefaultTab::Terminal => Text::Terminal,
    }
}

/// How a harness's mark is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Paint {
    /// One colour: the file is `currentColor` and drawn as a mask.
    Tint(u32),
    /// The maker's own colours, which the file carries: drawn as an image.
    Own,
}

/// Which file a harness's mark is, and how it is painted. Codex's and
/// OpenCode's are their makers' pictures — a green
/// and a blue tint was a colour neither of them uses; OpenCode's pale
/// frame has a dark-framed twin for the light palette.
pub(crate) fn harness_mark_file(tab: DefaultTab, theme: theme::Theme) -> (&'static str, Paint) {
    match tab {
        DefaultTab::Claude => ("ui/mark-claude.svg", Paint::Tint(theme::brand::CLAUDE)),
        DefaultTab::OpenCode if theme.kind.is_light() => ("ui/mark-opencode-light.svg", Paint::Own),
        DefaultTab::OpenCode => ("ui/mark-opencode.svg", Paint::Own),
        DefaultTab::Codex => ("ui/mark-codex.svg", Paint::Own),
        DefaultTab::Terminal => ("ui/mark-terminal.svg", Paint::Tint(theme.text.muted)),
    }
}

/// How much of a mark shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shade {
    Full,
    /// A menu row that cannot be chosen, or a Sessions row muted under
    /// Settled.
    Disabled,
}

/// A harness's mark at a size — the tab's, the **+**'s, its menu rows'
/// and the usage card's, which were four copies of one match.
pub(crate) fn harness_mark(
    tab: DefaultTab,
    theme: theme::Theme,
    size: impl Into<gpui::Length> + Clone,
    shade: Shade,
) -> gpui::AnyElement {
    use gpui::prelude::*;
    let (path, paint) = harness_mark_file(tab, theme);
    match paint {
        Paint::Tint(colour) => gpui::svg()
            .path(path)
            .flex_none()
            .size(size)
            .text_color(match shade {
                Shade::Full => gpui::rgb(colour),
                Shade::Disabled => gpui::rgb(theme.text.faint),
            })
            .into_any_element(),
        Paint::Own => gpui::img(path)
            .flex_none()
            .size(size)
            .opacity(match shade {
                Shade::Full => 1.0,
                Shade::Disabled => FAINT,
            })
            .into_any_element(),
    }
}

/// Opacity of a disabled full-colour mark — a harness's, or the title
/// bar's Open In app icon with no project to open — and of a Settled
/// row's project tile.
pub(crate) const FAINT: f32 = 0.35;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::title;

    /// Codex's and OpenCode's marks are their makers' pictures in their
    /// own colours; tinted with one brand colour they
    /// were a green and a blue neither of them uses. Claude's and a
    /// terminal's stay one colour. OpenCode's pale frame would vanish on
    /// the light palette, so that palette has the dark-framed one.
    #[test]
    fn codex_and_opencode_wear_their_own_colours() {
        let dark = theme::Theme::dark();
        let light = theme::Theme::all()
            .into_iter()
            .find(|theme| theme.kind.is_light())
            .expect("there is a light palette");

        assert_eq!(
            harness_mark_file(DefaultTab::Codex, dark),
            ("ui/mark-codex.svg", Paint::Own)
        );
        assert_eq!(
            harness_mark_file(DefaultTab::Codex, light),
            ("ui/mark-codex.svg", Paint::Own)
        );
        assert_eq!(
            harness_mark_file(DefaultTab::OpenCode, dark),
            ("ui/mark-opencode.svg", Paint::Own)
        );
        assert_eq!(
            harness_mark_file(DefaultTab::OpenCode, light),
            ("ui/mark-opencode-light.svg", Paint::Own)
        );
        assert_eq!(
            harness_mark_file(DefaultTab::Claude, dark),
            ("ui/mark-claude.svg", Paint::Tint(theme::brand::CLAUDE))
        );
        assert_eq!(
            harness_mark_file(DefaultTab::Terminal, dark),
            ("ui/mark-terminal.svg", Paint::Tint(dark.text.muted))
        );
        // A picture in its own colours carries them in the file.
        for (name, bytes) in crate::icons::UI {
            if [
                "ui/mark-codex.svg",
                "ui/mark-opencode.svg",
                "ui/mark-opencode-light.svg",
            ]
            .contains(name)
            {
                let text = std::str::from_utf8(bytes).expect("an svg is text");
                assert!(!text.contains("currentColor"), "{name} is tinted");
            }
        }
    }

    /// The kind is told by shape, not by shade. Colour used to carry it
    /// alone, and it stopped carrying it exactly when it mattered: a tab
    /// waiting for an answer turns green, and the one signal a person was
    /// scanning for went with it.
    #[test]
    fn a_shell_and_a_session_are_different_shapes() {
        let session = runs_glyph(Runs::Claude(SessionMode::Terminal));
        let shell = runs_glyph(Runs::Shell);

        assert_ne!(session, shell, "the shapes must differ");
    }

    /// **Claude's mark is Claude's colour, whatever the palette** — asked
    /// for on 2026-09-16, when a dark palette's `#ffff00` brass drew it yellow.
    /// A brand mark in a theme's accent is somebody else's mark.
    #[test]
    fn claude_s_mark_wears_claude_s_colour_in_every_palette() {
        for kind in theme::Kind::ALL {
            let palette = theme::Theme::of(*kind);
            assert_eq!(
                harness_mark_file(DefaultTab::Claude, palette).1,
                Paint::Tint(theme::brand::CLAUDE),
                "{kind:?}"
            );
            assert_ne!(
                palette.accent.brass,
                theme::brand::CLAUDE,
                "{kind:?}: a brass equal to the brand would hide a regression"
            );
        }
    }

    /// The marker is drawn as text, not as an emoji. `U+2733` has a colour
    /// form, and a colour glyph in the tab strip shouts.
    #[test]
    fn the_session_marker_is_text_rather_than_emoji() {
        assert!(
            runs_glyph(Runs::Claude(SessionMode::Terminal)).ends_with('\u{fe0e}'),
            "the variation selector forces the text presentation"
        );
    }

    /// Settings is told from a session at a glance, like a shell is.
    #[test]
    fn settings_has_a_mark_of_its_own() {
        let settings = kind_glyph(Kind::Settings);

        assert_ne!(settings, kind_glyph(Kind::Session(Runs::Shell)));
        assert_ne!(
            settings,
            kind_glyph(Kind::Session(Runs::Claude(SessionMode::Terminal)))
        );
        assert!(
            settings.ends_with('\u{fe0e}'),
            "text presentation, like the others"
        );
    }

    /// The mark is the program's own: `claude` writes it in its title, and
    /// `title::of` strips it precisely because it belongs on the tab as a
    /// kind rather than in the label as decoration.
    #[test]
    fn the_marker_is_the_one_claude_writes_and_the_label_drops() {
        let raw = "\u{2733} Rust uninstall workflow";
        let glyph = runs_glyph(Runs::Claude(SessionMode::Terminal));

        assert!(raw.starts_with(glyph.trim_end_matches('\u{fe0e}')));
        assert_eq!(
            title::of(raw).as_deref(),
            Some("Rust uninstall workflow"),
            "the label keeps the words and the tab keeps the mark"
        );
    }
}
