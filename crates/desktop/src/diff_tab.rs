//! A diff in a tab: what is behind `Body::Diff`.
//!
//! One file's changes against `HEAD`, or every changed file's — decision
//! 3 of the M8.2 plan. The hunks are `crate::diff`'s; what is here is
//! the fetching of `HEAD`'s text on a thread, the working text taken
//! from an open file tab's buffer when there is one and from the disk
//! otherwise, and the rows the list draws. Nothing here runs git on the
//! window's thread, and nothing here writes.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

mod authors;
mod compute;
mod hunks;
pub(crate) use hunks::{
    converted as hunk_converted, hint as hunk_hint, present as hunk_present,
    view_of as hunk_view_of,
};
mod interaction;
use crate::diff::document::{DisplayRow, Document};
pub(crate) use authors::*;
pub(crate) use interaction::*;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription,
    UniformListScrollHandle,
};
use gpui_component::input::{EditorState, InputEvent};

use crate::diff::{self, DIFF_LIMIT, Hunk, Pair, Row};
use crate::file_tab::Look;
use crate::git::status::{Entry, Kind, StatusMap};
use guild_session::{Base, DiffView};

use crate::git::{self, GitError};
use crate::highlight::{Language, Text};
use crate::i18n::Ui;
use crate::syntax::{self, Span};

/// What a file's diff turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// `HEAD`'s text has not arrived yet.
    Loading,
    /// Changed: the hunks say how.
    Hunks,
    /// The working tree is what `HEAD` has.
    NoChanges,
    /// `HEAD` has no such file: everything is new.
    Added,
    /// The working tree has no such file: everything is old.
    Deleted,
    /// One side is not text.
    Binary,
    /// Past [`DIFF_LIMIT`] on one side.
    TooLarge,
    /// The project is not in a repository.
    NoRepository,
    /// No `git` on this machine.
    NoGit,
    /// Git refused, in its words.
    Failed(String),
}

/// One file's diff.
#[derive(Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// Relative to the project.
    pub path: PathBuf,
    pub state: State,
    pub hunks: Vec<Hunk>,
    /// Lines added and removed.
    pub counts: (usize, usize),
}

#[cfg(test)]
thread_local! { static FILE_DIFF_CLONES: Cell<usize> = const { Cell::new(0) }; }

impl Clone for FileDiff {
    fn clone(&self) -> Self {
        #[cfg(test)]
        FILE_DIFF_CLONES.with(|count| count.set(count.get() + 1));
        Self {
            path: self.path.clone(),
            state: self.state.clone(),
            hunks: self.hunks.clone(),
            counts: self.counts,
        }
    }
}

impl FileDiff {
    fn loading(path: PathBuf) -> Self {
        Self {
            path,
            state: State::Loading,
            hunks: Vec::new(),
            counts: (0, 0),
        }
    }

    /// The diff from what `HEAD` has and what the working tree has —
    /// pure, so that every state is testable without git or a disk.
    /// `head` is `None` when `HEAD` has no such file, `working` when the
    /// working tree has none.
    pub fn compute(path: PathBuf, head: Option<&[u8]>, working: Option<&[u8]>) -> Self {
        #[cfg(test)]
        diff::note_computation(0);
        let too_large = |bytes: &[u8]| bytes.len() as u64 > DIFF_LIMIT;
        let text = |bytes: &[u8]| match Text::of(bytes) {
            Text::Source { text, .. } => Some(text),
            Text::Binary => None,
        };
        let binary = |bytes: &[u8]| text(bytes).is_none();
        let (state, hunks) = match (head, working) {
            (None, None) => (State::NoChanges, Vec::new()),
            (h, w) if h.is_some_and(too_large) || w.is_some_and(too_large) => {
                (State::TooLarge, Vec::new())
            }
            (h, w) if h.is_some_and(binary) || w.is_some_and(binary) => (State::Binary, Vec::new()),
            (None, Some(w)) => (State::Added, diff::hunks("", &text(w).unwrap_or_default())),
            (Some(h), None) => (
                State::Deleted,
                diff::hunks(&text(h).unwrap_or_default(), ""),
            ),
            (Some(h), Some(w)) => {
                let hunks = diff::hunks(&text(h).unwrap_or_default(), &text(w).unwrap_or_default());
                if hunks.is_empty() {
                    (State::NoChanges, hunks)
                } else {
                    (State::Hunks, hunks)
                }
            }
        };
        let counts = diff::counts(&hunks);
        Self {
            path,
            state,
            hunks,
            counts,
        }
    }

    /// A diff that cannot be had, with the reason as its state.
    fn failed(path: PathBuf, err: &GitError) -> Self {
        Self {
            path,
            state: match err {
                GitError::NoGit => State::NoGit,
                GitError::NoRepository => State::NoRepository,
                GitError::Failed(why) => State::Failed(why.clone()),
            },
            hunks: Vec::new(),
            counts: (0, 0),
        }
    }
}

/// One side of a diff, painted: the text the spans were taken from, and
/// the spans of each of its lines.
///
/// **The text is kept on purpose.** The rows of a diff come from `git diff`
/// and the spans from parsing the blobs, and the two can disagree — over
/// CRLF, over a rev that moved under an open tab, over an off-by-one
/// anywhere. So a row is coloured only when it *is* the line the spans were
/// taken from, which costs one string compare per drawn row and turns every
/// misalignment into plain text. The rule earns its keep twice here: it is
/// also what makes spans that arrive late safe, because a buffer that has
/// moved ahead of them simply draws plain until they catch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidePaint {
    /// The text the spans came from.
    text: String,
    /// Where each of its lines is in it.
    lines: Vec<Range<usize>>,
    /// The spans of each line, in that line's own offsets.
    spans: Vec<Vec<Span>>,
}

impl SidePaint {
    /// This text, parsed with this grammar.
    fn of(text: &str, grammar: &str) -> Self {
        Self {
            text: text.to_owned(),
            lines: syntax::line_ranges(text),
            spans: syntax::spans(text, grammar),
        }
    }

    /// The spans of line `number` — one-based, as a diff numbers them —
    /// **and only when `row` is that line**.
    pub fn of_line(&self, number: usize, row: &str) -> Option<&[Span]> {
        let index = number.checked_sub(1)?;
        let at = self.lines.get(index)?;
        (self.text.get(at.clone())? == row).then(|| self.spans[index].as_slice())
    }
}

/// Both sides of a file's diff, painted — or nothing at all.
///
/// **All or nothing across the two sides**, which is the right rule: one
/// coloured column beside a plain one reads as *the other side is not code*,
/// and that is a worse answer than two plain columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Painted {
    /// What `HEAD`, the index or a commit has.
    pub old: SidePaint,
    /// What the working tree or the newer side has.
    pub new: SidePaint,
}

impl Painted {
    /// The two sides painted, or `None` when nothing claims this path or
    /// its grammar has no query to run.
    pub fn of(path: &Path, old: &str, new: &str) -> Option<Self> {
        // **The tab's limit, not one of its own.** A file coloured in one
        // place and plain in another is the thing this pass exists to
        // stop; the number that decides is the same number.
        let limit = crate::file_tab::HIGHLIGHT_LIMIT;
        if old.len() as u64 > limit || new.len() as u64 > limit {
            return None;
        }
        let language = Language::of(path)?;
        let grammar = crate::grammars::of(language.grammar)?;
        if grammar.highlights.is_empty() {
            return None;
        }
        Some(Self {
            old: SidePaint::of(old, language.grammar),
            new: SidePaint::of(new, language.grammar),
        })
    }
}

/// One row of the list: the working tree's summary and its list of
/// files at the top, then a file's header, one of its hunks' rows, or
/// the one line a file with no hunks shows. `file` is the index into the
/// tab's files, for the jump from a row to its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A row backed by retained immutable source ranges.
    Source {
        file: usize,
        row: DisplayRow,
    },
    /// *N files · +a −r*, the working tree's tab only.
    Summary,
    /// One changed file in the list at the top — a click scrolls to its
    /// header.
    Listed {
        file: usize,
    },
    File {
        file: usize,
    },
    Hunk {
        file: usize,
        row: Row,
    },
    /// One row of the side-by-side view: the two halves. Decision 11.
    Pair {
        file: usize,
        pair: Pair,
    },
    Note {
        file: usize,
    },
}

/// The files the working tree's tab lists, from the status map: the
/// tracked changes in path order, then the untracked, never the ignored.
pub fn tree_files(map: &StatusMap) -> Vec<Entry> {
    let mut tracked: Vec<Entry> = Vec::new();
    let mut untracked: Vec<Entry> = Vec::new();
    for entry in map.entries() {
        match entry.kind {
            Kind::Ignored => {}
            Kind::Untracked if entry.is_dir => {}
            Kind::Untracked => untracked.push(entry.clone()),
            _ => tracked.push(entry.clone()),
        }
    }
    tracked.extend(untracked);
    tracked
}

/// Lines added and removed over every file.
pub fn totals(files: &[FileDiff]) -> (usize, usize) {
    files
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.counts.0, r + f.counts.1))
}

/// Where a file's header row is among the lines, for the jump from the
/// list at the top.
pub fn header_index(lines: &[Line], file: usize) -> Option<usize> {
    lines.iter().position(|line| *line == Line::File { file })
}

/// What the tab asks the window to do — the gestures that leave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEvent {
    /// Open the file at this one-based line.
    OpenAt { path: PathBuf, line: usize },
    /// The working tree's list wants its working texts read again: the
    /// files changed, and the window is what knows which are open.
    NeedsWorking { paths: Vec<PathBuf> },
    /// This proposal is accepted: write it. M8.6.
    ///
    /// An event rather than a call, for the reason `OpenAt` is one: the
    /// tab knows the two texts and nothing else — which agent is waiting
    /// for the answer, and what writing the file means, are the window's.
    Accept,
    /// This proposal is refused. The tab closes and the agent is told.
    Reject,
    /// The diff window's ← or →: the commit's previous or next file.
    /// M8.3.6.
    StepFile { forward: bool },
    /// Select an absolute file in the current immutable inspection order.
    SelectFile { index: usize, key: String },
    /// Stage or unstage one shown block: the window sends the write to the
    /// project's git thread. GLD-026.
    Hunk(crate::git::Write),
    /// A hunk action on a diff whose working text has moved on since it
    /// was computed: nothing is written.
    HunkStale,
}

/// One of a diff's two texts: a blob git is asked for by its spec, or
/// the working tree's text as the window sees it — an open tab's buffer,
/// or the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Blob(String),
    Working,
}

/// The two texts a base names for a path, old then new — decision 5 of
/// M8.3. `Head` and `Unstaged` end at the working tree; `Index` and a
/// commit end at a blob, so what they show does not follow an editor.
///
/// **Two spellings of a path, because the paths are two people's.** The
/// working tree's bases are given the path the tree drew, relative to the
/// project, and write `HEAD:./path` and `:./path` — git resolves `./` from
/// the directory it runs in, so a project that is a folder of its
/// repository reads its own file and not the root's of the same name. A
/// commit's paths come from `git show --name-status`, relative to the
/// repository wherever it runs, and keep `sha:path`. Decision 18 of M5.11.
pub fn sides(base: &Base, path: &Path) -> (Side, Side) {
    let here = |revision: &str| Side::Blob(format!("{revision}:./{}", path.display()));
    let spec = |revision: &str| Side::Blob(format!("{revision}:{}", path.display()));
    match base {
        Base::Head => (here("HEAD"), Side::Working),
        Base::Index => (here("HEAD"), here("")),
        Base::Unstaged => (here(""), Side::Working),
        Base::Commit(sha) => (spec(&format!("{sha}^")), spec(sha)),
        // Neither side is git's: both texts are held by the window, and
        // `open_proposal` puts them there before anything is fetched.
        Base::Proposal => (Side::Working, Side::Working),
    }
}

/// Which of the two texts a thread answered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Old,
    New,
}

/// A thread's answer for one side of one path.
type BlobAnswer = (
    Which,
    PathBuf,
    Result<Option<Vec<u8>>, GitError>,
    Option<String>,
    u64,
);

/// A followed buffer, serialized as a save would write it.
type Serialize = std::rc::Rc<dyn Fn(&gpui::App) -> Vec<u8>>;

/// A diff in a tab.
pub struct DiffTab {
    root: PathBuf,
    /// `None` is the working tree — every changed file.
    target: Option<PathBuf>,
    /// What the two texts are. The working tree's list is always `Head`.
    base: Base,
    /// Selected immutable commits in a Push inspector; never persisted.
    inspection: Option<String>,
    /// Two columns or one — the setting when the tab opened, and the
    /// head's button since.
    mode: DiffView,
    files: Vec<FileDiff>,
    lines: Vec<Line>,
    /// The old text of each path, once git has answered.
    old: HashMap<PathBuf, Result<Option<Vec<u8>>, GitError>>,
    /// The new text of each path when the base ends at a blob, once git
    /// has answered; unused when it ends at the working tree.
    fetched: HashMap<PathBuf, Result<Option<Vec<u8>>, GitError>>,
    /// The working tree's text for each path, as last seen — an open
    /// tab's buffer, or the disk.
    working: HashMap<PathBuf, Option<Vec<u8>>>,
    /// Each path's working text by revision: a read started for one is put
    /// in only while it is still the newest — review r4 `bugs+impl-7`.
    working_revision: HashMap<PathBuf, u64>,
    /// Where the revisions come from; never reused, so a path cleared and
    /// listed again cannot match a read from before.
    next_working_revision: u64,
    answers: mpsc::Receiver<BlobAnswer>,
    fetch_generation: Cell<u64>,
    expected_fetch: RefCell<HashMap<PathBuf, u64>>,
    asks: mpsc::Sender<BlobAnswer>,
    /// Each path's two sides painted, once the painting thread has
    /// answered. A path with none is drawn plain, which is also what a
    /// path whose text has moved ahead of its spans draws — the row
    /// guard in [`SidePaint::of_line`] takes care of that.
    paints: HashMap<PathBuf, Arc<Painted>>,
    computation: compute::Owner,
    inputs: HashMap<PathBuf, compute::Input>,
    retained: HashMap<PathBuf, (compute::Input, compute::ResultPair)>,
    tickets: HashMap<PathBuf, u64>,
    serial: u64,
    _compute_wake: gpui::Task<()>,
    _compute_quit: gpui::Subscription,
    /// The editors followed, so that the diff is of what is on screen.
    followed: HashMap<PathBuf, (gpui::EntityId, Subscription)>,
    look: Look,
    ui: Ui,
    focus: FocusHandle,
    pub(crate) scroll: UniformListScrollHandle,
    /// A row's height in pixels at the last render, for the row at the
    /// top of the view: the list's handle keeps an offset, not an index.
    pub(crate) row_px: f32,
    /// Which of a commit's files this is and of how many, when the diff
    /// is shown in its own window and steps through them — M8.3.6.
    stepping: Option<(usize, usize)>,
    pub(crate) inspection_paths: Vec<PathBuf>,
    pub(crate) file_picker: Option<FilePicker>,
    pub(crate) viewer: Viewer,
    pub(crate) authors: Authors,
    pub(crate) authors_menu: Option<(usize, gpui::Point<gpui::Pixels>, FocusHandle)>,
    pub(crate) documents: HashMap<PathBuf, Arc<Document>>,
    pub(crate) expanded: HashMap<PathBuf, HashSet<usize>>,
    pub(crate) revisions: HashMap<PathBuf, [Option<String>; 2]>,
    /// What each shown block offers to stage or unstage — GLD-026.
    pub(crate) hunks: hunks::Hunks,
    /// A followed buffer, serialized as a save would write it: what a hunk
    /// click checks the shown working text against.
    buffers: HashMap<PathBuf, Serialize>,
}

impl EventEmitter<DiffEvent> for DiffTab {}

impl DiffTab {
    /// A tab for one file's diff, with the working text the window has
    /// for it — an open tab's buffer, or what it read from the disk;
    /// `None` when the file is not on the disk.
    // Eight things a diff is made of; a struct for them would be this
    // list with a name, and the name would be `DiffTab`.
    #[allow(clippy::too_many_arguments)]
    pub fn open_file(
        root: &Path,
        path: PathBuf,
        base: Base,
        mode: DiffView,
        working: Option<Vec<u8>>,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let inspection = match &base {
            Base::Commit(key) if key.contains(',') => Some(key.clone()),
            _ => None,
        };
        Self::open_file_for(root, path, base, inspection, mode, working, look, ui, cx)
    }

    /// A Push preview or satellite comparing the selected commits' blobs.
    /// The persisted `Base` remains one commit, while the inspector keeps
    /// its immutable selection separately.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_inspection(
        root: &Path,
        path: PathBuf,
        key: String,
        mode: DiffView,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let base = Base::Commit(key.split(',').next().unwrap_or_default().to_owned());
        Self::open_file_for(root, path, base, Some(key), mode, None, look, ui, cx)
    }

    /// Reuse the surface/focus/scroll owners across immutable-file transitions.
    /// Clear old bytes before exposing the new identity; late answers are rejected
    /// by the monotonically increasing fetch generation, including A -> B -> A.
    pub(crate) fn show_inspection(
        &mut self,
        root: &Path,
        path: PathBuf,
        key: String,
        cx: &mut Context<Self>,
    ) {
        if self.root == root
            && self.target.as_ref() == Some(&path)
            && self.inspection.as_ref() == Some(&key)
        {
            return;
        }
        if self.file_picker.take().is_some() {
            self.viewer.focus_requested = true;
        }
        self.root = root.to_path_buf();
        self.base = Base::Commit(key.split(',').next().unwrap_or_default().to_owned());
        self.inspection = Some(key);
        self.target = Some(path.clone());
        self.authors.entries.clear();
        self.authors.ranks.clear();
        if self.authors_menu.take().is_some() {
            self.viewer.focus_requested = true;
        }
        self.expected_fetch.borrow_mut().clear();
        self.old.clear();
        self.fetched.clear();
        self.working.clear();
        self.followed.clear();
        self.documents.clear();
        self.revisions.clear();
        self.paints.clear();
        self.inputs.clear();
        self.retained.clear();
        self.tickets.clear();
        self.computation.retain(&HashSet::new());
        self.expanded.clear();
        self.viewer.invalidate();
        self.viewer.active_file = 0;
        self.viewer.file_anchor = Some(0);
        self.viewer.resizing = false;
        self.viewer.pointer = None;
        self.files = vec![FileDiff::loading(path.clone())];
        self.scroll
            .scroll_to_item_strict(0, gpui::ScrollStrategy::Top);
        self.fetch(vec![path]);
        self.rebuild();
        cx.notify();
    }

    pub(crate) fn inspection_key(&self) -> Option<&str> {
        self.inspection.as_deref()
    }

    pub(crate) fn set_inspection_files(
        &mut self,
        paths: Vec<PathBuf>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.inspection_paths = paths;
        self.set_stepping(Some((index, self.inspection_paths.len())), cx);
    }

    #[allow(clippy::too_many_arguments)]
    fn open_file_for(
        root: &Path,
        path: PathBuf,
        base: Base,
        inspection: Option<String>,
        mode: DiffView,
        working: Option<Vec<u8>>,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let (asks, answers) = mpsc::channel();
        let (computation, compute_wake, compute_quit) = Self::computation(cx);
        let mut tab = Self {
            root: root.to_path_buf(),
            target: Some(path.clone()),
            base,
            inspection,
            mode,
            files: vec![FileDiff::loading(path.clone())],
            lines: Vec::new(),
            old: HashMap::new(),
            fetched: HashMap::new(),
            working: HashMap::new(),
            working_revision: HashMap::new(),
            next_working_revision: 0,
            answers,
            fetch_generation: Cell::new(0),
            expected_fetch: RefCell::new(HashMap::new()),
            asks,
            paints: HashMap::new(),
            computation,
            inputs: HashMap::new(),
            retained: HashMap::new(),
            tickets: HashMap::new(),
            serial: 0,
            _compute_wake: compute_wake,
            _compute_quit: compute_quit,
            followed: HashMap::new(),
            look,
            ui,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            row_px: 26.0,
            stepping: None,
            inspection_paths: Vec::new(),
            file_picker: None,
            viewer: Viewer::default(),
            authors: Authors::default(),
            authors_menu: None,
            documents: HashMap::new(),
            hunks: Default::default(),
            buffers: HashMap::new(),
            expanded: HashMap::new(),
            revisions: HashMap::new(),
        };
        tab.working.insert(path.clone(), working);
        tab.fetch(vec![path]);
        tab.rebuild();
        tab
    }

    /// A tab for **what an agent proposes**: the file as it is against
    /// the text `claude` sent over the IDE socket. M8.6.
    ///
    /// Neither side comes from git, so nothing is fetched: both texts are
    /// put in before the first build, and the tab never asks anybody for
    /// anything. That is also why it must never be written to the
    /// project's state — the new side exists only in this window.
    #[allow(clippy::too_many_arguments)]
    pub fn open_proposal(
        root: &Path,
        path: PathBuf,
        old: Option<Vec<u8>>,
        proposed: Vec<u8>,
        mode: DiffView,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let (asks, answers) = mpsc::channel();
        let (computation, compute_wake, compute_quit) = Self::computation(cx);
        let mut tab = Self {
            root: root.to_path_buf(),
            target: Some(path.clone()),
            base: Base::Proposal,
            inspection: None,
            mode,
            files: vec![FileDiff::loading(path.clone())],
            lines: Vec::new(),
            old: HashMap::new(),
            fetched: HashMap::new(),
            working: HashMap::new(),
            working_revision: HashMap::new(),
            next_working_revision: 0,
            answers,
            fetch_generation: Cell::new(0),
            expected_fetch: RefCell::new(HashMap::new()),
            asks,
            paints: HashMap::new(),
            computation,
            inputs: HashMap::new(),
            retained: HashMap::new(),
            tickets: HashMap::new(),
            serial: 0,
            _compute_wake: compute_wake,
            _compute_quit: compute_quit,
            followed: HashMap::new(),
            look,
            ui,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            row_px: 26.0,
            stepping: None,
            inspection_paths: Vec::new(),
            file_picker: None,
            viewer: Viewer::default(),
            authors: Authors::default(),
            authors_menu: None,
            documents: HashMap::new(),
            hunks: Default::default(),
            buffers: HashMap::new(),
            expanded: HashMap::new(),
            revisions: HashMap::new(),
        };
        tab.old.insert(path.clone(), Ok(old));
        tab.working.insert(path, Some(proposed));
        tab.rebuild();
        tab
    }

    /// A tab for the working tree: every file the status map lists,
    /// with the working texts the window has for them.
    pub fn open_tree(
        root: &Path,
        files: Vec<(PathBuf, Option<Vec<u8>>)>,
        mode: DiffView,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let (asks, answers) = mpsc::channel();
        let (computation, compute_wake, compute_quit) = Self::computation(cx);
        let mut tab = Self {
            root: root.to_path_buf(),
            target: None,
            base: Base::Head,
            inspection: None,
            mode,
            files: Vec::new(),
            lines: Vec::new(),
            old: HashMap::new(),
            fetched: HashMap::new(),
            working: HashMap::new(),
            working_revision: HashMap::new(),
            next_working_revision: 0,
            answers,
            fetch_generation: Cell::new(0),
            expected_fetch: RefCell::new(HashMap::new()),
            asks,
            paints: HashMap::new(),
            computation,
            inputs: HashMap::new(),
            retained: HashMap::new(),
            tickets: HashMap::new(),
            serial: 0,
            _compute_wake: compute_wake,
            _compute_quit: compute_quit,
            followed: HashMap::new(),
            look,
            ui,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            row_px: 26.0,
            stepping: None,
            inspection_paths: Vec::new(),
            file_picker: None,
            viewer: Viewer::default(),
            authors: Authors::default(),
            authors_menu: None,
            documents: HashMap::new(),
            hunks: Default::default(),
            buffers: HashMap::new(),
            expanded: HashMap::new(),
            revisions: HashMap::new(),
        };
        tab.set_files(files);
        tab
    }

    /// The working tree's list changed — a new status map: keep what is
    /// still there, ask `HEAD` for what is new, drop what is gone.
    pub fn set_files(&mut self, files: Vec<(PathBuf, Option<Vec<u8>>)>) {
        let previous = self.current_file();
        let previous_path = self.files.get(previous).map(|file| file.path.clone());
        let wanted: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
        let mut to_fetch = Vec::new();
        for (path, working) in files {
            if !self.old.contains_key(&path) {
                to_fetch.push(path.clone());
            }
            if !self.followed.contains_key(&path) {
                self.next_revision(&path);
                self.working.insert(path.clone(), working);
            }
        }
        self.old.retain(|p, _| wanted.contains(p));
        self.fetched.retain(|p, _| wanted.contains(p));
        self.working.retain(|p, _| wanted.contains(p));
        // A path's revision goes with its bytes: a read started for it
        // finds nothing to match, and the counter, never reset, gives a path
        // that comes back a revision no old read holds — review r5.
        self.working_revision.retain(|p, _| wanted.contains(p));
        self.followed.retain(|p, _| wanted.contains(p));
        self.buffers.retain(|p, _| wanted.contains(p));
        self.hunks.retain(&wanted);
        let list_changed = self.files.iter().map(|f| &f.path).ne(wanted.iter());
        if list_changed {
            self.authors.entries.clear();
            self.authors.ranks.clear();
            if self.authors_menu.take().is_some() {
                self.viewer.focus_requested = true;
            }
            if self.file_picker.take().is_some() {
                self.viewer.focus_requested = true;
            }
            self.viewer.invalidate();
        }
        let mut retained: HashMap<_, _> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect();
        self.files = wanted
            .iter()
            .map(|path| {
                retained
                    .remove(path)
                    .unwrap_or_else(|| FileDiff::loading(path.clone()))
            })
            .collect();
        let paths: HashSet<_> = wanted.into_iter().collect();
        self.inputs.retain(|p, _| paths.contains(p));
        self.retained.retain(|p, _| paths.contains(p));
        self.tickets.retain(|p, _| paths.contains(p));
        self.documents.retain(|p, _| paths.contains(p));
        self.paints.retain(|p, _| paths.contains(p));
        self.expected_fetch
            .borrow_mut()
            .retain(|p, _| paths.contains(p));
        self.computation.retain(&paths);
        if !to_fetch.is_empty() {
            self.fetch(to_fetch);
        }
        self.rebuild();
        if list_changed {
            let file = previous_path
                .and_then(|path| self.files.iter().position(|file| file.path == path))
                .unwrap_or_else(|| previous.min(self.files.len().saturating_sub(1)));
            self.viewer.active_file = file;
            self.viewer.file_anchor = (!self.files.is_empty()).then_some(file);
        }
    }

    /// Live working-tree diffs follow a local move. Historical blobs and agent
    /// proposals keep the paths captured by their immutable comparison.
    pub(crate) fn relocate_paths(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        if !matches!(self.base, Base::Head | Base::Unstaged) {
            return;
        }
        if !self.paths().iter().any(|path| path.starts_with(from)) {
            return;
        }
        if let Some(path) = &mut self.target
            && let Some(next) = crate::file_operations::remap(path, from, to)
        {
            *path = next;
        }
        let files = self
            .paths()
            .into_iter()
            .map(|path| {
                let next =
                    crate::file_operations::remap(&path, from, to).unwrap_or_else(|| path.clone());
                let working = self.working.get(&path).cloned().flatten();
                (next, working)
            })
            .collect();
        self.followed.retain(|path, _| !path.starts_with(from));
        self.paints.retain(|path, _| !path.starts_with(from));
        self.inputs.retain(|path, _| !path.starts_with(from));
        self.set_files(files);
        cx.notify();
    }

    /// The paths the tab is about.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }

    /// Ask one thread for the texts of several paths, in order — the old
    /// side of each and, when the base ends at a blob, the new side too:
    /// one process per text, but not fifty threads. The answers come to
    /// [`Self::poll`], on the window's clock.
    fn fetch(&mut self, paths: Vec<PathBuf>) {
        for path in &paths {
            if let Some(input) = self.inputs.get(path).cloned()
                && let Some(file) = self
                    .files
                    .iter_mut()
                    .find(|f| &f.path == path && f.state != State::Loading)
            {
                let file = std::mem::replace(file, FileDiff::loading(path.clone()));
                self.retained.insert(
                    path.clone(),
                    (
                        input,
                        compute::ResultPair {
                            ticket: 0,
                            file,
                            document: self.documents.remove(path),
                            paint: self.paints.remove(path),
                        },
                    ),
                );
            }
            self.old.remove(path);
            self.fetched.remove(path);
            self.inputs.remove(path);
            self.tickets.remove(path);
            self.documents.remove(path);
            self.paints.remove(path);
        }
        self.computation
            .retain(&self.tickets.keys().cloned().collect());
        let generation = self.fetch_generation.get().wrapping_add(1);
        self.fetch_generation.set(generation);
        for path in &paths {
            self.expected_fetch
                .borrow_mut()
                .insert(path.clone(), generation);
        }
        self.fetch_hunk_facts(&paths, generation);
        let root = self.root.clone();
        let asks = self.asks.clone();
        let wake = self.computation.signal();
        let base = self.base.clone();
        let key = self.inspection.clone();
        let spawned = std::thread::Builder::new()
            .name("guild-git-show".into())
            .spawn(move || {
                // Push can inspect several commits. Use each file's
                // first before and last after, also following renames
                // for a single commit opened from the Log.
                let inspection = match &base {
                    Base::Commit(sha) => {
                        Some(git::inspection::read(&root, key.as_deref().unwrap_or(sha)))
                    }
                    _ => None,
                };
                for path in paths {
                    if let Some(changes) = &inspection {
                        let pair = changes
                            .as_ref()
                            .map(|changes| changes.iter().find(|c| c.file.path == path));
                        match pair {
                            Ok(Some(change)) => {
                                for (which, blob) in
                                    [(Which::Old, &change.before), (Which::New, &change.after)]
                                {
                                    let answer = match blob {
                                        None => Ok(None),
                                        Some((sha, name)) => {
                                            git::blob(&root, &format!("{sha}:{}", name.display()))
                                        }
                                    };
                                    if asks
                                        .send((
                                            which,
                                            path.clone(),
                                            answer,
                                            blob.as_ref().map(|(sha, name)| {
                                                format!("{sha}:{}", name.display())
                                            }),
                                            generation,
                                        ))
                                        .is_err()
                                    {
                                        return;
                                    }
                                    wake.notify_one();
                                }
                                continue;
                            }
                            Err(err) => {
                                for which in [Which::Old, Which::New] {
                                    if asks
                                        .send((
                                            which,
                                            path.clone(),
                                            Err(err.clone()),
                                            None,
                                            generation,
                                        ))
                                        .is_err()
                                    {
                                        return;
                                    }
                                    wake.notify_one();
                                }
                                continue;
                            }
                            Ok(None) => {}
                        }
                    }
                    let (old, new) = sides(&base, &path);
                    let wanted = [(Which::Old, old), (Which::New, new)];
                    for (which, side) in wanted {
                        let Side::Blob(spec) = side else {
                            continue;
                        };
                        let answer = git::blob(&root, &spec);
                        if asks
                            .send((which, path.clone(), answer, Some(spec), generation))
                            .is_err()
                        {
                            return;
                        }
                        wake.notify_one();
                    }
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "the git show thread could not be started");
        }
    }

    /// Take what the threads have answered; true when something changed.
    pub fn poll(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        while let Ok((which, path, answer, revision, generation)) = self.answers.try_recv() {
            if self.expected_fetch.borrow().get(&path) != Some(&generation) {
                continue;
            }
            self.revisions.entry(path.clone()).or_default()[usize::from(which == Which::New)] =
                revision;
            match which {
                Which::Old => self.old.insert(path, answer),
                Which::New => self.fetched.insert(path, answer),
            };
            changed = true;
        }
        if changed {
            self.rebuild();
        }
        for (path, result) in self.computation.drain() {
            if self.tickets.get(&path) != Some(&result.ticket) {
                continue;
            }
            let Some(file) = self.files.iter_mut().find(|f| f.path == path) else {
                continue;
            };
            *file = result.file;
            if let Some(document) = result.document {
                self.documents.insert(path.clone(), document);
            } else {
                self.documents.remove(&path);
            }
            if let Some(paint) = result.paint {
                self.paints.insert(path.clone(), paint);
            } else {
                self.paints.remove(&path);
            }
            changed = true;
        }
        if changed {
            self.project_rows();
            cx.notify();
        }
        // After the documents: offers are computed for the one shown.
        if self.poll_hunks(cx) {
            cx.notify();
        }
        changed
    }

    /// Follow an open file tab's editor: every change to the buffer is a
    /// new working text, so the diff is of what is on screen and not of
    /// the disk.
    ///
    /// The buffer is taken **in the file's own line endings** — what a save
    /// would write — so that a staged hunk carries the file's bytes and not
    /// the editor's `\n`. GLD-026.
    pub fn follow(
        &mut self,
        path: PathBuf,
        editor: &Entity<EditorState>,
        ending: impl Fn(&gpui::App) -> crate::highlight::LineEnding + 'static,
        cx: &mut Context<Self>,
    ) {
        // A base that ends at a blob shows git's text, not the buffer's:
        // the index against `HEAD` does not change as one types.
        if !self.new_is_working() {
            return;
        }
        // Read at each change, not when following began: a reload keeps the
        // editor and can change the file's endings.
        let weak = editor.downgrade();
        let serialize: Serialize = std::rc::Rc::new(move |cx: &gpui::App| {
            weak.upgrade()
                .map(|editor| {
                    ending(cx)
                        .apply(&editor.read(cx).text().to_string())
                        .into_bytes()
                })
                .unwrap_or_default()
        });
        self.buffers.insert(path.clone(), serialize.clone());
        // Following the same editor again — the window does on a reload's
        // `BufferChanged`, which replaces the text without a Change event —
        // takes the buffer as it is now.
        if self
            .followed
            .get(&path)
            .is_some_and(|(id, _)| *id == editor.entity_id())
        {
            let now = serialize(cx);
            if self.working.get(&path).cloned().flatten().as_deref() != Some(&now[..]) {
                self.set_working(path, Some(now), cx);
            }
            return;
        }
        let for_path = path.clone();
        let on_change = serialize.clone();
        let subscription = cx.subscribe(editor, move |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = on_change(cx);
                this.set_working(for_path.clone(), Some(text), cx);
            }
        });
        self.followed
            .insert(path.clone(), (editor.entity_id(), subscription));
        let text = serialize(cx);
        self.set_working(path, Some(text), cx);
    }

    /// Whether this path's editor is being followed.
    pub fn follows(&self, path: &Path) -> bool {
        self.followed.contains_key(path)
    }

    /// Stop following a path's editor — its tab closed; the disk is the
    /// working text again.
    pub fn unfollow(&mut self, path: &Path, from_disk: Option<Vec<u8>>, cx: &mut Context<Self>) {
        self.followed.remove(path);
        self.buffers.remove(path);
        self.set_working(path.to_path_buf(), from_disk, cx);
    }

    /// The working text of a path changed: a buffer, or the disk.
    pub fn set_working(&mut self, path: PathBuf, text: Option<Vec<u8>>, cx: &mut Context<Self>) {
        self.next_revision(&path);
        self.working.insert(path, text);
        self.rebuild();
        cx.notify();
    }

    /// Whether a file of the diff still waits for its texts — for the tests
    /// that must let its reads land before they end.
    #[cfg(test)]
    pub(crate) fn loading(&self) -> bool {
        self.files.iter().any(|f| f.state == State::Loading)
    }

    /// The working text held for a path, for the tests outside this module.
    #[cfg(test)]
    pub(crate) fn working_for_test(&self, path: &Path) -> Option<Vec<u8>> {
        self.working.get(path).cloned().flatten()
    }

    /// A new revision of a path's working text, superseding every read
    /// started for an older one.
    fn next_revision(&mut self, path: &Path) -> u64 {
        self.next_working_revision += 1;
        self.working_revision
            .insert(path.to_path_buf(), self.next_working_revision);
        self.next_working_revision
    }

    /// `HEAD` or the index moved — a commit, a stage: every path is
    /// asked for again, on both sides git holds.
    pub fn head_moved(&mut self, cx: &mut Context<Self>) {
        let paths = self.paths();
        self.fetch(paths);
        self.rebuild();
        cx.notify();
    }

    /// One path asked for again — its texts and its hunk facts — after a
    /// hunk write about it refused. GLD-026.
    ///
    /// The working side is read again too — review r3 `bugs+impl-6`: a
    /// refusal for disk bytes that moved would otherwise rebuild the same
    /// offer over the old text, and a tracked file under an ignored folder
    /// sends no watcher event to correct it. A followed buffer is serialized
    /// now; the disk is read off the window's thread, without waiting on a
    /// FIFO.
    pub fn refetch(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !self.files.iter().any(|f| f.path == path) {
            return;
        }
        if matches!(self.base, Base::Head | Base::Unstaged) {
            if let Some(serialize) = self.buffers.get(path) {
                let now = serialize(cx);
                self.next_revision(path);
                self.working.insert(path.to_path_buf(), Some(now));
            } else {
                // Bounded at the diff's limit and a byte — still *too large*
                // past it, without the rest in memory (review r4
                // `adversarial-6`) — and put in only while no newer working
                // text has come: a watcher's, a buffer's, another refetch's
                // (`bugs+impl-7`).
                let revision = self.next_revision(path);
                let file = self.root.join(path);
                let path = path.to_path_buf();
                let read = cx.background_executor().spawn(async move {
                    crate::file_operations::read_regular_file_up_to(&file, DIFF_LIMIT)
                });
                cx.spawn(async move |tab, cx| {
                    let text = read.await;
                    let _ = tab.update(cx, |tab, cx| {
                        if tab.working_revision.get(&path) == Some(&revision) && !tab.follows(&path)
                        {
                            tab.set_working(path, text, cx);
                        }
                    });
                })
                .detach();
            }
        }
        self.fetch(vec![path.to_path_buf()]);
        self.rebuild();
        cx.notify();
    }

    /// Whether the new side is the working tree's text rather than a
    /// blob of git's.
    fn new_is_working(&self) -> bool {
        // A proposal's new side is the agent's text, which is put in
        // `working` when the tab is made — the same slot the working
        // tree's text uses, because neither is fetched from git.
        matches!(self.base, Base::Head | Base::Unstaged | Base::Proposal)
    }

    /// What the two texts compare, as the tab was opened.
    pub fn base(&self) -> &Base {
        &self.base
    }

    /// Two columns or one.
    pub fn mode(&self) -> DiffView {
        self.mode
    }

    /// The head's button: the other view, for this tab.
    pub fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.mode = self.mode.toggled();
        self.project_preserving_position(cx);
        cx.notify();
    }
    /// Scroll to the next hunk after the row at the top of the view, or
    /// the previous one before it; nothing past the ends.
    pub fn step_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.can_step_change(forward) {
            return;
        }
        let targets = self.change_targets();
        let index = if forward {
            self.current_change().map_or(0, |i| i + 1)
        } else {
            self.current_change().unwrap_or(0).saturating_sub(1)
        };
        if let Some((file, block, row)) = targets.get(index) {
            self.viewer.active_change = Some((*file, *block));
            self.viewer.active_file = *file;
            self.scroll_to_row(*row, cx);
        }
    }

    /// The row at the top of the view, from the list's offset and a
    /// row's height: the handle keeps an offset, and its own index is
    /// behind gpui's test feature.
    pub fn top_row(&self) -> usize {
        let offset = f32::from(self.scroll.0.borrow().base_handle.offset().y);
        ((-offset) / self.row_px.max(1.0)).round().max(0.0) as usize
    }

    /// Scroll to a row: the bar's click.
    pub fn scroll_to_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Line::Source {
            file,
            row: DisplayRow::Text {
                block: Some(block), ..
            },
        }) = self.lines.get(index)
        {
            self.viewer.active_change = Some((*file, *block));
        }
        self.scroll
            .scroll_to_item_strict(index, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    /// How many hunks a file has — *K differences*.
    /// Show the toolbar's file stepping — *i/N files* with ← and → and
    /// the pencil — for a diff in its own window. M8.3.6.
    pub fn set_stepping(&mut self, stepping: Option<(usize, usize)>, cx: &mut Context<Self>) {
        self.stepping = stepping;
        cx.notify();
    }

    /// Which file of how many, when stepping.
    pub fn stepping(&self) -> Option<(usize, usize)> {
        self.stepping
    }

    /// The one-based line of the first change in the first file — where
    /// the pencil opens the file. The top when nothing has changed yet.
    pub fn first_change_line(&self) -> usize {
        self.files
            .get(self.current_file())
            .and_then(|file| self.documents.get(&file.path))
            .and_then(|doc| doc.blocks.first())
            .map_or(1, |block| block.new.start + 1)
    }

    pub fn differences(&self, file: usize) -> usize {
        self.files
            .get(file)
            .and_then(|f| self.documents.get(&f.path))
            .map_or(0, |d| d.blocks.len())
    }

    /// Capture only changed pairs. All algorithms execute on the owned worker.
    fn rebuild(&mut self) {
        let working = self.new_is_working();
        let mut invalidated = false;
        for file in &mut self.files {
            let old = self.old.get(&file.path);
            let new = if working {
                Some(Ok(self.working.get(&file.path).and_then(|v| v.as_deref())))
            } else {
                self.fetched
                    .get(&file.path)
                    .map(|v| v.as_ref().map(|v| v.as_deref()))
            };
            if self.retained.get(&file.path).is_some_and(|(input, _)| {
                input.matches((&self.root, &self.base, &self.inspection), old, new)
            }) {
                let (input, result) = self.retained.remove(&file.path).unwrap();
                self.computation.revoke(&file.path);
                self.serial += 1;
                self.tickets.insert(file.path.clone(), self.serial);
                self.inputs.insert(file.path.clone(), input);
                if let Some(document) = result.document {
                    self.documents.insert(file.path.clone(), document);
                }
                if let Some(paint) = result.paint {
                    self.paints.insert(file.path.clone(), paint);
                }
                *file = result.file;
                continue;
            }
            if self.inputs.get(&file.path).is_some_and(|input| {
                input.matches((&self.root, &self.base, &self.inspection), old, new)
            }) {
                continue;
            }
            // A changed ready pair supersedes the one retained during its fetch.
            if old.is_some() && new.is_some() {
                self.retained.remove(&file.path);
            }
            let input = compute::Input::capture(
                (
                    self.root.clone(),
                    self.base.clone(),
                    self.inspection.clone(),
                ),
                old,
                new,
            );
            self.serial += 1;
            self.tickets.insert(file.path.clone(), self.serial);
            self.inputs.insert(file.path.clone(), input.clone());
            self.computation
                .submit(file.path.clone(), self.serial, input);
            self.documents.remove(&file.path);
            self.paints.remove(&file.path);
            if !self.retained.contains_key(&file.path) {
                self.expanded.remove(&file.path);
                invalidated = true;
            }
            *file = FileDiff::loading(file.path.clone());
        }
        if invalidated {
            self.viewer.invalidate();
        }
        self.project_rows();
    }

    fn computation(cx: &mut Context<Self>) -> (compute::Owner, gpui::Task<()>, gpui::Subscription) {
        let owner = compute::Owner::new(cx.background_executor().clone());
        let changed = owner.signal();
        let wake = cx.spawn(async move |tab, cx| {
            loop {
                changed.notified().await;
                if tab
                    .update(cx, |tab, cx| {
                        tab.poll(cx);
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        let quit = cx.on_app_quit(|tab, cx| {
            let worker = tab.computation.stop();
            cx.background_spawn(async move {
                if let Some(worker) = worker {
                    let _ = worker.join();
                }
            })
        });
        (owner, wake, quit)
    }

    pub(crate) fn project_rows(&mut self) {
        let mut out = Vec::new();
        if self.target.is_none() {
            out.push(Line::Summary);
            out.extend((0..self.files.len()).map(|file| Line::Listed { file }));
        }
        for (file, f) in self.files.iter().enumerate() {
            out.push(Line::File { file });
            if let Some(doc) = self.documents.get(&f.path) {
                if doc.blocks.is_empty() {
                    out.push(Line::Note { file });
                }
                let expanded = self.expanded.entry(f.path.clone()).or_default();
                out.extend(
                    doc.rows(self.mode == DiffView::Unified, expanded)
                        .into_iter()
                        .map(|row| Line::Source { file, row }),
                );
            } else {
                out.push(Line::Note { file });
            }
        }
        self.lines = out;
    }

    pub(crate) fn revision_labels(&self, file: usize) -> [String; 2] {
        let Some(f) = self.files.get(file) else {
            return [String::new(), String::new()];
        };
        if matches!(self.base, Base::Commit(_)) {
            if f.state == State::Loading {
                return std::array::from_fn(|_| {
                    self.viewer_label("Loading…", "Загрузка…").to_owned()
                });
            }
            let revisions = self.revisions.get(&f.path).cloned().unwrap_or_default();
            return std::array::from_fn(|side| {
                revisions[side].clone().unwrap_or_else(|| {
                    let answer = if side == 0 {
                        self.old.get(&f.path)
                    } else {
                        self.fetched.get(&f.path)
                    };
                    if matches!(answer, Some(Ok(None))) {
                        self.viewer_label("Absent", "Отсутствует").to_owned()
                    } else {
                        self.viewer_label("Unavailable", "Недоступно").to_owned()
                    }
                })
            });
        }
        match self.base {
            Base::Head => [
                "HEAD".into(),
                self.viewer_label("Working tree", "Рабочее дерево").into(),
            ],
            Base::Index => ["HEAD".into(), self.viewer_label("Index", "Индекс").into()],
            Base::Unstaged => [
                self.viewer_label("Index", "Индекс").into(),
                self.viewer_label("Working tree", "Рабочее дерево").into(),
            ],
            Base::Proposal => [
                self.viewer_label("Current", "Текущий").into(),
                self.viewer_label("Proposed", "Предложенный").into(),
            ],
            Base::Commit(_) => unreachable!(),
        }
    }

    pub(crate) fn viewer_label<'a>(&self, english: &'a str, russian: &'a str) -> &'a str {
        match self.ui.0 {
            guild_session::Language::English => english,
            guild_session::Language::Russian => russian,
        }
    }

    /// The two sides of a path, painted, when they have arrived.
    pub fn paint(&self, path: &Path) -> Option<Arc<Painted>> {
        self.paints.get(path).cloned()
    }

    /// The file this tab is about, when it is about one.
    pub fn target(&self) -> Option<&Path> {
        self.target.as_deref()
    }

    /// The files, each with its state and hunks.
    pub fn files(&self) -> &[FileDiff] {
        &self.files
    }

    /// The rows the list draws.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn look(&self) -> &Look {
        &self.look
    }

    pub fn ui(&self) -> Ui {
        self.ui
    }

    /// Set the tab another way: the palette or the font changed.
    pub fn restyle(&mut self, look: Look, ui: Ui, cx: &mut Context<Self>) {
        self.look = look;
        self.ui = ui;
        cx.notify();
    }
}

impl Focusable for DiffTab {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// The rows for a list of files: for the working tree a summary and the
/// list first; then each file's header, then its hunks' rows, or the one
/// line that says why there are none.
pub fn lines(files: &[FileDiff], listed: bool, mode: DiffView) -> Vec<Line> {
    let mut out = Vec::new();
    if listed {
        out.push(Line::Summary);
        for index in 0..files.len() {
            out.push(Line::Listed { file: index });
        }
    }
    for (index, file) in files.iter().enumerate() {
        out.push(Line::File { file: index });
        if file.hunks.is_empty() {
            out.push(Line::Note { file: index });
        } else if mode == DiffView::SideBySide {
            for pair in diff::sides(&file.hunks) {
                out.push(Line::Pair { file: index, pair });
            }
        } else {
            for row in diff::rows(&file.hunks) {
                out.push(Line::Hunk { file: index, row });
            }
        }
    }
    out
}

/// The indices of every hunk header in the rows — the places the next
/// and the previous change step between, and the marks on the bar.
pub fn hunk_headers(lines: &[Line]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            if let Line::Source {file, row: DisplayRow::Text {block:Some(block),..}} = line {
                return !matches!(index.checked_sub(1).and_then(|i| lines.get(i)), Some(Line::Source {file:f,row:DisplayRow::Text {block:Some(b),..}}) if f==file && b==block);
            }
            matches!(
                line,
                Line::Hunk {
                    row: Row::Header { .. },
                    ..
                } | Line::Pair {
                    pair: Pair::Header { .. },
                    ..
                }
            )
        })
        .map(|(index, _)| index)
        .collect()
}

/// The first header after `from`, or none past the last.
pub fn next_change(headers: &[usize], from: usize) -> Option<usize> {
    headers.iter().copied().find(|h| *h > from)
}

/// The last header before `from`, or none before the first.
pub fn previous_change(headers: &[usize], from: usize) -> Option<usize> {
    headers.iter().copied().rev().find(|h| *h < from)
}

/// Each header's place as a share of the list, for the marks on the
/// bar: a header at row 25 of 100 is a quarter of the way down. An
/// empty list puts every mark at the top rather than dividing by zero.
pub fn change_positions(headers: &[usize], count: usize) -> Vec<f32> {
    headers
        .iter()
        .map(|h| {
            if count == 0 {
                0.0
            } else {
                *h as f32 / count as f32
            }
        })
        .collect()
}

/// The file whose rows the line at `index` belongs to — the one whose
/// header was last passed — for *i / N files*.
pub fn file_at(lines: &[Line], index: usize) -> Option<usize> {
    lines
        .iter()
        .take(index.saturating_add(1).min(lines.len()))
        .rev()
        .find_map(|line| match line {
            Line::Source { file, .. }
            | Line::File { file }
            | Line::Hunk { file, .. }
            | Line::Pair { file, .. }
            | Line::Note { file } => Some(*file),
            Line::Summary | Line::Listed { .. } => None,
        })
}

/// Whether a hunk adds, removes, or both — the mark's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    Added,
    Removed,
    Changed,
}

/// The kind of the hunk that starts at a header row.
pub fn hunk_kind(lines: &[Line], header: usize) -> HunkKind {
    if let Some(Line::Source {
        file,
        row: DisplayRow::Text {
            block: Some(block), ..
        },
    }) = lines.get(header)
    {
        let mut added = false;
        let mut removed = false;
        for line in lines.iter().skip(header) {
            if let Line::Source {
                file: f,
                row:
                    DisplayRow::Text {
                        old,
                        new,
                        block: Some(b),
                    },
            } = line
                && f == file
                && b == block
            {
                added |= new.is_some();
                removed |= old.is_some();
            } else {
                break;
            }
        }
        return match (added, removed) {
            (true, false) => HunkKind::Added,
            (false, true) => HunkKind::Removed,
            _ => HunkKind::Changed,
        };
    }
    let (mut added, mut removed) = (false, false);
    for line in lines.iter().skip(header + 1) {
        match line {
            Line::Hunk {
                row: Row::Added { .. },
                ..
            } => added = true,
            Line::Hunk {
                row: Row::Removed { .. },
                ..
            } => removed = true,
            Line::Pair {
                pair: Pair::Changed { old, new },
                ..
            } => {
                added |= new.is_some();
                removed |= old.is_some();
            }
            Line::Hunk {
                row: Row::Header { .. },
                ..
            }
            | Line::Pair {
                pair: Pair::Header { .. },
                ..
            }
            | Line::File { .. }
            | Line::Summary
            | Line::Listed { .. }
            | Line::Note { .. } => break,
            _ => {}
        }
    }
    match (added, removed) {
        (true, false) => HunkKind::Added,
        (false, true) => HunkKind::Removed,
        _ => HunkKind::Changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_of(head: Option<&str>, working: Option<&str>) -> FileDiff {
        FileDiff::compute(
            PathBuf::from("a.rs"),
            head.map(str::as_bytes),
            working.map(str::as_bytes),
        )
    }

    /// The next change from before the first header is the first; from
    /// the last there is none; the previous walks back the same way,
    /// and from the first there is none.
    #[test]
    fn the_next_and_previous_change_walk_the_headers() {
        let files = vec![
            diff_of(Some("a\nb\nc\n"), Some("a\nB\nc\n")),
            diff_of(Some("x\ny\n"), Some("x\nY\n")),
        ];
        let all = lines(&files, true, DiffView::Unified);
        let headers = hunk_headers(&all);
        assert_eq!(headers.len(), 2, "{all:?}");
        assert_eq!(next_change(&headers, 0), Some(headers[0]));
        assert_eq!(next_change(&headers, headers[0]), Some(headers[1]));
        assert_eq!(next_change(&headers, headers[1]), None);
        assert_eq!(previous_change(&headers, headers[1]), Some(headers[0]));
        assert_eq!(previous_change(&headers, headers[0]), None);
        assert_eq!(previous_change(&headers, all.len()), Some(headers[1]));
    }

    /// Every state a file's diff can be in, from the two texts alone.
    #[test]
    fn a_file_diff_names_what_it_found() {
        assert_eq!(diff_of(Some("a\n"), Some("a\n")).state, State::NoChanges);
        let changed = diff_of(Some("a\n"), Some("b\n"));
        assert_eq!(changed.state, State::Hunks);
        assert_eq!(changed.counts, (1, 1));

        let added = diff_of(None, Some("a\nb\n"));
        assert_eq!(added.state, State::Added);
        assert_eq!(added.counts, (2, 0), "everything is new");

        let deleted = diff_of(Some("a\nb\nc\n"), None);
        assert_eq!(deleted.state, State::Deleted);
        assert_eq!(deleted.counts, (0, 3), "everything is old");

        let png = FileDiff::compute(
            PathBuf::from("x.png"),
            Some(b"\x89PNG\x00\x00"),
            Some(b"\x89PNG\x00\x01"),
        );
        assert_eq!(png.state, State::Binary);
        let big = vec![b'x'; DIFF_LIMIT as usize + 1];
        assert_eq!(
            FileDiff::compute(PathBuf::from("big"), Some(&big), Some(b"x")).state,
            State::TooLarge
        );
    }

    /// The rows: a header per file, then hunks or the one note line.
    #[test]
    fn the_lines_are_a_header_then_hunks_or_a_note() {
        let files = vec![
            diff_of(Some("a\n"), Some("b\n")),
            diff_of(Some("a\n"), Some("a\n")),
        ];
        let all = lines(&files, false, DiffView::Unified);
        assert_eq!(all[0], Line::File { file: 0 });
        assert!(matches!(
            all[1],
            Line::Hunk {
                file: 0,
                row: Row::Header { .. }
            }
        ));
        let note = all
            .iter()
            .position(|l| matches!(l, Line::Note { file: 1 }))
            .expect("a note for the unchanged file");
        assert_eq!(all[note - 1], Line::File { file: 1 });
    }

    /// The working tree's tab lists the changed files first — tracked in
    /// path order, then untracked, never ignored, never an untracked
    /// directory — and knows where each file's header is for the jump.
    #[test]
    fn the_working_tree_lists_its_files_and_knows_where_each_starts() {
        use crate::git::status::parse;
        let captured = include_bytes!("../tests/fixtures/porcelain-v2.bin");
        let map = StatusMap::from_entries(parse(captured).unwrap());
        let files = tree_files(&map);
        let paths: Vec<&str> = files.iter().map(|f| f.path.to_str().unwrap()).collect();
        assert_eq!(
            paths,
            [
                "docs/файл.md",
                "src/added.rs",
                "src/both.rs",
                "src/conflict.rs",
                "src/deleted.rs",
                "src/modified.rs",
                "src/new-name.rs",
                "src/staged.rs",
                "with space/file name.md",
                "untracked.txt",
            ]
        );

        let diffs = vec![
            diff_of(Some("a\n"), Some("b\n")),
            diff_of(None, Some("x\ny\n")),
        ];
        assert_eq!(totals(&diffs), (3, 1));
        let all = lines(&diffs, true, DiffView::Unified);
        assert_eq!(all[0], Line::Summary);
        assert_eq!(all[1], Line::Listed { file: 0 });
        assert_eq!(all[2], Line::Listed { file: 1 });
        assert_eq!(header_index(&all, 0), Some(3));
        let second = header_index(&all, 1).expect("the second file's header");
        assert!(second > 3);
        assert_eq!(all[second], Line::File { file: 1 });
    }
}

#[cfg(test)]
mod scheduling_tests;
