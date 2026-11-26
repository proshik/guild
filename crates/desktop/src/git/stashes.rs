//! What `git stash list` says, in a marked format, parsed — never asked
//! for here. `--format='%gd%x00%H%x00%ar%x00%gs'`: the reference, a NUL,
//! the entry's commit, a NUL, how long ago, a NUL, and what the entry is
//! called, one line each.
//!
//! **The commit since M5.11.** A reference is a position and the list
//! shifts under it — a stash pushed from a terminal moves every entry
//! down one — while `git stash drop` takes only a position. So an entry
//! is carried by its commit, and [`reference_of`] finds the position that
//! commit has at the moment of the drop. Decision 17 of that plan.
//!
//! **`%x00`, not `%00`.** The stash list speaks the *log* format, where
//! a byte is `%x00`; `git branch` speaks ref-filter's, where the same
//! byte is `%00`. Written the other way round the escape arrives as the
//! four characters `%00` in the middle of the line and the fields never
//! separate — which is how this was found.

/// One entry of the stash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    /// `stash@{0}` and its neighbours — git's own reference, which is
    /// what any command about this entry is given. Never a person's
    /// typing, and never an index we counted ourselves: the list shifts
    /// under a drop.
    pub reference: String,
    /// The entry's own commit, forty hex digits: what the entry *is*,
    /// wherever the list has moved it.
    pub sha: String,
    /// *2 hours ago*, as git says it.
    pub age: String,
    /// *On main: what it was called*, or git's own *WIP on …* when the
    /// person gave it no name.
    pub subject: String,
}

/// Bytes that were not the format asked for.
#[derive(Debug, thiserror::Error)]
#[error("not the stash format at line {line}: {reason}")]
pub struct ParseError {
    /// Which line, counting from one.
    pub line: usize,
    pub reason: &'static str,
}

/// The entries, newest first, which is the order `git stash list` gives
/// and the order the popup shows.
pub fn parse(bytes: &[u8]) -> Result<Vec<Stash>, ParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError {
        line: 1,
        reason: "not UTF-8",
    })?;
    text.lines()
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(index, line)| {
            let line_number = index + 1;
            let mut fields = line.split('\0');
            let reference = fields.next().unwrap_or_default();
            let sha = fields.next().ok_or(ParseError {
                line: line_number,
                reason: "a line without the NUL before the commit",
            })?;
            let age = fields.next().ok_or(ParseError {
                line: line_number,
                reason: "a line without the NUL before the age",
            })?;
            // The subject is the rest, NULs and all: it is a person's
            // own words and nothing may be assumed about them.
            let subject: Vec<&str> = fields.collect();
            if subject.is_empty() {
                return Err(ParseError {
                    line: line_number,
                    reason: "a line without the NUL before the subject",
                });
            }
            if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ParseError {
                    line: line_number,
                    reason: "an entry whose second field is not a commit",
                });
            }
            if reference.is_empty() {
                return Err(ParseError {
                    line: line_number,
                    reason: "an entry with no reference",
                });
            }
            Ok(Stash {
                reference: reference.to_owned(),
                sha: sha.to_owned(),
                age: age.to_owned(),
                subject: subject.join("\0"),
            })
        })
        .collect()
}

/// The reference the entry with this commit has in `stashes`, or nothing
/// when no entry is that commit any more — dropped, popped or cleared by
/// another hand since the list was read.
pub fn reference_of<'a>(stashes: &'a [Stash], sha: &str) -> Option<&'a str> {
    stashes
        .iter()
        .find(|stash| stash.sha == sha)
        .map(|stash| stash.reference.as_str())
}
