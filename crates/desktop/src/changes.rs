//! The Changes panel's model: what `git status` said, in the three
//! groups git itself uses — what a commit would take, what it would
//! leave, and what git does not know about. Pure: built from a
//! `StatusMap` and nothing else, so `tests/changes.rs` checks it on the
//! captured porcelain without a repository.
//!
//! The groups are git's rather than an IDE's changelists on purpose.
//! A changelist is the IDE's own idea, kept in its own files, and what
//! it does at commit time is stage what was ticked; showing the index
//! as it is means a `git add` in the terminal beside the panel moves a
//! row the moment the map lands, and a person who reads `git status`
//! reads the panel.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::git::status::{Entry, Kind, StatusMap};
use crate::theme::Mark;

/// A directory read by a background task. Only Git-eligible files are targets.
#[derive(Debug, Clone, Default)]
pub struct Directory {
    pub files: Vec<PathBuf>,
    pub ignored: usize,
    pub directories: Vec<PathBuf>,
    pub ignored_paths: Vec<PathBuf>,
    pub empty: bool,
    pub empty_directories: HashSet<PathBuf>,
}

pub fn discover_directory(root: &Path, path: &Path) -> Result<Directory, crate::git::GitError> {
    let name = path
        .to_str()
        .ok_or_else(|| crate::git::GitError::Failed("Invalid directory name".into()))?;
    let selector = if name.is_empty() { "." } else { name };
    let read = |ignored| {
        let mut args = vec![
            "--literal-pathspecs",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
        ];
        if ignored {
            args.push("--ignored");
        }
        args.extend(["--", selector]);
        crate::git::run(root, &args).and_then(|bytes| {
            bytes
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| {
                    std::str::from_utf8(p).map(PathBuf::from).map_err(|_| {
                        crate::git::GitError::Failed("Unsupported non-UTF-8 path".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
    };
    let files: Vec<PathBuf> = read(false)?
        .into_iter()
        .filter(|p| !root.join(p).symlink_metadata().is_ok_and(|m| m.is_dir()))
        .collect();
    let ignored_paths = read(true)?;
    let mut directories = Vec::new();
    let mut empty_directories = HashSet::new();
    for entry in ignore::WalkBuilder::new(root.join(path))
        .hidden(false)
        .follow_links(false)
        .build()
    {
        let entry = entry.map_err(|e| crate::git::GitError::Failed(e.to_string()))?;
        if entry.depth() > 0
            && entry.file_type().is_some_and(|t| t.is_dir())
            && let Ok(relative) = entry.path().strip_prefix(root)
        {
            directories.push(relative.into());
            if std::fs::read_dir(entry.path())
                .map_err(|e| crate::git::GitError::Failed(e.to_string()))?
                .next()
                .is_none()
            {
                empty_directories.insert(relative.into());
            }
        }
    }
    // Git is authoritative even if a .ignore rule hides an ancestor from the walker.
    for file in &files {
        for parent in file
            .ancestors()
            .skip(1)
            .take_while(|p| *p != path && p.starts_with(path))
        {
            if !directories.iter().any(|p| p == parent) {
                directories.push(parent.into());
            }
        }
    }
    Ok(Directory {
        files,
        ignored: ignored_paths.len(),
        ignored_paths,
        directories,
        empty_directories,
        empty: std::fs::read_dir(root.join(path))
            .map_err(|e| crate::git::GitError::Failed(e.to_string()))?
            .next()
            .is_none(),
    })
}

/// Build descendant counts in the same background pass as filesystem discovery.
pub fn discover_directory_tree(
    root: &Path,
    path: &Path,
) -> HashMap<PathBuf, Result<Directory, String>> {
    let mut snapshots = HashMap::new();
    let result = discover_directory(root, path).map_err(|error| error.to_string());
    if let Ok(directory) = &result {
        for child in &directory.directories {
            snapshots.insert(
                child.clone(),
                Ok(Directory {
                    files: directory
                        .files
                        .iter()
                        .filter(|f| f.starts_with(child))
                        .cloned()
                        .collect(),
                    ignored: directory
                        .ignored_paths
                        .iter()
                        .filter(|f| f.starts_with(child))
                        .count(),
                    empty: directory.empty_directories.contains(child),
                    ..Default::default()
                }),
            );
        }
    }
    snapshots.insert(path.into(), result);
    snapshots
}

/// Structured guards are translated at display time, never stored as UI prose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EligibilityError {
    #[error("{} is no longer available", .0.display())]
    Missing(PathBuf),
    #[error("Unsupported non-UTF-8 path")]
    UnsupportedPath,
    #[error("Path contains NUL")]
    EmbeddedNul,
    #[error("{} has unresolved conflicts", .0.display())]
    Conflict(PathBuf),
    #[error("{} is still loading", .0.display())]
    Loading(PathBuf),
    #[error("{} has no eligible files; Git does not track empty directories", .0.display())]
    Empty(PathBuf),
    #[error("{}: {}", .0.display(), .1)]
    Read(PathBuf, String),
}

fn check_path_identities(paths: &[PathBuf]) -> Result<(), EligibilityError> {
    if paths
        .iter()
        .any(|path| path.as_os_str().as_encoded_bytes().contains(&0))
    {
        return Err(EligibilityError::EmbeddedNul);
    }
    if paths.iter().any(|path| path.to_str().is_none()) {
        return Err(EligibilityError::UnsupportedPath);
    }
    Ok(())
}

/// Resolve a whole selection or explain why it cannot be honored; never skip rows.
pub fn eligible_targets(
    list: &[Listed],
    directories: &HashMap<PathBuf, Result<Directory, String>>,
    selected: &[PathBuf],
) -> Result<Vec<PathBuf>, EligibilityError> {
    check_path_identities(selected)?;
    let mut paths = Vec::new();
    let rows: HashMap<_, _> = list.iter().map(|l| (&l.row.path, l)).collect();
    let selected = crate::file_operations::normalize(selected);
    for path in &selected {
        let row = rows
            .get(path)
            .ok_or_else(|| EligibilityError::Missing(path.clone()))?;
        if row.row.unmerged {
            return Err(EligibilityError::Conflict(path.clone()));
        }
        if row.row.is_dir {
            let dir = directories
                .get(path)
                .ok_or_else(|| EligibilityError::Loading(path.clone()))?
                .as_ref()
                .map_err(|error| EligibilityError::Read(path.clone(), error.clone()))?;
            if dir.files.is_empty() {
                return Err(EligibilityError::Empty(path.clone()));
            }
            paths.extend(dir.files.iter().cloned());
        } else {
            paths.push(path.clone());
        }
    }
    check_path_identities(&paths)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Which of git's three lists a row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// The index differs from `HEAD`: what a commit would take.
    Staged,
    /// The working tree differs from the index: what a commit would leave.
    Unstaged,
    /// Files and directories git does not know about.
    Untracked,
}

impl GroupKind {
    /// The three, in git's order.
    pub const ALL: [GroupKind; 3] = [GroupKind::Staged, GroupKind::Unstaged, GroupKind::Untracked];
}

/// One file — or one untracked directory — in a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Relative to the project, as the tree's paths are.
    pub path: PathBuf,
    /// The letter the tree draws for it.
    pub mark: Mark,
    /// A staged rename's old name. Nowhere else.
    pub from: Option<PathBuf>,
    /// An untracked directory git did not descend into: background discovery
    /// supplies its child rows; the directory itself has no file diff.
    pub is_dir: bool,
    /// In the middle of a merge with both sides changed: a commit cannot
    /// take it, so it is never ticked on arrival and the plan leaves its
    /// index entry alone — `git add` on it would call it resolved.
    pub unmerged: bool,
    /// The index differs from `HEAD` for this path — git's `X` side. What
    /// `to_tick` watches: a side that became staged was staged by a hand
    /// that meant it.
    pub staged: bool,
}

/// One of the three lists, with its rows in path order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub kind: GroupKind,
    pub rows: Vec<Row>,
}

/// The three groups, always all three and in git's order, so the panel
/// draws one shape whether the tree is clean or not. A file changed in
/// the index and in the working tree is in two groups; an unmerged file
/// is unstaged only, because a commit cannot take a conflict and `git
/// add` is how it stops being one; what an ignore rule names is in none.
pub fn groups(map: &StatusMap) -> Vec<Group> {
    GroupKind::ALL
        .into_iter()
        .map(|kind| Group {
            kind,
            rows: map
                .entries()
                .filter_map(|entry| row_in(entry, kind))
                .collect(),
        })
        .collect()
}

fn row_in(entry: &Entry, kind: GroupKind) -> Option<Row> {
    let (mark, from) = match (kind, &entry.kind) {
        (_, Kind::Ignored) => return None,
        (GroupKind::Untracked, Kind::Untracked) => (Mark::Untracked, None),
        (GroupKind::Untracked, _) | (_, Kind::Untracked) => return None,
        (GroupKind::Staged, Kind::Unmerged) => return None,
        (GroupKind::Staged, kind) if entry.staged => (mark_of(kind), renamed_from(kind)),
        (GroupKind::Unstaged, kind) if entry.unstaged => (mark_of(kind), None),
        _ => return None,
    };
    Some(Row {
        path: entry.path.clone(),
        mark,
        from,
        is_dir: entry.is_dir,
        unmerged: entry.kind == Kind::Unmerged,
        staged: entry.staged,
    })
}

fn mark_of(kind: &Kind) -> Mark {
    match kind {
        Kind::Added => Mark::Added,
        Kind::Deleted => Mark::Deleted,
        Kind::Untracked => Mark::Untracked,
        Kind::Modified | Kind::Renamed { .. } | Kind::Unmerged | Kind::Ignored => Mark::Modified,
    }
}

fn renamed_from(kind: &Kind) -> Option<PathBuf> {
    match kind {
        Kind::Renamed { from } => Some(from.clone()),
        _ => None,
    }
}

/// Every row in the order the panel draws them: the changes, then the
/// unversioned — what the arrows walk.
pub fn rows_in_order(list: &[Listed]) -> Vec<&Listed> {
    list.iter().collect()
}

/// The line a row is drawn on, counting the head of every section that
/// has rows — which is what a scroll to the row needs, and why it lives
/// beside the model rather than in the view.
pub fn line_of(list: &[Listed], section: Section, path: &Path) -> Option<usize> {
    let mut line = 0;
    for current in [Section::Changes, Section::Unversioned] {
        let rows: Vec<&Listed> = list.iter().filter(|l| l.section == current).collect();
        if rows.is_empty() {
            continue;
        }
        line += 1;
        for listed in rows {
            if current == section && listed.row.path == path {
                return Some(line);
            }
            line += 1;
        }
    }
    None
}

/// Which of the two lists a row is in — a Commit window's, since
/// M8.3.1: every tracked change in one, whatever git has staged of it,
/// and the files git does not know in the other. Decision 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// Every tracked change, one row per file.
    Changes,
    /// Untracked files and directories.
    Unversioned,
}

/// One row of the list, with the section it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub section: Section,
    pub row: Row,
}

/// The list: the changes first, then the unversioned, each in path
/// order; what an ignore rule names is in neither. A file changed on
/// both sides of the index is one row, and a staged rename still says
/// where it came from.
pub fn list(map: &StatusMap) -> Vec<Listed> {
    let mut changes = Vec::new();
    let mut unversioned = Vec::new();
    for entry in map.entries() {
        let row = Row {
            path: entry.path.clone(),
            mark: mark_of(&entry.kind),
            from: renamed_from(&entry.kind),
            is_dir: entry.is_dir,
            unmerged: entry.kind == Kind::Unmerged,
            staged: entry.staged,
        };
        match entry.kind {
            Kind::Ignored => {}
            Kind::Untracked => unversioned.push(Listed {
                section: Section::Unversioned,
                row,
            }),
            _ => changes.push(Listed {
                section: Section::Changes,
                row,
            }),
        }
    }
    changes.extend(unversioned);
    changes
}

/// The paths a list that just landed ticks, against the list before it.
///
/// **The ticks decide what a commit takes** — decision 19 of the spec — so
/// a staged row left unticked is a row the commit unstages. A change is
/// ticked when it arrives in *Changes* from nowhere, from *Unversioned*
/// or from a conflict, and when its index side went from unstaged to
/// staged since the last list. Nothing in the window stages a tracked
/// change on its own except a commit, which stages only what is already
/// ticked, so an index side that became staged was staged by a terminal,
/// an agent or *Add to VCS* — a hand that meant it. Every other row keeps
/// the tick the person left, and a conflict is never ticked. Decision 16
/// of M5.11.
///
/// The list before is indexed by path first: this runs on the window's
/// thread for every map that lands, and an agent reformatting a few
/// thousand tracked files would make a lookup per row a quadratic walk.
pub fn to_tick(before: &[Listed], now: &[Listed]) -> Vec<PathBuf> {
    let mut was_listed: HashMap<&Path, &Listed> = HashMap::with_capacity(before.len());
    for was in before {
        was_listed.entry(was.row.path.as_path()).or_insert(was);
    }
    now.iter()
        .filter(|listed| listed.section == Section::Changes && !listed.row.unmerged)
        .filter(|listed| match was_listed.get(listed.row.path.as_path()) {
            None => true,
            Some(was) => {
                was.section == Section::Unversioned
                    || was.row.unmerged
                    || (!was.row.staged && listed.row.staged)
            }
        })
        .map(|listed| listed.row.path.clone())
        .collect()
}

/// [`to_tick`], with the one case a list without renames brings: a staged
/// rename `old → new` split into `D old` and `A new` — the walk that settles
/// a hunk write detects no renames in a partial clone (decision 55). `old`
/// was never a row's path, only the rename's `from`, so `to_tick` sees it
/// first now; it takes the rename row's tick instead — ticked only when
/// `new` is in `included`, the ticked and partial rows. An excluded rename
/// stays out of the commit. The person's diff review of GLD-026 r5.
pub fn ticks_for(before: &[Listed], now: &[Listed], included: &HashSet<PathBuf>) -> Vec<PathBuf> {
    to_tick(before, now)
        .into_iter()
        .filter(|path| {
            match before
                .iter()
                .find(|listed| listed.row.from.as_deref() == Some(path.as_path()))
            {
                Some(rename) => included.contains(&rename.row.path),
                None => true,
            }
        })
        .collect()
}

/// The two writes a commit makes before its own, to bring the index to
/// the ticks. Decision 4 of M8.3.1.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitPlan {
    /// `git add`: ticked rows not fully staged, ticked unversioned files.
    pub stage: Vec<PathBuf>,
    /// `git restore --staged`: staged rows left unticked, a rename's old
    /// path beside its new one.
    pub unstage: Vec<PathBuf>,
}

/// What the index needs before the commit: a ticked row with something
/// unstaged is added, a staged row left unticked is taken back out —
/// with both of its paths when it is a rename — and a ticked
/// unversioned file is added. A ticked row already staged needs
/// nothing; an ignored entry is nobody's, and so is a conflict.
///
/// **A partial row is left as its index stands** — GLD-026, decision 55:
/// part of it was staged by the hunk, and adding it whole or taking it back
/// out would undo exactly that.
pub fn commit_plan(
    map: &StatusMap,
    ticked: &HashSet<PathBuf>,
    partial: &HashSet<PathBuf>,
) -> CommitPlan {
    let mut plan = CommitPlan::default();
    for entry in map.entries() {
        if partial.contains(&entry.path) {
            continue;
        }
        let is_ticked = ticked.contains(&entry.path);
        match entry.kind {
            // A conflict is nobody's to stage or unstage from here: `git add`
            // would call it resolved, and git refuses the commit itself.
            Kind::Ignored | Kind::Unmerged => {}
            Kind::Untracked => {
                if entry.is_dir {
                    plan.stage.extend(
                        ticked
                            .iter()
                            .filter(|path| *path != &entry.path && path.starts_with(&entry.path))
                            .cloned(),
                    );
                } else if is_ticked {
                    plan.stage.push(entry.path.clone());
                }
            }
            _ => {
                if is_ticked && (entry.unstaged || !entry.staged) {
                    plan.stage.push(entry.path.clone());
                } else if !is_ticked && entry.staged {
                    plan.unstage.push(entry.path.clone());
                    if let Kind::Renamed { from } = &entry.kind {
                        plan.unstage.push(from.clone());
                    }
                }
            }
        }
    }
    plan.stage.sort();
    plan.unstage.sort();
    plan
}

/// What rolling one row back means, as the write it becomes — or
/// nothing, when the row has nothing to go back to.
///
/// **Three answers, and only one of them is [`Write::Rollback`].**
/// Measured against a real git before this was written:
///
/// - An **unversioned** row is in neither `HEAD` nor the index, so
///   there is nothing to restore. `restore` refuses it — *pathspec did
///   not match any file known to git* — and the menu does not offer the
///   item at all rather than putting up a sheet that ends in a refusal.
/// - A row that is **added**: `restore --source=HEAD --worktree` on a
///   path `HEAD` does not have reads as *there should be nothing here*
///   and **deletes the file from the disk**. The usual IDE default is the
///   opposite — the file is unstaged and left where it is, untracked —
///   so this is [`Write::Unstage`], the write we already had.
/// - A **conflicted** row is refused, and this one is refused for the
///   opposite reason to the unversioned one: git does not argue.
///   `restore --source=HEAD --staged --worktree` on an unmerged path
///   succeeds, writes `HEAD`'s side into both, and thereby takes the
///   path out of the conflicted state — leaving the merge standing with
///   the incoming side silently gone. *Rollback* says *undo my
///   changes*; that would be *resolve this in favour of ours*, which is
///   a different word and belongs to the merge pass decision 21 named.
/// - Everything else is the rollback, **with a rename's old path
///   beside its new one**: `restore` given only the new path leaves the
///   old one deleted in the index, which is a half-undone rename and
///   worse than either end of it.
///
/// Pure, and here beside the list rather than in the window: it is a
/// rule about a row, and the window's job is to ask before it runs.
pub fn rollback_of(list: &[Listed], path: &Path) -> Option<crate::git::Write> {
    let listed = list.iter().find(|l| l.row.path == path)?;
    if listed.section == Section::Unversioned || listed.row.unmerged {
        return None;
    }
    let mut paths = vec![listed.row.path.clone()];
    if listed.row.mark == Mark::Added {
        return Some(crate::git::Write::Unstage(paths));
    }
    paths.extend(listed.row.from.clone());
    Some(crate::git::Write::Rollback(paths))
}

/// Whether a commit can be made: something ticked, and something said —
/// a message of only whitespace is not a message.
pub fn can_commit(ticked: &HashSet<PathBuf>, partial: &HashSet<PathBuf>, message: &str) -> bool {
    (!ticked.is_empty() || !partial.is_empty()) && !message.trim().is_empty()
}

/// The tick of a row a hunk write changed, from its sides now — the person's
/// choice: partial when part is staged, full when all of it, cleared when
/// none; a row no longer listed keeps nothing. Whatever the tick was.
pub fn tick_from_index(
    path: &Path,
    sides: Option<(bool, bool)>,
    ticked: &mut HashSet<PathBuf>,
    partial: &mut HashSet<PathBuf>,
) {
    ticked.remove(path);
    partial.remove(path);
    match sides {
        Some((true, true)) => {
            partial.insert(path.to_path_buf());
        }
        Some((true, false)) => {
            ticked.insert(path.to_path_buf());
        }
        _ => {}
    }
}

/// Each tracked, unconflicted row's two sides: staged, unstaged.
pub fn sides(map: &StatusMap) -> HashMap<PathBuf, (bool, bool)> {
    map.entries()
        .filter(|entry| {
            !matches!(entry.kind, Kind::Ignored | Kind::Untracked | Kind::Unmerged) && !entry.is_dir
        })
        .map(|entry| (entry.path.clone(), (entry.staged, entry.unstaged)))
        .collect()
}

/// The ticks after a map lands. **A status transition alone never adds a
/// path to the commit** — review r3 `bugs+impl-1`: a full or cleared tick
/// is the person's and stays, whichever way the row's sides turn, so an
/// excluded row edited on disk does not come back as partial, and a full
/// one edited on disk is still committed whole, as the person chose. Only a
/// partial tick follows git: full once nothing of the row is unstaged —
/// the commit takes the same bytes — cleared once nothing of it is staged,
/// and gone with its row. A hunk write's own rows follow the index through
/// [`tick_from_index`]. GLD-026.
pub fn retick(
    now: &HashMap<PathBuf, (bool, bool)>,
    ticked: &mut HashSet<PathBuf>,
    partial: &mut HashSet<PathBuf>,
) {
    partial.retain(|path| match now.get(path) {
        Some((true, false)) => {
            ticked.insert(path.clone());
            false
        }
        Some((true, true)) => true,
        _ => false,
    });
    ticked.retain(|path| !partial.contains(path));
}

/// What goes into the message file: the message as typed, with one
/// newline at the end and nothing escaped — `-F` is why a file at all.
/// Trailing blank lines go, since git would drop them and the file would
/// otherwise say the message was longer than it was.
pub fn message_file_contents(message: &str) -> String {
    format!("{}\n", message.trim_end_matches(['\n', '\r']))
}
