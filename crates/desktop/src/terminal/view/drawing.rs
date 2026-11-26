//! Putting the grid on the screen.
//!
//! Called `drawing` rather than `paint` because `terminal::paint` already
//! exists and does the other half — turning a cell into a styled run. Two
//! modules of one name in one tree is the confusion that outlives whoever
//! made it.
//!
//! Capture viewport rows, cursor and scrollbar under one grid guard, then
//! release it before shaping and painting. Accessors that lock the emulator
//! must not be called while capturing: the mutex is not reentrant.

use super::Cursor;
use super::TerminalTab;
use super::{find_bar, hover};
use crate::terminal::paint::TextRunStyle;
use crate::terminal::{paint, scroll, sprite};
use crate::theme;
use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{Bounds, Hsla, Pixels, Point as GpuiPoint, Window, px, quad, transparent_black};
use std::time::Duration;

impl TerminalTab {
    /// Paint the grid.
    pub(super) fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        cell: gpui::Size<Pixels>,
        family: &str,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        #[cfg(feature = "native-preview")]
        crate::native_preview::resource_probe::PAINTS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // The selection bounds are taken before the grid is locked:
        // reaching for them inside the lock is impossible — it is already
        // held below.
        let restoring = self.restoring.clone();
        if let Some(preview) = &restoring {
            preview.shown(std::time::Instant::now());
        }
        let selection = restoring
            .is_none()
            .then(|| self.emulator.selection_range())
            .flatten();

        let Ok(term) = self.emulator.term().lock() else {
            return;
        };

        #[cfg(feature = "adaptive-terminal-refresh")]
        let captured = restoring
            .is_none()
            .then(|| self.emulator.visual_demand())
            .flatten()
            .map(|d| {
                let capture = d.capture();
                (d, capture)
            });

        #[cfg(feature = "native-preview")]
        crate::native_preview::frame_meter::capture(&term, &self.emulator.frame_generation);

        #[cfg(feature = "native-preview")]
        let held = std::time::Instant::now();
        let content = crate::terminal::geometry::content(bounds);
        let origin = content.origin;

        let colors = restoring
            .as_ref()
            .map_or(term.colors(), |preview| &preview.colors);
        let grid = restoring
            .as_ref()
            .map_or(term.grid(), |preview| &preview.grid);
        // The cursor's state is taken from the guard that is already held.
        // Asking the emulator for it would take the same lock a second
        // time, and a std mutex is not reentrant: the first frame would
        // deadlock and the window would never appear. It did.
        let mut point = grid.cursor.point;
        if grid[point]
            .flags
            .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER)
            && point.column.0 > 0
        {
            point.column.0 -= 1;
        }
        let cursor = Cursor {
            point,
            columns: if grid[point]
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR)
            {
                2
            } else {
                1
            },
            offset: grid.display_offset(),
            screen_lines: grid.screen_lines(),
            style: term.cursor_style(),
            visible: restoring.is_none()
                && term
                    .mode()
                    .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR),
        };
        // The find's matches on the rows shown, from the same guard — a
        // field borrow of the bar, not an accessor that locks again. M5.12.
        if self.find.revision
            != self
                .emulator
                .search_revision()
                .load(std::sync::atomic::Ordering::Acquire)
        {
            self.find.current = None;
        }
        let visible = find_bar::visible_matches(&mut self.find, &term);
        let columns = grid.columns();
        // What a drawn box line is as thick as, and the device pixel its
        // edges are snapped to — task 10.
        let light = (f32::from(font_size) / 12.0).floor().max(1.0);
        let scale = window.scale_factor();

        self.prepared_rows.begin(colors, grid.screen_lines());
        let mut prepared_frame = Vec::with_capacity(grid.screen_lines());
        for (row_index, row) in grid
            .display_iter()
            .collect::<Vec<_>>()
            .chunks(columns)
            .enumerate()
        {
            let prepared = self.prepared_rows.get(
                row_index,
                row.iter().map(|indexed| {
                    let mark = paint::Mark {
                        selected: selection
                            .as_ref()
                            .is_some_and(|range| range.contains(indexed.point)),
                        found: if restoring.is_none() {
                            find_bar::found(&self.find, &visible, indexed.point)
                        } else {
                            paint::Found::No
                        },
                        link: restoring.is_none() && hover::underlined(&self.hover, indexed.point),
                    };
                    (indexed.cell, mark)
                }),
                &self.palette,
                colors,
            );
            prepared_frame.push(prepared);
        }

        let under = (cursor.point.column.0 < columns)
            .then(|| {
                paint::runs_of_row(
                    std::iter::once((&grid[cursor.point], paint::Mark::default())),
                    &self.palette,
                    colors,
                )
            })
            .and_then(|runs| runs.into_iter().next())
            .filter(|run| !run.text.trim().is_empty());
        let bar = scroll::bar(
            grid.total_lines(),
            grid.screen_lines(),
            grid.display_offset(),
            f32::from(content.size.height),
        );
        #[cfg(feature = "native-preview")]
        let observed_text = crate::native_preview::resource_probe::wants_text().then(|| {
            grid.display_iter()
                .map(|cell| cell.cell.c)
                .collect::<String>()
        });
        #[cfg(feature = "native-preview")]
        crate::native_preview::resource_probe::grid_held(held.elapsed());
        self.composition.caret =
            paint::cursor_row(cursor.point.line.0, cursor.offset, cursor.screen_lines).map(|row| {
                Bounds::new(
                    gpui::point(
                        origin.x + cell.width * cursor.point.column.0 as f32,
                        origin.y + cell.height * row as f32,
                    ),
                    cell,
                )
            });
        drop(term);

        // These rows own their values. Shaping, rasterization and Metal scene
        // construction cannot block the PTY parser on the live grid mutex.
        for (row_index, prepared) in prepared_frame.iter().enumerate() {
            let runs = &prepared.runs;
            if runs.is_empty() {
                continue;
            }

            let y = origin.y + cell.height * (row_index as f32);

            // Backgrounds first: text over background, not the other way
            // round. Every edge on a device pixel — task 12: rows fall on
            // fractions of a pixel, and two rows of one colour (`claude`'s
            // prompt box) showed a hairline where their edges met.
            let snap = |value: Pixels| px(sprite::snap(f32::from(value), scale));
            for run in runs {
                let Some(background) = run.background else {
                    continue;
                };
                let x0 = snap(origin.x + cell.width * (run.start_column as f32));
                let x1 = snap(origin.x + cell.width * ((run.start_column + run.width) as f32));
                let y0 = snap(y);
                let y1 = snap(y + cell.height);
                window.paint_quad(quad(
                    Bounds {
                        origin: GpuiPoint { x: x0, y: y0 },
                        size: gpui::Size {
                            width: x1 - x0,
                            height: y1 - y0,
                        },
                    },
                    px(0.0),
                    background,
                    px(0.0),
                    transparent_black(),
                    gpui::BorderStyle::default(),
                ));
            }

            // Every run is painted **at its own place in the grid**
            // rather than after the previous one. A row assembled as one
            // piece rests on the assumption that every glyph is one cell
            // wide; one icon from a fallback font with a different advance
            // and the rest of the row slides away. That is exactly what
            // was visible in the shell prompt: the segment backgrounds
            // parted company with the text.
            for (run_index, run) in runs.iter().enumerate() {
                // **Blocks and box lines are drawn, not typed** — M5.14
                // task 10: taken from the font they stop at its height,
                // and `claude`'s logo showed a line between its rows. The
                // font gets a space in their place; within a run a
                // character stands at its own column, because a wide one
                // ends the run it is in. The run measured is the run
                // typed: a space is shorter than a `▛`, and measuring the
                // original crashed the window.
                let typed = &prepared.typed[run_index];
                for (at, c) in run.text.chars().enumerate() {
                    if let Some(drawn) =
                        sprite::sprite(c, f32::from(cell.width), f32::from(cell.height), light)
                    {
                        let corner = GpuiPoint {
                            x: origin.x
                                + cell.width
                                    * ((run.start_column + if run.natural_width { 0 } else { at })
                                        as f32),
                            y,
                        };
                        paint_sprite(&drawn, corner, run.foreground, scale, window);
                    }
                }
                if run.underline {
                    let decoration = paint::underline_sprite(
                        run,
                        f32::from(cell.width),
                        f32::from(cell.height),
                        1.0 / scale,
                    );
                    paint_sprite(
                        &decoration,
                        gpui::point(origin.x + cell.width * run.start_column as f32, y),
                        run.underline_color.unwrap_or(run.foreground),
                        scale,
                        window,
                    );
                }
                if typed.text.trim().is_empty() {
                    continue;
                }

                // Every glyph at its own column — task 11: one from a
                // fallback font has its own advance, and without this the
                // rest of the run slid off the grid after it.
                let mut shaped = paint::text_run(typed, family, &TextRunStyle);
                shaped.underline = None;
                let line = window.text_system().shape_line(
                    prepared.text[run_index].clone(),
                    font_size,
                    &[shaped],
                    // GPUI's forced advance counts shaped bases, not grid
                    // columns. Keep composed/wide cells natural and isolated.
                    (!run.natural_width).then_some(cell.width),
                );
                let _ = line.paint(
                    GpuiPoint {
                        x: origin.x + cell.width * (run.start_column as f32),
                        y,
                    },
                    cell.height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }

        // **The letter under a block is drawn again, in the background's
        // colour, over the block.** The block used to hide it: a cursor at
        // the end of a prompt sits on an empty cell and nobody saw, and
        // `cmd-left` in a shell — M5.12's task 4 — left `#echo` reading
        // `#▮cho`. Seen in M5.12's window check 3.
        if let (Some(at), Some(mut run)) = (self.paint_cursor(cursor, origin, cell, window), under)
        {
            run.foreground = self.palette.background();
            for c in run.text.chars() {
                if let Some(drawn) =
                    sprite::sprite(c, f32::from(cell.width), f32::from(cell.height), light)
                {
                    paint_sprite(&drawn, at, run.foreground, scale, window);
                }
            }
            let run = paint::typed(&run);
            let shaped = paint::text_run(&run, family, &TextRunStyle);
            let line = window.text_system().shape_line(
                run.text.clone().into(),
                font_size,
                &[shaped],
                None,
            );
            let _ = line.paint(at, cell.height, gpui::TextAlign::Left, None, window, cx);
        }

        #[cfg(feature = "native-preview")]
        if let Some(text) = observed_text {
            #[cfg(feature = "adaptive-terminal-refresh")]
            if crate::native_preview::resource_probe::wants_generations()
                && let Some((_, capture)) = captured.as_ref()
                && let Some(rest) = text.split("Owned policy probe generation ").nth(1)
                && let Ok(marker) = rest.chars().take(6).collect::<String>().parse::<u64>()
            {
                println!(
                    "{{\"terminal_generation\":{},\"epoch\":{},\"content_marker\":{marker}}}",
                    capture.generation, capture.epoch
                );
            }
            crate::native_preview::resource_probe::painted_text(|| text);
        }
        if let Some(bar) = bar {
            self.paint_scrollbar(bar, bounds, window);
        }
        #[cfg(feature = "adaptive-terminal-refresh")]
        if let Some((demand, capture)) = captured {
            // App effects run after this draw, not as a pending frame ticket.
            demand.painted(capture, cx);
        }
    }

    /// Paint the cursor.
    ///
    /// Three things decide what is drawn, and they are separate:
    ///
    /// - **focus** — an unfocused tab gets a hollow block whatever the
    ///   program asked for. There is more than one tab, and it has to be
    ///   visible which one takes the keys.
    /// - **shape** — the program's, through DECSCUSR: `vim` asks for a
    ///   beam in insert mode. Drawing our own regardless would mean
    ///   fighting it over the look of its own interface.
    /// - **blink** — the phase, restarted by typing. A cursor that does
    ///   not blink reads as a picture rather than as a place.
    pub(super) fn paint_cursor(
        &self,
        cursor: Cursor,
        origin: GpuiPoint<Pixels>,
        cell: gpui::Size<Pixels>,
        window: &mut Window,
    ) -> Option<GpuiPoint<Pixels>> {
        // Whatever is drawn below, the blink asks for a repaint only if
        // it is a caret that blinks — and an inactive window's does not.
        let now = std::time::Instant::now();
        self.caret.seen(window.is_window_active(), now);

        // A dead tab keeps its output on screen, but a cursor there would
        // promise input that goes nowhere.
        if self.facts.exited() || !cursor.visible {
            return None;
        }

        let style = cursor.style;
        if style.shape == CursorShape::Hidden {
            return None;
        }

        // Scrolled up, the cursor's line is further down the view or below
        // it: drawn at the screen's own row it would sit on history.
        let row = paint::cursor_row(cursor.point.line.0, cursor.offset, cursor.screen_lines)?;

        let focused = self.focus.is_focused(window);
        // An unfocused tab does not blink: motion in the corner of the eye
        // pulls attention to the window that is not being typed into. Nor
        // does any tab of an inactive window, for the same reason.
        if !self.caret.visible(now, focused && style.blinking) {
            return None;
        }
        let origin = GpuiPoint {
            x: origin.x + cell.width * (cursor.point.column.0 as f32),
            y: origin.y + cell.height * (row as f32),
        };
        let cell = gpui::Size {
            width: cell.width * cursor.columns as f32,
            height: cell.height,
        };
        let colour: Hsla = self.palette.cursor();

        // The bar for a beam and an underline is two pixels rather than
        // one: a single hairline disappears on a Retina screen against the
        // text next to it.
        let bar = px(2.0);
        let (bounds, filled) = match (focused, style.shape) {
            (false, _) => (Bounds { origin, size: cell }, false),
            (true, CursorShape::Beam) => (
                Bounds {
                    origin,
                    size: gpui::Size {
                        width: bar,
                        height: cell.height,
                    },
                },
                true,
            ),
            (true, CursorShape::Underline) => (
                Bounds {
                    origin: GpuiPoint {
                        x: origin.x,
                        y: origin.y + cell.height - bar,
                    },
                    size: gpui::Size {
                        width: cell.width,
                        height: bar,
                    },
                },
                true,
            ),
            (true, CursorShape::HollowBlock) => (Bounds { origin, size: cell }, false),
            (true, _) => (Bounds { origin, size: cell }, true),
        };

        let block = filled && style.shape == CursorShape::Block;
        if filled {
            window.paint_quad(quad(
                bounds,
                px(0.0),
                colour,
                px(0.0),
                transparent_black(),
                gpui::BorderStyle::default(),
            ));
        } else {
            window.paint_quad(quad(
                bounds,
                px(0.0),
                transparent_black(),
                px(1.0),
                colour,
                gpui::BorderStyle::Solid,
            ));
        }
        block.then_some(origin)
    }

    /// Paint the scrollbar down the right edge.
    pub(super) fn paint_scrollbar(
        &self,
        bar: scroll::Bar,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        let opacity = self.scrollbar_opacity();
        if opacity <= 0.0 {
            return;
        }

        let width = px(theme::terminal::SCROLLBAR);
        let content = crate::terminal::geometry::content(bounds);

        let mut colour: Hsla = gpui::rgb(self.theme.text.muted).into();
        colour.a = opacity;

        window.paint_quad(quad(
            Bounds {
                origin: GpuiPoint {
                    x: content.right() - width,
                    y: content.origin.y + px(bar.top),
                },
                size: gpui::Size {
                    width,
                    height: px(bar.height),
                },
            },
            // Rounded to its own width: a rounded end reads as a thumb,
            // a square one as a scrap of another element.
            width / 2.0,
            colour,
            px(0.0),
            transparent_black(),
            gpui::BorderStyle::default(),
        ));
    }

    /// How solid the scrollbar is right now.
    ///
    /// Full while the wheel is turning, then a short fade. An overlay bar
    /// rather than a permanent one: a permanent one takes a column of the
    /// terminal away for something only interesting while scrolling.
    pub(super) fn scrollbar_opacity(&self) -> f32 {
        /// How long the bar stays at full strength after the last scroll.
        const HOLD: Duration = Duration::from_millis(700);
        /// How long it then takes to disappear.
        const FADE: Duration = Duration::from_millis(400);

        let Some(at) = self.scrolled_at else {
            return 0.0;
        };

        let elapsed = at.elapsed();
        if elapsed < HOLD {
            return 1.0;
        }

        let fading = (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32();
        (1.0 - fading).clamp(0.0, 1.0)
    }
}

/// Paint a drawn character at a cell's top left, in its colour.
///
/// **Every edge is snapped to the device pixel**: the rows are 16 on 12
/// and fall on fractions of a pixel, and two blocks one above the other
/// must share an edge exactly, or the line they were drawn to remove
/// comes back as a seam.
fn paint_sprite(
    drawn: &sprite::Sprite,
    corner: GpuiPoint<Pixels>,
    colour: Hsla,
    scale: f32,
    window: &mut Window,
) {
    let snap = |value: Pixels| px(sprite::snap(f32::from(value), scale));
    for piece in &drawn.pieces {
        let x0 = snap(corner.x + px(piece.x0));
        let y0 = snap(corner.y + px(piece.y0));
        let x1 = snap(corner.x + px(piece.x1));
        let y1 = snap(corner.y + px(piece.y1));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        window.paint_quad(quad(
            Bounds {
                origin: GpuiPoint { x: x0, y: y0 },
                size: gpui::Size {
                    width: x1 - x0,
                    height: y1 - y0,
                },
            },
            px(0.0),
            Hsla {
                a: colour.a * piece.alpha,
                ..colour
            },
            px(0.0),
            transparent_black(),
            gpui::BorderStyle::default(),
        ));
    }
    for stroke in &drawn.strokes {
        let mut builder = gpui::PathBuilder::stroke(px(stroke.width));
        let mut points = stroke.points.iter().map(|(x, y)| GpuiPoint {
            x: corner.x + px(*x),
            y: corner.y + px(*y),
        });
        let Some(first) = points.next() else {
            continue;
        };
        builder.move_to(first);
        for point in points {
            builder.line_to(point);
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, colour);
        }
    }
}
