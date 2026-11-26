//! Session state, shared launch vocabulary and provider event parsing.
//!
//! No GPUI: session management and provider events can be checked without
//! an open window. Interactive agent
//! processes run in the desktop's terminal owner.
//!
//! Guild assigns Claude conversation ids before launch, preserving exact
//! identity across terminal restoration and CLI provenance.

#![warn(missing_docs)]

pub mod action_bar;
pub mod areas;
pub mod changelog;
pub mod codex_transcript;
pub mod command;
pub mod desk;
pub mod environment;
pub mod event;
pub mod harness;
pub mod history;
pub mod layout;
pub mod login_path;
pub mod markdown;
pub mod merge_requests;
pub mod network;
pub mod open_in;
pub mod panes;
pub mod persist;
pub mod picker;
pub mod process;
pub mod project_state;
pub mod session;
pub mod session_id;
pub mod settings;
pub mod split;
pub mod status;
pub mod store;
pub mod transcript;
pub mod workspace;
pub mod worktree_order;

pub use crate::areas::{AreaId, AreaState, Areas, LastArea, SavedAreas};
pub use crate::desk::{Desk, Project, ProjectId, Runs, SessionRecord};
pub use crate::environment::{is_parent_session_marker, markers_to_drop};
pub use crate::event::{InitInfo, ResultInfo, SessionError, SessionEvent, parse_line};
pub use crate::harness::{Harness, HarnessKind};
pub use crate::panes::{PaneId, Panes};
pub use crate::picker::{Grab, PickerPlace, Spot};
pub use crate::project_state::{
    Base, Columns, Commands, Dock, Docks, Face, GitFace, OpenTab, Panel, Panels, ProjectState,
    Search, SeatState, Shows, Stretch, Tool,
};
pub use crate::session::{
    Continuation, Conversation, HOOK_SOCKET_VAR, Hooks, Resumption, SESSION_VAR, SOCKET_VAR,
    USER_STATUSLINE_VAR, continuation, resumption,
};
pub use crate::session_id::{SessionId, SessionMode};
pub use crate::settings::{
    DefaultTab, DiffView, Language, MAX_FONT_SIZE, MAX_UI_FONT_SIZE, MIN_FONT_SIZE,
    MIN_UI_FONT_SIZE, PermissionMode, Settings, TabGrid, ThemeCustom, ThemeKind, ThemeMode, Tint,
    WindowPlace,
};
pub use crate::store::{
    Normalise, ReadFailure, Store, StoreError, Unreadable, flush, take_failure, take_read_failures,
    take_unreadable,
};
pub use crate::transcript::Usage;
pub use crate::workspace::{DEFAULT_NAME, ICONS, Refused, Released, Seat, Workspace, WorkspaceId};

pub mod overlay;
pub mod terminal_job;

pub mod repository_settings;

pub mod resume_handoff;

/// Update-source settings and shared distribution protocol.
pub mod update;
/// Configured development-process definitions and portable run state.
pub mod workflow;

pub mod project_group;
pub mod project_sort;
