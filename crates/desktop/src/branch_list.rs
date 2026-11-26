//! What the branch popup shows for what has been typed. Pure.
//!
//! The field does two things at once, which is what its placeholder
//! says: it narrows the list, and when nothing is left it offers to
//! make what was typed. Both answers are here so that a test can read
//! them without a window, and so that the row a person sees and the
//! branch a click checks out cannot come from two different rules.

use crate::picker::Lit;

/// The names git prints for a default branch, best first.
///
/// `main` beats `master` where a repository has both, which is what one
/// mid-rename looks like: one of them has to be first, and the newer word
/// is the one people are moving to.
const DEFAULT_BRANCH: [&str; 2] = ["main", "master"];

/// The order the lists are drawn in: the default branch, then git's.
///
/// **Why not sort.** `git branch` already gives a useful order —
/// alphabetical — and a person reading a column of names relies on it.
/// The only name that earns a place out of order is the one everybody
/// looks for first, so exactly one row moves and the rest are untouched.
/// A repository with neither `main` nor `master` is left exactly as git
/// gave it: inventing a first row would be inventing a default branch it
/// does not have.
///
/// Indices rather than names, like [`matching`], so a caller keeps
/// whatever else it knows about each branch.
pub fn ordered(names: &[String]) -> Vec<usize> {
    let first = DEFAULT_BRANCH
        .iter()
        .find_map(|wanted| names.iter().position(|name| name == wanted));

    first
        .into_iter()
        .chain((0..names.len()).filter(|index| Some(*index) != first))
        .collect()
}

/// The branch a task's new worktree starts from — M13.1, decision 2.
///
/// The local branch `origin/HEAD` names — `origin_head` as git prints it
/// short, `origin/develop` — when it is one of `locals`; otherwise the
/// first [`DEFAULT_BRANCH`] there is; otherwise `None`, and git starts
/// from `HEAD` as it always has. **Local, never the remote-tracking
/// branch**: git makes a remote-tracking start point the new branch's
/// upstream, and a bare `git push` from a task's worktree would then go
/// to `main`.
pub fn main_branch(origin_head: Option<&str>, locals: &[String]) -> Option<String> {
    origin_head
        .and_then(|name| name.split_once('/'))
        .map(|(_, branch)| branch)
        .filter(|branch| locals.iter().any(|local| local == branch))
        .or_else(|| {
            DEFAULT_BRANCH
                .into_iter()
                .find(|wanted| locals.iter().any(|local| local == wanted))
        })
        .map(str::to_owned)
}

/// Which rows of `names` a query matches, in the list's own order.
///
/// A plain contains, case-insensitively: the names are short and a
/// person types the middle of one as often as the start. An empty query
/// matches everything, which is the list as git gave it.
///
/// **The Log's branch column's, since M8.9** — a docked list, which keeps
/// git's order under a filter. The popup's rows are [`rows`], found by
/// scattered letters.
pub fn matching(names: &[String], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    names
        .iter()
        .enumerate()
        .filter(|(_, name)| query.is_empty() || name.to_lowercase().contains(&query))
        .map(|(index, _)| index)
        .collect()
}

/// Whether the list should offer to create what was typed.
///
/// Only when something was typed, nothing in the list is called exactly
/// that, and the name is one git would take. **Not** "nothing matched":
/// typing `ma` in a repository with `main` matches, and the offer to
/// make a branch called `ma` is still the right one to show, and a person
/// who wanted `main` clicks `main`.
pub fn offers_create(names: &[String], query: &str) -> bool {
    let query = query.trim();
    !query.is_empty() && !names.iter().any(|name| name == query) && is_legal(query)
}

/// One row of the branch popup's Branches face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// *Create branch "…"* — what was typed, made here.
    Create,
    /// *Create worktree "…"* — the same name as a worktree of its own,
    /// beside the repository. M12's decision 7.
    Worktree,
    /// *Local*, over the branches there are — a title, never lit.
    Heading,
    /// A branch, by its index into the names.
    Branch(usize),
}

impl Row {
    /// What the row is as far as lighting it goes: the heading is never
    /// lit, and everything else can be taken.
    pub fn lit(&self) -> Lit {
        match self {
            Row::Heading => Lit::Heading,
            Row::Create | Row::Worktree | Row::Branch(_) => Lit::Row,
        }
    }
}

/// The rows the Branches face shows for what has been typed — M8.9.
///
/// **Two create rows, or none** (M12): a name that is not a branch yet can
/// be made as a branch here or as a worktree of its own. Then **the
/// *Local* heading, only above branches there are** — a heading with
/// nothing under it is what a person typing a brand-new name used to get —
/// then the branches, found by their scattered letters and best first, or
/// in the list's own order while nothing is typed.
///
/// One list rather than a count of create rows plus a list of matches plus
/// an offset for the heading: those were four pieces of arithmetic in
/// three files, and the row a person clicked and the thing that happened
/// could come apart at any of them.
pub fn rows(names: &[String], query: &str) -> Vec<Row> {
    let query = query.trim();
    let mut rows = Vec::new();
    if offers_create(names, query) {
        rows.extend([Row::Create, Row::Worktree]);
    }
    let found = crate::fuzzy::rank(query, names, String::as_str, crate::fuzzy::Shape::Path);
    if !found.is_empty() {
        rows.push(Row::Heading);
        rows.extend(found.into_iter().map(|(index, _)| Row::Branch(index)));
    }
    rows
}

/// An Update branch source as a person reads it: `origin/main` for a
/// remote-tracking branch, `main` for a local one. Git's `refs/heads/`
/// and `refs/remotes/` say which kind it is, and the popup says that
/// with the row's trailing words rather than with a prefix on every name.
pub fn source_name(reference: &str) -> &str {
    reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/remotes/"))
        .or_else(|| reference.strip_prefix("refs/"))
        .unwrap_or(reference)
}

/// The Update branch popup's rows for what has been typed, as indices
/// into `references`.
///
/// **With nothing typed, the sources a branch is usually updated from
/// come first**: the repository's default source — what `origin/HEAD`
/// names — then every other `main` or `master`, remote-tracking before
/// local and `main` before `master`, then the rest in git's order. A
/// query ranks the names the rows show by their scattered letters, so
/// `heads` finds nothing a person can see.
pub fn source_rows(references: &[String], default: Option<&str>, query: &str) -> Vec<usize> {
    let query = query.trim();
    if !query.is_empty() {
        return crate::fuzzy::rank(
            query,
            references,
            |reference| source_name(reference),
            crate::fuzzy::Shape::Name,
        )
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    }
    let rank = |index: usize| {
        let reference = references[index].as_str();
        if Some(reference) == default {
            return (0, 0);
        }
        let (remote, branch) = match reference.strip_prefix("refs/remotes/") {
            Some(name) => (true, name.split_once('/').map_or(name, |(_, b)| b)),
            None => (false, source_name(reference)),
        };
        match DEFAULT_BRANCH.iter().position(|name| *name == branch) {
            Some(main) => (1 + usize::from(!remote), main),
            None => (3, 0),
        }
    };
    let mut order: Vec<usize> = (0..references.len()).collect();
    // Stable: git's order is kept inside each group.
    order.sort_by_key(|index| rank(*index));
    order
}

/// Whether git would take this as a branch name.
///
/// Not git's whole rule — `git check-ref-format` is the whole rule, and
/// it is a process. This is the part a person hits by accident, and
/// anything that slips through is refused by git itself, in its words.
fn is_legal(name: &str) -> bool {
    !name.starts_with('-')
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.ends_with('/')
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains(char::is_whitespace)
        && !name.contains(['~', '^', ':', '?', '*', '[', '\\'])
}
