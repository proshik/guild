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

use super::{Harness, HarnessKind};
use crate::settings::PermissionMode;

/// OpenCode.
pub struct OpenCodeHarness;

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
