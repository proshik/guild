//! Claude Code's implementation of [`Harness`] — M14.
//!
//! Claude is the harness the window was written around: its launch, resume
//! and status were all here already, scattered as `claude` specifics, and
//! this module is where they come to rest. Translation is the same table
//! everywhere because every harness's hook script posts the same envelope;
//! what differs here from the other two is the program, how to detect it in
//! front of a pty, and how to resume it.

use std::ffi::OsString;

use super::HarnessKind;
use crate::settings::PermissionMode;

use super::Harness;

/// Claude Code.
pub struct ClaudeHarness;

impl Harness for ClaudeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Claude
    }

    fn detect_in_front(&self, args: &str) -> bool {
        let mut words = args.split_whitespace();
        let Some(first) = words.next() else {
            return false;
        };
        let is_claude = |word: &str| {
            let name = word.rsplit('/').next().unwrap_or(word);
            name == "claude"
                || word.contains("/share/claude/versions/")
                || word.contains("@anthropic-ai/claude-code/")
                || super::word_names_program("claude", word)
        };
        if is_claude(first) {
            return true;
        }
        let program = first.rsplit('/').next().unwrap_or(first);
        matches!(program, "node" | "bash" | "sh" | "zsh" | "bun")
            && words.next().is_some_and(is_claude)
    }

    fn resumed_in(&self, args: &str) -> Option<String> {
        if !self.detect_in_front(args) {
            return None;
        }
        super::flag_value(args, &["--resume", "-r"])
    }

    /// `--session-id` too: `claude` refuses a taken id, so it never
    /// *continues* one — but a process started so is writing it.
    fn holds(&self, args: &str, conversation: &str) -> bool {
        self.detect_in_front(args)
            && (self.resumed_in(args).as_deref() == Some(conversation)
                || super::flag_value(args, &["--session-id"]).as_deref() == Some(conversation))
    }

    fn resume_args(&self, id: &str) -> Vec<OsString> {
        vec!["--resume".into(), id.into()]
    }

    fn approval_args(&self, permission: PermissionMode) -> Vec<OsString> {
        vec!["--permission-mode".into(), permission.as_cli_value().into()]
    }
}
