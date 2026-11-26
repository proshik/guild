//! Why auto fast-forward left the default branch alone, worded when shown —
//! the reason is kept language-neutral, decision 21.
use super::{Language, Ui};
use crate::git::auto_ff::Reason;

impl Ui {
    /// The one line Update branch's hint and source list carry for a skip.
    pub fn auto_ff_skipped(self, branch: &str, reason: Reason) -> String {
        let russian = self.0 == Language::Russian;
        let why = match (reason, russian) {
            (Reason::NoUpstream, false) => "it has no remote upstream",
            (Reason::NoUpstream, true) => "у неё нет upstream на remote",
            (Reason::SymbolicTracking, false) => {
                "its remote-tracking ref is a symbolic ref; see git symbolic-ref"
            }
            (Reason::SymbolicTracking, true) => {
                "её remote-tracking ref — символическая ссылка, см. git symbolic-ref"
            }
            (Reason::SymbolicBranch, false) => "it is a symbolic ref; see git symbolic-ref",
            (Reason::SymbolicBranch, true) => "она — символическая ссылка, см. git symbolic-ref",
            (Reason::FetchFailed, false) => "the fetch failed; see Console",
            (Reason::FetchFailed, true) => "fetch не выполнен, подробности в Console",
            (Reason::Diverged, false) => "it has commits its upstream does not",
            (Reason::Diverged, true) => "в ней есть коммиты, которых нет в upstream",
            (Reason::Dirty, false) => "its checkout has uncommitted changes",
            (Reason::Dirty, true) => "в её checkout есть незакоммиченные изменения",
            (Reason::OperationInProgress, false) => {
                "a Git operation is in progress in its checkout"
            }
            (Reason::OperationInProgress, true) => "в её checkout не завершена операция Git",
            (Reason::Unsaved, false) => "its checkout has unsaved files",
            (Reason::Unsaved, true) => "в её checkout есть несохранённые файлы",
            (Reason::BranchChanged, false) => "it changed while it was being fetched",
            (Reason::BranchChanged, true) => "она изменилась, пока шёл fetch",
            (Reason::MissingWorktree, false) => "its worktree folder is missing",
            (Reason::MissingWorktree, true) => "папки её worktree нет на диске",
            (Reason::Refused, false) => "Git refused the fast-forward; see Console",
            (Reason::Refused, true) => "Git отклонил перемотку, подробности в Console",
            (Reason::GroupFolder, false) => "its checkout holds a project group's folder",
            (Reason::GroupFolder, true) => "в её checkout лежит папка группы проектов",
        };
        if russian {
            format!("Автоперемотка {branch} пропущена: {why}")
        } else {
            format!("Auto fast-forward of {branch} skipped: {why}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [Reason; 12] = [
        Reason::NoUpstream,
        Reason::SymbolicTracking,
        Reason::SymbolicBranch,
        Reason::FetchFailed,
        Reason::Diverged,
        Reason::Dirty,
        Reason::OperationInProgress,
        Reason::Unsaved,
        Reason::BranchChanged,
        Reason::MissingWorktree,
        Reason::Refused,
        Reason::GroupFolder,
    ];

    /// Every reason names the branch and reads differently from the others,
    /// in both languages.
    #[test]
    fn every_reason_names_the_branch_and_says_something_of_its_own() {
        // A new reason fails this match, which keeps the list above whole.
        match Reason::NoUpstream {
            Reason::NoUpstream
            | Reason::SymbolicTracking
            | Reason::SymbolicBranch
            | Reason::FetchFailed
            | Reason::Diverged
            | Reason::Dirty
            | Reason::OperationInProgress
            | Reason::Unsaved
            | Reason::BranchChanged
            | Reason::MissingWorktree
            | Reason::Refused
            | Reason::GroupFolder => {}
        }
        for language in Language::ALL {
            let ui = Ui(*language);
            let lines: Vec<String> = EVERY
                .iter()
                .map(|reason| ui.auto_ff_skipped("trunk", *reason))
                .collect();
            for line in &lines {
                assert!(line.contains("trunk"), "{line}");
            }
            let mut distinct = lines.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), lines.len(), "{lines:?}");
        }
    }
}
