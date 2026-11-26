//! The workspace switcher's right-button menu.

use gpui::{Context, Window, point, px};

use super::{Deed, Entry, Target};
use crate::Root;
use crate::i18n::Text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDeed {
    Rename,
    ChangeIcon,
    Delete,
}

pub fn entries_for_workspace(last: bool) -> Vec<Entry> {
    let mut entries = vec![
        Entry::Item(
            Text::WorkspaceRenameAction,
            Deed::Workspace(WorkspaceDeed::Rename),
        ),
        Entry::Item(
            Text::WorkspaceChangeIcon,
            Deed::Workspace(WorkspaceDeed::ChangeIcon),
        ),
    ];
    if !last {
        entries.push(Entry::Separator);
        entries.push(Entry::Item(
            Text::WorkspaceDelete,
            Deed::Workspace(WorkspaceDeed::Delete),
        ));
    }
    entries
}

impl Root {
    pub(crate) fn run_workspace_deed(
        &mut self,
        deed: WorkspaceDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Target::Workspace { id, .. } = target else {
            return;
        };
        match deed {
            WorkspaceDeed::Rename => {
                self.open_rename_workspace(*id, point(px(12.0), px(42.0)), window, cx)
            }
            WorkspaceDeed::ChangeIcon => {
                self.workspace_icon_picker =
                    Some(crate::view::workspaces::IconTarget::Workspace(*id));
                self.workspace_icon_at.set(point(px(12.), px(42.)));
                // Reset every time it opens — a highlight left from the
                // workspace picked last time is not where this one's
                // current icon is.
                self.workspace_icon_lit = None;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            WorkspaceDeed::Delete => self.delete_workspace_asking(*id, window, cx),
        }
    }
}
