//! What a program asks the terminal, answered — M5.12, task 6.
//!
//! Device attributes, a cursor report and a mode arrive formed from
//! `alacritty_terminal`; a colour needs the palette and a pixel size needs
//! the cell, which only the view has. So a question is a reply waiting to be
//! completed on the tab's own clock, and a size asked before the first
//! paint waits for it. Decision 8 of the plan.

use std::fmt;
use std::sync::Arc;

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::vte::ansi::Rgb;

use super::emulator::GridSize;

/// A closure `alacritty_terminal` hands over to finish an answer with what
/// only the view knows — a colour or the text area in pixels.
///
/// Its own type so that [`Reply`], and `TerminalEvent` around it, keep
/// deriving `Debug`, `Clone`, `PartialEq` and `Eq`: a closure has none of
/// them. **Equal only to itself** — `Arc::ptr_eq` compares the addresses and
/// ignores the vtable — which is all an event needs; a test compares two by
/// what they make.
pub struct Formatter<T>(pub Arc<dyn Fn(T) -> String + Send + Sync>);

impl<T> Clone for Formatter<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> fmt::Debug for Formatter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Formatter")
    }
}

impl<T> PartialEq for Formatter<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// By hand, because a derive would ask `T` for `Eq`, and `Rgb` and
/// `WindowSize` are not asked anything: the closure is what is compared.
impl<T> Eq for Formatter<T> {}

/// What a program asked, as far as the terminal could answer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// An answer already formed — device attributes, a cursor report, a
    /// mode, the size in cells.
    Text(String),
    /// A colour by its index — 0 to 255, then 256 foreground, 257
    /// background, 258 cursor — which the palette completes.
    Colour(usize, Formatter<Rgb>),
    /// The text area in pixels, which the cell completes.
    Size(Formatter<WindowSize>),
}

/// The bytes to send back for `reply`.
///
/// **`None` only for a size asked while there is no `area`** — before the
/// first paint there is no cell, and the pty is still the size it was
/// spawned at, so a number then would describe a grid about to change: a
/// plausible lie. The view holds such a question until it has painted.
pub fn answer(
    reply: &Reply,
    colour: impl Fn(usize) -> Rgb,
    area: Option<WindowSize>,
) -> Option<String> {
    match reply {
        Reply::Text(text) => Some(text.clone()),
        Reply::Colour(index, formatter) => Some((formatter.0)(colour(*index))),
        Reply::Size(formatter) => area.map(|area| (formatter.0)(area)),
    }
}

/// The text area a program is told of: the grid in cells, and the cell in
/// device pixels, `(width, height)`.
pub fn text_area(grid: GridSize, cell_px: (u16, u16)) -> WindowSize {
    WindowSize {
        num_lines: u16::try_from(grid.rows).unwrap_or(u16::MAX),
        num_cols: u16::try_from(grid.cols).unwrap_or(u16::MAX),
        cell_width: cell_px.0,
        cell_height: cell_px.1,
    }
}

/// How many pixel-size questions wait for a cell at most.
///
/// A program asks once and reads one answer, so a handful is every
/// honest case; the bound is for the one that asks in a loop in a tab
/// nobody draws, which would otherwise grow the list for the tab's life.
pub const SIZES_WAITING: usize = 8;

/// Hold a size asked before the first paint, until the cell is known —
/// each question once, and past [`SIZES_WAITING`] none: a program that
/// asked that often will ask again.
pub fn wait_for_cell(waiting: &mut Vec<Formatter<WindowSize>>, formatter: Formatter<WindowSize>) {
    if waiting.len() < SIZES_WAITING {
        waiting.push(formatter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::emulator::{Emulator, GridSize, TerminalEvent};
    use alacritty_terminal::event::WindowSize;
    use alacritty_terminal::grid::Dimensions as _;
    use alacritty_terminal::vte::ansi::Rgb;
    use portable_pty::CommandBuilder;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn area() -> WindowSize {
        WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 16,
            cell_height: 34,
        }
    }

    /// **A pixel size waits for a cell** — decision 8. Until the first
    /// paint there is no cell and the pty is still the size it was spawned
    /// at, so a number sent then would describe a grid about to change.
    #[test]
    fn a_pixel_size_waits_for_a_cell() {
        let size = Reply::Size(Formatter(Arc::new(|area: WindowSize| {
            format!(
                "\x1b[4;{};{}t",
                area.num_lines * area.cell_height,
                area.num_cols * area.cell_width
            )
        })));
        let black = |_| Rgb { r: 0, g: 0, b: 0 };

        assert_eq!(answer(&size, black, None), None, "no cell, no answer yet");
        assert_eq!(
            answer(&size, black, Some(area())).as_deref(),
            Some("\x1b[4;816;1280t")
        );
        assert_eq!(
            answer(&Reply::Text("\x1b[?6c".into()), black, None).as_deref(),
            Some("\x1b[?6c"),
            "a formed text needs nothing"
        );

        let cells = text_area(GridSize::new(80, 24), (16, 34));
        assert_eq!(
            (
                cells.num_cols,
                cells.num_lines,
                cells.cell_width,
                cells.cell_height
            ),
            (80, 24, 16, 34)
        );
    }

    /// **What waits for a cell is bounded.** Each question is answered
    /// once, so each waits; but a program that asks again and again in a
    /// restored tab nobody has drawn would grow the list for as long as
    /// the tab lives. Past [`SIZES_WAITING`] a question is dropped — a
    /// program that asked that often will ask again.
    #[test]
    fn what_waits_for_a_cell_is_bounded() {
        let mut waiting = Vec::new();
        for _ in 0..SIZES_WAITING + 5 {
            wait_for_cell(
                &mut waiting,
                Formatter(Arc::new(|_: WindowSize| String::new())),
            );
        }
        assert_eq!(waiting.len(), SIZES_WAITING);
    }

    /// What the grid shows, row by row.
    fn screen(emulator: &Emulator) -> String {
        let term = emulator.term().lock().expect("the grid's lock");
        let columns = term.grid().columns();
        let mut text = String::new();
        for (index, indexed) in term.grid().display_iter().enumerate() {
            if index > 0 && index % columns == 0 {
                text.push('\n');
            }
            text.push(indexed.cell.c);
        }
        text
    }

    /// **A reply reaches the program that asked** — M5.12 task 6, the one
    /// pty test. `sh` puts its terminal in raw mode, asks for the cursor
    /// from a known place and then for the device attributes, and reads
    /// exactly as many bytes as each answer has, printing them with `od`.
    /// The test plays the tab's clock: it drains the events and sends back
    /// what `answer` makes of each. Without the answers `dd` waits, and
    /// the wait is ended by the clock — the trap.
    #[test]
    fn a_reply_reaches_the_program_that_asked() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-c");
        command.arg(
            "stty raw -echo; printf '\\033[H\\033[6n'; dd bs=1 count=6 2>/dev/null | od -An -c; \
             printf '\\033[c'; dd bs=1 count=5 2>/dev/null | od -An -c; sleep 5",
        );
        let mut emulator =
            Emulator::spawn(command, std::path::Path::new("/"), GridSize::new(120, 8))
                .expect("sh starts");

        let cursor = "033   [   1   ;   1   R";
        let attributes = "033   [   ?   6   c";
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut answered = 0;
        let mut seen = String::new();
        while Instant::now() < deadline {
            for event in emulator.drain_events() {
                if let TerminalEvent::Reply(reply) = event
                    && let Some(text) = answer(&reply, |_| Rgb { r: 0, g: 0, b: 0 }, Some(area()))
                {
                    emulator.send(text.as_bytes());
                    answered += 1;
                }
            }
            seen = screen(&emulator);
            if seen.contains(cursor) && seen.contains(attributes) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(seen.contains(cursor), "the cursor report: {seen}");
        assert!(seen.contains(attributes), "the device attributes: {seen}");
        assert_eq!(answered, 2, "one answer per question");
    }
}
