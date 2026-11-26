//! Where a project's dependencies were unpacked. M8.8.3, and M8.14.
//!
//! **The index space of the libraries a project has opened.** Since
//! M8.14 what *External Libraries* lists is what the build system
//! resolved — `root::dependencies` — and this is where a listed library
//! becomes a root once it is unfolded, and where a root `f12` reached
//! arrives: *the nearest ancestor of the landed file that holds the
//! language's manifest*, [`crate::highlight::dependency_root`]. Both
//! routes land on the same list, so a library unfolded after an `f12`
//! into it is one root with one index, not two.
//!
//! Until M8.14 this was the whole of the section, and a root arrived only
//! **because a person went there** — M8.8.3's decision 15, *nothing is
//! scanned, indexed or discovered in advance*, retired by decision 41 of
//! the spec when the person asked for the full list.
//!
//! **A tab under one of these roots is a place to read, and nothing
//! else.** It has no git status, no diff, no server document and no row
//! in the tree's own section for the project — every one of those
//! answers a question about *this project's* working tree, and a
//! dependency's sources are not in it. The window's gestures that would
//! act on the wrong file therefore match `Origin::Project` explicitly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use guild_session::ProjectId;

use crate::Root;

/// Where a project's dependencies were unpacked, per project.
///
/// A root is found by walking up from a file `f12` landed on until the
/// language's manifest is there — one rule for every language, which is
/// what makes `manifest` a column of the language table rather than a
/// function per language.
///
/// The order is the order roots were first landed in, and it is the
/// order `ProjectState.external_roots` keeps: `Origin::External(i)` is
/// an index into it, so the list may be appended to and never reordered.
#[derive(Debug, Default)]
pub(crate) struct Libraries {
    roots: HashMap<ProjectId, Vec<PathBuf>>,
}

impl Libraries {
    /// The roots known for a project, in their persisted order.
    pub(crate) fn of(&self, project: ProjectId) -> &[PathBuf] {
        self.roots.get(&project).map_or(&[], Vec::as_slice)
    }

    /// Where a root sits in the list, putting it there if it is new.
    ///
    /// Answers the index and whether the list grew, because growing is
    /// what has to be written to the disk and nothing else is.
    fn place(&mut self, project: ProjectId, root: PathBuf) -> (usize, bool) {
        let roots = self.roots.entry(project).or_default();
        match roots.iter().position(|known| *known == root) {
            Some(at) => (at, false),
            None => {
                roots.push(root);
                (roots.len() - 1, true)
            }
        }
    }

    /// What the state file said, read once per project.
    fn adopt(&mut self, project: ProjectId, roots: Vec<PathBuf>) {
        self.roots.entry(project).or_insert(roots);
    }
}

impl Root {
    /// The roots this project has landed in, read from its state file
    /// the first time they are asked for.
    pub(crate) fn ensure_libraries(&mut self, project: ProjectId) {
        if self.libraries.roots.contains_key(&project) {
            return;
        }
        let roots = self
            .desk
            .project(project)
            .map(|p| guild_session::project_state::load(&p.path).external_roots)
            .unwrap_or_default();
        // **A root that has gone is not a root.** A crate is deleted by
        // `cargo clean`, a module by `go clean -modcache`, and a stale
        // entry would keep its index alive for a tab nobody can open.
        let roots = roots.into_iter().filter(|root| root.is_dir()).collect();
        self.libraries.adopt(project, roots);
    }

    /// The tree a file tab's path is relative to: the project, or one of
    /// its external roots.
    ///
    /// `None` for an index no list has, which is what a state file
    /// written before a root was forgotten would ask for.
    pub(crate) fn root_of(&self, project: ProjectId, origin: crate::Origin) -> Option<PathBuf> {
        match origin {
            crate::Origin::Project => self.desk.project(project).map(|found| found.path.clone()),
            crate::Origin::External(at) => self.libraries.of(project).get(at).cloned(),
            crate::Origin::Outside => Some(PathBuf::from(crate::Origin::OUTSIDE_ROOT)),
        }
    }

    /// Note a dependency's root against a project and answer its origin,
    /// writing the list to the project's state when it grew.
    pub(crate) fn note_library(&mut self, project: ProjectId, root: &Path) -> crate::Origin {
        self.ensure_libraries(project);
        let (at, grew) = self.libraries.place(project, root.to_path_buf());
        if grew {
            self.store_libraries(project);
        }
        crate::Origin::External(at)
    }

    /// Write the roots into the project's state file.
    ///
    /// The whole file is read again before it is written, because `save`
    /// serialises all of `ProjectState` and a stale copy would drop the
    /// tabs, the sessions and the panels — the shape `remember_search`
    /// already has.
    fn store_libraries(&self, project: ProjectId) {
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let roots = self.libraries.of(project).to_vec();
        let mut file = guild_session::project_state::load(&path);
        if file.external_roots == roots {
            return;
        }
        file.external_roots = roots;
        if let Err(err) = guild_session::project_state::save(&path, &file) {
            tracing::warn!(project = %path.display(), %err, "the external roots were not saved");
        }
    }
}

#[cfg(feature = "native-preview")]
impl Root {
    pub(crate) fn native_occurrence_library(
        &mut self,
        project: ProjectId,
        root: PathBuf,
    ) -> crate::Origin {
        let (index, _) = self.libraries.place(project, root);
        crate::Origin::External(index)
    }
}
