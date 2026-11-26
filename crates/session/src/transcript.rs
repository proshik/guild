//! The transcripts `claude` keeps on its own.
//!
//! Every session is a `<session-id>.jsonl` file in a per-project directory
//! under `~/.claude/projects`. We need one fact from it: **is there a
//! conversation to come back to**.
//!
//! The presence of the file is not enough, and that cost us a dead tab.
//! `claude` creates the file as soon as a session starts and puts service
//! records into it — `ai-title`, `agent-name`. There is no conversation
//! yet: `--resume` with such an id answers `No conversation found with
//! session ID` and exits. A conversation begins where `user` and
//! `assistant` records appear.
//!
//! The id is not searched for here. Guild assigns it with `--session-id`,
//! and that is the only reliable way: in one directory every session
//! writes at once — one started from a terminal, an agent's, our tab — and
//! "the newest transcript" turns out to be someone else's as often as
//! ours.

use std::io::BufRead as _;
use std::path::Path;

/// Is there a conversation to come back to?
///
/// `false` means either there was no conversation, or there is no file, or
/// the file holds nothing but service records. All three mean the same:
/// `--resume` will return an error.
pub fn has_conversation(projects_dir: &Path, session_id: &str) -> bool {
    let Some(path) = path(projects_dir, session_id) else {
        return false;
    };
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };

    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
        .any(|record| {
            matches!(
                record.get("type").and_then(serde_json::Value::as_str),
                Some("user" | "assistant")
            )
        })
}

/// Is the identifier already taken?
///
/// If the file exists, `claude` has seen this id before, and
/// `--session-id` answers `Error: Session ID … is already in use`.
/// Together with [`has_conversation`] this gives three distinguishable
/// cases rather than two: see [`crate::session::Continuation`].
pub fn is_id_taken(projects_dir: &Path, session_id: &str) -> bool {
    path(projects_dir, session_id).is_some()
}

/// Where the transcript file is.
///
/// `claude` promises its directory naming scheme nowhere, so we **do not
/// compute** it: we look for the file across all directories at once. A
/// computed path would drift from the real one silently — returning "no
/// session" instead of an error.
pub fn path(projects_dir: &Path, session_id: &str) -> Option<std::path::PathBuf> {
    let file_name = format!("{session_id}.jsonl");

    std::fs::read_dir(projects_dir)
        .ok()?
        .flatten()
        .find_map(|project| {
            let candidate = project.path().join(&file_name);
            candidate.exists().then_some(candidate)
        })
}

/// What a session has spent.
///
/// Tokens are the reliable number: `message.usage` is on every assistant
/// record in every transcript. Money is the lucky one — `claude` writes a
/// `cost-state` record carrying `totalCostUSD` in about one transcript in
/// thirty-five.
///
/// We do not price the tokens ourselves. That would mean a table of model
/// prices, keyed by names we do not control and changed by someone else,
/// and a stale table shows a confident wrong number. Money that is
/// silently wrong is worse than money that is absent.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    /// Fresh input tokens.
    pub input_tokens: u64,
    /// Generated tokens.
    pub output_tokens: u64,
    /// Tokens served from the cache.
    pub cache_read_tokens: u64,
    /// Tokens written into the cache.
    pub cache_creation_tokens: u64,
    /// What it cost, when the transcript says so.
    ///
    /// `None` is not zero. Zero says the session was free.
    pub cost_usd: Option<f64>,
}

impl Usage {
    /// Everything that passed through, in tokens.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_creation_tokens)
    }

    /// Fold in what was read later.
    ///
    /// Tokens add up; cost replaces. `totalCostUSD` is what the session
    /// has cost so far rather than what a turn cost, so adding it the way
    /// tokens are added would bill the person once per reading.
    pub fn absorb(&mut self, later: Usage) {
        self.input_tokens = self.input_tokens.saturating_add(later.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(later.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(later.cache_read_tokens);
        self.cache_creation_tokens = self
            .cache_creation_tokens
            .saturating_add(later.cache_creation_tokens);
        if later.cost_usd.is_some() {
            self.cost_usd = later.cost_usd;
        }
    }
}

/// What the transcript gained after `offset`, and where reading stopped.
///
/// Read from an offset because a transcript only ever grows and reaches
/// megabytes: re-reading it whole on every poll is work nobody asked for.
///
/// Reading stops at the last complete line. `claude` is writing while we
/// read, so the tail is regularly half a record — parsing it would drop a
/// turn, and counting its bytes would skip that turn for good.
pub fn usage_since(path: &Path, offset: u64) -> Option<(Usage, u64)> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();

    // Shorter than where we stopped means the file was replaced or
    // truncated, and the offset means nothing now. Start again rather than
    // seek past the end.
    let from = if offset > length { 0 } else { offset };

    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    // Lossy on purpose: one malformed byte must not cost the whole tail.
    let text = String::from_utf8_lossy(&bytes);

    let Some(last) = text.rfind('\n') else {
        return Some((Usage::default(), from));
    };
    let complete = &text[..=last];

    let mut usage = Usage::default();
    for line in complete.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        match record.get("type").and_then(serde_json::Value::as_str) {
            Some("assistant") => {
                // The top-level counts only. `usage` also carries an
                // `iterations` array repeating them, and summing both
                // doubles every turn.
                let Some(raw) = record.pointer("/message/usage") else {
                    continue;
                };
                let count = |key: &str| {
                    raw.get(key)
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                };
                usage.input_tokens = usage.input_tokens.saturating_add(count("input_tokens"));
                usage.output_tokens = usage.output_tokens.saturating_add(count("output_tokens"));
                usage.cache_read_tokens = usage
                    .cache_read_tokens
                    .saturating_add(count("cache_read_input_tokens"));
                usage.cache_creation_tokens = usage
                    .cache_creation_tokens
                    .saturating_add(count("cache_creation_input_tokens"));
            }
            Some("cost-state") => {
                // A running total: the last one wins rather than being
                // added to the one before it.
                if let Some(total) = record
                    .get("totalCostUSD")
                    .and_then(serde_json::Value::as_f64)
                {
                    usage.cost_usd = Some(total);
                }
            }
            _ => {}
        }
    }

    Some((usage, from + complete.len() as u64))
}

/// What this conversation is about, according to its own transcript.
///
/// **Not the terminal's title.** `claude` writes an OSC title, and it is
/// not this session's: a tab whose transcript has no title at all and whose
/// first words are "что за проект" was shown as "Rust uninstall workflow",
/// which is a different conversation entirely. That is the same shape as
/// the trap M5 already paid for — "the newest transcript in this directory"
/// was somebody else's — and the answer is the same one: the file named by
/// **this** session's id is the only thing that speaks for it.
///
/// `/rename`'s `customTitle`, else `ai-title`'s `aiTitle` — the last of each
/// — else the first thing the person said.
pub fn name(path: &Path) -> Option<String> {
    use std::io::BufRead as _;

    let file = std::fs::File::open(path).ok()?;
    let mut titles = Titles::default();
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        if let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) {
            titles.take(&record);
        }
    }
    titles.best()
}

/// What a transcript says it is called, as its records go by — kept
/// apart from the reading so the history reader, which reads a file's
/// head and tail and not its middle, ranks them by the same rule.
#[derive(Debug, Default)]
pub(crate) struct Titles {
    /// The person's `/rename`, the last one.
    pub(crate) custom: Option<String>,
    /// `claude`'s own, the last one: it rewrites it as it goes.
    pub(crate) ai: Option<String>,
    /// The first thing the person said.
    pub(crate) first_words: Option<String>,
}

impl Titles {
    pub(crate) fn take(&mut self, record: &serde_json::Value) {
        let text = |key: &str| {
            record
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        match record.get("type").and_then(serde_json::Value::as_str) {
            Some("custom-title") => self.custom = text("customTitle").or(self.custom.take()),
            Some("ai-title") => self.ai = text("aiTitle").or(self.ai.take()),
            Some("user") if self.first_words.is_none() && !is_meta(record) => {
                self.first_words = said(record);
            }
            _ => {}
        }
    }

    /// The person's name, else `claude`'s, else their first words.
    pub(crate) fn best(self) -> Option<String> {
        self.custom.or(self.ai).or(self.first_words)
    }
}

/// A `user` record `claude` wrote itself — a command's caveat, a hook's
/// output — rather than one a person typed.
pub(crate) fn is_meta(record: &serde_json::Value) -> bool {
    record.get("isMeta").and_then(serde_json::Value::as_bool) == Some(true)
}

/// The words in a `user` record, if it holds any.
///
/// The content is a string in the simple case and a list of parts when
/// there are attachments or tool results. Only text is wanted: a tool
/// result as a tab's name says nothing about the conversation.
pub(crate) fn said(record: &serde_json::Value) -> Option<String> {
    let content = record.pointer("/message/content")?;

    let text = match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .find(|part| part.get("type").and_then(serde_json::Value::as_str) == Some("text"))
            .and_then(|part| part.get("text").and_then(serde_json::Value::as_str))?
            .to_owned(),
        _ => return None,
    };
    if let Some(command) = slash_command(&text) {
        return Some(command);
    }

    // The first line only. A pasted stack trace is not a name.
    let first = text.lines().find(|line| !line.trim().is_empty())?.trim();
    (!first.is_empty()).then(|| first.to_owned())
}

/// A slash command as it was typed — `/fix-review`, and its arguments —
/// where `text` is the markup Claude Code records one as, or `None`.
///
/// Recorded as `<command-message>fix-review</command-message>` then
/// `<command-name>/fix-review</command-name>` and, when it was given any,
/// `<command-args>…</command-args>` (measured on 2026-09-28). **The
/// message tag alone is enough**: a session named before this rule kept
/// only the markup's first line, and the window reads that kept name
/// through here too.
pub fn slash_command(text: &str) -> Option<String> {
    let inside = |tag: &str| {
        let open = format!("<{tag}>");
        let start = text.find(&open)? + open.len();
        let end = text[start..].find(&format!("</{tag}>"))? + start;
        let words = text[start..end].trim();
        (!words.is_empty()).then_some(words)
    };
    let name = match inside("command-name") {
        Some(name) => name.to_owned(),
        None => format!("/{}", inside("command-message")?.trim_start_matches('/')),
    };
    // The first line of what it was asked: a pasted stack trace is not a
    // name here either.
    Some(
        match inside("command-args").and_then(|args| args.lines().next()) {
            Some(args) => format!("{name} {}", args.trim()),
            None => name,
        },
    )
}

/// Where `claude` keeps its session transcripts — M13 moved the rule here.
///
/// Its directory, not ours: it writes the files, we only read them. **One
/// rule for both binaries**: the window reads a transcript to learn whether
/// a session has a conversation, and `guild agent read` reads the same file
/// for what the agent said, and a second copy of where that file lives is a
/// second answer waiting to drift.
pub fn projects_dir() -> std::path::PathBuf {
    if let Some(config) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(config).join("projects");
    }
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => std::path::PathBuf::from(home)
            .join(".claude")
            .join("projects"),
        _ => std::path::PathBuf::from(".claude").join("projects"),
    }
}

/// The last `count` things the agent said in a transcript, oldest first —
/// M13, for `guild agent read`.
///
/// **Only what the agent wrote as text.** A reply is a list of parts: the
/// words, and the tool calls between them. What a caller waiting on a task
/// wants back is the words — the report — and a tool call's JSON is how it
/// got there, not what it found.
pub fn replies(path: &Path, count: usize) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => replies_in(&text, count),
        Err(_) => Vec::new(),
    }
}

/// [`replies`], over a transcript's text — pure, so a test is a string.
pub fn replies_in(transcript: &str, count: usize) -> Vec<String> {
    let mut found: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    for line in transcript.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record.get("type").and_then(serde_json::Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(text) = spoken(&record) else {
            continue;
        };
        found.push_back(text);
        if found.len() > count {
            found.pop_front();
        }
    }
    found.into()
}

/// Every text part of a record, whole and in order — unlike [`said`], which
/// wants a name and takes one line of one part.
fn spoken(record: &serde_json::Value) -> Option<String> {
    let text = match record.pointer("/message/content")? {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter(|part| part.get("type").and_then(serde_json::Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}
