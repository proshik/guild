//! What a push would send, and where it would go. Pure: these tests do
//! not talk to a network or to a repository, they read the arguments
//! that would be asked of git and the destination worked out from what
//! git already printed.
//!
//! **Nothing here is typed by a person.** The remote is one of the names
//! `git remote` printed, the branch is the one `git symbolic-ref`
//! printed, and the branch it lands on is the upstream's own — decision
//! 21's rule that a reference is git's word rather than ours, met by the
//! one write in this crate that reaches another machine.

use guild_desktop::git::{Ahead, Destination};

fn ahead(branch: &str, remotes: &[&str], upstream: Option<&str>) -> Ahead {
    Ahead {
        branch: Some(branch.to_owned()),
        remotes: remotes.iter().map(|name| (*name).to_owned()).collect(),
        upstream: upstream.map(str::to_owned),
        commits: Vec::new(),
    }
}

/// With an upstream the range is the upstream's: exactly what `push`
/// would send, asked of `git log` in the Log's own format so that a row
/// here and a row there are the same row.
#[test]
fn the_commits_of_a_tracked_branch_are_the_range_above_its_upstream() {
    let args = guild_desktop::git::ahead_args(Some("origin/main"), "origin", 200);
    assert!(args.contains(&"origin/main..HEAD".to_owned()), "{args:?}");
    assert_eq!(args.first().map(String::as_str), Some("log"));
    assert_eq!(args.last().map(String::as_str), Some("--"));
}

/// **With no upstream there is no range**, and inventing one would be a
/// guess. What is asked instead is everything on `HEAD` that no ref of
/// the chosen remote already holds — which is the question `push` itself
/// answers before it sends anything.
#[test]
fn an_untracked_branch_asks_for_what_the_remote_does_not_already_have() {
    let args = guild_desktop::git::ahead_args(None, "origin", 200);
    assert!(args.contains(&"HEAD".to_owned()), "{args:?}");
    assert!(args.contains(&"--not".to_owned()), "{args:?}");
    assert!(args.contains(&"--remotes=origin".to_owned()), "{args:?}");
    assert!(
        !args.iter().any(|word| word.contains("..")),
        "no range is invented: {args:?}"
    );
}

/// An upstream is `<remote>/<branch>`, and the remote is the part before
/// the **first** slash: a remote name cannot hold one and a branch name
/// can, so splitting from the other end would call `origin` a branch of
/// a remote named `feature`.
#[test]
fn an_upstream_splits_at_the_first_slash_because_a_branch_may_hold_more() {
    let to = ahead("feature/lists", &["origin"], Some("origin/feature/lists"))
        .destination(None)
        .expect("a destination");
    assert_eq!(to.remote, "origin");
    assert_eq!(to.onto, "feature/lists");
    assert!(!to.set_upstream);
}

/// No upstream: the push goes to the only remote there is, under the
/// branch's own name, and it sets the upstream — which is the one bool
/// `Write::Push` takes.
#[test]
fn with_no_upstream_the_push_sets_one_on_the_only_remote() {
    let to = ahead("main", &["origin"], None)
        .destination(None)
        .expect("a destination");
    assert_eq!(to.remote, "origin");
    assert_eq!(to.branch, "main");
    assert_eq!(to.onto, "main");
    assert!(to.set_upstream);
}

/// **An upstream under another name is the case a one-argument push gets
/// wrong.** `git push origin bar` means `bar:bar` whatever `bar` tracks,
/// so a branch whose upstream is `origin/foo` would quietly grow a
/// second remote branch and leave the one it tracks behind. The
/// destination carries both names and the write spells the pair.
#[test]
fn a_branch_tracking_another_name_keeps_that_name() {
    let to = ahead("bar", &["origin"], Some("origin/foo"))
        .destination(None)
        .expect("a destination");
    assert_eq!(to.branch, "bar");
    assert_eq!(to.onto, "foo");
    assert!(!to.set_upstream);
}

/// A second remote chosen in the head is where the push goes — and
/// because the upstream is on the other one, this push sets a new
/// upstream and lands under the branch's own name.
#[test]
fn choosing_another_remote_moves_the_push_and_the_upstream_with_it() {
    let ahead = ahead("main", &["origin", "fork"], Some("origin/main"));
    let to = ahead.destination(Some("fork")).expect("a destination");
    assert_eq!(to.remote, "fork");
    assert_eq!(to.onto, "main");
    assert!(
        to.set_upstream,
        "the branch tracks origin, and this push is to fork"
    );
    // A name no `git remote` printed is not a remote: the head offers
    // only what git listed, and a stale choice falls back to the
    // upstream's rather than being sent as typed.
    let stale = ahead.destination(Some("gone")).expect("a destination");
    assert_eq!(stale.remote, "origin");
}

/// A repository with no remote, and a detached `HEAD`: neither has a
/// destination, and the tab says so rather than offering a button that
/// cannot work.
#[test]
fn no_remote_and_no_branch_are_two_ways_of_having_nowhere_to_push() {
    assert_eq!(ahead("main", &[], None).destination(None), None);
    assert_eq!(
        Ahead {
            branch: None,
            remotes: vec!["origin".to_owned()],
            upstream: None,
            commits: Vec::new(),
        }
        .destination(None),
        None
    );
}

/// The destination is what the head reads out: `<branch> → <remote> :
/// <onto>`. Kept as three fields rather than one string so the head can
/// draw the remote as a button and the rest as text, where
/// the local branch is the one part that cannot be changed.
#[test]
fn the_destination_is_three_names_and_not_a_sentence() {
    let to = Destination {
        remote: "origin".to_owned(),
        branch: "main".to_owned(),
        onto: "main".to_owned(),
        set_upstream: false,
    };
    assert_eq!(to.remote, "origin");
    assert_eq!(to.branch, "main");
    assert_eq!(to.onto, "main");
}
