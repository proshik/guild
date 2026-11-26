//! Viewport-only terminal preparation; never owns scrollback or consumes damage.
use crate::terminal::{
    paint::{self, Mark, Run},
    palette::Palette,
};
use alacritty_terminal::term::{cell::Cell, color::Colors};
use std::rc::Rc;

pub(super) struct Prepared {
    pub runs: Vec<Run>,
    pub typed: Vec<Run>,
    pub text: Vec<gpui::SharedString>,
}
struct Row {
    cells: Vec<(Cell, Mark)>,
    prepared: Rc<Prepared>,
}
impl Row {
    fn matches<'a>(&self, mut cells: impl Iterator<Item = (&'a Cell, Mark)>) -> bool {
        self.cells.iter().all(|(cell, mark)| {
            cells
                .next()
                .is_some_and(|(new_cell, new_mark)| cell == new_cell && *mark == new_mark)
        }) && cells.next().is_none()
    }
}
#[derive(Default)]
pub(super) struct Rows {
    rows: Vec<Option<Row>>,
    colors: Option<Colors>,
    #[cfg(test)]
    prepared: usize,
}
impl Rows {
    pub fn begin(&mut self, colors: &Colors, rows: usize) {
        if self.colors.as_ref().is_none_or(|old| {
            (0..alacritty_terminal::term::color::COUNT).any(|i| old[i] != colors[i])
        }) {
            self.clear();
            self.colors = Some(*colors);
        }
        self.rows.resize_with(rows, || None);
        if self.rows.capacity() > rows.saturating_mul(2) {
            self.rows.shrink_to_fit();
        }
    }
    pub fn clear(&mut self) {
        self.rows.clear();
        self.colors = None;
    }
    pub fn get<'a>(
        &mut self,
        row: usize,
        cells: impl Iterator<Item = (&'a Cell, Mark)> + Clone,
        palette: &Palette,
        colors: &Colors,
    ) -> Rc<Prepared> {
        // Output commonly scrolls retained rows up the viewport. Align once
        // using exact content/marks, then still check each following row: a
        // matching first row does not prove that the rest stayed unchanged.
        if row == 0
            && self.rows[0]
                .as_ref()
                .is_some_and(|old| !old.matches(cells.clone()))
            && let Some(offset) = self
                .rows
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(index, old)| {
                    old.as_ref()
                        .filter(|old| old.matches(cells.clone()))
                        .map(|_| index)
                })
        {
            self.rows.rotate_left(offset);
        }
        let slot = &mut self.rows[row];
        if let Some(old) = slot.as_ref()
            && old.matches(cells.clone())
        {
            return old.prepared.clone();
        }
        #[cfg(test)]
        {
            self.prepared += 1;
        }
        #[cfg(feature = "native-preview")]
        crate::native_preview::resource_probe::ROWS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let runs = paint::runs_of_row(cells.clone(), palette, colors);
        let typed: Vec<_> = runs.iter().map(paint::typed).collect();
        let text = typed
            .iter()
            .map(|run| gpui::SharedString::from(run.text.clone()))
            .collect();
        let prepared = Rc::new(Prepared { runs, typed, text });
        *slot = Some(Row {
            cells: cells.map(|(cell, mark)| (cell.clone(), mark)).collect(),
            prepared: prepared.clone(),
        });
        prepared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cells() -> Vec<Cell> {
        "hello界"
            .chars()
            .map(|c| Cell {
                c,
                ..Cell::default()
            })
            .collect()
    }
    #[test]
    fn wide_combining_and_emoji_rows_match_uncached_preparation() {
        use alacritty_terminal::term::cell::Flags;
        let mut cells = [
            Cell {
                c: 'Ж',
                ..Default::default()
            },
            Cell {
                c: 'e',
                ..Default::default()
            },
            Cell {
                c: '界',
                flags: Flags::WIDE_CHAR,
                ..Default::default()
            },
            Cell {
                flags: Flags::WIDE_CHAR_SPACER,
                ..Default::default()
            },
            Cell {
                c: '🙂',
                flags: Flags::WIDE_CHAR,
                ..Default::default()
            },
            Cell {
                flags: Flags::WIDE_CHAR_SPACER,
                ..Default::default()
            },
        ];
        cells[1].push_zerowidth('\u{301}');
        let colors = Colors::default();
        let palette = Palette::new(Default::default());
        let mut cache = Rows::default();
        cache.begin(&colors, 1);
        for generation in 0..3 {
            if generation == 2 {
                cells[1].push_zerowidth('\u{308}');
            }
            let prepared = cache.get(
                0,
                cells.iter().map(|cell| (cell, Mark::default())),
                &palette,
                &colors,
            );
            assert_eq!(
                prepared
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>(),
                if generation == 2 {
                    "Жe\u{301}\u{308}界🙂"
                } else {
                    "Жe\u{301}界🙂"
                }
            );
            assert_eq!(prepared.runs.iter().map(|r| r.width).sum::<usize>(), 6);
            assert_eq!(
                prepared.runs,
                paint::runs_of_row(
                    cells.iter().map(|cell| (cell, Mark::default())),
                    &palette,
                    &colors
                )
            );
            assert_eq!(
                prepared.typed,
                prepared.runs.iter().map(paint::typed).collect::<Vec<_>>()
            );
            for (text, run) in prepared.text.iter().zip(&prepared.typed) {
                assert_eq!(text.as_ref(), run.text.as_str());
            }
        }
        assert_eq!(
            cache.prepared, 2,
            "combining changes were missed or stable text was rebuilt"
        );
    }

    #[test]
    fn scrolling_reuses_rows_at_their_new_viewport_positions() {
        let lines: Vec<Vec<Cell>> = (0..5)
            .map(|row| {
                format!("line {row}")
                    .chars()
                    .map(|c| Cell {
                        c,
                        ..Cell::default()
                    })
                    .collect()
            })
            .collect();
        let colors = Colors::default();
        let palette = Palette::new(Default::default());
        let mut cache = Rows::default();
        cache.begin(&colors, 4);
        for (row, cells) in lines.iter().take(4).enumerate() {
            cache.get(
                row,
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors,
            );
        }
        for (row, cells) in lines.iter().skip(1).enumerate() {
            let prepared = cache.get(
                row,
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors,
            );
            assert_eq!(
                prepared.runs,
                paint::runs_of_row(
                    cells.iter().map(|c| (c, Mark::default())),
                    &palette,
                    &colors
                )
            );
        }
        assert_eq!(
            cache.prepared, 5,
            "scrolling rebuilt retained text instead of only the newly exposed row"
        );
        // Alignment is only a hint: another retained row may change in
        // the same frame, and must not inherit the old row's text.
        for (row, source) in [2, 0, 4, 1].into_iter().enumerate() {
            let cells = &lines[source];
            let prepared = cache.get(
                row,
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors,
            );
            assert_eq!(
                prepared.runs,
                paint::runs_of_row(
                    cells.iter().map(|c| (c, Mark::default())),
                    &palette,
                    &colors
                )
            );
        }
        assert_eq!(
            cache.prepared, 6,
            "alignment hid a changed row or rebuilt unchanged rows"
        );
    }

    #[test]
    fn unchanged_rows_are_not_prepared_again() {
        let cells = cells();
        let colors = Colors::default();
        let palette = Palette::new(Default::default());
        let mut cache = Rows::default();
        for _ in 0..10 {
            cache.begin(&colors, 1);
            let row = cache.get(
                0,
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors,
            );
            assert_eq!(
                row.runs,
                paint::runs_of_row(
                    cells.iter().map(|c| (c, Mark::default())),
                    &palette,
                    &colors
                )
            );
            assert_eq!(
                row.typed,
                row.runs.iter().map(paint::typed).collect::<Vec<_>>()
            );
        }
        assert_eq!(cache.prepared, 1);
    }
    #[test]
    fn one_changed_row_and_style_changes_are_not_stale() {
        let mut cells = cells();
        let mut colors = Colors::default();
        let palette = Palette::new(Default::default());
        let mut cache = Rows::default();
        cache.begin(&colors, 2);
        for row in 0..2 {
            cache.get(
                row,
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors,
            );
        }
        cells[0].c = 'X';
        let row = cache.get(
            1,
            cells.iter().map(|c| (c, Mark::default())),
            &palette,
            &colors,
        );
        assert_eq!(
            row.runs,
            paint::runs_of_row(
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors
            )
        );
        assert_eq!(cache.prepared, 3);
        colors[256] = Some(alacritty_terminal::vte::ansi::Rgb {
            r: 10,
            g: 20,
            b: 30,
        });
        cache.begin(&colors, 2);
        let row = cache.get(
            1,
            cells.iter().map(|c| {
                (
                    c,
                    Mark {
                        selected: true,
                        ..Default::default()
                    },
                )
            }),
            &palette,
            &colors,
        );
        assert_eq!(
            row.runs,
            paint::runs_of_row(
                cells.iter().map(|c| (
                    c,
                    Mark {
                        selected: true,
                        ..Default::default()
                    }
                )),
                &palette,
                &colors
            )
        );
    }
    #[test]
    fn palette_marks_and_viewport_shrink_do_not_retain_stale_rows() {
        let cells = cells();
        let mut colors = Colors::default();
        let palette = Palette::new(Default::default());
        let mut cache = Rows::default();
        cache.begin(&colors, 20);
        let original = cache.get(
            19,
            cells.iter().map(|c| (c, Mark::default())),
            &palette,
            &colors,
        );
        let weak = Rc::downgrade(&original);
        drop(original);
        cache.begin(&colors, 1);
        assert!(weak.upgrade().is_none(), "off-viewport row retained");
        let normal = cache.get(
            0,
            cells.iter().map(|c| (c, Mark::default())),
            &palette,
            &colors,
        );
        let marked = cache.get(
            0,
            cells.iter().map(|c| {
                (
                    c,
                    Mark {
                        selected: true,
                        ..Default::default()
                    },
                )
            }),
            &palette,
            &colors,
        );
        assert_ne!(normal.runs, marked.runs);
        let unchanged = cache.get(
            0,
            cells.iter().map(|c| {
                (
                    c,
                    Mark {
                        selected: true,
                        ..Default::default()
                    },
                )
            }),
            &palette,
            &colors,
        );
        assert!(Rc::ptr_eq(&marked, &unchanged));
        colors[256] = Some(alacritty_terminal::vte::ansi::Rgb {
            r: 40,
            g: 50,
            b: 60,
        });
        cache.begin(&colors, 1);
        let recolored = cache.get(
            0,
            cells.iter().map(|c| {
                (
                    c,
                    Mark {
                        selected: true,
                        ..Default::default()
                    },
                )
            }),
            &palette,
            &colors,
        );
        assert_ne!(marked.runs, recolored.runs);
        cache.clear();
        cache.begin(&colors, 1);
        let rebuilt = cache.get(
            0,
            cells.iter().map(|c| (c, Mark::default())),
            &palette,
            &colors,
        );
        assert_eq!(
            rebuilt.runs,
            paint::runs_of_row(
                cells.iter().map(|c| (c, Mark::default())),
                &palette,
                &colors
            )
        );
    }
}
