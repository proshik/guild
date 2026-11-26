//! The two palettes, transcribed from section 2 of the design system.
//!
//! **Two since `two-themes-customize`.** Light is what Cloud was and Dark
//! is what Midnight was, value for value — `tests/all/builtin_palettes.rs`
//! holds the literal values they had on the day the other six (Guild, the
//! old Light, Dusk, Ember, Graphite, Slate) were removed. A person's tint
//! is computed from these and stored as a choice, never as a palette.
//!
//! **Every value was read off a file, never written from memory.** Dark's
//! greys are derived rather than transcribed, and section 2.6 marks which.
//! A derived value "corrected" against a screenshot stops being
//! reproducible from its source, silently.

use super::{Accent, Diff, Ink, Kind, Surface, Syntax, Text, Theme};

impl Theme {
    /// The built-in palette of a kind.
    pub fn of(kind: Kind) -> Self {
        match kind {
            Kind::Light => Self::light(),
            Kind::Dark => Self::dark(),
        }
    }

    /// Both palettes, for the tests that must hold of all of them.
    pub fn all() -> Vec<Self> {
        Kind::ALL.iter().copied().map(Theme::of).collect()
    }

    /// Light: the approved settings proposal's cool light palette, which
    /// was called Cloud.
    pub fn light() -> Self {
        Self {
            kind: Kind::Light,
            surface: Surface {
                backdrop: 0xdbe3ec,
                app: 0xf8fafc,
                panel: 0xe7edf3,
                sunken: 0xf0f4f8,
                inset: 0xe2eaf2,
                card: 0xffffff,
                card_muted: 0xf0f4f8,
                raised: 0xdbe3ec,
                badge: 0xe2eaf2,
                divider: 0xcbd5e1,
            },
            text: Text {
                primary: 0x172337,
                secondary: 0x33465d,
                tertiary: 0x52657c,
                muted: 0x63768b,
                faint: 0x98a8ba,
                gutter: 0xaebdcd,
            },
            accent: Accent {
                brass: 0x805900,
                brass_bg: 0xe7e0ce,
                patina: 0x286493,
                patina_bg: 0xd7e8f5,
                green: 0x2f7a3d,
                rust: 0xc4452a,
                rust_bg: 0xf6ebe9,
                violet: 0x7a3fa8,
                highlight: 0xe7e0ce,
            },
            syntax: Syntax {
                keyword: Ink::plain(0x7a3fa8),
                ty: Ink::plain(0x0f6c7c),
                string: Ink::plain(0x805900),
                number: Ink::plain(0x2f7a3d),
                comment: Ink::italic(0x63768b),
                // The palettes not transcribed from an IDE scheme keep what
                // the old mapping drew: a doc comment was a comment, a
                // constant a number, a tag a type. M8.10.
                doc: Ink::italic(0x63768b),
                r#macro: Ink::plain(0x286493),
                function: Ink::plain(0x286493),
                constant: Ink::plain(0x2f7a3d),
                tag: Ink::plain(0x0f6c7c),
                // A grammar that asks for these gets what the seven-role
                // mapping drew here. M8.10.
                type_ref: Ink::plain(0x0f6c7c),
                namespace: Ink::plain(0x286493),
                builtin: Ink::plain(0x7a3fa8),
            },
            diff: Diff {
                added: 0xe3ece5,
                removed: 0xf5e7e7,
            },
        }
    }

    /// Dark — M8.10, section 2.6 — and called Midnight before.
    ///
    /// **Read off a colour scheme's own files**, never off a screenshot:
    /// one for the editor's colours and one for the interface's greys. A
    /// colour read off a capture is a blend of two colours and the
    /// antialiasing between them.
    ///
    /// Derived, by section 2.6's rule: the surfaces the greys have no name
    /// for, the three accent backdrops, and the two diff grounds — an IDE's
    /// scheme says what a *line* of a diff is, not what a row of ours is.
    pub fn dark() -> Self {
        Self {
            kind: Kind::Dark,
            surface: Surface {
                backdrop: 0x131416,
                app: 0x191a1c,
                panel: 0x212326,
                sunken: 0x1d1e21,
                inset: 0x1f2024,
                card: 0x26282c,
                card_muted: 0x212326,
                raised: 0x313337,
                badge: 0x33353b,
                divider: 0x33353b,
            },
            text: Text {
                primary: 0xbcbec4,
                secondary: 0x9fa2a8,
                tertiary: 0x8b8e94,
                muted: 0x73767c,
                faint: 0x6f737a,
                gutter: 0x4b5059,
            },
            accent: Accent {
                brass: 0xc29e4a,
                brass_bg: 0x3e3726,
                patina: 0x548af7,
                patina_bg: 0x26334c,
                green: 0x549159,
                rust: 0xf75464,
                rust_bg: 0x45262a,
                violet: 0xc77dbb,
                highlight: 0x3e3726,
            },
            // The whole of the scheme's own vocabulary, by its attribute
            // names: `DEFAULT_KEYWORD`, `DEFAULT_CLASS_REFERENCE` — the
            // scheme does not colour a type — `DEFAULT_STRING`,
            // `DEFAULT_NUMBER`, `DEFAULT_LINE_COMMENT`,
            // `DEFAULT_DOC_COMMENT`, `DEFAULT_METADATA`,
            // `DEFAULT_FUNCTION_DECLARATION`, `DEFAULT_CONSTANT` and
            // `XML_TAG_NAME`.
            syntax: Syntax {
                keyword: Ink::plain(0xcf8e6d),
                ty: Ink::plain(0xbcbec4),
                string: Ink::plain(0x6aab73),
                number: Ink::plain(0x2aacb8),
                comment: Ink::plain(0x7a7e85),
                doc: Ink::italic(0x5f826b),
                r#macro: Ink::plain(0xb3ae60),
                function: Ink::plain(0x56a8f5),
                constant: Ink::italic(0xc77dbb),
                tag: Ink::plain(0xd5b778),
                // A Go file's type references, packages and builtins, in
                // the colours the same scheme gives them.
                type_ref: Ink::plain(0x6fafbd),
                namespace: Ink::plain(0xafbf7e),
                builtin: Ink::plain(0xcc7832),
            },
            diff: Diff {
                added: 0x242f27,
                removed: 0x382226,
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
