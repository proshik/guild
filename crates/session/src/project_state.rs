//! A project's own state, kept where the project is.
//!
//! A session belongs to a project, and a project is a directory. Keeping
//! every project's sessions in one global file made that file the place all
//! work accumulated, and made closing a project the only way to be rid of
//! them — which is why closing one lost them: there was nowhere else to put
//! what it had.
//!
//! So the state goes beside the project, in a directory of its own.
//! What that buys beyond the fix: a project moved to another machine keeps
//! its own history, and the global file goes back to being a short list of
//! directories.
//!
//! **The directory carries its own `.gitignore`.** Editing a person's is
//! not ours to do, and a directory that ignores itself needs nobody's
//! permission and shows up in no `git status`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::desk::SessionRecord;
use crate::session_id::SessionId;
use crate::store::{Normalise, Store, StoreError};

/// The directory Guild keeps a project's state in.
const DIRECTORY: &str = ".guild";

/// What a tab was showing.
///
/// Not a `SessionId` on its own: from M5.4 a tab is a view onto
/// *something*, and settings is already not a session. A `File` variant is
/// what M8 adds here, and it changes nothing already written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shows {
    /// A Claude session or a shell.
    Session(SessionId),
    /// Guild's own settings.
    Settings,
    /// Decode-only marker for obsolete native Review tabs. Never restore a body.
    #[doc(hidden)]
    LegacyReview,
    /// A durable workflow run reference; no session identity is invented.
    Process {
        /// Stable retained run identity.
        run: crate::workflow::RunId,
    },
    /// A file of the project, by its path relative to the project.
    File(PathBuf),
    /// A diff: one file's by its relative path, or the working tree's
    /// when there is none — M8.2 — and what it compares, M8.3.
    Diff {
        /// One file's, by its relative path; the working tree's when none.
        path: Option<PathBuf>,
        /// What the two texts are.
        base: Base,
    },
    /// A file annotated, by its relative path. M8.3.
    Annotate(PathBuf),
}

/// Which two texts a diff compares — decision 5 of M8.3. Kept beside
/// the path so that a tab comes back comparing what it compared.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base {
    /// The last commit against the working tree: M8.2's diff.
    #[default]
    Head,
    /// The last commit against the index: what a commit would take.
    Index,
    /// The index against the working tree: what is not yet staged.
    Unstaged,
    /// A commit's parent against the commit, by its hash.
    Commit(String),
    /// **What an agent proposes**, against the file as it is — M8.6.
    ///
    /// The new side is a text the window holds and nothing else knows:
    /// `claude` sent it over the IDE socket and it exists nowhere on
    /// disk. So a tab on this base is **never written to the project's
    /// state** — restoring it would restore a diff against a proposal
    /// that is gone. The variant is here rather than in the desktop
    /// because `Shows::Diff` carries a `Base`, and two kinds of base
    /// would be two enums to keep in step.
    Proposal,
}

/// `Shows` on the wire. M8.2 wrote a diff tab as its path alone —
/// `"diff": "src/lib.rs"`, or `null` for the working tree — and M8.3
/// writes the path with its base. Both read, as `Head` when there is no
/// base; only the second is written.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShowsWire {
    Session(SessionId),
    Settings,
    /// The project review surface; execution belongs to its archive.
    Review,
    /// A durable workflow run reference; no session identity is invented.
    Process {
        /// Stable retained run identity.
        run: crate::workflow::RunId,
    },
    File(PathBuf),
    Diff(DiffWire),
    Annotate(PathBuf),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum DiffWire {
    WithBase {
        path: Option<PathBuf>,
        #[serde(default)]
        base: Base,
    },
    PathOnly(Option<PathBuf>),
}

impl From<Shows> for ShowsWire {
    fn from(shows: Shows) -> Self {
        match shows {
            Shows::Session(id) => ShowsWire::Session(id),
            Shows::Settings => ShowsWire::Settings,
            Shows::LegacyReview => ShowsWire::Review,
            Shows::Process { run } => ShowsWire::Process { run },
            Shows::File(path) => ShowsWire::File(path),
            Shows::Diff { path, base } => ShowsWire::Diff(DiffWire::WithBase { path, base }),
            Shows::Annotate(path) => ShowsWire::Annotate(path),
        }
    }
}

impl From<ShowsWire> for Shows {
    fn from(wire: ShowsWire) -> Self {
        match wire {
            ShowsWire::Session(id) => Shows::Session(id),
            ShowsWire::Settings => Shows::Settings,
            ShowsWire::Review => Shows::LegacyReview,
            ShowsWire::Process { run } => Shows::Process { run },
            ShowsWire::File(path) => Shows::File(path),
            ShowsWire::Diff(DiffWire::WithBase { path, base }) => Shows::Diff { path, base },
            ShowsWire::Diff(DiffWire::PathOnly(path)) => Shows::Diff {
                path,
                base: Base::Head,
            },
            ShowsWire::Annotate(path) => Shows::Annotate(path),
        }
    }
}

impl Serialize for Shows {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ShowsWire::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Shows {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ShowsWire::deserialize(deserializer).map(Shows::from)
    }
}

/// A tab that was open when the project was last put away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenTab {
    /// What it was showing — the focused pane's, when it was divided, so
    /// that a build from before M12.8 opens the same thing (decision 5).
    pub shows: Shows,
    /// Its panes — M12.8. Absent from every file written before it and
    /// from every tab of one pane; read back through `panes_or_one`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panes: Option<crate::panes::Panes<Shows>>,
    /// The single pane's mode, or the focused file's mirror for older readers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown_mode: Option<crate::markdown::Mode>,
    /// Modes by stable pane identity for a divided tab. Older builds ignore these keys.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub markdown_modes: std::collections::BTreeMap<crate::PaneId, crate::markdown::Mode>,
}

impl OpenTab {
    /// A tab of one pane showing `shows`: written as M12.5 wrote it.
    pub fn showing(shows: Shows) -> Self {
        Self {
            shows,
            panes: None,
            markdown_mode: None,
            markdown_modes: Default::default(),
        }
    }

    /// A tab to write from its panes. `shows` mirrors the focused pane,
    /// or the first that shows something; `panes` is kept only when there
    /// is a division. `None` when no pane shows anything: such a tab has
    /// nothing to reopen.
    pub fn from_panes(panes: crate::panes::Panes<Shows>) -> Option<Self> {
        let shows = panes
            .body()
            .or_else(|| panes.bodies().first().map(|(_, body)| *body))?
            .clone();
        let divided = panes.ids().len() > 1;
        Some(Self {
            shows,
            panes: divided.then_some(panes),
            markdown_mode: None,
            markdown_modes: Default::default(),
        })
    }

    /// The tab's panes: the ones written, or one showing `shows`. Panes
    /// written that show nothing are not something this build writes
    /// (a hand-edited file, at most), and are read as `shows` alone —
    /// the window is never handed a tab that shows nothing.
    pub fn panes_or_one(&self) -> crate::panes::Panes<Shows> {
        match &self.panes {
            Some(panes) => {
                let panes = panes.clone().normalised();
                if panes.showing() == 0 {
                    crate::panes::Panes::one(self.shows.clone())
                } else {
                    panes
                }
            }
            None => crate::panes::Panes::one(self.shows.clone()),
        }
    }

    /// The tab with the panes `keep` refuses dropped, and the tab with
    /// them when nothing is left: `restorable`'s rule, per pane.
    pub fn keeping(&self, keep: impl Fn(&Shows) -> bool) -> Option<Self> {
        let panes = self
            .panes_or_one()
            .map(|shows| keep(shows).then(|| shows.clone()))?;
        let modes = panes
            .bodies()
            .into_iter()
            .filter_map(|(id, shows)| {
                matches!(shows, Shows::File(path) if crate::markdown::has_preview(path))
                    .then(|| self.markdown_mode_for(id).map(|mode| (id, mode)))
                    .flatten()
            })
            .collect();
        Self::from_panes_with_modes(panes, modes)
    }

    /// Remove obsolete native review bodies without changing the pane tree.
    /// `None` means that this tab has no surviving content to reopen.
    pub fn without_native_review(&self) -> Option<Self> {
        let mut panes = self.panes_or_one();
        let focused_review = matches!(panes.body(), Some(Shows::LegacyReview));
        let retired: Vec<_> = panes
            .bodies()
            .into_iter()
            .filter_map(|(id, shows)| matches!(shows, Shows::LegacyReview).then_some(id))
            .collect();
        if retired.is_empty() {
            return Some(self.clone());
        }
        for id in retired {
            panes.take(id);
        }
        let first = panes.bodies().first().map(|(id, _)| *id)?;
        if focused_review {
            panes.focus(first);
        }
        Self::from_panes_with_modes(panes, self.markdown_modes.clone())
    }

    /// Read presentation for a pane, including a single-pane legacy mirror.
    pub fn markdown_mode_for(&self, pane: crate::PaneId) -> Option<crate::markdown::Mode> {
        if self.panes.is_some() {
            self.markdown_modes.get(&pane).copied()
        } else {
            self.markdown_mode
        }
    }

    /// Keep presentation beside the unchanged `Shows::File` wire representation.
    pub fn from_panes_with_modes(
        panes: crate::Panes<Shows>,
        mut modes: std::collections::BTreeMap<crate::PaneId, crate::markdown::Mode>,
    ) -> Option<Self> {
        modes.retain(|id, _| matches!(panes.body_of(*id), Some(Shows::File(path)) if crate::markdown::has_preview(path)));
        let mirrored = panes
            .body()
            .map(|_| panes.focused())
            .or_else(|| panes.bodies().first().map(|(id, _)| *id));
        let mode = mirrored.and_then(|id| modes.get(&id).copied());
        let mut tab = Self::from_panes(panes)?;
        tab.markdown_mode = mode;
        if tab.panes.is_some() {
            tab.markdown_modes = modes;
        }
        Some(tab)
    }
}

/// One panel: whether it is open, and how big it was made.
///
/// The size is a person's, set by dragging a border, and it lives with the
/// project rather than with the settings for the same reason the open state
/// does: a repository with deep paths needs a wider tree than one without,
/// and that is a fact about the repository.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    /// Whether it is showing.
    pub open: bool,
    /// How wide, or for the bottom one how tall, in pixels.
    ///
    /// Pixels rather than rems, deliberately. A person drags pixels, and a
    /// width that moved when the interface font changed would fight the hand
    /// that set it.
    pub size: f32,
}

impl Panel {
    /// The narrowest a side panel may be made.
    ///
    /// A panel dragged to nothing looks exactly like a panel that failed to
    /// open, and the way back from it is a pixel wide.
    pub const MIN_WIDTH: f32 = 160.0;
    /// The shortest the bottom one may be made.
    pub const MIN_HEIGHT: f32 = 90.0;
    /// What is left of the work when a panel has taken everything else.
    ///
    /// **A panel's ceiling is the window, not a number.** M5.7 gave the
    /// side panels 560 and the bottom one 340, which read as generous
    /// until somebody wanted the Log at the top of a tall screen: a
    /// number cannot know how tall the screen is, and *a third of it* was
    /// a guess about a window nobody had measured. The rule now is that a
    /// panel may take everything but this — enough that the tab strip and
    /// a line of what it holds stay visible, so the way back is on
    /// screen.
    pub const MIN_WORK: f32 = 120.0;

    /// The most a panel may be given, in a window of this size along its
    /// own axis.
    pub fn ceiling(available: f32, least: f32) -> f32 {
        (available - Self::MIN_WORK).max(least)
    }

    /// The most a panel may be given when `taken` of the window's
    /// `available` along its axis is already another's — the other side
    /// panel, the sidebar. 2026-09-24: the ceiling was the window's alone
    /// for each panel, and two of them together stretched the work off
    /// the screen.
    pub fn room(available: f32, taken: f32, least: f32) -> f32 {
        Self::ceiling(available - taken, least)
    }

    /// A closed panel of the given size.
    pub fn of(size: f32) -> Self {
        Self { open: false, size }
    }

    /// Make it bigger or smaller, within its bounds.
    ///
    /// The bounds are enforced here rather than where the dragging happens,
    /// because there are two places a size can come from — a border under the
    /// pointer and a keystroke — and a rule written at each is a rule that
    /// will differ at each.
    pub fn resize(&mut self, by: f32, bounds: (f32, f32)) {
        self.size = (self.size + by).clamp(bounds.0, bounds.1);
    }
}

/// The three panels around the tab.
///
/// No fourth for the status bar: it is always there, and a thing that is
/// always there has no state to keep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panels {
    /// Which face the left panel showed in M8.3. Kept and written so that
    /// an M8.3 build still reads the file; read only to make `docks` for
    /// a file that has none.
    #[serde(default)]
    pub face: Face,
    /// Where every tool is and what each dock shows. M8.3.1; `None` in a
    /// file from before, which `docks()` reads through the face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docks: Option<Docks>,
    /// The project tree, once there is one.
    pub left: Panel,
    /// The sessions, tasks, whatever a project is being watched by.
    pub right: Panel,
    /// Output that arrives, is read, and is then in the way.
    pub bottom: Panel,
    /// The borders *inside* a tool. M8.3.2; the defaults for a file from
    /// before, which is every file written until then.
    #[serde(default)]
    pub columns: Columns,

    /// Which of the Git tool's tabs was left showing.
    #[serde(default)]
    pub git_face: GitFace,
}

/// The borders inside a tool, in pixels.
///
/// Pixels rather than rems, for the reason the panels' own sizes are:
/// a person drags pixels, and a width that moved when the interface font
/// changed would fight the hand that set it. The defaults are the design
/// system's numbers — section 6.13 — and a border double-clicked comes
/// back to them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Columns {
    /// The Log's branch column, when the panel is wide enough for three.
    pub branches: f32,
    /// The Log's details column, the commit that is open.
    pub details: f32,
    /// How tall the message block under the opened commit's files is.
    ///
    /// A height rather than a share: a person drags pixels, and a
    /// one-line message and a twenty-line one want different room from
    /// the same window.
    #[serde(default = "Columns::default_message")]
    pub message: f32,
    /// The Push tab's files beside its commits — M12.3.
    #[serde(default = "Columns::default_push_files")]
    pub push_files: f32,
}

impl Columns {
    /// Section 6.13's width for the branch column.
    pub const BRANCHES: f32 = 200.0;
    /// Section 6.13's width for the details column.
    pub const DETAILS: f32 = 320.0;
    /// Enough for a summary, three lines of body and the footer.
    pub const MESSAGE: f32 = 140.0;
    /// The Push tab's files, where M8.8.4 fixed them.
    pub const PUSH_FILES: f32 = 320.0;
    /// A file tree needs a name's room; past this the commits are a strip.
    pub const PUSH_FILES_BOUNDS: (f32, f32) = (200.0, 720.0);

    /// What the branch column may be dragged between: narrow enough for
    /// short names, never so wide that the commits are a strip.
    pub const BRANCHES_BOUNDS: (f32, f32) = (120.0, 400.0);
    /// The same for the details, which holds a commit's body and files
    /// and so is allowed more.
    pub const DETAILS_BOUNDS: (f32, f32) = (200.0, 640.0);
    /// Below sixty a message is a line and a half; above four hundred
    /// there is no room left for the files it belongs to.
    pub const MESSAGE_BOUNDS: (f32, f32) = (60.0, 400.0);

    /// Move one border, clamped to what it may be.
    pub fn resize_branches(&mut self, by: f32) {
        let (min, max) = Self::BRANCHES_BOUNDS;
        self.branches = (self.branches + by).clamp(min, max);
    }

    /// The details column grows as the pointer comes back towards it,
    /// which is why the caller passes the movement already turned round.
    /// Drag the border between the files and the message.
    pub fn resize_message(&mut self, by: f32) {
        let (min, max) = Self::MESSAGE_BOUNDS;
        self.message = (self.message + by).clamp(min, max);
    }

    /// What a file written before the message had a height of its own
    /// reads back as.
    fn default_message() -> f32 {
        Self::MESSAGE
    }

    fn default_push_files() -> f32 {
        Self::PUSH_FILES
    }

    /// Drag the border between the Push tab's commits and its files, which
    /// grow as the pointer comes back towards them — the caller turns the
    /// movement round, as it does for the Log's details.
    pub fn resize_push_files(&mut self, by: f32) {
        let (min, max) = Self::PUSH_FILES_BOUNDS;
        self.push_files = (self.push_files + by).clamp(min, max);
    }

    /// Drag the border between the commits and the opened commit.
    pub fn resize_details(&mut self, by: f32) {
        let (min, max) = Self::DETAILS_BOUNDS;
        self.details = (self.details + by).clamp(min, max);
    }
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            branches: Self::BRANCHES,
            details: Self::DETAILS,
            message: Self::MESSAGE,
            push_files: Self::PUSH_FILES,
        }
    }
}

/// What a project has been asked to run. M8.4.
///
/// The line the Run button repeats, and a short history for the list to
/// offer above what the project declares. **A run is not saved and this
/// is:** a process is not state, but the command is a thing a person
/// chose, and choosing it again after a restart is work they already did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commands {
    /// What Run repeats.
    #[serde(default)]
    pub last: Option<String>,
    /// What was run lately, newest first.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// What a project was last searched for. M8.7.
///
/// **The query is state and the answer is not.** A search is a walk of
/// the disk that costs milliseconds and can be run again; what a person
/// chose — the words, the three toggles, the mask — is work they did, and
/// coming back to a project to find the field empty is that work thrown
/// away. Nothing about the *hits* is written down: they belong to a tree
/// as it was, and a restored list would be an answer to a question about
/// files that may since have changed.
///
/// A separate type from `desktop::search::Query`, deliberately, for the
/// reason `Commands` is separate from the Output panel's own state: this
/// is the wire format, and a desktop type in it is a rename away from
/// making every project's file unreadable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    /// What was being looked for.
    #[serde(default)]
    pub query: String,
    /// Whether `Alpha` and `alpha` were different.
    #[serde(default)]
    pub match_case: bool,
    /// Whether a word had to be whole.
    #[serde(default)]
    pub whole_word: bool,
    /// Whether the query was a pattern.
    #[serde(default)]
    pub regex: bool,
    /// Which files it looked in.
    #[serde(default)]
    pub mask: String,
    /// What was searched before, newest first — M8.9's Find in Files,
    /// `alt-up` and the query's chevron. A query is written here when it is
    /// used, not per keystroke.
    #[serde(default)]
    pub history: Vec<String>,
    /// The masks used before, newest first — the mask field's chevron.
    #[serde(default)]
    pub masks: Vec<String>,
    /// Whether the mask is turned off. **The mask is kept while it is off**:
    /// a person who unticks `*.rs` to look everywhere for a moment ticks it
    /// back, and finding the field empty is the work thrown away. M8.9.
    #[serde(default)]
    pub mask_off: bool,
}

impl Commands {
    /// How many lines the history keeps.
    ///
    /// Short on purpose: it sits above what the project declares in the
    /// list, and a hundred rows of history would bury the recipes it is
    /// meant to sit above.
    pub const REMEMBERED: usize = 10;

    /// Put a line at the front, and make it what Run repeats.
    ///
    /// A repeat **moves** rather than duplicates: a person who runs `just
    /// check` forty times has one row, not forty. A blank line is not a
    /// command anywhere in Guild, and is not one here either.
    pub fn remember(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.recent.retain(|kept| kept != line);
        self.recent.insert(0, line.to_owned());
        self.recent.truncate(Self::REMEMBERED);
        self.last = Some(line.to_owned());
    }
}

/// What the left panel shows: one of three, chosen on the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    /// The project's files.
    #[default]
    Explorer,
    /// What the branch has that the last commit does not.
    Changes,
    /// The commits.
    Log,
}

impl Face {
    /// Every face, in the rail's order.
    pub const ALL: &'static [Face] = &[Face::Explorer, Face::Changes, Face::Log];
}

/// Which of the Git tool's two tabs is showing.
///
/// A tool with tabs inside it is new in M8.3.4: the dock head already
/// draws tabs, and they mean *which tool*, so a second row inside the
/// body means *which face of this tool*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitFace {
    /// The commits.
    #[default]
    Log,
    /// What the window told git, and what git said back.
    Console,
}

/// A tool window — a thing a dock shows. Decision 1 of M8.3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// The project's files.
    Explorer,
    /// The changes, ticked, and the message: the Commit tool window.
    Commit,
    /// Git: the log, and the console of what the window told git.
    ///
    /// **Named `log` on the wire, and it must stay that way.** The
    /// variant is that string inside `docks.placed` in every project's
    /// file; renaming it makes serde fail the *whole file*, and
    /// `Store::load` then answers a default — the project would lose its
    /// sessions, its tabs and its panel sizes because a tool was renamed.
    #[serde(rename = "log")]
    Git,
    /// A command, its log and its exit code: M8.4's Output panel.
    ///
    /// **Retired between M8.3.4 and M8.4**, when it was a button that
    /// opened *Builds, tests and logs will be shown here* and nothing
    /// else. The variant stayed through the retirement because it is the
    /// string `"output"` inside `docks.placed` in every project's file
    /// and deserialising is all or nothing — and it is what let the tool
    /// come back without a migration of the files written meanwhile.
    Output,
    /// Text across the project: M8.7's Search tool.
    ///
    /// `"search"` on the wire, which `snake_case` gives without help.
    /// **The order of the variants above it is not free** — `Git` is the
    /// string `"log"` on disk, and every one of them is a key inside
    /// `docks.placed`.
    Search,
    /// What the language servers say is wrong, across the project.
    /// M8.8.5, and `cmd-6` is the common IDE number for it.
    ///
    /// **Appended, so that no variant above it moves.** Added by the
    /// seams commit of M8.8 and drawing nothing until M8.8.5 fills it,
    /// which is one frame of *Nothing here* against six branches all
    /// editing this enum.
    Problems,
    /// What was waiting to go to a remote — M8.8.4's Push tab, a tool from
    /// M12.3 until M8.3.6 made Push Commits a window of its own.
    ///
    /// **Retired, and kept**: a file of those passes names it, and serde
    /// fails a whole file on a variant it does not know. [`Tool::RETIRED`]
    /// takes it out of every dock that is read.
    Push,
    /// The usages of a name, a tab per search: the Find tool, which
    /// `alt-f7` opens. M8.15.
    ///
    /// **Appended after the retired `Push`**, so that no variant above it
    /// moves; `"find"` on the wire.
    Find,
    /// Independent ordinary shells, one tab set per project/worktree.
    Terminal,
}

impl Tool {
    /// Every tool a person can reach, in the order the status bar names
    /// them within a dock.
    ///
    /// **Hand-kept, and the compiler says nothing about a gap.**
    /// `Docks::adopt_missing` walks this list and the status bar draws
    /// its buttons from it, so a variant that exists and is not here is
    /// a panel reachable by its stroke with no button anywhere.
    pub const ALL: &'static [Tool] = &[
        Tool::Explorer,
        Tool::Commit,
        Tool::Git,
        Tool::Output,
        Tool::Terminal,
        Tool::Search,
        Tool::Problems,
        Tool::Find,
    ];

    /// Tools that no longer exist, taken out of a dock set whenever it is
    /// read — the variants stay for the files that name them.
    pub const RETIRED: &'static [Tool] = &[Tool::Push];

    /// A tool the status bar names only while it is up — a Find window
    /// has no button until something opens it.
    ///
    /// Search, since 2026-09-23: the person read its button as a second
    /// `cmd-shift-f`. The popup is the way in, and its *Open in Find
    /// Window* the way here; the tool, its dock and its place on disk
    /// stay as they were.
    pub fn on_demand(self) -> bool {
        matches!(self, Tool::Search)
    }

    /// The defaults: files and the commit at the left, git at the
    /// bottom.
    pub fn default_dock(self) -> Dock {
        match self {
            Tool::Explorer | Tool::Commit => Dock::Left,
            // Search is at the bottom with Git and the Output, because
            // what it draws is a wide list of lines: Find in Files
            // belongs there, and a result row in a 300-pixel left dock is
            // a path with no line beside it.
            // Find too: the Find tool is a wide list of lines with the
            // file beside them, which a side dock cannot hold.
            Tool::Git
            | Tool::Output
            | Tool::Terminal
            | Tool::Search
            | Tool::Problems
            | Tool::Find => Dock::Bottom,
            // Retired, so never placed; an answer is still owed.
            Tool::Push => Dock::Right,
        }
    }
}

/// Where a tool lives: one of the three panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dock {
    /// The panel at the window's left edge.
    Left,
    /// The panel at the right edge.
    Right,
    /// The panel along the bottom, above the status bar.
    Bottom,
}

impl Dock {
    /// The three, in the order the status bar's groups go.
    pub const ALL: &'static [Dock] = &[Dock::Left, Dock::Bottom, Dock::Right];

    /// Whether a stretch the arrow's way makes this panel bigger (`1.0`) or
    /// smaller (`-1.0`), or `None` for an arrow along an edge it does not
    /// have. The border goes the way the arrow points, so the arrow that
    /// grows the left panel shrinks the right one.
    pub fn stretched(self, toward: Stretch) -> Option<f32> {
        match (self, toward) {
            (Dock::Left, Stretch::Right) | (Dock::Right, Stretch::Left) => Some(1.0),
            (Dock::Left, Stretch::Left) | (Dock::Right, Stretch::Right) => Some(-1.0),
            (Dock::Bottom, Stretch::Top) => Some(1.0),
            (Dock::Bottom, Stretch::Bottom) => Some(-1.0),
            _ => None,
        }
    }
}

/// Which way a panel is stretched from the keyboard — Stretch to Left,
/// Right, Top and Bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stretch {
    /// `ctrl-alt-left`.
    Left,
    /// `ctrl-alt-right`.
    Right,
    /// `ctrl-alt-up`.
    Top,
    /// `ctrl-alt-down`.
    Bottom,
}

impl Stretch {
    /// How many characters, or lines, one stroke moves a border: five,
    /// horizontally and vertically alike.
    pub const CHARACTERS: f32 = 5.0;

    /// One stroke's move, in pixels, for the interface's character width
    /// and line height — a character is the width of a label holding *W*.
    pub fn step(self, character: f32, line: f32) -> f32 {
        Self::CHARACTERS
            * match self {
                Stretch::Left | Stretch::Right => character,
                Stretch::Top | Stretch::Bottom => line,
            }
    }
}

/// Every tool's dock, in the order the dock's buttons draw them, and the
/// tool each dock shows — or showed when it was last open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Docks {
    placed: Vec<(Tool, Dock)>,
    shown: Vec<(Dock, Tool)>,
}

impl Default for Docks {
    fn default() -> Self {
        Self {
            placed: Tool::ALL.iter().map(|t| (*t, t.default_dock())).collect(),
            shown: vec![(Dock::Left, Tool::Explorer), (Dock::Bottom, Tool::Git)],
        }
    }
}

impl Docks {
    /// The tools in a dock, in the order their buttons go.
    pub fn tools_in(&self, dock: Dock) -> Vec<Tool> {
        self.placed
            .iter()
            .filter(|(_, d)| *d == dock)
            .map(|(t, _)| *t)
            .collect()
    }

    /// Whether a dock holds the tool — M12.3. Push is on the tab strip
    /// until a person moves it; every other tool is always in a dock.
    pub fn docked(&self, tool: Tool) -> bool {
        self.placed.iter().any(|(placed, _)| *placed == tool)
    }

    /// Where a tool is. A tool the file does not name — one added after
    /// the file was written — is where it is by default.
    pub fn dock_of(&self, tool: Tool) -> Dock {
        self.placed
            .iter()
            .find(|(t, _)| *t == tool)
            .map_or(tool.default_dock(), |(_, d)| *d)
    }

    /// The tool a dock shows, or would show if open.
    pub fn shown(&self, dock: Dock) -> Option<Tool> {
        self.shown.iter().find(|(d, _)| *d == dock).map(|(_, t)| *t)
    }

    /// Make a tool the one its dock shows.
    pub fn show(&mut self, tool: Tool) {
        let dock = self.dock_of(tool);
        self.shown.retain(|(d, _)| *d != dock);
        self.shown.push((dock, tool));
    }

    /// Take a retired tool out of the docks entirely.
    ///
    /// A dock **showing** it falls back to whatever else it holds rather
    /// than coming up on *Nothing here* — the hole M8.3.1 met when a move
    /// left a dock empty, and closed the same way.
    pub fn retire(&mut self, tool: Tool) {
        self.placed.retain(|(placed, _)| *placed != tool);
        for (dock, shown) in std::mem::take(&mut self.shown) {
            if shown != tool {
                self.shown.push((dock, shown));
            } else if let Some(next) = self.tools_in(dock).first().copied() {
                self.shown.push((dock, next));
            }
        }
    }

    /// Take in every tool the file does not name, at its default dock.
    ///
    /// The inverse of [`Docks::retire`], and the reason a tool can come
    /// back — or arrive for the first time — without a migration. A
    /// [`Tool::RETIRED`] tool is taken out first.
    /// [`Docks::dock_of`] already answers `default_dock` for a tool a
    /// file does not name, but **the list has to know too**: the status
    /// bar draws a dock's buttons from `tools_in`, so a tool missing
    /// there is one reachable by its shortcut with no button anywhere —
    /// a tool that exists and cannot be found.
    pub fn adopt_missing(&mut self) {
        for tool in Tool::RETIRED {
            self.retire(*tool);
        }
        for tool in Tool::ALL {
            if !self.placed.iter().any(|(placed, _)| placed == tool) {
                self.placed.push((*tool, tool.default_dock()));
            }
        }
    }

    /// Move a tool: out of its dock's order and last in the new one,
    /// which shows it; the old dock shows the first tool it has left, or
    /// nothing. Moving a tool to where it is changes nothing.
    pub fn move_tool(&mut self, tool: Tool, dock: Dock) {
        // **A tool no dock holds arrives rather than moves** — M12.3's Push,
        // dropped from the tab strip: there is no dock it leaves, and its
        // default dock must not read as *already there*.
        if !self.docked(tool) {
            self.placed.push((tool, dock));
            self.show(tool);
            return;
        }
        let from = self.dock_of(tool);
        if from == dock {
            return;
        }
        self.placed.retain(|(t, _)| *t != tool);
        self.placed.push((tool, dock));
        self.shown.retain(|(d, _)| *d != from);
        if let Some(next) = self.tools_in(from).first() {
            self.shown.push((from, *next));
        }
        self.show(tool);
    }

    /// M8.3's face, as docks: the Log was a face of the left panel and is
    /// a bottom tool now, so a project left on the Log comes back on it
    /// there.
    pub fn from_face(face: Face) -> Self {
        let mut docks = Self::default();
        match face {
            Face::Explorer => docks.show(Tool::Explorer),
            Face::Changes => docks.show(Tool::Commit),
            Face::Log => docks.show(Tool::Git),
        }
        docks
    }
}

impl Panels {
    /// Bring a layout read from a file up to date: docks made from an M8.3
    /// face, a tool the file does not name adopted, a retired one taken
    /// out, and a dock left open with nothing in it closed. One rule for a
    /// project's file and for the window's settings.
    pub fn tidy(&mut self) {
        if self.docks.is_none() {
            self.docks = Some(Docks::from_face(self.face));
            if self.face == Face::Log && self.left.open {
                self.left.open = false;
                self.bottom.open = true;
            }
        }
        // A tool the file does not name arrives at its default dock —
        // M8.4's Output for every file written while it was retired, and
        // whatever comes after it for every file written today.
        self.docks_mut().adopt_missing();

        // A dock open with no tool in it is a hole where a tool was: a
        // file written before a move closed emptied docks can say so.
        let docks = self.docks();
        for (dock, panel) in [
            (Dock::Left, &mut self.left),
            (Dock::Right, &mut self.right),
            (Dock::Bottom, &mut self.bottom),
        ] {
            if panel.open && docks.tools_in(dock).is_empty() {
                panel.open = false;
            }
        }
    }

    /// Where the tools are: from the file, or from M8.3's face when the
    /// file has no docks yet.
    pub fn docks(&self) -> Docks {
        self.docks
            .clone()
            .unwrap_or_else(|| Docks::from_face(self.face))
    }

    /// The docks to change, made from the face first when the file had
    /// none — so the change is written, not lost against a `None`.
    pub fn docks_mut(&mut self) -> &mut Docks {
        if self.docks.is_none() {
            self.docks = Some(Docks::from_face(self.face));
        }
        self.docks.as_mut().expect("just filled")
    }
}

impl Default for Panels {
    /// Closed, at the widths section 5 of the design system gives.
    fn default() -> Self {
        Self {
            face: Face::default(),
            docks: None,
            left: Panel::of(230.0),
            right: Panel::of(330.0),
            bottom: Panel::of(198.0),
            columns: Columns::default(),
            git_face: GitFace::default(),
        }
    }
}

/// Everything Guild knows about one project.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    /// View ownership retained independently of asynchronously loaded run snapshots.
    /// This is placement metadata, never runtime or mutation authority.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub process_origins: std::collections::BTreeMap<crate::workflow::RunId, SessionId>,
    /// The sessions started in this project, open or closed.
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    /// The tabs that were open, in the order they were in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<OpenTab>,
    /// Which panels were showing, and how big.
    #[serde(default)]
    pub panels: Panels,
    /// Which directories of the tree were expanded, relative to the
    /// project, shallowest first. A person opens `crates/desktop/src` in
    /// the repository they read, and finds it open the next time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expanded: Vec<PathBuf>,
    /// Which of `tabs` was in front, by position. Absent from a file
    /// written before M8.1, and read back as none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<usize>,
    /// What each workspace arranged while holding this root.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub seats: std::collections::BTreeMap<crate::WorkspaceId, SeatState>,
    /// What this project has been asked to run. M8.4; absent from every
    /// file written before it, and read back as nothing remembered.
    #[serde(default)]
    pub commands: Commands,
    /// What this project was last searched for. M8.7; absent from every
    /// file written before it, and read back as an empty field.
    #[serde(default)]
    pub search: Search,
    /// The files opened lately, newest first, for M8.8.2's `cmd-e`.
    ///
    /// **Declared by the seams commit, filled by the branch.** This is a
    /// wire format: `Store::load` answering `T::default()` on a parse
    /// failure is the data-loss path this repository has recorded, so a
    /// field is added once, before anything writes it, rather than
    /// arriving later and making every file written meanwhile unreadable
    /// by an older build.
    #[serde(default)]
    pub recent_files: Vec<std::path::PathBuf>,
    /// Where this project's dependencies were unpacked, for M8.8.3's
    /// External Libraries. Declared here for the same reason.
    #[serde(default)]
    pub external_roots: Vec<std::path::PathBuf>,
}

/// The person-arranged state of one root in one workspace.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatState {
    /// Tabs in strip order.
    #[serde(default)]
    pub tabs: Vec<OpenTab>,
    /// The tab in front, by position.
    #[serde(default)]
    pub active: Option<usize>,
    /// Expanded directories, relative to the root.
    #[serde(default)]
    pub expanded: Vec<PathBuf>,
    /// The areas this seat was divided into — M12.8. Absent from every
    /// file written before it and from every seat never divided; read
    /// back through `areas_or_one` as one area holding `tabs`. `tabs`
    /// and `active` keep mirroring an area (decision 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub areas: Option<crate::areas::SavedAreas>,
}

impl SeatState {
    /// The seat's areas: the ones written, or one holding `tabs`.
    pub fn areas_or_one(&self) -> crate::areas::SavedAreas {
        match &self.areas {
            Some(saved) => saved.clone().normalised(),
            None => crate::areas::SavedAreas::one(self.tabs.clone(), self.active),
        }
    }

    /// A seat to write: the mirror in the old fields, the areas only when
    /// there is a division — decision 5.
    pub fn from_areas(saved: crate::areas::SavedAreas, expanded: Vec<PathBuf>) -> Self {
        let mirror = saved.mirror();
        let divided = saved.areas.ids().len() > 1;
        Self {
            tabs: mirror.tabs,
            active: mirror.active,
            expanded,
            areas: divided.then_some(saved),
        }
    }
}

impl ProjectState {
    /// Read a seat, falling back to legacy top-level state for the first workspace.
    pub fn seat(&self, workspace: crate::WorkspaceId, first: crate::WorkspaceId) -> SeatState {
        self.seats.get(&workspace).cloned().unwrap_or_else(|| {
            if workspace == first {
                SeatState {
                    tabs: self.tabs.clone(),
                    active: self.active,
                    expanded: self.expanded.clone(),
                    areas: None,
                }
            } else {
                SeatState::default()
            }
        })
    }

    /// Replace one seat, migrating legacy state before the first write.
    pub fn set_seat(
        &mut self,
        workspace: crate::WorkspaceId,
        first: crate::WorkspaceId,
        seat: SeatState,
    ) {
        if !self.tabs.is_empty() || self.active.is_some() || !self.expanded.is_empty() {
            self.seats.entry(first).or_insert_with(|| SeatState {
                tabs: std::mem::take(&mut self.tabs),
                active: self.active.take(),
                expanded: std::mem::take(&mut self.expanded),
                areas: None,
            });
        }
        self.seats.insert(workspace, seat);
    }

    /// Forget one workspace's arranged state.
    pub fn forget_seat(&mut self, workspace: crate::WorkspaceId) {
        self.seats.remove(&workspace);
    }
}

impl Normalise for ProjectState {
    /// A file from M8.3 has a face and no docks. The docks are made from
    /// the face so that the next save writes them; and a project left on
    /// the Log — a face of the left panel then, a bottom tool now — comes
    /// back with the bottom dock open on it and the left closed, since
    /// the left was open for the Log and not for the Explorer.
    fn normalise(&mut self) {
        self.panels.tidy();
    }
}

/// Where a project's state lives.
pub fn dir(project: &Path) -> PathBuf {
    project.join(DIRECTORY)
}

/// The file inside it.
fn file(project: &Path) -> PathBuf {
    // The queue and the removal gate must address the same file even when
    // the worktree was opened through a symlink.
    dir(&removal_key(project)).join("state.json")
}

/// Read what is known about a project.
///
/// A directory that has gone — moved, deleted, on a disk nobody mounted —
/// reads as a project with nothing in it rather than as a failure. The
/// window has to open either way. So does a file that is there and could
/// not be read — too many open files, a failing disk — and that one is
/// then not saved over for the rest of the run: [`save`] refuses it with
/// [`StoreError::NotRead`], so the default read here never replaces it.
pub fn load(project: &Path) -> ProjectState {
    Store::<ProjectState>::new(file(project))
        .load()
        .unwrap_or_default()
}

/// Read what is known about a project for a change built on it, or `None`
/// when the read is not one to build on: the folder is not there on both
/// sides of the read, or its state is there and cannot be read.
///
/// [`load`] answers an empty state for a folder that is away — a volume
/// not mounted — and [`save`] looks at the folder only when it is called:
/// a change made between a read of the absent folder and its return would
/// write that emptiness over what the folder holds. A caller given `None`
/// keeps its change and writes it later. A folder that goes and comes
/// back within the read itself is not caught.
pub fn load_for_change(project: &Path) -> Option<ProjectState> {
    if !project.is_dir() {
        return None;
    }
    let state = Store::<ProjectState>::new(file(project)).load().ok()?;
    project.is_dir().then_some(state)
}

/// Write what is known about a project.
///
/// Through the same [`Store`] as everything else, so this inherits the
/// write-beside-and-rename and the backup rather than repeating them —
/// and queued rather than waited for, because the window calls this on
/// every tab change. [`crate::store::flush`] is what waits.
///
/// **A project whose folder is gone is not written.** Making `.guild/`
/// makes the folder above it, and a worktree removed outside the window
/// came back as a directory holding nothing but Guild's state — which the
/// window then took for a folder that was still there. What the global
/// workspace file keeps of the project is untouched.
pub fn save(project: &Path, state: &ProjectState) -> Result<(), StoreError> {
    // Admission covers both the synchronous .gitignore and the queued JSON.
    // A removal marks this path before draining the writer, so neither can
    // recreate .guild while git is walking the directory.
    let removing = removals().lock().unwrap_or_else(|e| e.into_inner());
    if removing.contains(&removal_key(project)) {
        return Err(removal_busy(project));
    }
    if !project.is_dir() {
        tracing::debug!(path = %project.display(), "the project's folder is gone; its state is not written");
        return Ok(());
    }
    ignore_itself(project);
    let result = Store::new(file(project)).save_soon(state);
    drop(removing);
    result
}

/// Forget only the final holder's view state, including fallback and commit draft.
/// Drain and exclude the existing writer so no older queued snapshot can recreate
/// these files. Configured workflows, journals and resource owners are retained.
pub fn forget_views(project: &Path) -> Result<(), StoreError> {
    let _removal = prepare_removal(project)?;
    for name in [
        "state.json",
        "state.json.bak",
        "state.json.tmp",
        "COMMIT_MESSAGE",
    ] {
        let path = dir(project).join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(StoreError::Write { path, source }),
        }
    }
    Ok(())
}

/// Put a `.gitignore` in the directory that ignores everything in it.
///
/// Written once, when the directory is made, and never rewritten: a person
/// who deliberately un-ignores part of it should not have Guild put it back
/// on the next save.
///
/// A failure here is not a failure to save. The worst it costs is a dirty
/// `git status`, and refusing to keep a person's sessions over that would
/// be the wrong trade.
fn ignore_itself(project: &Path) {
    let directory = dir(project);
    let ignore = directory.join(".gitignore");
    if ignore.exists() {
        return;
    }

    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }

    let _ = std::fs::write(
        &ignore,
        "# Guild's own state for this project: open tabs, sessions, what\n\
         # they cost. All of it is one person's, none of it is the project's.\n\
         *\n",
    );
}

/// Hold a project's state writes while its worktree is being removed.
///
/// Called on the git worker, after the window closes every seat. New saves
/// are refused while the last admitted snapshot drains; other roots keep
/// saving. No files are deleted here, and dropping the guard after a git
/// refusal lets the surviving project's state be saved again.
pub fn prepare_removal(project: &Path) -> Result<Removal, StoreError> {
    let key = removal_key(project);
    let mut removing = removals().lock().unwrap_or_else(|e| e.into_inner());
    if !removing.insert(key.clone()) {
        return Err(removal_busy(project));
    }
    let removal = Removal { key };
    drop(removing);
    crate::store::flush_path(&file(project))?;
    Ok(removal)
}

fn removals() -> &'static Mutex<HashSet<PathBuf>> {
    static REMOVALS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    REMOVALS.get_or_init(Mutex::default)
}

fn removal_key(project: &Path) -> PathBuf {
    project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf())
}

fn removal_busy(project: &Path) -> StoreError {
    StoreError::Write {
        path: file(project),
        source: std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "the worktree is being removed",
        ),
    }
}

/// Released when the removal succeeds or git refuses it.
pub struct Removal {
    key: PathBuf,
}

impl Drop for Removal {
    fn drop(&mut self) {
        removals()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}
