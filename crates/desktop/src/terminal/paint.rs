//! Painting the grid: from cells into runs of one style.
//!
//! Two decisions the module exists separately for.
//!
//! **Every attribute is applied.** `DIM` dims, `INVERSE` swaps background
//! and text, `HIDDEN` hides, `STRIKEOUT` strikes through. Their absence
//! was the main complaint about the previous component: `claude` sends
//! `DIM` dozens of times — that is how it draws hints — and without
//! handling they came out at full brightness, indistinguishable from typed
//! text.
//!
//! **Runs are shaped as runs, not character by character.** Adjacent cells
//! with the same style merge into a run, and `shape_line` is called once
//! per run. The previous component called it for every character: on a
//! 120×34 grid that is four thousand calls per frame instead of a few
//! dozen.

use alacritty_terminal::term::cell::{Cell, Flags};
use gpui::{Hsla, TextRun};

use super::palette::Palette;

/// ANSI SGR underline shape, retained separately from the hover affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Underline {
    #[default]
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// A run of a row with identical styling.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The run's text.
    pub text: String,
    /// The letter colour.
    pub foreground: Hsla,
    /// The background colour; `None` means the terminal background, which
    /// needs no painting.
    pub background: Option<Hsla>,
    /// The starting column — the background is placed by it.
    pub start_column: usize,
    /// Width in cells.
    pub width: usize,
    /// Preserve the natural shaping of one wide or composed grid cell.
    pub natural_width: bool,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    pub underline_kind: Underline,
    pub underline_color: Option<Hsla>,
    /// Strikethrough.
    pub strikeout: bool,
}

/// How the find marks a cell — M5.12, section 6.30.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Found {
    /// Not a match.
    #[default]
    No,
    /// One of the matches on screen.
    Match,
    /// The match *next* was just pressed on.
    Current,
}

/// What the window lays over a cell's own styling: a selection, the find
/// and a link under the pointer.
///
/// Arrives beside the cell rather than as `alacritty_terminal`'s selection
/// or search types, for the reason the selection always did: this module
/// turns cells into runs, and *a different background* is all it needs to
/// know of any of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    /// The cell is selected.
    pub selected: bool,
    /// The cell is a match of the find.
    pub found: Found,
    /// The cell is part of the link under the pointer while `cmd` is held.
    pub link: bool,
}

/// A cell's styling, resolved to on-screen colours.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Style {
    foreground: Hsla,
    background: Option<Hsla>,
    bold: bool,
    italic: bool,
    underline: bool,
    underline_kind: Underline,
    underline_color: Option<Hsla>,
    strikeout: bool,
}

/// Resolve the styling of a single cell.
///
/// The order matters here: `DIM` applies to the colour already chosen, and
/// `INVERSE` comes after it, to the pair as a whole. The other order gives
/// a dim background instead of dim letters.
fn style_of(
    cell: &Cell,
    palette: &Palette,
    colors: &alacritty_terminal::term::color::Colors,
) -> Style {
    let flags = cell.flags;

    let mut foreground = palette.resolve(cell.fg, colors);
    let mut background = palette.resolve(cell.bg, colors);

    if flags.contains(Flags::DIM) || flags.contains(Flags::DIM_BOLD) {
        foreground = palette.dim(foreground, colors);
    }

    if flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut foreground, &mut background);
    }

    // Hidden text is not "do not paint": it takes up space and the cursor
    // moves over it. We paint it in the background colour.
    if flags.contains(Flags::HIDDEN) {
        foreground = background;
    }

    // A background equal to the terminal background is not painted: that
    // is most cells, and drawing rectangles under them wastes the frame.
    let background = (background != palette.background()).then_some(background);

    Style {
        foreground,
        background,
        bold: flags.contains(Flags::BOLD) || flags.contains(Flags::BOLD_ITALIC),
        italic: flags.contains(Flags::ITALIC) || flags.contains(Flags::BOLD_ITALIC),
        underline: flags.intersects(Flags::ALL_UNDERLINES) && !flags.contains(Flags::HIDDEN),
        underline_kind: if flags.contains(Flags::DOUBLE_UNDERLINE) {
            Underline::Double
        } else if flags.contains(Flags::UNDERCURL) {
            Underline::Curly
        } else if flags.contains(Flags::DOTTED_UNDERLINE) {
            Underline::Dotted
        } else if flags.contains(Flags::DASHED_UNDERLINE) {
            Underline::Dashed
        } else {
            Underline::Single
        },
        underline_color: cell.underline_color().map(|color| {
            let color = palette.resolve(color, colors);
            if flags.intersects(Flags::DIM | Flags::DIM_BOLD) {
                palette.dim(color, colors)
            } else {
                color
            }
        }),
        strikeout: flags.contains(Flags::STRIKEOUT),
    }
}

/// Assemble a row of cells into runs of one style.
///
/// The input is a cell plus its [`Mark`]: a selection, a match and a link
/// are as much part of the styling as a colour, and a run has to break on
/// each of them.
///
/// **The order, M5.12:** the current match wins over everything — it is
/// where *next* was just pressed — and paints `accent.brass` behind the
/// terminal's background colour as text; a selection wins over an ordinary
/// match, which paints `accent.highlight` under the cell's own text colour;
/// a link underlines in the run's own colour, whatever else is on it.
pub fn runs_of_row<'a, I>(
    cells: I,
    palette: &Palette,
    colors: &alacritty_terminal::term::color::Colors,
) -> Vec<Run>
where
    I: IntoIterator<Item = (&'a Cell, Mark)>,
{
    let mut runs: Vec<Run> = Vec::new();

    for (column, (cell, mark)) in cells.into_iter().enumerate() {
        // The second half of a wide character has no content of its own:
        // painting it would double the glyph.
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let mut style = style_of(cell, palette, colors);
        match (mark.found, mark.selected) {
            (Found::Current, _) => {
                style.background = Some(palette.current());
                style.foreground = palette.background();
            }
            (_, true) => style.background = Some(palette.selection()),
            (Found::Match, false) => style.background = Some(palette.found()),
            (Found::No, false) => {}
        }
        if mark.link && !cell.flags.contains(Flags::HIDDEN) {
            style.underline = true;
        }

        if cell.flags.contains(Flags::HIDDEN) {
            style.foreground = style.background.unwrap_or(palette.background());
            style.underline = false;
        }
        let width = if cell.flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        let natural_width = width == 2 || cell.zerowidth().is_some_and(|marks| !marks.is_empty());
        let joins = !natural_width
            && runs.last().is_some_and(|run| {
                !run.natural_width
                    && run.foreground == style.foreground
                    && run.background == style.background
                    && run.bold == style.bold
                    && run.italic == style.italic
                    && run.underline == style.underline
                    && run.underline_kind == style.underline_kind
                    && run.underline_color == style.underline_color
                    && run.strikeout == style.strikeout
                    && run.start_column + run.width == column
            });

        if joins {
            let run = runs.last_mut().expect("checked above");
            run.text.push(cell.c);
            run.width += width;
        } else {
            let mut text = cell.c.to_string();
            if let Some(marks) = cell.zerowidth() {
                text.extend(marks);
            }
            runs.push(Run {
                text,
                foreground: style.foreground,
                background: style.background,
                start_column: column,
                width,
                natural_width,
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                underline_kind: style.underline_kind,
                underline_color: style.underline_color,
                strikeout: style.strikeout,
            });
        }
    }

    runs
}

/// The run the font is given: every drawn character a space.
///
/// **A run, not a string** — M5.14 task 10's crash. Blocks and box lines
/// are drawn by `sprite` and the font gets a space in their place, but a
/// `▛` is three bytes and a space is one. Blanking the text and measuring
/// the original told GPUI the line was longer than it is, and it sliced
/// past the end — or into a letter — and took the window down. The text
/// and its length now come from one value.
pub fn typed(run: &Run) -> Run {
    Run {
        text: run
            .text
            .chars()
            .map(|c| if super::sprite::drawn(c) { ' ' } else { c })
            .collect(),
        ..run.clone()
    }
}

/// The styling of a single run for `shape_line`.
pub fn text_run(run: &Run, font_family: &str, ui: &TextRunStyle) -> TextRun {
    ui.text_run(run, font_family)
}

/// How a run turns into GPUI styling.
///
/// A type rather than a free function: font settings will end up here, and
/// passing them as five parameters gets old fast.
pub struct TextRunStyle;

impl TextRunStyle {
    /// The styling of a single run.
    pub fn text_run(&self, run: &Run, font_family: &str) -> TextRun {
        use gpui::{
            Font, FontFeatures, FontStyle, FontWeight, StrikethroughStyle, UnderlineStyle, px,
        };

        TextRun {
            len: run.text.len(),
            font: Font {
                family: font_family.to_owned().into(),
                features: FontFeatures::default(),
                // Fallback fonts: without them a character missing from
                // the main one is drawn as a box with a question mark.
                fallbacks: Some(gpui::FontFallbacks::from_fonts(super::font::fallbacks())),
                weight: if run.bold {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                },
                style: if run.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
            },
            color: run.foreground,
            // The background is drawn as separate rectangles the full
            // height of the cell: a background under the text is clipped
            // to the glyph and leaves gaps between rows.
            background_color: None,
            underline: run.underline.then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(run.underline_color.unwrap_or(run.foreground)),
                wavy: run.underline_kind == Underline::Curly,
            }),
            strikethrough: run.strikeout.then(|| StrikethroughStyle {
                thickness: px(1.0),
                color: Some(run.foreground),
            }),
        }
    }
}

/// The row of the view the cursor is drawn on, or `None` when it is not
/// shown.
///
/// `line` is the grid's — 0 is the top of the screen at the bottom of the
/// output — and `display_offset` how far the view is scrolled up into the
/// history, so the cursor moves down the view by that many rows and leaves
/// it past the last.
pub fn cursor_row(line: i32, display_offset: usize, screen_lines: usize) -> Option<usize> {
    let row = usize::try_from(line).ok()?.checked_add(display_offset)?;
    (row < screen_lines).then_some(row)
}

/// Decorations cover grid cells, including spaces. Pattern phase comes from the
/// absolute column, so selection/style run boundaries cannot restart a pattern.
pub fn underline_sprite(run: &Run, cell: f32, height: f32, pixel: f32) -> super::sprite::Sprite {
    use super::sprite::{Piece, Sprite, Stroke};
    let width = cell * run.width as f32;
    let y = height - pixel * 2.;
    let mut sprite = Sprite::default();
    let mut line = |y| {
        sprite.pieces.push(Piece {
            x0: 0.,
            x1: width,
            y0: y,
            y1: y + pixel,
            alpha: 1.,
        })
    };
    match run.underline_kind {
        Underline::Single => line(y),
        Underline::Double => {
            line(y);
            line((y - pixel * 2.).max(0.));
        }
        Underline::Dotted | Underline::Dashed => {
            let on = if run.underline_kind == Underline::Dotted {
                pixel
            } else {
                pixel * 3.
            };
            let period = on + pixel * 2.;
            let start = run.start_column as f32 * cell;
            let mut x = -(start % period);
            while x < width {
                sprite.pieces.push(Piece {
                    x0: x.max(0.),
                    x1: (x + on).min(width).max(0.),
                    y0: y,
                    y1: y + pixel,
                    alpha: 1.,
                });
                x += period;
            }
        }
        Underline::Curly => {
            let phase = run.start_column as f32 * cell;
            let steps = (width / pixel).ceil() as usize;
            let points = (0..=steps)
                .map(|i| {
                    let x = (i as f32 * pixel).min(width);
                    (
                        x,
                        y - pixel
                            + (phase + x)
                                .mul_add(std::f32::consts::TAU / (pixel * 6.), 0.)
                                .sin()
                                * pixel,
                    )
                })
                .collect();
            sprite.strokes.push(Stroke {
                points,
                width: pixel,
            });
        }
    }
    sprite
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::color::Colors;
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    use gpui::Rgba;

    #[test]
    fn unicode_cells_keep_text_and_grid_columns() {
        let mut accented = cell('e', Flags::DIM);
        accented.push_zerowidth('\u{301}');
        let row = runs(&[
            accented,
            cell('─', Flags::empty()),
            cell('漢', Flags::WIDE_CHAR),
            cell(' ', Flags::WIDE_CHAR_SPACER),
            cell('😀', Flags::WIDE_CHAR),
            cell(' ', Flags::WIDE_CHAR_SPACER),
            cell('x', Flags::empty()),
        ]);
        assert_eq!(
            row.iter().map(|r| r.text.as_str()).collect::<String>(),
            "e\u{301}─漢😀x"
        );
        assert_eq!(
            row.iter()
                .map(|r| (r.start_column, r.width))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 1), (2, 2), (4, 2), (6, 1)]
        );
        assert_eq!(typed(&row[1]).text, " ");
    }

    #[test]
    fn composed_cell_isolated_from_same_style_text_and_sprites() {
        let mut accented = cell('e', Flags::empty());
        accented.push_zerowidth('\u{301}');
        let row = runs(&[
            cell('a', Flags::empty()),
            accented,
            cell('─', Flags::empty()),
            cell('x', Flags::empty()),
        ]);
        assert_eq!(
            row.iter()
                .map(|r| (r.text.as_str(), r.start_column, r.width, r.natural_width))
                .collect::<Vec<_>>(),
            [
                ("a", 0, 1, false),
                ("e\u{301}", 1, 1, true),
                ("─x", 2, 2, false)
            ]
        );
        assert_eq!(typed(&row[2]).text, " x");
    }

    #[test]
    fn selected_wide_cursor_cell_occupies_two_columns() {
        let mut wide = cell('漢', Flags::WIDE_CHAR);
        wide.push_zerowidth('\u{301}');
        let row = runs_with_selection(
            &[
                wide.clone(),
                cell(' ', Flags::WIDE_CHAR_SPACER),
                cell('x', Flags::empty()),
            ],
            &[true, true, false],
        );
        assert_eq!(
            (row[0].text.as_str(), row[0].start_column, row[0].width),
            ("漢\u{301}", 0, 2)
        );
        assert_eq!(row[0].background, Some(Palette::default().selection()));
        assert_eq!(row[1].start_column, 2);
        let cursor = runs(&[wide]);
        assert_eq!((cursor[0].text.as_str(), cursor[0].width), ("漢\u{301}", 2));
    }

    /// **The run handed to the font is the run measured** — M5.14 task
    /// 10's crash. A drawn character (`▛`, three bytes) was blanked to a
    /// space (one byte) while the run's length was still the original's,
    /// and GPUI sliced past the end: "end byte index 5 is out of bounds
    /// for string of length 3", twice in one morning.
    #[test]
    fn a_run_with_its_drawn_characters_blanked_is_measured_as_blanked() {
        let run = Run {
            text: "▛a▜".to_owned(),
            foreground: gpui::black(),
            background: None,
            start_column: 0,
            width: 3,
            natural_width: false,
            bold: false,
            italic: false,
            underline: false,
            underline_kind: Underline::Single,
            underline_color: None,
            strikeout: false,
        };
        let typed = typed(&run);
        assert_eq!(typed.text, " a ");
        assert_eq!(
            text_run(&typed, "Menlo", &TextRunStyle).len,
            typed.text.len()
        );
    }

    /// **The cursor is drawn where its cell is shown, and not at all while
    /// the history is scrolled past it.** The grid's cursor is a line of
    /// the screen at the bottom of the output; scrolled up by the history
    /// keys, a find or the wheel, that line is further down the view or
    /// below it, and a block painted at the screen's own row lands on a
    /// line of history that holds no cursor — seen in M5.12's window
    /// checks 3 and 6.
    #[test]
    fn the_cursor_row_follows_the_scroll_and_leaves_with_it() {
        assert_eq!(cursor_row(28, 0, 29), Some(28), "at the bottom");
        assert_eq!(cursor_row(3, 10, 29), Some(13), "scrolled a little");
        assert_eq!(cursor_row(28, 1, 29), None, "one row past the view");
        assert_eq!(cursor_row(0, 29, 29), None, "a screen up");
        assert_eq!(cursor_row(-1, 0, 29), None, "never above the screen");
    }

    fn cell(ch: char, flags: Flags) -> Cell {
        Cell {
            c: ch,
            flags,
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            ..Cell::default()
        }
    }

    fn runs(cells: &[Cell]) -> Vec<Run> {
        runs_of_row(
            cells.iter().map(|cell| (cell, Mark::default())),
            &Palette::default(),
            &Colors::default(),
        )
    }

    fn runs_with_selection(cells: &[Cell], selected: &[bool]) -> Vec<Run> {
        runs_marked(
            cells,
            &selected
                .iter()
                .map(|&selected| Mark {
                    selected,
                    ..Mark::default()
                })
                .collect::<Vec<_>>(),
        )
    }

    fn runs_marked(cells: &[Cell], marks: &[Mark]) -> Vec<Run> {
        runs_of_row(
            cells.iter().zip(marks.iter().copied()),
            &Palette::default(),
            &Colors::default(),
        )
    }

    /// Where the milestone started: a dim cell must differ from a normal
    /// one. The previous component ignored the flag, and `claude`'s hints
    /// came out at full brightness.
    #[test]
    fn dim_cells_get_a_dimmer_colour() {
        let row = runs(&[cell('a', Flags::empty()), cell('b', Flags::DIM)]);

        assert_eq!(row.len(), 2, "a dim cell must not merge with a normal one");
        let normal = Rgba::from(row[0].foreground);
        let dimmed = Rgba::from(row[1].foreground);

        assert!(
            dimmed.r < normal.r,
            "the dim colour is not dimmed: {} against {}",
            dimmed.r,
            normal.r
        );
    }

    /// `INVERSE` swaps the colours. `claude` sends it for highlighting —
    /// without handling, the highlighted is indistinguishable from the
    /// ordinary.
    #[test]
    fn inverse_swaps_the_colours() {
        let palette = Palette::default();
        let row = runs(&[cell('i', Flags::INVERSE)]);

        let run = &row[0];
        assert_eq!(
            run.foreground,
            palette.background(),
            "the letters must take the background colour"
        );
        assert!(
            run.background.is_some(),
            "a background must appear, otherwise the inversion is invisible"
        );
    }

    /// Hidden text takes up space but cannot be read.
    #[test]
    fn hidden_text_is_painted_in_the_background_colour() {
        let row = runs(&[cell('h', Flags::HIDDEN)]);

        assert_eq!(row[0].text, "h", "the character still takes its place");
        assert_eq!(
            Rgba::from(row[0].foreground).r,
            Rgba::from(Palette::default().background()).r,
            "but its colour merges with the background"
        );
    }

    #[test]
    fn hidden_text_and_decoration_stay_hidden_under_current_match() {
        let rows = runs_marked(
            &[cell('x', Flags::HIDDEN | Flags::UNDERCURL)],
            &[Mark {
                found: Found::Current,
                link: true,
                ..Default::default()
            }],
        );
        assert_eq!(rows[0].foreground, rows[0].background.unwrap());
        assert!(!rows[0].underline);
    }

    #[test]
    fn every_ansi_underline_variant_reaches_the_run() {
        for flag in [
            Flags::UNDERLINE,
            Flags::DOUBLE_UNDERLINE,
            Flags::UNDERCURL,
            Flags::DOTTED_UNDERLINE,
            Flags::DASHED_UNDERLINE,
        ] {
            assert!(runs(&[cell('x', flag)])[0].underline, "missing {flag:?}");
        }
    }

    #[test]
    fn bold_italic_underline_and_strikeout_reach_the_run() {
        let row = runs(&[cell(
            'x',
            Flags::BOLD | Flags::ITALIC | Flags::UNDERLINE | Flags::STRIKEOUT,
        )]);

        let run = &row[0];
        assert!(run.bold && run.italic && run.underline && run.strikeout);
    }

    /// This is what runs exist for: adjacent identical cells are shaped
    /// in one call rather than one by one.
    #[test]
    fn identical_neighbours_merge_into_one_run() {
        let row = runs(&[
            cell('o', Flags::empty()),
            cell('n', Flags::empty()),
            cell('e', Flags::empty()),
        ]);

        assert_eq!(row.len(), 1, "three identical cells make one run");
        assert_eq!(row[0].text, "one");
        assert_eq!(row[0].width, 3);
    }

    #[test]
    fn a_change_of_style_starts_a_new_run() {
        let row = runs(&[
            cell('a', Flags::empty()),
            cell('b', Flags::BOLD),
            cell('c', Flags::empty()),
        ]);

        assert_eq!(row.len(), 3);
        assert_eq!(row[1].start_column, 1);
        assert_eq!(row[2].start_column, 2);
    }

    /// The second half of a wide character has no content of its own:
    /// painting it would double the glyph.
    #[test]
    fn the_spacer_of_a_wide_char_is_skipped() {
        let row = runs(&[
            cell('漢', Flags::WIDE_CHAR),
            cell(' ', Flags::WIDE_CHAR_SPACER),
            cell('!', Flags::empty()),
        ]);

        let text: String = row.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(text, "漢!", "the spacer is not painted");
    }

    /// A selection is part of the styling: a run has to break on it,
    /// otherwise selected and unselected are painted as one piece and the
    /// difference is invisible.
    #[test]
    fn a_selection_gets_its_own_background_and_breaks_the_run() {
        let row = runs_with_selection(
            &[
                cell('a', Flags::empty()),
                cell('b', Flags::empty()),
                cell('c', Flags::empty()),
            ],
            &[false, true, false],
        );

        assert_eq!(
            row.len(),
            3,
            "a selected cell does not merge with its neighbours"
        );
        assert!(row[0].background.is_none());
        assert_eq!(
            row[1].background,
            Some(Palette::default().selection()),
            "a selected cell has its own background"
        );
        assert!(row[2].background.is_none());
    }

    /// **A mark changes the run it is on, in one order** — M5.12,
    /// section 6.30: the current match over a selection, a selection over
    /// an ordinary match, and a link's underline over whatever else is
    /// there. Each mark breaks the run, or a highlight would bleed into
    /// the cells beside it.
    #[test]
    fn a_mark_changes_the_run_it_is_on() {
        let palette = Palette::default();
        let plain = cell('a', Flags::empty());
        let cells = [
            plain.clone(),
            plain.clone(),
            plain.clone(),
            plain.clone(),
            plain.clone(),
            plain,
        ];
        let marks = [
            Mark::default(),
            Mark {
                found: Found::Match,
                ..Mark::default()
            },
            Mark {
                found: Found::Match,
                selected: true,
                ..Mark::default()
            },
            Mark {
                found: Found::Current,
                selected: true,
                ..Mark::default()
            },
            Mark {
                link: true,
                ..Mark::default()
            },
            Mark {
                found: Found::Current,
                link: true,
                ..Mark::default()
            },
        ];
        let row = runs_marked(&cells, &marks);

        assert_eq!(row.len(), 6, "every mark breaks the run: {row:?}");
        assert_eq!(row[0].background, None);
        assert_eq!(row[1].background, Some(palette.found()), "a match");
        assert_eq!(
            row[1].foreground, row[0].foreground,
            "a match keeps the cell's own text colour"
        );
        assert_eq!(
            row[2].background,
            Some(palette.selection()),
            "a selection wins over a match"
        );
        assert_eq!(
            row[3].background,
            Some(palette.current()),
            "the current match wins over a selection"
        );
        assert_eq!(
            row[3].foreground,
            palette.background(),
            "the current match's text is the terminal's background colour"
        );
        assert!(
            row[4].underline && row[4].background.is_none(),
            "a link underlines"
        );
        assert_eq!(
            row[4].foreground, row[0].foreground,
            "in the run's own colour"
        );
        assert!(
            row[5].underline && row[5].background == Some(palette.current()),
            "a link under the current match is both"
        );
    }

    /// A background equal to the terminal background is not painted: that
    /// is most cells, and a rectangle under each one is a wasted frame.
    #[test]
    fn the_default_background_is_not_painted() {
        let row = runs(&[cell('o', Flags::empty())]);

        assert!(row[0].background.is_none());
    }

    /// A run's length in a `TextRun` is in **bytes**, not characters: for
    /// non-Latin text and emoji the difference is twofold, and GPUI would
    /// split the string at the wrong boundaries.
    ///
    /// The non-Latin characters here are test data: they are what makes
    /// the difference between bytes and characters visible.
    #[test]
    fn run_length_is_measured_in_bytes() {
        let row = runs(&[cell('ж', Flags::empty()), cell('ё', Flags::empty())]);
        let shaped = text_run(&row[0], "Menlo", &TextRunStyle);

        assert_eq!(row[0].text, "жё");
        assert_eq!(
            shaped.len,
            row[0].text.len(),
            "a run's length must be in bytes, not in characters"
        );
    }

    /// Without fallback fonts a character missing from the main one is
    /// drawn as a box with a question mark — which is how the shell prompt
    /// looked.
    #[test]
    fn a_run_carries_fallback_fonts() {
        // A Nerd Font glyph: an ordinary monospace font does not have
        // it.
        let row = runs(&[cell('\u{e0b0}', Flags::empty())]);
        let shaped = text_run(&row[0], "Menlo", &TextRunStyle);

        assert!(
            shaped.font.fallbacks.is_some(),
            "a run must carry fallback fonts"
        );
    }
}
