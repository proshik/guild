//! What the branch popup shows for what has been typed.

use guild_desktop::branch_list::{Row, main_branch, matching, offers_create, rows};
use guild_desktop::picker::Lit;

fn names() -> Vec<String> {
    ["main", "m8-3-2-menus", "m8-3-3-branches", "release"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// An empty query is the list as git gave it; a query narrows it, in
/// the same order, matching the middle of a name as well as its start.
///
/// **This is the Log's branch column since M8.9**, a docked list that
/// keeps a substring and git's order; the popup ranks by scattered
/// letters through [`rows`], which the tests below pin.
#[test]
fn a_query_narrows_the_list_and_keeps_its_order() {
    let names = names();
    assert_eq!(matching(&names, ""), [0, 1, 2, 3]);
    assert_eq!(matching(&names, "m8"), [1, 2]);
    assert_eq!(matching(&names, "menus"), [1], "the middle of a name");
    assert_eq!(matching(&names, "MAIN"), [0], "case is not a filter");
    assert_eq!(matching(&names, "nothing at all"), [] as [usize; 0]);
}

/// The offer to create appears while what was typed is not already a
/// branch — even when it matches one as a prefix, which is the only
/// behaviour that lets a new name be a prefix of an
/// old one.
#[test]
fn creating_is_offered_for_a_name_that_is_not_there() {
    let names = names();
    assert!(!offers_create(&names, ""), "nothing typed, nothing to make");
    assert!(!offers_create(&names, "main"), "that one exists");
    assert!(offers_create(&names, "ma"), "a prefix is still a new name");
    assert!(offers_create(&names, "m8-3-4-output"));
    assert!(
        !offers_create(&names, "  main  "),
        "the field's spaces are not part of the name"
    );
}

/// **What is typed is offered twice — M12.** A name that is not a branch
/// yet can be made as a branch here, or as a worktree of its own beside
/// the repository; the popup offers both rather than growing a fourth
/// field of the window for the second. Since M8.9 the two rows are rows
/// of [`rows`] — the list's own arithmetic in one place, where it was a
/// count added at three and an offset for the heading at a fourth.
#[test]
fn a_new_name_is_offered_as_a_branch_and_as_a_worktree() {
    let names = names();
    let creates = |query: &str| {
        rows(&names, query)
            .iter()
            .filter(|row| matches!(row, Row::Create | Row::Worktree))
            .count()
    };
    assert_eq!(creates("m8-3-4-output"), 2);
    assert_eq!(creates("ma"), 2, "a prefix is still a new name");
    assert_eq!(creates(""), 0, "nothing typed, nothing to make");
    assert_eq!(creates("main"), 0, "that one exists");
    assert_eq!(creates("bad..name"), 0, "git would refuse it");
}

/// **The *Local* heading is a row of the list, and it is never lit** —
/// M8.9's decision 4. It sits after the two create rows and above the
/// branches there are, and not at all when no branch matched.
#[test]
fn the_local_heading_is_a_row_and_is_never_lit() {
    let names = names();

    let typed = rows(&names, "ma");
    assert_eq!(&typed[..3], &[Row::Create, Row::Worktree, Row::Heading]);
    assert!(
        typed[3..].iter().all(|row| matches!(row, Row::Branch(_))),
        "{typed:?}"
    );
    assert_eq!(Row::Heading.lit(), Lit::Heading);
    assert_eq!(Row::Create.lit(), Lit::Row);
    assert_eq!(Row::Branch(0).lit(), Lit::Row);

    let all = rows(&names, "");
    assert_eq!(
        all[0],
        Row::Heading,
        "nothing typed: the heading, then git's order"
    );
    assert_eq!(
        &all[1..],
        &[
            Row::Branch(0),
            Row::Branch(1),
            Row::Branch(2),
            Row::Branch(3)
        ]
    );

    assert_eq!(
        rows(&names, "zzz-new"),
        vec![Row::Create, Row::Worktree],
        "a heading over nothing is not drawn"
    );
    assert!(rows(&[], "").is_empty());
}

/// A branch is found by its scattered letters — `m833` is `m8-3-3-…` —
/// and the best match is first.
#[test]
fn a_branch_is_found_by_its_scattered_letters() {
    let names = names();

    let found = rows(&names, "m833");
    assert_eq!(
        found,
        vec![Row::Create, Row::Worktree, Row::Heading, Row::Branch(2)]
    );

    let found = rows(&names, "rls");
    assert_eq!(found.last(), Some(&Row::Branch(3)), "r(e)l(ea)s(e)");
}

/// Names git would refuse are not offered. Not git's whole rule — that
/// is a process — but the part a person hits by accident; whatever
/// slips through is refused by git, in git's words.
#[test]
fn a_name_git_would_refuse_is_not_offered() {
    let names = names();
    for bad in [
        "-force",
        ".hidden",
        "ends.",
        "a..b",
        "a//b",
        "with space",
        "tilde~1",
        "caret^",
        "colon:",
        "star*",
        "question?",
        "brack[et",
        "back\\slash",
        "trail/",
        "at@{0}",
        "x.lock",
    ] {
        assert!(!offers_create(&names, bad), "git would refuse {bad}");
    }
    assert!(offers_create(&names, "feature/what-a-person-types"));
}

/// **`main` first, wherever the alphabet would have put it.**
///
/// It is the branch a person looks for first, and `git branch` gives its
/// own alphabetical order, so `main` lands wherever the letter m falls —
/// under `feature/…`, under `m5-6-code-split`, under whatever a repository
/// happens to hold.
#[test]
fn main_is_the_first_branch() {
    let names = ["alpha", "feature-x", "main", "zeta"];

    assert_eq!(order(&names), ["main", "alpha", "feature-x", "zeta"]);
}

/// A repository that never renamed its default branch is the same
/// question with the other word.
#[test]
fn master_goes_first_when_there_is_no_main() {
    let names = ["alpha", "master", "zeta"];

    assert_eq!(order(&names), ["master", "alpha", "zeta"]);
}

/// **`main` wins over `master` when a repository has both**, which is
/// what a repository mid-rename looks like. One of them has to be first
/// and the newer word is the one people are moving to.
#[test]
fn main_wins_over_master() {
    let names = ["master", "main", "alpha"];

    assert_eq!(order(&names), ["main", "master", "alpha"]);
}

/// **Neither is present, so nothing moves.** Task 0 read what git gives
/// there: its own alphabetical order and nothing else. Inventing a first
/// row would be inventing a default branch the repository does not have.
#[test]
fn a_repository_with_neither_is_left_alone() {
    let names = ["alpha", "feature-x", "trunk"];

    assert_eq!(order(&names), ["alpha", "feature-x", "trunk"]);
}

/// The order is about the list, not about where `HEAD` is: the current
/// branch is marked with a tick wherever it sits, and `main` is first
/// even while somebody is working on something else.
#[test]
fn the_current_branch_does_not_move() {
    let names = ["alpha", "main", "zeta"];

    assert_eq!(order(&names), ["main", "alpha", "zeta"]);
}

/// A name that merely starts with `main` is not `main`.
#[test]
fn only_an_exact_name_is_the_default_branch() {
    let names = ["main-old", "maintenance", "main"];

    assert_eq!(order(&names), ["main", "main-old", "maintenance"]);
}

/// The indices `ordered` gives, read back as the names they point at.
fn order(names: &[&str]) -> Vec<String> {
    let owned: Vec<String> = names.iter().map(|name| (*name).to_string()).collect();
    guild_desktop::branch_list::ordered(&owned)
        .into_iter()
        .map(|index| owned[index].clone())
        .collect()
}

/// **The branch a task's worktree starts from** — M13.1, decision 2: the
/// local branch origin's `HEAD` names, else `main`, else `master`, else
/// none, which leaves git its own `HEAD`. Local, never `origin/…`.
#[test]
fn a_task_starts_from_the_local_branch_origin_names_then_main_then_master() {
    let locals = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        main_branch(
            Some("origin/develop"),
            &locals(&["develop", "main", "feat"])
        ),
        Some("develop".to_owned()),
        "origin's word wins"
    );
    assert_eq!(
        main_branch(Some("origin/develop"), &locals(&["main", "feat"])),
        Some("main".to_owned()),
        "a branch origin names and nobody has out locally is not invented"
    );
    assert_eq!(
        main_branch(None, &locals(&["feat", "master", "main"])),
        Some("main".to_owned())
    );
    assert_eq!(
        main_branch(None, &locals(&["feat", "master"])),
        Some("master".to_owned())
    );
    assert_eq!(main_branch(None, &locals(&["feat", "trunk"])), None);
    // A remote that is not called origin still names a branch after its
    // own prefix.
    assert_eq!(
        main_branch(Some("upstream/trunk"), &locals(&["trunk", "main"])),
        Some("trunk".to_owned())
    );
}

/// Update branch's sources are named as a person reads them: a remote
/// branch as `origin/main`, a local one as `main` — never `refs/`,
/// `heads/` or `remotes/`.
#[test]
fn a_source_is_named_without_its_ref_prefix() {
    use guild_desktop::branch_list::source_name;
    assert_eq!(source_name("refs/remotes/origin/main"), "origin/main");
    assert_eq!(source_name("refs/heads/main"), "main");
    assert_eq!(
        source_name("refs/heads/feature/remotes/x"),
        "feature/remotes/x"
    );
    assert_eq!(source_name("refs/tags/v1"), "tags/v1");
    assert_eq!(source_name("main"), "main");
}

fn sources() -> Vec<String> {
    [
        "refs/heads/feature",
        "refs/heads/main",
        "refs/heads/master",
        "refs/heads/release",
        "refs/remotes/origin/feature",
        "refs/remotes/origin/main",
        "refs/remotes/origin/master",
        "refs/remotes/upstream/main",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// With nothing typed, the repository's default source leads, then the
/// other `main`/`master` refs — remote before local — then everything
/// else in the order git gave it.
#[test]
fn the_default_source_and_main_lead_an_empty_query() {
    use guild_desktop::branch_list::source_rows;
    let sources = sources();
    assert_eq!(
        source_rows(&sources, Some("refs/remotes/origin/main"), ""),
        [5, 7, 6, 1, 2, 0, 3, 4]
    );
    // A default that is not a main still leads; main and master follow.
    assert_eq!(
        source_rows(&sources, Some("refs/remotes/origin/feature"), ""),
        [4, 5, 7, 6, 1, 2, 0, 3]
    );
    // No default: the mains alone move.
    assert_eq!(source_rows(&sources, None, "  "), [5, 7, 6, 1, 2, 0, 3, 4]);
    // A default git names but no longer lists moves nothing.
    assert_eq!(
        source_rows(&sources[..1], Some("refs/remotes/origin/gone"), ""),
        [0]
    );
}

/// Typing ranks by scattered letters over the names shown, so `heads`
/// or `remotes` match nothing the person can see.
#[test]
fn typing_ranks_the_shown_names() {
    use guild_desktop::branch_list::source_rows;
    let sources = sources();
    assert_eq!(
        source_rows(&sources, Some("refs/remotes/origin/main"), "heads"),
        [] as [usize; 0]
    );
    let found = source_rows(&sources, Some("refs/remotes/origin/main"), "rel");
    assert_eq!(found, [3]);
    let found = source_rows(&sources, None, "origin/ma");
    assert!(found.starts_with(&[5]), "{found:?}");
    assert!(found.contains(&6), "{found:?}");
    assert!(!found.contains(&0), "{found:?}");
}
