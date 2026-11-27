//! The menu the right button opens, and what goes in it — and, since
//! M12.7, every floating menu in the window besides: the tab strip's add
//! button, its hidden-tabs chevron and its `⋮`, a panel head's `⋮`, the
//! agents list, the title bar's project popup, the Log's filter menus
//! and Find in Files' history.
//!
//! **Guild's own, not `gpui-component`'s.** Task 0 of M8.3.2 measured the
//! component's version for the right button — an action item dispatched
//! all the way to the root's handler, the arrows and `enter` and `escape`
//! walked it — and it served until M12.2, when it was replaced: the
//! component fixes its rows at 26 pixels and its text at 0.875 rem in its
//! own code, where no theme reaches. What draws now is ours:
//! `render_context_menu` places one card by `Anchor`, `Root.context_menu:
//! Option<Open>` holds the one menu that may be up, and `Root::open_menu`
//! is the single door every surface opens it through.
//!
//! **A menu belongs to a surface, not to a row.** One card behind one
//! field of state is one menu for the whole window, so a menu per row
//! would be many rows sharing that one state. So the row records what
//! the pointer was on — `on_mouse_down` with the right button — and the
//! surface's builder reads it a frame later, when `open_context_menu`
//! runs.
//!
//! **Most items are deeds on the target, not actions.** The plan's
//! decision 2 offered both, and the menus turn out to want the deed: a
//! tab's *Close* has to close the tab under the pointer, and an action
//! would close the tab in front — a person who right-clicks a background
//! tab and picks Close would lose the wrong file. So the items are deeds
//! by default, the keymap keeps actions of its own for what a stroke
//! would mean, and no item promises a stroke it would not honour. **The
//! exceptions carry `Deed::Action(Named)` instead** — the tab strip's
//! `⋮`, a panel head's folded toolbar buttons, and the project popup's
//! *Open…* and *New Project* — because each of those reaches the window
//! itself, or a control already bound as an action, rather than the row
//! it hangs from. M12.7.
//!
//! **A file per surface, since M8.8.0.** The tree, the tabs, the Commit
//! panel and the Git tool each own their half of `Deed`, their entries
//! and their `run_*` — because three of M8.8's six branches add items,
//! and one enum with one `entries` and one `run_deed` is three long
//! matches all three would edit. M12.7 followed the same rule for every
//! menu it moved onto the engine: `strip.rs`, `dock.rs`, `agents.rs`,
//! `projects.rs`, `log_filter.rs`, `history.rs`. What stays here is what
//! they all share: `Subject`, `Target`, `Entry`, the outer `Deed`, the
//! two dispatchers, the one builder every trigger calls
//! (`entries_for_target`), and the rule that decides whether an item may
//! show a stroke.

pub mod agents;
pub mod changes;
pub mod column;
pub mod dock;
pub mod gutter;
pub mod history;
pub mod log;
pub mod log_filter;
pub mod open_in;
pub mod sessions;
pub mod strip;
pub mod tabs;
pub mod terminal;
pub mod tree;
pub mod workspace;

use std::path::PathBuf;

use gpui::{ClipboardItem, Context, SharedString, Window};
use guild_session::{Dock, SessionId};

use crate::TabId;
use crate::actions::keymap::Named;
use crate::find_in_files::Field;
use crate::i18n::Text;

pub use changes::ChangeDeed;
pub use column::{ProjectDeed, WorktreeDeed};
pub use dock::DockDeed;
pub use log::LogDeed;
pub use log_filter::FilterDeed;
pub use open_in::OpenInDeed;
pub use tabs::TabDeed;
pub(crate) use terminal::TerminalAt;
pub use terminal::TerminalDeed;
pub use tree::TreeDeed;
pub use workspace::WorkspaceDeed;

/// How a directory of the tree stands to the roots Guild has open —
/// GLD-057.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRoot {
    /// The directory is itself an open root: a member of a group folder,
    /// a worktree opened from inside its checkout.
    Is,
    /// An open root lies somewhere below it.
    Holds,
}

/// What a menu is about, as far as its items are concerned.
///
/// The facts the rule below turns on, and nothing else: no path, no tab
/// identity. Those belong to the window, which has to act on them;
/// **this** is the part a test can read without one, and the part that
/// says what a person will see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// A file in the tree.
    TreeFile,
    /// A directory in the tree, and which way it is folded now.
    TreeDirectory {
        /// Open, so the item offers to close it.
        expanded: bool,
        /// The project's own row — the tree's empty path.
        ///
        /// **It offers nothing that writes to it.** *Delete* would hand
        /// the directory the window has open to the Trash and
        /// *Rename…* would rename it; no confirmation makes either a
        /// reasonable thing to offer, which is why the keyboard route
        /// has refused the delete since it was written. A row is a row
        /// to the menu, so the flag is how it tells them apart.
        root: bool,
        /// Whether the directory is, or holds, another root Guild has
        /// open — GLD-057. Its writes are drawn disabled.
        ///
        /// [`Target::subject`] cannot know it, since a target is a row and
        /// nothing about the desk; the window answers it where the menu is
        /// built, from the projects it holds.
        open_root: Option<OpenRoot>,
    },
    /// A row under External Libraries: somebody else's source, which
    /// the window reads and never writes. M8.8.3.
    Library {
        /// A directory, which folds rather than opens.
        is_dir: bool,
        /// Open, when it is a directory.
        expanded: bool,
    },
    /// A tab in the strip.
    Tab {
        /// Whether a file stands behind it: a session tab has none, and
        /// neither has the diff of a whole tree.
        has_file: bool,
        /// Whether a session is in it — the one kind of tab a person can
        /// name. M12.1.
        session: bool,
        /// Where it sits in the strip, and how many tabs the strip
        /// shows.
        ///
        /// **One pair of facts rather than three flags.** *Close the
        /// others*, *close the ones to the left* and *close the ones to
        /// the right* are the same question asked three ways, and the
        /// answer to all three is a place in a row. An `alone` beside an
        /// `of` would be two truths free to disagree — M8.8.1 replaced
        /// it for that reason and not for the new items alone.
        index: usize,
        /// How many tabs the strip shows.
        of: usize,
        /// Whether an arrow from the tab's own area reaches another area
        /// to the left, right, above and below — the areas *Move to Area
        /// ▸* names by where they are. M12.8, decision 21.
        neighbours: [bool; 4],
    },
    /// A row of the Commit panel: one changed file.
    Change {
        /// Ticked now, so the item offers to leave it out.
        ticked: bool,
        /// In the middle of a merge with both sides changed. A commit
        /// cannot take it, so it is never ticked and never untickable.
        unmerged: bool,
        /// A directory git did not descend into: it has no diff.
        is_dir: bool,
        /// A file git does not know about — the *Unversioned* section.
        ///
        /// **Two items turn on it and they point opposite ways.** *Add
        /// to VCS* is only for a file git has never seen, and
        /// *Rollback* is only for one it has: there is nothing in
        /// `HEAD` or in the index for an unversioned row to go back to,
        /// and git refuses the path outright. M8.8.4.
        unversioned: bool,
        /// Deleted already — git's `D`.
        ///
        /// *Delete* is not offered on it: the file is by definition no
        /// longer on the disk, so the item would open a warning sheet
        /// whose only possible ending is the platform refusing to trash
        /// a path that is not there. A state a person can reach and not
        /// leave, in its smallest shape.
        gone: bool,
    },
    /// A section head of the Commit panel.
    ChangeSection,
    /// A branch in the Log's first column.
    Branch {
        /// Already the one being listed, so *show its log* would do
        /// nothing.
        listed: bool,
    },
    /// A commit in the Log's list.
    Commit,
    /// A repository's head in the column — M12. Its items are about the
    /// repository, not about any one of its worktrees.
    Repository {
        /// Whether any of its worktrees is a root of the window's: with
        /// none, there is no `HEAD` to make a worktree from.
        opened: bool,
    },
    /// A worktree's row in the column — M12.
    Worktree {
        /// The repository's own checkout, which git will not remove.
        primary: bool,
        /// A root of the window's, rather than one git lists that nobody
        /// has opened: only an open one has anything to close.
        opened: bool,
        /// Somebody has locked it, so git refuses to remove it until the
        /// lock is taken off — M12 task 18.
        locked: bool,
        /// Its branch has a merge request the row shows — GLD-063.
        mr: bool,
    },
    /// A terminal, a tab's or the Output panel's — M5.12.
    Terminal {
        /// Something is selected that a Copy would take.
        selection: bool,
    },
    /// A named workspace in the sidebar switcher.
    Workspace { last: bool },
    /// The tab strip's `⋮`: actions aimed at the window — M12.7 — and
    /// since M12.8 the area's, which ask how many areas the seat has:
    /// the last one is offered and not taken (decision 9).
    TabActions {
        /// The seat in front's areas; one when it is undivided.
        areas: usize,
    },
}

/// What the pointer was on when the menu was asked for.
///
/// The window's own record: the subject above plus what it takes to act
/// — the path, and which tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    ProjectSort {
        workspace: guild_session::WorkspaceId,
    },
    /// A row of the column's Sessions mode.
    SessionRow {
        key: crate::session_list::Key,
    },
    /// The Sessions mode's filter, over `workspace`'s page, with the
    /// repositories it offered when it opened — a pick is resolved against
    /// these, not against an order that may have moved since.
    SessionFilter {
        workspace: guild_session::WorkspaceId,
        projects: Vec<(String, String)>,
    },
    WorktreeActivity {
        seat: guild_session::Seat,
        return_focus: Option<gpui::FocusHandle>,
    },
    TerminalJobScopes {
        popup: gpui::FocusHandle,
    },

    ActionBarAdd {
        source: crate::view::settings::SettingsHost,
        return_focus: Option<gpui::FocusHandle>,
    },
    Setting(crate::view::settings::choices::Value),
    /// A row of the file tree, by its path relative to the project.
    TreeRow {
        path: PathBuf,
        is_dir: bool,
        expanded: bool,
    },
    /// A row under one of the project's external roots, by that root's
    /// place in the list and the path within it. M8.8.3.
    LibraryRow {
        at: usize,
        path: PathBuf,
        is_dir: bool,
        expanded: bool,
    },
    /// A tab, by its identity, with the file behind it when it has one
    /// and its place in the strip.
    Tab {
        id: TabId,
        path: Option<PathBuf>,
        /// Whether a session is in it.
        session: bool,
        /// Its place among the tabs the strip shows, and how many those
        /// are. *To the left* and *to the right* are deeds about a
        /// place, so the place travels with the target.
        index: usize,
        of: usize,
        /// Which areas an arrow from its own area reaches — left, right,
        /// up, down — for *Move to Area ▸*. M12.8.
        neighbours: [bool; 4],
    },
    /// A row of the Commit panel, by its path and its group.
    Change {
        path: PathBuf,
        ticked: bool,
        unmerged: bool,
        is_dir: bool,
        unversioned: bool,
        gone: bool,
    },
    /// A section head of the Commit panel. It has no path: its items
    /// are about the whole panel.
    ChangeSection,
    /// A branch of the Log, by the name `git branch` printed.
    Branch {
        name: String,
        listed: bool,
    },
    /// A commit of the Log, by its hash, with what its menu copies.
    Commit {
        sha: String,
        short: String,
        summary: String,
    },
    /// A repository's head in the column — M12, by an opened root of it:
    /// which one does not matter, because git reaches the repository from
    /// any of its worktrees. `None` when none of them is open.
    Repository {
        /// The workspace whose page the head belongs to.
        workspace: guild_session::WorkspaceId,
        /// Every open root grouped under this repository head.
        projects: Vec<guild_session::ProjectId>,
        /// What a name given to it is kept by — M12.2.
        key: PathBuf,
        /// Its folder: the repository's own checkout, or the directory.
        folder: PathBuf,
        /// What it is called now.
        name: String,
    },
    /// A project group's header in the column, by its workspace and id.
    ProjectGroup {
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
    },
    /// A project group header's `+` — GLD-055: the workspace's projects
    /// that are not in the group, by key, as they were when it opened.
    ProjectGroupAdd {
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        /// Each project's key, name, and the group it is in now.
        choices: Vec<column::AddChoice>,
    },
    /// A worktree's row in the column, by its folder — M12. The project is
    /// `None` for one git lists that nobody has opened.
    Worktree {
        path: PathBuf,
        project: Option<guild_session::ProjectId>,
        primary: bool,
        locked: bool,
        /// The row showed a merge request when it was pressed — GLD-063.
        /// Its deeds take the URL the row shows when they run.
        mr: bool,
    },
    /// A terminal, by the tab it is in or the run it shows — M5.12.
    Terminal {
        at: TerminalAt,
        selection: bool,
    },
    /// A file's line numbers — M12.9: the file, relative to its project,
    /// and whether an Annotate tab of it is open. Data, like `NewTab`.
    Gutter {
        path: PathBuf,
        annotated: bool,
        file: gpui::Entity<crate::file_tab::FileTab>,
    },
    /// A workspace switcher button.
    Workspace {
        id: guild_session::WorkspaceId,
        last: bool,
    },
    /// The add button's chevron, with the harnesses the `PATH` has —
    /// M12.7.
    NewTab {
        available: Vec<guild_session::HarnessKind>,
    },
    /// The chevron over the tabs, with the tabs the strip cannot show: by
    /// identity, name and file icon stem — M12.7.
    HiddenTabs(Vec<(TabId, String, Option<String>)>),
    /// The tab strip's `⋮` — M12.7 — with the seat's number of areas,
    /// M12.8.
    TabActions {
        areas: usize,
    },
    /// A panel head's `⋮`, by its dock and the toolbar buttons that did
    /// not fit beside it — M12.7. Data, like `NewTab` and `HiddenTabs`:
    /// `dock_menu::items_with`'s folded buttons are live, so this carries
    /// them rather than being a `Subject`.
    DockHead {
        dock: Dock,
        folded: Vec<Named>,
    },
    /// The status bar's counter — M12.7 task 4. Data, like `NewTab` and
    /// `HiddenTabs`: the agents open in the window and the session behind
    /// the tab in front, both asked for at the click, so what the list
    /// shows is what was true when it opened.
    Agents {
        groups: Vec<crate::agents::Group>,
        front: Option<SessionId>,
    },
    /// One of the Git tool's filter dropdowns, with what it lists — M12.7
    /// task 6. Data, gathered at the click: a row's deed names a branch
    /// or an author by its place in this list.
    LogFilter(log_filter::FilterList),
    /// Find in Files' chevron over a field, with what it remembers —
    /// M12.7 task 7. Data, the counter's own pattern: gathered at the
    /// click, so a `UseHistory` deed resolves against the texts that were
    /// there then and not against whatever has been typed since.
    ///
    /// **Opened over the popup rather than in place of it**: Find in
    /// Files is itself `Root.picker`, and this is the one menu that does
    /// not put it away — `open_menu` and `refocus_after_menu` carry the
    /// exception.
    FindHistory {
        field: Field,
        items: Vec<String>,
    },
    /// The title bar's Open In — M12.10: the folder it acts on, and the
    /// installed applications found at the press, `(name, bundle id)`.
    OpenIn {
        folder: PathBuf,
        apps: Vec<(String, String)>,
    },
}

impl Target {
    /// What its menu is about — `None` for a menu whose items are the
    /// target's own data, which `strip`'s builders read instead. M12.7.
    pub(crate) fn subject(&self) -> Option<Subject> {
        Some(match self {
            Target::TreeRow {
                path,
                is_dir: true,
                expanded,
            } => Subject::TreeDirectory {
                expanded: *expanded,
                root: path.as_os_str().is_empty(),
                open_root: None,
            },
            Target::TreeRow { .. } => Subject::TreeFile,
            Target::LibraryRow {
                is_dir, expanded, ..
            } => Subject::Library {
                is_dir: *is_dir,
                expanded: *expanded,
            },
            Target::Tab {
                path,
                session,
                index,
                of,
                neighbours,
                ..
            } => Subject::Tab {
                has_file: path.is_some(),
                session: *session,
                index: *index,
                of: *of,
                neighbours: *neighbours,
            },
            Target::Change {
                ticked,
                unmerged,
                is_dir,
                unversioned,
                gone,
                ..
            } => Subject::Change {
                ticked: *ticked,
                unmerged: *unmerged,
                is_dir: *is_dir,
                unversioned: *unversioned,
                gone: *gone,
            },
            Target::ChangeSection => Subject::ChangeSection,
            Target::Branch { listed, .. } => Subject::Branch { listed: *listed },
            Target::Commit { .. } => Subject::Commit,
            Target::Repository { projects, .. } => Subject::Repository {
                opened: !projects.is_empty(),
            },
            Target::Worktree {
                project,
                primary,
                locked,
                mr,
                ..
            } => Subject::Worktree {
                primary: *primary,
                opened: project.is_some(),
                locked: *locked,
                mr: *mr,
            },
            Target::Terminal { selection, .. } => Subject::Terminal {
                selection: *selection,
            },
            Target::Workspace { last, .. } => Subject::Workspace { last: *last },
            Target::TabActions { areas } => Subject::TabActions { areas: *areas },
            Target::Gutter { .. }
            | Target::NewTab { .. }
            | Target::HiddenTabs(_)
            | Target::DockHead { .. }
            | Target::Agents { .. }
            | Target::WorktreeActivity { .. }
            | Target::ActionBarAdd { .. }
            | Target::Setting(_)
            | Target::LogFilter(_)
            | Target::ProjectSort { .. }
            | Target::SessionRow { .. }
            | Target::SessionFilter { .. }
            | Target::ProjectGroup { .. }
            | Target::ProjectGroupAdd { .. }
            | Target::TerminalJobScopes { .. }
            | Target::OpenIn { .. }
            | Target::FindHistory { .. } => {
                return None;
            }
        })
    }

    /// The path this target is about, when it has one.
    pub(crate) fn path(&self) -> Option<&PathBuf> {
        match self {
            Target::TreeRow { path, .. } | Target::LibraryRow { path, .. } => Some(path),
            Target::Tab { path, .. } => path.as_ref(),
            Target::Change { path, .. } => Some(path),
            // **A worktree's path is not one of these.** What this answers
            // is a path inside a project, which `full_path` joins the
            // project's root onto; a worktree's is the root itself, and an
            // absolute one. The column's menu offers no `PathDeed`, and
            // answering here would be how it came to offer one that copies
            // the wrong thing.
            Target::ChangeSection
            | Target::Branch { .. }
            | Target::Commit { .. }
            | Target::Repository { .. }
            | Target::Worktree { .. }
            | Target::Terminal { .. }
            | Target::Gutter { .. } => None,
            Target::Workspace { .. }
            | Target::NewTab { .. }
            | Target::HiddenTabs(_)
            | Target::TabActions { .. }
            | Target::DockHead { .. }
            | Target::Agents { .. }
            | Target::WorktreeActivity { .. }
            | Target::ActionBarAdd { .. }
            | Target::Setting(_)
            | Target::LogFilter(_)
            | Target::ProjectSort { .. }
            | Target::SessionRow { .. }
            | Target::SessionFilter { .. }
            | Target::ProjectGroup { .. }
            | Target::ProjectGroupAdd { .. }
            | Target::TerminalJobScopes { .. }
            | Target::OpenIn { .. }
            | Target::FindHistory { .. } => None,
        }
    }
}

/// What can be done to **any** row that stands for a path.
///
/// These belong to no one surface: the tree, a tab and a change all
/// offer them, and all three mean the same thing by them. Keeping them
/// here rather than repeating four variants in four files is what stops
/// *Copy Path* being three deeds that drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathDeed {
    /// Open its diff against `HEAD`.
    ShowDiff,
    /// The path inside the project, on the clipboard.
    CopyPath,
    /// The whole path from the root of the disk.
    CopyFullPath,
    /// Ask the platform to show it in the Finder.
    Reveal,
}

/// One thing a menu item does to the target it was built for.
///
/// Four surfaces and the paths they share. A branch that adds an item
/// adds a variant **inside its own file**, which is the whole reason
/// this enum has arms rather than twenty-two variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deed {
    ActionBarAdd(guild_session::action_bar::Item),
    Setting(crate::view::settings::choices::Value),
    /// Something any row with a path offers.
    Path(PathDeed),
    /// The file tree's own.
    Tree(TreeDeed),
    /// The tab strip's own.
    Tab(TabDeed),
    /// The Commit panel's own.
    Change(ChangeDeed),
    /// The Git tool's own.
    Log(LogDeed),
    /// The column's own — M12.
    Column(WorktreeDeed),
    /// A project's, in the column — M12.2.
    Project(ProjectDeed),
    /// A project group header's own.
    ProjectGroup(column::ProjectGroupDeed),
    /// The title bar's Open In — M12.10.
    OpenIn(OpenInDeed),
    TerminalJobScope(guild_session::terminal_job::Scope),
    /// Choose a workspace-local repository order.
    ProjectSort(guild_session::project_sort::Mode),
    /// A Sessions mode row's own.
    Session(sessions::SessionDeed),
    /// A check of the Sessions mode's filter.
    SessionFilter(sessions::FilterPick),
    /// A terminal's — M5.12.
    Terminal(TerminalDeed),
    /// The line numbers' own — M12.9.
    Gutter(gutter::GutterDeed),
    /// A workspace switcher button's own deeds.
    Workspace(WorkspaceDeed),
    /// A panel head's own: where it may be moved to, and how it is put
    /// away — the `⋮`'s, M12.7.
    Dock(DockDeed),
    /// Open a tab of this kind — the add button's chevron, M12.7.
    NewTab(guild_session::DefaultTab),
    /// Bring this tab forward — the chevron over the tabs, M12.7.
    ShowTab(TabId),
    /// Bring this session's tab forward — the counter's own list, M12.7
    /// task 4.
    ShowSession(SessionId),
    /// Dispatch this action to the thing in front — the tab strip's `⋮`,
    /// and since task 3 a folded toolbar button in a dock head's, the one
    /// kind of item that is an action rather than a deed. M12.7.
    Action(Named),
    /// A row of the Git tool's filter dropdowns — M12.7 task 6.
    LogFilter(FilterDeed),
    /// A row of Find in Files' history, by its place in the
    /// `Target::FindHistory` it was opened with — M12.7 task 7. The field
    /// travels with the deed because `Deed` stays `Copy` and the target,
    /// not the deed, carries the texts.
    UseHistory(Field, usize),
}

/// What a row says: a word of the translation table, or a name the model
/// supplies — a branch, a workspace, a file. M12.7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Label {
    /// A word of the translation table.
    Text(Text),
    /// A name as the model has it, drawn as it is.
    Named(String),
}

/// What stands in a row's icon slot — M12.7.
///
/// **A name for a drawing, not the drawing**: the list stays something a
/// test can read without a window, and `Root::menu_mark` turns each one
/// into what the surface it came from already draws, so a harness looks
/// the same in the menu as on its tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// Nothing; the slot stays empty when the menu has one.
    None,
    /// A toolbar icon, in `text.secondary`.
    Icon(&'static str),
    /// A harness's mark, as the tab strip's add button draws it.
    Harness(guild_session::DefaultTab),
    /// A file-type icon, by its stem in the icon set.
    File(String),
    /// What a session's agent is doing, as the agents list draws it.
    Status(guild_session::SessionId),
    /// The star the Log puts on the branch `HEAD` is on, in `accent.brass`.
    Star,
    /// A repository's letter tile, as the column draws it — by its name.
    Tile(String),
    /// An installed application's own icon, by its bundle identifier —
    /// Open In's rows, M12.10.
    App(String),
}

/// A row of a menu that may have a current item and may refuse — M12.7.
///
/// What every floating list of the window needs and the right button's
/// menu never did: a check beside the one that is chosen, a mark of the
/// row's own, and a row drawn but not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// Its words.
    pub label: Label,
    /// What stands in its icon slot.
    pub mark: Mark,
    /// What taking it does.
    pub deed: Deed,
    /// `Some(true)` is the current item; `Some(false)` a row of a menu
    /// that has one; `None` a menu with no current item and no column.
    pub check: Option<bool>,
    /// Whether it can be taken. A disabled row is drawn faint and is not
    /// a place the keyboard stands.
    pub enabled: bool,
    /// A faint word or two at the row's right, in the stroke's own slot
    /// and styling — M12.7 fix round 1: the project popup's path and
    /// session count, the agents list's place, anything a label alone
    /// cannot tell two same-named rows apart by. Shown instead of the
    /// deed's stroke when both would fill the slot.
    ///
    /// **A `Label`, not a string** — M12.8, decision 9: the last area's
    /// *Close Area* says *the last area stays* here, and `entries` is pure
    /// and has no language to translate it with; a word of the table is
    /// resolved where the row is drawn, a place is `Label::Named`.
    pub detail: Option<Label>,
    /// A second deed at the row's end, drawn only on the lit row — the
    /// project popup's ✕, M12.7 task 5. The row's own click still takes
    /// `deed`; this one is a control of its own, with its own hover and
    /// its own `stop_propagation` (the trap: a cross inside a row runs
    /// the row's handler too, and second).
    pub trailing: Option<Deed>,
}

/// One line of a menu: something to do, a rule between groups, or a
/// group that opens sideways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A label and what it does.
    Item(Text, Deed),
    /// A label supplied by the model rather than the translation table.
    /// Workspace names are the first such labels.
    NamedItem(String, Deed),
    /// A row with a mark, a check and a refusal of its own — M12.7.
    Choice(Choice),
    /// A hairline between two groups of items.
    Separator,
    /// A group's name in small faint type, as the agents list heads its
    /// kinds. Not a place — M12.7.
    Heading(Label),
    /// A footnote under a rule, as the Log's filter menus end with *Press
    /// ⌫ to clear the filter*. Not a place — M12.7.
    Note(Label),
    /// A label and the entries behind it.
    ///
    /// **What a menu writes with a `▸`.** A file's menu has fourteen
    /// things in it, and a flat fourteen is a menu people stop reading;
    /// the four that group — *New*, *Open In*, *Git* — are three lines
    /// with the rest one press further away. Recursive: a submenu holds
    /// entries, including separators.
    Submenu(Text, Vec<Entry>),
}

/// What a subject offers.
///
/// Pure, so that a test can read a menu without a window. The order is
/// the order a person reads: what this row *is* first, then what can be
/// copied from it, then where it lives on the disk.
pub fn entries(subject: Subject) -> Vec<Entry> {
    match subject {
        Subject::TreeFile => tree::entries_for_file(),
        Subject::TreeDirectory {
            expanded,
            root,
            open_root,
        } => tree::entries_for_directory(expanded, root, open_root),
        Subject::Library { is_dir, expanded } => tree::entries_for_library(is_dir, expanded),
        Subject::Tab {
            has_file,
            session,
            index,
            of,
            neighbours,
        } => tabs::entries_for_tab(has_file, session, index, of, neighbours),
        Subject::Change {
            ticked,
            unmerged,
            is_dir,
            unversioned,
            gone,
        } => changes::entries_for_change(ticked, unmerged, is_dir, unversioned, gone),
        Subject::ChangeSection => changes::entries_for_section(),
        Subject::Branch { listed } => log::entries_for_branch(listed),
        Subject::Commit => log::entries_for_commit(),
        Subject::Repository { opened } => column::entries_for_repository(opened),
        Subject::Worktree {
            primary,
            opened,
            locked,
            mr,
        } => column::entries_for_worktree(primary, opened, locked, mr),
        Subject::Terminal { selection } => terminal::entries_for_terminal(selection),
        Subject::Workspace { last } => workspace::entries_for_workspace(last),
        Subject::TabActions { areas } => strip::tab_actions(areas),
    }
}

/// What a menu's target is to the strokes: whether it is the thing in
/// front, and — for a tab — whether that tab is divided into panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Front {
    /// The target is what the actions reach: [`stroke_for`]'s `front`.
    pub front: bool,
    /// The target is a tab of more than one pane — M12.8.
    pub divided: bool,
}

/// The stroke a row prints: [`stroke_for`]'s, except *Close* on a divided
/// tab, whose `cmd-w` closes the focused pane and leaves the tab open —
/// decision 14 — so the key would not do what the row does.
pub fn stroke_shown(deed: Deed, front: Front) -> Option<Named> {
    match deed {
        Deed::Tab(TabDeed::Close) if front.divided => None,
        _ => stroke_for(deed, front.front),
    }
}

/// The stroke an item may show, if any.
///
/// **A menu item may advertise a stroke only when the action would do
/// the same thing to the same target.** Every item is a deed aimed at the
/// row under the pointer and every action reaches the thing in front —
/// decision 20 of the spec — so *Close* on the tab in front may show
/// `cmd-w` and *Close* on a background tab must show nothing, or the
/// menu is lying about what the key will do.
///
/// `front` is whether the target **is** the thing the action would
/// reach: for a tab, whether it is the tab in front. A deed that does
/// the same thing wherever it was invoked from — closing every tab,
/// ticking every change — ignores it.
///
/// Pure, and here rather than in each surface: the rule is one rule, and
/// four copies of it would be four chances to get it wrong.
pub fn stroke_for(deed: Deed, front: bool) -> Option<Named> {
    match deed {
        // The `⋮`'s items are the actions themselves, so the stroke is
        // theirs wherever the menu was opened from. M12.7.
        Deed::Action(named) => Some(named),
        // `CloseTab` closes the tab in front and `CloseOtherTabs` keeps
        // it, so both are honest only on the tab in front.
        Deed::Tab(TabDeed::Close) if front => Some(Named::CloseTab),
        Deed::Tab(TabDeed::CloseOthers) if front => Some(Named::CloseOtherTabs),
        // This one closes every tab of the pressed tab's own strip, and
        // the stroke every tab of the focused area's — the same only on
        // the tab in front since M12.8, when a seat has several strips.
        Deed::Tab(TabDeed::CloseAll) if front => Some(Named::CloseAllTabs),
        // The splits divide the focused pane of the tab in front, and
        // the deeds that tab's focused pane — the same only in front.
        Deed::Tab(TabDeed::SplitRight) if front => Some(Named::SplitPaneRight),
        Deed::Tab(TabDeed::SplitDown) if front => Some(Named::SplitPaneDown),
        // The section head's four are about the whole panel, and so are
        // the actions: there is no row for them to disagree about.
        Deed::Change(ChangeDeed::TickAll) => Some(Named::TickAllChanges),
        Deed::Change(ChangeDeed::UntickAll) => Some(Named::UntickAllChanges),
        Deed::Change(ChangeDeed::CollapseSections) => Some(Named::CollapseChanges),
        Deed::Change(ChangeDeed::ExpandSections) => Some(Named::ExpandChanges),
        Deed::Change(ChangeDeed::ShowDiff) => Some(Named::ShowSelectedDiff),
        Deed::Change(ChangeDeed::Include) => Some(Named::IncludeSelectedChanges),
        Deed::Change(ChangeDeed::Exclude) => Some(Named::ExcludeSelectedChanges),
        Deed::Change(ChangeDeed::CopyPaths) => Some(Named::Copy),
        Deed::Change(ChangeDeed::Rollback) => Some(Named::Rollback),
        Deed::Change(ChangeDeed::AddToVcs) => Some(Named::AddToVcs),
        Deed::Change(ChangeDeed::Delete) => Some(Named::DeleteFile),
        // `f4` opens the row the tree has selected, which is this row only
        // when the pointer is on the selection.
        Deed::Tree(TreeDeed::Open) if front => Some(Named::JumpToSource),
        Deed::Tree(TreeDeed::Move) => Some(Named::MoveFile),
        Deed::Tree(TreeDeed::Cut) => Some(Named::CutFiles),
        Deed::Tree(TreeDeed::Copy) => Some(Named::Copy),
        Deed::Tree(TreeDeed::Paste) => Some(Named::Paste),
        Deed::Tree(TreeDeed::Rename) => Some(Named::RenameFile),
        Deed::Tree(TreeDeed::Delete) => Some(Named::DeleteFile),
        Deed::Tree(TreeDeed::NewFile) => Some(Named::NewFile),
        Deed::Tree(TreeDeed::NewDirectory) => Some(Named::NewDirectory),
        // A terminal's five act on the terminal holding the focus, which
        // after the menu is the tab's: honest there, and on the Output
        // panel's terminal a promise about the wrong one. M5.12.
        Deed::Terminal(deed) if front => Some(match deed {
            TerminalDeed::Copy => Named::Copy,
            TerminalDeed::Paste => Named::Paste,
            TerminalDeed::SelectAll => Named::SelectAll,
            TerminalDeed::Find => Named::Find,
            TerminalDeed::Clear => Named::ClearTerminal,
            TerminalDeed::PreviousCommand
            | TerminalDeed::NextCommand
            | TerminalDeed::SelectCommandOutput
            | TerminalDeed::CopyCommandOutput => return None,
        }),
        Deed::Column(WorktreeDeed::CopyPath)
        | Deed::Project(ProjectDeed::CopyPath)
        | Deed::OpenIn(OpenInDeed::CopyPath)
            if front =>
        {
            Some(Named::CopyWorktreePath)
        }
        Deed::Column(WorktreeDeed::Remove) if front => Some(Named::RemoveWorktree),
        Deed::Column(WorktreeDeed::ViewMr) if front => Some(Named::ViewMergeRequest),
        Deed::Column(WorktreeDeed::Run) if front => Some(Named::Run),
        Deed::Column(WorktreeDeed::Terminal) if front => Some(Named::NewTerminal),
        // Everything else shows nothing, deliberately. `ShowDiff` on a
        // change row is the nearest miss: `ShowSelectedDiff` acts on the
        // row the keyboard is on, which is not always the row the
        // pointer is on.
        _ => None,
    }
}

/// Where the keyboard's highlight goes next in a menu — M12.2 task 8.
///
/// **Past the separators, and no further than the ends**: a separator is
/// not a place to stand, a submenu is, and a menu that wrapped would send a
/// held `down` round and round a list a person is trying to reach the end
/// of. From nothing, `down` is the first item and `up` the last. Since
/// M12.7 a heading, a footnote and a disabled row are passed over too.
pub fn step(entries: &[Entry], from: Option<usize>, down: bool) -> Option<usize> {
    let standing = |index: &usize| match &entries[*index] {
        Entry::Separator | Entry::Heading(_) | Entry::Note(_) => false,
        Entry::Choice(choice) => choice.enabled,
        _ => true,
    };
    let next = match (from, down) {
        (None, true) => (0..entries.len()).find(standing),
        (None, false) => (0..entries.len()).rev().find(standing),
        (Some(at), true) => (at + 1..entries.len()).find(standing),
        (Some(at), false) => (0..at).rev().find(standing),
    };
    next.or(from)
}

/// Whether an item loses something when it is taken — drawn in
/// `accent.rust`, as *Remove Project* is. M12.2.
pub fn loses(deed: Deed) -> bool {
    matches!(
        deed,
        Deed::Tree(TreeDeed::Delete)
            | Deed::Change(ChangeDeed::Rollback)
            | Deed::Change(ChangeDeed::Delete)
            | Deed::Column(WorktreeDeed::Remove)
            | Deed::Project(ProjectDeed::Remove)
            | Deed::Workspace(WorkspaceDeed::Delete)
            | Deed::Terminal(TerminalDeed::Clear)
            | Deed::Action(
                Named::ClearTerminal
                    | Named::ClearRun
                    | Named::Rollback
                    | Named::DeleteFile
                    | Named::RemoveWorktree
            )
    )
}

/// An icon for an item, where the menu it is in carries icons. A row menu
/// carries none — and the
/// project's menu is where they are. M12.2.
pub fn icon_of(deed: Deed) -> Option<&'static str> {
    match deed {
        Deed::Project(ProjectDeed::Rename) => Some("ui/menu-rename.svg"),
        Deed::Project(ProjectDeed::NewWorktree) => Some("ui/column-new-worktree.svg"),
        Deed::Project(ProjectDeed::CopyPath) => Some("ui/menu-copy.svg"),
        Deed::Project(ProjectDeed::Reveal) => Some("ui/menu-reveal.svg"),
        Deed::Project(ProjectDeed::MoveTo(_)) => Some("ui/workspace-layers.svg"),
        Deed::Project(ProjectDeed::Remove) => Some("ui/menu-remove.svg"),
        Deed::ProjectGroup(column::ProjectGroupDeed::Rename) => Some("ui/menu-rename.svg"),
        Deed::ProjectGroup(column::ProjectGroupDeed::Delete) => Some("ui/menu-remove.svg"),
        Deed::ProjectGroup(column::ProjectGroupDeed::NewSession) => Some("ui/head-plus.svg"),
        Deed::ProjectGroup(column::ProjectGroupDeed::BindFolder) => Some("ui/workspace-folder.svg"),
        _ => None,
    }
}

/// Whether any item of a menu has an icon, so the rest line up with it.
fn has_icons(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| match entry {
        Entry::Item(_, deed) | Entry::NamedItem(_, deed) => icon_of(*deed).is_some(),
        Entry::Choice(choice) => choice.mark != Mark::None,
        _ => false,
    })
}

/// Whether a menu has a current item, so every row keeps a column for the
/// check — M12.7.
fn has_checks(entries: &[Entry]) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry, Entry::Choice(choice) if choice.check.is_some()))
}

/// What a row's check column holds — M12.7.
///
/// In a menu with no current item, nothing and no column. In one that has
/// one, every row that is drawn as a row keeps the column — an item, a
/// named item and a submenu empty, a choice checked when it is the
/// current one — so every row's words start in one place.
pub fn check_column(entries: &[Entry]) -> Vec<Option<bool>> {
    let checks = has_checks(entries);
    entries
        .iter()
        .map(|entry| check_of(entry, checks))
        .collect()
}

/// One row's share of `check_column`, given whether its menu has checks.
fn check_of(entry: &Entry, checks: bool) -> Option<bool> {
    if !checks {
        return None;
    }
    match entry {
        Entry::Choice(choice) => Some(choice.check == Some(true)),
        Entry::Item(..) | Entry::NamedItem(..) | Entry::Submenu(..) => Some(false),
        Entry::Separator | Entry::Heading(_) | Entry::Note(_) => None,
    }
}

/// How far a menu's card keeps from the window's edge, in pixels.
const MARGIN: f32 = 8.0;

/// Where a menu hangs — M12.7.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Anchor {
    /// At the pointer, as the right button opens one.
    Pointer(gpui::Point<gpui::Pixels>),
    /// Under a trigger: the card's top-left at its bottom-left.
    Below(gpui::Bounds<gpui::Pixels>),
    /// Over a trigger, as a status-bar button's list rises: the card's
    /// bottom-left at its top-left.
    Above(gpui::Bounds<gpui::Pixels>),
}

/// The menu that is up — M12.2 task 8.
pub(crate) struct Open {
    commit: Option<changes::Snapshot>,
    /// What it is about.
    pub target: Target,
    /// Where it hangs: the pointer, or a trigger's bounds — M12.7.
    pub anchor: Anchor,
    /// Whether its tab is the one in front, and divided, for the strokes
    /// it may show.
    pub front: Front,
    /// The row lit, by the pointer or the keyboard.
    pub lit: Option<usize>,
    /// The submenu open, by its row.
    pub submenu: Option<usize>,
    /// The row lit in that submenu.
    pub sub_lit: Option<usize>,
    /// Whether the keyboard is in the submenu rather than the menu.
    pub in_submenu: bool,
    /// The card's scroll, so the keyboard's highlight is kept in view in
    /// a list longer than the window — M12.7.
    pub scroll: gpui::ScrollHandle,
    /// The submenu card's, for the same reason; a fresh one for every
    /// submenu that opens, so a new one starts at its top.
    pub sub_scroll: gpui::ScrollHandle,
    /// How tall its cards may grow, when not the window less its margins
    /// — the Log's filters keep the height their lists always had, M12.7
    /// task 6.
    pub cap: Option<gpui::Rems>,
    /// Where the pointer was the moment this menu opened — M12.7 final
    /// fix: a move whose reported position is exactly this one is not a
    /// real arrival (a stray or coalesced event at the same spot, the
    /// title bar's own bottom edge sitting flush against the card's
    /// first row), and must not light whatever row now happens to be
    /// there. Only a move that actually changed the position does; see
    /// `moved_since_open`.
    pub opened_at: gpui::Point<gpui::Pixels>,
    pub(crate) agent_selected: Option<SessionId>,
    _agent_age_refresh: Option<gpui::Task<()>>,
}

/// Keep child layers above their owning menu, including menus over another popup.
fn context_menu_priority(target: &Target) -> usize {
    if matches!(target, Target::TerminalJobScopes { .. }) {
        3
    } else {
        0
    }
}

impl Open {
    /// Open the submenu on row `index`, and start its scroll over when it
    /// is a different submenu from the one open.
    fn show_submenu(&mut self, index: Option<usize>) {
        if self.submenu != index {
            self.sub_scroll = gpui::ScrollHandle::new();
        }
        self.submenu = index;
    }
}

/// Whether a move to `at` is a real arrival, given where the pointer was
/// when the menu opened — M12.7 final fix: at open nothing is lit until
/// the pointer moves onto a row, and a `MouseMoveEvent` reporting the
/// exact spot the menu opened at is not that — it is the click's own
/// position read back, with nothing having actually moved. Pure, so a
/// test can check it without a window.
pub(crate) fn moved_since_open(
    at: gpui::Point<gpui::Pixels>,
    opened_at: gpui::Point<gpui::Pixels>,
) -> bool {
    at != opened_at
}

/// Whether a footnote drawn by `render_menu_card` has anything above it —
/// a residual fix: an empty agents list is `vec![Entry::Note(_)]`, a
/// single entry, and both of `popup::note`'s two call sites (a `Note` held
/// out as the card's `foot`, and one drawn in place among `rows`) knew
/// only their own row count, not whether it was zero — so *No agents
/// open* carried the rule that separates a footnote from the rows above
/// it, with no rows above it to separate from. `rows_before` is the
/// number of rows drawn before the note: `rows.len()` for the `foot` and
/// the note's own `index` within `rows` otherwise. Pure, so a test can
/// check it without a window.
pub(crate) fn note_is_preceded(rows_before: usize) -> bool {
    rows_before > 0
}

/// A menu card's height, guessed cheaply from its entries before they are
/// laid out — M12.7 residual fix: deciding which side of a trigger a card
/// fits on needs a height, and the true one exists only once
/// `render_menu_card` has built it. Every entry is counted as one
/// [`theme::space::MENU_ROW`], which overstates a separator's or a
/// footnote's — safely, since the only thing this feeds is a choice
/// between two sides, and overstating never puts the card on a side it
/// does not actually fit either.
pub(crate) fn estimated_card_height(entries: usize, rem_size: gpui::Pixels) -> gpui::Pixels {
    crate::theme::space::MENU_ROW.to_pixels(rem_size) * entries
        + crate::theme::space::MENU_PADDING.to_pixels(rem_size) * 2.0
}

/// Where a menu hangs — M12.7 residual fix.
///
/// `anchored()`'s own `snap_to_window_with_margin` only ever *translates*
/// the card to keep it inside the window; unlike `snap_to_window`'s
/// `SwitchAnchor` mode (which this engine does not use, since it wants a
/// margin), it never changes which corner faces the trigger. A card asked
/// for `Below` a trigger with no room under it — the Log's Branch, User
/// and Date menus, opened from a button in the bottom dock — was
/// translated up over its own button rather than moved to the other side.
///
/// **The flip needs the other side to have more room, not merely enough
/// of it.** A card that fits on neither side stays where it was asked
/// for: flipping there would only move the overlap to a side with even
/// less room to spare. Pure, so a test can check it without a window.
pub(crate) fn flip(
    anchor: Anchor,
    window_h: gpui::Pixels,
    margin: gpui::Pixels,
    estimated_h: gpui::Pixels,
) -> (gpui::Point<gpui::Pixels>, gpui::Anchor) {
    let zero = gpui::px(0.0);
    match anchor {
        Anchor::Pointer(at) => (at, gpui::Anchor::TopLeft),
        Anchor::Below(bounds) => {
            let below = (window_h - margin - bounds.bottom_left().y).max(zero);
            let above = (bounds.origin.y - margin).max(zero);
            if below < estimated_h && below < above {
                (bounds.origin, gpui::Anchor::BottomLeft)
            } else {
                (bounds.bottom_left(), gpui::Anchor::TopLeft)
            }
        }
        Anchor::Above(bounds) => {
            let above = (bounds.origin.y - margin).max(zero);
            let below = (window_h - margin - bounds.bottom_left().y).max(zero);
            if above < estimated_h && above < below {
                (bounds.bottom_left(), gpui::Anchor::TopLeft)
            } else {
                (bounds.origin, gpui::Anchor::BottomLeft)
            }
        }
    }
}

impl crate::Root {
    fn entries_for_open(&self, open: &Open, cx: &gpui::App) -> Vec<Entry> {
        open.commit.as_ref().map_or_else(
            || self.entries_for_target(&open.target, cx),
            |snapshot| snapshot.entries.clone(),
        )
    }

    /// Entries whose labels depend on the live desk. Most menus are pure;
    /// a project's destination list is necessarily the workspaces that exist
    /// now and do not already hold that repository.
    fn entries_for_target(&self, target: &Target, cx: &gpui::App) -> Vec<Entry> {
        if matches!(target, Target::Change { .. }) {
            return self.changes_selection_entries();
        }
        let mut found = match target {
            Target::ProjectSort { workspace } => {
                return self
                    .desk
                    .workspace(*workspace)
                    .map(|holder| column::sort_entries(holder.project_sort))
                    .unwrap_or_default();
            }
            Target::ProjectGroup { workspace, id } => {
                let bound = self
                    .desk
                    .workspace(*workspace)
                    .and_then(|holder| holder.project_group(*id))
                    .is_some_and(|group| group.folder.is_some());
                return column::entries_for_project_group(bound);
            }
            Target::ProjectGroupAdd { choices, .. } => {
                return column::add_project_entries(choices, |group| {
                    self.ui().group_member_of(group)
                });
            }
            Target::TreeRow {
                path,
                is_dir: true,
                expanded,
            } => {
                return entries(Subject::TreeDirectory {
                    expanded: *expanded,
                    root: path.as_os_str().is_empty(),
                    open_root: self.open_root_of_row(path),
                });
            }
            Target::SessionRow { key } => return sessions::row_entries(key),
            Target::SessionFilter { projects, .. } => {
                return sessions::filter_entries(&self.session_filter, projects);
            }
            Target::NewTab { available } => {
                return strip::ordered_new_tab(available, &self.settings.tab_launcher_order());
            }
            Target::HiddenTabs(tabs) => return strip::hidden_tabs(tabs),
            Target::DockHead { dock, folded } => return dock::dock_head(*dock, folded),
            Target::Agents { groups, front } => return agents::agents(groups, *front),
            Target::WorktreeActivity { seat, .. } => return self.activity_entries(*seat),
            Target::ActionBarAdd { source, .. } => {
                return if self.action_bar_settings_source() == Some(*source) {
                    self.action_bar_choices()
                } else {
                    Vec::new()
                };
            }
            Target::Setting(value) => return self.settings_choices(*value),
            Target::TerminalJobScopes { popup } => {
                let Some(list) = self
                    .terminal_list
                    .as_ref()
                    .filter(|list| &list.focus == popup)
                else {
                    return Vec::new();
                };
                return guild_session::terminal_job::Scope::ALL
                    .into_iter()
                    .map(|scope| {
                        let enabled = scope.available(&list.context);
                        Entry::Choice(Choice {
                            label: Label::Named(self.ui().terminal_scope(scope).into()),
                            mark: Mark::None,
                            deed: Deed::TerminalJobScope(scope),
                            check: Some(scope == list.scope),
                            enabled,
                            detail: (!enabled).then_some(Label::Named(
                                self.ui()
                                    .text(if scope == guild_session::terminal_job::Scope::Session {
                                        Text::TerminalNoSession
                                    } else {
                                        Text::TerminalNoRoot
                                    })
                                    .into(),
                            )),
                            trailing: None,
                        })
                    })
                    .collect();
            }
            Target::LogFilter(list) => return log_filter::log_filter(list),
            Target::OpenIn { folder, apps } => {
                return open_in::open_in(apps, folder, &self.selected_open_in_target());
            }
            Target::Gutter {
                annotated, file, ..
            } => return gutter::gutter(*annotated, file.read(cx).can_run_file()),
            Target::FindHistory { field, items } => {
                return history::history(*field, history::title(*field), items);
            }
            Target::Terminal { at, selection } => {
                let mut entries = terminal::entries_for_terminal(*selection);
                let (previous, next, output, status) = self
                    .terminal_at(*at)
                    .map(|v| v.read(cx).command_status())
                    .unwrap_or_default();
                entries.push(Entry::Separator);
                for (text, deed, enabled, detail) in [
                    (
                        Text::TerminalPreviousCommand,
                        TerminalDeed::PreviousCommand,
                        previous,
                        Some("⌘↑"),
                    ),
                    (
                        Text::TerminalNextCommand,
                        TerminalDeed::NextCommand,
                        next,
                        Some("⌘↓"),
                    ),
                    (
                        Text::TerminalSelectOutput,
                        TerminalDeed::SelectCommandOutput,
                        output,
                        None,
                    ),
                    (
                        Text::TerminalCopyOutput,
                        TerminalDeed::CopyCommandOutput,
                        output,
                        None,
                    ),
                ] {
                    entries.push(Entry::Choice(Choice {
                        label: Label::Text(text),
                        mark: Mark::None,
                        deed: Deed::Terminal(deed),
                        check: None,
                        enabled,
                        detail: detail.map(|s| Label::Named(s.into())),
                        trailing: None,
                    }));
                }
                if let Some(status) = status {
                    entries.push(Entry::Note(Label::Named(format!(
                        "{}: {status}",
                        self.ui().text(Text::TerminalExitStatus)
                    ))));
                }
                entries
            }
            _ => target.subject().map(entries).unwrap_or_default(),
        };
        let Target::Repository {
            workspace,
            projects,
            key,
            ..
        } = target
        else {
            return found;
        };
        let destinations: Vec<Entry> = self
            .desk
            .workspaces()
            .iter()
            .filter(|candidate| candidate.id != *workspace)
            .filter(|candidate| {
                !projects
                    .iter()
                    .any(|project| candidate.projects.contains(project))
            })
            .map(|candidate| {
                Entry::NamedItem(
                    candidate.name.clone(),
                    Deed::Project(ProjectDeed::MoveTo(candidate.id)),
                )
            })
            .collect();
        // Before *Remove Project* and its rule, as *Move to Workspace* is.
        let at = found.len().saturating_sub(2);
        if !destinations.is_empty() {
            found.insert(at, Entry::Submenu(Text::MenuMoveToWorkspace, destinations));
        }
        if let Some(holder) = self.desk.workspace(*workspace) {
            let current = holder.project_group_of(key).map(|group| group.id);
            let groups: Vec<_> = holder
                .project_groups
                .iter()
                .map(|group| (group.id, group.name.clone()))
                .collect();
            found.insert(
                at,
                Entry::Submenu(
                    Text::MenuMoveToGroup,
                    column::move_to_group_entries(&groups, current),
                ),
            );
        }
        found
    }

    /// A row's words, from the translation table or as the model has them.
    fn menu_label(&self, label: &Label) -> SharedString {
        match label {
            Label::Text(text) => self.ui().text(*text).to_owned().into(),
            Label::Named(name) => name.clone().into(),
        }
    }

    /// What a mark is drawn as, by the surface it comes from — M12.7.
    ///
    /// Each one is the drawing that surface already has, so a harness, a
    /// file or a session looks the same in a menu as where it lives. A
    /// disabled row's mark goes `text.faint` with its label, except a
    /// file's, which is an image in its own colours.
    fn menu_mark(&self, mark: &Mark, loses: bool, disabled: bool) -> Option<gpui::AnyElement> {
        use gpui::prelude::*;
        let theme = self.theme;
        let faint = gpui::rgb(theme.text.faint);
        let size = crate::theme::space::MENU_ICON;
        Some(match mark {
            Mark::None => return None,
            Mark::Icon(path) if disabled => crate::icons::toolbar(path)
                .size(size)
                .text_color(faint)
                .into_any_element(),
            Mark::Icon(path) => crate::view::popup::icon(path, loses, theme),
            Mark::Harness(tab) => crate::view::kind::harness_mark(
                *tab,
                theme,
                size,
                if disabled {
                    crate::view::kind::Shade::Disabled
                } else {
                    crate::view::kind::Shade::Full
                },
            ),
            Mark::File(stem) => crate::icons::icon(stem).size(size).into_any_element(),
            // The application's own icon, as Launch Services found it; an
            // empty slot while it has not been read.
            Mark::App(id) => match self.open_in_icons.borrow().get(id).cloned() {
                Some(image) => gpui::img(image.clone()).size(size).into_any_element(),
                None => crate::icons::toolbar("ui/title-open-in.svg")
                    .size(size)
                    .text_color(gpui::rgb(self.theme.text.secondary))
                    .into_any_element(),
            },
            // Disabling navigation does not hide ongoing work. The canvas
            // checks paint visibility and reduced motion before requesting frames.
            Mark::Status(session) => {
                crate::view::activity::mark(self.status_of(*session), theme, true, false)
            }
            // The star the Log's branch column and filter put on the
            // branch `HEAD` is on: one mark, one meaning.
            Mark::Tile(name) => {
                crate::view::column::letter_tile(theme, name, self.rem_px(size)).into_any_element()
            }
            Mark::Star => crate::icons::glyph("ui/star-filled.svg", crate::theme::size::BODY)
                .text_color(if disabled {
                    faint
                } else {
                    gpui::rgb(theme.accent.brass)
                })
                .into_any_element(),
        })
    }

    /// Record what the right button went down on.
    ///
    /// The component builds its menu one frame later, in a `defer`, so
    /// this has always landed by then.
    pub(crate) fn note_menu_target(&mut self, target: Target) {
        self.menu_target = Some(target);
    }

    /// Whether the menu up is about a row `about` accepts. The menu takes
    /// the keys while it is up, and a list's selection keeps its focused
    /// colour under its own menu — decision 54.
    pub(crate) fn menu_over(&self, about: impl Fn(&Target) -> bool) -> bool {
        self.context_menu
            .as_ref()
            .is_some_and(|open| about(&open.target))
    }

    /// Open the menu about what the right button just went down on —
    /// M12.2 task 8.
    ///
    /// **Guild's own menu, not the component's.** A surface calls this
    /// from its right button; the row under the pointer recorded itself a
    /// moment earlier in the same press, because a child hears a press
    /// before its parent. A surface pressed where no row is has recorded
    /// nothing and opens nothing — the target is taken, never reused.
    pub(crate) fn open_context_menu(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        let Some(target) = self.menu_target.take() else {
            return;
        };
        self.open_menu(target, Anchor::Pointer(at), window, cx);
    }

    /// Open the menu about `target`, hung where `anchor` says — M12.7.
    ///
    /// The right button's door and every trigger's: a button that opens a
    /// list hands its own bounds, and the list hangs under or over it.
    pub(crate) fn open_menu(
        &mut self,
        mut target: Target,
        anchor: Anchor,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        if matches!(target, Target::Repository { .. }) {
            let resolved = self.repository_menu_folder(&target);
            if let Target::Repository { folder, .. } = &mut target {
                *folder = resolved;
            }
        }
        if !matches!(target, Target::NewTab { .. }) {
            self.tab_launcher.visible = false;
        }
        if let Target::Change { path, .. } = &target
            && !self
                .changes()
                .is_some_and(|state| state.selection.contains(path))
        {
            self.close_context_menu(window, cx);
            self.warn(self.ui().text(Text::CommitSelectionChanged).to_owned());
            cx.notify();
            return;
        }
        let entries = self.entries_for_target(&target, cx);
        if entries.is_empty() {
            return;
        }
        let commit = if matches!(target, Target::Change { .. }) {
            changes::Snapshot::capture(self, entries)
        } else {
            None
        };
        let front = match &target {
            Target::Tab { id, .. } => self.active_tab == Some(*id),
            // A tab's terminal is drawn only while its tab is in front, and
            // the focus comes back to it; the Output's never gets it back.
            Target::Terminal {
                at: TerminalAt::Tab(id),
                ..
            } => self.active_tab == Some(*id),
            // `f4` opens the selected row, and the right button selected
            // this one before its menu — decision 54 — unless it kept a
            // group; *Open* shows the stroke only on the selected row.
            Target::TreeRow { path, .. } => {
                self.tree().and_then(|state| state.selected.as_deref()) == Some(path.as_path())
            }
            Target::Worktree {
                project: Some(project),
                ..
            } => self.active_project == Some(*project),
            Target::Repository { folder, .. } | Target::OpenIn { folder, .. } => self
                .active_project
                .and_then(|id| self.desk.project(id))
                .is_some_and(|project| project.path == *folder),
            _ => false,
        };
        // A tab of several panes: `cmd-w` closes one of them, not the tab.
        let divided = matches!(&target, Target::Tab { id, .. }
            if self.tabs.iter().any(|tab| tab.id == *id && tab.panes.ids().len() > 1));
        let front = Front { front, divided };
        let cap = match &target {
            Target::LogFilter(_)
            | Target::ProjectSort { .. }
            | Target::SessionFilter { .. }
            | Target::TerminalJobScopes { .. }
            | Target::WorktreeActivity { .. } => Some(crate::theme::space::BRANCH_LIST),
            _ => None,
        };
        // Every field of ours goes: two things taking the keyboard at once
        // is one of them drawn and the other one typed into.
        //
        // Nested Find history and Terminal Jobs scopes hang over their
        // parent popup. Keep that card and its field alive, replace only
        // the previous menu, and return focus to the parent when closed.
        if matches!(
            target,
            Target::FindHistory { .. } | Target::TerminalJobScopes { .. }
        ) {
            self.context_menu = None;
        } else if !self.tab_launcher.visible {
            self.put_fields_away();
        }
        let age_refresh = matches!(target, Target::Agents { .. }).then(|| {
            cx.spawn(async move |root, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(30))
                        .await;
                    if root
                        .update(cx, |root, cx| {
                            let open = matches!(
                                root.context_menu.as_ref().map(|m| &m.target),
                                Some(Target::Agents { .. })
                            );
                            if open {
                                cx.notify();
                            }
                            open
                        })
                        .ok()
                        != Some(true)
                    {
                        break;
                    }
                }
            })
        });
        self.context_menu = Some(Open {
            commit,
            target,
            anchor,
            front,
            lit: None,
            submenu: None,
            sub_lit: None,
            in_submenu: false,
            scroll: gpui::ScrollHandle::new(),
            sub_scroll: gpui::ScrollHandle::new(),
            cap,
            opened_at: window.mouse_position(),
            agent_selected: None,
            _agent_age_refresh: age_refresh,
        });
        // The menu reads its keys where every key passes, and a binding is
        // dispatched before that — so the focus comes here, or an editor
        // behind it would take `enter` as a newline.
        if !self.tab_launcher.visible {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Put the menu away, and the focus back on what is drawn.
    pub(crate) fn close_context_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        if let Some(open) = self.context_menu.take() {
            let transient = self.tab_launcher.visible;
            self.tab_launcher.visible = false;
            if !transient {
                self.refocus_after_menu(&open.target, window, cx);
            }
        }
    }

    /// Put the menu away because the active project or workspace is about
    /// to be a different one, or the project it was about is gone —
    /// M12.7 final fix: a menu built from a frame-old snapshot (the Log's
    /// filters, Find in Files' history, the agents list, the projects
    /// popup itself) would otherwise go on acting on whatever project was
    /// active when it opened — pick a branch in project A's Log after
    /// switching to B, and B's Log narrows to A's branch.
    ///
    /// **No `Window` here, unlike `close_context_menu`.** `select_project`
    /// and `close_project_in` are reached from `guild`'s own door
    /// (`root/command.rs`), which runs on the watch loop's clock and has
    /// none — the same reason `select_workspace` already puts every field
    /// away through `put_fields_away` rather than through a refocusing
    /// close.
    ///
    /// **The focus is not given back here, but it is asked for** —
    /// residual fix: `open_menu` focuses `self.focus`, the root's own
    /// handle, which is drawn every frame — so a menu closed by a switch
    /// leaves nothing for `on_focus_lost` to notice, unlike a tab or a
    /// panel going away under a focused element. A person who opened a
    /// menu with the mouse and then pressed a stroke that switches
    /// projects — `NextProject`'s own — was left with the keyboard
    /// answering nobody. `focusing_front` is `open_tab`'s own mechanism,
    /// drained and applied to whatever the switch brought to the front on
    /// the render its caller's own `cx.notify()` brings — every caller of
    /// this method sends one, whether from the window or from `guild`'s
    /// CLI door. **Only when a menu was actually there**: a background
    /// switch from `guild` with nothing open must not steal focus from
    /// whatever the person is doing.
    pub(crate) fn close_menu_for_switch(&mut self) {
        self.tab_launcher.visible = false;
        let terminal_list_closed = self.terminal_list.take().is_some();
        if self.context_menu.take().is_some() || terminal_list_closed {
            self.focusing_front = true;
        }
    }

    /// Put the focus back on what a menu was opened over.
    ///
    /// **Find in Files' history is the one menu opened over a field of
    /// ours rather than over the window** — M12.7 task 7: its popup keeps
    /// `Root.picker` the whole time the menu is up, so the keyboard goes
    /// back to its own card — or nowhere, if the popup closed meanwhile —
    /// rather than to the tab in front, which `refocus` would otherwise
    /// give it.
    fn refocus_after_menu(
        &mut self,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        if matches!(target, Target::Change { .. })
            && let Some(state) = self.changes()
        {
            window.focus(&state.focus, cx);
            cx.notify();
            return;
        }
        if let Target::Terminal {
            at: TerminalAt::Panel(project, id),
            ..
        } = target
            && self.active_project == Some(*project)
            && self.tool_is_up(guild_session::Tool::Terminal)
            && let Some(entry) = self
                .panel_terminals
                .get(project)
                .and_then(|tabs| tabs.active())
            && entry.id == *id
        {
            use gpui::Focusable;
            // Already drawn: restore now, before a Find deed focuses its field.
            window.focus(&entry.value.read(cx).focus_handle(cx), cx);
            cx.notify();
            return;
        }
        if let Target::TerminalJobScopes { popup } = target {
            if let Some(list) = self
                .terminal_list
                .as_ref()
                .filter(|list| &list.focus == popup)
            {
                list.focus.focus(window, cx);
                cx.notify();
            }
            return;
        }
        if let Target::ActionBarAdd {
            source,
            return_focus,
        } = target
        {
            if self.action_bar_settings_source() == Some(*source)
                && let Some(focus) = return_focus
                && self.focus.contains(focus, window)
            {
                focus.focus(window, cx);
                cx.notify();
            } else {
                self.refocus(window, cx);
            }
            return;
        }
        if let Target::WorktreeActivity {
            return_focus: Some(focus),
            ..
        } = target
        {
            focus.focus(window, cx);
            cx.notify();
            return;
        }
        if matches!(target, Target::FindHistory { .. })
            && let Some(open) = self.picker.as_ref()
        {
            window.focus(&open.focus, cx);
            cx.notify();
            return;
        }
        self.refocus(window, cx);
    }

    /// Take an item of the menu or of its open submenu.
    pub(crate) fn take_menu_item(
        &mut self,
        entry: Entry,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        let deed = match entry {
            Entry::Item(_, deed) | Entry::NamedItem(_, deed) => Some(deed),
            Entry::Choice(choice) if choice.enabled => Some(choice.deed),
            _ => None,
        };
        if let Some(deed) = deed {
            self.take_deed(deed, window, cx);
        }
    }

    /// Close the menu and carry out `deed` on the target it was about.
    ///
    /// What every row's own click does, and what a row's `trailing`
    /// control does too — the project popup's ✕, M12.7 task 5, which
    /// takes a deed of its own rather than the row's.
    fn take_deed(&mut self, deed: Deed, window: &mut Window, cx: &mut Context<crate::Root>) {
        let Some(open) = self.context_menu.take() else {
            return;
        };
        self.refocus_after_menu(&open.target, window, cx);
        if open
            .commit
            .as_ref()
            .is_some_and(|snapshot| !snapshot.valid(self, deed))
        {
            self.warn(self.ui().text(Text::CommitSelectionChanged).to_owned());
            cx.notify();
            return;
        }
        self.run_deed(deed, &open.target, window, cx);
        cx.notify();
    }

    /// A key pressed while the menu is up. Every key is the menu's: a
    /// letter that reached the terminal behind it would be typed into a
    /// session the person was not looking at.
    pub(crate) fn context_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) -> bool {
        let Some(open) = self.context_menu.as_ref() else {
            return false;
        };
        if matches!(open.target, Target::Agents { .. }) {
            return self.agents_key(event, window, cx);
        }
        let main = self.entries_for_open(open, cx);
        let inner = open.submenu.and_then(|index| match main.get(index) {
            Some(Entry::Submenu(_, inner)) => Some(inner.clone()),
            _ => None,
        });
        let key = event.keystroke.key.clone();
        let (lit, sub_lit, in_submenu) = (open.lit, open.sub_lit, open.in_submenu);

        match (key.as_str(), in_submenu, inner.as_ref()) {
            ("escape" | "left", true, _) => {
                if let Some(open) = self.context_menu.as_mut() {
                    open.in_submenu = false;
                    open.submenu = None;
                    open.sub_lit = None;
                }
            }
            ("escape", ..) => {
                self.close_context_menu(window, cx);
                return true;
            }
            // **The footnote's promise**: over a filter dropdown of the
            // Log, Backspace takes that filter off and puts the menu away
            // — the Note at the menu's foot says so. M12.7
            // task 6; no other menu reads it.
            ("backspace" | "delete", ..) if matches!(open.target, Target::LogFilter(_)) => {
                if let Target::LogFilter(list) = &open.target {
                    let which = list.kind();
                    self.close_context_menu(window, cx);
                    self.clear_log_filter(which, cx);
                }
                return true;
            }
            ("down" | "up", true, Some(inner)) => {
                let next = step(inner, sub_lit, key == "down");
                if let Some(open) = self.context_menu.as_mut() {
                    open.sub_lit = next;
                    if let Some(next) = next {
                        open.sub_scroll.scroll_to_item(next);
                    }
                }
            }
            ("down" | "up", false, _) => {
                let next = step(&main, lit, key == "down");
                if let Some(open) = self.context_menu.as_mut() {
                    open.lit = next;
                    open.submenu = None;
                    // Applied at prepaint, so it can be asked for here.
                    if let Some(next) = next {
                        open.scroll.scroll_to_item(next);
                    }
                }
            }
            ("enter", true, Some(inner)) => {
                if let Some(entry) = sub_lit.and_then(|index| inner.get(index)).cloned() {
                    self.take_menu_item(entry, window, cx);
                    return true;
                }
            }
            ("enter" | "right", false, _) => match lit.and_then(|index| main.get(index)).cloned() {
                Some(Entry::Submenu(_, inner)) => {
                    let first = step(&inner, None, true);
                    if let Some(open) = self.context_menu.as_mut() {
                        open.show_submenu(lit);
                        open.in_submenu = true;
                        open.sub_lit = first;
                    }
                }
                Some(entry @ (Entry::Item(..) | Entry::NamedItem(..) | Entry::Choice(..)))
                    if key == "enter" =>
                {
                    self.take_menu_item(entry, window, cx);
                    return true;
                }
                _ => {}
            },
            _ => {}
        }
        cx.notify();
        true
    }

    /// One card of the menu — the menu itself, or its submenu.
    ///
    /// **It scrolls, and only ever when it must** — M12.7: a list of
    /// branches or files can be longer than the window, and a card that
    /// cannot scroll loses its last rows over the edge. The cap is the
    /// window's height less the margin the card is snapped inside, so
    /// every right-button menu of today, which fits, is drawn as it was;
    /// a menu may ask for a lower cap on its `Open` (the Log's filters).
    /// The list inside the card is the scrolling element, so its children
    /// are the entries one for one and `scroll_to_item` takes an entry's
    /// index — a footnote at the foot is held out of it, after them.
    fn render_menu_card(
        &self,
        entries: &[Entry],
        lit: Option<usize>,
        sub: bool,
        front: Front,
        scroll: Option<&gpui::ScrollHandle>,
        cx: &mut Context<crate::Root>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;
        let theme = self.theme;
        let ui = self.ui();
        let icons = has_icons(entries);
        let checks = has_checks(entries);
        let window_cap = (self.viewport().height - gpui::px(2.0 * MARGIN)).max(gpui::px(0.0));
        // **A footnote stays put while the list scrolls** — M12.7 task 6:
        // a footer that says how to undo the thing, and then scrolls away
        // while the list is read, is a footer that is not there when it is
        // wanted. Only a note at the very foot is held out, so the rows
        // are still the scrolling element's children one for one and
        // `scroll_to_item` still takes an entry's index.
        let (rows, foot) = match entries.split_last() {
            Some((Entry::Note(label), rest)) => (rest, Some(label.clone())),
            _ => (entries, None),
        };
        let list = gpui::div()
            .id(if sub { "menu-sub-card" } else { "menu-card" })
            .flex()
            .flex_col()
            // The trap: a flex item's minimum is its content, and a list
            // taller than its box would push the box open rather than
            // scroll.
            .min_h_0()
            .overflow_y_scroll();
        let list = match scroll {
            Some(scroll) => list.track_scroll(scroll),
            None => list,
        };
        let list = list.children(rows.iter().enumerate().map(|(index, entry)| {
            // What an item, a named item and a choice share: a row that
            // is taken. The three differ only in where their words, their
            // mark and their check come from.
            let taken = |label: SharedString,
                         mark: Option<gpui::AnyElement>,
                         deed: Deed,
                         stroke: Option<String>,
                         check: Option<bool>,
                         disabled: bool,
                         trailing: Option<gpui::AnyElement>| {
                let entry = entry.clone();
                let id = (if sub { "menu-sub-item" } else { "menu-item" }, index);
                let content = crate::view::popup::Row {
                    label,
                    mark,
                    icons,
                    check,
                    stroke,
                    loses: loses(deed),
                    submenu: false,
                    lit: lit == Some(index),
                    disabled,
                    trailing,
                };
                let row = if !sub
                    && self
                        .context_menu
                        .as_ref()
                        .is_some_and(|open| matches!(open.target, Target::NewTab { .. }))
                {
                    crate::view::popup::numbered_row(id, content, index + 1, theme)
                } else {
                    crate::view::popup::row(id, content, theme)
                }
                .when(
                    self.context_menu
                        .as_ref()
                        .is_some_and(|open| matches!(open.target, Target::Gutter { .. })),
                    |row| row.px(crate::theme::space::MENU_PADDING),
                );
                // A disabled row is drawn and never taken: no move lights
                // it and no click reaches its deed.
                if disabled {
                    return row.into_any_element();
                }
                row.debug_selector(move || {
                    format!(
                        "{}-{index}",
                        if sub { "menu-sub-item" } else { "menu-item" }
                    )
                })
                // The pointer moves the one highlight, on a move rather
                // than a hover: a hover tint beside a keyboard highlight
                // is two rows lit.
                .on_mouse_move(cx.listener(
                    move |root, event: &gpui::MouseMoveEvent, _window, cx| {
                        let Some(open) = root.context_menu.as_mut() else {
                            return;
                        };
                        // Not a real arrival — the position the menu
                        // opened at, read back. M12.7 final fix.
                        if !moved_since_open(event.position, open.opened_at) {
                            return;
                        }
                        let changed = if sub {
                            open.sub_lit != Some(index) || !open.in_submenu
                        } else {
                            open.lit != Some(index) || open.submenu.is_some()
                        };
                        if sub {
                            open.sub_lit = Some(index);
                            open.in_submenu = true;
                        } else {
                            open.lit = Some(index);
                            open.submenu = None;
                            open.in_submenu = false;
                        }
                        if changed {
                            cx.notify();
                        }
                    },
                ))
                .on_click(cx.listener(move |root, _event, window, cx| {
                    cx.stop_propagation();
                    root.take_menu_item(entry.clone(), window, cx);
                }))
                .into_any_element()
            };
            let printed = |deed: Deed| {
                stroke_shown(deed, front)
                    .and_then(crate::actions::stroke_for)
                    .map(|stroke| crate::actions::printed(&stroke))
            };
            let icon = |deed: Deed| {
                icon_of(deed).map(|icon| crate::view::popup::icon(icon, loses(deed), theme))
            };
            match entry {
                Entry::Item(text, deed) => taken(
                    ui.text(*text).to_owned().into(),
                    icon(*deed),
                    *deed,
                    printed(*deed),
                    check_of(entry, checks),
                    false,
                    None,
                ),
                Entry::NamedItem(label, deed) => taken(
                    label.clone().into(),
                    icon(*deed),
                    *deed,
                    printed(*deed),
                    check_of(entry, checks),
                    false,
                    None,
                ),
                Entry::Choice(choice) => {
                    // The slot is reserved on every row that carries a
                    // trailing deed, lit or not — only the glyph itself
                    // (and its click) is the lit row's, the branch
                    // popup's bin. Before this the slot only existed on
                    // the lit row, so the row — and the card, which is as
                    // wide as its widest row — grew when the pointer
                    // arrived. `stop_propagation` keeps the row's own
                    // click from also firing — a cross inside a row runs
                    // the row's handler too, and second. M12.7 final fix.
                    let lit_here = lit == Some(index);
                    let trailing = choice
                        .trailing
                        .map(|close| {
                            let slot = gpui::div()
                                .id(("menu-trailing", index))
                                .flex_none()
                                .size(crate::theme::space::MENU_ICON)
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(crate::theme::radius::ROW)
                                .text_size(crate::theme::size::META)
                                .text_color(gpui::rgb(theme.text.faint));
                            if lit_here {
                                slot.cursor_pointer()
                                    .group("menu-trailing")
                                    .child(
                                        crate::icons::glyph(
                                            "icons/close.svg",
                                            crate::theme::size::META,
                                        )
                                        .text_color(gpui::rgb(theme.text.faint))
                                        .group_hover("menu-trailing", |cross| {
                                            cross.text_color(gpui::rgb(theme.accent.rust))
                                        }),
                                    )
                                    .on_click(cx.listener(move |root, _event, window, cx| {
                                        cx.stop_propagation();
                                        root.take_deed(close, window, cx);
                                    }))
                                    .into_any_element()
                            } else {
                                slot.into_any_element()
                            }
                        })
                        .or_else(|| {
                            if matches!(
                                self.context_menu.as_ref().map(|open| &open.target),
                                Some(Target::WorktreeActivity { .. })
                            ) && let Deed::ShowSession(session) = choice.deed
                            {
                                Some(
                                    gpui::div()
                                        .flex_none()
                                        .w(crate::theme::activity::GLYPH)
                                        .h(crate::theme::activity::SLOT_HEIGHT)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .debug_selector(move || {
                                            format!("disclosure-activity-{session}")
                                        })
                                        .child(crate::view::activity::mark(
                                            self.status_of(session),
                                            theme,
                                            true,
                                            false,
                                        ))
                                        .into_any_element(),
                                )
                            } else {
                                None
                            }
                        });
                    taken(
                        self.menu_label(&choice.label),
                        self.menu_mark(&choice.mark, loses(choice.deed), !choice.enabled),
                        choice.deed,
                        // The detail wins the stroke's slot when a row has
                        // both — fix round 1. *Close Area* is an action
                        // with no stroke, so its reason stands alone.
                        choice
                            .detail
                            .as_ref()
                            .map(|detail| self.menu_label(detail).to_string())
                            .or_else(|| printed(choice.deed)),
                        check_of(entry, checks),
                        !choice.enabled,
                        trailing,
                    )
                }
                Entry::Separator => crate::view::popup::separator(theme),
                Entry::Heading(label) => crate::view::popup::heading(self.menu_label(label), theme),
                Entry::Note(label) => {
                    crate::view::popup::note(self.menu_label(label), theme, note_is_preceded(index))
                }
                Entry::Submenu(text, inner) => {
                    // The child is owned by the current parent row. Its anchor is
                    // resolved during this row's prepaint, after snapping/scrolling.
                    let child = self
                        .context_menu
                        .as_ref()
                        .filter(|open| !sub && open.submenu == Some(index))
                        .map(|open| {
                            (
                                self.render_menu_card(
                                    inner,
                                    open.sub_lit,
                                    true,
                                    front,
                                    Some(&open.sub_scroll),
                                    cx,
                                ),
                                context_menu_priority(&open.target) + 2,
                            )
                        });
                    crate::view::popup::row(
                        ("menu-submenu", index),
                        crate::view::popup::Row {
                            label: ui.text(*text).to_owned().into(),
                            mark: None,
                            icons,
                            check: check_of(entry, checks),
                            stroke: None,
                            loses: false,
                            submenu: true,
                            lit: lit == Some(index),
                            disabled: false,
                            trailing: None,
                        },
                        theme,
                    )
                    .debug_selector(move || format!("menu-submenu-{index}"))
                    .relative()
                    .on_mouse_move(cx.listener(
                        move |root, event: &gpui::MouseMoveEvent, _window, cx| {
                            let Some(open) = root.context_menu.as_mut() else {
                                return;
                            };
                            // Not a real arrival — the position the menu
                            // opened at, read back. M12.7 final fix.
                            if !moved_since_open(event.position, open.opened_at) {
                                return;
                            }
                            if open.lit != Some(index) || open.submenu != Some(index) {
                                open.lit = Some(index);
                                open.show_submenu(Some(index));
                                open.sub_lit = None;
                                open.in_submenu = false;
                                cx.notify();
                            }
                        },
                    ))
                    // A submenu opens on a click as well as on hover.
                    // The project menu is commonly opened with the `⋯`
                    // button and then read by clicks; making its next row
                    // hover-only makes *Move to workspace* look dead.
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        cx.stop_propagation();
                        let first = root.context_menu.as_ref().and_then(|open| {
                            root.entries_for_open(open, cx)
                                .get(index)
                                .and_then(|entry| match entry {
                                    Entry::Submenu(_, inner) => step(inner, None, true),
                                    _ => None,
                                })
                        });
                        let Some(open) = root.context_menu.as_mut() else {
                            return;
                        };
                        open.lit = Some(index);
                        open.show_submenu(Some(index));
                        open.in_submenu = true;
                        open.sub_lit = first;
                        cx.notify();
                    }))
                    // Queue from the measured row in the same frame. No stored
                    // coordinate, extra refresh, or previous-row content/anchor pairing.
                    .child(
                        gpui::canvas(
                            move |bounds, window, cx| {
                                if let Some((card, priority)) = child {
                                    // A scrolled-out parent has no visible anchor. Keep the
                                    // child outside the list's clip only while its owner is visible.
                                    if !window.content_mask().bounds.intersects(&bounds) {
                                        return;
                                    }
                                    let mut submenu = gpui::anchored()
                                        .position(bounds.top_right())
                                        .snap_to_window_with_margin(gpui::px(MARGIN))
                                        .child(card)
                                        .into_any_element();
                                    submenu.layout_as_root(
                                        gpui::AvailableSpace::min_size(),
                                        window,
                                        cx,
                                    );
                                    window.defer_draw(
                                        submenu,
                                        gpui::Point::default(),
                                        priority,
                                        None,
                                    );
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .into_any_element()
                }
            }
        }));
        let card = crate::view::popup::card(theme).min_h_0();
        let cap = self.context_menu.as_ref().and_then(|open| open.cap);
        let card = match cap {
            Some(cap) => card.max_h(cap),
            None => card.max_h(window_cap),
        };
        let preceded = note_is_preceded(rows.len());
        card.debug_selector(move || {
            if sub {
                "context-menu-child".into()
            } else {
                "context-menu-main".into()
            }
        })
        .child(list)
        .children(
            foot.map(|label| crate::view::popup::note(self.menu_label(&label), theme, preceded)),
        )
        .into_any_element()
    }

    /// The menu, when one is open: a layer over the window that closes it,
    /// the card at the pointer, and a submenu beside its row.
    pub(crate) fn render_context_menu(
        &self,
        cx: &mut Context<crate::Root>,
    ) -> Option<gpui::AnyElement> {
        use gpui::prelude::*;
        let open = self.context_menu.as_ref()?;
        let main = self.entries_for_open(open, cx);
        let anchor = if matches!(open.target, Target::Agents { .. }) {
            Anchor::Above(self.counter_at.get())
        } else {
            open.anchor
        };
        let (lit, front) = (open.lit, open.front);
        let scroll = open.scroll.clone();

        let viewport = self.viewport();
        let priority = context_menu_priority(&open.target);
        let close = |root: &mut crate::Root, window: &mut Window, cx: &mut Context<crate::Root>| {
            cx.stop_propagation();
            root.close_context_menu(window, cx);
        };
        let backdrop = gpui::deferred(
            gpui::anchored().position(gpui::Point::default()).child(
                gpui::div()
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |root, _event, window, cx| close(root, window, cx)),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |root, _event, window, cx| close(root, window, cx)),
                    ),
            ),
        )
        .priority(priority);
        // Where the card hangs: at the pointer, or from the trigger's
        // corner that faces the way the card opens — flipped to the other
        // corner when the card's own side has no room for it. M12.7
        // residual fix.
        let rem_size = crate::scale::rem_size(self.settings.ui_font_size);
        let estimated_h = if matches!(open.target, Target::Agents { .. }) {
            gpui::px(self.rem_px(crate::theme::space::BRANCH_LIST) + 80.0)
        } else {
            estimated_card_height(main.len(), rem_size)
        };
        let (position, corner) = flip(anchor, viewport.height, gpui::px(MARGIN), estimated_h);
        let placed = gpui::anchored().position(position).anchor(corner);
        let menu = gpui::deferred(placed.snap_to_window_with_margin(gpui::px(MARGIN)).child(
            if matches!(open.target, Target::Agents { .. }) {
                self.render_agents_card(cx)
            } else {
                self.render_menu_card(&main, lit, false, front, Some(&scroll), cx)
            },
        ))
        .priority(priority + 1);

        Some(gpui::div().child(backdrop).child(menu).into_any_element())
    }
    /// What a menu item does when it is chosen.
    ///
    /// Until M8.8 every one of them either opened something the window
    /// can already draw, copied text, or asked the platform to show a
    /// path — decision 3 of M8.3.2, and *no menu writes* was the whole
    /// of it. **That rule is spent.** The file tree makes, renames and
    /// trashes files, and the Commit panel rolls a change back and adds
    /// one to git. What replaces it is the one decision 21 already made
    /// for the writes themselves: a gesture that cannot be taken back
    /// asks first, in a sheet that names what it is about, and the
    /// writes are still only the ones `git::Write` spells.
    pub(crate) fn run_deed(
        &mut self,
        deed: Deed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        match deed {
            Deed::Path(deed) => self.run_path_deed(deed, target, window, cx),
            Deed::Tree(deed) => self.run_tree_deed(deed, target, window, cx),
            Deed::Tab(deed) => self.run_tab_deed(deed, target, window, cx),
            Deed::Change(deed) => self.run_change_deed(deed, target, window, cx),
            Deed::Log(deed) => self.run_log_deed(deed, target, cx),
            Deed::Column(deed) => self.run_worktree_deed(deed, target, window, cx),
            Deed::Project(deed) => self.run_project_deed(deed, target, window, cx),
            Deed::ProjectGroup(deed) => self.run_project_group_deed(deed, target, window, cx),
            Deed::OpenIn(deed) => self.run_open_in_deed(deed, target, cx),
            Deed::Terminal(deed) => self.run_terminal_deed(deed, target, window, cx),
            Deed::Gutter(deed) => self.run_gutter_deed(deed, target, window, cx),
            Deed::Workspace(deed) => self.run_workspace_deed(deed, target, window, cx),
            Deed::Dock(deed) => self.run_dock_deed(deed, target, cx),
            Deed::NewTab(tab) => {
                if let Some(project) = self.active_project {
                    let runs = match strip::harness_of(tab) {
                        Some(kind) => self.runs_for_pick(kind),
                        None => guild_session::Runs::Shell,
                    };
                    self.open_harness_tab(project, runs, window, cx);
                }
            }
            Deed::ShowTab(id) => self.show_tab(id, cx),
            Deed::ShowSession(session) => self.go_to_session(session, cx),
            // The menu is closed and the focus given back before a deed
            // runs, so the action reaches the thing in front and not the
            // window's own handle the menu held.
            Deed::Action(named) => window.dispatch_action(named.action(), cx),
            Deed::ActionBarAdd(item) => {
                if let Target::ActionBarAdd { source, .. } = target
                    && self.action_bar_settings_source() == Some(*source)
                {
                    self.add_action_bar_item(item, window, cx);
                }
            }
            Deed::Setting(value) => self.take_setting(value, cx),
            Deed::ProjectSort(mode) => {
                if let Target::ProjectSort { workspace } = target
                    && self.desk.set_project_sort(*workspace, mode)
                {
                    self.save();
                    cx.notify();
                }
            }
            Deed::TerminalJobScope(scope) => {
                if let Target::TerminalJobScopes { popup } = target
                    && self
                        .terminal_list
                        .as_ref()
                        .is_some_and(|list| &list.focus == popup)
                {
                    self.select_terminal_scope(scope, cx);
                }
            }
            Deed::LogFilter(deed) => self.run_filter_deed(deed, target, cx),
            Deed::Session(deed) => self.run_session_deed(deed, target, window, cx),
            Deed::SessionFilter(pick) => self.run_filter_pick(pick, target, cx),
            // Resolved against the target's own snapshot, not against
            // whatever the popup holds now — the counter's own rule for a
            // menu whose rows are the window's data.
            Deed::UseHistory(field, index) => {
                if let Target::FindHistory { items, .. } = target
                    && let Some(text) = items.get(index)
                {
                    self.find_in_files_put(field, text.clone(), true, window, cx);
                }
            }
        }
    }

    /// Carry out what any row with a path offers.
    fn run_path_deed(
        &mut self,
        deed: PathDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        if matches!(target, Target::Change { .. }) {
            match deed {
                PathDeed::CopyPath | PathDeed::CopyFullPath => {
                    if let Some(state) = self.changes() {
                        let root = self.project_root().unwrap_or_default();
                        let text = if deed == PathDeed::CopyFullPath {
                            state
                                .selection
                                .selected
                                .iter()
                                .map(|p| {
                                    if p.as_os_str().is_empty() {
                                        root.clone()
                                    } else {
                                        root.join(p)
                                    }
                                    .display()
                                    .to_string()
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            state.selection.copied_paths()
                        };
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    return;
                }
                PathDeed::ShowDiff => {
                    self.show_selected_diff(window, cx);
                    return;
                }
                _ => {}
            }
        }
        match deed {
            PathDeed::ShowDiff => {
                self.open_diff(
                    target.path().cloned(),
                    guild_session::Base::Head,
                    window,
                    cx,
                );
            }
            PathDeed::CopyPath => {
                if let Some(path) = target.path() {
                    let text = crate::file_selection::relative_path_text(path);
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            PathDeed::CopyFullPath => {
                if let Some(path) = self.full_path(target) {
                    cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
                }
            }
            PathDeed::Reveal => {
                if let Some(path) = self.full_path(target) {
                    cx.reveal_path(&path);
                }
            }
        }
    }

    /// The file a tab is showing, when one stands behind it.
    ///
    /// A session tab has none, and neither has the diff of a whole tree
    /// — which is what makes half a tab's menu appear or not.
    ///
    /// **And neither has a tab under a dependency's root.** Its path is
    /// relative to that root, and every deed this answer unlocks joins
    /// the path to the *project's*: *Show Diff* would diff this
    /// repository's file of the same name, *Copy Full Path* would put a
    /// path that does not exist on the clipboard, and *Reveal in Finder*
    /// would ask the platform for it. The library tree's own menu is
    /// where such a row is acted on, and it resolves against the right
    /// root — `full_path`'s `Target::LibraryRow` arm. So the path half
    /// of a dependency tab's menu is not drawn at all, which is the same
    /// answer `cmd-d` gives.
    pub(crate) fn tab_path(&self, id: TabId) -> Option<PathBuf> {
        match self.tabs.iter().find(|tab| tab.id == id)?.body()? {
            crate::Body::File {
                path,
                origin: crate::Origin::Project,
                ..
            } => Some(path.clone()),
            crate::Body::Diff { path, .. } => path.clone(),
            crate::Body::Annotate { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    /// The target's path from the root of the disk: the project's, and
    /// the row's under it. `None` when the target has no file.
    fn full_path(&self, target: &Target) -> Option<PathBuf> {
        // A row under a dependency's root is relative to that root, not
        // to the project — joining it to the project would name a file
        // of this repository with the same name. M8.8.3.
        if let Target::LibraryRow { at, path, .. } = target {
            return self.library_path(*at, path);
        }
        let path = target.path()?;
        let project = self.desk.project(self.active_project?)?;
        Some(project.path.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::{Anchor, estimated_card_height, flip, moved_since_open, note_is_preceded};
    use gpui::{Anchor as Corner, bounds, point, px, size};

    /// The exact spot the menu opened at is not a real arrival; anywhere
    /// else is. M12.7 final fix.
    #[test]
    fn a_move_back_to_where_the_menu_opened_is_not_a_real_arrival() {
        let opened_at = gpui::point(gpui::px(10.0), gpui::px(20.0));
        assert!(!moved_since_open(opened_at, opened_at));
        assert!(moved_since_open(
            gpui::point(gpui::px(10.0), gpui::px(21.0)),
            opened_at
        ));
        assert!(moved_since_open(
            gpui::point(gpui::px(11.0), gpui::px(20.0)),
            opened_at
        ));
    }

    /// A footnote with nothing above it draws no rule; one with a row
    /// above it does. Residual fix task 3.
    #[test]
    fn a_lone_footnote_has_no_rule_and_a_followed_one_does() {
        assert!(!note_is_preceded(0));
        assert!(note_is_preceded(1));
        assert!(note_is_preceded(7));
    }

    /// A trigger flush against the window's bottom, with plenty of room
    /// above it: `Below` flips to hang above instead. Residual fix task
    /// 2 — the Log's filter menus in the bottom dock.
    #[test]
    fn a_menu_asked_for_below_flips_above_when_there_is_no_room_under_it() {
        let trigger = bounds(point(px(0.0), px(380.0)), size(px(100.0), px(20.0)));
        let (position, corner) = flip(
            Anchor::Below(trigger),
            px(400.0),
            px(8.0),
            px(200.0), // more than fits below (12px) or above (372px alone would fit, but the choice is below vs above)
        );
        assert_eq!(position, trigger.origin);
        assert_eq!(corner, Corner::BottomLeft);
    }

    /// The same trigger with room to spare below it: `Below` stays put.
    #[test]
    fn a_menu_asked_for_below_stays_below_when_there_is_room() {
        let trigger = bounds(point(px(0.0), px(50.0)), size(px(100.0), px(20.0)));
        let (position, corner) = flip(Anchor::Below(trigger), px(400.0), px(8.0), px(200.0));
        assert_eq!(position, trigger.bottom_left());
        assert_eq!(corner, Corner::TopLeft);
    }

    /// `Above` is `Below`'s mirror: no room over a trigger near the
    /// window's top flips it to hang under the trigger instead.
    #[test]
    fn a_menu_asked_for_above_flips_below_when_there_is_no_room_over_it() {
        let trigger = bounds(point(px(0.0), px(10.0)), size(px(100.0), px(20.0)));
        let (position, corner) = flip(Anchor::Above(trigger), px(400.0), px(8.0), px(200.0));
        assert_eq!(position, trigger.bottom_left());
        assert_eq!(corner, Corner::TopLeft);
    }

    /// The same trigger far enough from the top: `Above` stays put.
    #[test]
    fn a_menu_asked_for_above_stays_above_when_there_is_room() {
        let trigger = bounds(point(px(0.0), px(350.0)), size(px(100.0), px(20.0)));
        let (position, corner) = flip(Anchor::Above(trigger), px(400.0), px(8.0), px(200.0));
        assert_eq!(position, trigger.origin);
        assert_eq!(corner, Corner::BottomLeft);
    }

    /// Neither side has room: the card stays where it was asked for
    /// rather than flip to a side with even less — flipping there would
    /// only move the overlap.
    #[test]
    fn a_menu_that_fits_nowhere_does_not_flip_to_a_worse_side() {
        // 22px above the trigger, 42px below it in a 100px window — both
        // far short of the 200px the card wants, but below is still the
        // roomier of the two, so it is where the card stays.
        let trigger = bounds(point(px(0.0), px(30.0)), size(px(100.0), px(20.0)));
        let (position, corner) = flip(Anchor::Below(trigger), px(100.0), px(8.0), px(200.0));
        assert_eq!(position, trigger.bottom_left());
        assert_eq!(corner, Corner::TopLeft);
    }

    /// The pointer anchor is untouched by the flip: it has no trigger to
    /// flip around.
    #[test]
    fn a_menu_at_the_pointer_is_never_flipped() {
        let at = point(px(12.0), px(34.0));
        let (position, corner) = flip(Anchor::Pointer(at), px(400.0), px(8.0), px(200.0));
        assert_eq!(position, at);
        assert_eq!(corner, Corner::TopLeft);
    }

    /// The estimate grows with the entries and never with nothing to
    /// draw — the card still keeps its own top and bottom padding.
    #[test]
    fn the_estimated_height_grows_with_the_entries() {
        let rem = px(16.0);
        let empty = estimated_card_height(0, rem);
        let some = estimated_card_height(3, rem);
        assert!(empty > px(0.0));
        assert!(some > empty);
    }
}

#[cfg(test)]
mod terminal_panel_tests;

#[cfg(test)]
mod background_activity_tests;

#[cfg(test)]
mod commit_tests;

#[cfg(test)]
mod submenu_tests;
