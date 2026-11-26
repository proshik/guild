//! Headless isolation of viewport preparation cost, excluding shaping/painting.
//! Includes the production cache source without exposing its private API.
use guild_desktop::{native_preview, terminal};
#[path = "../src/terminal/view/prepared_rows.rs"]
mod prepared_rows;
use alacritty_terminal::term::{cell::Cell, color::Colors};
use std::{hint::black_box, time::Instant};
use terminal::{paint, palette::Palette};

fn line(number: usize) -> Vec<Cell> {
    let mut cells = vec![Cell::default(); 110];
    for (cell, c) in cells
        .iter_mut()
        .zip(format!("row {number:06}: terminal output fixture").chars())
    {
        cell.c = c;
    }
    cells
}
fn run(cached: bool) -> f64 {
    let colors = Colors::default();
    let palette = Palette::new(Default::default());
    let mut cache = prepared_rows::Rows::default();
    let mut grid: Vec<_> = (0..46).map(line).collect();
    let mut sequence = 46;
    let mut elapsed = std::time::Duration::ZERO;
    for frame in 0..2100 {
        for _ in 0..if frame % 3 == 0 { 1 } else { 2 } {
            grid.rotate_left(1);
            grid[45] = line(sequence);
            sequence += 1;
        }
        let started = Instant::now();
        if cached {
            cache.begin(&colors, grid.len());
        }
        for (index, row) in grid.iter().enumerate() {
            let cells = row.iter().map(|cell| (cell, paint::Mark::default()));
            if cached {
                let prepared = cache.get(index, cells, &palette, &colors);
                black_box((&prepared.runs, &prepared.typed, &prepared.text));
            } else {
                native_preview::resource_probe::ROWS
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let runs = paint::runs_of_row(cells, &palette, &colors);
                for run in &runs {
                    let typed = paint::typed(run);
                    if !typed.text.trim().is_empty() {
                        black_box(gpui::SharedString::from(typed.text));
                    }
                }
                black_box(runs);
            }
        }
        if frame >= 100 {
            elapsed += started.elapsed();
        }
    }
    elapsed.as_secs_f64() * 1_000_000. / 2000.
}
fn main() {
    for trial in 0..6 {
        let cached = trial % 2 != 0;
        let micros = run(cached);
        println!(
            "{{\"trial\":{trial},\"cached\":{cached},\"microseconds_per_frame\":{micros:.3}}}"
        );
    }
}
