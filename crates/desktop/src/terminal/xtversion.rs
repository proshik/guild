//! What the terminal says it is: XTVERSION, `CSI > q` — M5.14.
//!
//! `claude` draws a frame whole (synchronized output, mode 2026) only
//! after asking the terminal whether it can, and it asks only once the
//! terminal has answered this query. vte 0.15 has no handler for it, so
//! the reader looks for it in the bytes itself; `alacritty_terminal`
//! answers the DECRQM 2026 that follows. Pure.

/// Finds the query in a stream of reads, a split one included.
///
/// A state per byte rather than a carry of bytes: the query is five bytes
/// at most, and where a read ended is where the state stands.
#[derive(Debug, Default)]
pub struct Scanner {
    state: u8,
}

impl Scanner {
    /// How many complete queries — `ESC [ > q` or `ESC [ > 0 q` — these
    /// bytes finished.
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut found = 0;
        for &byte in bytes {
            self.state = match (self.state, byte) {
                (_, 0x1b) => 1,
                (1, b'[') => 2,
                (2, b'>') => 3,
                (3, b'0') => 4,
                (3 | 4, b'q') => {
                    found += 1;
                    0
                }
                _ => 0,
            };
        }
        found
    }
}

/// The answer: a DCS naming the terminal and its version, as xterm's is.
pub fn answer(version: &str) -> String {
    format!("\x1bP>|Guild {version}\x1b\\")
}
