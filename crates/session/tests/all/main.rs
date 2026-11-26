//! Everything a caller can reach in `guild-session`, in one binary.
//!
//! The same reason as the desktop's, in smaller numbers: cargo builds
//! every `tests/*.rs` as its own crate and links it separately. This
//! crate links no gpui, so each link is cheap — the layout is one for
//! both crates so that a test has one home to be added to, not two.

mod areas;
mod codex_transcript;
mod command;
mod desk;
mod history;
mod layout;
mod lifecycle;
mod login_path;
mod panes;
mod parsing;
mod picker;
mod project_state;
mod scratch;
mod settings;
mod split;
mod status;
mod store;
mod transcript;
mod usage;
mod workspace;

mod overlay;
mod terminal_job;

mod process;

mod markdown_modes;

mod merge_requests;
mod merge_requests_glab;

mod repository_settings;

mod resume_handoff;

mod stream_compatibility;

mod update;
mod workflow;

mod project_group;
mod project_sort;
