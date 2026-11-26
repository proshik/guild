//! The terminal palette: from an ANSI code to a colour on screen.
//!
//! The sixteen base colours are not taken from a ready-made theme but
//! aligned with the design system: red is `accent.rust`, green is
//! `accent.green`, yellow is `accent.brass`, cyan is `accent.patina`,
//! magenta is `accent.violet`. Otherwise the terminal would look like a
//! patch from another application: it takes up almost the whole window,
//! and its own palette there outweighs everything else.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use gpui::{Hsla, Rgba};

use crate::theme;

/// ANSI white (7) on a light palette: Slate's `text.secondary`, the value it
/// had before the dark palettes other than Midnight were removed.
const LIGHT_ANSI_WHITE: u32 = 0xb4b4b9;
/// ANSI bright white (15) on a light palette: Slate's `text.primary`.
const LIGHT_ANSI_BRIGHT_WHITE: u32 = 0xfafafc;

/// The terminal's colours.
pub struct Palette {
    /// The background of selected text, mixed when the palette was built.
    selection: Hsla,
    /// The background of a match of the find — `accent.highlight`. M5.12.
    found: Hsla,
    /// The background of the current match — `accent.brass`. M5.12.
    current: Hsla,
    ansi: [Hsla; 16],
    foreground: Hsla,
    background: Hsla,
    cursor: Hsla,
}

impl Palette {
    /// A palette aligned with the design system.
    ///
    /// Takes the theme rather than reaching for one: a terminal repaints
    /// with the rest of the window when a person changes palette, and a
    /// terminal left on the old colours while the interface moved is the
    /// most visible way a theme can be half-applied.
    pub fn new(theme: crate::theme::Theme) -> Self {
        // A program's sixteen colours are the built-in palette's, whatever
        // the person tinted: they are the program's choice of meaning, not
        // the window's look (`two-themes-customize`). Only what the
        // terminal draws around them — its ground, its text, selection and
        // matches — follows the theme in effect.
        let built_in = crate::theme::Theme::of(theme.kind);
        // ANSI white is literal, unlike the default foreground: a light
        // palette's dark text would make a program's white-on-dark pair
        // unreadable (traps.md, "Default foreground is not ANSI white").
        // Slate's two whites, kept as numbers when Slate itself went.
        let (ansi_white, ansi_bright_white) = if theme.kind.is_light() {
            (LIGHT_ANSI_WHITE, LIGHT_ANSI_BRIGHT_WHITE)
        } else {
            (built_in.text.secondary, built_in.text.primary)
        };
        // Normal and bright come in pairs. Bright is not "the same colour
        // lighter": in a terminal these are eight separate colours, and
        // deriving them by lightening loses a distinction programs rely
        // on.
        let ansi = [
            from_hex(0x14141a), // black
            from_hex(built_in.accent.rust),
            from_hex(built_in.accent.green),
            from_hex(built_in.accent.brass),
            from_hex(if theme.kind.is_light() {
                built_in.accent.patina
            } else {
                0x6f9ae8
            }), // blue
            from_hex(built_in.accent.violet),
            from_hex(built_in.accent.patina),
            // ANSI white is literal, unlike the default foreground. A
            // program may pair it with an explicit dark background.
            from_hex(ansi_white),
            from_hex(built_in.text.gutter), // bright black: grey
            from_hex(0xff8a72),
            from_hex(0x8fd67a),
            from_hex(0xf2dda4),
            from_hex(0x9dbcf5),
            from_hex(0xdcc0ff),
            from_hex(0x7fe0cd),
            from_hex(ansi_bright_white),
        ];

        Self {
            selection: theme::blend(
                from_hex(theme.accent.patina),
                from_hex(theme.surface.app),
                0.30,
            ),
            found: from_hex(theme.accent.highlight),
            current: from_hex(theme.accent.brass),
            ansi,
            foreground: from_hex(theme.text.primary),
            background: from_hex(theme.surface.app),
            // White, like every terminal people come from: they draw the
            // cursor in the foreground colour. Brass was chosen to tie the
            // terminal to the design system, and it made the cursor read as
            // one more coloured character rather than as the place typing
            // lands.
            cursor: from_hex(theme.text.primary),
        }
    }

    /// The terminal's background colour.
    pub fn background(&self) -> Hsla {
        self.background
    }

    /// The cursor colour.
    pub fn cursor(&self) -> Hsla {
        self.cursor
    }

    /// Resolve a cell's colour.
    ///
    /// `overrides` is what the program set through OSC; it takes
    /// precedence over our palette, otherwise `claude` could not set its
    /// own colours.
    pub fn resolve(&self, color: Color, overrides: &Colors) -> Hsla {
        match color {
            Color::Spec(rgb) => from_rgb(rgb),
            Color::Named(named) => self.named(named, overrides),
            Color::Indexed(index) => self.indexed(index, overrides),
        }
    }

    fn named(&self, named: NamedColor, overrides: &Colors) -> Hsla {
        if let Some(rgb) = overrides[named] {
            return from_rgb(rgb);
        }

        let index = named as usize;
        if index < 16 {
            return self.ansi[index];
        }

        match named {
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            // The dim variants come from alacritty when a program asks
            // for a dim named colour. We dim them the same way as the DIM
            // flag — otherwise the same "dim" would look different
            // depending on how it was asked for.
            NamedColor::DimForeground => {
                self.dim(self.named(NamedColor::Foreground, overrides), overrides)
            }
            NamedColor::BrightForeground => self.foreground,
            other => {
                let index = other as usize;
                // Dim* and Bright* follow the service entries in the
                // enum; we map them back to the base colour by
                // remainder.
                self.ansi[index % 16]
            }
        }
    }

    fn indexed(&self, index: u8, overrides: &Colors) -> Hsla {
        if let Some(rgb) = overrides[index as usize] {
            return from_rgb(rgb);
        }

        match index {
            0..=15 => self.ansi[index as usize],
            // The 6×6×6 cube: the standard xterm layout.
            16..=231 => {
                let value = index - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                from_rgb(Rgb {
                    r: level(value / 36),
                    g: level((value % 36) / 6),
                    b: level(value % 6),
                })
            }
            // A grey ramp of twenty-four steps.
            232..=255 => {
                let value = 8 + (index - 232) * 10;
                from_rgb(Rgb {
                    r: value,
                    g: value,
                    b: value,
                })
            }
        }
    }

    /// The colour a program asking with `OSC 4`, `10`, `11` or `12` is
    /// told — M5.12 task 6: 0 to 255 by the palette and the cube, 256 the
    /// foreground, 257 the background, 258 the cursor, **a program's own
    /// override first**, because that is what is on the screen. Nothing
    /// asks for another index; one that did is told the foreground.
    pub fn answer_colour(&self, index: usize, overrides: &Colors) -> Rgb {
        if index > 255
            && index < alacritty_terminal::term::color::COUNT
            && let Some(rgb) = overrides[index]
        {
            return rgb;
        }
        let colour = match index {
            0..=255 => self.indexed(index as u8, overrides),
            257 => self.background,
            258 => self.cursor,
            _ => self.foreground,
        };
        to_rgb(colour)
    }

    /// The background of selected text.
    ///
    /// Not inversion: inversion breaks the output's colours — a selected
    /// error stops being red. A muted patina is visible on a dark
    /// background and does not fight the content.
    pub fn selection(&self) -> Hsla {
        self.selection
    }

    /// The background of a match of the find — section 6.30, M5.12.
    pub fn found(&self) -> Hsla {
        self.found
    }

    /// The background of the current match, under text in the terminal's
    /// background colour — section 6.30, M5.12.
    pub fn current(&self) -> Hsla {
        self.current
    }

    /// Dim toward the terminal's effective background, including OSC 11.
    pub fn dim(&self, color: Hsla, overrides: &Colors) -> Hsla {
        let background = self.named(NamedColor::Background, overrides);
        theme::blend(color, background, theme::DIM_KEEP)
    }
}

impl Default for Palette {
    /// Guild's own, for the tests and for anything that has no palette to
    /// hand. Everything that draws does have one.
    fn default() -> Self {
        Self::new(crate::theme::Theme::dark())
    }
}

fn from_hex(hex: u32) -> Hsla {
    gpui::rgb(hex).into()
}

/// A colour as a program is told it, in bytes.
fn to_rgb(colour: Hsla) -> Rgb {
    let rgba = Rgba::from(colour);
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb {
        r: byte(rgba.r),
        g: byte(rgba.g),
        b: byte(rgba.b),
    }
}

fn from_rgb(rgb: Rgb) -> Hsla {
    Rgba {
        r: rgb.r as f32 / 255.0,
        g: rgb.g as f32 / 255.0,
        b: rgb.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

#[cfg(test)]
mod tests {

    /// `two-themes-customize`: a person's tint reaches what the terminal
    /// draws around a program — its ground, its text, its selection and
    /// matches — and never a program's own sixteen colours, which stay the
    /// built-in palette's (spec "What customization never changes").
    #[test]
    fn a_tint_reaches_the_ground_and_never_the_ansi_colours() {
        use guild_session::settings::{Customization, Tint};
        for kind in crate::theme::Kind::ALL.iter().copied() {
            let every = Some(Tint {
                color: 0xf43f5e,
                intensity: 100,
            });
            let custom = Customization {
                background: every,
                accent: every,
                highlight: every,
            };
            let tinted = crate::theme::Theme::customized(kind, &custom);
            let (built_in, ours) = (
                Palette::new(crate::theme::Theme::of(kind)),
                Palette::new(tinted),
            );
            assert_eq!(ours.ansi, built_in.ansi, "{kind:?}: program colours");
            assert_eq!(ours.background, from_hex(tinted.surface.app), "{kind:?}");
            assert_eq!(ours.foreground, from_hex(tinted.text.primary), "{kind:?}");
            assert_eq!(ours.found, from_hex(tinted.accent.highlight), "{kind:?}");
            assert_ne!(
                ours.background, built_in.background,
                "{kind:?}: the tint shows"
            );
        }
    }
    use super::*;

    fn rgba(color: Hsla) -> Rgba {
        Rgba::from(color)
    }

    #[test]
    fn light_default_text_and_explicit_white_have_different_roles() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::light()] {
            let palette = Palette::new(theme);
            let colors = Colors::default();
            for named in [NamedColor::White, NamedColor::BrightWhite] {
                let white = rgba(palette.resolve(Color::Named(named), &colors));
                assert!(
                    white.r > 0.7 && white.g > 0.7 && white.b > 0.7,
                    "explicit white on a dark program background must stay light"
                );
            }
            for named in [NamedColor::Foreground, NamedColor::BrightForeground] {
                let text = rgba(palette.resolve(Color::Named(named), &colors));
                assert!(
                    text.r < 0.4 && text.g < 0.4 && text.b < 0.4,
                    "default text must remain readable on the light terminal"
                );
            }
        }
    }

    /// What the milestone started for: dim must differ from normal. The
    /// previous component ignored the `DIM` flag, and `claude`'s hints
    /// came out at full brightness — indistinguishable from typed text.
    #[test]
    fn dim_is_visibly_different_from_normal() {
        let palette = Palette::new(crate::theme::Theme::dark());
        let normal = palette.resolve(Color::Named(NamedColor::Foreground), &Colors::default());
        let dimmed = palette.dim(normal, &Colors::default());

        let (normal, dimmed) = (rgba(normal), rgba(dimmed));
        let distance = (normal.r - dimmed.r).abs();

        assert!(
            distance > 0.15,
            "the dim colour barely differs from the normal one: {} against {}",
            normal.r,
            dimmed.r
        );
    }

    /// A program's override takes precedence over our palette: otherwise
    /// colours set through OSC would be ignored silently.
    #[test]
    fn program_overrides_win_over_our_palette() {
        let palette = Palette::new(crate::theme::Theme::dark());
        let mut overrides = Colors::default();
        overrides[NamedColor::Red as usize] = Some(Rgb { r: 1, g: 2, b: 3 });

        let resolved = rgba(palette.resolve(Color::Named(NamedColor::Red), &overrides));

        assert!((resolved.r - 1.0 / 255.0).abs() < 1e-6);
        assert!((resolved.g - 2.0 / 255.0).abs() < 1e-6);
    }

    /// The 6×6×6 cube is an xterm standard, and the formula is easy to
    /// get wrong: the colours drift unnoticed, because each one on its own
    /// looks plausible.
    #[test]
    fn the_colour_cube_matches_the_xterm_layout() {
        let palette = Palette::new(crate::theme::Theme::dark());
        let colors = Colors::default();

        // 16 is the start of the cube, pure black.
        let first = rgba(palette.resolve(Color::Indexed(16), &colors));
        assert_eq!((first.r, first.g, first.b), (0.0, 0.0, 0.0));

        // 231 is the end of the cube, white.
        let last = rgba(palette.resolve(Color::Indexed(231), &colors));
        assert!((last.r - 1.0).abs() < 1e-6 && (last.b - 1.0).abs() < 1e-6);

        // 196 is the cube's red: r at maximum, g and b at zero.
        let red = rgba(palette.resolve(Color::Indexed(196), &colors));
        assert!((red.r - 1.0).abs() < 1e-6);
        assert_eq!((red.g, red.b), (0.0, 0.0));
    }

    #[test]
    fn the_grey_ramp_ascends() {
        let palette = Palette::new(crate::theme::Theme::dark());
        let colors = Colors::default();

        let dark = rgba(palette.resolve(Color::Indexed(232), &colors));
        let light = rgba(palette.resolve(Color::Indexed(255), &colors));

        assert!(dark.r < light.r, "the ramp must ascend");
        assert!(dark.r < 0.05 && light.r > 0.9);
    }

    /// **A program asking for a colour is told the one it sees** — M5.12
    /// task 6, `OSC 4`, `10`, `11` and `12`. 0 to 255 are the palette and the
    /// cube, 256 the foreground, 257 the background, 258 the cursor; a
    /// colour the program set itself is answered as set, because that is
    /// what is on the screen.
    #[test]
    fn a_colour_is_answered_from_the_palette_unless_the_program_set_it() {
        let theme = crate::theme::Theme::dark();
        let palette = Palette::new(theme);
        let colors = Colors::default();
        let hex = |hex: u32| Rgb {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
        };

        assert_eq!(palette.answer_colour(1, &colors), hex(theme.accent.rust));
        assert_eq!(
            palette.answer_colour(196, &colors),
            Rgb { r: 255, g: 0, b: 0 }
        );
        assert_eq!(palette.answer_colour(256, &colors), hex(theme.text.primary));
        assert_eq!(palette.answer_colour(257, &colors), hex(theme.surface.app));
        assert_eq!(palette.answer_colour(258, &colors), hex(theme.text.primary));

        let mut overrides = Colors::default();
        overrides[257] = Some(Rgb { r: 1, g: 2, b: 3 });
        overrides[1] = Some(Rgb { r: 4, g: 5, b: 6 });
        assert_eq!(
            palette.answer_colour(257, &overrides),
            Rgb { r: 1, g: 2, b: 3 }
        );
        assert_eq!(
            palette.answer_colour(1, &overrides),
            Rgb { r: 4, g: 5, b: 6 }
        );
    }

    /// The bright colours are eight separate ones, not lightened normals.
    /// Programs rely on the distinction; making them the same loses it
    /// silently.
    #[test]
    fn bright_colours_differ_from_their_normal_pairs() {
        let palette = Palette::new(crate::theme::Theme::dark());
        let colors = Colors::default();

        for base in 1..8u8 {
            let normal = rgba(palette.resolve(Color::Indexed(base), &colors));
            let bright = rgba(palette.resolve(Color::Indexed(base + 8), &colors));

            let distance = (normal.r - bright.r).abs()
                + (normal.g - bright.g).abs()
                + (normal.b - bright.b).abs();
            assert!(
                distance > 0.05,
                "colour {base} and its bright pair are almost identical"
            );
        }
    }
}
