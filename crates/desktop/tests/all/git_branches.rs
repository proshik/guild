//! What `git branch` says in the marked format, parsed on captured
//! output — the scratch repository of `tests/git_log.rs`, which has
//! `main` and `feature`.

use guild_desktop::git::branches::parse;

const BRANCHES: &[u8] = include_bytes!("../fixtures/branches.bin");

/// Every local branch, in git's order, the current one marked by the
/// asterisk git prints for `%(HEAD)`.
#[test]
fn branches_parse_with_the_current_one_marked() {
    let branches = parse(BRANCHES).expect("branch format");
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["feature", "main"]);
    assert!(branches.iter().find(|b| b.name == "main").unwrap().current);
    assert!(
        !branches
            .iter()
            .find(|b| b.name == "feature")
            .unwrap()
            .current
    );
}

/// Bytes that are not the format are an error; no branches yet — a
/// repository with no commit — is an empty list.
#[test]
fn what_is_not_the_format_is_an_error() {
    assert!(parse(b"fatal: not a git repository").is_err());
    assert!(parse(b"").expect("no branches yet").is_empty());
}

/// Since M8.3.3 the format carries the tip as well: who wrote it, how
/// long ago, and its first line — what the branch popup draws under
/// each name.
#[test]
fn a_branch_carries_its_tip() {
    const RICH: &[u8] = include_bytes!("../fixtures/branches-rich.bin");
    let branches = parse(RICH).expect("the richer branch format");
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["main", "unmerged"]);

    let main = branches.iter().find(|b| b.name == "main").unwrap();
    assert!(main.current);
    assert_eq!(main.author, "Sam Example");
    assert_eq!(main.subject, "a file to stage in halves");
    assert!(main.age.ends_with("ago"), "git's own words: {}", main.age);

    let other = branches.iter().find(|b| b.name == "unmerged").unwrap();
    assert!(!other.current);
    assert_eq!(other.subject, "a commit only this branch has");
}

/// A capture from before the tip was asked for still parses, and says
/// nothing about a tip rather than refusing.
#[test]
fn a_capture_from_before_the_tip_still_parses() {
    let branches = parse(BRANCHES).expect("the M8.3.1 format");
    assert!(branches.iter().all(|b| b.author.is_empty()));
    assert!(branches.iter().all(|b| b.subject.is_empty()));
}
