//! Codex's implementation of [`Harness`] — M14.
//!
//! From M14's Task 0, measured on the installed binary (0.155.1) and the
//! live configuration:
//! - the interactive TUI is `codex [PROMPT]` — no subcommand; `codex resume <id>` continues a session by id;
//! - Guild launches Codex with explicit sandbox and approval options from its
//!   Codex-specific settings; the shared Claude PermissionMode does not apply;
//! - status arrives through Codex's **native hooks**, `[[hooks.<event>]]`
//!   in `~/.codex/config.toml`, whose event names are the same as Claude's
//!   (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
//!   `PermissionRequest`, `Stop`). The hook command receives the event name
//!   as its argument, and Guild's own hook script posts a normalised
//!   envelope to `GUILD_HOOK_SOCKET`, reading the session id from
//!   `GUILD_SESSION`.

use std::ffi::OsString;
use std::path::PathBuf;

use super::{Harness, HarnessKind};
use crate::settings::PermissionMode;

/// Codex.
pub struct CodexHarness;

impl CodexHarness {
    /// Where Codex keeps everything Guild touches of it — the config its
    /// hooks go in and the rollouts the start screen reads: `$CODEX_HOME`,
    /// else `~/.codex`. One rule for both, or a `$CODEX_HOME` installs the
    /// hooks in one directory and reads the sessions from another.
    pub fn home() -> PathBuf {
        Self::home_in(std::env::var_os("CODEX_HOME"), std::env::var_os("HOME"))
    }

    /// [`Self::home`] with the two variables given rather than read —
    /// the rule as a test can reach it without touching the process's
    /// environment.
    pub fn home_in(codex_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
        codex_home
            .map(PathBuf::from)
            .or_else(|| home.map(|home| PathBuf::from(home).join(".codex")))
            .unwrap_or_default()
    }
}

impl Harness for CodexHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Codex
    }

    fn detect_in_front(&self, args: &str) -> bool {
        let mut words = args.split_whitespace();
        let Some(first) = words.next() else {
            return false;
        };
        if super::word_names_program("codex", first) {
            return true;
        }
        let program = first.rsplit('/').next().unwrap_or(first);
        matches!(program, "node" | "bun" | "bash" | "sh" | "zsh")
            && words
                .next()
                .is_some_and(|w| super::word_names_program("codex", w))
    }

    /// `codex resume ID` — the id as the first word after `resume`. Its
    /// options may come before the id (`codex resume -c k=v ID`), and some
    /// of them take a value a word-splitter cannot tell from the id, so
    /// that spelling is not recognised: M13.3's task 0.
    fn resumed_in(&self, args: &str) -> Option<String> {
        if !self.detect_in_front(args) {
            return None;
        }
        let mut words = args
            .split_whitespace()
            .skip_while(|word| !super::word_names_program("codex", word));
        words.next()?;
        // Configured global arguments precede Guild's resume subcommand.
        // Stop at positional text and unknown options rather than scanning a prompt.
        loop {
            let word = words.next()?;
            if word == "resume" {
                break;
            }
            if matches!(
                word,
                "--model"
                    | "-m"
                    | "--config"
                    | "-c"
                    | "--profile"
                    | "-p"
                    | "--cd"
                    | "-C"
                    | "--sandbox"
                    | "-s"
                    | "--ask-for-approval"
                    | "-a"
                    | "--enable"
                    | "--disable"
                    | "--add-dir"
            ) {
                words.next()?;
            } else if !matches!(
                word,
                "--oss" | "--search" | "--full-auto" | "--dangerously-bypass-approvals-and-sandbox"
            ) && ![
                "--model=",
                "--config=",
                "--profile=",
                "--cd=",
                "--sandbox=",
                "--ask-for-approval=",
                "--enable=",
                "--disable=",
                "--add-dir=",
            ]
            .iter()
            .any(|flag| word.starts_with(flag))
            {
                return None;
            }
        }
        words
            .next()
            .filter(|word| !word.starts_with('-'))
            .map(str::to_owned)
    }

    fn resume_args(&self, id: &str) -> Vec<OsString> {
        vec!["resume".into(), id.into()]
    }

    fn approval_args(&self, _permission: PermissionMode) -> Vec<OsString> {
        // Guild’s shared permission mode is Claude vocabulary. The desktop
        // launcher composes Codex-specific sandbox and approval settings.
        Vec::new()
    }
}

impl CodexHarness {
    /// One of Codex's hook payloads as the managed-input latch reads it —
    /// a status signal no longer shown, only Codex managed live input's
    /// evidence. A Codex `Stop` is a completed turn.
    pub fn status_from_payload(
        &self,
        payload: &serde_json::Value,
    ) -> Option<(crate::SessionId, crate::status::Signal)> {
        crate::status::from_hook(payload).map(|(session, signal)| {
            (
                session,
                if signal == crate::status::Signal::TurnEnded {
                    crate::status::Signal::TurnCompleted
                } else {
                    signal
                },
            )
        })
    }
}
