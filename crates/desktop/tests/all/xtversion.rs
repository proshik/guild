//! What the terminal says it is — M5.14, 2026-09-24.
//!
//! `claude` draws a frame whole (synchronized output, mode 2026) only
//! after asking the terminal for it, and it asks only once the terminal
//! has answered XTVERSION (`CSI > q`). vte 0.15 has no handler for that
//! query, so nothing answered, and every repaint went out half-erased —
//! the blink the person saw while a panel was stretched.

use guild_desktop::terminal::xtversion::{Scanner, answer};

#[test]
fn a_version_query_is_found_even_split_across_reads() {
    let mut scanner = Scanner::default();
    assert_eq!(scanner.feed(b"text\x1b[>q more"), 1);
    assert_eq!(scanner.feed(b"\x1b[>0q"), 1);

    let mut split = Scanner::default();
    let total: usize = b"a\x1b[>q".iter().map(|byte| split.feed(&[*byte])).sum();
    assert_eq!(total, 1, "byte by byte");

    assert_eq!(
        Scanner::default().feed(b"\x1b[>1q\x1b[q"),
        0,
        "neither is the query"
    );
}

#[test]
fn the_answer_is_a_dcs_naming_guild() {
    assert_eq!(answer("0.1.0"), "\x1bP>|Guild 0.1.0\x1b\\");
}
