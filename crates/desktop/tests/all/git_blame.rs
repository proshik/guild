//! What `git blame --porcelain` says, parsed on captured output. The
//! fixture is from the scratch repository of `tests/git_log.rs`: a file
//! renamed in its history, a commit that added two lines in a row, an
//! author with a non-ASCII name, and a line not committed yet.

use guild_desktop::git::blame::{Run, parse};

const BLAME: &[u8] = include_bytes!("../fixtures/blame.bin");

#[test]
fn omitted_filename_reuses_the_hash_metadata_across_nonconsecutive_groups() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let text = format!(
        "{a} 1 1 1\nauthor A\nauthor-time 1\nsummary first\nfilename old/one.txt\n\tone\n{b} 1 2 1\nauthor B\nauthor-time 2\nfilename other.txt\n\ttwo\n{a} 9 3 1\n\tthree\n{a} 10 4 1\nfilename old/renamed.txt\n\tfour\n{b} 2 5 1\n\tfive\n{a} 11 6 1\n\tsix\n"
    );
    let runs = parse(text.as_bytes()).unwrap();
    let lines = guild_desktop::git::blame::map(&runs, "one\ntwo\nthree\nfour\nfive\nsix\n");
    for (row, path, original) in [
        (2, "old/one.txt", 9),
        (3, "old/renamed.txt", 10),
        (4, "other.txt", 2),
        (5, "old/renamed.txt", 11),
    ] {
        let line = lines[row].as_ref().unwrap();
        assert_eq!(line.filename, std::path::Path::new(path));
        assert_eq!(line.original_line, original);
    }
}

#[test]
fn normalized_editor_lines_retain_crlf_blame_except_the_edited_line() {
    let text = format!(
        "{} 1 1 2\nauthor A\nauthor-time 1\nfilename file.txt\n\tone\r\n{} 2 2\n\ttwo\r\n",
        "a".repeat(40),
        "a".repeat(40)
    );
    let runs = parse(text.as_bytes()).unwrap();
    let clean = guild_desktop::git::blame::map(&runs, "one\ntwo\n");
    assert!(
        clean[0].is_some() && clean[1].is_some(),
        "unchanged normalized CRLF lines remain committed"
    );
    let edited = guild_desktop::git::blame::map(&runs, "changed\ntwo\n");
    assert!(edited[0].is_none());
    assert_eq!(edited[1].as_ref().unwrap().original_line, 2);
}

fn runs() -> Vec<Run> {
    parse(BLAME).expect("porcelain blame")
}

/// Every line has a commit, an author and a time, and consecutive lines
/// of one commit are one run — the porcelain repeats the hash for each
/// line and gives the author once, and the parser carries it.
#[test]
fn lines_of_one_commit_in_a_row_are_one_run() {
    let runs = runs();
    assert_eq!(runs.len(), 4, "{runs:#?}");
    let lines: Vec<usize> = runs.iter().map(|r| r.lines.len()).collect();
    assert_eq!(lines, [1, 1, 2, 1]);
    let firsts: Vec<usize> = runs.iter().map(|r| r.first_line).collect();
    assert_eq!(firsts, [1, 2, 3, 5], "one-based, as git counts");
    let two = &runs[2];
    assert_eq!(two.sha, "a264f9d411a4e9e7ab40b44f2350f9d675c66e52");
    assert_eq!(two.author, "Иван Петров");
    assert_eq!(two.time, 1_788_383_090);
    assert_eq!(two.summary, "Add two more");
    assert_eq!(two.lines, ["fn third() {}", "fn fourth() {}"]);
    assert!(runs.iter().all(|r| !r.author.is_empty() && r.time > 0));
}

/// A file renamed in its history keeps the older commit for the lines
/// that came through the rename: the first line is the first commit's,
/// made when the file had its old name.
#[test]
fn a_rename_in_the_history_keeps_the_older_commit() {
    let runs = runs();
    assert_eq!(runs[0].sha, "af80cfc59769d486c4f46ba625811001454d1b43");
    assert_eq!(runs[0].summary, "First commit");
    assert_eq!(runs[0].lines, ["fn main() {}"]);
}

/// A line not committed yet is the zero hash, and the run says so
/// rather than naming git's placeholder author.
#[test]
fn an_uncommitted_line_is_the_zero_hash() {
    let runs = runs();
    let last = &runs[3];
    assert!(last.is_uncommitted());
    assert!(runs[..3].iter().all(|r| !r.is_uncommitted()));
    assert_eq!(last.lines, ["// not committed yet"]);
}

/// Bytes that are not the format are an error, and nothing is not a
/// file with no lines.
#[test]
fn what_is_not_porcelain_is_an_error_and_empty_is_empty() {
    assert!(parse(b"fatal: no such path").is_err());
    assert!(parse(b"").expect("an empty file").is_empty());
}

#[test]
fn repeated_hash_with_different_historical_location_is_a_separate_run() {
    let sha = "a".repeat(40);
    let source = format!(
        "{sha} 1 1 1\nauthor Иван Петров\nauthor-time 1788383090\nsummary original\nfilename old.rs\n\tfirst\n{sha} 20 2 1\nfilename \"\\320\\260.rs\"\n\tsecond\n"
    );
    let runs = parse(source.as_bytes()).unwrap();
    assert_eq!(
        runs.len(),
        2,
        "historical original lines are not contiguous"
    );
    assert_eq!(runs[0].filename, std::path::Path::new("old.rs"));
    assert_eq!(runs[1].filename, std::path::Path::new("а.rs"));
    assert_eq!(runs[1].original_line, 20);
    assert_eq!(runs[1].author, "Иван Петров");
}

#[test]
fn unsaved_insertions_and_replacements_never_reuse_a_hash() {
    let runs = runs();
    let text =
        "added\nfn main() {}\nchanged\nfn third() {}\nfn fourth() {}\n// not committed yet\n";
    let mapped = guild_desktop::git::blame::map(&runs, text);
    assert!(mapped[0].is_none());
    assert!(mapped[2].is_none());
    assert!(mapped[5].is_none());
    assert_eq!(mapped[1].as_ref().unwrap().sha, runs[0].sha);
    assert_eq!(
        mapped[3].as_ref().unwrap().original_line,
        runs[2].original_line
    );
    assert!(mapped[3].as_ref().unwrap().compact().contains("Петров"));
    let unchanged =
        guild_desktop::git::blame::map(&runs, "fn main() {}\nfn third() {}\nfn fourth() {}");
    assert_eq!(unchanged[1].as_ref().unwrap().sha, runs[2].sha);
}
