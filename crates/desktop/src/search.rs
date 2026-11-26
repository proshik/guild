//! Searching a project for text — M8.7.
//!
//! The engine is ripgrep's own: `ignore` walks, which this crate has
//! depended on since M8.1 for the file tree's `.gitignore` rules, and
//! `grep-regex` and `grep-searcher` match. Decision 1 of the plan says
//! why it is neither `rg` on the `PATH` nor `git grep`: a binary that may
//! not be installed, against a tool that searches **tracked files** and
//! would therefore be blind to the file an agent has just written — which
//! is the file a person is looking for.
//!
//! **Nothing here writes anything.** Decision 2. It reads a tree and
//! answers with places.
//!
//! This module knows nothing about the window: it takes a root, a query
//! and a flag that says stop, and hands hits to a closure a file at a
//! time. `root::search` is what runs it on a thread.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};

/// How many hits one file may contribute.
///
/// Task 0 measured a common word at 16,765 hits in this repository and
/// 503,629 in a tree with no ignore file. Nobody reads them and nothing
/// should build the rows: one file cannot fill the panel by itself, and
/// the whole search stops at [`TOTAL`].
pub const PER_FILE: usize = 100;

/// How many hits a search may find before it stops looking.
pub const TOTAL: usize = 5_000;

/// How much of a line a row may draw.
///
/// A minified file is one line, and task 0 measured one at 295,067 bytes.
/// A longer line is cut **around its match** rather than at its start —
/// cutting at the start shows a screenful of somebody's bundle with the
/// match off the end of it.
pub const LINE_LIMIT: usize = 400;

/// What is being looked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The text, as typed.
    pub text: String,
    /// Whether `Alpha` and `alpha` are different.
    pub case_sensitive: bool,
    /// Whether `cat` may match inside `concatenate`.
    pub whole_word: bool,
    /// Whether the text is a pattern or a string.
    pub regular_expression: bool,
    /// Which files to look in — globs over the name, separated by
    /// commas, as a file mask usually is. Empty means every file.
    pub mask: String,
    /// Which directory to look in, relative to the project. `None` is
    /// the project, which is what `cmd-shift-f` asks by itself.
    ///
    /// **M8.7 shipped no scope selector deliberately**, on the grounds
    /// that a fifth control would not fit the toolbar. This is not a
    /// fifth control: it arrives from *Find in Files…* on a tree row,
    /// which is the route people actually take, and the toolbar
    /// only ever draws it when there is one — with a cross, because a
    /// state that can be reached and not left is a bug this repository
    /// has a name for.
    pub scope: Option<PathBuf>,
}

impl Query {
    /// Whether there is anything to search for.
    ///
    /// An empty pattern matches every line of every file, which is not
    /// what a person who has cleared the field meant by clearing it.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// One of the three toggles every search field has — M8.7's, moved here
/// by M8.9 so that the Search tool and the Find in Files popup turn the
/// same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// `Aa`: whether `Alpha` and `alpha` differ.
    MatchCase,
    /// `W`: whether a match must be a whole word.
    WholeWord,
    /// `.*`: whether the text is a pattern.
    Regex,
}

impl Toggle {
    /// Turn it over in a query.
    pub fn turn(self, query: &mut Query) {
        match self {
            Self::MatchCase => query.case_sensitive = !query.case_sensitive,
            Self::WholeWord => query.whole_word = !query.whole_word,
            Self::Regex => query.regular_expression = !query.regular_expression,
        }
    }

    /// Whether it is on in a query.
    pub fn is_on(self, query: &Query) -> bool {
        match self {
            Self::MatchCase => query.case_sensitive,
            Self::WholeWord => query.whole_word,
            Self::Regex => query.regular_expression,
        }
    }
}

/// One place the text was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The file, relative to the project — the key everything else in
    /// the window uses, so that the checkout can move.
    pub path: PathBuf,
    /// The line, **one-based**, as an editor counts and as the tab this
    /// row opens will be asked to land on.
    pub line: u32,
    /// The line's text, cut to something a row can draw.
    pub text: String,
    /// Where the matches are **within `text`** — after the cut, so a row
    /// can paint them without knowing a cut happened.
    pub ranges: Vec<Range<usize>>,
    /// Whether `text` is less than the line.
    pub cut: bool,
    /// Where the first match starts, in **characters of the whole line**,
    /// counted before any cut — what the editor's `Position` counts, so a
    /// tab can open at the match. 0 when there are no ranges. M5.11,
    /// decision 6.
    pub column: u32,
}

/// What a whole search came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    /// How many files had at least one hit.
    pub files: usize,
    /// How many hits were given out.
    pub hits: usize,
    /// Whether a limit stopped it — [`PER_FILE`] or [`TOTAL`].
    pub capped: bool,
    /// Whether the caller's flag stopped it.
    pub stopped: bool,
}

/// Why a search could not be run at all.
///
/// **Both carry somebody else's words.** A person who typed `a(` is told
/// what the regex crate says about `a(`; a message of ours would be a
/// worse version of it, written by someone who has not seen the pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The pattern will not compile.
    Pattern(String),
    /// The file mask will not compile.
    Mask(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pattern(said) | Self::Mask(said) => f.write_str(said),
        }
    }
}

/// Search a tree, handing over each file's hits as they are found.
///
/// **A file at a time, not a hit at a time.** The window draws on the
/// watch loop's clock and a batch per file is what a panel can absorb;
/// per hit would be a channel send for each of five thousand.
///
/// `stop` is checked before every file, so a query the person has already
/// replaced costs at most one more file.
pub fn search(
    root: &Path,
    query: &Query,
    stop: &AtomicBool,
    mut give: impl FnMut(Vec<Hit>),
) -> Result<Summary, Refused> {
    let mut summary = Summary::default();
    if query.is_empty() {
        return Ok(summary);
    }

    let matcher = matcher(query)?;
    let mask = mask(query)?;

    // **The walk starts at the scope and the hits stay the project's.**
    // A row's path is what every other panel keys on, so a scoped search
    // narrows where it looks and changes nothing about what it answers.
    let from = match &query.scope {
        Some(scope) => root.join(scope),
        None => root.to_path_buf(),
    };
    let mut walker = walker(&from);
    if let Some(mask) = mask {
        walker.overrides(mask);
    }

    for entry in walker.build().flatten() {
        if stop.load(Ordering::Relaxed) {
            summary.stopped = true;
            return Ok(summary);
        }
        if summary.hits >= TOTAL {
            summary.capped = true;
            return Ok(summary);
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };

        let left = TOTAL - summary.hits;
        let (hits, capped) = in_file(&matcher, entry.path(), relative, left);
        summary.capped |= capped;
        if hits.is_empty() {
            continue;
        }
        summary.files += 1;
        summary.hits += hits.len();
        give(hits);
    }
    Ok(summary)
}

/// A search on a thread: the walk, its channel and its generation, and the
/// hits it has sent so far — M8.9's decision 12.
///
/// **One engine, three holders.** The Search tool, the Find in Files popup
/// and Search Everywhere's *Text* tab each own one; the thread M8.7 wrote
/// in `root::search` is this, moved, so the rules it learned hold for all
/// three:
///
/// - **It streams.** Hits arrive a file at a time and are on hand while
///   the walk goes on — M8.7 measured a tree with no ignore file at twelve
///   seconds.
/// - **A replaced walk says nothing more.** A generation number rather
///   than trust in the order: the thread of the third keystroke may answer
///   after the thread of the fifth.
/// - **The old hits stay until the new walk answers** — its first file, or
///   its *nothing found* — because blanking them per keystroke is the blink
///   M8.3.4 took out of the Log. A pattern that will not compile says so in
///   [`Walk::refused`] and leaves them.
/// - **A dropped walk is a stopped walk.**
#[derive(Debug, Default)]
pub struct Walk {
    /// The thread that is walking, if one is.
    worker: Option<Worker>,
    /// Which walk was asked for last.
    generation: u64,
    /// Which walk the hits on hand belong to — moved when a new answer
    /// replaces them, so a holder can tell a replacement from a batch.
    answered: u64,
    hits: Vec<Hit>,
    documents: Vec<crate::project_search::Snapshot>,
    files: usize,
    capped: bool,
    running: bool,
    refused: Option<String>,
    /// Whether this walk's answers have replaced what was on hand.
    fresh: bool,
}

/// The thread walking a tree, and the two ways to reach it.
#[derive(Debug)]
struct Worker {
    answers: std::sync::mpsc::Receiver<Answer>,
    /// Set when the walk is abandoned. The walk reads it before every
    /// file, so an abandoned query costs at most one more.
    stop: std::sync::Arc<AtomicBool>,
    generation: u64,
}

impl Drop for Worker {
    /// **A dropped worker is a stopped walk.** A holder goes when a popup
    /// closes, a project closes and the window quits, and a thread walking
    /// ninety thousand files with nobody to answer is exactly the shape of
    /// thing M8.4 found outliving `cmd-q`.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What the thread says.
#[derive(Debug)]
enum Answer {
    /// One file's hits.
    Hits(Vec<Hit>),
    Document(crate::project_search::Snapshot, Vec<Hit>),
    /// The walk is over, and what it came to.
    Done(Summary),
    /// It never started: the pattern or the mask would not compile.
    Refused(String),
}

impl Walk {
    /// Stop whatever is walking and start again from `root` for `query`.
    ///
    /// **Nothing typed is not a search, and not an answer of one**: the
    /// hits go at once, because there is no older answer this is a
    /// narrowing of.
    pub fn restart(&mut self, root: &Path, query: Query) {
        self.restart_inner(root, query, None);
    }
    pub fn restart_documents(
        &mut self,
        root: &Path,
        query: Query,
        buffers: std::collections::HashMap<PathBuf, crate::project_search::Buffer>,
    ) {
        self.restart_inner(root, query, Some(buffers));
    }
    fn restart_inner(
        &mut self,
        root: &Path,
        query: Query,
        buffers: Option<std::collections::HashMap<PathBuf, crate::project_search::Buffer>>,
    ) {
        // Dropping the old worker sets its flag: the thread notices before
        // its next file and ends. Nothing waits for it.
        self.worker = None;
        self.generation += 1;
        self.refused = None;
        self.capped = false;
        self.fresh = false;

        if query.is_empty() {
            self.documents.clear();
            self.hits.clear();
            self.files = 0;
            self.running = false;
            self.answered = self.generation;
            return;
        }
        self.running = true;

        let generation = self.generation;
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let (answers, receiver) = std::sync::mpsc::channel::<Answer>();
        let mine = stop.clone();
        let root = root.to_path_buf();
        let started = std::thread::Builder::new()
            .name(format!("search-{generation}"))
            .spawn(move || {
                let sent = if let Some(buffers) = buffers {
                    crate::project_search::scan(&root, &query, &buffers, &mine, |snapshot, hits| {
                        let _ = answers.send(Answer::Document(snapshot, hits));
                    })
                } else {
                    search(&root, &query, &mine, |hits| {
                        let _ = answers.send(Answer::Hits(hits));
                    })
                };
                let _ = match sent {
                    Ok(summary) => answers.send(Answer::Done(summary)),
                    Err(refused) => answers.send(Answer::Refused(refused.to_string())),
                };
            });
        match started {
            Ok(_) => {
                self.worker = Some(Worker {
                    answers: receiver,
                    stop,
                    generation,
                });
            }
            Err(err) => {
                tracing::warn!(%err, "a search thread would not start");
                self.running = false;
                self.refused = Some(err.to_string());
            }
        }
    }

    /// Take what the thread has sent since the last look. Answers whether
    /// anything changed.
    pub fn take(&mut self) -> bool {
        use std::sync::mpsc::TryRecvError;

        // Taken out for the loop and put back after it, so that the
        // answers can change the walk they arrive on.
        let Some(worker) = self.worker.take() else {
            return false;
        };
        let mut moved = false;
        loop {
            match worker.answers.try_recv() {
                Ok(answer) => {
                    // **A late answer from an abandoned walk is dropped.**
                    // The channel belongs to one generation, so this can
                    // only be a worker nobody has replaced yet — but the
                    // check is what makes that a fact rather than a belief
                    // about `Drop` ordering.
                    if worker.generation != self.generation {
                        continue;
                    }
                    moved = true;
                    match answer {
                        Answer::Document(snapshot, hits) => {
                            if !self.fresh {
                                self.replace();
                            }
                            self.documents.push(snapshot);
                            self.files += 1;
                            self.hits.extend(hits);
                        }
                        Answer::Hits(hits) => {
                            if !self.fresh {
                                self.replace();
                            }
                            self.files += 1;
                            self.hits.extend(hits);
                        }
                        Answer::Done(summary) => {
                            // A walk that found nothing still replaces what
                            // was on hand — this is the moment the older
                            // answer stops being an answer.
                            if !self.fresh {
                                self.replace();
                            }
                            self.capped = summary.capped;
                            self.running = false;
                        }
                        Answer::Refused(said) => {
                            self.refused = Some(said);
                            self.running = false;
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.running = false;
                    break;
                }
            }
        }
        if self.running {
            self.worker = Some(worker);
        } else {
            self.fresh = false;
        }
        moved
    }

    /// The new answer's hits replace the old ones.
    fn replace(&mut self) {
        self.hits.clear();
        self.documents.clear();
        self.files = 0;
        self.fresh = true;
        self.answered = self.generation;
    }

    /// Stop walking, and keep what is on hand.
    pub fn stop(&mut self) {
        self.worker = None;
        self.running = false;
    }

    /// Keep current results usable after a local move; stale workers cannot append old paths.
    pub(crate) fn relocate_paths(&mut self, from: &Path, to: &Path) {
        self.stop();
        for hit in &mut self.hits {
            if let Some(next) = crate::file_operations::remap(&hit.path, from, to) {
                hit.path = next;
            }
        }
    }

    pub fn documents(&self) -> &[crate::project_search::Snapshot] {
        &self.documents
    }

    /// The hits so far.
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// How many files have hits.
    pub fn files(&self) -> usize {
        self.files
    }

    /// Whether the walk stopped at [`TOTAL`] or a file at [`PER_FILE`].
    pub fn capped(&self) -> bool {
        self.capped
    }

    /// Whether the thread is still walking.
    pub fn running(&self) -> bool {
        self.running
    }

    /// What the pattern or the mask said when it would not compile.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// Which walk was asked for last — a number that moves with every
    /// [`Self::restart`].
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Which walk the hits on hand are the answer of. It moves when a new
    /// answer replaces the old one, which is when a holder's highlight, kept
    /// by row, stops meaning anything.
    pub fn answered(&self) -> u64 {
        self.answered
    }
}

/// The walk a search takes from `from`: the ignore rules, the dotfiles and
/// the two names never entered — **the one place they are written.**
///
/// Taken out of [`search`] by M8.9's seams commit, as M5.11 left it and
/// with no change in behaviour, because the file-name index walks by the
/// same rules: a file Go to File cannot find is a file Find in Files
/// cannot search, and two builders is how the two would come to disagree.
/// A caller adds what is its own — a search its mask — to what this
/// answers.
pub fn walker(from: &Path) -> ignore::WalkBuilder {
    let mut walker = ignore::WalkBuilder::new(from);
    walker
        // `.gitignore` is obeyed, so a search is not a search of
        // `target/` — task 0: twelve times the files, all of them build
        // output.
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        // **Without a repository around it, too.** `ignore`'s default is
        // ripgrep's: git's rules apply only inside a git repository. The
        // file tree has never made that distinction — it dims whatever a
        // `.gitignore` names, wherever it finds one — and decision 4 is
        // that a search looks where the tree looks. Left at the default,
        // a project nobody has run `git init` in would have its
        // `node_modules` searched while the tree showed it greyed out.
        .require_git(false)
        // **A dotfile the Explorer draws is a file a search reads** —
        // M5.11, decision 3. `hidden(true)` skipped `.github/`,
        // `.env.example` and `.claude/settings.json`, which the tree has
        // shown since M8.1, against M8.7's decision 4 that a search looks
        // where the tree looks.
        .hidden(false)
        .filter_entry(|entry| !never_searched(entry));
    walker
}

/// The two names a walk never enters, at any depth below where it starts.
///
/// **`.git`**, which the tree leaves out too, directory or file: it is
/// git's database, or in a linked worktree a line pointing at it, and
/// neither is the project's text. **`.guild`**, which the tree does show
/// and a search does not: it is Guild's own record, and it holds the query
/// itself in `state.json`, so a search that read it would find its own
/// words in every project it was ever run in.
fn never_searched(entry: &ignore::DirEntry) -> bool {
    entry.depth() > 0 && is_guarded(entry.file_name())
}

/// Whether a path component names a directory a search never looks in.
///
/// Public since M8.9's seams commit, and task 0 is why: the file-name
/// index's second pass keeps every file the first did not yield, and in a
/// linked worktree `.git` is such a file. The index refuses it by this
/// rule rather than by a second spelling of `.git` and `.guild`.
pub fn is_guarded(name: &std::ffi::OsStr) -> bool {
    name == ".git" || name == ".guild"
}

/// How long the disk must be quiet before a stale answer is asked again —
/// autosave's pause. M5.11, decision 4.
pub const QUIET: Duration = Duration::from_millis(300);

/// How long an answer may stay stale while the disk never goes quiet.
///
/// An agent writing for a minute never leaves 300 ms between batches; a
/// panel that waited for quiet alone would say something about a tree
/// that stopped existing a minute ago.
pub const STALE_AT_MOST: Duration = Duration::from_secs(2);

/// Whether a change to these files, relative to the project, can change
/// what this query answers.
///
/// A path inside the query's scope — anywhere, when it has none — and not
/// under `.git` or `.guild`, which the walk never enters. `.guild` is the
/// one that matters: the window writes `.guild/state.json` on ordinary
/// gestures, and without it every tab switch would make every search
/// stale.
///
/// **The mask is not consulted.** A change to a file the mask excludes
/// costs one quiet walk; a mask applied here would be a second copy of
/// `ignore`'s glob rules, and two copies of a rule is how they come to
/// disagree.
pub fn touches(query: &Query, files: &[PathBuf]) -> bool {
    files.iter().any(|path| {
        let inside = query
            .scope
            .as_ref()
            .is_none_or(|scope| path.starts_with(scope));
        inside && !path.components().any(|part| is_guarded(part.as_os_str()))
    })
}

/// Whether a stale answer is due to be asked again: the disk quiet for
/// [`QUIET`] since the last batch, or [`STALE_AT_MOST`] since the first —
/// and **never while a walk is running**. A twelve-second walk restarted
/// every two seconds is a walk that never finishes; the one running is
/// let finish, and the refresh follows it.
pub fn refresh_due(first: Instant, last: Instant, now: Instant, walking: bool) -> bool {
    !walking
        && (now.saturating_duration_since(last) >= QUIET
            || now.saturating_duration_since(first) >= STALE_AT_MOST)
}

/// Where a hit's match is in a file's text as it is now: the one-based
/// line and the column in characters.
///
/// **The hit's own text first, nearest its old line.** A prepend moves
/// every line, and a nearer match of the query is a different place than
/// the one the person clicked. A hit that was cut has no whole line to
/// compare, so it goes straight to the second rule: the nearest line the
/// query matches. `None` when neither finds anything, or the pattern does
/// not compile — the caller then opens at the hit's own place.
///
/// This decides where a tab opens and never what a row paints: the rows
/// are replaced by the refresh, and a row coloured from a second search
/// of the same text is the lie `Hit::ranges` exists to avoid.
pub fn relocate(text: &str, query: &Query, hit: &Hit) -> Option<(u32, u32)> {
    let matcher = matcher(query).ok()?;
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let old = hit.line as usize;
    let nearest = |accept: &dyn Fn(&str) -> bool| {
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| accept(line))
            .min_by_key(|(at, _)| (at + 1).abs_diff(old))
            .map(|(at, line)| (at, *line))
    };

    let own = (!hit.cut)
        .then(|| nearest(&|line| line == hit.text))
        .flatten();
    let (at, line) = own.or_else(|| nearest(&|line| !matches_in(line, &matcher).is_empty()))?;
    let column = matches_in(line, &matcher)
        .first()
        .map_or(0, |range| line[..range.start].chars().count());
    Some((at as u32 + 1, column as u32))
}

/// The matcher for a query.
///
/// The three toggles are three flags of `grep-regex`'s own builder rather
/// than three ways of rewriting the pattern: `fixed_strings` is what
/// makes a dot a dot, and `word` is what makes `cat` not match inside
/// `concatenate` — an escape of ours and a `\b` of ours would be two
/// places to get the same rule wrong.
fn matcher(query: &Query) -> Result<grep_regex::RegexMatcher, Refused> {
    RegexMatcherBuilder::new()
        .case_insensitive(!query.case_sensitive)
        .fixed_strings(!query.regular_expression)
        .word(query.whole_word)
        .line_terminator(Some(b'\n'))
        .build(&query.text)
        .map_err(|err| Refused::Pattern(err.to_string()))
}

/// The file mask, as ripgrep's `-g` is: globs over the path, and a comma
/// separates them because that is what a mask field usually takes.
pub(crate) fn mask(query: &Query) -> Result<Option<ignore::overrides::Override>, Refused> {
    let globs: Vec<&str> = query
        .mask
        .split(',')
        .map(str::trim)
        .filter(|glob| !glob.is_empty())
        .collect();
    if globs.is_empty() {
        return Ok(None);
    }
    // The override is built against the current directory rather than the
    // root: these are globs over a name — `*.rs` — and anchoring them at
    // a root would make `*.rs` mean "in the root only".
    let mut builder = ignore::overrides::OverrideBuilder::new("");
    for glob in globs {
        builder
            .add(glob)
            .map_err(|err| Refused::Mask(err.to_string()))?;
    }
    builder
        .build()
        .map(Some)
        .map_err(|err| Refused::Mask(err.to_string()))
}

/// One file's hits, at most `left` of them and at most [`PER_FILE`].
///
/// A file that cannot be read, or that turns out to be binary, is not an
/// error: it is a file with no hits. `grep-searcher`'s binary detection
/// is ripgrep's, and a hit inside a `.png` is a line nobody can draw.
fn in_file(
    matcher: &grep_regex::RegexMatcher,
    full: &Path,
    relative: &Path,
    left: usize,
) -> (Vec<Hit>, bool) {
    let mut hits: Vec<Hit> = Vec::new();
    let allowed = PER_FILE.min(left);
    let mut capped = false;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(0))
        .line_number(true)
        .build();

    let taken = searcher.search_path(
        matcher,
        full,
        UTF8(|line, text| {
            if hits.len() >= allowed {
                capped = true;
                return Ok(false);
            }
            hits.push(row(relative, line as u32, text, matcher));
            Ok(true)
        }),
    );
    if taken.is_err() {
        // Unreadable, or not text after all. Neither is worth a notice:
        // a search says where the words are, not what the disk refused.
        return (Vec::new(), capped);
    }
    (hits, capped)
}

/// One row: the line cut to something drawable, with the matches placed
/// inside what is left.
fn row(relative: &Path, line: u32, text: &str, matcher: &grep_regex::RegexMatcher) -> Hit {
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    let ranges = matches_in(text, matcher);
    // Before the cut, which moves the ranges into a window of the line and
    // would make a column of a minified file's match a column of the row.
    let column = ranges
        .first()
        .map_or(0, |first| text[..first.start].chars().count()) as u32;
    let (text, ranges, cut) = cut_around(text, ranges);
    Hit {
        path: relative.to_path_buf(),
        line,
        text,
        ranges,
        cut,
        column,
    }
}

/// Where the matches are in a line.
///
/// The searcher hands over the line, not the places in it, so the line is
/// matched again here. It is the same matcher and the same bytes, which
/// is what makes the ranges safe to paint — the rule `SidePaint::of_line`
/// keeps for a diff, met from the other side.
fn matches_in(text: &str, matcher: &grep_regex::RegexMatcher) -> Vec<Range<usize>> {
    use grep_matcher::Matcher;

    let mut ranges = Vec::new();
    let _ = matcher.find_iter(text.as_bytes(), |found| {
        ranges.push(found.start()..found.end());
        true
    });
    ranges
}

/// Cut a long line around its first match.
///
/// **Around, not at the start.** Task 0 measured a 295,067-byte line in
/// the wild; a cut at the start of that is a screenful of minified
/// JavaScript with the match somewhere off the end.
///
/// The window is moved to a character boundary in both directions. GPUI
/// refuses a text run that begins inside a character, so a Cyrillic
/// comment in a long line would otherwise take the window down — the
/// trap M8.1.1 already carries for syntax spans.
pub(crate) fn cut_around(
    text: &str,
    ranges: Vec<Range<usize>>,
) -> (String, Vec<Range<usize>>, bool) {
    if text.len() <= LINE_LIMIT {
        return (text.to_owned(), ranges, false);
    }
    let first = ranges.first().map(|r| r.start).unwrap_or(0);
    // A third of the window before the match, so the match sits where the
    // eye lands rather than at the very edge.
    let before = LINE_LIMIT / 3;
    let mut from = first.saturating_sub(before);
    while from < text.len() && !text.is_char_boundary(from) {
        from += 1;
    }
    let mut to = (from + LINE_LIMIT).min(text.len());
    while to > from && !text.is_char_boundary(to) {
        to -= 1;
    }

    let kept = &text[from..to];
    let ranges = ranges
        .into_iter()
        .filter(|range| range.start >= from && range.end <= to)
        .map(|range| (range.start - from)..(range.end - from))
        .collect();
    (kept.to_owned(), ranges, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A dropped walk is a stopped walk.** The popup goes when it
    /// closes, a project's tool when the project does, and a thread walking
    /// ninety thousand files with nobody to answer is the shape of thing
    /// M8.4 found outliving `cmd-q`. The flag the thread reads before every
    /// file is read here, because no caller can reach it.
    #[test]
    fn a_dropped_walk_stops_its_thread() {
        let folder = tempfile::Builder::new()
            .prefix("guild-walk-drop-")
            .tempdir()
            .expect("a scratch folder");
        let root = folder.path();
        std::fs::write(root.join("a.txt"), b"alpha\n").expect("a file");

        let mut walk = Walk::default();
        walk.restart(
            root,
            Query {
                text: "alpha".to_owned(),
                ..Query::default()
            },
        );
        let stop = walk
            .worker
            .as_ref()
            .map(|worker| worker.stop.clone())
            .expect("a walk that started has a thread");
        assert!(!stop.load(Ordering::Relaxed));

        drop(walk);

        let deadline = Instant::now() + Duration::from_secs(5);
        while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(stop.load(Ordering::Relaxed), "the flag reads true");
    }

    /// The cut is a window, and the match is inside it rather than at
    /// its edge — which is the whole reason it is not a truncation.
    #[test]
    fn the_cut_puts_the_match_where_the_eye_lands() {
        let text = format!("{}needle{}", "x".repeat(4000), "y".repeat(4000));
        let at = 4000..4006;

        let (kept, ranges, cut) = cut_around(&text, vec![at]);

        assert!(cut);
        assert_eq!(kept.len(), LINE_LIMIT);
        assert_eq!(&kept[ranges[0].clone()], "needle");
        assert_eq!(
            ranges[0].start,
            LINE_LIMIT / 3,
            "a third in, not at the edge"
        );
    }

    /// A short line is not cut, and its ranges are left alone.
    #[test]
    fn a_line_that_fits_is_left_as_it_is() {
        let at = 3..7;

        let (kept, ranges, cut) = cut_around("fn beta() {}", vec![at.clone()]);

        assert!(!cut);
        assert_eq!(kept, "fn beta() {}");
        assert_eq!(ranges, vec![at]);
    }

    /// A match that falls outside the window is dropped rather than
    /// pointing past the end of the text the row draws.
    #[test]
    fn a_match_outside_the_window_is_not_painted() {
        let text = format!("needle{}needle", "x".repeat(4000));

        let (kept, ranges, _) = cut_around(&text, vec![0..6, 4006..4012]);

        assert_eq!(ranges.len(), 1);
        assert_eq!(&kept[ranges[0].clone()], "needle");
    }
}
