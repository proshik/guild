//! OpenCode's implementation of [`Harness`] — M14.
//!
//! From M14's Task 0, measured on the installed binary (v2.0.10):
//! - the interactive TUI is `opencode [--session <id>] [--continue] [--auto]` — no subcommand;
//! - resuming a session is `--session <id>`;
//! - the only approval argument is `--auto`;
//! - status arrives through a **plugin** (OpenCode has no config hook
//!   command in V2); the plugin subscribes to `session.status` (busy/retry/
//!   idle), `permission.asked`, `question.asked`, `session.error` and posts
//!   a normalised envelope to `GUILD_HOOK_SOCKET`, so the window's
//!   translation is the shared table.

use std::ffi::OsString;
use std::path::PathBuf;

use super::{Harness, HarnessKind};
use crate::settings::PermissionMode;

/// OpenCode.
pub struct OpenCodeHarness;

impl OpenCodeHarness {
    /// opencode's own config directory, where its plugins live:
    /// `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`. Whether it
    /// exists is whether opencode is [known](super::known) for its plugin.
    pub fn config_dir() -> PathBuf {
        Self::config_dir_in(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }

    /// [`Self::config_dir`] with the two variables given rather than read.
    pub fn config_dir_in(xdg_config_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
        xdg_config_home
            .map(PathBuf::from)
            .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_default()
            .join("opencode")
    }

    /// opencode's own data directory, where its sessions are kept:
    /// `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`. No such
    /// directory is no sessions, so nothing is worth running opencode for.
    pub fn data_dir() -> PathBuf {
        Self::data_dir_in(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
    }

    /// [`Self::data_dir`] with the two variables given rather than read.
    pub fn data_dir_in(xdg_data_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
        xdg_data_home
            .map(PathBuf::from)
            .or_else(|| home.map(|home| PathBuf::from(home).join(".local").join("share")))
            .unwrap_or_default()
            .join("opencode")
    }
}

impl Harness for OpenCodeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenCode
    }

    fn detect_in_front(&self, args: &str) -> bool {
        let mut words = args.split_whitespace();
        let Some(first) = words.next() else {
            return false;
        };
        if super::word_names_program("opencode", first) {
            return true;
        }
        let program = first.rsplit('/').next().unwrap_or(first);
        matches!(program, "node" | "bun" | "bash" | "sh" | "zsh")
            && words
                .next()
                .is_some_and(|w| super::word_names_program("opencode", w))
    }

    fn resumed_in(&self, args: &str) -> Option<String> {
        if !self.detect_in_front(args) {
            return None;
        }
        super::flag_value(args, &["--session", "-s"])
    }

    fn resume_args(&self, id: &str) -> Vec<OsString> {
        vec!["--session".into(), id.into()]
    }

    fn approval_args(&self, _permission: PermissionMode) -> Vec<OsString> {
        // This is Claude's permission vocabulary. OpenCode owns its policy;
        // only the person's explicit OpenCode arguments may enable --auto.
        Vec::new()
    }

    fn version_floor(&self) -> Option<(u32, u32)> {
        // The status plugin and the launch flags are the V2 shape (measured
        // on 2.0.10); before V2 an opencode session would launch but never
        // report status, which is a harness offered as working that is not.
        Some((2, 0))
    }
}
