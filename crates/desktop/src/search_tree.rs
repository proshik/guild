//! A search's hits as a tree of files. Pure — hits in, rows out.
//!
//! M8.7. `files_tree` does this for a commit's files and this does it for
//! a search's, with two differences that are the whole reason it is a
//! second module rather than a parameter: a search's tree **folds**, and
//! it has a third kind of row under a file.
//!
//! The collapsing rule is the same one, and it is the same rule for the
//! same reason: `crates/desktop/tests/all` is one row and not four,
//! because the rows a person walks past to reach a file are the ones
//! worth removing.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use crate::search::Hit;

/// One line of the results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A directory, or a run of them collapsed into one.
    Directory {
        /// What is written on it: the segments **this** row adds, not
        /// the whole path — `assets/component` under a row that already
        /// said `crates/desktop`. A tree that repeats its parents on
        /// every line is a tree nobody reads down the left edge, which
        /// is the only thing the indentation is for.
        label: String,
        /// The whole path, which is what folds it and what the fold set
        /// holds.
        path: PathBuf,
        /// How many hits are beneath it, however deep.
        hits: usize,
        depth: usize,
    },
    /// A file with hits in it.
    File {
        /// Its own name, the directories being on the row above.
        name: String,
        /// Its path in the project, which opens it and folds it.
        path: PathBuf,
        /// How many hits it has, whether or not they are drawn.
        hits: usize,
        depth: usize,
    },
    /// One hit, under the file it is in.
    Hit {
        /// Which hit, by its place in the list the search gave.
        ///
        /// **An index rather than the hit itself**: the rows are rebuilt
        /// whenever a batch lands, and a row that borrowed a hit would
        /// tie every row's lifetime to the list it was built from.
        at: usize,
        depth: usize,
    },
}

/// What folding this row would fold, if it can be folded at all.
///
/// A hit does not fold: it is the thing the folding is about.
pub fn foldable(row: &Row) -> Option<PathBuf> {
    match row {
        Row::Directory { path, .. } | Row::File { path, .. } => Some(path.clone()),
        Row::Hit { .. } => None,
    }
}

/// The first row that is a hit, or `None` when every hit is folded away.
///
/// M5.11, decision 2. Row 0 is always a directory or a file, and Enter on
/// either folds; a person who types a word and presses Enter means the
/// first place it was found, which is what Find in Files usually lights.
pub fn first_hit(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|row| matches!(row, Row::Hit { .. }))
}

/// The hit the highlight was on when a refresh was asked for, kept until
/// the new answer brings it back. M5.11, decision 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// Its file, relative to the project.
    pub path: PathBuf,
    /// The line it was on, one-based — a hint, not a key.
    pub line: u32,
    /// The text of its row, which is the key together with the path.
    pub text: String,
}

/// The row of the hit in `kept.path` whose text is `kept.text`, nearest
/// `kept.line`.
///
/// **By its path and its text, not its line.** What made the answer stale
/// is usually a write that moved lines, and a path and a line would lose
/// the hit to the very prepend that asked for the refresh.
pub fn row_of_kept(rows: &[Row], hits: &[Hit], kept: &Kept) -> Option<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| match row {
            Row::Hit { at, .. } => hits.get(*at).map(|hit| (index, hit)),
            _ => None,
        })
        .filter(|(_, hit)| hit.path == kept.path && hit.text == kept.text)
        .min_by_key(|(_, hit)| hit.line.abs_diff(kept.line))
        .map(|(index, _)| index)
}

/// What the highlight is on, by what the row is rather than where it is.
///
/// **A row's number is not the row while a walk lands.** Each batch is one
/// file, `ignore` walks in the directory's own order and the tree is
/// sorted, so a file that lands later can put rows in front of the lit one
/// or split a collapsed chain — `crates/a` becoming `crates` and `a` — and
/// a highlight kept by its number is then on a file, where Enter folds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    /// A hit, found again as a kept hit is: by its path and its text,
    /// nearest its line.
    Hit(Kept),
    /// A directory or a file, by its path — no directory and file share one.
    Path(PathBuf),
}

/// What the row at `index` is, as something a rebuilt list can find again.
pub fn lit_of(rows: &[Row], hits: &[Hit], index: usize) -> Option<Lit> {
    match rows.get(index)? {
        Row::Hit { at, .. } => hits.get(*at).map(|hit| {
            Lit::Hit(Kept {
                path: hit.path.clone(),
                line: hit.line,
                text: hit.text.clone(),
            })
        }),
        row => foldable(row).map(Lit::Path),
    }
}

/// The row the highlight belongs on: the row `held` names, or — when
/// nothing holds it — the first hit, which is what a walk lights until the
/// person moves (decision 2). `None` when the held row is not drawn, or
/// there is no hit to light; the caller settles then.
pub fn lit_row(rows: &[Row], hits: &[Hit], held: Option<&Lit>) -> Option<usize> {
    match held {
        None => first_hit(rows),
        Some(Lit::Hit(kept)) => row_of_kept(rows, hits, kept),
        Some(Lit::Path(path)) => rows
            .iter()
            .position(|row| foldable(row).as_ref() == Some(path)),
    }
}

#[cfg(test)]
thread_local! { pub(crate) static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

/// The rows, in the order they are drawn.
///
/// `folded` holds the paths of directories and files that are shut — a
/// directory takes everything under it, and a file keeps its own row and
/// loses its hits, so there is always something left to press.
pub fn of(hits: &[Hit], folded: &HashSet<PathBuf>) -> Vec<Row> {
    #[cfg(test)]
    BUILDS.with(|count| count.set(count.get() + 1));
    let mut root = Node::default();
    for (at, hit) in hits.iter().enumerate() {
        root.add(&hit.path, at);
    }
    let mut rows = Vec::new();
    root.walk(PathBuf::new(), String::new(), 0, folded, &mut rows);
    rows
}

/// One directory of the tree while it is being built.
#[derive(Default)]
struct Node {
    directories: BTreeMap<String, Node>,
    /// The hits of each file, by name, in the order the walk found them.
    files: BTreeMap<String, Vec<usize>>,
}

impl Node {
    fn add(&mut self, path: &std::path::Path, at: usize) {
        let mut parts: Vec<String> = path
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let Some(name) = parts.pop() else {
            return;
        };
        let mut node = self;
        for part in parts {
            node = node.directories.entry(part).or_default();
        }
        node.files.entry(name).or_default().push(at);
    }

    /// Every hit beneath, however deep.
    fn count(&self) -> usize {
        self.files.values().map(Vec::len).sum::<usize>()
            + self.directories.values().map(Node::count).sum::<usize>()
    }

    /// Emit the rows, collapsing a run of single-child directories into
    /// the row that begins it.
    fn walk(
        &self,
        prefix: PathBuf,
        label: String,
        depth: usize,
        folded: &HashSet<PathBuf>,
        rows: &mut Vec<Row>,
    ) {
        let named = !prefix.as_os_str().is_empty();
        if named {
            rows.push(Row::Directory {
                label,
                path: prefix.clone(),
                hits: self.count(),
                depth: depth.saturating_sub(1),
            });
            if folded.contains(&prefix) {
                return;
            }
        }

        // Directories first, then files: a tree read by the eye needs the
        // same order every time, and the directories are the summary.
        for (name, node) in &self.directories {
            let (path, label, node) = node.collapse(prefix.join(name), name.clone());
            node.walk(path, label, depth + 1, folded, rows);
        }
        for (name, at) in &self.files {
            let path = prefix.join(name);
            rows.push(Row::File {
                name: name.clone(),
                path: path.clone(),
                hits: at.len(),
                depth,
            });
            if folded.contains(&path) {
                continue;
            }
            rows.extend(at.iter().map(|at| Row::Hit {
                at: *at,
                depth: depth + 1,
            }));
        }
    }

    /// Follow a run of directories that each hold exactly one directory
    /// and no file, joining their names into one path.
    fn collapse(&self, path: PathBuf, label: String) -> (PathBuf, String, &Self) {
        let mut path = path;
        let mut label = label;
        let mut node = self;
        while node.files.is_empty() && node.directories.len() == 1 {
            let (only, child) = node.directories.iter().next().expect("one");
            path = path.join(only);
            label = format!("{label}/{only}");
            node = child;
        }
        (path, label, node)
    }
}
