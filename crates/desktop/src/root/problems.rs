//! What the servers say is wrong, as a panel reads it. M8.8.5.
//!
//! **The data was already there.** `Root.diagnostics` is a map from
//! `(project, path)` to what a server published, kept for every file a
//! server has an opinion about and not only the open ones — LSP has no
//! way to ask again, so a tab opened a second later would otherwise show
//! nothing until the next edit. The project-wide list is therefore a
//! read of that map; the only thing that was missing was somewhere to
//! draw it, and decision 14 makes that a tool rather than a popup.
//!
//! Everything above `Problems` itself is pure — problems in, rows out —
//! and is checked in `tests/all/problems.rs` without a window and
//! without a server.
//!
//! **The tree is a third one, and it is not `files_tree`.** The plan
//! said to reuse that module; what it can be reused for is its *rule*,
//! not its rows. `files_tree::Row::File` carries git's `Mark` and a
//! rename's old path, and a Problems row that filled those in would be
//! carrying two lies to borrow a walk. `search_tree` is the same shape
//! as this one and says so in its own doc for the same reason: it folds,
//! and it has a third kind of row under a file. The collapsing rule is
//! the one rule all three share — `crates/desktop/tests/all` is one row
//! and not four — and it is thirty lines. Folding the three into one
//! parameterised walk is a pass of its own, over three files two other
//! branches are reading; it is named in the plan rather than done here.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use crate::Root;

/// How many problems the panel will hold.
///
/// **A cap that a real project has never reached.** Task 0 tried to
/// measure the number and got `files=1 entries=0` over a tree that
/// compiles, which is the useless case; M8.8.5 measured its own window
/// and the plan records what it found. The tool takes a cap anyway, on
/// M8.7's reasoning about five thousand search hits: the cost of a cap
/// that is never reached is a constant nobody sees, and the cost of not
/// having one is a frame that never ends — the tree is rebuilt at every
/// render, and a `BTreeMap` insert per problem is cheap exactly until it
/// is not.
pub const CAP: usize = 2_000;

/// How bad a problem is.
///
/// The protocol's four, in the order the design system's section 6.21
/// paints them: `accent.rust`, `accent.brass`, `accent.patina`,
/// `text.faint`. **Declaration order is severity order**, so the worst
/// of a set is its `min`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    /// What the server said, or what to make of its saying nothing.
    ///
    /// **A missing severity is information.** The protocol allows the
    /// field to be absent and says the client decides; deciding *error*
    /// would invent a mistake out of a missing field, and deciding
    /// *hint* would hide one. Information is the reading that claims
    /// least.
    pub fn of(said: Option<lsp_types::DiagnosticSeverity>) -> Self {
        match said {
            Some(lsp_types::DiagnosticSeverity::ERROR) => Self::Error,
            Some(lsp_types::DiagnosticSeverity::WARNING) => Self::Warning,
            Some(lsp_types::DiagnosticSeverity::HINT) => Self::Hint,
            _ => Self::Info,
        }
    }
}

/// One thing a server says is wrong, flattened out of the map.
///
/// The order of the fields **is** the order the list is sorted in: by
/// file, then down the file. A `HashMap` of servers' answers has no
/// order at all, and a panel whose rows move between frames is a panel
/// nobody can click.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Problem {
    /// Where it is, within its project.
    pub path: PathBuf,
    /// The line, 0-based as the protocol gives it.
    pub line: u32,
    /// The character within that line, 0-based.
    pub character: u32,
    pub severity: Severity,
    /// The server's own words.
    pub message: String,
    /// Which server, when it names itself — *rustc*, *clippy*.
    pub source: Option<String>,
}

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A directory, or a run of them collapsed into one.
    Directory {
        /// The segments **this** row adds, not the whole path. A tree
        /// that repeats its parents on every line is a tree nobody reads
        /// down the left edge, which is the only thing the indentation
        /// is for.
        label: String,
        /// The whole path, which is what folds it.
        path: PathBuf,
        /// How many problems are beneath it, however deep.
        problems: usize,
        /// The worst of them: a count alone cannot tell six hints from
        /// six errors.
        worst: Severity,
        depth: usize,
    },
    /// A file something is wrong with.
    File {
        /// Its own name, the directories being on the row above.
        name: String,
        /// Its path in the project, which opens it and folds it.
        path: PathBuf,
        problems: usize,
        worst: Severity,
        depth: usize,
    },
    /// One problem, under the file it is in.
    Problem {
        /// Which problem, by its place in the settled list.
        ///
        /// **An index rather than the problem itself**: the list is
        /// replaced whole whenever a server publishes, and a row that
        /// borrowed one would tie its lifetime to a list that has gone.
        at: usize,
        depth: usize,
    },
}

/// What folding this row would fold, if it can be folded at all.
///
/// A problem does not fold: it is the thing the folding is about.
pub fn foldable(row: &Row) -> Option<PathBuf> {
    match row {
        Row::Directory { path, .. } | Row::File { path, .. } => Some(path.clone()),
        Row::Problem { .. } => None,
    }
}

/// Put the problems into reading order and stop at the cap.
///
/// Answers whether the cap bit, because a count with no note is a lie
/// about the project — the rule the search panel's *and more* keeps.
pub fn settle(problems: Vec<Problem>) -> (Vec<Problem>, bool) {
    let mut problems = problems;
    problems.sort_unstable();
    let capped = problems.len() > CAP;
    problems.truncate(CAP);
    (problems, capped)
}

/// How many errors and how many warnings, for the button in the status
/// bar.
///
/// **Information and hints are worth a row and not a number.**
/// rust-analyzer publishes an unused import as a hint; a bar saying
/// *84* over a project that compiles would be counting those and calling
/// them problems.
pub fn counts(problems: &[Problem]) -> (usize, usize) {
    let count = |wanted: Severity| problems.iter().filter(|one| one.severity == wanted).count();
    (count(Severity::Error), count(Severity::Warning))
}

/// Why the panel is empty, when it is.
///
/// **Three states, because *nothing is wrong* and *nothing has looked*
/// are two different claims** and only one of them is about the code. A
/// panel that said the first while the second was true would be telling
/// a person their project is clean before anything had read it — and on
/// a cold start those two are two minutes apart, which is what M8.5
/// measured from a first `initialize` to the first diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// No server of this project has been started at all: nothing has
    /// looked. A language with no server in the table lands here too,
    /// which is the same fact from the other side.
    NobodyLooked,
    /// One is starting, indexing or running `cargo check`.
    Looking,
    /// They are listening and quiet, and there is nothing to say.
    Clean,
}

/// Which emptiness a project is in, from how many servers it has and how
/// many of them are still busy.
///
/// Pure, and takes two numbers rather than the map: what the window has
/// to do is count, and what is worth a test is the reading of the count.
pub fn quiet(servers: usize, busy: usize) -> Quiet {
    match (servers, busy) {
        (0, _) => Quiet::NobodyLooked,
        (_, 0) => Quiet::Clean,
        _ => Quiet::Looking,
    }
}

/// What each emptiness says.
///
/// Here rather than in the view, so that a test can check the three are
/// three: the state is only worth having if the words differ.
pub fn label_of(quiet: Quiet) -> crate::i18n::Text {
    match quiet {
        Quiet::NobodyLooked => crate::i18n::Text::ProblemsNoServer,
        Quiet::Looking => crate::i18n::Text::ProblemsLooking,
        Quiet::Clean => crate::i18n::Text::PanelProblemsEmpty,
    }
}

/// The rows, in the order they are drawn.
///
/// `folded` holds the paths of directories and files that are shut — a
/// directory takes everything under it, and a file keeps its own row and
/// loses its problems, so there is always something left to press.
pub fn of(problems: &[Problem], folded: &HashSet<PathBuf>) -> Vec<Row> {
    let mut root = Node::default();
    for (at, problem) in problems.iter().enumerate() {
        root.add(&problem.path, at, problem.severity);
    }
    let mut rows = Vec::new();
    root.walk(PathBuf::new(), String::new(), 0, folded, &mut rows);
    rows
}

/// One directory of the tree while it is being built.
#[derive(Default)]
struct Node {
    directories: BTreeMap<String, Node>,
    /// Each file's problems, by name, in the order the list has them —
    /// which is reading order, because `settle` put it there.
    files: BTreeMap<String, Vec<(usize, Severity)>>,
}

impl Node {
    fn add(&mut self, path: &std::path::Path, at: usize, severity: Severity) {
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
        node.files.entry(name).or_default().push((at, severity));
    }

    /// Every problem beneath, however deep.
    fn count(&self) -> usize {
        self.files.values().map(Vec::len).sum::<usize>()
            + self.directories.values().map(Node::count).sum::<usize>()
    }

    /// The worst of them. `Severity`'s order is severity order, so the
    /// worst is the smallest; nothing beneath at all is a hint, which
    /// cannot happen — a node exists because something is under it.
    fn worst(&self) -> Severity {
        let files = self
            .files
            .values()
            .flat_map(|one| one.iter().map(|(_, severity)| *severity));
        let directories = self.directories.values().map(Node::worst);
        files.chain(directories).min().unwrap_or(Severity::Hint)
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
        if !prefix.as_os_str().is_empty() {
            rows.push(Row::Directory {
                label,
                path: prefix.clone(),
                problems: self.count(),
                worst: self.worst(),
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
        for (name, found) in &self.files {
            let path = prefix.join(name);
            rows.push(Row::File {
                name: name.clone(),
                path: path.clone(),
                problems: found.len(),
                worst: found
                    .iter()
                    .map(|(_, severity)| *severity)
                    .min()
                    .unwrap_or(Severity::Hint),
                depth,
            });
            if folded.contains(&path) {
                continue;
            }
            rows.extend(found.iter().map(|(at, _)| Row::Problem {
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

/// What the language servers say is wrong, as the Problems panel reads
/// it.
///
/// The list itself is read out of `Root.diagnostics` at each render, so
/// what this holds is only what the panel adds to it: what is folded,
/// and where it is scrolled to.
///
/// **Folding is keyed by the project as well as by the path.** The paths
/// in the map are project-relative, so `src/lib.rs` shut in one project
/// would otherwise be shut in every other project open in the window.
#[derive(Debug, Default)]
pub(crate) struct Problems {
    /// The directories and files shut, by the project they are in.
    folded: HashSet<(guild_session::ProjectId, PathBuf)>,
    /// Where the list is, so that it does not go back to the top every
    /// time a server publishes.
    scroll: gpui::UniformListScrollHandle,
}

impl Root {
    /// Every problem in the active project, in reading order, and
    /// whether the cap stopped the list.
    ///
    /// Read out of the map at each render rather than kept in step with
    /// it: a server publishes a whole file's worth at a time, and a
    /// window holding two copies of one fact is a window where they
    /// disagree.
    pub(crate) fn problems(&self) -> (Vec<Problem>, bool) {
        let Some(project) = self.active_project else {
            return (Vec::new(), false);
        };
        let found = self
            .diagnostics
            .iter()
            .filter(|((of, _), _)| *of == project)
            .flat_map(|((_, path), said)| {
                said.iter().map(|one| Problem {
                    path: path.clone(),
                    line: one.range.start.line,
                    character: one.range.start.character,
                    severity: Severity::of(one.severity),
                    message: one.message.clone(),
                    source: one.source.clone(),
                })
            })
            .collect();
        settle(found)
    }

    /// The rows the panel draws, from the problems and what is shut.
    ///
    /// Rebuilt at each render rather than kept, for the reason the
    /// search results are: a server publishes a whole file's worth at a
    /// time, and a cached tree would be one more thing to keep in step
    /// with the list it came from.
    pub(crate) fn problem_rows(&self, problems: &[Problem]) -> Vec<Row> {
        of(problems, &self.problems_folded())
    }

    /// What is shut in the active project. The panel needs it twice —
    /// once to build the rows and once to point every chevron the right
    /// way — and a chevron drawn from a second opinion is a chevron that
    /// disagrees with the row under it.
    pub(crate) fn problems_folded(&self) -> HashSet<PathBuf> {
        let Some(project) = self.active_project else {
            return HashSet::new();
        };
        self.problems
            .folded
            .iter()
            .filter(|(of, _)| *of == project)
            .map(|(_, path)| path.clone())
            .collect()
    }

    /// Which emptiness the panel is in, for the note it draws instead of
    /// rows.
    ///
    /// **Counted over the active project's servers**, not over the
    /// window's: a second project indexing in another tab says nothing
    /// about this one.
    pub(crate) fn problems_quiet(&self) -> Quiet {
        let Some(project) = self.active_project else {
            return Quiet::NobodyLooked;
        };
        let mine = self.servers.iter().filter(|((of, _), _)| *of == project);
        let (mut servers, mut busy) = (0, 0);
        for (_, server) in mine {
            servers += 1;
            if !server.ready || !server.work.is_idle() {
                busy += 1;
            }
        }
        quiet(servers, busy)
    }

    /// The handle the list is scrolled by.
    pub(crate) fn problems_scroll(&self) -> gpui::UniformListScrollHandle {
        self.problems.scroll.clone()
    }

    /// Fold or unfold whatever this row is about. A problem does not
    /// fold — it is the thing the folding is about — so a click on one
    /// opens its file instead.
    pub(crate) fn toggle_problems_fold(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let key = (project, path);
        if !self.problems.folded.remove(&key) {
            self.problems.folded.insert(key);
        }
        cx.notify();
    }

    /// Open a problem: its file, at the place the server named.
    ///
    /// **`land_on`'s route**, which is `f12`'s — open the file, record
    /// the landing, put the caret there — because a row here is a place
    /// a server named, exactly as a definition is.
    pub(crate) fn open_problem(
        &mut self,
        at: usize,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let (problems, _) = self.problems();
        let Some(problem) = problems.get(at) else {
            return;
        };
        let (path, place) = (
            problem.path.clone(),
            lsp_types::Position::new(problem.line, problem.character),
        );
        self.land_at(project, path, place, window, cx);
    }

    /// Forget a project's problems when the project closes.
    ///
    /// **The map goes too, not only the folding.** Stopping the servers
    /// stops anything new arriving and leaves everything that already
    /// has: a window opened and closed on a few repositories all day
    /// would keep every diagnostic any of them ever published, for a
    /// panel that filters them out by project id and never draws one.
    pub(crate) fn forget_problems(&mut self, project: guild_session::ProjectId) {
        self.problems.folded.retain(|(of, _)| *of != project);
        self.diagnostics.retain(|(of, _), _| *of != project);
    }
}
