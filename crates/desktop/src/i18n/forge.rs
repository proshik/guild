//! A merge request's path to merge, worded when shown — GLD-063. The model
//! (`guild_session::merge_requests::status`) keeps facts; the words are here.
use super::{Language, Ui};
use crate::forge::{Problem, Why};
use guild_session::merge_requests::{
    glab::Error,
    status::{Block, Check, Pipeline},
};

/// The Russian noun for `n`: one, a few, many — `1 тред`, `3 треда`,
/// `6 тредов`.
fn russian<'a>(n: u32, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (tens, units) = (n % 100, n % 10);
    if (11..=14).contains(&tens) {
        many
    } else if units == 1 {
        one
    } else if (2..=4).contains(&units) {
        few
    } else {
        many
    }
}

impl Ui {
    /// One line of the hover card. `now` is seconds since the epoch, for a
    /// merged or closed MR's age.
    pub fn mr_check(self, check: &Check, now: i64) -> String {
        let ru = self.0 == Language::Russian;
        match check {
            Check::Approvals {
                given: 0,
                required: 0,
            } => if ru {
                "Одобрение не требуется"
            } else {
                "No approval required"
            }
            .to_owned(),
            Check::Approvals { given, required } => {
                if ru {
                    format!("Одобрено {given} из {required}")
                } else {
                    format!("Approved {given} of {required}")
                }
            }
            Check::Threads(0) => if ru {
                "Нет нерешённых тредов"
            } else {
                "No unresolved threads"
            }
            .to_owned(),
            Check::Threads(n) => {
                if ru {
                    format!(
                        "{n} {}",
                        russian(
                            *n,
                            "нерешённый тред",
                            "нерешённых треда",
                            "нерешённых тредов"
                        )
                    )
                } else if *n == 1 {
                    "1 unresolved thread".to_owned()
                } else {
                    format!("{n} unresolved threads")
                }
            }
            Check::Pipeline(pipeline) => match (pipeline, ru) {
                (Pipeline::Passed, false) => "Pipeline passed".to_owned(),
                (Pipeline::Passed, true) => "Пайплайн прошёл".to_owned(),
                (Pipeline::Running, false) => "Pipeline running".to_owned(),
                (Pipeline::Running, true) => "Пайплайн выполняется".to_owned(),
                (Pipeline::Failed, false) => "Pipeline failed".to_owned(),
                (Pipeline::Failed, true) => "Пайплайн упал".to_owned(),
                (Pipeline::Missing, false) => "No pipeline has run".to_owned(),
                (Pipeline::Missing, true) => "Пайплайн не запускался".to_owned(),
                (Pipeline::Other(word), false) => format!("Pipeline {word}"),
                (Pipeline::Other(word), true) => format!("Пайплайн: {word}"),
            },
            Check::Conflict { target } => {
                if ru {
                    format!("Конфликт с {target}")
                } else {
                    format!("Conflict with {target}")
                }
            }
            Check::NoConflict { target } => {
                if ru {
                    format!("Нет конфликта с {target}")
                } else {
                    format!("No conflict with {target}")
                }
            }
            Check::Rebase { target } => {
                if ru {
                    format!("Нужен rebase на {target}")
                } else {
                    format!("Needs a rebase onto {target}")
                }
            }
            Check::Blocked(Block::Unknown(status)) => {
                if ru {
                    format!("GitLab пока не даёт слить: {status}")
                } else {
                    format!("GitLab says it cannot be merged yet: {status}")
                }
            }
            Check::Blocked(block) => {
                if ru {
                    format!("Заблокирован: {}", self.mr_block(block))
                } else {
                    format!("Blocked: {}", self.mr_block(block))
                }
            }
            Check::Draft => if ru { "Черновик" } else { "Draft" }.to_owned(),
            Check::Unpushed(n) => {
                if ru {
                    format!(
                        "{n} {}",
                        russian(
                            *n,
                            "коммит не отправлен",
                            "коммита не отправлены",
                            "коммитов не отправлено"
                        )
                    )
                } else if *n == 1 {
                    "1 commit not pushed".to_owned()
                } else {
                    format!("{n} commits not pushed")
                }
            }
            Check::Merged(at) => match (at, ru) {
                (Some(at), false) => format!("Merged {}", self.ago((now - at).max(0))),
                (Some(at), true) => format!("Слит {}", self.ago((now - at).max(0))),
                (None, false) => "Merged".to_owned(),
                (None, true) => "Слит".to_owned(),
            },
            Check::Closed(at) => match (at, ru) {
                (Some(at), false) => format!("Closed {}", self.ago((now - at).max(0))),
                (Some(at), true) => format!("Закрыт {}", self.ago((now - at).max(0))),
                (None, false) => "Closed".to_owned(),
                (None, true) => "Закрыт".to_owned(),
            },
        }
    }

    /// What the status bar says beside `!N ↗`: the card's first failing
    /// line, with threads called comments, as the approved board has it.
    pub fn mr_status_line(self, check: &Check) -> String {
        match check {
            Check::Threads(n) if *n > 0 => {
                if self.0 == Language::Russian {
                    format!(
                        "{n} {}",
                        russian(
                            *n,
                            "нерешённый комментарий",
                            "нерешённых комментария",
                            "нерешённых комментариев"
                        )
                    )
                } else if *n == 1 {
                    "1 unresolved comment".to_owned()
                } else {
                    format!("{n} unresolved comments")
                }
            }
            other => self.mr_check(other, 0),
        }
    }

    /// Why GitLab blocks an MR, for *Blocked: …*.
    pub fn mr_block(self, block: &Block) -> String {
        let ru = self.0 == Language::Russian;
        let words = match (block, ru) {
            (Block::AnotherMergeRequest, false) => "waits for another merge request",
            (Block::AnotherMergeRequest, true) => "ждёт другой merge request",
            (Block::ExternalChecks, false) => "external status checks have not passed",
            (Block::ExternalChecks, true) => "внешние проверки статуса не пройдены",
            (Block::ChangesRequested, false) => "a reviewer requested changes",
            (Block::ChangesRequested, true) => "ревьюер запросил изменения",
            (Block::JiraIssue, false) => "needs a Jira issue key",
            (Block::JiraIssue, true) => "нужен ключ задачи Jira",
            (Block::MergeTime, false) => "waits for its scheduled merge time",
            (Block::MergeTime, true) => "ждёт назначенного времени слияния",
            (Block::SecurityPolicy, false) => "a security policy is not met",
            (Block::SecurityPolicy, true) => "не выполнена политика безопасности",
            (Block::LockedPaths, false) => "it touches locked paths",
            (Block::LockedPaths, true) => "затрагивает заблокированные пути",
            (Block::LockedLfsFiles, false) => "it touches locked LFS files",
            (Block::LockedLfsFiles, true) => "затрагивает заблокированные файлы LFS",
            (Block::TitlePattern, false) => "its title does not match the required pattern",
            (Block::TitlePattern, true) => "заголовок не соответствует шаблону",
            (Block::CommitStatus, false) => "commit status checks have not passed",
            (Block::CommitStatus, true) => "проверки статуса коммитов не пройдены",
            (Block::NotApproved, false) => "it needs approval",
            (Block::NotApproved, true) => "нужно одобрение",
            (Block::Unknown(status), _) => return status.clone(),
        };
        words.to_owned()
    }

    /// Why a row says nothing about an MR, in its glyph's hover.
    pub fn mr_why(self, why: &Why) -> String {
        match (why, self.0 == Language::Russian) {
            (Why::NotLoggedIn(host), false) => format!("glab is not logged in to {host}"),
            (Why::NotLoggedIn(host), true) => format!("glab не выполнил вход на {host}"),
            (Why::NotFound, false) => "glab not found".to_owned(),
            (Why::NotFound, true) => "glab не найден".to_owned(),
            (Why::PortedHost(host), false) => {
                format!("glab cannot address a host with a port: {host}")
            }
            (Why::PortedHost(host), true) => {
                format!("glab не умеет обращаться к хосту с портом: {host}")
            }
        }
    }

    /// What a repository could not ask at all, in one line — its header's
    /// words: the reason, or the project and why its batch failed.
    pub fn mr_problem(self, problem: &Problem) -> String {
        match problem {
            Problem::Why(why) => self.mr_why(why),
            Problem::Batch { project, error, .. } => {
                let why = self.mr_error(error);
                if self.0 == Language::Russian {
                    format!("{project}: merge requests не прочитаны: {why}")
                } else {
                    format!("{project}: merge requests not read: {why}")
                }
            }
        }
    }

    /// The second line's start: `!294 · 1m ago`.
    pub fn mr_meta(self, iid: u64, age: &str) -> String {
        if age.is_empty() {
            format!("!{iid}")
        } else {
            format!("!{iid} · {age}")
        }
    }

    /// A branch asked and never answered: unknown, and why — the plain
    /// glyph's hover (code review r1, adversarial-9).
    pub fn mr_unknown(self, error: &Error) -> String {
        let why = self.mr_error(error);
        if self.0 == Language::Russian {
            format!("Merge request неизвестен: {why}")
        } else {
            format!("Merge request unknown: {why}")
        }
    }

    /// A refresh that failed after the card's state was read: when that
    /// read was — the last that landed — and why the refresh after it
    /// failed (code review r1, bugs+impl-14).
    pub fn mr_failure(self, error: &Error, read_seconds_ago: i64) -> String {
        let why = self.mr_error(error);
        let ago = self.ago(read_seconds_ago.max(0));
        if self.0 == Language::Russian {
            format!("Прочитано {ago} · обновление не удалось: {why}")
        } else {
            format!("Read {ago} · the refresh after it failed: {why}")
        }
    }

    /// Why a read failed, in words.
    fn mr_error(self, error: &Error) -> String {
        let ru = self.0 == Language::Russian;
        match (error, ru) {
            (Error::NotInstalled, false) => "glab not found".to_owned(),
            (Error::NotInstalled, true) => "glab не найден".to_owned(),
            (Error::SignedOut { host }, _) => self.mr_why(&Why::NotLoggedIn(host.clone())),
            (Error::TimedOut, false) => "glab did not answer in time".to_owned(),
            (Error::TimedOut, true) => "glab не ответил вовремя".to_owned(),
            (Error::Changed, false) => "the merge request changed while it was read".to_owned(),
            (Error::Changed, true) => "merge request изменился, пока читался".to_owned(),
            (Error::Stopped, false) => "stopped".to_owned(),
            (Error::Stopped, true) => "остановлено".to_owned(),
            (Error::Failed(line), _) => line.clone(),
        }
    }
}
