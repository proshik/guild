//! The annotate tab's two pure parts: the lines its list draws and how
//! dim each run is.

use std::path::Path;

use guild_desktop::annotate_tab::{lines, shades};
use guild_desktop::git::blame::Run;

fn run(sha: &str, time: i64, count: usize) -> Run {
    Run {
        sha: sha.repeat(40),
        author: "a".into(),
        time,
        summary: "s".into(),
        first_line: 1,
        original_line: 1,
        filename: "src/lib.rs".into(),
        lines: (0..count).map(|i| format!("line {i}")).collect(),
    }
}

/// One row per line, each knowing its run and its place in it.
#[test]
fn every_line_knows_its_run_and_its_place() {
    let runs = vec![run("a", 10, 2), run("b", 20, 1), run("c", 30, 3)];
    assert_eq!(
        lines(&runs),
        [(0, 0), (0, 1), (1, 0), (2, 0), (2, 1), (2, 2)]
    );
    assert!(lines(&[]).is_empty());
}

/// The newest third is drawn plainly and the oldest third dimmest, by
/// rank among the file's commits; a line not committed yet is newest.
#[test]
fn runs_are_shaded_by_rank_newest_plain_oldest_dim() {
    let runs = vec![
        run("a", 100, 1),
        run("b", 200, 1),
        run("c", 300, 1),
        run("d", 400, 1),
        run("e", 500, 1),
        run("f", 600, 1),
        run("0", 0, 1),
    ];
    assert_eq!(shades(&runs), [2, 2, 1, 1, 0, 0, 0]);
    let one = vec![run("a", 100, 1), run("a", 100, 1)];
    assert_eq!(
        shades(&one),
        [0, 0],
        "one commit has nothing to be older than"
    );
}

/// The text a blame describes is its runs' lines in order — which is what
/// the spans are taken from, and the reason Annotate needs no guard of
/// the diff's kind: the line drawn *is* the line parsed.
#[test]
fn the_text_of_a_blame_is_its_runs_in_order() {
    let runs = vec![
        Run {
            sha: "a".repeat(40),
            author: "One".into(),
            time: 1,
            summary: "first".into(),
            first_line: 1,
            original_line: 1,
            filename: "src/lib.rs".into(),
            lines: vec!["fn main() {".into(), "    let x = 1;".into()],
        },
        Run {
            sha: "b".repeat(40),
            author: "Two".into(),
            time: 2,
            summary: "second".into(),
            first_line: 3,
            original_line: 3,
            filename: "src/lib.rs".into(),
            lines: vec!["}".into()],
        },
    ];
    assert_eq!(
        guild_desktop::annotate_tab::text(&runs),
        "fn main() {\n    let x = 1;\n}\n"
    );
}

/// There is one row of spans for every line the tab draws, and they line
/// up with `lines` — an off-by-one here would colour each line with its
/// neighbour's colours, which reads as a highlighter that is simply wrong.
#[test]
fn every_drawn_line_has_its_own_row_of_spans() {
    let runs = vec![Run {
        sha: "a".repeat(40),
        author: "One".into(),
        time: 1,
        summary: "first".into(),
        first_line: 1,
        original_line: 1,
        filename: "src/lib.rs".into(),
        lines: vec!["fn main() {".into(), "    let x = 1;".into(), "}".into()],
    }];
    let spans = guild_desktop::annotate_tab::spans(Path::new("src/lib.rs"), &runs);
    assert_eq!(spans.len(), guild_desktop::annotate_tab::lines(&runs).len());
    assert!(
        spans[0].iter().any(|span| span.paint.is_some()),
        "the first line is coloured"
    );
    assert!(
        guild_desktop::annotate_tab::spans(Path::new("notes.txt"), &runs)
            .iter()
            .all(|row| row.is_empty()),
        "a file no language claims is plain, and still one row per line"
    );
}
