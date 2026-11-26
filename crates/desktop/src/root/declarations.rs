//! What the tree's Kotlin and Java files declare, read off the window's
//! thread — M8.13.
//!
//! `desktop::declaration` answers for a text; this is where the texts come
//! from. The tree's rows are built on every render and there can be
//! thousands of them, so nothing here reads a file while a frame is being
//! drawn: the watch loop hands the rows nobody has read yet to a
//! background task, the answers land in a map, and the next frame draws
//! them. Until then a row wears its language's icon, which is also what
//! it wears when it declares nothing — so a late answer is a change of
//! icon, never a blank.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::{AppContext as _, Context};
use guild_session::ProjectId;

use crate::Root;
use crate::declaration::{self, Kind};
use crate::highlight::Language;

/// How many files one task reads. A tree opened on a large Kotlin module
/// asks again on the next tick for the rest, so the first rows on the
/// screen are answered in the first tick rather than the last.
const BATCH: usize = 256;

/// What has been read, and what is being read.
#[derive(Default)]
pub(crate) struct Declarations {
    /// A row's answer, `None` included: a file of functions has been read
    /// and declares nothing, which is not the same as not read.
    known: HashMap<(ProjectId, PathBuf), Option<Kind>>,
    /// Handed to a task and not back yet — asked once, not once a tick.
    asked: HashSet<(ProjectId, PathBuf)>,
    generations: HashMap<ProjectId, u64>,
}

impl Declarations {
    pub(crate) fn invalidate_subtree(&mut self, project: ProjectId, from: &std::path::Path) {
        *self.generations.entry(project).or_default() += 1;
        self.known
            .retain(|(owner, path), _| *owner != project || !path.starts_with(from));
        self.asked.retain(|(owner, _)| *owner != project);
    }

    /// What a row declares, when that is known and is something.
    pub(crate) fn of(&self, project: ProjectId, path: &std::path::Path) -> Option<Kind> {
        self.known
            .get(&(project, path.to_path_buf()))
            .copied()
            .flatten()
    }

    /// Forget the files the disk says changed, so they are read again.
    /// An answer about a file somebody has since rewritten is the
    /// plausible lie `SidePaint::of_line` exists to stop in a diff.
    pub(crate) fn forget(&mut self, project: ProjectId, files: &[PathBuf]) {
        for file in files {
            self.known.remove(&(project, file.clone()));
        }
    }
}

impl Root {
    /// Hand the active tree's unread Kotlin and Java rows to a background
    /// task. On the watch loop's clock, beside everything else a thread
    /// answers.
    pub(crate) fn read_declarations(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(root) = self.project_root() else {
            return;
        };
        let seat = self.seat_for_project(project);
        let Some(state) = self.trees.get(&seat) else {
            return;
        };
        let wanted: Vec<(PathBuf, &'static str)> = state
            .tree
            .rows()
            .iter()
            .filter(|row| !row.is_dir())
            .filter_map(|row| {
                let language = Language::of(row.path())?;
                declaration::reads(language.name).then(|| (row.path().to_path_buf(), language.name))
            })
            .filter(|(path, _)| {
                let key = (project, path.clone());
                !self.declarations.known.contains_key(&key)
                    && !self.declarations.asked.contains(&key)
            })
            .take(BATCH)
            .collect();
        if wanted.is_empty() {
            return;
        }
        for (path, _) in &wanted {
            self.declarations.asked.insert((project, path.clone()));
        }

        let generation = self
            .declarations
            .generations
            .get(&project)
            .copied()
            .unwrap_or_default();
        let work = cx.background_spawn(async move {
            wanted
                .into_iter()
                .map(|(path, language)| {
                    let kind = read(&root.join(&path), &path, language);
                    (path, kind)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |root, cx| {
            let answers = work.await;
            let _ = root.update(cx, |root, cx| {
                if root
                    .declarations
                    .generations
                    .get(&project)
                    .copied()
                    .unwrap_or_default()
                    != generation
                {
                    return;
                }
                for (path, kind) in answers {
                    let key = (project, path);
                    root.declarations.asked.remove(&key);
                    root.declarations.known.insert(key, kind);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// One file's answer. A file too large to open in a tab is not read for
/// its icon either — the limit is the tab's, so the two cannot disagree
/// about what counts as a source file.
fn read(full: &std::path::Path, relative: &std::path::Path, language: &str) -> Option<Kind> {
    let size = std::fs::metadata(full).ok()?.len();
    if size > crate::file_tab::OPEN_LIMIT {
        return None;
    }
    let text = std::fs::read_to_string(full).ok()?;
    let stem = relative.file_stem()?.to_str()?;
    declaration::of(language, stem, &text)
}
