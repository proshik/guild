//! What `git log` says, in the marked format, and what `git show
//! --name-status` says of one commit — parsed, never asked for here.
//!
//! The format: `%H%x00%P%x00%an%x00%ae%x00%at%x00%s%x00%b%x00%x01`
//! under `-z`, a NUL between fields and `\x01` after each commit, so a
//! body with newlines, quotes and `#` lines arrives as it was written
//! and nothing has to be unescaped. Decision 8 of M8.3.

use std::path::PathBuf;

use crate::theme::Mark;

/// One commit of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// The full hash; the row shows the first seven.
    pub sha: String,
    /// One for most commits, two for a merge, none for the first.
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// The author's time, seconds since the epoch.
    pub time: i64,
    /// The branches and tags pointing at this commit, as names.
    ///
    /// git prints `HEAD -> main` for the branch `HEAD` is on, a bare
    /// name for any other, and `tag: v1.0` for a tag. The arrow and the
    /// word are git's punctuation; a row shows names, so they are taken
    /// off here and not in the view.
    pub refs: Vec<String>,
    /// The first line of the message.
    pub summary: String,
    /// The rest, without the blank line that separates it — empty for a
    /// one-line message.
    pub body: String,
}

impl Commit {
    /// The first seven of the hash, git's own abbreviation here.
    pub fn short(&self) -> &str {
        let end = self
            .sha
            .char_indices()
            .nth(7)
            .map_or(self.sha.len(), |(i, _)| i);
        &self.sha[..end]
    }
}

/// One file of a commit, with the mark the tree would draw for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// The path after the commit.
    pub path: PathBuf,
    pub mark: Mark,
    /// A rename's path before the commit.
    pub from: Option<PathBuf>,
}

/// Bytes that were not the format asked for.
#[derive(Debug, thiserror::Error)]
#[error("not the log format at commit {commit}: {reason}")]
pub struct ParseError {
    /// Which commit, counting from one.
    pub commit: usize,
    pub reason: &'static str,
}

/// The number of fields before the record mark.
///
/// Eight since M8.3.4, when `%D` joined the format. A capture in the old
/// shape is an **error** rather than something to take as far as it goes:
/// a parser that shifted would read the summary as the refs and the body
/// as the summary, and every row in the window would be quietly wrong.
const FIELDS: usize = 8;

/// The commits, in the order git printed them — newest first.
pub fn parse(bytes: &[u8]) -> Result<Vec<Commit>, ParseError> {
    let mut out = Vec::new();
    for (index, record) in bytes.split(|b| *b == 1).enumerate() {
        // `-z` puts a NUL after the record mark, before the next commit.
        let record = strip_nuls(record);
        if record.is_empty() {
            continue;
        }
        let number = index + 1;
        let fail = |reason| ParseError {
            commit: number,
            reason,
        };
        let text = std::str::from_utf8(record).map_err(|_| fail("not UTF-8"))?;
        let fields: Vec<&str> = text.splitn(FIELDS + 1, '\0').collect();
        // **Exactly, not at least.** The format ends `%b%x00%x01`, so a
        // record carries a trailing NUL and a correct one splits into
        // `FIELDS + 1` parts, the last empty. A capture with one field
        // fewer splits into `FIELDS` and destructures happily — reading
        // the summary as the refs and the body as the summary, every row
        // in the window quietly wrong. Counting is what catches it.
        if fields.len() != FIELDS + 1 {
            return Err(fail("a commit has eight fields"));
        }
        let [sha, parents, author, email, time, refs, summary, body, ..] = fields[..] else {
            return Err(fail("a commit has eight fields"));
        };
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(fail("the hash is not forty hex digits"));
        }
        out.push(Commit {
            sha: sha.to_owned(),
            parents: parents.split_whitespace().map(str::to_owned).collect(),
            author: author.to_owned(),
            email: email.to_owned(),
            time: time.parse().map_err(|_| fail("the time is not a number"))?,
            refs: names_of(refs),
            summary: summary.to_owned(),
            body: body.trim_end_matches('\n').to_owned(),
        });
    }
    Ok(out)
}

/// The refs of a row, in the room a row can spare.
///
/// **They are the least of what a row says and they were taking whatever
/// they liked**: two long branch names covered the message entirely.
/// The rule: several names join with `&`, and a name
/// too long for the room gives up its leading path segments before its
/// tail, because `fix/` says nothing that `e2e-web-qa-delegation` does
/// not.
///
/// `budget` is in characters rather than pixels: this is a string, the
/// width belongs to the view, and a test can ask about a string.
pub fn refs_label(names: &[String], budget: usize) -> String {
    let joined = names.join(" & ");
    if joined.chars().count() <= budget {
        return joined;
    }

    // One name may be a path. Drop a segment at a time, and say so with
    // `..`, which is what a person reads as *there was more in front*.
    let mut shortened = joined;
    if names.len() == 1 {
        while shortened.chars().count() > budget {
            let Some(slash) = shortened.find('/') else {
                break;
            };
            let rest = &shortened[slash..];
            let next = format!("..{rest}");
            if next.chars().count() >= shortened.chars().count() {
                break;
            }
            shortened = next;
        }
        if shortened.chars().count() <= budget {
            return shortened;
        }
    }

    let keep = budget.saturating_sub(1);
    let cut: String = shortened.chars().take(keep).collect();
    format!("{cut}\u{2026}")
}

/// What `%D` printed, as the names a row shows.
///
/// `HEAD -> main, origin/main, tag: v1.0` is four words of punctuation
/// around three names. An empty field is no refs at all rather than one
/// empty name — which is almost every commit.
fn names_of(decorations: &str) -> Vec<String> {
    decorations
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            let name = name.rsplit("-> ").next().unwrap_or(name);
            name.strip_prefix("tag: ").unwrap_or(name).to_owned()
        })
        .filter(|name| name != "HEAD")
        .collect()
}

/// The NUL `-z` puts after the record mark belongs to the commit before,
/// and lands at the start of the next chunk; the NUL at the end of a
/// chunk is the body's own terminator and stays, or an empty body would
/// be a missing field.
fn strip_nuls(record: &[u8]) -> &[u8] {
    let start = record.iter().position(|b| *b != 0).unwrap_or(record.len());
    &record[start..]
}

/// One commit's files from `git show --name-status -z --format= <sha>`:
/// a status letter, the path, and for a rename or a copy the path before
/// it as a record of its own — the same shape as porcelain's rename.
pub fn files_of(bytes: &[u8]) -> Result<Vec<FileChange>, ParseError> {
    let mut out = Vec::new();
    let mut records = bytes
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .enumerate()
        .peekable();
    while let Some((index, status)) = records.next() {
        let number = index + 1;
        let fail = |reason| ParseError {
            commit: number,
            reason,
        };
        let status = std::str::from_utf8(status).map_err(|_| fail("not UTF-8"))?;
        let (_, path) = records.next().ok_or(fail("a status without its path"))?;
        let path = PathBuf::from(std::str::from_utf8(path).map_err(|_| fail("not UTF-8"))?);
        let letter = status.chars().next().ok_or(fail("an empty status"))?;
        let (mark, from) = match letter {
            'M' | 'T' => (Mark::Modified, None),
            'A' => (Mark::Added, None),
            'D' => (Mark::Deleted, None),
            'R' | 'C' => {
                let (_, to) = records
                    .next()
                    .ok_or(fail("a rename without its new path"))?;
                let to = PathBuf::from(std::str::from_utf8(to).map_err(|_| fail("not UTF-8"))?);
                out.push(FileChange {
                    path: to,
                    mark: if letter == 'R' {
                        Mark::Modified
                    } else {
                        Mark::Added
                    },
                    from: Some(path),
                });
                continue;
            }
            _ => return Err(fail("an unknown status letter")),
        };
        out.push(FileChange { path, mark, from });
    }
    Ok(out)
}
