//! The Guild desktop: an agent workspace with terminals and files.
//!
//! A library with a thin binary on top. `guild-desktop` was a binary
//! crate, and `tests/` cannot reach a single module of one — Cargo builds
//! an integration test against a library, and there was none. Every test
//! living inside the module it checks was a constraint, not a choice.
//!
//! `Root` coordinates native views and adapters. Pure models and the headless
//! session crate carry independently testable behavior; splitting Root's
//! methods into files does not enforce ownership of its shared fields.
//!
//! The desktop works without a server;
//! local settings and project state remain Guild's responsibility. `anyhow`
//! is also used by library adapters, while stable failure contracts use named
//! error types. See `docs/runtime-ownership.md` for lifecycle boundaries.
//!
//! Agent sessions run their CLI in a pseudo-terminal.

use std::time::{Duration, Instant};

use gpui::{Context, FocusHandle, Window};
use guild_session::{Desk, ProjectId, Runs, Seat, SessionId, Settings, Store, WorkspaceId};

pub mod actions;
pub mod agents;
pub mod annotate_tab;
pub mod autosave;
pub mod binding;
pub mod blink;
pub mod branch_list;
pub mod changes;
pub mod clone_project;
pub mod closed;
pub mod closing;
pub mod column;
pub mod console;
pub mod customizer;
pub mod declaration;
pub mod deps;
pub mod diff;
pub mod diff_tab;
pub mod diff_window;
pub mod discovery;
pub mod dock_menu;
pub mod double_tap;
pub mod edit;
pub mod editor;
pub mod everywhere;
pub mod file_index;
pub mod file_operations;
mod file_scroll;
pub mod file_selection;
pub mod file_tab;
pub mod files_tree;
pub mod find_in_files;
pub mod fonts;
pub mod forge;
pub mod frame;
pub mod fronts;
pub mod fuzzy;
pub mod git;
pub mod grammars;
pub mod gutter;
pub mod highlight;
pub mod hooks;
pub mod hunk;
pub mod i18n;
pub mod icons;
pub mod ide;
pub mod jars;
pub mod line_changes;
pub mod log_graph;
pub mod log_layout;
pub mod logging;
pub mod lsp;
pub mod menu;
#[cfg(feature = "native-preview")]
pub mod native_preview;
pub(crate) mod network;
#[cfg(feature = "notice-preview")]
pub mod notice_preview;
pub mod notify;
pub mod open_in;
pub mod panel_corners;
pub mod paths;
pub mod pick;
pub mod picker;
pub mod preview;
mod process_view;
pub mod push_dialog;
mod review_activity;
pub mod root;
pub mod run;
pub mod scale;
pub mod search;
pub mod search_tree;
pub mod session_list;
pub mod sourcekit;
pub mod split;
pub mod stage;
pub mod start;
pub mod swipe;
pub mod symbols;
pub mod syntax;
pub mod system;
pub mod terminal;
pub mod terminal_job;
pub mod terminal_panel;
pub mod text_field;
pub mod theme;
pub mod title;
pub mod tree;
pub mod update;
pub mod usage;
pub mod usages;
pub mod view;
pub mod watch;
pub mod windows;
pub mod workspaces;

/// Quit, once every queued save is on the disk. The state is written on
/// a thread of its own so that a tab switch does not wait for a synced
/// write; a quit a moment after a change must not lose the change.
///
/// **Every quit of Guild's own comes through here**, and it tells AppKit's
/// `applicationShouldTerminate:` so first: the questions have been asked,
/// or there were none, and the `terminate:` this sends must not be
/// cancelled to ask them again. M5.11's decision 20.
pub fn quit(cx: &mut gpui::App) {
    system::allow_termination();
    cx.quit();
}

pub use actions::{
    About, AddProject, ClearRecent, ClearTerminal, ClosePanel, CloseProject, CloseTab,
    DecreaseFontSize, IncreaseFontSize, NewProject, NewSession, NewTerminal, NextProject, NextTab,
    NextWorkspace, OpenRecent, OpenSettings, PreviousTab, PreviousWorkspace, ReopenTab,
    ResetFontSize, SelectProject, ShowLog, ToggleFullScreen, ToggleSessions,
};
use closed::ClosedTabs;
use i18n::{Text, Ui};
use view::settings::Section;

/// How often a terminal tab checks whether a turn happened.
///
/// Walking the transcript directory is not something to do every frame.
const PROBE: Duration = Duration::from_secs(2);

/// How often the active project's branch is re-read.
///
/// The same two seconds: a branch switched in a terminal shows up about as
/// fast as a person looks up at the title bar.
const BRANCH_PROBE: Duration = Duration::from_secs(2);

/// The root of the interface.
pub struct Root {
    diagrams: gpui::Entity<diagrams::runtime::Manager>,
    _diagram_watch: gpui::Subscription,
    integrations: root::isolation::Policy,
    // Tests draw the production frame with isolated state, without background services.
    #[cfg(test)]
    isolated_render: bool,
    #[cfg(test)]
    titlebar_double_clicks: usize,
    #[cfg(test)]
    titlebar_drag_starts: usize,

    settings: Settings,
    network_check: network::Check,
    /// The single source of truth about projects and sessions.
    ///
    /// The tabs below hold only what cannot be saved — live processes and
    /// views.
    desk: Desk,
    /// Workspace whose seats are currently drawn.
    active_workspace: WorkspaceId,
    /// The trackpad gesture over the sidebar, measured and classified by
    /// M12.5's pure detector.
    workspace_swipe: swipe::Detector,
    workspace_swipe_epoch: std::time::Instant,
    workspace_swipe_offset: f32,
    workspace_swipe_landing: u64,
    /// Workspaces whose persisted seat tabs have been materialised in this
    /// window. Hidden tabs stay live; this only prevents restoring them twice.
    restored_workspaces: std::collections::HashSet<WorkspaceId>,
    active_project: Option<ProjectId>,
    /// The live tabs of **every** project: switching projects must not
    /// stop other projects' sessions.
    tabs: Vec<Tab>,
    active_tab: Option<TabId>,
    /// Whether the settings are shown.
    /// What every session's agent is doing, as far as anything has said.
    ///
    /// Everything that shows a status reads this board.
    board: guild_session::status::Board,
    /// Managed live input's private witness of a Codex turn — hooks, the
    /// rollout and local interrupts, as the board was fed before OSC 7501.
    /// Never displayed; read only by `continuation_live_gap` and the
    /// rollout probe's human-wait guard (`docs/runtime-ownership.md`).
    latch: guild_session::status::Latch,
    /// Workflow turn starts seen before their session's conversation id was
    /// known, sent once it is — `Root::conversation_admitted`.
    unbound_starts: std::collections::HashSet<SessionId>,
    done_heard: guild_session::status::DoneCompletions,
    /// Which agent sessions run — alive and used since their tab opened —
    /// and so keep their project in Recent activity's top block.
    running: guild_session::project_sort::Running<SessionId>,
    /// When each column group became active, by workspace and group key —
    /// Recent activity's top block. Moved by `Root::note_active_groups`
    /// after the board is, read by `column_groups_for`; in memory only.
    active_groups:
        guild_session::project_sort::Active<(guild_session::WorkspaceId, std::path::PathBuf)>,
    sound_player: root::status::SoundPlayer,
    /// Whether the window has focus — whether a result counts as seen
    /// depends on it, and whether the worktrees are polled. Written by
    /// the activation observer, never by a paint: `Root::watch_activation`
    /// says why.
    window_focused: bool,
    /// Whether the window is in the system's full screen.
    ///
    /// The traffic lights are gone there — macOS slides them away with the
    /// menu bar — and the space they were holding is ours to use.
    fullscreen: bool,
    /// Which settings section is open.
    /// Tabs closed during this run, for bringing one back.
    closed: ClosedTabs,
    /// Each seat's areas, the one with the keys, and the tab in front of
    /// each area, so that coming back to a project brings that one
    /// forward and not the strip's last — M12.8, in place of the one front
    /// per seat `fronts` kept since 2026-09-26. `stage` says why.
    stage: stage::Stage<Seat, TabId>,
    /// The tab under the pointer — M12.6. Kept here rather than as a
    /// `group_hover` on the name: a text colour is fixed when the text is
    /// laid out, before a group's hitbox is registered at paint, so a
    /// hovered group never recoloured its text.
    hovered_tab: Option<TabId>,
    /// The most recent accepted drop, transient and never persisted.
    tab_landing: Option<(TabId, view::tabs::landing::Landing)>,
    /// The windows open beside this one — M8.3.6.
    satellites: windows::Registry<gpui::AnyWindowHandle>,
    /// Satellites asked for and not yet open — the open is deferred.
    satellites_opening: std::collections::HashSet<windows::Satellite>,
    /// Push Commits, while it is open — what `push_landed` tells.
    /// Shared with the window's builder, which fills it once the window
    /// exists.
    /// The diff window, while it is open.
    diff_window:
        std::rc::Rc<std::cell::RefCell<Option<gpui::WeakEntity<view::diff_window::DiffWindow>>>>,
    /// This window — what a satellite asks to bring forward and to open a
    /// file in. Set at the first render.
    main_window: Option<gpui::AnyWindowHandle>,
    push_dialog:
        std::rc::Rc<std::cell::RefCell<Option<gpui::WeakEntity<view::push_dialog::PushDialog>>>>,
    /// The size a new tab's terminal is spawned at: the last fit of any tab
    /// in the body, seeded from the settings — 2026-09-16.
    start_size: terminal::view::StartSize,
    /// The popup that is up, whichever it is — M8.9's one picker.
    ///
    /// **Only one popup can be up because there is one field for it.** An
    /// `Option` rather than a flag beside a query, too: closed and "open
    /// with nothing typed" are different states, and a flag lets a query
    /// outlive the popup and filter a list the person did not filter.
    picker: Option<root::picker::Open>,
    /// The Settings page's right column, which scrolls.
    ///
    /// Until M5.11 it did not, and the Keymap — one row per action — ran
    /// past the bottom of the window with its last rows out of reach.
    settings_group_positions: std::rc::Rc<std::cell::RefCell<Vec<(i18n::Text, gpui::Pixels)>>>,
    settings_query: String,
    settings_modified: bool,
    settings_width: std::rc::Rc<std::cell::Cell<f32>>,
    settings_search_focus: gpui::FocusHandle,
    settings_scroll: gpui::ScrollHandle,
    action_bar_scroll: view::settings::OrderScroll,
    /// The Settings page `settings_scroll` was last drawn for. The handle is
    /// one for every Settings tab and the window's page, so a page that
    /// comes to the front puts it back to the top.
    settings_scroll_host: Option<view::settings::SettingsHost>,
    /// The Settings page drawn by the window itself, on this section, when
    /// no project is in front — M5.11's decision 15.
    ///
    /// A tab belongs to a project and there is none, and `Tab.project`
    /// becoming an `Option` would reach every reader of a tab for one empty
    /// window. So the page is drawn in the body in place of the hints, and
    /// it goes by itself when a project comes to the front: every change on
    /// it is already saved.
    settings_page: Option<Section>,
    /// The action whose shortcut is being recorded, if any.
    ///
    /// While this is set the window's own shortcuts stand down — see the
    /// `!recording` predicate in `actions::rebind`. The stroke being
    /// pressed is the answer to a question, not a command.
    recording: Option<actions::keymap::Named>,
    /// The action that already owns the stroke just pressed.
    conflict: Option<actions::keymap::Named>,
    /// The key just pressed needs `cmd`, `ctrl` or `alt` to be a shortcut,
    /// and the slot says so while the recording goes on. M5.11's
    /// decision 14.
    stroke_refused: bool,
    /// What the window is telling the person right now.
    ///
    /// Failures used to go to the log and stop, which from outside is
    /// indistinguishable from the button doing nothing.
    notices: notify::Notices,
    notice_pointer_exited: bool,
    notice_views: std::cell::RefCell<std::collections::HashMap<usize, view::notices::NoticeView>>,
    /// A question about unsaved text is on the screen. A second `cmd-w`
    /// meanwhile must not queue a second sheet for the same tab.
    asking: bool,
    /// Hears the window stop being the active one: a blur of the whole
    /// window, and autosave's cue.
    activation: Option<gpui::Subscription>,
    /// How many key downs and mouse presses the window has heard — the
    /// count the double Shift compares between two taps (M8.9, decision 8).
    /// The ears only add one to it, so neither updates `Root` from inside
    /// another dispatch; `shift_heard` reads it.
    heard: std::rc::Rc<std::cell::Cell<u64>>,
    /// The regions drawn as views of their own so that a frame can reuse
    /// them — `view::region`. Made the first time each is drawn: a view
    /// needs the root's entity, and `Root::new` has none yet.
    regions: std::collections::HashMap<view::region::Part, gpui::Entity<view::region::Region>>,
    /// The ear for every key down: `cx.intercept_keystrokes`, which runs
    /// before the bindings are matched — a key a binding takes (`shift-f7`)
    /// never reaches a key listener. Dropping it removes the interceptor.
    #[allow(
        dead_code,
        reason = "held so the interceptor lives as long as the window"
    )]
    ears: Option<gpui::Subscription>,
    /// The double Shift's detector. M8.9, decision 8.
    #[allow(dead_code, reason = "filled by M8.9 group A")]
    double_shift: double_tap::DoubleTap,
    /// Go to File's index of every project. M8.9, decision 9.
    #[allow(dead_code, reason = "filled by M8.9 group B")]
    file_indexes: root::file_index::Indexes,
    /// Go to Symbol's index of every project. M8.9, decision 11.
    #[allow(dead_code, reason = "filled by M8.9 group B")]
    symbols: root::symbols::Symbols,
    overlays: guild_session::overlay::Board,
    terminal_jobs: terminal_job::LiveJobs,
    review_activity: review_activity::Activity,
    continuation_receipts: guild_session::resume_handoff::Ledger,
    continuation_starts: std::collections::HashMap<
        guild_session::SessionId,
        (guild_session::resume_handoff::Attempt, gpui::EntityId, u64),
    >,
    workflows: Option<root::workflow::Queue>,
    processes: root::process::Views,
    terminal_job_error: Option<String>,
    terminal_focus: Option<guild_session::terminal_job::JobId>,
    terminal_list: Option<root::terminal_jobs::List>,
    terminal_geometry: std::collections::HashMap<
        guild_session::terminal_job::JobId,
        Option<guild_session::terminal_job::Geometry>,
    >,
    overlay_focus: std::cell::RefCell<std::collections::HashMap<String, root::overlay::Focus>>,
    /// The previews being made for a popup's lit row. M8.9, decision 13.
    #[allow(dead_code, reason = "filled by M8.9 group C")]
    previews: root::preview::Previews,
    /// The Find tool's preview, beside its list. M8.15.
    find_previews: root::preview::Previews,
    /// Every project's Find tool: its searches, a tab each. M8.15.
    finds: root::usages::Finds,
    /// The balloon over the caret, when Find Usages has something to say
    /// there. M8.15.
    balloon: Option<root::usages::Balloon>,
    /// What the tree's Kotlin and Java files declare, read off the
    /// window's thread. M8.13.
    declarations: root::declarations::Declarations,
    /// What each open project depends on, asked of its build systems.
    /// M8.14.
    dependencies: root::dependencies::Dependencies,
    /// `cmd-k` asked for the commit message's box, which is focused at
    /// the next render that has it.
    commit_message_wanted: bool,
    /// Heard when the application is going: every process the window
    /// owns is stopped before it does.
    quitting: Option<gpui::Subscription>,
    /// Hears the focus go to something no longer drawn — a tab that went
    /// behind another — and brings it back to what is.
    focus_lost: Option<gpui::Subscription>,
    /// What the window has told git, per project.
    ///
    /// Per project because the Git tool is, and because a console
    /// holding two repositories' commands would need every line to say
    /// which — which is a column nobody reads for the one case in a
    /// hundred where it differs.
    consoles: std::collections::HashMap<ProjectId, crate::console::Console>,
    /// The Console's scrolling.
    ///
    /// **Newest at the bottom is only newest if it can be seen.** A
    /// console longer than its box keeps the offset it had, so the answer
    /// to the button just pressed lands below the fold — the one line the
    /// person pressed it to read. One handle rather than one per project:
    /// only one console is on screen.
    console_scroll: gpui::ScrollHandle,
    /// The Console as the component's editor, read-only, and the text last
    /// set in it — 2026-09-23, so what git said can be selected and
    /// copied. One, as the scroll is one: only one console is on screen.
    /// The display the main window stands on, and where on it — read at
    /// every render, so that a window beside it opens on the same screen
    /// and over it. 2026-09-24: Push Commits opened on the other monitor.
    main_display: Option<(gpui::DisplayId, gpui::Bounds<gpui::Pixels>)>,
    console_view: Option<(gpui::Entity<gpui_component::input::TextareaState>, String)>,
    /// Where each dock's panel head ended up, by `guild_session::Dock` as
    /// `usize`, for the ⋮'s menu to hang under — M12.7, §6.17's *four
    /// under the head*. The ⋮ itself is a trigger of the right button's
    /// engine now, which keeps whether one is open in `context_menu`.
    dock_head_at: std::rc::Rc<std::cell::Cell<[Option<gpui::Bounds<gpui::Pixels>>; 3]>>,

    /// A language server per project and language, started when a file
    /// of that language is opened and stopped with the project. Task 0
    /// of M8.5 measured rust-analyzer at 3.5 GB, which is why none of
    /// this is eager.
    servers: std::collections::HashMap<root::lsp::Key, root::lsp::Server>,
    scoped_settings: root::repository_settings::State,
    #[cfg(test)]
    test_server: Option<(String, Vec<String>)>,
    #[cfg(test)]
    test_launch_fallback: Option<(String, Vec<String>)>,
    /// Where a jump to a definition is waiting to land: the caret is
    /// put there when that file's editor exists, which for a restored
    /// tab is its first render.
    landing: Option<(ProjectId, std::path::PathBuf, lsp_types::Position)>,
    /// What a server says is wrong with each file, by project and path.
    /// **Kept whether or not a tab is open**: a server publishes for
    /// every file it has an opinion about, and LSP has no way to ask it
    /// again.
    diagnostics: std::collections::HashMap<root::lsp::Path_, Vec<lsp_types::Diagnostic>>,
    /// Every file a server has been told about, by project and path:
    /// its version, its buffer, and the subscription that hears it
    /// change. Dropped when the tab closes, which is how the window
    /// stops listening.
    documents: std::collections::HashMap<root::lsp::Path_, root::lsp::Document>,
    document_work: root::lsp::DocumentWork,
    /// How many times a language's server has died in this session: at
    /// three it is not started again, because the window starts one
    /// whenever a file wants one and a server that dies as it starts
    /// would be spawned once a frame.
    server_deaths: std::collections::HashMap<root::lsp::Key, u8>,
    /// Where a server was tried and would not start, so that a project
    /// with no rust-analyzer does not spawn one per file opened. What
    /// went wrong is kept, for the status bar. M8.5.1.
    serverless: std::collections::HashMap<root::lsp::Key, lsp::progress::Fault>,
    /// *Set up for Xcode*, per project, while it runs and after it failed.
    xcode_setups: std::collections::HashMap<ProjectId, root::xcode::Setup>,
    /// The run of every project that has one: the line, its terminal and
    /// what became of it. Per project because a run belongs to the
    /// project rather than to the window — switching projects leaves it
    /// running, as closing a tab leaves a session running.
    runs: std::collections::HashMap<ProjectId, run::tabs::RunTabs<root::run::Run>>,
    run_tab_scroll: gpui::ScrollHandle,
    run_tab_frame: std::rc::Rc<std::cell::Cell<Option<view::run::TabFrame>>>,
    panel_terminal_scroll: gpui::ScrollHandle,
    panel_terminal_width: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    panel_terminal_front: Option<ProjectId>,
    panel_terminals: std::collections::HashMap<
        ProjectId,
        terminal_panel::TerminalTabs<gpui::Entity<terminal::view::TerminalTab>>,
    >,
    /// Which build this Guild is, and the newer one staged beside it —
    /// M5.13, M5.15.
    update: root::update::Update,
    update_settings: view::settings::updates::Draft,
    /// The plans' limits, for the title bar's chip — Claude Code's since
    /// M12.10, Codex's since M12.12.
    usage: root::usage::Usage,
    /// Each worktree's GitLab merge request, and the refreshes out —
    /// GLD-063.
    forge: root::forge::ForgeState,
    /// Whether the usage card is up, and where the chip was drawn, for
    /// the card that hangs under it.
    showing_usage: bool,
    usage_at: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// The agent whose limits the chip showed last — what a file or a
    /// shell in front keeps showing, decision 47. A cell because the
    /// chip is drawn from `&self`, as `usage_at` is.
    usage_last: std::cell::Cell<agents::Agent>,
    /// Whether the card's *All providers* is open; open until folded,
    /// and not saved.
    usage_all_open: bool,
    /// Where the Open In button was drawn, for the menu under it; the
    /// installed applications and their icons, loaded before the first render.
    /// Opening gestures revalidate the selected bundle and refresh this cache
    /// if it disappeared; rendering itself does not poll Launch Services.
    open_in_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    open_in_apps: Option<Vec<open_in::Connection>>,
    #[cfg(test)]
    open_in_launches: Option<Vec<(guild_session::settings::OpenInTarget, std::path::PathBuf)>>,
    #[cfg(test)]
    open_in_availability: Option<std::collections::HashSet<String>>,
    open_in_icons:
        std::cell::RefCell<std::collections::HashMap<String, std::sync::Arc<gpui::Image>>>,
    open_in_inventory: Option<std::sync::Arc<Vec<system::InstalledApplication>>>,
    open_in_inventory_task: Option<gpui::Task<()>>,
    open_in_picker: Option<view::settings::AppPicker>,
    open_in_new_custom: Option<String>,
    /// What each project has been asked to run, cached over what
    /// `.guild/` holds — the shape `panel_state` has, and for the same
    /// reason: the panel reads it every frame and a disk read per frame
    /// is work nobody asked for.
    commands: std::collections::HashMap<ProjectId, guild_session::Commands>,
    /// What each project's task editors or Output declare it can run: Just,
    /// Make, package scripts and Taskfile tasks. Asked off-thread once per
    /// project, including restored sources, then invalidated by source/imports.
    declared: std::collections::HashMap<ProjectId, root::run::Declared>,
    /// The socket `claude --ide` connects to, once there is a project
    /// to name in its lock file. M8.6. `None` when nothing is listening
    /// — a window with no project, or a port that would not bind.
    ide: Option<ide::server::Server>,
    /// The hook package and the socket it posts to — M5.10. `None` when
    /// either could not be made; the window works without them, and its
    /// terminal tabs simply say nothing about their agents.
    hooks: Option<root::status::HookState>,
    /// What the Dock's badge was last told, so AppKit hears only when the
    /// number moves. M5.10.
    dock_badge: usize,
    /// Every repository's worktrees, by its common git directory — M12.
    /// What the column draws its rows from; a repository nobody has
    /// opened a root of is not in it.
    worktrees: std::collections::HashMap<std::path::PathBuf, Vec<git::worktrees::Worktree>>,
    /// When the worktrees were last asked for. A window nobody is looking
    /// at asks for nothing, and one that is asks every
    /// `root::git::WORKTREES_EVERY` — for the list alone, since
    /// 2026-09-24.
    worktrees_asked: Option<std::time::Instant>,
    worktree_reads: root::git::WorktreeReads,
    /// Hunk writes in flight per project, and a commit held for them.
    hunk_flights: std::collections::HashMap<ProjectId, root::hunks::Flight>,
    /// The projects whose disk moved since the last ask, which the next
    /// one asks to count again: an edit changes a row's `+N −M` and not
    /// the list, so the cheap poll alone would never see it. Asked on the
    /// cadence rather than per batch, which the watcher sends ten times a
    /// second while an agent writes.
    worktrees_stale: std::collections::HashSet<guild_session::ProjectId>,
    /// The hidden column is out because the pointer is at the edge — M12
    /// task 10.
    ///
    /// **Not saved, and that is the point.** Where the pointer happens to
    /// be is not a choice a person made, and a window that came back
    /// showing a panel nobody asked for would be a bug wearing the clothes
    /// of a restoration. `column_folded` in the settings is the choice.
    column_peeking: bool,
    /// A native row drag owns cancellation without moving keyboard focus.
    worktree_dragging: bool,
    project_drag: view::column::project_drag::State,
    /// The column's items drawn as spacers far from its viewport.
    column_spacers: view::column::spacers::Spacers,
    tab_drag: view::tabs::drag::State,
    action_bar_dragging: bool,
    /// The column's own border is being dragged — M12 task 11.
    ///
    /// A flag and not a point: the column is against the window's left
    /// edge, so the pointer's x **is** the width and there is no offset to
    /// carry from where the drag began. `dragging` beside it is the docks'
    /// and `column_drag` is the Git log's columns — three borders, three
    /// gestures, and a shared field would be a match at every use.
    column_edge_drag: bool,
    /// Writes a `guild` command is waiting on, each with the way back to
    /// the connection holding the line — M13.
    ///
    /// **Answered from git's own line in the Console, matched by the
    /// write's words.** `Write::args` is the one place a write is spelled,
    /// so comparing a Console line's `argv` against it is the whole of the
    /// matching and nothing new has to parse git. An entry whose caller has
    /// given up is still answered when git finishes; the send simply goes
    /// nowhere, which is the right end for a question nobody is waiting on.
    pending_writes: Vec<(
        crate::git::Write,
        std::sync::mpsc::Sender<guild_session::command::Answer>,
    )>,
    /// Answers git has earned and the window has not yet shown — M13.
    ///
    /// **A command answers when its result can be read back, not when git
    /// finishes.** Checked against a running window: `guild worktree
    /// create` said *created*, and the `guild worktree list` straight after
    /// it did not have the new worktree, because the list is re-read every
    /// three seconds and nothing asked it sooner. An agent told *created*
    /// that then cannot find what was created has been told something
    /// false. So the answer waits here, the list is asked for at once, and
    /// the answer goes when the list agrees.
    pending_settle: Vec<(
        crate::root::command::Settles,
        std::sync::mpsc::Sender<guild_session::command::Answer>,
        guild_session::command::Answer,
    )>,
    /// The tasks `guild worktree create --prompt` asked for, by the folder
    /// of the worktree each waits on — M13.1. Started by `answer_settled`
    /// when the list has the folder, the moment `created` would be said;
    /// dropped when git refuses the write.
    pending_tasks: Vec<(std::path::PathBuf, crate::root::command::Task)>,
    /// What a process was started as → the record its tab holds now —
    /// M13.3, decision 52. A tab rebound by `/resume` keeps its process,
    /// and the process keeps its `GUILD_SESSION`; this is what makes that
    /// id mean the tab's record to `guild` and to the hooks. In memory:
    /// after a restart the tab comes back as its record, with that id.
    started_as: std::collections::HashMap<SessionId, SessionId>,
    /// Which tabs were told a conversation is open elsewhere — once each.
    told: crate::binding::Told<TabId>,
    /// Terminal tabs being handed over, so the probe asks once — M13.3.
    handing_over: std::collections::HashSet<TabId>,
    /// The `openDiff` calls waiting for a person, by the tab each one
    /// opened. **The agent is blocked on every one of these**, so a tab
    /// leaving this map without an answer is a session that never moves
    /// again: `close_tab` sends the refusal for a tab closed any other
    /// way.
    parked: std::collections::HashMap<TabId, root::ide::Parked>,
    /// The tab whose name is being typed, and what has been typed — M12.1.
    renaming_tab: Option<(crate::root::tabs::Renamed, String)>,
    /// Where each of the two font buttons was drawn, so its list can hang
    /// from it. One cell per button, indexed by `Which`: both are drawn
    /// in the same frame, and one shared cell would keep whichever was
    /// drawn last.
    font_at: [std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>; 3],
    // ── One field per branch of M8.8, declared by the seams commit. ──
    //
    // **`dead_code` is allowed here on purpose, and each allow names the
    // branch that removes it.** The alternative to declaring these ahead
    // is six branches editing this struct and `Root::new`'s literal, and
    // an unused field is a warning where that is a merge conflict. A
    // stray allow left behind after M8.8 is a greppable mistake; a
    // conflict in `lib.rs` is a day.
    //
    // Each is a `Default`-able type in a module of its own, empty until
    // its branch fills it. They are here rather than in six branches
    // because a field on this struct needs a line in `Root::new`'s
    // literal too — so two branches adding one each would conflict
    // **guaranteed**, in two places, rather than merely likely.
    /// Where the caret has been. M8.8.2.
    history: root::history::History,
    /// The files opened lately. M8.8.2.
    recent_files: root::recent_files::RecentFiles,
    /// What the servers say is wrong, as the panel reads it. M8.8.5.
    problems: root::problems::Problems,
    /// What is waiting to go to a remote. M8.8.4.
    push: root::push::Push,
    /// Where each project's dependencies were unpacked. M8.8.3.
    libraries: root::library::Libraries,
    /// Where each area's tab strip is scrolled to — one handle per area
    /// since M12.8, made on the area's first draw and dropped with it
    /// (`keep_strip_scrolls`). `None` is the one card drawn when the seat
    /// in front has no areas yet, or there is no seat.
    ///
    /// A handle rather than an offset of ours: GPUI applies it during
    /// prepaint, which is the only moment a tab's width is known, and it
    /// is also what `scroll_to_item` needs to bring the tab in front
    /// into view when a stroke rather than a click chose it. **One per
    /// strip**: a handle shared by two strips records whichever painted
    /// last, and the chevron of the other would list the wrong tabs.
    tab_scroll: std::collections::HashMap<Option<guild_session::AreaId>, gpui::ScrollHandle>,
    /// The tab each strip was last scrolled to, so that following the
    /// one in front of it happens once per change rather than once per
    /// frame — per area since M12.8's second window check: a restart
    /// brings back every area's front, and each strip scrolls its own.
    tab_scrolled: std::collections::HashMap<Option<guild_session::AreaId>, Option<TabId>>,
    /// How many frames in a row a strip has been waited for — the bound
    /// on asking for one more (`root::render`).
    strip_waits: u8,
    /// Each strip's bounds as the last render read them — a strip is
    /// scrolled only once they hold still (`root::render`, O5).
    strip_bounds:
        std::collections::HashMap<Option<guild_session::AreaId>, gpui::Bounds<gpui::Pixels>>,
    /// Every project's search: what was asked, what has come back, and
    /// the thread still walking. Per project because the tool is, and
    /// because switching projects must not throw away an answer. M8.7.
    searches: root::search::Searches,
    /// Where the command button was drawn last frame, so the list can
    /// hang from it. A `Cell` written by a `canvas` inside the button,
    /// for the reason `branch_at` is one: prepaint runs inside the frame
    /// the button is drawn in, and updating the entity from there is the
    /// "cannot read while it is already being updated" panic.
    run_at: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    /// The file tree of every project that has shown one, with what is
    /// selected in it and where it is scrolled to. Read on first sight of
    /// the left panel rather than at startup: a tree is one directory read,
    /// but there is no reason to read it for a project nobody is looking
    /// at.
    file_operations: root::transfers::State,
    trees: std::collections::HashMap<Seat, root::tree::TreeState>,
    /// The changes of every project whose status has landed, in git's
    /// three groups, with what is selected among them.
    changes: std::collections::HashMap<ProjectId, root::changes::ChangesState>,
    /// The log of every project whose Log face has been shown: its pages,
    /// the commit opened, and the files of the commits opened so far.
    logs: std::collections::HashMap<ProjectId, root::log::LogState>,
    /// The code face, resolved once from what is installed: walking the
    /// font list is a CoreText enumeration and a sort, and doing it per
    /// frame in two views cost more than everything else a keystroke did.
    code_font: gpui::SharedString,
    /// The setting the resolved face came from, to notice a change.
    code_font_setting: Option<String>,
    /// Whether the face has been resolved against the installed list at
    /// all: before the first render there is no text system to ask.
    code_font_resolved: bool,
    /// A watcher per open project.
    watchers: root::watch::Watchers,
    /// Diff tabs restored before there was a window to wire their
    /// listeners with; the first render wires them.
    unheard_diffs: Vec<gpui::Entity<diff_tab::DiffTab>>,
    /// A git status thread per open project, and what each last said.
    status_workers: root::git::StatusWorkers,
    git_status: root::git::GitAnswers,
    /// The loop that asks the watchers is running.
    watching: bool,
    /// The tab the tree last followed, so that following happens once
    /// per change of tab rather than once per frame.
    tree_followed: Option<TabId>,
    /// Latest source choices and identity, refreshed with Git status.
    update_sources: std::collections::HashMap<
        guild_session::ProjectId,
        Result<crate::git::branch_update::Snapshot, crate::git::GitError>,
    >,
    updating_branches: std::collections::HashMap<
        guild_session::ProjectId,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    >,
    /// Auto fast-forward of each open repository's default branch.
    auto_ff: root::auto_ff::Schedule,
    /// Update branch clicks held while their repository's auto
    /// fast-forward ran.
    held_updates: std::collections::HashMap<guild_session::ProjectId, root::branches::Held>,
    footer_height: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    status_right_width: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    status_tools_width: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    /// Where the title bar's ↓ ended up, for the source popup hanging
    /// under it.
    update_branch_at: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    update_branch_focus: gpui::FocusHandle,
    update_focus_listeners: Vec<gpui::Subscription>,
    /// The local branches and the stash of every project the window has
    /// asked about. One list each, read by the git thread: the popup and
    /// the Log's branch column draw the same one, because two copies are
    /// two lists that will disagree.
    branch_lists: root::branches::Lists<crate::git::branches::Branch>,
    stash_lists: root::branches::Lists<crate::git::stashes::Stash>,
    /// Where the branch button was drawn last frame, in window
    /// coordinates, so the popup can hang from it.
    ///
    /// A `Cell` written by a `canvas` inside the button rather than a
    /// field written by an update: prepaint runs inside the frame the
    /// button is drawn in, and updating the entity from there is the
    /// "cannot read while it is already being updated" panic. The
    /// button is drawn every frame, so by the time a click opens the
    /// popup the place is already recorded.
    branch_at: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    /// What the right button was last pressed on, and so what the
    /// context menu being built is about. M8.3.2: the menu belongs to a
    /// surface rather than to a row, and the row records itself here as
    /// the button goes down — a frame before the component asks.
    menu_target: Option<crate::view::menu::Target>,
    /// The right button's menu, while it is up — Guild's own since M12.2.
    context_menu: Option<crate::view::menu::Open>,
    tab_launcher: crate::root::tab_launcher::Launcher,
    /// Where the tab strip's three menu buttons ended up, by
    /// `view::tab_list::StripTrigger`, for the menus that hang under them
    /// — M12.7. **By area since M12.8**: every strip draws the three, and
    /// one cell for many writers keeps the last (traps.md), so a menu
    /// opened from the left strip hung under the right one's button.
    strip_at: std::rc::Rc<std::cell::RefCell<StripPlaces>>,
    /// The area whose strip opened the strip menu that is up — its
    /// button is the one drawn lit. M12.8.
    strip_menu_area: Option<guild_session::AreaId>,
    /// Where the room the areas divide ended up — M12.8, decision 17. A
    /// `Cell` written by a `canvas` at prepaint, `branch_at`'s reason, and
    /// read at the next render: the areas' frames are cut from the room
    /// as it was measured, never from the window's size. `None` until the
    /// first frame has been laid out.
    work_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The border between two areas under the hand: its seat, its path
    /// from the root of the seat's tree (decision 12), and its split as it
    /// was taken hold of. `drag_from` is where the hand pressed, and the
    /// border is put where the hand's whole way from there says — a dock
    /// follows step by step, and a split's hundredth ate a slow hand's
    /// steps (M12.8's third window check).
    area_drag: Option<(
        Seat,
        Vec<guild_session::layout::Side>,
        guild_session::split::Split,
    )>,
    /// Where each area's body ended up — the room its front tab's panes
    /// divide, M12.8 decisions 17 and 19. Written by a `canvas` in each
    /// area as `work_at` is, and read at the next render; one cell per
    /// area, because one cell for many writers keeps the last (traps.md).
    body_at: std::rc::Rc<
        std::cell::RefCell<
            std::collections::HashMap<Option<guild_session::AreaId>, gpui::Bounds<gpui::Pixels>>,
        >,
    >,
    /// The border between two panes under the hand: its tab, its path
    /// from the root of the tab's tree (decision 12), and its split as it
    /// was taken hold of. Follows the pointer as `area_drag` does.
    pane_drag: Option<(
        TabId,
        Vec<guild_session::layout::Side>,
        guild_session::split::Split,
    )>,
    /// Where the counter ended up, for the menu that hangs above it —
    /// M12.2; drawn by the engine since M12.7 task 4.
    counter_at: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Where the Log's three filter buttons were last drawn, by
    /// `FilterMenu as usize`, for the menu that hangs under each — M12.7
    /// task 6. Recorded only while that button's menu is not up.
    log_filter_at: std::rc::Rc<std::cell::Cell<[Option<gpui::Bounds<gpui::Pixels>>; 3]>>,
    /// Where Find in Files' two chevrons were last drawn, by `Field as
    /// usize`, for the history menu that hangs under each — M12.7 task 7.
    /// Recorded only while that field's menu is not up.
    find_history_at: std::rc::Rc<std::cell::Cell<[Option<gpui::Bounds<gpui::Pixels>>; 2]>>,
    /// Whether the list of the project's language servers is up. M8.5.1.
    showing_servers: bool,
    /// Where the server's state ended up, for the list above it.
    server_state_at: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// A URL clone stays window-owned while its initiating workspace changes.
    clone_project: Option<crate::root::clone_project::CloneProject>,
    /// The popup a project's `+` opens, while it is up — M12.2.
    new_worktree: Option<crate::view::new_worktree::NewWorktree>,
    worktree_removal: Option<crate::view::worktree_removal::Confirmation>,
    worktree_tooltips: gpui::Entity<gpui_kit::base::TooltipOverlay>,
    /// The row whose action last asked `worktree_tooltips` to show.
    worktree_tooltip_owner: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    hovered_worktree: Option<(guild_session::WorkspaceId, std::path::PathBuf)>,
    /// What the column reads of project groups' folders from disk — their
    /// branches and canonical paths — on the title bar's clock. GLD-055.
    group_reads: crate::root::column::GroupReads,
    /// The tab row under the pointer in the column's list of a worktree's
    /// tabs, whose status gives its slot to a `×`. Kept here and notified,
    /// as `hovered_worktree` is: the column is a cached view, and a group
    /// hover alone does not redraw it.
    hovered_column_tab: Option<TabId>,
    /// The small create/rename field opened from the workspace switcher.
    workspace_field: Option<crate::view::workspaces::WorkspaceField>,
    workspace_icon_picker: Option<view::workspaces::IconTarget>,
    workspace_icon_at: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    /// The one row the pointer or the keyboard is on, while the icon
    /// picker is up — a grid, so `view::workspaces::grid_step` moves it
    /// rather than the engine's own `step`. Reset whenever the picker
    /// opens or closes. M12.7 task 8.
    workspace_icon_lit: Option<usize>,
    /// The border under the hand, if one is.
    dragging: Option<crate::view::panels::Side>,
    /// The border *inside* a tool under the hand, if one is. M8.3.2.
    ///
    /// Beside `dragging` rather than inside it: the two obey different
    /// bounds and write to different state, and one field holding either
    /// would need a match at every use.
    column_drag: Option<crate::view::panels::Column>,
    /// The window's size at the last render: what a panel that fills the
    /// width is as wide as.
    viewport: gpui::Size<gpui::Pixels>,
    /// The window's backing scale at the last render: what layout snaps to.
    scale_factor: f32,
    /// The tool being dragged from the status bar, while one is: the
    /// drop zones are drawn for it, and it is forgotten on any move with
    /// no button held — a release outside the window never arrives.
    tool_drag: Option<guild_session::Tool>,
    /// Where the pointer was when the border last moved.
    ///
    /// The border follows the pointer's *position* rather than a delta
    /// accumulated per event: an accumulator drifts from the hand the moment
    /// a frame is dropped, and then the border is somewhere the pointer is
    /// not.
    drag_from: Option<gpui::Point<gpui::Pixels>>,
    /// Which panel a resize stroke means — the last one opened.
    last_panel: crate::view::panels::Side,
    /// The palette in force.
    ///
    /// A copy rather than a lookup at each colour: `Theme` is `Copy`, and a
    /// render method binds it once so the closures inside it can capture
    /// it. Kept in step with `settings.theme` by `apply_theme`.
    theme: theme::Theme,
    /// The palette the file, diff and Annotate tabs last received — apart
    /// from `theme` because a slider's drag may leave their restyle for its
    /// release (`root::appearance`).
    tabs_theme: theme::Theme,
    /// Whether macOS draws its dark appearance — what a System mode follows.
    system_dark: bool,
    /// Hears macOS change its appearance.
    appearance: Option<gpui::Subscription>,
    /// The theme customizer, while its card is open.
    customizer: Option<root::customizer::Customizer>,
    /// *Customize theme*'s button: where the focus goes when the card closes.
    customize_button: gpui::FocusHandle,
    /// The window's own Settings page: a boundary, not a tab stop, inside
    /// which the focus is the page's.
    settings_page_focus: gpui::FocusHandle,
    /// The active project's branch, and when it was last read.
    ///
    /// Cached because it is read from disk and drawn every frame: a file
    /// read per frame is work nobody asked for, and a branch changes when
    /// a person changes it, not sixty times a second.
    branch: Option<String>,
    branch_read: Option<Instant>,
    /// The window's own focus.
    ///
    /// Without it, a window showing a panel instead of a tab has nothing
    /// focused at all — and GPUI dispatches a shortcut along the path from
    /// whatever holds focus. With nothing there the path is empty and
    /// **every** shortcut silently does nothing. That is what "the
    /// shortcuts do not work" was.
    focus: FocusHandle,
    /// One per dock, on the panel itself — M12.3: a press in a panel with
    /// nothing focusable of its own (Problems, Push, a head) makes that
    /// panel the active one, and a stroke that acts on the active panel
    /// finds it on the focus path.
    dock_focus: [FocusHandle; 3],
    /// A dock a person just opened, whose tool takes the focus at the next
    /// frames — a tool window's activation, M12.3 — and how many more frames
    /// to wait for the tool's own element to be drawn before settling for the
    /// panel.
    activating: Option<(guild_session::Dock, u8)>,
    /// The sidebar's own, for the same two reasons as a dock's: a stroke
    /// that stretches it finds it on the focus path, and bringing it back
    /// by hand puts the focus in it. M12.3.
    sidebar_focus: FocusHandle,
    sidebar_speed: Option<speed_search::Search<column::Listed>>,
    sidebar_speed_workspace: Option<WorkspaceId>,
    sidebar_speed_revealed: Option<(column::Listed, usize)>,
    /// Where a drag held over the column would land: the line or the lit
    /// row it draws. In memory only, and gone with the drag.
    pub(crate) column_drop_hint: Option<column::DropHint>,
    /// The start screen's own focus — M14.5. Its keys (`1`, `t`, `enter`)
    /// are bare, so they live on a handle of their own: bound on the root,
    /// they would fire whenever the focus fell back to it, a terminal that
    /// had just closed included.
    start_focus: FocusHandle,
    /// The start screen's rows, chips and highlight.
    start: crate::start::Start,
    /// The history read on a thread, and whose folder it is for.
    start_reading: root::start::StartReading,
    /// The Sessions mode's Settled history, read on a thread.
    settled: root::settled::SettledReading,
    /// What the Sessions mode's filter menu has unticked.
    session_filter: crate::session_list::Filter,
    /// The Sessions mode's selected row — the arrows' and the speed
    /// search's, as a list's selection is (decision 54).
    session_cursor: Option<crate::session_list::Key>,
    /// The Sessions mode's speed search, over its rows' names.
    session_speed: Option<speed_search::Search<crate::session_list::Key>>,
    /// The search's match last brought into view, and its place among the
    /// page's rows: a match that moves is shown again, a still one is not
    /// scrolled back to over the person's own scrolling.
    session_speed_shown: Option<(crate::session_list::Key, usize)>,
    /// The shown Sessions page's list, scrolled — so the arrows and the
    /// search can bring the chosen row into view.
    session_scroll: gpui::ScrollHandle,
    /// The shown Sessions page's open Settled, scrolled inside its capped
    /// height at the column's foot.
    settled_scroll: gpui::ScrollHandle,
    /// The start screen's rows, scrolled — so the arrows can bring the lit
    /// row into view.
    start_scroll: gpui::ScrollHandle,
    /// The tile under the pointer: `surface.raised` and its name in
    /// `text.primary`. Kept here rather than in `.hover`, because a text
    /// colour is fixed at layout, before hover is known
    /// (`docs/traps.md`: `group_hover` cannot recolour text).
    start_tile: Option<guild_session::DefaultTab>,
    /// The rows' age words as last drawn: `poll_start` repaints an idle
    /// screen when they would now read differently.
    start_ages: std::cell::RefCell<Vec<String>>,
    /// The harnesses on the login `PATH`, asked with the history: what the
    /// start screen greys and whose keys it leaves dead.
    available: Vec<guild_session::HarnessKind>,
    /// The sidebar was just brought back by hand and takes the focus at
    /// the next frame.
    activating_sidebar: bool,
    /// A tab a person just opened, or a menu a project switch just closed
    /// out from under the root's own handle, which takes the keys at the
    /// next frame whatever held them. M5.12's window check 12;
    /// `close_menu_for_switch`'s residual fix reused it rather than
    /// growing a second flag with the same shape.
    focusing_front: bool,
    /// This render moved the keys: the terminal bodies it draws are not
    /// replayed from the frame before — `view::body`.
    focus_moved_in_draw: bool,
    tab_shortcuts: root::tabs::TabShortcuts,
    /// What a closed pane's body was focused by — M12.8's final review,
    /// Important 1. The heir takes the keys at the next frame **only if
    /// one of these still holds them**: a pane closed by an agent (`guild
    /// session end`, the IDE's `close_tab`) while the person types in the
    /// Commit box, the tree or a popup leaves those keys where they are.
    /// Held for one frame, which keeps a dropped body's handle alive long
    /// enough to be asked.
    closed_focus: Vec<gpui::FocusHandle>,
}

/// Which tree a file tab's path is relative to. M8.8.3.
///
/// **A project, one of the roots its dependencies were unpacked
/// under, or the file system's own, for a file a terminal's link led to.** A path is stored relative to something so that the checkout
/// can move; for a file inside `~/.cargo/registry/src/…` the something
/// is not the project, and storing an absolute path instead would make
/// the two kinds of tab different in every place that handles one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Origin {
    /// The project the tab belongs to, which is nearly every file tab.
    #[default]
    Project,
    /// An external root, by its place in the project's own list —
    /// where `f12` into a dependency lands. M8.8.3.
    External(usize),
    /// A file anywhere else on the disk, opened by a `cmd`-click on its
    /// path in a terminal. Its root is the file system's, `/`, so the path
    /// is still relative to something; it is read-only, never written
    /// down, and it is not a root of the project — nothing indexes it and
    /// the Explorer has no row for it.
    Outside,
}

impl Origin {
    /// The root an [`Origin::Outside`] tab's path is relative to.
    pub(crate) const OUTSIDE_ROOT: &'static str = "/";
}

/// What identifies an open tab.
///
/// **Not a `SessionId`.** A tab is a view onto *something*, and until now
/// that something was always a session, so the session's identity stood in
/// for the tab's. Settings broke that — a page about Guild is not a
/// session — and from M8 files will break it far harder.
///
/// The two identities are genuinely different: a session outlives every
/// tab that ever showed it, and section 2.3 of the spec allows two tabs
/// onto one session.
///
/// Public and opaque since M12.7: a menu's deed names the tab it brings
/// forward, and `Deed` is public. Nothing outside can make one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(uuid::Uuid);

impl TabId {
    fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An open tab: what cannot be saved to a file.
struct Tab {
    id: TabId,
    /// The workspace-local place this tab belongs to.
    seat: Seat,
    /// The area of its seat whose strip it is on — M12.8. Set by
    /// `Root::adopt`, the one place a tab is made.
    area: guild_session::AreaId,
    /// Kept as a compatibility projection while root-owned subsystems
    /// continue to key themselves by project.
    project: ProjectId,
    /// What each of its panes shows — decision 6 of M12.8; one pane until
    /// it is split. A pane with no body is empty and shows the start
    /// screen (decision 13).
    panes: guild_session::Panes<Body>,
    /// When we last checked whether a turn happened.
    last_probe: Instant,
}

impl Tab {
    /// The focused pane's body — *what is in front* in this tab. `None`
    /// when that pane is empty.
    fn body(&self) -> Option<&Body> {
        self.panes.body()
    }

    /// What names the tab in the strip: the focused pane's body, else the
    /// first pane's, in draw order, that shows something. A tab always
    /// has one (invariant 3), so `None` only for a model that broke it.
    fn face(&self) -> Option<&Body> {
        self.body()
            .or_else(|| self.panes.bodies().first().map(|(_, body)| *body))
    }

    /// Every body the tab holds, in draw order — *does this tab hold X*.
    fn bodies(&self) -> impl Iterator<Item = &Body> {
        self.panes.bodies().into_iter().map(|(_, body)| body)
    }

    /// The session the focused pane watches, if it watches one —
    /// decision 6.
    fn session(&self) -> Option<SessionId> {
        self.body().and_then(Body::session)
    }

    /// Every session the tab's panes watch, in draw order.
    fn sessions(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.bodies().filter_map(Body::session)
    }
}

/// What a tab is showing.
///
/// Terminal and Broken carry session identity; other content has none.
/// Asking a pane for its session can therefore honestly answer `None`.
pub(crate) enum Body {
    /// The real Claude Code in a pseudo-terminal.
    Terminal {
        session: SessionId,
        view: gpui::Entity<terminal::view::TerminalTab>,
        /// What the window reads about the tab without reading `view` —
        /// `terminal::facts` says why.
        facts: std::rc::Rc<terminal::facts::Facts>,
    },
    /// The tab failed to open — the window shows what happened.
    Broken {
        session: SessionId,
        reason: String,
    },
    /// A file of the project, in the editor. `path` is relative to the
    /// project, so that the checkout can move.
    File {
        path: std::path::PathBuf,
        tab: gpui::Entity<file_tab::FileTab>,
        /// Presentation belongs to this pane even when its buffer is shared.
        presentation: Option<markdown::Presentation>,
        /// Which tree the path is relative to. M8.8.3.
        ///
        /// A file opened by `f12` inside a dependency's sources lives
        /// under a root that is not the project's, and `path` alone
        /// cannot say which. It is on `Body` rather than on `FileTab`
        /// because every question the window asks about a tab — which
        /// project, which diff, which document to tell a server about —
        /// is asked here, and the answer for an external tab is *none of
        /// them*.
        origin: Origin,
    },
    /// A diff against the last commit: one file's when `path` is some,
    /// the working tree's when it is none. Decision 3 of M8.2.
    Diff {
        path: Option<std::path::PathBuf>,
        /// What the two texts are. Decision 5 of M8.3.
        base: guild_session::Base,
        tab: gpui::Entity<diff_tab::DiffTab>,
    },
    /// A file annotated: who wrote each line and when. Decision 10 of
    /// M8.3.
    Annotate {
        path: std::path::PathBuf,
        tab: gpui::Entity<annotate_tab::AnnotateTab>,
    },
    /// Guild's own settings.
    ///
    /// The section lives here rather than on `Root` so that it is the
    /// tab's, like everything else a tab remembers: two settings tabs can
    /// sit on different sections, and opening settings can aim at one.
    Settings {
        section: Section,
    },
    Process {
        run: guild_session::workflow::RunId,
    },
}

impl Root {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = Store::<Settings>::new(paths::settings_file())
            .load()
            .unwrap_or_default();
        let mut root = Self::from_state(settings, load_desk(), cx);
        root.start_ide();
        root.start_hooks();
        root.restore_tabs(cx);
        root
    }

    // State construction is separate from process/socket startup so panel behavior
    // can be exercised with an in-memory desk and disposable projects.
    fn from_state(settings: Settings, desk: Desk, cx: &mut Context<Self>) -> Self {
        Self::from_state_with_policy(settings, desk, root::isolation::Policy::Production, cx)
    }

    fn from_state_with_policy(
        settings: Settings,
        mut desk: Desk,
        integrations: root::isolation::Policy,
        cx: &mut Context<Self>,
    ) -> Self {
        network::publish(&settings.network);
        // First-run windows also need a real workspace to own an asynchronous clone.
        // Persisted desks already pass through this normalization.
        if desk.workspaces().is_empty() {
            desk.normalise();
        }
        // Read before `settings` is moved into the struct below.
        let language = settings.language;
        // The system is read before Guild's mode is handed to AppKit:
        // nothing is forced yet, so this is the system's own answer.
        let system_dark = theme::system_dark(cx.window_appearance());
        cx.set_window_appearance(theme::forced_appearance(settings.theme_mode()));
        let palette = theme::in_effect(&settings, system_dark);
        let start_size = terminal::view::StartSize::seeded(settings.tab_grid);

        // Records there is no returning to would pile up with every
        // launch. Before restoring rather than after: there is nothing to
        // restore in what is about to be forgotten.
        desk.forget_unresumable();

        // First run: the directory the application was started from is a
        // reasonable guess at what the person is working on. But only if
        // it looks like one: for a bundled application that is the root,
        // and a project named `/` is not how you greet someone.
        if integrations.external()
            && desk.projects().next().is_none()
            && let Some(cwd) = paths::plausible_first_project()
        {
            let workspace = desk.active_workspace();
            desk.open_in(workspace, &cwd);
        }

        // Where you were, unless you asked not to be put back there. Off,
        // the window still opens on a project — the first in the row — so
        // the setting chooses *which*, never *whether*: an empty window is
        // nobody's idea of a fresh start.
        let active_workspace = desk.active_workspace();
        let active_project = settings
            .reopen_last_project
            .then(|| desk.active_project())
            .flatten()
            .filter(|id| desk.holders(*id).contains(&active_workspace))
            .or_else(|| {
                desk.projects_of(active_workspace)
                    .next()
                    .map(|project| project.id)
            });

        let mut commands = std::collections::HashMap::new();
        let mut in_front = None;
        for project in desk.projects() {
            let state = guild_session::project_state::load(&project.path);
            if Some(project.id) == active_project {
                in_front = Some(state.panels.clone());
            }
            commands.insert(project.id, state.commands);
        }
        // **The panels are the window's since M12.2**, and a layout that
        // lived in a project's file is not thrown away: the project in
        // front gives it, once, and it is written down at once.
        let mut settings = settings;
        if integrations.external()
            && settings.adopt_panels(in_front)
            && let Err(err) =
                guild_session::Store::new(crate::paths::settings_file()).save(&settings)
        {
            tracing::warn!(%err, "the adopted panels were not saved");
        }

        let state_dir = integrations.state_dir();
        use gpui::AppContext as _;
        let diagrams =
            cx.new(|cx| diagrams::runtime::Manager::new(diagrams::Resources::discover(), cx));
        let diagram_watch = cx.observe(&diagrams, |_, _, cx| cx.notify());
        let mut root = Self {
            diagrams,
            _diagram_watch: diagram_watch,
            integrations,
            #[cfg(test)]
            isolated_render: false,
            #[cfg(test)]
            titlebar_double_clicks: 0,
            #[cfg(test)]
            titlebar_drag_starts: 0,

            settings,
            desk,
            active_workspace,
            workspace_swipe: swipe::Detector::default(),
            workspace_swipe_epoch: std::time::Instant::now(),
            workspace_swipe_offset: 0.0,
            workspace_swipe_landing: 0,
            restored_workspaces: std::collections::HashSet::new(),
            active_project,
            tabs: Vec::new(),
            active_tab: None,
            board: Default::default(),
            latch: Default::default(),
            unbound_starts: Default::default(),
            done_heard: Default::default(),
            running: Default::default(),
            active_groups: Default::default(),
            sound_player: Default::default(),
            hooks: None,
            dock_badge: 0,
            worktrees: std::collections::HashMap::new(),
            worktrees_asked: None,
            worktree_reads: Default::default(),
            hunk_flights: Default::default(),
            worktrees_stale: std::collections::HashSet::new(),
            column_peeking: false,
            worktree_dragging: false,
            project_drag: Default::default(),
            column_spacers: Default::default(),
            tab_drag: Default::default(),
            action_bar_dragging: false,
            column_edge_drag: false,
            pending_writes: Vec::new(),
            pending_settle: Vec::new(),
            pending_tasks: Vec::new(),
            started_as: std::collections::HashMap::new(),
            told: crate::binding::Told::default(),
            handing_over: std::collections::HashSet::new(),
            renaming_tab: None,
            window_focused: true,
            fullscreen: false,
            closed: ClosedTabs::default(),
            stage: stage::Stage::default(),
            hovered_tab: None,
            tab_landing: None,
            satellites: windows::Registry::default(),
            satellites_opening: std::collections::HashSet::new(),
            push_dialog: Default::default(),
            diff_window: Default::default(),
            main_window: None,
            start_size,
            picker: None,
            settings_group_positions: Default::default(),
            network_check: Default::default(),
            settings_query: String::new(),
            settings_modified: false,
            settings_width: Default::default(),
            settings_search_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            settings_scroll: gpui::ScrollHandle::new(),
            action_bar_scroll: Default::default(),
            settings_scroll_host: None,
            settings_page: None,
            recording: None,
            conflict: None,
            stroke_refused: false,
            consoles: std::collections::HashMap::new(),
            console_scroll: gpui::ScrollHandle::new(),
            console_view: None,
            main_display: None,
            dock_head_at: Default::default(),
            servers: std::collections::HashMap::new(),
            scoped_settings: Default::default(),
            #[cfg(test)]
            test_server: None,
            #[cfg(test)]
            test_launch_fallback: None,
            serverless: std::collections::HashMap::new(),
            xcode_setups: std::collections::HashMap::new(),
            server_deaths: std::collections::HashMap::new(),
            documents: std::collections::HashMap::new(),
            document_work: Default::default(),
            diagnostics: std::collections::HashMap::new(),
            landing: None,
            runs: std::collections::HashMap::new(),
            run_tab_scroll: gpui::ScrollHandle::new(),
            run_tab_frame: Default::default(),
            panel_terminal_scroll: gpui::ScrollHandle::new(),
            panel_terminal_width: Default::default(),
            panel_terminal_front: None,
            panel_terminals: std::collections::HashMap::new(),
            update: root::update::Update::here(),
            update_settings: Default::default(),
            usage: Default::default(),
            forge: Default::default(),
            showing_usage: false,
            usage_at: Default::default(),
            usage_last: std::cell::Cell::new(agents::Agent::ClaudeCode),
            usage_all_open: true,
            open_in_at: Default::default(),
            open_in_apps: None,
            #[cfg(test)]
            open_in_launches: None,
            #[cfg(test)]
            open_in_availability: None,
            open_in_icons: Default::default(),
            open_in_inventory: None,
            open_in_inventory_task: None,
            open_in_picker: None,
            open_in_new_custom: None,
            declared: std::collections::HashMap::new(),
            ide: None,
            parked: std::collections::HashMap::new(),
            font_at: Default::default(),
            history: root::history::History::default(),
            recent_files: root::recent_files::RecentFiles::default(),
            problems: root::problems::Problems::default(),
            push: root::push::Push::default(),
            libraries: root::library::Libraries::default(),
            tab_scroll: std::collections::HashMap::new(),
            tab_scrolled: std::collections::HashMap::new(),
            strip_waits: 0,
            strip_bounds: std::collections::HashMap::new(),
            searches: std::collections::HashMap::new(),
            run_at: Default::default(),
            commands,
            file_operations: Default::default(),
            trees: std::collections::HashMap::new(),
            changes: std::collections::HashMap::new(),
            logs: std::collections::HashMap::new(),
            code_font: terminal::font::choose_code(&[], None).into(),
            code_font_setting: None,
            code_font_resolved: false,
            watchers: std::collections::HashMap::new(),
            unheard_diffs: Vec::new(),
            status_workers: std::collections::HashMap::new(),
            git_status: std::collections::HashMap::new(),
            watching: false,
            tree_followed: None,
            update_sources: Default::default(),
            updating_branches: Default::default(),
            auto_ff: Default::default(),
            held_updates: Default::default(),
            update_branch_at: Default::default(),
            footer_height: Default::default(),
            status_right_width: Default::default(),
            status_tools_width: Default::default(),
            update_branch_focus: cx.focus_handle(),
            update_focus_listeners: Vec::new(),
            branch_lists: Default::default(),
            stash_lists: Default::default(),
            branch_at: Default::default(),
            menu_target: None,
            context_menu: None,
            tab_launcher: Default::default(),
            strip_at: Default::default(),
            strip_menu_area: None,
            work_at: Default::default(),
            area_drag: None,
            body_at: Default::default(),
            pane_drag: None,
            clone_project: None,
            new_worktree: None,
            worktree_removal: None,
            worktree_tooltips: gpui::AppContext::new(cx, |_| gpui_kit::base::TooltipOverlay::new()),
            worktree_tooltip_owner: Default::default(),
            hovered_worktree: None,
            group_reads: Default::default(),
            hovered_column_tab: None,
            workspace_field: None,
            workspace_icon_picker: None,
            workspace_icon_at: std::rc::Rc::new(std::cell::Cell::new(gpui::point(
                gpui::px(12.),
                gpui::px(42.),
            ))),
            workspace_icon_lit: None,
            counter_at: Default::default(),
            log_filter_at: Default::default(),
            find_history_at: Default::default(),
            showing_servers: false,
            server_state_at: Default::default(),
            dragging: None,
            tool_drag: None,
            viewport: gpui::Size::default(),
            scale_factor: 1.,
            column_drag: None,
            drag_from: None,
            last_panel: crate::view::panels::Side::Left,
            theme: palette,
            tabs_theme: palette,
            system_dark,
            appearance: None,
            customizer: None,
            customize_button: cx.focus_handle(),
            settings_page_focus: cx.focus_handle().tab_stop(false),
            asking: false,
            focus_lost: None,
            quitting: None,
            activation: None,
            heard: Default::default(),
            regions: Default::default(),
            ears: None,
            double_shift: Default::default(),
            file_indexes: Default::default(),
            symbols: Default::default(),
            overlays: Default::default(),
            terminal_jobs: terminal_job::LiveJobs::load(state_dir.join("terminal-jobs.json")),
            review_activity: review_activity::Activity::new(
                state_dir.join("review-source-pins.json"),
            ),
            continuation_receipts: guild_session::resume_handoff::Ledger::load(
                state_dir.join("session-continuations.json"),
            ),
            continuation_starts: Default::default(),
            workflows: None,
            processes: Default::default(),
            terminal_job_error: None,
            terminal_focus: None,
            terminal_list: None,
            terminal_geometry: Default::default(),
            overlay_focus: Default::default(),
            previews: Default::default(),
            find_previews: Default::default(),
            finds: Default::default(),
            balloon: None,
            declarations: Default::default(),
            dependencies: Default::default(),
            commit_message_wanted: false,
            notice_pointer_exited: false,
            notice_views: Default::default(),
            notices: {
                // Asked once, at startup, because that is when the file is
                // read. A person whose keymap will not parse is working
                // with shortcuts they did not choose, and until now the
                // only place that said so was the log.
                let mut notices = notify::Notices::default();
                if actions::keymap::unreadable() {
                    notices.push(
                        notify::Level::Warning,
                        Ui(language).text(Text::NoticeKeymapNotRead).to_owned(),
                        std::time::Instant::now(),
                    );
                }
                notices
            },
            branch: None,
            branch_read: None,
            focus: cx.focus_handle(),
            dock_focus: [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()],
            activating: None,
            sidebar_focus: cx.focus_handle(),
            sidebar_speed: None,
            sidebar_speed_workspace: None,
            sidebar_speed_revealed: None,
            column_drop_hint: None,
            start_focus: cx.focus_handle(),
            start: Default::default(),
            start_reading: Default::default(),
            settled: Default::default(),
            session_filter: Default::default(),
            session_cursor: None,
            session_speed: None,
            session_speed_shown: None,
            session_scroll: gpui::ScrollHandle::new(),
            settled_scroll: gpui::ScrollHandle::new(),
            start_scroll: gpui::ScrollHandle::new(),
            start_tile: None,
            start_ages: std::cell::RefCell::new(Vec::new()),
            // Every harness until the first read says otherwise: a tile
            // greyed for the tens of milliseconds a read takes would be
            // seen more often than one that is really missing.
            available: guild_session::HarnessKind::ALL.to_vec(),
            activating_sidebar: false,
            focusing_front: false,
            focus_moved_in_draw: false,
            tab_shortcuts: Default::default(),
            closed_focus: Vec::new(),
        };

        // Quit must own cleanup even before a Root has been rendered.
        root.quitting = Some(cx.on_app_quit(|root, cx| {
            root.stop_every_process(cx);
            // The merge requests' refreshes are aborted — their glab
            // children asked to end with SIGTERM — and their threads waited
            // for off the window's thread. GPUI gives a quit 200 ms and no
            // way to ask for more, so whatever glab is still alive after
            // 150 ms is killed then, never left behind (GLD-063, code review
            // r2, bugs+impl-1).
            let forge_threads = root.forge_shutdown();
            let forge = cx.background_executor().spawn(async move {
                let until = std::time::Instant::now() + std::time::Duration::from_millis(150);
                while std::time::Instant::now() < until
                    && !forge_threads
                        .iter()
                        .all(std::thread::JoinHandle::is_finished)
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                guild_session::merge_requests::glab::kill_every_child();
            });
            let diagram_worker = root.diagrams.update(cx, |manager, cx| manager.shutdown(cx));
            let transports = root.terminal_jobs.finish_shutdown(cx);
            crate::system::set_dock_badge(0);
            let update = root.finish_update(cx);
            let preferences: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> =
                if root.integrations.external() {
                    root.finish_preferences(cx)
                } else {
                    Box::pin(async {})
                };
            Box::pin(async move {
                forge.await;
                diagram_worker.await;
                transports.await;
                update.await;
                preferences.await;
            })
        }));

        // **The first of the double Shift's three ears** — M8.9, decision
        // 8: every key down, counted before any binding is matched. It only
        // counts; nothing here may update `Root` from inside a dispatch.
        let heard = root.heard.clone();
        root.ears = Some(cx.intercept_keystrokes(move |event, _window, _cx| {
            // GPUI 0.3.8 synthesizes an isolated modifier keystroke on release.
            // Counting Shift's own release spoils the tap before modifiers_changed.
            if !matches!(
                event.keystroke.key.as_str(),
                "shift" | "control" | "alt" | "platform" | "function"
            ) {
                heard.set(heard.get().wrapping_add(1));
            }
        }));
        root
    }

    /// Say in the corner of the window that something happened a person
    /// should know — it leaves by itself. [`notify::Level::Info`].
    ///
    /// The log keeps its entry for the two that wait: a notice is read
    /// once and dismissed, while the log is what gets sent when someone is
    /// asked what happened. No `cx`: the repaint comes from whatever the
    /// person did, and the fading is the watch loop's.
    fn inform(&mut self, text: String) {
        self.notices
            .push(notify::Level::Info, text, std::time::Instant::now());
    }

    /// Refused, or a default standing in; nothing was lost. Waits.
    fn warn(&mut self, text: String) {
        self.notices
            .push(notify::Level::Warning, text, std::time::Instant::now());
    }

    /// Something was not written, or could not run. Waits.
    fn fail(&mut self, text: String) {
        self.notices
            .push(notify::Level::Error, text, std::time::Instant::now());
    }

    /// The session in the active tab, if that tab shows one.
    fn active_session(&self) -> Option<SessionId> {
        let active = self.active_tab?;
        self.tabs
            .iter()
            .find(|tab| tab.id == active)
            .and_then(Tab::session)
    }

    fn save(&mut self) {
        // Nearly every change of the tab in front ends here, whichever
        // path made it — a click, a jump, a reopen into another project —
        // so this is where a seat's front is written down.
        self.note_front();
        // The session being looked at, not the tab: the workspace file
        // knows about sessions. Which tab shows it is this run's business,
        // and a settings tab has no session to record at all.
        self.desk
            .look_at(self.active_project, self.active_session());
        self.write();
        // The tabs are this run's business, and each project's own file
        // is where they go — with the one in front, so that a restart
        // shows the same tab.
        self.store_tabs();
    }

    /// Write the state to disk.
    ///
    /// On every change rather than on exit: an application that saves
    /// state only on a clean shutdown loses it at the first crash — and
    /// loses it silently.
    fn write(&mut self) {
        // Every change of the desk ends here — a workspace switched, a
        // project opened, closed or moved — so this is where a repository
        // that left the column has its merge request refresh revoked, at
        // once rather than at the next poll (GLD-063, code review r3,
        // lenses-6).
        self.forge_reconcile();
        if let Err(err) = guild_session::persist::save(
            &self.desk,
            &self.integrations.state_dir().join("workspace.json"),
        ) {
            tracing::warn!(%err, "state was not saved");
            // Refused because this run could not read the file: named once
            // when it was read, and every change would say it again.
            if matches!(err, guild_session::StoreError::NotRead { .. }) {
                return;
            }
            let text = self.ui().text(Text::NoticeStateNotSaved).to_owned();
            self.fail(text);
        }
    }

    /// Is an agent in this project actually working or waiting?
    ///
    /// **One rule for *live***, decision 2 of M5.12: the tab close, Close
    /// Project and Remove Project all read [`Root::stake_of`], which is what
    /// `confirm_closing_live_project`'s own doc always said — working or
    /// waiting, not merely open. It used to count any session tab, so an
    /// idle shell asked as loudly as an agent mid-turn, and a question that
    /// is asked about nothing is one people learn to dismiss unread.
    pub(crate) fn has_live_session(&self, project: ProjectId, cx: &gpui::App) -> bool {
        self.tabs
            .iter()
            .any(|tab| tab.project == project && self.stake_of(tab, cx).is_some())
    }

    /// What closing this tab would interrupt — M5.12, decision 1.
    ///
    /// A terminal's agent by the board, only while its process lives.
    /// Anything else has no turn.
    ///
    /// A tab of several panes is at stake when any of them is — M12.8:
    /// closing it ends every one — and its stake is the **strongest** of
    /// its panes' (`closing::strongest`), not the first one's: a shell
    /// beside an agent mid-turn is a tab mid-turn.
    pub(crate) fn stake_of(&self, tab: &Tab, cx: &gpui::App) -> Option<closing::Stake> {
        closing::strongest(tab.bodies().filter_map(|body| self.body_stake(body, cx)))
    }

    /// [`Root::stake_of`] for one pane's body.
    fn body_stake(&self, body: &Body, _cx: &gpui::App) -> Option<closing::Stake> {
        match body {
            Body::Terminal { session, facts, .. } => {
                closing::terminal_stake(self.status_of(*session), !facts.exited())
            }
            _ => None,
        }
    }

    /// What closing this tab asks about — [`Root::stake_of`] while
    /// `confirm_closing_live_project` is on, and, while
    /// `confirm_closing_agent_tab` is, an open agent's tab whatever its
    /// agent is doing (2026-09-16). Only a tab's close reads this: closing
    /// a project still asks about turns alone. A divided tab's is its
    /// strongest pane's: closing the tab closes every pane.
    pub(crate) fn close_stake_of(&self, tab: &Tab, cx: &gpui::App) -> Option<closing::Stake> {
        closing::strongest(
            tab.bodies()
                .filter_map(|body| self.body_close_stake(body, cx)),
        )
    }

    /// [`Root::close_stake_of`] for one pane's body — what closing that
    /// pane alone asks about (M12.8).
    fn body_close_stake(&self, body: &Body, cx: &gpui::App) -> Option<closing::Stake> {
        let live = if self.settings.confirm_closing_live_project {
            self.body_stake(body, cx)
        } else {
            None
        };
        let agent_open = match body {
            Body::Terminal { session, facts, .. } => closing::agent_open(
                self.desk.session(*session).map(|record| record.runs),
                facts.harness_in_front(),
                facts.exited(),
            ),
            _ => false,
        };
        closing::tab_stake(live, agent_open, self.settings.confirm_closing_agent_tab)
    }

    /// What closing `target` asks about: a tab's stake, or one pane's.
    pub(crate) fn target_stake(
        &self,
        target: root::files::CloseTarget,
        cx: &gpui::App,
    ) -> Option<closing::Stake> {
        let tab = self.tabs.iter().find(|tab| tab.id == target.tab())?;
        match target {
            root::files::CloseTarget::Tab(_) => self.close_stake_of(tab, cx),
            root::files::CloseTarget::Pane(_, pane) => {
                self.body_close_stake(tab.panes.body_of(pane)?, cx)
            }
        }
    }

    /// The tabs and panes among these whose agents are at work, each with
    /// the name the strip gives it — the close sheet's list. In the
    /// strip's order. A pane is named by its own session, and by its
    /// tab's place when it has no name of its own; a divided tab by the
    /// pane whose stake it is (`closing::at_stake`).
    pub(crate) fn staked_targets(
        &self,
        targets: &[root::files::CloseTarget],
        cx: &gpui::App,
    ) -> Vec<(String, closing::Stake)> {
        self.tabs
            .iter()
            .flat_map(|tab| {
                targets
                    .iter()
                    .filter(move |target| target.tab() == tab.id)
                    .map(move |target| (tab, *target))
            })
            .filter_map(|(tab, target)| {
                let stake = self.target_stake(target, cx)?;
                // **Named after the pane whose stake it is** — M12.8's
                // second window check, 11.6b: a divided tab asked about
                // its agent's turn was named after the shell beside it
                // that had the keys. The focused pane is listed first,
                // so it is the one named among equals.
                let face = match target {
                    root::files::CloseTarget::Tab(_) => {
                        let focused = tab.panes.focused();
                        let bodies = tab.panes.bodies();
                        let listed = bodies
                            .iter()
                            .filter(|(pane, _)| *pane == focused)
                            .chain(bodies.iter().filter(|(pane, _)| *pane != focused));
                        closing::at_stake(
                            listed.map(|(_, body)| (*body, self.body_close_stake(body, cx))),
                        )
                        .map(|(body, _)| body)
                        .or_else(|| tab.face())
                    }
                    root::files::CloseTarget::Pane(_, pane) => tab.panes.body_of(pane),
                };
                let name = face
                    .and_then(|body| self.session_body_name(body))
                    .unwrap_or_else(|| {
                        // *Session 2* where it has no name of its own, by
                        // its place in its own strip — the strip's and the
                        // agents list's fallback, so the sheet says the
                        // same. A shell with `claude` in front is a
                        // *Session* in the strip, and so here — M12.3's
                        // foreground process.
                        let claude = face
                            .and_then(Body::session)
                            .and_then(|session| self.desk.session(session))
                            .is_none_or(|record| record.runs.is_claude())
                            || matches!(face, Some(Body::Terminal { facts, .. })
                                if facts.claude_in_front());
                        self.ui().tab_label(claude, self.tab_number(tab))
                    });
                Some((name, stake))
            })
            .collect()
    }

    /// Clear the active terminal.
    fn clear_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.active_tab else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        if let Some(Body::Terminal { view, .. }) = tab.body() {
            view.update(cx, |tab, cx| tab.clear(cx));
        }
    }

    /// Remember where the window stands.
    ///
    /// Written on paint rather than on close: closing is sometimes a
    /// crash, and then the place would be lost exactly when restoring it
    /// matters most.
    fn remember_window(&mut self, window: &Window) {
        // In full screen there is nothing to remember: the window cannot
        // be put back into it anyway, and the size there is not its
        // own.
        if window.is_fullscreen() {
            return;
        }

        let gpui::WindowBounds::Windowed(bounds) = window.window_bounds() else {
            return;
        };

        let place = guild_session::WindowPlace {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };

        // The tabs' last fit goes with the window's place, in the same
        // write: a resize changes both, and a synced write is twenty
        // milliseconds.
        let grid = self.start_size.grid().or(self.settings.tab_grid);
        if self.settings.window == Some(place) && self.settings.tab_grid == grid {
            return;
        }
        self.settings.window = Some(place);
        self.settings.tab_grid = grid;
        // Queued, not waited for: a resize or a dock dragged changes the
        // grid on every frame, and a synced write is twenty milliseconds
        // (the trap). A failure still reaches the window through the
        // queue's own report.
        self.store_settings();
    }

    /// Labels in the chosen language.
    pub fn ui(&self) -> Ui {
        Ui(self.settings.language)
    }

    /// The strip in front: the seat in front's tabs in its focused area.
    fn visible_tabs(&self) -> impl Iterator<Item = &Tab> {
        let active = self.active_project.map(|project| Seat {
            workspace: self.active_workspace,
            project,
        });
        let area = self.focused_area();
        self.tabs
            .iter()
            .filter(move |tab| Some(tab.seat) == active && Some(tab.area) == area)
    }

    /// Write down the tab in front as its own area's front, and that area
    /// as its seat's focused one — the tab's seat, not the active one,
    /// which may already have moved on.
    fn note_front(&mut self) {
        if let Some((seat, area, id)) = self
            .active_tab
            .and_then(|active| self.tabs.iter().find(|tab| tab.id == active))
            .map(|tab| (tab.seat, tab.area, tab.id))
        {
            self.stage.bring_forward(seat, area, id);
        }
    }

    /// Bring forward the tab the person left in the seat now in front, or
    /// its last when there is none to remember — after anything that
    /// changes the project or the workspace in front. It was the strip's
    /// last on every one of those paths, so the second of four came back
    /// as the fourth. `fronts`, 2026-09-26.
    fn come_back(&mut self) {
        // What was in front a moment ago is its seat's, even if no save
        // came between it and this.
        self.note_front();
        // The focused area of the seat now in front, and the tab left in
        // front of it — every way into a project comes through here
        // (traps.md, 2026-09-26), and since M12.8 it is the area's front.
        let strip: Vec<TabId> = self.visible_tabs().map(|tab| tab.id).collect();
        self.active_tab = self
            .focused_area()
            .and_then(|area| self.stage.front(area, &strip));
    }

    fn active_body(&self) -> Option<&Body> {
        let active = self.active_tab?;
        self.tabs
            .iter()
            .find(|tab| tab.id == active)
            .and_then(Tab::body)
    }
}

/// Where each strip's three menu buttons ended up, by the strip's area
/// and `view::tab_list::StripTrigger` — `Root::strip_at`.
type StripPlaces = std::collections::HashMap<
    Option<guild_session::AreaId>,
    [Option<gpui::Bounds<gpui::Pixels>>; 3],
>;

/// What a tab is, as far as the strip is concerned.
///
/// A separate little type rather than reaching into `Body`: the strip needs
/// the kind and nothing else, and a view entity has no business in a tuple
/// that is collected before rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Session(Runs),
    Settings,
    Process,
    /// A file of the project, marked by its icon rather than a glyph.
    File,
    /// A diff, marked by a plus-minus sign.
    Diff,
    /// A file annotated, marked by a pilcrow.
    Annotate,
}
impl Root {
    /// Re-read the active project's branch, now and then.
    ///
    /// From disk, so not every frame: a file read sixty times a second for
    /// something that changes when a person changes it is work nobody
    /// asked for. The same two seconds the transcript probe uses — a
    /// branch switched in a terminal shows up about as fast as one would
    /// look up at the title bar.
    fn refresh_branch(&mut self) {
        if !self.integrations.external() {
            return;
        }
        let due = self
            .branch_read
            .is_none_or(|read| read.elapsed() >= BRANCH_PROBE);
        if !due {
            return;
        }

        self.branch_read = Some(Instant::now());
        self.branch = self
            .active_project
            .and_then(|id| self.desk.project(id))
            .and_then(|project| git::branch(&project.path));
    }
}

/// Read the state, migrating the old one if needed.
///
/// The format changed: it used to be a flat list of sessions, now it is
/// projects with sessions inside them. The old file is read until the new
/// one appears — otherwise a person's sessions would vanish on upgrade,
/// and vanish silently.
pub fn load_desk() -> Desk {
    let current = paths::workspace_file();
    if current.exists() {
        // The global file plus every project's own. A file still in the
        // old format carries its sessions here, and they move to their
        // projects on the next save.
        return guild_session::persist::load(&current);
    }

    let legacy = paths::legacy_sessions_file();
    if legacy.exists() {
        tracing::info!(
            path = %legacy.display(),
            "migrating state from the previous format"
        );
        return Store::new(legacy).load().unwrap_or_default();
    }

    Desk::default()
}

/// A path for display: the home directory collapses to a tilde.
///
/// The row is narrow, and a full path to a project almost always starts
/// the same way — that part is not worth the space.
pub(crate) fn short_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text.into_owned(),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_home_directory_is_shortened_to_a_tilde() {
        let home = std::env::var("HOME").expect("HOME is set");
        let path = PathBuf::from(&home).join("work").join("guild");

        assert_eq!(short_path(&path), "~/work/guild");
    }

    #[test]
    fn a_path_outside_home_is_shown_in_full() {
        assert_eq!(short_path(std::path::Path::new("/opt/guild")), "/opt/guild");
    }

    /// The point of the whole change: a tab is a view onto something, and
    /// that something need not be a session. Settings is the first case;
    /// files and diffs are the ones it was done for.
    #[test]
    fn a_tab_need_not_be_a_session() {
        let settings = Body::Settings {
            section: Section::General,
        };

        assert_eq!(settings.session(), None, "a page about Guild has none");
    }

    /// A tab that does watch a session says which, so nothing has to look
    /// it up by assuming the tab's identity is the session's.
    #[test]
    fn a_session_tab_names_its_session() {
        let session = SessionId::new();
        let body = Body::Broken {
            session,
            reason: "claude is not on the PATH".to_owned(),
        };

        assert_eq!(body.session(), Some(session));
    }

    /// Section 2.3 of the spec: a tab is a view onto a session, and two
    /// views onto one session are allowed. That was not representable
    /// while the tab's identity *was* the session's.
    #[test]
    fn two_tabs_can_watch_one_session() {
        let session = SessionId::new();
        let one = TabId::new();
        let other = TabId::new();

        assert_ne!(one, other, "two tabs, two identities");
        assert_eq!(
            Body::Broken {
                session,
                reason: String::new()
            }
            .session(),
            Body::Broken {
                session,
                reason: String::new()
            }
            .session(),
            "and one session behind both"
        );
    }
}

pub mod observer;

pub mod diagrams;
/// Markdown preview resources.
pub mod markdown;

pub mod speed_search;

#[cfg(test)]
#[path = "../../../test-support/lsp_fixture.rs"]
mod lsp_fixture;

mod repository_settings;

pub mod document_search;

pub mod local_search;

pub mod project_search;

mod transport;

mod source_occurrences;

pub mod source_snippet;
