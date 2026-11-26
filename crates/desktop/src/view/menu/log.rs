//! The Git tool's half of the right-button menu: a branch, or a commit.
//!
//! **Nothing here moves `HEAD`.** A branch offers to be *listed*, which
//! is a read — decision 3 of M8.3.2, and spec decision 18 still standing
//! at the time. Switching branches arrived in M8.3.3 and lives in the
//! title bar's popup, where it is a deliberate gesture rather than one
//! item in a list of copies.

use gpui::{ClipboardItem, Context};

use super::{Deed, Entry, Target};
use crate::Root;
use crate::i18n::Text;

/// What only a branch or a commit of the Git tool can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDeed {
    /// List this branch's commits. A read: `HEAD` does not move.
    ShowBranchLog,
    /// Go back to listing the branch `HEAD` is on.
    ShowHeadLog,
    /// The branch's name on the clipboard.
    CopyBranchName,
    /// The commit's hash, short and whole.
    CopyShortRevision,
    CopyRevision,
    /// Its first line.
    CopySubject,
}

/// A branch's menu. The one already listed offers the way back instead,
/// because *show its log* would do nothing.
pub fn entries_for_branch(listed: bool) -> Vec<Entry> {
    vec![
        if listed {
            Entry::Item(Text::MenuShowHeadLog, Deed::Log(LogDeed::ShowHeadLog))
        } else {
            Entry::Item(Text::MenuShowBranchLog, Deed::Log(LogDeed::ShowBranchLog))
        },
        Entry::Separator,
        Entry::Item(Text::MenuCopyBranchName, Deed::Log(LogDeed::CopyBranchName)),
    ]
}

/// A commit's menu: what a person copies out of a log — the revision, in
/// both lengths, and the message.
pub fn entries_for_commit() -> Vec<Entry> {
    vec![
        Entry::Item(
            Text::MenuCopyRevisionShort,
            Deed::Log(LogDeed::CopyShortRevision),
        ),
        Entry::Item(Text::MenuCopyRevision, Deed::Log(LogDeed::CopyRevision)),
        Entry::Item(Text::MenuCopySubject, Deed::Log(LogDeed::CopySubject)),
    ]
}

impl Root {
    /// Carry out a Git-tool deed on the row the pointer was on.
    pub(super) fn run_log_deed(&mut self, deed: LogDeed, target: &Target, cx: &mut Context<Root>) {
        match deed {
            LogDeed::ShowBranchLog => {
                if let Target::Branch { name, .. } = target {
                    self.show_branch_log(crate::git::Scope::Named(name.clone()), cx);
                }
            }
            LogDeed::ShowHeadLog => self.show_branch_log(crate::git::Scope::Head, cx),
            LogDeed::CopyBranchName => {
                if let Target::Branch { name, .. } = target {
                    cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
                }
            }
            LogDeed::CopyShortRevision => {
                if let Target::Commit { short, .. } = target {
                    cx.write_to_clipboard(ClipboardItem::new_string(short.clone()));
                }
            }
            LogDeed::CopyRevision => {
                if let Target::Commit { sha, .. } = target {
                    cx.write_to_clipboard(ClipboardItem::new_string(sha.clone()));
                }
            }
            LogDeed::CopySubject => {
                if let Target::Commit { summary, .. } = target {
                    cx.write_to_clipboard(ClipboardItem::new_string(summary.clone()));
                }
            }
        }
    }
}
