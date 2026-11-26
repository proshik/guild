//! What `git blame --porcelain` says, parsed — never asked for here.
//!
//! The porcelain gives every line a header — the hash, the line in the
//! commit, the line in the file, and for the first line of a group how
//! many follow — and, the first time a commit appears, its author, time
//! and summary as key-value lines; the line's text follows a tab. A
//! commit seen again carries only the header, so the parser keeps what
//! it learned. Decision 10 of M8.3.

use std::collections::HashMap;
use std::path::PathBuf;

/// Lines in a row that one commit wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The full hash; forty zeros for a line not committed yet.
    pub sha: String,
    pub author: String,
    /// The author's time, seconds since the epoch.
    pub time: i64,
    pub summary: String,
    /// The first of the run's lines in the file, one-based as git counts.
    pub first_line: usize,
    pub original_line: usize,
    pub filename: PathBuf,
    /// The lines' text, without their newlines.
    pub lines: Vec<String>,
}

impl Run {
    /// A line in the working tree that no commit has: git names it with
    /// the zero hash and a placeholder author.
    pub fn is_uncommitted(&self) -> bool {
        self.sha.bytes().all(|b| b == b'0')
    }

    /// The first seven of the hash.
    pub fn short(&self) -> &str {
        let end = self
            .sha
            .char_indices()
            .nth(7)
            .map_or(self.sha.len(), |(i, _)| i);
        &self.sha[..end]
    }
}

/// Bytes that were not porcelain blame.
#[derive(Debug, thiserror::Error)]
#[error("not porcelain blame at line {line}: {reason}")]
pub struct ParseError {
    /// Which line of the output, counting from one.
    pub line: usize,
    pub reason: &'static str,
}

/// What a commit's header lines said, kept for its later lines.
#[derive(Default, Clone)]
struct Who {
    author: String,
    time: i64,
    summary: String,
    filename: PathBuf,
}

/// The runs, in the file's order.
pub fn parse(bytes: &[u8]) -> Result<Vec<Run>, ParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError {
        line: 1,
        reason: "not UTF-8",
    })?;
    let mut runs: Vec<Run> = Vec::new();
    let mut known: HashMap<String, Who> = HashMap::new();
    let mut lines = text.split_inclusive('\n').enumerate().peekable();
    while let Some((index, header)) = lines.next() {
        let number = index + 1;
        let fail = |reason| ParseError {
            line: number,
            reason,
        };
        let header = header.trim_end_matches('\n');
        if header.is_empty() {
            continue;
        }
        let mut fields = header.split(' ');
        let sha = fields.next().ok_or(fail("no hash"))?;
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(fail("the hash is not forty hex digits"));
        }
        let original: usize = fields
            .next()
            .and_then(|f| f.parse().ok())
            .ok_or(fail("no line in the commit"))?;
        let final_line: usize = fields
            .next()
            .and_then(|f| f.parse().ok())
            .ok_or(fail("no line in the file"))?;
        if original == 0 || final_line == 0 {
            return Err(fail("line numbers must be positive"));
        }
        // The key-value lines up to the tab-led text.
        let mut who = known.get(sha).cloned().unwrap_or_default();
        let mut filename = who.filename.clone();
        let mut content = None;
        for (_, line) in lines.by_ref() {
            if let Some(rest) = line.strip_prefix('\t') {
                content = Some(rest.trim_end_matches('\n').to_owned());
                break;
            }
            let line = line.trim_end_matches('\n');
            if let Some(author) = line.strip_prefix("author ") {
                who.author = author.to_owned();
            } else if let Some(time) = line.strip_prefix("author-time ") {
                who.time = time.parse().map_err(|_| fail("the time is not a number"))?;
            } else if let Some(summary) = line.strip_prefix("summary ") {
                who.summary = summary.to_owned();
            } else if let Some(path) = line.strip_prefix("filename ") {
                filename = decode_filename(path).ok_or(fail("invalid quoted filename"))?;
            }
        }
        let content = content.ok_or(fail("a header without its line"))?;
        who.filename = filename.clone();
        known.insert(sha.to_owned(), who.clone());
        match runs.last_mut() {
            Some(run)
                if run.sha == sha
                    && run.first_line + run.lines.len() == final_line
                    && run.original_line + run.lines.len() == original
                    && run.filename == filename =>
            {
                run.lines.push(content);
            }
            _ => runs.push(Run {
                sha: sha.to_owned(),
                author: who.author,
                time: who.time,
                summary: who.summary,
                first_line: final_line,
                original_line: original,
                filename,
                lines: vec![content],
            }),
        }
    }
    Ok(runs)
}

fn decode_filename(raw: &str) -> Option<PathBuf> {
    if !raw.starts_with('"') {
        return Some(raw.into());
    }
    let bytes = raw.strip_prefix('"')?.strip_suffix('"')?.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let ch = bytes[at];
        at += 1;
        if ch != b'\\' {
            result.push(ch);
            continue;
        }
        let ch = *bytes.get(at)?;
        at += 1;
        if (b'0'..=b'7').contains(&ch) {
            let mut value = u16::from(ch - b'0');
            for _ in 0..2 {
                if let Some(ch) = bytes.get(at).filter(|c| (b'0'..=b'7').contains(c)) {
                    value = value * 8 + u16::from(*ch - b'0');
                    at += 1;
                } else {
                    break;
                }
            }
            result.push(u8::try_from(value).ok()?);
        } else {
            result.push(match ch {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'b' => 8,
                b'f' => 12,
                b'v' => 11,
                b'a' => 7,
                b'\\' | b'"' => ch,
                _ => return None,
            });
        }
    }
    Some(String::from_utf8(result).ok()?.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    pub filename: PathBuf,
    pub original_line: usize,
}
impl Line {
    pub fn date(&self) -> String {
        time::OffsetDateTime::from_unix_timestamp(self.time)
            .map(|t| format!("{:02}.{:02}.{}", t.day(), t.month() as u8, t.year()))
            .unwrap_or_else(|_| "?".into())
    }
    pub fn compact(&self) -> String {
        format!(
            "{} {}",
            self.date(),
            self.author.split_whitespace().last().unwrap_or("?")
        )
    }
    pub fn tooltip(&self) -> String {
        format!(
            "{} — {}\n{}\n{}\n{}:{}",
            self.author,
            self.date(),
            self.sha,
            self.summary,
            self.filename.display(),
            self.original_line
        )
    }
}
/// Only equal diff lines retain commit attribution. Changed/inserted lines are
/// immediately non-navigable, including zero hashes from the working tree.
pub fn map(runs: &[Run], buffer: &str) -> Vec<Option<Line>> {
    use similar::{Algorithm, ChangeTag, TextDiff};
    let mut old = String::new();
    let mut lines = Vec::new();
    for run in runs {
        for (i, text) in run.lines.iter().enumerate() {
            // FileTab normalizes CRLF through Text::of; preserve row indexes
            // while comparing the same logical text as the retained editor.
            old.push_str(text.strip_suffix('\r').unwrap_or(text));
            old.push('\n');
            lines.push((!run.is_uncommitted()).then(|| Line {
                sha: run.sha.clone(),
                author: run.author.clone(),
                time: run.time,
                summary: run.summary.clone(),
                filename: run.filename.clone(),
                original_line: run.original_line + i,
            }));
        }
    }
    // Porcelain removes line endings. Normalize the final newline on both
    // sides so a file without one retains its final line's attribution.
    let current = if buffer.is_empty() || buffer.ends_with('\n') {
        buffer.to_owned()
    } else {
        format!("{buffer}\n")
    };
    let count = buffer.split('\n').count();
    let mut mapped = vec![None; count];
    if old.len() as u64 > crate::diff::DIFF_LIMIT || current.len() as u64 > crate::diff::DIFF_LIMIT
    {
        return mapped;
    }
    for change in TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_lines(&old, &current)
        .iter_all_changes()
    {
        if change.tag() == ChangeTag::Equal
            && let (Some(before), Some(after)) = (change.old_index(), change.new_index())
            && let Some(slot) = mapped.get_mut(after)
        {
            *slot = lines.get(before).cloned().flatten();
        }
    }
    mapped
}
