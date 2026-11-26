//! What `git branch` says, in a marked format, parsed — never asked for
//! here. Decision 6 of M8.3.1, and since M8.3.3 the format carries the
//! tip as well:
//! `--format='%(refname:short)%00%(HEAD)%00%(authorname)%00%(committerdate:relative)%00%(contents:subject)'`
//! — the name, `*` for the branch `HEAD` is on, and who wrote the tip,
//! how long ago and what they called it, one line each.
//!
//! **The tip is free.** Task 0 of M8.3.3 measured 8.4 ms against 8.1
//! for the name and the mark alone: the cost is the process, not the
//! fields. So there is one format, and the Log's column ignores what it
//! does not draw.
//!
//! **`%00`, not `%x00`.** `git branch` speaks ref-filter's format, where
//! a byte is `%00`; the stash list speaks the log's, where the same byte
//! is `%x00`. Neither accepts the other's spelling, and the wrong one
//! arrives as four visible characters rather than an error.

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// `HEAD` is on it.
    pub current: bool,
    /// Who wrote the commit the branch points at.
    pub author: String,
    /// How long ago they wrote it, as git says it: *3 hours ago*.
    pub age: String,
    /// Its first line.
    pub subject: String,
}

/// Bytes that were not the format asked for.
#[derive(Debug, thiserror::Error)]
#[error("not the branch format at line {line}: {reason}")]
pub struct ParseError {
    /// Which line, counting from one.
    pub line: usize,
    pub reason: &'static str,
}

/// The branches, in git's order — alphabetical, which is what its
/// `--sort` defaults to.
pub fn parse(bytes: &[u8]) -> Result<Vec<Branch>, ParseError> {
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
            let name = fields.next().unwrap_or_default();
            let mark = fields.next().ok_or(ParseError {
                line: line_number,
                reason: "a line without the NUL between the name and the mark",
            })?;
            if name.is_empty() {
                return Err(ParseError {
                    line: line_number,
                    reason: "a branch with no name",
                });
            }
            // The tip's three fields, absent in a file captured before
            // M8.3.3 asked for them: an older capture still parses, and
            // its branches simply have nothing to say about their tip.
            let author = fields.next().unwrap_or_default();
            let age = fields.next().unwrap_or_default();
            // The subject is the rest, NULs and all: a commit message is
            // a person's own words and nothing may be assumed about them.
            let subject: Vec<&str> = fields.collect();
            Ok(Branch {
                name: name.to_owned(),
                current: mark.trim() == "*",
                author: author.to_owned(),
                age: age.to_owned(),
                subject: subject.join("\0"),
            })
        })
        .collect()
}
