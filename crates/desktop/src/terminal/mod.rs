//! The terminal: ours, on top of `alacritty_terminal`.
//!
//! The off-the-shelf `gpui-terminal` 0.1.0 was tried and dropped. It does
//! not handle `Flags::DIM` — and `claude` sends that flag dozens of times,
//! it is how hints are drawn — so they came out at full brightness,
//! indistinguishable from typed text. `INVERSE`, `HIDDEN` and `STRIKEOUT`
//! were lost along with `DIM`. It cannot be fixed from outside: the flags
//! are lost inside its own rendering.
//!
//! ANSI parsing is still done by `alacritty_terminal`. Ours here is only what
//! touches the screen: colours, line runs, keys, size.
//!
//! - [`emulator`] — the process, the pty and the grid state. No GPUI: the
//!   grid can be checked by feeding it bytes.
//! - [`palette`] — from an ANSI code to a colour aligned with the design
//!   system.
//! - [`paint`] — from cells into runs of one style.
//! - [`keys`] — from a key press into bytes.
//! - [`mouse`] — reporting the wheel to a program that asked to read the
//!   mouse itself, as `claude` does.
//! - [`scroll`] — from a scroll event into rows of history; a trackpad
//!   sends pixels, not notches.
//! - [`shell`] — which shell to run in an ordinary terminal tab.
//! - [`font`] — which font to draw with: shell prompts need a Nerd Font,
//!   otherwise the glyphs turn into boxes.
//! - [`reply`] — the answers to what a program asks the terminal: its
//!   identity, where the cursor is, a colour, a size. M5.12.
//! - [`find`] — a word found in the history, case and pattern as asked.
//!   M5.12.
//! - [`select`] — a selection by a double-click, a triple-click, a shift-click
//!   and Select All. M5.12.
//! - [`links`] — a URL or a `path:line:column` found in the output. M5.12.
//! - [`facts`] — what the window knows about a tab without reading it.
//! - [`view`] — the whole tab.

pub mod emulator;
pub mod facts;
pub mod failure;
pub mod find;
mod find_worker;
pub mod font;
pub(crate) mod geometry;
pub mod keys;
mod launch_prefix;
pub mod links;
pub mod mouse;
pub(crate) mod ownership;
pub mod paint;
pub mod palette;
pub mod program_status;
pub mod reply;
pub mod resize;
pub mod retiring;
pub mod scroll;
pub mod select;
mod semantic;
pub mod shell;
pub(crate) mod shell_integration;
mod shell_state;
pub mod sprite;
pub mod view;
pub mod xtversion;

#[cfg(feature = "adaptive-terminal-refresh")]
pub(crate) mod refresh;
