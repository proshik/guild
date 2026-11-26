//! What each open project depends on, asked of its build systems on a
//! thread — M8.14.
//!
//! `desktop::deps` reads an answer and `deps::ask` asks for one; this is
//! where the asking is started and where the answers land. **A thread per
//! project and per asking**, the git worker's shape: it asks every stack
//! the project has, one after another, sends each answer down a channel,
//! and ends. The watch loop takes what has arrived, as it takes what git
//! and the language servers say, so a Gradle that needs minutes the first
//! time is a list that fills in, never a window that stops.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use gpui::{AppContext as _, Context};
use guild_session::ProjectId;

use crate::Root;
use crate::deps::{self, Answer, Stack};

/// What is known of each project's dependencies.
#[derive(Default)]
pub(crate) struct Dependencies {
    of: HashMap<ProjectId, Known>,
    /// The library roots whose archive is being extracted now.
    unpacking: std::collections::HashSet<PathBuf>,
}

/// What an asking thread says.
enum Heard {
    /// What the stacks answered last time, before any is asked — so a
    /// reopened project lists at once. Task 4.
    Cached(Answer),
    GoInputs(crate::watch::DependencyInputs),
    /// A stack's answer now.
    Asked(Stack, Result<Answer, String>),
}

/// One project's.
#[derive(Default)]
pub(crate) struct Known {
    /// Everything answered so far, merged across the project's stacks.
    pub answer: Answer,
    /// The stacks that could not be asked, and why — said where the list
    /// is, so that a missing Gradle is not a list that is merely short.
    pub failed: Vec<(Stack, String)>,
    /// The asking in progress, while it is.
    asking: Option<Receiver<Heard>>,
    /// Asked again while an asking was in progress — a manifest that
    /// changed mid-way — and so asked again when it ends.
    again: bool,
    go_inputs: Option<crate::watch::DependencyInputs>,
    /// What the asking in progress has answered so far. It replaces
    /// `answer` when the asking ends, **not stack by stack**: answers of
    /// one asking arrive over several looks — Cargo in half a second,
    /// Gradle in five — and replacing on each look would drop the stack
    /// that answered first.
    pending: Option<(Answer, Vec<(Stack, String)>)>,
}

impl Known {
    /// Whether a build system is still being asked.
    pub fn asking(&self) -> bool {
        self.asking.is_some()
    }

    /// What to show: the last whole answer, or — the first time, when
    /// there is none — what has arrived so far, so a first list fills in.
    pub fn shown(&self) -> &Answer {
        match &self.pending {
            Some((pending, _)) if self.answer == Answer::default() => pending,
            _ => &self.answer,
        }
    }
}

impl Dependencies {
    /// What was known of a project, dropped — a root bound as a project
    /// group's folder is asked nothing more (GLD-055 decision 12).
    pub(crate) fn forget(&mut self, project: ProjectId) {
        self.of.remove(&project);
    }

    /// What is known of a project, if it has been asked about at all.
    pub(crate) fn of(&self, project: ProjectId) -> Option<&Known> {
        self.of.get(&project)
    }
}

impl Root {
    /// Ask a project's build systems what it depends on, unless they are
    /// being asked already. What was answered before stays on screen
    /// until the new answer replaces it: a list that empties while a
    /// Gradle thinks is the blink the Log's filter learned not to do.
    pub(crate) fn ask_dependencies(&mut self, project: ProjectId) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let known = self.dependencies.of.entry(project).or_default();
        if known.asking.is_some() {
            known.again = true;
            return;
        }
        let first = known.answer == Answer::default();
        let stacks = Stack::of(&root);
        if stacks.is_empty() {
            known.answer = Answer::default();
            known.failed.clear();
            return;
        }
        let scratch = scratch_for(&root);
        let (send, receive) = mpsc::channel();
        known.asking = Some(receive);
        std::thread::Builder::new()
            .name("guild-dependencies".into())
            .spawn(move || {
                if first {
                    let cached = stacks
                        .iter()
                        .filter_map(|stack| deps::ask::cached(&root, *stack, &scratch))
                        .fold(Answer::default(), Answer::merge);
                    if send.send(Heard::Cached(cached)).is_err() {
                        return;
                    }
                }
                for stack in stacks {
                    if stack == Stack::Go {
                        let inputs =
                            crate::watch::DependencyInputs::read(deps::go_vendor::watched(&root));
                        if send.send(Heard::GoInputs(inputs)).is_err() {
                            return;
                        }
                    }
                    let answer = deps::ask::ask(&root, stack, &scratch);
                    if send.send(Heard::Asked(stack, answer)).is_err() {
                        return;
                    }
                }
            })
            .ok();
    }

    /// Take whatever the askings have answered since the last look. On
    /// the watch loop's clock.
    pub(crate) fn poll_dependencies(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        // A build system that could not be asked says so once, when its
        // asking ends — where a person is looking, not only in the log.
        let mut said: Vec<(Stack, String)> = Vec::new();
        let mut again: Vec<ProjectId> = Vec::new();
        let mut refresh_go = Vec::new();
        for (project, known) in self.dependencies.of.iter_mut() {
            let Some(receive) = &known.asking else {
                continue;
            };
            let finished = loop {
                match receive.try_recv() {
                    Ok(Heard::GoInputs(inputs)) => known.go_inputs = Some(inputs),
                    Ok(Heard::Cached(cached)) => {
                        if known.answer == Answer::default() {
                            known.answer = cached;
                            changed = true;
                        }
                    }
                    Ok(Heard::Asked(stack, result)) => {
                        let (answer, failed) = known.pending.get_or_insert_with(Default::default);
                        match result {
                            Ok(got) => *answer = std::mem::take(answer).merge(got),
                            Err(why) => {
                                tracing::warn!(?stack, %why, "a build system could not be asked");
                                failed.push((stack, why));
                            }
                        }
                        changed = true;
                    }
                    Err(TryRecvError::Empty) => break false,
                    Err(TryRecvError::Disconnected) => break true,
                }
            };
            // The whole asking replaces the old list, so a dependency
            // taken out of the build leaves it.
            if finished {
                known.asking = None;
                if std::mem::take(&mut known.again) {
                    again.push(*project);
                }
                if let Some((answer, failed)) = known.pending.take() {
                    let roots: Vec<_> = known
                        .answer
                        .dependencies
                        .iter()
                        .chain(&answer.dependencies)
                        .filter(|dep| dep.stack == Stack::Go)
                        .filter_map(|dep| dep.source.directory().map(std::path::Path::to_path_buf))
                        .collect();
                    if !roots.is_empty() {
                        refresh_go.push((*project, roots));
                    }
                    known.answer = answer;
                    said.extend(failed.iter().cloned());
                    known.failed = failed;
                }
                changed = true;
            }
        }
        // An already-open browser must not retain a former module boundary or
        // a directory listing from before vendor was replaced.
        for (project, roots) in refresh_go {
            let libraries = self.libraries.of(project);
            for (seat, state) in &mut self.trees {
                if seat.project == project {
                    state.library_trees.retain(|at, _| {
                        !libraries.get(*at).is_some_and(|root| roots.contains(root))
                    });
                    state.settle_selection();
                }
            }
        }
        for (project, known) in &mut self.dependencies.of {
            if let Some(watcher) = self.watchers.get(project)
                && let Some(inputs) = known.go_inputs.take()
            {
                watcher.track_dependencies(inputs);
            }
        }
        for project in again {
            self.ask_dependencies(project);
        }
        for (stack, why) in said {
            let text = self
                .ui()
                .build_system_not_asked(&format!("{stack:?}"), &why);
            self.warn(text);
        }
        if changed {
            cx.notify();
        }
    }
}

/// Where the asking may write — Gradle's init script, Maven's list — one
/// directory per project under Guild's own state, never in the project.
fn scratch_for(root: &std::path::Path) -> PathBuf {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned());
    crate::paths::state_dir().join("dependencies").join(name)
}

/// One row of *External Libraries*: a JDK or a library, and where
/// unfolding it leads. Worked out from the answer without touching the
/// disk, because the section is drawn on every frame.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// What the row says.
    pub label: String,
    /// The JDK icon rather than the library icon.
    pub jdk: bool,
    /// Where it opens.
    pub opens: Opens,
}

/// Where a row opens.
#[derive(Debug, Clone)]
pub(crate) enum Opens {
    /// A directory of sources, as it is.
    Dir(PathBuf),
    /// A jar whose sources, found beside it, are extracted into `root`.
    Jar { jar: PathBuf, root: PathBuf },
    /// A JDK's `src.zip`, extracted into `root`.
    Zip { zip: PathBuf, root: PathBuf },
    /// Nothing on the disk to open.
    Missing,
}

impl Entry {
    /// The library root this row is, or becomes when it is unfolded — the
    /// key it shares with a root `f12` landed in.
    pub(crate) fn root(&self) -> Option<&std::path::Path> {
        match &self.opens {
            Opens::Dir(dir) => Some(dir),
            Opens::Jar { root, .. } | Opens::Zip { root, .. } => Some(root),
            Opens::Missing => None,
        }
    }
}

impl Root {
    /// The section's rows for a project, the JDKs first.
    pub(crate) fn library_entries(&self, project: ProjectId) -> Vec<Entry> {
        let Some(known) = self.dependencies.of(project) else {
            return Vec::new();
        };
        let sources = crate::jars::sources_dir();
        let answer = known.shown();
        let jdks = answer.jdks.iter().map(|jdk| Entry {
            label: jdk.label.clone(),
            jdk: true,
            opens: Opens::Zip {
                zip: jdk.sources(),
                root: sources.join(format!(
                    "jdk-{}",
                    jdk.label.trim_matches(|c| c == '<' || c == '>' || c == ' ')
                )),
            },
        });
        let libraries = answer.dependencies.iter().map(|dependency| Entry {
            label: dependency.label.clone(),
            jdk: false,
            opens: match &dependency.source {
                deps::Source::Dir(dir) | deps::Source::Vendor { root: dir, .. } => {
                    Opens::Dir(dir.clone())
                }
                deps::Source::Jar(jar) => match crate::jars::extracted_dir(jar, &sources) {
                    Some(root) => Opens::Jar {
                        jar: jar.clone(),
                        root,
                    },
                    None => Opens::Missing,
                },
                deps::Source::Missing => Opens::Missing,
            },
        });
        jdks.chain(libraries).collect()
    }

    /// Unfold a row that has not been unfolded before: its directory as a
    /// library root, a jar's or a JDK's sources extracted first — on a
    /// thread, because a JDK's are 218 MB — and a notice where there is
    /// nothing to open.
    pub(crate) fn open_dependency(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(entry) = self.library_entries(project).into_iter().nth(index) else {
            return;
        };
        match entry.opens {
            Opens::Dir(dir) => self.show_library(project, &dir, cx),
            Opens::Jar { jar, root } => match crate::jars::sources_beside(&jar) {
                Some(sources) => self.extract_then_show(project, sources, root, cx),
                None => {
                    let text = self.ui().no_downloaded_sources(&entry.label);
                    self.inform(text);
                    cx.notify();
                }
            },
            Opens::Zip { zip, root } if zip.is_file() => {
                self.extract_then_show(project, zip, root, cx);
            }
            Opens::Zip { .. } => {
                let text = self.ui().no_downloaded_sources(&entry.label);
                self.inform(text);
                cx.notify();
            }
            Opens::Missing => {
                let text = self.ui().dependency_not_on_disk(&entry.label);
                self.inform(text);
                cx.notify();
            }
        }
    }

    /// Ask again when the disk says a manifest or a lockfile changed —
    /// `build.gradle.kts` edited, `cargo update` run in a terminal, a
    /// `pnpm install` finished. Task 4. Only a project that has been
    /// asked before: the first asking is the tree's to start.
    pub(crate) fn dependencies_heard(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        if !self.dependencies.of.contains_key(&project) || self.desk.is_project_group_root(project)
        {
            return;
        }
        if batch.dependencies() {
            self.ask_dependencies(project);
        }
    }

    /// Construct a library browser through the source's ownership rules.
    pub(crate) fn library_source(
        &self,
        project: ProjectId,
        root: &std::path::Path,
    ) -> deps::Source {
        self.dependencies
            .of(project)
            .and_then(|known| {
                known
                    .shown()
                    .dependencies
                    .iter()
                    .find(|dep| dep.source.directory() == Some(root))
                    .map(|dep| dep.source.clone())
            })
            .unwrap_or_else(|| deps::Source::Dir(root.to_path_buf()))
    }

    /// What is known of a project's dependencies, for the section.
    pub(crate) fn dependencies_of(&self, project: ProjectId) -> Option<&Known> {
        self.dependencies.of(project)
    }

    /// Whether a row's archive is being extracted now — drawn as a row
    /// that is busy rather than one that ignored the click.
    pub(crate) fn unpacking(&self, root: &std::path::Path) -> bool {
        self.dependencies.unpacking.contains(root)
    }

    fn extract_then_show(
        &mut self,
        project: ProjectId,
        archive: PathBuf,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if !self.dependencies.unpacking.insert(root.clone()) {
            return;
        }
        cx.notify();
        let work = cx.background_spawn({
            let root = root.clone();
            async move { crate::jars::extract_all(&archive, &root) }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.dependencies.unpacking.remove(&root);
                match result {
                    Ok(dir) => this.show_library(project, &dir, cx),
                    Err(why) => {
                        tracing::warn!(root = %root.display(), %why, "sources were not extracted");
                        this.fail(why);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// A directory as one of the project's library roots, unfolded — the
    /// same root, and the same index, as `f12` gives it.
    fn show_library(&mut self, project: ProjectId, dir: &std::path::Path, cx: &mut Context<Self>) {
        let crate::Origin::External(at) = self.note_library(project, dir) else {
            return;
        };
        let source = self.library_source(project, dir);
        let seat = self.seat_for_project(project);
        if let Some(state) = self.trees.get_mut(&seat) {
            state.libraries_open = true;
            state
                .library_trees
                .entry(at)
                .or_insert_with(|| source.open_tree().expect("directory source"));
        }
        cx.notify();
    }
}
