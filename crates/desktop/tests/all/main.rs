//! Everything a caller can reach in `guild-desktop`, in one binary.
//!
//! **One target, not thirty.** Cargo builds every `tests/*.rs` as its own
//! crate and links it separately, and each link here pulls in the whole
//! graph with gpui in it: measured on 2026-09-04, one such binary costs
//! about 13 seconds and there were thirty-one of them — ten minutes of
//! linking for fifteen seconds of tests.
//!
//! So the files became modules of one target. Nothing else changed: each
//! is still a topic, still reaches the crate only through its public
//! door, and still says in its own words what it is checking.

mod action_labels;
mod agents;
mod annotate;
mod app_appearance;
mod area_cards;
mod auto_ff;
mod autosave;
mod binding;
mod branch;
mod branch_list;
mod branch_update;
mod builtin_palettes;
mod card_outline;
mod changelog;
mod changes;
#[path = "../../../../test-support/child_lifetime.rs"]
mod child_lifetime;
mod clone_project;
mod closed_tabs;
mod closing;
mod column;
mod console;
mod customizer;
mod declaration;
mod deps;
mod diff;
mod diff_base;
mod diff_tab;
mod diff_window;
mod dock_menu;
mod double_tap;
mod drag;
mod editor_editing;
mod editor_git_markers;
mod editor_reveal;
mod editor_runs;
mod everywhere;
mod file_index;
mod file_selection;
mod files_tree;
mod find_in_files;
mod font_list;
mod fonts;
mod forge;
mod fronts;
mod fuzzy;
mod git_blame;
mod git_branches;
mod git_hunk;
mod git_log;
mod git_read_budget;
mod git_stashes;
mod git_status;
mod git_worktrees;
mod git_writes;
mod go_vendor;
mod gutter;
mod highlight;
mod history;
mod hooks;
mod hunk;
mod icons;
mod ide;
mod image_file;
mod jars;
mod labels;
mod leftovers;
mod log_layout;
mod lsp;
mod lsp_progress;
mod markdown_component;
mod markdown_file;
mod menu;
mod notices;
mod observer;
mod occurrence_paint;
mod occurrences;
mod one_menu;
mod open_files;
mod open_in;
mod origin;
mod palettes;
mod panel_corners;
mod paths;
mod pick;
mod picker;
mod preview;
mod problems;
mod program_status;
mod project_groups;
mod push;
mod push_dialog;
mod recent_files;
mod run;
mod run_tabs;
mod scratch;
mod search;
mod search_routing;
mod search_tree;
mod session_list;
mod sizes;
mod smart_selection;
mod sourcekit;
mod stage;
mod start;
mod statusline;
mod swap_guild;
mod symbols;
mod syntax;
mod tab_list;
mod tab_names;
mod task_invocations;
mod task_sources;
mod terminal_facts;
mod terminal_find;
mod terminal_links;
mod terminal_resize;
mod terminal_select;
mod terminal_sprite;
mod terminal_wrapped_links;
mod theme_custom;
mod tree;
mod tree_status;
mod update;
mod usage;
mod usage_codex;
mod usages;
mod watch;
mod windows;
mod work_card;
mod workspaces;
mod worktree_tabs;
mod xtversion;

mod desktop_cli;
mod terminal_jobs;

mod file_save;

mod text_fields;

mod terminal_panel;

mod speed_search;

mod component_theme;

#[path = "../../../../test-support/lsp_fixture.rs"]
mod lsp_fixture;

mod source_snippet;

mod log_graph;
mod source_viewers;

mod diagrams;
