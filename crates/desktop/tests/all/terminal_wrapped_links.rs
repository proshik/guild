//! A link an agent CLI wrapped onto the next row itself.
//!
//! Claude Code and Codex lay out their own text: a long path or address is
//! broken with a newline and the next row starts with an indent, so the
//! grid has no `WRAPLINE` between the two halves. The shapes here are the
//! ones measured on 2026-11-25 by recording each CLI's bytes in a pty a
//! hundred columns wide and replaying them into the emulator — Claude Code
//! 2.1.332 and codex-cli 0.172.0, asked to print a list of two long
//! absolute paths and a long URL:
//!
//! - **Claude Code** breaks a path and a URL by character, at the last
//!   column exactly, and starts every continuation row with four spaces —
//!   the message's gutter and the list's hanging indent.
//! - **Codex** breaks a URL the same way, at the last column, but a path
//!   after a hyphen, as text is wrapped: the row ends where the next piece
//!   would not fit, eleven or twelve columns short of the edge. Its
//!   continuation rows start with four spaces too.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use guild_desktop::terminal::emulator::GridSize;
use guild_desktop::terminal::links::{self, Found, Target};

const COLUMNS: usize = 100;

fn term_with(bytes: &str) -> Term<VoidListener> {
    let mut term = Term::new(Config::default(), &GridSize::new(COLUMNS, 12), VoidListener);
    let mut parser: Processor<StdSyncHandler> = Processor::new();
    parser.advance(&mut term, bytes.as_bytes());
    term
}

fn at(line: i32, column: usize) -> Point {
    Point::new(Line(line), Column(column))
}

fn is_file(found: &Found) -> bool {
    matches!(&found.target, Target::File { path, .. } if path.is_file())
}

fn bare(path: &Path) -> Target {
    Target::File {
        path: path.to_path_buf(),
        line: None,
        column: None,
    }
}

/// Claude Code's shape: `text` cut by character at the last column, the
/// first row after `prefix`, every other after `indent`.
fn by_character(prefix: &str, indent: &str, text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut rows = Vec::new();
    let mut lead = prefix;
    let mut rest = &chars[..];
    while !rest.is_empty() {
        let room = COLUMNS - lead.chars().count();
        let take = room.min(rest.len());
        rows.push(format!("{lead}{}", rest[..take].iter().collect::<String>()));
        rest = &rest[take..];
        lead = indent;
    }
    rows
}

/// Codex's shape for a path: `text` broken after a hyphen, each row taking
/// as many pieces as fit in the width.
fn after_hyphens(prefix: &str, indent: &str, text: &str) -> Vec<String> {
    let mut rows = vec![prefix.to_owned()];
    for piece in text.split_inclusive('-') {
        let row = rows.last_mut().unwrap();
        if row.chars().count() + piece.chars().count() > COLUMNS {
            rows.push(format!("{indent}{piece}"));
        } else {
            row.push_str(piece);
        }
    }
    rows
}

/// A file with a path long enough to take three rows of a hundred columns,
/// named like the generated screenshot paths that showed the bug.
fn long_file(base: &Path) -> PathBuf {
    let dir = base.join(
        "build-cache/-Users-someone-work-projects-atlas-guild-terminal-wrapped-links\
         /958745ca-f001-43d2-b47e-80bc853b9814/artifacts/files\
         /3cb1438d-16de-469c-9a7e-0123456789ab/screens",
    );
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("proposal-1-all-expanded.png");
    std::fs::write(&file, "png").unwrap();
    assert!(file.to_str().unwrap().len() > 2 * COLUMNS, "{file:?}");
    file
}

/// The runs the rows of `rows` give `text` when it starts after `prefix`
/// and continues after `indent`, from grid line 0.
fn runs_of(rows: &[String], prefix: &str, indent: &str) -> Vec<(Point, Point)> {
    rows.iter()
        .enumerate()
        .map(|(line, row)| {
            let lead = if line == 0 { prefix } else { indent }.chars().count();
            (
                at(line as i32, lead),
                at(line as i32, row.chars().count() - 1),
            )
        })
        .collect()
}

/// Every row of the link finds the whole of it, and nothing else does.
fn assert_one_link_over(term: &Term<VoidListener>, runs: &[(Point, Point)], target: &Target) {
    for (first, last) in runs {
        let middle = at(first.line.0, (first.column.0 + last.column.0) / 2);
        for point in [*first, middle, *last] {
            let found =
                links::at(term, point, is_file).unwrap_or_else(|| panic!("no link at {point:?}"));
            assert_eq!(&found.target, target, "at {point:?}");
            assert_eq!(found.runs(), runs, "at {point:?}");
        }
    }
    let found = links::at(term, runs[0].0, is_file).unwrap();
    for (first, _) in &runs[1..] {
        for column in 0..first.column.0 {
            let indent = at(first.line.0, column);
            assert!(
                !found.covers(indent),
                "the indent at {indent:?} is not underlined"
            );
            assert_eq!(links::at(term, indent, is_file), None, "{indent:?}");
        }
    }
}

#[test]
fn claude_code_s_path_wrapped_at_the_edge_is_one_link_over_its_three_rows() {
    let base = Scratch::new("guild-wrapped-");
    let file = long_file(&base);
    let text = file.to_str().unwrap();
    let rows = by_character("⏺ - ", "    ", text);
    assert_eq!(rows.len(), 3, "{rows:#?}");
    let term = term_with(&format!("{}\r\n  - the next item\r\n", rows.join("\r\n")));

    assert_one_link_over(&term, &runs_of(&rows, "⏺ - ", "    "), &bare(&file));
    assert_eq!(links::at(&term, at(0, 0), is_file), None, "the gutter");
}

#[test]
fn codex_s_path_wrapped_after_a_hyphen_is_one_link_over_its_rows() {
    let base = Scratch::new("guild-wrapped-");
    let file = long_file(&base);
    let text = file.to_str().unwrap();
    let rows = after_hyphens("• • ", "    ", text);
    assert!(rows.len() >= 3, "{rows:#?}");
    assert!(
        rows[..rows.len() - 1]
            .iter()
            .any(|row| row.chars().count() < COLUMNS),
        "Codex leaves the edge empty: {rows:#?}"
    );
    let term = term_with(&format!("{}\r\n\r\n  • next\r\n", rows.join("\r\n")));

    assert_one_link_over(&term, &runs_of(&rows, "• • ", "    "), &bare(&file));
}

#[test]
fn a_url_either_cli_wrapped_at_the_edge_is_one_link() {
    let url = "https://example.com/guild/terminal/wrapped-links/a-rather-long-address\
               /that-an-agent-prints/when-it-reports/index.html?query=value&other=1";
    for prefix in ["⏺ - ", "  • "] {
        let rows = by_character(prefix, "    ", url);
        assert_eq!(rows.len(), 2);
        let term = term_with(&format!("{}\r\n", rows.join("\r\n")));

        assert_one_link_over(
            &term,
            &runs_of(&rows, prefix, "    "),
            &Target::Url(url.to_owned()),
        );
    }
}

#[test]
fn a_joined_path_that_names_no_file_leaves_each_row_as_it_was() {
    let base = Scratch::new("guild-wrapped-");
    let file = long_file(&base);
    let text = file.to_str().unwrap();
    let rows = by_character("⏺ - ", "    ", text);
    let term = term_with(&rows.join("\r\n"));

    // The disk says no: each row is the candidate it always was.
    let found = links::at(&term, at(0, 10), |_| false).unwrap();
    assert_eq!(found.runs(), [(at(0, 4), at(0, COLUMNS - 1))]);
    let found = links::at(&term, at(1, 10), |_| false).unwrap();
    assert_eq!(found.runs(), [(at(1, 4), at(1, COLUMNS - 1))]);

    // A file that is a link on its own row stays one when the next row,
    // indented, begins with a word that is not its continuation.
    let alone = base.join("r.md");
    std::fs::write(&alone, "").unwrap();
    let row = format!("see {}", alone.display());
    assert!(
        row.len() < COLUMNS - 4,
        "the temporary path is short enough: {row}"
    );
    let pad = " ".repeat(COLUMNS - row.chars().count());
    let term = term_with(&format!("{pad}{row}\r\n    and-then more prose\r\n"));
    let found = links::at(&term, at(0, COLUMNS - 3), is_file).unwrap();
    assert_eq!(found.target, bare(&alone));
    assert_eq!(found.runs().len(), 1);
}

#[test]
fn a_row_that_ends_where_its_continuation_would_have_fit_is_not_joined() {
    // `a.txt` fits in the eighty-odd empty columns after `/tmp/x/`: no
    // program wrapped it there, whatever the disk would say.
    let term = term_with("  see /tmp/x/\r\n    a.txt\r\n");
    let found = links::at(&term, at(0, 8), |_| true).unwrap();
    assert_eq!(found.target, bare(Path::new("/tmp/x/")));
    assert_eq!(found.runs(), [(at(0, 6), at(0, 12))]);
}

#[test]
fn a_url_is_joined_only_at_the_edge_onto_what_looks_like_an_address() {
    let url = "https://example.com/docs/guide";
    let edge = |row: &str| format!("{}{row}", " ".repeat(COLUMNS - row.chars().count()));
    let single = |term: &Term<VoidListener>| {
        let found = links::at(term, at(0, COLUMNS - 2), |_| true).unwrap();
        assert_eq!(found.runs().len(), 1, "{found:?}");
        found.target
    };
    for next in [
        // Prose after an address that happened to end at the edge.
        "  and then some prose",
        "  here.",
        "  README",
        // Not indented: not a continuation.
        "/more/of-it",
    ] {
        let term = term_with(&format!("{}\r\n{next}\r\n", edge(&format!("see {url}"))));
        assert_eq!(single(&term), Target::Url(url.to_owned()), "{next:?}");
    }

    // One column short of the edge, the address ended there.
    let term = term_with(&format!(
        "{} \r\n    /more/of-it\r\n",
        &edge(&format!("see {url}"))[1..]
    ));
    let found = links::at(&term, at(0, COLUMNS - 3), |_| true).unwrap();
    assert_eq!(found.target, Target::Url(url.to_owned()));

    // At the edge, onto a piece of an address: one link.
    let term = term_with(&format!(
        "{}\r\n    /more/of-it now\r\n",
        edge(&format!("see {url}"))
    ));
    let found = links::at(&term, at(1, 6), |_| true).unwrap();
    assert_eq!(found.target, Target::Url(format!("{url}/more/of-it")));
}

#[test]
fn an_osc8_hyperlink_keeps_its_cells_at_the_edge() {
    let label = "a-label-that-ends-at-the-edge";
    let pad = " ".repeat(COLUMNS - label.len());
    let term = term_with(&format!(
        "{pad}\x1b]8;;https://example.org/osc8\x1b\\{label}\x1b]8;;\x1b\\\r\n    /tail/more\r\n"
    ));
    let found = links::at(&term, at(0, COLUMNS - 1), |_| true).unwrap();
    assert_eq!(
        found.target,
        Target::Url("https://example.org/osc8".to_owned())
    );
    assert_eq!(found.runs(), [(at(0, pad.len()), at(0, COLUMNS - 1))]);

    let found = links::at(&term, at(1, 6), |_| true).unwrap();
    assert_eq!(found.target, bare(Path::new("/tail/more")));
    assert_eq!(found.runs(), [(at(1, 4), at(1, 13))]);
}

/// What both CLIs print for an address in a reply: an OSC 8 hyperlink.
/// Claude Code writes it again on each row under one id; Codex writes it
/// once and moves the cursor to the next row inside it. A click already
/// had the whole address; the underline is on every row now.
#[test]
fn an_osc8_hyperlink_either_cli_wrapped_is_underlined_on_every_row() {
    let url = "https://example.com/guild/terminal/wrapped-links/a-rather-long-address\
               /that-an-agent-prints/when-it-reports/index.html?query=value&other=1";
    let rows = by_character("⏺ - ", "    ", url);
    let claude = format!(
        "⏺ - \x1b]8;id=1s3nb7d;{url}\x07{}\x1b]8;;\x07\r\x1b[4C\x1b[1B\x1b]8;id=1s3nb7d;{url}\x07{}\x1b]8;;\x07\r\n",
        &rows[0][rows[0].find("https").unwrap()..],
        rows[1].trim_start(),
    );
    let codex = format!(
        "  • \x1b]8;;{url}\x07{}\x1b[2;5H{}\x1b]8;;\x07\r\n",
        &rows[0][rows[0].find("https").unwrap()..],
        rows[1].trim_start(),
    );
    for bytes in [claude, codex] {
        let term = term_with(&bytes);
        let runs = runs_of(&rows, "⏺ - ", "    ");
        for point in [at(0, 4), at(0, 99), at(1, 4), at(1, 20)] {
            let found = links::at(&term, point, is_file).unwrap();
            assert_eq!(found.target, Target::Url(url.to_owned()), "{point:?}");
            assert_eq!(found.runs(), runs, "{point:?}");
        }
        assert_eq!(links::at(&term, at(1, 2), is_file), None, "the indent");
    }

    // Two hyperlinks are two, even to one address, and one with text after
    // it on its row was not broken there.
    let other = format!(
        "{}\x1b]8;id=a;{url}\x07x\x1b]8;;\x07\r\n    \x1b]8;id=b;{url}\x07y\x1b]8;;\x07\r\n",
        " ".repeat(COLUMNS - 1)
    );
    let term = term_with(&other);
    assert_eq!(
        links::at(&term, at(0, 99), is_file).unwrap().runs().len(),
        1
    );
    let after = format!(
        "\x1b]8;id=a;{url}\x07x\x1b]8;;\x07 then\r\n    \x1b]8;id=a;{url}\x07y\x1b]8;;\x07\r\n"
    );
    let term = term_with(&after);
    assert_eq!(links::at(&term, at(0, 0), is_file).unwrap().runs().len(), 1);
}
