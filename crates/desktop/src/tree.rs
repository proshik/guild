//! The file tree as a model: what is in a directory, what is expanded,
//! what is ignored, in what order. The window only reads it.
//!
//! **A directory is read when it is opened, and not before.** A
//! gitignored directory is left unloaded and read only when it is
//! expanded; that is the difference between a tree that opens in a
//! blink and one that walks `target/` — a hundred thousand files under a
//! Rust project — before it draws its first row. So `open` reads the root
//! and nothing else; `expand` reads what it opens; `refresh` re-reads what
//! the watcher says changed, and only if it was read before.
//!
//! **What `.gitignore` names is shown dimmed, not hidden.** A person
//! looking for `target/debug/guild-desktop` should find it, dimmed, rather
//! than wonder where it went. The rules come from the `ignore` crate — the
//! one `ripgrep` is built on — read from every `.gitignore` on the
//! way down from the root, the deeper file overriding the shallower, and
//! an ignored directory's children ignored with it. `.git` is hidden, and
//! so is the staging folder of a copy running in this window, while it
//! runs; nothing else is.
//!
//! **Directories first, then files, without regard to case** — the Finder
//! agrees, and a person expects a tree to look like the one they already
//! know.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// What an entry is. A symlink to a directory is a leaf here and is not
/// followed: a tree that follows links can be a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A directory, which can be expanded.
    Dir,
    /// A file.
    File,
    /// A symlink, shown but not followed.
    Symlink,
}

/// One entry of a directory. `path` is relative to the tree's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    name: String,
    path: PathBuf,
    kind: Kind,
    ignored: bool,
}

impl Entry {
    /// The file name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The path relative to the root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What it is.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Whether `.gitignore` names it, or a directory above it.
    pub fn is_ignored(&self) -> bool {
        self.ignored
    }

    /// Whether it can be expanded.
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }
}

/// One row of the flattened tree: an entry at a depth, expanded or not.
///
/// **A row can be a chain of directories** — M8.10. A directory whose only
/// child is a directory is drawn as one row, `com/example/inaction`, and a
/// Kotlin or Java source tree is mostly such chains: they were most of the
/// tree's width. The row then carries two paths, and which is which matters:
/// [`Row::path`] is the **deepest**, because that is what a deed aims at —
/// *New File…* in `com/example/inaction` makes it in `inaction` — and
/// [`Row::chain_root`] is the **topmost**, because folding the row must take
/// the whole chain with it.
#[derive(Debug, Clone)]
pub struct Row<'a> {
    entry: &'a Entry,
    /// The topmost entry of the chain; the same one when there is none.
    top: &'a Entry,
    /// What the row says: the names of the chain, joined.
    label: String,
    depth: usize,
    expanded: bool,
}

impl<'a> Row<'a> {
    /// The entry the row shows.
    pub fn entry(&self) -> &'a Entry {
        self.entry
    }

    /// The file name — the deepest of a chain, which is what its icon
    /// is chosen by.
    pub fn name(&self) -> &'a str {
        &self.entry.name
    }

    /// What the row says: a name, or a chain's names joined by `/`.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The path relative to the root: **the deepest of a chain**.
    pub fn path(&self) -> &'a Path {
        &self.entry.path
    }

    /// Where a chain starts, which is where folding it must begin. The
    /// row's own path when it is not a chain.
    pub fn chain_root(&self) -> &'a Path {
        &self.top.path
    }

    /// How deep: the root's entries are at 0.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Whether `.gitignore` names it, or a directory above it.
    pub fn is_ignored(&self) -> bool {
        self.entry.ignored
    }

    /// Whether it can be expanded.
    pub fn is_dir(&self) -> bool {
        self.entry.is_dir()
    }

    /// Whether it is expanded — meaningful for a directory only.
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }
}

/// What a directory read gave: its entries, or the reason it gave none.
#[derive(Debug, Clone, Default)]
struct Listing {
    entries: Vec<Entry>,
    error: Option<String>,
}

/// The tree: the directories read so far, the ones expanded, and the
/// ignore rules met on the way.
#[derive(Debug)]
pub struct Tree {
    root: PathBuf,
    scope: Option<Vec<PathBuf>>,
    /// Loaded directories by their relative path; the root is `""`.
    dirs: HashMap<PathBuf, Listing>,
    expanded: BTreeSet<PathBuf>,
    /// The `.gitignore` of every directory that has one, by that
    /// directory's relative path. Matching walks the chain from the entry's
    /// directory up to the root, the deeper file first.
    ignores: HashMap<PathBuf, Gitignore>,
    /// The root as a row of its own: the project's name, at the top,
    /// with everything one level in.
    root_entry: Entry,
}

impl Tree {
    /// Open a tree at `root`, reading the root directory and nothing else.
    pub fn open(root: &Path) -> Self {
        Self::open_with_scope(root, None)
    }

    /// A library sharing a physical tree with other modules. Symlinks are
    /// omitted: vendor navigation must stay within the validated source root.
    pub(crate) fn scoped(root: &Path, excluded: &[PathBuf]) -> Self {
        Self::open_with_scope(root, Some(excluded.to_vec()))
    }

    fn open_with_scope(root: &Path, scope: Option<Vec<PathBuf>>) -> Self {
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let mut tree = Self {
            root: root.to_path_buf(),
            scope,
            dirs: HashMap::new(),
            expanded: BTreeSet::new(),
            ignores: HashMap::new(),
            root_entry: Entry {
                name,
                path: PathBuf::new(),
                kind: Kind::Dir,
                ignored: false,
            },
        };
        tree.load(Path::new(""), false);
        // The root starts open: a tree that shows one closed row is a
        // tree that has to be opened before it says anything.
        tree.expanded.insert(PathBuf::new());
        tree
    }

    /// Open a tree with these directories expanded — what the state file
    /// remembered. A directory that no longer exists is skipped.
    pub fn with_expanded(root: &Path, dirs: &[PathBuf]) -> Self {
        let mut tree = Self::open(root);
        for dir in dirs {
            if root.join(dir).is_dir() {
                tree.expand(dir);
            }
        }
        tree
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The flattening: the root itself first, then its entries in order,
    /// and under every expanded directory its own, one level deeper.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut rows = Vec::new();
        let expanded = self.expanded.contains(Path::new(""));
        rows.push(Row {
            entry: &self.root_entry,
            top: &self.root_entry,
            label: self.root_entry.name.clone(),
            depth: 0,
            expanded,
        });
        if expanded {
            self.push_rows(Path::new(""), 1, &mut rows);
        }
        rows
    }

    fn push_rows<'a>(&'a self, dir: &Path, depth: usize, rows: &mut Vec<Row<'a>>) {
        let Some(listing) = self.dirs.get(dir) else {
            return;
        };
        for entry in &listing.entries {
            let expanded = entry.is_dir() && self.expanded.contains(&entry.path);
            // The chain is walked only through directories that are
            // themselves expanded, which is what `expand` leaves behind:
            // a folded `com` is a row saying `com`.
            let mut deepest = entry;
            let mut label = entry.name.clone();
            while expanded && let Some(only) = self.only_child(&deepest.path) {
                label.push('/');
                label.push_str(&only.name);
                deepest = only;
            }
            rows.push(Row {
                entry: deepest,
                top: entry,
                label,
                depth,
                expanded,
            });
            if expanded {
                self.push_rows(&deepest.path, depth + 1, rows);
            }
        }
    }

    /// The row a path is drawn in, by its place in [`Tree::rows`].
    ///
    /// **A chain's row stands for every directory on the chain** — M8.10
    /// made `cfg` and `cfg/guild` one row called by the deeper — so a
    /// path is found by the row whose chain runs through it and not only
    /// by the row that bears its name. Without that, a selection made
    /// before a chain formed named no row after it: nothing was lit, and
    /// the next arrow, finding no current row, went to the top. `None`
    /// for a path no row draws — folded away, or not in the tree.
    pub fn row_covering(&self, path: &Path) -> Option<usize> {
        self.rows()
            .iter()
            .position(|row| path.starts_with(row.chain_root()) && row.path().starts_with(path))
    }

    /// The one directory a directory holds, when it holds exactly that
    /// and the window has it expanded — the step of a chain. M8.10.
    fn only_child(&self, dir: &Path) -> Option<&Entry> {
        let listing = self.dirs.get(dir)?;
        let [only] = listing.entries.as_slice() else {
            return None;
        };
        (only.is_dir() && self.expanded.contains(&only.path)).then_some(only)
    }

    /// Whether a directory has been read.
    pub fn is_loaded(&self, dir: &Path) -> bool {
        self.dirs.contains_key(dir)
    }

    /// Whether a directory is expanded.
    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    /// Why a directory shows no entries, when it is because it could not
    /// be read.
    pub fn error(&self, dir: &Path) -> Option<&str> {
        self.dirs.get(dir).and_then(|l| l.error.as_deref())
    }

    /// Expand a directory, reading it if it has not been read.
    /// **A chain is expanded with it**, M8.10: a directory whose only
    /// entry is a directory is opened too, as far down as that holds, so
    /// that one click on `com` shows what is inside `com/example/inaction`
    /// rather than one directory at a time.
    pub fn expand(&mut self, dir: &Path) {
        if self.outside_scope(dir) {
            return;
        }
        let mut dir = dir.to_path_buf();
        loop {
            if !self.is_loaded(&dir) {
                let ignored = self.is_path_ignored(&dir, true);
                self.load(&dir, ignored);
            }
            self.expanded.insert(dir.clone());
            let next = match self.dirs.get(&dir).map(|l| l.entries.as_slice()) {
                Some([only]) if only.is_dir() => only.path.clone(),
                _ => return,
            };
            dir = next;
        }
    }

    /// Collapse a directory, **and everything expanded inside it**.
    /// What was read stays read.
    ///
    /// The alternative — shutting only this row — looks the same at the
    /// moment it is done and is a different tree the next time the row
    /// is opened: a directory reopens with the eight descendants a
    /// person opened an hour ago still spread out, which is exactly the
    /// state they were collapsing it to get rid of. Every file tree that
    /// is pleasant to use collapses recursively.
    ///
    /// The root is `""`, which is a prefix of every path, so collapsing
    /// the project shuts the whole tree.
    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.retain(|open| !open.starts_with(dir));
    }

    /// Toggle a directory between expanded and collapsed.
    pub fn toggle(&mut self, dir: &Path) {
        if self.is_expanded(dir) {
            self.collapse(dir);
        } else {
            self.expand(dir);
        }
    }

    /// Re-read a directory that was read before. A directory never read
    /// is left alone: there is nothing on screen to bring up to date.
    pub fn refresh(&mut self, dir: &Path) {
        if self.is_loaded(dir) {
            let ignored = self.is_path_ignored(dir, true);
            self.load(dir, ignored);
        }
    }

    /// Expand every directory on the way to a file and answer its row —
    /// or `None`, and nothing expanded, when the file is not there.
    pub fn reveal(&mut self, file: &Path) -> Option<usize> {
        if self.outside_scope(file) {
            return None;
        }
        if !self.root.join(file).exists() {
            return None;
        }
        self.expand(Path::new(""));
        let mut dir = PathBuf::new();
        for component in file.parent()?.components() {
            dir.push(component);
            self.expand(&dir.clone());
        }
        self.rows().iter().position(|row| row.path() == file)
    }

    /// The expanded directories, shallowest first — what the state file
    /// stores.
    pub fn expanded(&self) -> impl Iterator<Item = &Path> {
        self.expanded.iter().map(PathBuf::as_path)
    }

    /// Paths outside this library's module boundary are never browsed.
    fn outside_scope(&self, path: &Path) -> bool {
        self.scope.as_ref().is_some_and(|excluded| {
            path.components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
                || excluded.iter().any(|root| path.starts_with(root))
                || self
                    .root
                    .join(path)
                    .symlink_metadata()
                    .is_ok_and(|meta| meta.is_symlink())
        })
    }

    /// Read one directory into the tree.
    fn load(&mut self, dir: &Path, parent_ignored: bool) {
        let absolute = self.root.join(dir);
        let ignore_file = absolute.join(".gitignore");
        if ignore_file.is_file() {
            let mut builder = GitignoreBuilder::new(&absolute);
            builder.add(&ignore_file);
            if let Ok(rules) = builder.build() {
                self.ignores.insert(dir.to_path_buf(), rules);
            }
        }

        let listing = match std::fs::read_dir(&absolute) {
            Ok(read) => {
                let mut entries: Vec<Entry> = read
                    .filter_map(Result::ok)
                    .filter_map(|found| {
                        let name = found.file_name().to_string_lossy().into_owned();
                        if name == ".git" || crate::file_operations::is_staging(&name) {
                            return None;
                        }
                        let kind = match found.file_type() {
                            Ok(t) if t.is_symlink() => Kind::Symlink,
                            Ok(t) if t.is_dir() => Kind::Dir,
                            _ => Kind::File,
                        };
                        let path = dir.join(&name);
                        if self.outside_scope(&path) {
                            return None;
                        }
                        let ignored =
                            parent_ignored || self.is_path_ignored(&path, kind == Kind::Dir);
                        Some(Entry {
                            name,
                            path,
                            kind,
                            ignored,
                        })
                    })
                    .collect();
                entries.sort_by(|a, b| order(a).cmp(&order(b)));
                Listing {
                    entries,
                    error: None,
                }
            }
            Err(err) => Listing {
                entries: Vec::new(),
                error: Some(err.to_string()),
            },
        };
        self.dirs.insert(dir.to_path_buf(), listing);
    }

    /// Whether the rules read so far ignore a path: the deepest
    /// `.gitignore` above it speaks first, and a parent already ignored
    /// settles it before any rule is asked.
    fn is_path_ignored(&self, path: &Path, is_dir: bool) -> bool {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && self.is_entry_ignored(parent)
        {
            return true;
        }
        let absolute = self.root.join(path);
        let mut dir = path.parent().map(Path::to_path_buf);
        while let Some(current) = dir {
            if let Some(rules) = self.ignores.get(&current) {
                match rules.matched_path_or_any_parents(&absolute, is_dir) {
                    ignore::Match::Ignore(_) => return true,
                    ignore::Match::Whitelist(_) => return false,
                    ignore::Match::None => {}
                }
            }
            dir = if current.as_os_str().is_empty() {
                None
            } else {
                Some(current.parent().map(Path::to_path_buf).unwrap_or_default())
            };
        }
        false
    }

    /// Whether an entry already read carries the ignored flag.
    fn is_entry_ignored(&self, path: &Path) -> bool {
        let Some(parent) = path.parent() else {
            return false;
        };
        self.dirs
            .get(parent)
            .and_then(|listing| listing.entries.iter().find(|e| e.path == path))
            .is_some_and(|e| e.ignored)
    }
}

/// The path a typed name asks for, under the directory it was typed in.
///
/// **The one rule behind *New File…*, *New Directory…* and *Rename…***
/// — M8.8.3. All three take a line from a person and end in the same
/// question, and the answer is the only place the tree decides what may
/// be written.
///
/// The name is trimmed, because a trailing space makes a file nobody can
/// pick out of this list again; a `/` in it makes the directories on the
/// way, as New File usually does; and a name that would leave
/// the directory is not a name. `None` for every refusal: there is one
/// thing to do with all of them, which is nothing.
///
/// Pure, and here rather than beside the window, because everything that
/// can be wrong about it can be wrong without one.
pub fn under(dir: &Path, typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let asked = Path::new(typed);
    // An absolute path is not a name typed under a directory; on the way
    // in it would silently ignore the directory the menu was opened on.
    if asked.is_absolute() {
        return None;
    }
    let mut path = dir.to_path_buf();
    for part in asked.components() {
        match part {
            std::path::Component::Normal(name) => path.push(name),
            // `.`, `..`, a root, a prefix — every one of them either says
            // nothing or climbs out.
            _ => return None,
        }
    }
    (path != dir).then_some(path)
}

/// The sort key: directories first, then the name without regard to case,
/// then the name itself so that two names differing only by case have a
/// stable order.
fn order(entry: &Entry) -> (bool, String, &str) {
    (!entry.is_dir(), entry.name.to_lowercase(), &entry.name)
}

#[cfg(test)]
mod tests {
    use super::Tree;
    use crate::file_operations::LiveStage;
    use std::path::Path;

    /// A running copy's staging folder appears beside its destination and
    /// goes when the copy publishes; listed, it was a row that came and went
    /// and moved every row below it. Only the copier knows which are
    /// running, and only those are hidden — a name merely of that form, or
    /// a folder a failed copy left, shows.
    #[test]
    fn a_running_copys_staging_folder_is_hidden_while_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let running = ".guild-transfer-6f1c1a52-3a0b-4c55-9d8e-0a1b2c3d4e5f";
        let left = ".guild-transfer-0b5c2f3e-6a1d-4c8e-9f27-1d2e3f4a5b6c";
        for name in [running, left, "src"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        let names = |tree: &Tree| -> Vec<String> {
            tree.rows()
                .iter()
                .skip(1)
                .map(|r| r.name().to_owned())
                .collect()
        };
        let held = LiveStage::hold(running.into());
        let mut tree = Tree::open(dir.path());
        assert_eq!(names(&tree), [left, "src"]);
        drop(held);
        tree.refresh(Path::new(""));
        assert_eq!(names(&tree), [left, running, "src"]);
    }
}
