//! What the window does, grouped by subject.
//!
//! `Root` lives in the crate root; these modules implement its actions. They
//! share its fields, so the split is navigation, not compiler-enforced state
//! ownership. Keep protocol and lifecycle rules in their owning modules and
//! leave window coordination here. See `docs/runtime-ownership.md`.

mod agent_submit;
pub mod annotate;
pub(crate) mod appearance;
pub mod areas;
pub(crate) mod auto_ff;
pub mod body;
pub mod branches;
pub mod changes;
pub mod column;
pub mod command;
pub(crate) mod customizer;
pub mod declarations;
pub mod dependencies;
pub mod diff;
pub mod docks;
pub mod everywhere;
pub mod file_index;
pub mod files;
pub mod find_in_files;
pub mod fonts;
pub mod git;
pub mod history;
pub(crate) mod hunks;
pub mod ide;
pub(crate) mod isolation;
pub mod library;
pub mod links;
pub mod log;
pub mod lsp;
pub mod menu;
pub mod panels;
pub mod picker;
pub mod preview;
pub mod problems;
pub(crate) mod process;
pub mod projects;
pub mod push;
pub mod quit;
pub mod recent;
pub mod recent_files;
pub mod render;
pub mod run;
pub mod satellites;
pub mod search;
pub(crate) mod session_page;
pub mod sessions;
pub mod settings;
pub(crate) mod settled;
pub mod start;
pub mod status;
pub mod symbols;
pub mod tabs;
pub(crate) mod terminal_jobs;
pub mod terminal_panel;
pub mod tree;
pub(crate) mod tree_drag;
pub mod update;
pub mod usage;
pub mod usages;
pub mod watch;
pub mod workspaces;
pub mod xcode;

pub(crate) mod overlay;

pub(crate) mod clone_project;
pub(crate) mod forge;

pub(crate) mod speed_search;

pub(crate) mod transfers;

pub(crate) mod repository_settings;

mod resume_handoff;

#[cfg(feature = "adaptive-terminal-refresh")]
mod terminal_visibility;

pub(crate) mod workflow;

mod harness_retry;

pub(crate) mod tab_launcher;
