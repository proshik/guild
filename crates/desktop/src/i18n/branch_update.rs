//! Branch update guidance is formatted at display time, never persisted as UI text.
use super::{Language, Ui};

impl Ui {
    pub fn branch_update_receipt(
        self,
        receipt: &guild_session::settings::BranchUpdateReceipt,
    ) -> String {
        use guild_session::settings::BranchUpdateKind as ReceiptKind;
        let ru = self.0 == Language::Russian;
        let after = receipt.after.as_deref().unwrap_or("?");
        let guidance = match (receipt.kind, ru) {
            (ReceiptKind::Conflict, false) => "Merge conflicts retained. Open this worktree in Guild Terminal, resolve files, then git add <resolved paths> and git commit; or git merge --abort. No recovery command runs automatically.".into(),
            (ReceiptKind::Conflict, true) => "Конфликты сохранены. Откройте терминал этого worktree в Guild, исправьте файлы, затем git add <исправленные пути> и git commit; либо git merge --abort. Команды восстановления не запускаются автоматически.".into(),
            (ReceiptKind::MergeCommit, false) => format!("Merge commit created. In Guild Terminal inspect git show {after}; to undo, inspect current HEAD, then git revert -m 1 {after}."),
            (ReceiptKind::MergeCommit, true) => format!("Создан merge-коммит. В терминале Guild проверьте git show {after}; для отмены проверьте текущий HEAD, затем git revert -m 1 {after}."),
            (ReceiptKind::FastForward, false) => format!("Fast-forward; no single merge commit. In Guild Terminal inspect git log {}..{after}, then revert the intended commits in reverse order with git revert <commit> after checking current HEAD.", receipt.before),
            (ReceiptKind::FastForward, true) => format!("Fast-forward: отдельного merge-коммита нет. В терминале Guild проверьте git log {}..{after}, затем отмените нужные коммиты в обратном порядке через git revert <commit>, проверив текущий HEAD.", receipt.before),
            (ReceiptKind::Unchanged, false) => "Already up to date; HEAD did not change.".into(),
            (ReceiptKind::Unchanged, true) => "Ветка уже обновлена; HEAD не изменился.".into(),
            (ReceiptKind::FetchRefused, false) => "Fetch failed; no merge was performed. Inspect the upstream in Guild Terminal and fetch it manually before retrying. Full output is in Console.".into(),
            (ReceiptKind::FetchRefused, true) => "Fetch не выполнен; слияния не было. Проверьте upstream в терминале Guild и выполните fetch вручную перед повтором. Полный вывод доступен в Console.".into(),
            (ReceiptKind::Refused, false) => "Update refused. Inspect the diagnostics in this worktree's Guild Terminal before retrying.".into(),
            (ReceiptKind::Refused, true) => "Обновление отклонено. Перед повтором проверьте диагностику в терминале этого worktree в Guild.".into(),
        };
        format!(
            "{}\nHEAD: {} → {}\n{}\n{}",
            receipt.root.display(),
            receipt.before,
            after,
            guidance,
            self.branch_update_diagnostic(&receipt.diagnostic)
        )
    }
    /// Render current records in the active language and preserve legacy text.
    pub fn branch_update_record(
        self,
        record: &guild_session::settings::BranchUpdateRecord,
    ) -> String {
        match record {
            guild_session::settings::BranchUpdateRecord::Current(receipt) => {
                self.branch_update_receipt(receipt)
            }
            guild_session::settings::BranchUpdateRecord::Legacy(text) => text.clone(),
        }
    }

    /// The Update branch popup's footer: what the last manual update of
    /// this root did, in one short line. The hashes, the path and the
    /// recovery steps are the notice's and the Console's, not this line's;
    /// a legacy record, saved as text in an older language, says nothing.
    pub fn branch_update_last(
        self,
        record: &guild_session::settings::BranchUpdateRecord,
    ) -> Option<String> {
        use guild_session::settings::{BranchUpdateKind as Kind, BranchUpdateRecord as Record};
        let Record::Current(receipt) = record else {
            return None;
        };
        let ru = self.0 == Language::Russian;
        let outcome = match (receipt.kind, ru) {
            (Kind::Unchanged, false) => "already up to date",
            (Kind::Unchanged, true) => "ветка уже актуальна",
            (Kind::FastForward, false) => "fast-forwarded",
            (Kind::FastForward, true) => "перемотка вперёд",
            (Kind::MergeCommit, false) => "merge commit created",
            (Kind::MergeCommit, true) => "создан merge-коммит",
            (Kind::Conflict, false) => "stopped on conflicts",
            (Kind::Conflict, true) => "остановлено на конфликтах",
            (Kind::FetchRefused, false) => "fetch failed",
            (Kind::FetchRefused, true) => "fetch не выполнен",
            (Kind::Refused, false) => "refused",
            (Kind::Refused, true) => "отклонено",
        };
        Some(if ru {
            format!("Последний раз: {outcome}")
        } else {
            format!("Last: {outcome}")
        })
    }

    /// The sheet a chosen source asks with: which branch, from where.
    pub fn update_branch_title(self, branch: &str, source: &str) -> String {
        let (branch, source) = (super::sheet_name(branch), super::sheet_name(source));
        match self.0 {
            Language::English => format!("Update {branch} from {source}?"),
            Language::Russian => format!("Обновить {branch} из {source}?"),
        }
    }

    /// Under it, what the update will do: a fetch first only when the
    /// source is a remote's.
    pub fn update_branch_detail(self, branch: &str, source: &str, fetches: bool) -> String {
        let (branch, source) = (super::sheet_name(branch), super::sheet_name(source));
        match (self.0, fetches) {
            (Language::English, true) => {
                format!("Fetches {source} and merges it into {branch}.")
            }
            (Language::English, false) => format!("Merges {source} into {branch}."),
            (Language::Russian, true) => format!("Загрузит {source} и сольёт его в {branch}."),
            (Language::Russian, false) => format!("Сольёт {source} в {branch}."),
        }
    }

    /// Localize Guild's guard sentences without translating Git's diagnostics.
    pub fn branch_update_diagnostic(self, text: &str) -> String {
        if self.0 != Language::Russian {
            return text.to_owned();
        }
        let mut text = text.to_owned();
        for (english, russian) in [
            (
                "Update fetch timed out; no merge was run. Retry after checking the remote.",
                "Время ожидания fetch истекло; слияния не было. Проверьте remote перед повтором.",
            ),
            (
                "Update requires an attached branch with a commit.",
                "Для обновления нужна текущая ветка с коммитом.",
            ),
            (
                "Selected source no longer exists. Choose a source.",
                "Выбранного источника больше нет. Выберите другой.",
            ),
            (
                "The current branch has no available upstream. Choose another source.",
                "Для этой ветки нет доступного upstream. Выберите другой источник.",
            ),
            (
                "The selected remote is unavailable. Choose another source.",
                "Выбранный remote недоступен. Выберите другой источник.",
            ),
            (
                "The originating branch or HEAD changed. Retry from the current branch.",
                "Исходная ветка или HEAD изменились. Повторите из текущей ветки.",
            ),
            (
                "Finish the current Git operation before updating.",
                "Завершите текущую операцию Git перед обновлением.",
            ),
            (
                "Commit or remove working tree, index and untracked changes before updating.",
                "Перед обновлением сохраните в коммите или уберите изменения рабочих файлов, индекса и неотслеживаемые файлы.",
            ),
            (
                "The selected source changed. Choose it again before updating.",
                "Выбранный источник изменился. Выберите его заново перед обновлением.",
            ),
            (
                "Save unsaved files before updating the branch.",
                "Сохраните файлы перед обновлением ветки.",
            ),
        ] {
            text = text.replace(english, russian);
        }
        text
    }
}
