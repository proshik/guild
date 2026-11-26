//! A project's files by name — Go to File's index. M8.9, decision 9.
//!
//! **What is indexed is what a search walks, plus the ignored files beside
//! it.** The walk is [`crate::search::walker`] — the one place the rules
//! are written — so a file Go to File cannot find is a file Find in Files
//! cannot search. A second pass with the ignore rules off, pruned to the
//! directories the first pass entered, adds every file the first did not
//! yield: `.env`, a local `config.toml`, a `.DS_Store` the global ignore
//! names. **The inside of an ignored directory is not indexed** — `target/`
//! is most of a Rust checkout, and the watcher drops every event under an
//! ignored directory before the window hears it, so an index of it would
//! go stale without a word. And the second pass refuses
//! [`crate::search::is_guarded`] names too, because in a linked worktree
//! `.git` is a *file*, and the first pass not yielding it is exactly what
//! the second pass would read as *ignored* (task 0).
//!
//! Nothing here knows about the window: [`walk`] reads a tree, [`apply`]
//! keeps an [`Index`] in step with a watcher's batch, and [`rank`] answers
//! a query. `root::file_index` is the thread around them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fuzzy::{self, Shape};

/// How many rows a ranking hands back. Task 0 measured the marks for the
/// best 200 of the registry's ninety thousand paths at under 5 ms in
/// release; a popup draws a screenful of them.
pub const LIMIT: usize = 200;

/// Which root a file is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Under {
    /// The project's own directory.
    Project,
    /// A dependency's source, by its index into `Libraries::of(project)` —
    /// opened through `go_to_library`, never `go_to_place` (decision 17).
    Library(usize),
}

/// A file as a ranked list hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Which root `path` is relative to.
    pub under: Under,
    /// The file, relative to its root.
    pub path: PathBuf,
    /// Whether an ignore file names it — drawn dimmed, as the tree dims it,
    /// and never ranked above a file that is not.
    pub ignored: bool,
    /// The matched letters, as `char` indices into `path` — never bytes:
    /// `fuzzy::spans` turns them into byte ranges on character boundaries.
    pub marks: Vec<u32>,
    /// Whether it is among the project's recent files.
    pub recent: bool,
}

/// A file a walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The file, relative to the root it was walked from.
    pub path: PathBuf,
    /// Whether the ignore rules name it.
    pub ignored: bool,
    /// Which root that is.
    pub under: Under,
}

/// Every file of a project and of the dependencies walked so far.
///
/// **Ordered by path**, because a batch that says a directory went has to
/// find everything under it, and in a `BTreeMap` of `PathBuf` — ordered
/// by components — everything under `a/b` sits right after `a/b`.
#[derive(Debug, Clone, Default)]
pub struct Index {
    trees: HashMap<Under, BTreeMap<PathBuf, bool>>,
}

impl Index {
    /// Put a walk's answer in place of whatever this root held.
    pub fn replace(&mut self, under: Under, entries: Vec<Entry>) {
        let files = entries
            .into_iter()
            .filter(|entry| entry.under == under)
            .map(|entry| (entry.path, entry.ignored))
            .collect();
        self.trees.insert(under, files);
    }

    /// Whether a root has been walked.
    pub fn has(&self, under: Under) -> bool {
        self.trees.contains_key(&under)
    }

    /// Every file held, root by root.
    pub fn entries(&self) -> impl Iterator<Item = Entry> + '_ {
        self.trees.iter().flat_map(|(under, files)| {
            files.iter().map(|(path, ignored)| Entry {
                path: path.clone(),
                ignored: *ignored,
                under: *under,
            })
        })
    }

    /// A root's files and whether each is ignored, borrowed — a caller
    /// walking the index on every batch clones nothing.
    pub fn files(&self, under: Under) -> impl Iterator<Item = (&Path, bool)> + '_ {
        self.trees.get(&under).into_iter().flat_map(|files| {
            files
                .iter()
                .map(|(path, ignored)| (path.as_path(), *ignored))
        })
    }

    /// Whether a root holds `path`, and if it does whether it is ignored.
    pub fn ignored(&self, under: Under, path: &Path) -> Option<bool> {
        self.trees.get(&under)?.get(path).copied()
    }

    /// How many files are held.
    pub fn len(&self) -> usize {
        self.trees.values().map(BTreeMap::len).sum()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Every file under `root`, in the two passes decision 9 describes.
///
/// `stop` is read before every entry; a stopped walk answers what it had,
/// which a caller that stopped it throws away.
pub fn walk(root: &Path, under: Under, stop: &AtomicBool) -> Vec<Entry> {
    walk_from(root, root, under, None, stop)
}

/// The two passes from `from`, the paths made relative to `root`, no
/// deeper than `depth` below `from`.
fn walk_from(
    root: &Path,
    from: &Path,
    under: Under,
    depth: Option<usize>,
    stop: &AtomicBool,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut entered: HashSet<PathBuf> = HashSet::new();
    let mut yielded: HashSet<PathBuf> = HashSet::new();

    // The first pass: the search's own rules.
    let mut first = crate::search::walker(from);
    first.max_depth(depth);
    for entry in first.build().flatten() {
        if stop.load(Ordering::Relaxed) {
            return entries;
        }
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            entered.insert(entry.path().to_path_buf());
        } else if kind.is_file() {
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            yielded.insert(entry.path().to_path_buf());
            entries.push(Entry {
                path: relative.to_path_buf(),
                ignored: false,
                under,
            });
        }
    }

    // The second pass: no rules, but only into what the first entered —
    // so an ignored *file* is found and an ignored *directory* is not
    // opened.
    let entered = Arc::new(entered);
    let mut second = ignore::WalkBuilder::new(from);
    second
        .standard_filters(false)
        .hidden(false)
        .max_depth(depth)
        .filter_entry({
            let entered = entered.clone();
            move |entry| {
                if entry.depth() == 0 {
                    return true;
                }
                if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    entered.contains(entry.path())
                } else {
                    // A linked worktree's `.git` is a file.
                    !crate::search::is_guarded(entry.file_name())
                }
            }
        });
    for entry in second.build().flatten() {
        if stop.load(Ordering::Relaxed) {
            return entries;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if yielded.contains(entry.path()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        entries.push(Entry {
            path: relative.to_path_buf(),
            ignored: true,
            under,
        });
    }
    entries
}

/// How many touched files a batch may name before the index walks again
/// instead of applying it — a checkout of another branch, a generator
/// run. Past this, a stat per file is slower than the walk.
pub const APPLY_AT_MOST: usize = 1_000;

/// Whether a batch that touched `files` is walked again rather than
/// applied: **more than [`APPLY_AT_MOST`] of them, or an ignore file
/// among them**, since a changed `.gitignore` can change what is ignored
/// anywhere below it and [`apply`] lists only the directories touched.
pub fn walks_again(files: &[PathBuf]) -> bool {
    files.len() > APPLY_AT_MOST
        || files.iter().any(|file| {
            file.file_name()
                .is_some_and(|name| name == ".gitignore" || name == ".ignore")
        })
}

/// Keep an index in step with what a watcher's batch touched.
///
/// `files` and `directories` are the batch's, relative to `root`. **Every
/// touched path is asked of the disk**, because a batch says *something
/// happened here* and not what: a file that is gone is dropped with
/// everything under it — a rename reaches a batch as a path no longer
/// there, taken for a file — and a directory that is still there has its
/// own files listed again, one level, by the same two passes. A directory
/// the index has never held a file under is read from its nearest
/// ancestor that it has, which is what brings in a directory that appeared
/// with files already in it.
pub fn apply(
    index: &mut Index,
    root: &Path,
    under: Under,
    files: &[PathBuf],
    directories: &[PathBuf],
) {
    let tree = index.trees.entry(under).or_default();
    let mut reread: BTreeSet<PathBuf> = BTreeSet::new();

    for directory in directories {
        if root.join(directory).is_dir() {
            reread.insert(directory.clone());
        } else {
            drop_under(tree, directory);
            if let Some(parent) = directory.parent() {
                reread.insert(parent.to_path_buf());
            }
        }
    }
    for file in files {
        let full = root.join(file);
        if full.is_file() {
            if let Some(parent) = file.parent() {
                reread.insert(parent.to_path_buf());
            }
        } else if full.is_dir() {
            reread.insert(file.clone());
        } else {
            tree.remove(file);
            drop_under(tree, file);
            if let Some(parent) = file.parent() {
                reread.insert(parent.to_path_buf());
            }
        }
    }

    // Each directory is read from the nearest one the index already holds
    // files under — itself, if it does.
    let reread: BTreeSet<PathBuf> = reread
        .into_iter()
        .map(|directory| held_ancestor(tree, &directory))
        .collect();
    let stop = AtomicBool::new(false);
    for directory in reread {
        reconcile(tree, root, under, &directory, &stop);
    }
}

/// `directory` itself when the index holds a file under it, or the
/// nearest ancestor that does — the root at the last.
fn held_ancestor(tree: &BTreeMap<PathBuf, bool>, directory: &Path) -> PathBuf {
    let mut at = directory;
    loop {
        if at.as_os_str().is_empty() || holds_under(tree, at) {
            return at.to_path_buf();
        }
        match at.parent() {
            Some(parent) => at = parent,
            None => return PathBuf::new(),
        }
    }
}

/// Whether any file held is under `directory`.
fn holds_under(tree: &BTreeMap<PathBuf, bool>, directory: &Path) -> bool {
    tree.range(directory.to_path_buf()..)
        .next()
        .is_some_and(|(path, _)| path.starts_with(directory) && path != directory)
}

/// Drop everything under `directory`; the root's empty path is all of it.
fn drop_under(tree: &mut BTreeMap<PathBuf, bool>, directory: &Path) {
    if directory.as_os_str().is_empty() {
        tree.clear();
        return;
    }
    let gone: Vec<PathBuf> = tree
        .range(directory.to_path_buf()..)
        .take_while(|(path, _)| path.starts_with(directory))
        .map(|(path, _)| path.clone())
        .collect();
    for path in gone {
        tree.remove(&path);
    }
}

/// List `directory` again, one level: its own files replaced, a child
/// directory the index held that is gone or ignored now dropped, and a
/// child directory it held nothing under walked whole.
fn reconcile(
    tree: &mut BTreeMap<PathBuf, bool>,
    root: &Path,
    under: Under,
    directory: &Path,
    stop: &AtomicBool,
) {
    let full = root.join(directory);
    if !full.is_dir() {
        drop_under(tree, directory);
        return;
    }
    let listed = walk_from(root, &full, under, Some(1), stop);

    // What the index held directly in it, and the child directories it
    // held anything under.
    let mut held_files: Vec<PathBuf> = Vec::new();
    let mut held_children: BTreeSet<PathBuf> = BTreeSet::new();
    let below: Vec<&PathBuf> = if directory.as_os_str().is_empty() {
        tree.keys().collect()
    } else {
        tree.range(directory.to_path_buf()..)
            .take_while(|(path, _)| path.starts_with(directory))
            .map(|(path, _)| path)
            .collect()
    };
    for path in below {
        let Ok(rest) = path.strip_prefix(directory) else {
            continue;
        };
        let mut parts = rest.components();
        let Some(first) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            held_children.insert(directory.join(first));
        } else {
            held_files.push(path.clone());
        }
    }
    for path in held_files {
        tree.remove(&path);
    }
    for entry in &listed {
        tree.insert(entry.path.clone(), entry.ignored);
    }

    // The child directories the first pass enters now.
    let entered: BTreeSet<PathBuf> = crate::search::walker(&full)
        .max_depth(Some(1))
        .build()
        .flatten()
        .filter(|entry| entry.depth() == 1 && entry.file_type().is_some_and(|k| k.is_dir()))
        .filter_map(|entry| entry.path().strip_prefix(root).ok().map(Path::to_path_buf))
        .collect();
    for child in held_children.difference(&entered) {
        drop_under(tree, child);
    }
    for child in entered.difference(&held_children) {
        for entry in walk_from(root, &root.join(child), under, None, stop) {
            tree.insert(entry.path, entry.ignored);
        }
    }
}

/// The files `query` finds, best first, at most `limit` of them.
///
/// **Sorted by (ignored, filename relevance, score, recent, length, path)**: an ignored file
/// never outranks one that is not, at any score; being recent breaks a tie
/// and nothing more — a recent file that matches worse is still a worse
/// match (decision 10); and a shorter path before a longer one.
///
/// **An empty query answers the recent files**, in `recent`'s order —
/// `cmd-e`'s — and only those the index still holds. `recent` is the
/// project's list, relative to the project. A dependency's files are
/// ranked only when `libraries` is asked.
pub fn rank(
    query: &str,
    index: &Index,
    recent: &[PathBuf],
    libraries: bool,
    limit: usize,
) -> Vec<Row> {
    let project = index.trees.get(&Under::Project);
    if query.trim().is_empty() {
        let Some(project) = project else {
            return Vec::new();
        };
        return recent
            .iter()
            .filter_map(|path| {
                project.get(path).map(|ignored| Row {
                    under: Under::Project,
                    path: path.clone(),
                    ignored: *ignored,
                    marks: Vec::new(),
                    recent: true,
                })
            })
            .take(limit)
            .collect();
    }

    let recent: HashSet<&Path> = recent.iter().map(PathBuf::as_path).collect();
    let mut candidates: Vec<(Under, &Path, bool)> = Vec::new();
    for (under, files) in &index.trees {
        if *under != Under::Project && !libraries {
            continue;
        }
        candidates.extend(
            files
                .iter()
                .map(|(path, ignored)| (*under, path.as_path(), *ignored)),
        );
    }
    // A path that is not UTF-8 has no text to match, and is found by
    // nothing typed.
    let found = fuzzy::rank(
        query,
        &candidates,
        |(_, path, _)| path.to_str().unwrap_or(""),
        Shape::Path,
    );
    let relevance_query = query.trim().to_lowercase();
    let mut rows: Vec<(u8, u32, Row)> = found
        .into_iter()
        .map(|(at, found)| {
            let (under, path, ignored) = candidates[at];
            let is_recent = under == Under::Project && recent.contains(path);
            (
                filename_relevance(&relevance_query, path),
                found.score,
                Row {
                    under,
                    path: path.to_path_buf(),
                    ignored,
                    marks: found.marks,
                    recent: is_recent,
                },
            )
        })
        .collect();
    rows.sort_by(|(a_relevance, a_score, a), (b_relevance, b_score, b)| {
        a.ignored
            .cmp(&b.ignored)
            .then(a_relevance.cmp(b_relevance))
            .then(b_score.cmp(a_score))
            .then(b.recent.cmp(&a.recent))
            .then(a.path.as_os_str().len().cmp(&b.path.as_os_str().len()))
            .then(a.path.cmp(&b.path))
            .then(a.under.cmp(&b.under))
    });
    rows.into_iter()
        .take(limit)
        .map(|(_, _, row)| row)
        .collect()
}

/// Relevance is a tier above the fuzzy score, not a replacement for matching.
/// Path queries begin at a component boundary; filename queries prefer the
/// basename and stem even when their directory is much longer.
fn filename_relevance(query: &str, path: &Path) -> u8 {
    if query.contains('/') {
        let path = path.to_string_lossy().to_lowercase();
        if path == query || path.ends_with(&format!("/{query}")) {
            return 0;
        }
        if path.starts_with(query) || path.contains(&format!("/{query}")) {
            return 1;
        }
        return 3;
    }
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    if basename == query {
        0
    } else if path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.to_lowercase() == query)
    {
        1
    } else if basename.starts_with(query) {
        2
    } else {
        3
    }
}
