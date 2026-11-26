//! Commit selection words are formatted only when displayed, with localized counts.
use super::{Language, Text, Ui, russian_count};
use crate::changes::EligibilityError;

#[derive(Debug, Clone, Copy)]
pub enum CommitLabel {
    Diff,
    Include,
    Exclude,
    Add,
    Rollback,
    Trash,
    CopyPaths,
    CopyFullPaths,
}
impl Ui {
    pub fn commit_menu_label(self, action: CommitLabel, n: usize) -> String {
        use CommitLabel::*;
        if self.0 == Language::Russian {
            let items = russian_count(n as i64, "элемент", "элемента", "элементов");
            let files = russian_count(n as i64, "файл", "файла", "файлов");
            return match action {
                Diff => format!("Показать различия: {files}"),
                Include => format!("Включить в коммит: {items}"),
                Exclude => format!("Исключить из коммита: {items}"),
                Add => format!("Добавить в Git сейчас: {files}"),
                Rollback => format!("Откатить: {items}…"),
                Trash => format!("Переместить в корзину: {items}…"),
                CopyPaths => format!(
                    "Копировать {}",
                    russian_count(
                        n as i64,
                        "относительный путь",
                        "относительных пути",
                        "относительных путей"
                    )
                ),
                CopyFullPaths => format!(
                    "Копировать {}",
                    russian_count(n as i64, "полный путь", "полных пути", "полных путей")
                ),
            };
        }
        let items = if n == 1 { "item" } else { "items" };
        let files = if n == 1 { "file" } else { "files" };
        let paths = if n == 1 { "path" } else { "paths" };
        match action {
            Diff => format!("Show diffs for {n} selected {items}"),
            Include => format!("Include {n} selected {items} in commit"),
            Exclude => format!("Exclude {n} selected {items} from commit"),
            Add => format!("Add {n} {files} to Git now"),
            Rollback => format!("Rollback {n} selected {items}…"),
            Trash => format!("Move {n} selected {items} to Trash…"),
            CopyPaths => format!("Copy {n} relative {paths}"),
            CopyFullPaths => format!("Copy {n} full {paths}"),
        }
    }
    pub fn commit_eligibility(self, error: &EligibilityError) -> String {
        use EligibilityError::*;
        let ru = self.0 == Language::Russian;
        match error {
            EmbeddedNul => self.text(Text::CommitPathNul).to_owned(),
            UnsupportedPath => self.text(Text::CommitUnsupportedPath).to_owned(),
            Missing(path) => {
                if ru {
                    format!("{} больше нет в списке", path.display())
                } else {
                    format!("{} is no longer available", path.display())
                }
            }
            Conflict(path) => {
                if ru {
                    format!("В {} остались неразрешённые конфликты", path.display())
                } else {
                    format!("{} has unresolved conflicts", path.display())
                }
            }
            Loading(path) => {
                if ru {
                    format!("Содержимое {} ещё загружается", path.display())
                } else {
                    format!("{} is still loading", path.display())
                }
            }
            Empty(path) => {
                if ru {
                    format!(
                        "В {} нет файлов для коммита. Git не отслеживает пустые каталоги",
                        path.display()
                    )
                } else {
                    format!(
                        "{} has no eligible files; Git does not track empty directories",
                        path.display()
                    )
                }
            }
            Read(path, error) => {
                format!("{}: {}", path.display(), self.commit_directory_error(error))
            }
        }
    }
    pub fn commit_directory_error(self, error: &str) -> String {
        // Git/OS diagnostics stay intact; local gateway wrappers and guards use Ui.
        let error = error.strip_prefix("git failed: ").unwrap_or(error);
        let error = match error {
            "not a git repository" => self.text(Text::DiffNoRepository).to_owned(),
            "git is not installed" => self.text(Text::DiffNoGit).to_owned(),
            "Path contains NUL" => self.text(Text::CommitPathNul).to_owned(),
            "Unsupported non-UTF-8 path" => self.text(Text::CommitUnsupportedPath).to_owned(),
            "Invalid directory name" => self.text(Text::CommitDirectoryInvalidName).to_owned(),
            _ => error.to_owned(),
        };
        if self.0 == Language::Russian {
            format!("Не удалось прочитать: {error}")
        } else {
            format!("Cannot read: {error}")
        }
    }
    pub fn commit_directory_count(self, eligible: usize, ignored: usize) -> String {
        if self.0 == Language::Russian {
            format!(
                "{} для коммита · {}",
                russian_count(eligible as i64, "файл", "файла", "файлов"),
                russian_count(
                    ignored as i64,
                    "игнорируемый путь",
                    "игнорируемых пути",
                    "игнорируемых путей"
                )
            )
        } else {
            format!(
                "{eligible} eligible {} · {ignored} ignored",
                if eligible == 1 { "file" } else { "files" }
            )
        }
    }
    pub fn commit_ignored_only(self, ignored: usize) -> String {
        if self.0 == Language::Russian {
            format!(
                "Только игнорируемые: {}; нет файлов для коммита",
                russian_count(ignored as i64, "путь", "пути", "путей")
            )
        } else {
            format!("Ignored only ({ignored}); no eligible files")
        }
    }
    pub fn commit_rollback_title(self, n: usize) -> String {
        if self.0 == Language::Russian {
            format!(
                "Откатить {}?",
                russian_count(n as i64, "файл", "файла", "файлов")
            )
        } else {
            format!("Rollback {n} {}?", if n == 1 { "file" } else { "files" })
        }
    }
    /// Shared by Explorer and Commit; directories mean the whole on-disk subtree.
    pub fn trash_selection_detail(self, n: usize) -> String {
        if self.0 == Language::Russian {
            format!(
                "{}. Каталоги будут перемещены со всем содержимым, включая скрытые и игнорируемые файлы.",
                russian_count(
                    n as i64,
                    "выбранный путь",
                    "выбранных пути",
                    "выбранных путей"
                )
            )
        } else {
            format!(
                "{n} selected {}. Directories include all contents, including hidden and ignored files.",
                if n == 1 { "path" } else { "paths" }
            )
        }
    }
}
