//! What `git stash list` says in the marked format, parsed on captured
//! output — three entries of a scratch repository, one of them named by
//! git itself because the person gave it no message.

use guild_desktop::git::stashes::{Stash, parse, reference_of};

/// Captured from `git version 2.50.1 (Apple Git-155)` with
/// `stash list --format=%gd%x00%H%x00%ar%x00%gs`, on a scratch repository
/// of three stashes — the shape task 0.1 of M5.11 recorded with `od -c`:
/// a NUL between fields, a newline between entries, nothing after the
/// last. Inline rather than a fixture file: the bytes are what the test
/// is about, and a line of them is easier to read here than in `od`.
const STASHES: &[u8] = b"stash@{0}\x00708c4c945349fbd9c22c91456a50d96a0bfafa20\x000 seconds ago\x00On main: trying the popup\nstash@{1}\x003454355cdd398d5a8e924c39f94bb8f08eae063c\x000 seconds ago\x00On main: the second one\nstash@{2}\x009d21219f4af1191a5e7bef9fa21af100bb001a54\x000 seconds ago\x00WIP on main: 182cae1 one\n";

fn entry(reference: &str, sha: &str) -> Stash {
    Stash {
        reference: reference.to_owned(),
        sha: sha.to_owned(),
        age: "2 hours ago".to_owned(),
        subject: "On main: something".to_owned(),
    }
}

/// Newest first, which is the order git gives and the order the popup
/// shows; each with the reference any later command is given.
#[test]
fn stashes_parse_newest_first_with_their_references() {
    let stashes = parse(STASHES).expect("the stash format");
    let refs: Vec<&str> = stashes.iter().map(|s| s.reference.as_str()).collect();
    assert_eq!(refs, ["stash@{0}", "stash@{1}", "stash@{2}"]);

    assert_eq!(stashes[1].subject, "On main: the second one");
    assert!(
        stashes[2].subject.starts_with("WIP on main:"),
        "git names an unnamed stash itself: {}",
        stashes[2].subject
    );
    assert!(stashes.iter().all(|s| s.age.ends_with("ago")));
}

/// **An entry carries the commit it is.** A reference is a position, and
/// the list shifts under it; the hash is the entry, which is what a drop
/// finds its position by at the moment it runs. Decision 17 of M5.11.
#[test]
fn an_entry_carries_the_commit_it_is() {
    let stashes = parse(STASHES).expect("the stash format");
    assert_eq!(stashes[1].sha, "3454355cdd398d5a8e924c39f94bb8f08eae063c");
    assert!(
        stashes
            .iter()
            .all(|s| s.sha.len() == 40 && s.sha.chars().all(|c| c.is_ascii_hexdigit())),
        "{stashes:?}"
    );
}

/// A drop is carried by its commit, and finds where that commit is *now*:
/// a stash pushed after the list was read moves every entry down one.
#[test]
fn a_drop_finds_its_entry_after_the_list_moved() {
    let now = [
        entry("stash@{0}", "cccc"),
        entry("stash@{1}", "aaaa"),
        entry("stash@{2}", "bbbb"),
    ];
    assert_eq!(reference_of(&now, "aaaa"), Some("stash@{1}"));
    assert_eq!(reference_of(&now, "bbbb"), Some("stash@{2}"));
}

/// An entry dropped elsewhere since the list was read is not found, and
/// its old position — now somebody else's — is never the answer.
#[test]
fn an_entry_no_longer_listed_is_not_found() {
    let now = [entry("stash@{0}", "cccc"), entry("stash@{1}", "bbbb")];
    assert_eq!(reference_of(&now, "aaaa"), None);
    assert_eq!(reference_of(&[], "aaaa"), None);
}

/// Bytes that are not the format are an error; an empty stash is an
/// empty list, which is the common case and not a failure.
#[test]
fn what_is_not_the_format_is_an_error() {
    assert!(parse(b"stash@{0} no NUL anywhere").is_err());
    assert!(
        parse(b"stash@{0}\x002 days ago\x00On main: the old format").is_err(),
        "a line without the hash is not this format"
    );
    assert!(parse(b"").expect("nothing stashed").is_empty());
}

/// A message with a NUL in it — a person's own words, and nothing may
/// be assumed about them — stays whole rather than cutting the line
/// short.
#[test]
fn a_subject_keeps_whatever_is_in_it() {
    let line =
        b"stash@{0}\x00708c4c945349fbd9c22c91456a50d96a0bfafa20\x002 days ago\x00On main: a\x00b";
    let stashes = parse(line).expect("still the format");
    assert_eq!(stashes[0].subject, "On main: a\0b");
}
