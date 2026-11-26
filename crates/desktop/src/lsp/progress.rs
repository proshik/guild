//! What a server says it is doing, kept by token. M8.5.1.
//!
//! Pure: `root::lsp` feeds every notification to [`Work::absorb`], and the
//! status bar and its list read [`Work::headline`] and [`Work::tasks`].
//!
//! **A task is a token.** `$/progress` begins, reports and ends work by a
//! token of the server's choosing, and a server runs several at once:
//! expert begins *Starting engine node* and *Preparing engine* in the same
//! breath. M8.5 kept one slot, and the first `end` emptied it while the
//! other task still ran, so the bar went quiet as though the server were
//! ready.
//!
//! Two servers say more than the protocol does, and both are read here as
//! **details**, not tasks. kotlin-lsp reports Gradle's output in
//! `intellij/importLog`, and jdtls reports how far it has started in
//! `language/status`. Neither outlives the work: when the last task ends,
//! what they said ends with it.

use serde_json::Value;

/// Everything a server is doing now.
#[derive(Debug, Default)]
pub struct Work {
    /// In the order they began; the last is the one in front.
    tasks: Vec<(Token, Task)>,
    /// The last line of Gradle's output that said something.
    import_line: Option<String>,
    /// jdtls's *Starting …*, while it is starting.
    starting: Option<String>,
}

/// A task's token as the protocol has it: `integer | string`, and `7` and
/// `"7"` are two tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(i64),
    Text(String),
}

impl Token {
    fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Number(n) => n.as_i64().map(Token::Number),
            Value::String(s) => Some(Token::Text(s.clone())),
            _ => None,
        }
    }
}

/// One task, as the server last described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// What it is: *Indexing*, *Importing*.
    pub title: String,
    /// Where it has got to: *Loading packages…*.
    pub message: Option<String>,
    /// How far, from nought to a hundred, when the server knows.
    pub percentage: Option<u32>,
}

/// What the status bar says: the task in front, and how many are behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The task's title.
    pub title: String,
    /// Its message, or the import log's last line when it has none.
    pub detail: Option<String>,
    /// How far it has got.
    pub percentage: Option<u32>,
    /// How many other tasks are running.
    pub more: usize,
}

impl Work {
    /// Take in a notification. Answers whether anything the window draws
    /// has changed.
    pub fn absorb(&mut self, method: &str, params: &Value) -> bool {
        match method {
            "$/progress" => self.progress(params),
            "intellij/importLog" => {
                let Some(line) = params
                    .get("message")
                    .and_then(Value::as_str)
                    .and_then(last_line)
                else {
                    return false;
                };
                let changed = self.import_line.as_deref() != Some(line);
                self.import_line = Some(line.to_owned());
                changed
            }
            "language/status" => {
                let kind = params.get("type").and_then(Value::as_str);
                let was = self.starting.clone();
                match kind {
                    Some("Starting") => {
                        self.starting = params
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                    }
                    Some("Started" | "ServiceReady" | "Error") => self.starting = None,
                    _ => {}
                }
                self.starting != was
            }
            _ => false,
        }
    }

    fn progress(&mut self, params: &Value) -> bool {
        let Some(token) = params.get("token").and_then(Token::of) else {
            return false;
        };
        let Some(value) = params.get("value") else {
            return false;
        };
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned);
        // A server past a hundred is a server's mistake, not a bar drawn
        // past its end.
        let percentage = value
            .get("percentage")
            .and_then(Value::as_u64)
            .map(|p| p.min(100) as u32);
        let at = self.tasks.iter().position(|(t, _)| *t == token);
        match value.get("kind").and_then(Value::as_str) {
            Some("begin") => {
                let task = Task {
                    title: value
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    message,
                    percentage,
                };
                if let Some(at) = at {
                    self.tasks.remove(at);
                }
                self.tasks.push((token, task));
                true
            }
            Some("report") => {
                let Some(at) = at else {
                    return false;
                };
                let task = &mut self.tasks[at].1;
                let before = task.clone();
                if message.is_some() {
                    task.message = message;
                }
                if percentage.is_some() {
                    task.percentage = percentage;
                }
                *task != before
            }
            Some("end") => {
                let Some(at) = at else {
                    return false;
                };
                self.tasks.remove(at);
                if self.tasks.is_empty() {
                    self.import_line = None;
                    self.starting = None;
                }
                true
            }
            _ => false,
        }
    }

    /// What to say about it, or nothing when the server is quiet.
    pub fn headline(&self) -> Option<Line> {
        match self.tasks.last() {
            Some((_, task)) => Some(Line {
                title: task.title.clone(),
                detail: task.message.clone().or_else(|| self.import_line.clone()),
                percentage: task.percentage,
                more: self.tasks.len() - 1,
            }),
            None => self.starting.as_ref().map(|starting| Line {
                title: starting.clone(),
                detail: None,
                percentage: None,
                more: 0,
            }),
        }
    }

    /// Every task, newest first, for the list.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().rev().map(|(_, task)| task)
    }

    /// Whether there is nothing to say.
    pub fn is_idle(&self) -> bool {
        self.tasks.is_empty() && self.starting.is_none()
    }
}

/// The last line of a chunk of output that says something.
fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).rfind(|line| !line.is_empty())
}

/// Why a language that names a server has none running.
///
/// Only a language whose row names a server can have one. A language
/// with no server says nothing, as M8.5 decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The command would not start: nothing by that name is on the
    /// login shell's `PATH`.
    Missing,
    /// It died three times, and the window stopped starting it.
    Dying,
    /// It answered `initialize` with an error, in its own words.
    Refused(String),
    /// It was not started: the project's root holds none of the files
    /// the row says it needs, named here.
    Unprepared(&'static [&'static str]),
}
