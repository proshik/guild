//! What `git status --porcelain=v2 -z` said, parsed, and what it means for
//! every row of the tree and the directories above it.
//!
//! Porcelain v2 is the format `git status` promises to scripts: one record
//! per path, fields separated by spaces, the path last, records separated
//! by NUL under `-z` — and a rename's original path as a record of its
//! own right after. Stable since git 2.11; the fixture in `tests/` is
//! captured from git 2.50 and every case the parser knows is in it.
//!
//! The map is bounded by the number of *changed* paths and their
//! ancestors, never by the size of the repository — the reason the walk
//! asks git for nothing per row.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use crate::theme::Mark;

/// What happened to a path since the last commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Changed, in the index or the working tree or both.
    Modified,
    /// New since the last commit and staged.
    Added,
    /// Gone, from the index or the working tree.
    Deleted,
    /// Moved: the entry's path is the new one.
    Renamed { from: PathBuf },
    /// Git does not know it. A directory when the path ends in `/`.
    Untracked,
    /// Named by an ignore rule.
    Ignored,
    /// In the middle of a merge, with both sides changed.
    Unmerged,
}

/// One path git had something to say about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Relative to the repository's root, `/`-separated as git prints it.
    pub path: PathBuf,
    pub kind: Kind,
    /// The index differs from `HEAD` — what a commit would take.
    pub staged: bool,
    /// The working tree differs from the index.
    pub unstaged: bool,
    /// An untracked or ignored *directory*, which git reports without
    /// descending into it.
    pub is_dir: bool,
}

/// Bytes that were not porcelain v2.
#[derive(Debug, thiserror::Error)]
#[error("not porcelain v2 at record {record}: {reason}")]
pub struct ParseError {
    /// Which NUL-separated record, counting from one.
    pub record: usize,
    pub reason: &'static str,
}

/// Parse `git status --porcelain=v2 -z` output.
pub fn parse(bytes: &[u8]) -> Result<Vec<Entry>, ParseError> {
    let mut entries = Vec::new();
    let mut records = bytes
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .enumerate()
        .peekable();
    while let Some((index, record)) = records.next() {
        let record_number = index + 1;
        let text = std::str::from_utf8(record).map_err(|_| ParseError {
            record: record_number,
            reason: "not UTF-8",
        })?;
        let fail = |reason| ParseError {
            record: record_number,
            reason,
        };
        let (tag, rest) = text.split_at(text.find(' ').ok_or(fail("no fields"))?);
        let rest = &rest[1..];
        match tag {
            "1" => {
                let fields: Vec<&str> = rest.splitn(8, ' ').collect();
                let [xy, _sub, _mh, _mi, _mw, _hh, _hi, path] = fields[..] else {
                    return Err(fail("a changed entry has eight fields"));
                };
                entries.push(tracked(xy, path, None).ok_or(fail("unknown XY"))?);
            }
            "2" => {
                let fields: Vec<&str> = rest.splitn(9, ' ').collect();
                let [xy, _sub, _mh, _mi, _mw, _hh, _hi, _score, path] = fields[..] else {
                    return Err(fail("a renamed entry has nine fields"));
                };
                let (_, from) = records
                    .next()
                    .ok_or(fail("a rename without its original path"))?;
                let from = std::str::from_utf8(from).map_err(|_| fail("not UTF-8"))?;
                entries.push(tracked(xy, path, Some(from)).ok_or(fail("unknown XY"))?);
            }
            "u" => {
                let fields: Vec<&str> = rest.splitn(10, ' ').collect();
                let [_xy, _sub, _m1, _m2, _m3, _mw, _h1, _h2, _h3, path] = fields[..] else {
                    return Err(fail("an unmerged entry has ten fields"));
                };
                entries.push(Entry {
                    path: PathBuf::from(path),
                    kind: Kind::Unmerged,
                    staged: true,
                    unstaged: true,
                    is_dir: false,
                });
            }
            "?" | "!" => {
                let is_dir = rest.ends_with('/');
                entries.push(Entry {
                    path: PathBuf::from(rest.trim_end_matches('/')),
                    kind: if tag == "?" {
                        Kind::Untracked
                    } else {
                        Kind::Ignored
                    },
                    staged: false,
                    unstaged: false,
                    is_dir,
                });
            }
            // Headers (`# branch.oid …`) come only with `--branch`, which
            // is not asked for; anything else is not this format.
            "#" => {}
            _ => return Err(fail("unknown record tag")),
        }
    }
    Ok(entries)
}

/// A `1` or `2` record's XY into an entry. `X` is the index against
/// `HEAD`, `Y` the working tree against the index; `.` is unchanged.
fn tracked(xy: &str, path: &str, from: Option<&str>) -> Option<Entry> {
    let mut letters = xy.chars();
    let (x, y) = (letters.next()?, letters.next()?);
    let kind = match (x, y) {
        _ if x == 'D' || y == 'D' => Kind::Deleted,
        ('R', _) | (_, 'R') => Kind::Renamed {
            from: PathBuf::from(from?),
        },
        ('A', _) | ('C', _) => Kind::Added,
        _ if "MT".contains(x) || "MT".contains(y) => Kind::Modified,
        ('U', _) | (_, 'U') => Kind::Unmerged,
        _ => return None,
    };
    Some(Entry {
        path: PathBuf::from(path),
        kind,
        staged: x != '.',
        unstaged: y != '.',
        is_dir: false,
    })
}

/// Every changed path and its mark, and a mark for every directory above
/// one: the strongest of what is under it, `M` over `D` over `A` over `?`.
#[derive(Debug, Default, Clone)]
pub struct StatusMap {
    entries: BTreeMap<PathBuf, Entry>,
    /// Marks of files and of the directories above them. Not the marks of
    /// what is *inside* an untracked directory: those are answered by
    /// walking up, see `mark_for`.
    marks: HashMap<PathBuf, Mark>,
    /// Paths staged outside the folder the project is, when the project
    /// is a folder inside its repository — repository-relative, as git
    /// prints them. Not rows of the tree, and not marks: what a commit
    /// from here would take without showing it. Decision 18 of M5.11.
    staged_elsewhere: Vec<PathBuf>,
}

impl StatusMap {
    /// The map from what git said.
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        let mut map = Self::default();
        for entry in entries {
            if let Some(mark) = mark_of(&entry.kind) {
                for ancestor in ancestors(&entry.path) {
                    map.marks
                        .entry(ancestor)
                        .and_modify(|had| *had = (*had).max(mark))
                        .or_insert(mark);
                }
                map.marks.insert(entry.path.clone(), mark);
            }
            map.entries.insert(entry.path.clone(), entry);
        }
        map
    }

    /// The mark a row draws — its own, a directory's from what is under
    /// it, or a file's from the untracked directory it is inside, which is
    /// the one case a mark is inherited downwards: git reported the
    /// directory and not its contents.
    pub fn mark_for(&self, path: &Path) -> Option<Mark> {
        if let Some(mark) = self.marks.get(path) {
            return Some(*mark);
        }
        for ancestor in ancestors(path) {
            if let Some(entry) = self.entries.get(&ancestor)
                && entry.is_dir
                && entry.kind == Kind::Untracked
            {
                return Some(Mark::Untracked);
            }
        }
        None
    }

    /// What git said about exactly this path.
    pub fn entry(&self, path: &Path) -> Option<&Entry> {
        self.entries.get(path)
    }

    /// Every entry, in path order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// How many paths carry a mark of any kind, including the directories
    /// above changed files.
    pub fn marked(&self) -> usize {
        self.marks.len()
    }

    /// Nothing changed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The map, knowing what is staged outside its folder.
    pub fn with_staged_elsewhere(mut self, paths: Vec<PathBuf>) -> Self {
        self.staged_elsewhere = paths;
        self
    }

    /// What the index holds outside this folder, which a commit from here
    /// would take too. Empty for a project that is its repository's root.
    pub fn staged_elsewhere(&self) -> &[PathBuf] {
        &self.staged_elsewhere
    }
}

/// git's entries for a project that is the folder `prefix` of its
/// repository: those under it, made relative to it, and the paths staged
/// outside it.
///
/// The second list is the reason this is not a filter. git's index is
/// the repository's, and `git commit` takes all of it — so a change staged
/// in `../root.txt` by a terminal would ride along with a commit made from
/// a folder that cannot show it. What is merely changed or untracked
/// outside is not a commit's business and is dropped. A staged rename into
/// the folder from outside removes its old path from the index, which is
/// outside too, so that path is named as well. An empty `prefix` — the
/// repository itself opened — keeps every entry and names nothing.
/// Decision 18 of M5.11.
pub fn within(entries: Vec<Entry>, prefix: &Path) -> (Vec<Entry>, Vec<PathBuf>) {
    if prefix.as_os_str().is_empty() {
        return (entries, Vec::new());
    }
    let mut inside = Vec::new();
    let mut elsewhere = Vec::new();
    for mut entry in entries {
        // By component, not by string: `subway/` is not under `sub`.
        let Ok(relative) = entry.path.strip_prefix(prefix).map(Path::to_path_buf) else {
            if entry.staged {
                elsewhere.push(entry.path);
            }
            continue;
        };
        entry.path = relative;
        if let Kind::Renamed { from } = &mut entry.kind {
            match from.strip_prefix(prefix) {
                Ok(relative) => *from = relative.to_path_buf(),
                Err(_) if entry.staged => elsewhere.push(from.clone()),
                Err(_) => {}
            }
        }
        inside.push(entry);
    }
    (inside, elsewhere)
}

fn mark_of(kind: &Kind) -> Option<Mark> {
    Some(match kind {
        Kind::Modified | Kind::Renamed { .. } | Kind::Unmerged => Mark::Modified,
        Kind::Added => Mark::Added,
        Kind::Deleted => Mark::Deleted,
        Kind::Untracked => Mark::Untracked,
        Kind::Ignored => return None,
    })
}

/// The directories above a path, nearest first, the root excluded — the
/// root is every change's ancestor and a mark on it would say nothing.
fn ancestors(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dir = PathBuf::new();
    let parts: Vec<_> = path.components().collect();
    for component in &parts[..parts.len().saturating_sub(1)] {
        if let Component::Normal(part) = component {
            dir.push(part);
            out.push(dir.clone());
        }
    }
    out.reverse();
    out
}
