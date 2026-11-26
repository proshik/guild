//! A commit's files as a tree. Pure — paths in, rows out.
//!
//! `git show --name-status` gives a flat list of paths, and a flat list
//! is what the Log's right column showed until M8.3.4. Thirty files of
//! one commit read as thirty unrelated lines; the same thirty as a tree
//! read as *four directories, and here is which*.
//!
//! **A chain of directories with one child each is one row** —
//! `crates/desktop/tests/all` rather than four lines of nothing but a name.
//! That is what every file tree worth reading does: the rows a person walks
//! past to reach a file are the ones worth removing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::git::log::FileChange;
use crate::theme::Mark;

/// One line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A directory, or a run of them collapsed into one.
    Directory {
        /// What is written on it — `desktop/tests/all` for a collapsed run.
        path: String,
        /// Every file beneath it, however deep. Worth reading before
        /// opening anything.
        files: usize,
        depth: usize,
        /// Its full path from the tree's root — what folding it is kept by,
        /// which the label of a collapsed run is not. M8.3.6.
        key: String,
    },
    /// A file, under whichever directory row precedes it.
    File {
        /// Its own name, the directories being on the row above.
        name: String,
        /// Its path in the commit, for opening its diff.
        path: PathBuf,
        /// Where it came from, when the commit renamed it.
        from: Option<PathBuf>,
        mark: Mark,
        depth: usize,
    },
}

/// The rows, in the order they are drawn.
pub fn of(files: &[FileChange]) -> Vec<Row> {
    let mut root = Node::default();
    for file in files {
        root.add(&file.path, file);
    }
    let mut rows = Vec::new();
    root.walk(String::new(), String::new(), 0, &mut rows);
    rows
}

/// The rows a person sees with the directories keyed in `folded` closed:
/// a folded directory's own row stays, and everything deeper than it until
/// the next row at its depth or above goes. M8.3.6.
pub fn visible<'a>(rows: &'a [Row], folded: &std::collections::HashSet<String>) -> Vec<&'a Row> {
    let mut shown = Vec::with_capacity(rows.len());
    let mut hiding_below: Option<usize> = None;
    for row in rows {
        let depth = match row {
            Row::Directory { depth, .. } | Row::File { depth, .. } => *depth,
        };
        // What a folded directory holds is deeper than its row — its files
        // and its directories both sit one level in.
        if let Some(limit) = hiding_below {
            if depth > limit {
                continue;
            }
            hiding_below = None;
        }
        shown.push(row);
        if let Row::Directory { key, depth, .. } = row
            && folded.contains(key)
        {
            hiding_below = Some(*depth);
        }
    }
    shown
}

/// One directory of the tree while it is being built.
#[derive(Default)]
struct Node<'a> {
    directories: BTreeMap<String, Node<'a>>,
    files: BTreeMap<String, &'a FileChange>,
}

impl<'a> Node<'a> {
    fn add(&mut self, path: &Path, file: &'a FileChange) {
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
        node.files.insert(name, file);
    }

    /// Every file beneath, however deep.
    fn count(&self) -> usize {
        self.files.len() + self.directories.values().map(Node::count).sum::<usize>()
    }

    /// Emit the rows, collapsing a run of single-child directories into
    /// the row that begins it.
    fn walk(&self, prefix: String, key: String, depth: usize, rows: &mut Vec<Row>) {
        if !prefix.is_empty() {
            rows.push(Row::Directory {
                path: prefix,
                files: self.count(),
                depth: depth.saturating_sub(1),
                key: key.clone(),
            });
        }

        // Directories first, then files: a tree read by the eye needs the
        // same order every time, and the directories are the summary.
        for (name, node) in &self.directories {
            let (name, node) = node.collapse(name.clone());
            let key = if key.is_empty() {
                name.clone()
            } else {
                format!("{key}/{name}")
            };
            node.walk(name, key, depth + 1, rows);
        }
        for (name, file) in &self.files {
            rows.push(Row::File {
                name: name.clone(),
                path: file.path.clone(),
                from: file.from.clone(),
                mark: file.mark,
                depth,
            });
        }
    }

    /// Follow a run of directories that each hold exactly one directory
    /// and no file, joining their names with a slash.
    fn collapse(&self, name: String) -> (String, &Self) {
        let mut name = name;
        let mut node = self;
        while node.files.is_empty() && node.directories.len() == 1 {
            let (only, child) = node.directories.iter().next().expect("one");
            name = format!("{name}/{only}");
            node = child;
        }
        (name, node)
    }
}

/// A commit's presentation. Owned by its project's Log, reset on commit changes.
#[derive(Default)]
pub struct Tree {
    pub rows: Vec<Row>,
    pub folded: std::collections::HashSet<String>,
    pub root_folded: bool,
}
impl Tree {
    pub fn new(files: &[FileChange]) -> Self {
        Self {
            rows: of(files),
            ..Self::default()
        }
    }
    /// Refresh the same commit, keeping only folds whose compact rows still exist.
    pub fn refresh(&mut self, files: &[FileChange]) {
        self.rows = of(files);
        let keys: std::collections::HashSet<&str> = self
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Directory { key, .. } => Some(key.as_str()),
                Row::File { .. } => None,
            })
            .collect();
        self.folded.retain(|key| keys.contains(key.as_str()));
    }
    pub fn visible(&self) -> Vec<&Row> {
        if self.root_folded {
            Vec::new()
        } else {
            visible(&self.rows, &self.folded)
        }
    }
    pub fn paths(&self) -> Vec<PathBuf> {
        self.visible()
            .into_iter()
            .filter_map(|r| match r {
                Row::File { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn toggle(&mut self, key: Option<&str>) {
        if let Some(key) = key {
            if !self.folded.remove(key) {
                self.folded.insert(key.to_owned());
            }
        } else {
            self.root_folded = !self.root_folded;
        }
    }
    pub fn expand_all(&mut self) {
        self.folded.clear();
        self.root_folded = false;
    }
    pub fn collapse_all(&mut self) {
        self.folded = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Directory { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect();
        self.root_folded = true;
    }
    /// Reveal the selected historical path without expanding unrelated directories.
    /// The returned scroll row includes the repository row.
    pub fn reveal(&mut self, path: &Path) -> Option<usize> {
        if !self
            .rows
            .iter()
            .any(|r| matches!(r, Row::File { path: p, .. } if p == path))
        {
            return None;
        }
        self.root_folded = false;
        self.folded.retain(|key| !path.starts_with(Path::new(key)));
        self.visible()
            .iter()
            .position(|r| matches!(r, Row::File {path: p, ..} if p == path))
            .map(|i| i + 1)
    }
}
