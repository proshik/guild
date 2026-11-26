//! A symbol index per project: tags on a thread, and a ready server's
//! `workspace/symbol` merged over them — M8.9, decision 11.
//!
//! **The tags half is the file index's shape**: a thread per project, made
//! by the first [`Root::rank_symbols`], walking by `file_index::walk`'s
//! rules and kept by the same batches through `file_index::apply`, tagging
//! every file whose language has a tags query, ranking on the thread with
//! a generation per ask, and stopped when its [`Tagger`] is dropped.
//! **Only the project's own files are tagged**: task 0 measured 20,000
//! registry files at 27 s in release, and a dependency's symbols are a
//! server's to give.
//!
//! **The server half is asked, never started** (task 11): `rank_symbols`
//! asks `workspace/symbol` of every server of the project that is already
//! `ready`, through `ask_server`, and a project with no Rust file open asks
//! nothing of rust-analyzer. The answers are taken on the watch loop's
//! clock and handed to the tagging thread **one server's answer at a
//! time**, so a slow server holds back nobody else's; the thread reads each
//! named file once to count the columns in characters, ranks them, and
//! merges them over the tags — a server row winning at the same place. A
//! server that says nothing in five seconds is left out of that answer, as
//! `go_to_definition` gives up, and **a server has one ask out at a time**
//! ([`Flights`]): the letters typed while it works wait, and only the last
//! of them is sent.
//!
//! The three doors are the ones group A asks; the watch loop reaches the
//! rest through `root::file_index`'s doors, which call the `symbols_`
//! methods here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

use gpui::Context;
use guild_session::ProjectId;
use serde_json::Value;

use crate::Root;
use crate::file_index::{self, Index, Under};
use crate::root::file_index::Walk;
use crate::symbols::{self, Row, Symbol, Where};

/// Every project's symbols.
#[derive(Default)]
pub(crate) struct Symbols {
    by_project: HashMap<ProjectId, Tagger>,
    /// The `workspace/symbol` asks out, and the ones waiting behind them.
    flights: Flights,
}

/// How long a server has to answer before its rows are left out.
const SERVER_PATIENCE: Duration = Duration::from_secs(5);

/// A server, as `Root.servers` keys it.
type ServerKey = (ProjectId, &'static str);

/// The `workspace/symbol` asks, **one out per server at a time**.
///
/// A person typing asks five times a second. Revoking an obsolete lease
/// releases its payload and sends best-effort `$/cancelRequest`; the next
/// watch tick settles it before sending the latest query. A server that
/// answers in order must not work through every letter — so the last query of
/// a burst, the one on screen, could run past [`SERVER_PATIENCE`] behind the
/// others and never get its rows. While an ask is out, a newer one waits,
/// in place of whichever waited before it, and is sent when the one out
/// settles.
struct Flights<T = super::lsp::ServerRequest> {
    out: HashMap<ServerKey, Flight<T>>,
    waiting: HashMap<ServerKey, Waiting>,
}

/// An ask out to a server.
#[derive(Debug)]
struct Flight<T> {
    generation: u64,
    id: Option<T>,
    started: Instant,
}

/// An ask waiting for the one out to settle.
#[derive(Debug, PartialEq, Eq)]
struct Waiting {
    generation: u64,
    query: String,
}

impl<T> Default for Flights<T> {
    fn default() -> Self {
        Self {
            out: HashMap::new(),
            waiting: HashMap::new(),
        }
    }
}

impl<T> Flights<T> {
    /// A ranking would ask this server: whether it may be sent now. When
    /// one is out it may not, and it waits in place of any that waited.
    fn wants(&mut self, key: ServerKey, generation: u64, query: &str) -> bool {
        if !self.out.contains_key(&key) {
            return true;
        }
        self.waiting.insert(
            key,
            Waiting {
                generation,
                query: query.to_owned(),
            },
        );
        false
    }

    /// An ask went out.
    fn sent(&mut self, key: ServerKey, generation: u64, id: T) {
        self.out.insert(
            key,
            Flight {
                generation,
                id: Some(id),
                started: Instant::now(),
            },
        );
    }

    /// The ask out to this server has settled; the one that waited, if any.
    fn settled(&mut self, key: ServerKey) -> Option<Waiting> {
        self.out.remove(&key);
        self.waiting.remove(&key)
    }

    /// A project closed.
    fn forget(&mut self, project: ProjectId) {
        self.out.retain(|(of, _), _| *of != project);
        self.waiting.retain(|(of, _), _| *of != project);
    }
}

/// The generations of the asks, and which answer is still wanted.
///
/// **A number rather than trust in the order**: a person typing replaces
/// the query five times a second, and the thread's answer to the third
/// may land after the fifth was asked.
#[derive(Debug, Default)]
struct Asks {
    asked: u64,
}

impl Asks {
    /// A new ask, and the generation its answer will carry.
    fn ask(&mut self) -> u64 {
        self.asked += 1;
        self.asked
    }

    /// Whether an answer of this generation is the one still wanted.
    fn fresh(&self, generation: u64) -> bool {
        generation == self.asked
    }
}

/// What the window asks of a project's tagging thread.
enum Ask {
    /// A watcher batch: apply it and tag what it touched, or walk again.
    Heard {
        files: Vec<PathBuf>,
        directories: Vec<PathBuf>,
        again: bool,
    },
    /// A ranking.
    Rank { generation: u64, query: String },
    /// What one server answered for a ranking, with the dependencies'
    /// roots as `Libraries::of` lists them, to place the rows. Each server's
    /// answer comes on its own, so a slow one holds back nobody else's.
    Server {
        generation: u64,
        answer: Value,
        libraries: Vec<PathBuf>,
    },
}

/// What the thread says.
enum Answer {
    /// Every file has been tagged once.
    Tagged,
    /// The rows for an ask.
    Ranked { generation: u64, rows: Vec<Row> },
}

/// One project's tags: the thread, and what it has answered.
pub(crate) struct Tagger {
    asks: Sender<Ask>,
    answers: Receiver<Answer>,
    stop: Arc<AtomicBool>,
    generations: Asks,
    ready: bool,
    rows: Vec<Row>,
}

impl Drop for Tagger {
    /// **A dropped tagger is a stopped thread** — GPUI's quit drops
    /// nothing, and `stop_every_index` is what drops this.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Tagger {
    /// Start tagging `root` on a thread of its own.
    pub(crate) fn start(root: PathBuf) -> Self {
        let (asks, asked) = channel();
        let (answer, answers) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let started = std::thread::Builder::new()
            .name("guild-symbols".into())
            .spawn(move || run(&root, &asked, &answer, &stopping, &file_index::walk));
        if let Err(err) = started {
            tracing::warn!(%err, "the symbol index's thread could not be started");
        }
        Self {
            asks,
            answers,
            stop,
            generations: Asks::default(),
            ready: false,
            rows: Vec::new(),
        }
    }

    /// Ask for a ranking; the answer is taken by [`Tagger::take`]. Answers
    /// the generation the servers' answers will be handed over under.
    pub(crate) fn ask(&mut self, query: &str) -> u64 {
        let generation = self.generations.ask();
        let _ = self.asks.send(Ask::Rank {
            generation,
            query: query.to_owned(),
        });
        generation
    }

    /// What a server answered for a ranking — dropped here when a newer
    /// ranking has been asked since.
    pub(crate) fn found(&self, generation: u64, answer: Value, libraries: Vec<PathBuf>) {
        if !self.generations.fresh(generation) {
            return;
        }
        let _ = self.asks.send(Ask::Server {
            generation,
            answer,
            libraries,
        });
    }

    /// A watcher batch, relative to the project.
    pub(crate) fn heard(&self, files: &[PathBuf], directories: &[PathBuf]) {
        let _ = self.asks.send(Ask::Heard {
            files: files.to_vec(),
            directories: directories.to_vec(),
            again: file_index::walks_again(files),
        });
    }

    /// Take what the thread has said. Answers whether anything drawn
    /// changed.
    pub(crate) fn take(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.answers.try_recv() {
                Ok(Answer::Tagged) => {
                    changed |= !self.ready;
                    self.ready = true;
                }
                Ok(Answer::Ranked { generation, rows }) => {
                    if self.generations.fresh(generation) {
                        self.rows = rows;
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return changed,
            }
        }
    }
}

/// A project's files and their tags, as the thread keeps them.
#[derive(Default)]
struct Tags {
    index: Index,
    /// Every taggable file's definitions — empty for a file that has
    /// none, so that it is known.
    by_file: BTreeMap<PathBuf, Vec<Symbol>>,
    /// The grammar each of the index's files was found to have, or none —
    /// asked once per path, since a name's language does not change.
    grammars: HashMap<PathBuf, Option<&'static str>>,
}

impl Tags {
    /// Walk the project and tag every file.
    fn walk(&mut self, root: &Path, stop: &AtomicBool, walk: &Walk<'_>) {
        self.index = Index::default();
        self.index
            .replace(Under::Project, walk(root, Under::Project, stop));
        self.by_file.clear();
        self.follow(root, &HashSet::new(), stop);
    }

    /// Bring the tags in step with the index: a file it no longer holds
    /// loses its tags, a file it holds that has none yet is tagged, and a
    /// file a batch named is tagged again.
    ///
    /// **A pass over the index costs a lookup per file and no more**: the
    /// language of a path is asked the first time the path is seen, and
    /// `touched` is a set. An agent writing all day sends a batch every
    /// 100 ms, and a registry-sized project is twenty thousand files.
    fn follow(&mut self, root: &Path, touched: &HashSet<PathBuf>, stop: &AtomicBool) {
        let index = &self.index;
        self.grammars
            .retain(|path, _| index.ignored(Under::Project, path) == Some(false));
        for (path, ignored) in index.files(Under::Project) {
            if !ignored && !self.grammars.contains_key(path) {
                self.grammars.insert(path.to_path_buf(), grammar_of(path));
            }
        }
        let grammars = &self.grammars;
        self.by_file
            .retain(|path, _| grammars.get(path).is_some_and(Option::is_some));
        let wanted: Vec<(PathBuf, &'static str)> = grammars
            .iter()
            .filter_map(|(path, grammar)| grammar.map(|grammar| (path, grammar)))
            .filter(|(path, _)| !self.by_file.contains_key(*path) || touched.contains(*path))
            .map(|(path, grammar)| (path.clone(), grammar))
            .collect();
        for (path, grammar) in wanted {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let tags = tag_file(&root.join(&path), grammar);
            self.by_file.insert(path, tags);
        }
    }

    /// Every definition, placed in the project.
    fn placed(&self) -> Vec<(Where, Symbol)> {
        self.by_file
            .iter()
            .flat_map(|(path, symbols)| {
                symbols.iter().map(|symbol| {
                    (
                        Where::Project {
                            path: path.clone(),
                            line: symbol.line,
                            column: symbol.column,
                        },
                        symbol.clone(),
                    )
                })
            })
            .collect()
    }
}

/// The grammar a file's name says has tags, if one does.
fn grammar_of(path: &Path) -> Option<&'static str> {
    let language = crate::highlight::Language::of(path)?;
    symbols::configuration(language.grammar).map(|_| language.grammar)
}

/// A file's definitions; nothing for one that cannot be read as text or is
/// over the limit, which is asked of its size before it is read.
fn tag_file(path: &Path, grammar: &str) -> Vec<Symbol> {
    let small =
        std::fs::metadata(path).is_ok_and(|meta| meta.len() <= crate::file_tab::HIGHLIGHT_LIMIT);
    if !small {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .map(|text| symbols::of_text(&text, grammar))
        .unwrap_or_default()
}

/// A server's answers placed in the project and ranked for `query`.
///
/// **Each named file is read once**, to turn the protocol's UTF-16 column
/// into characters; a file that cannot be read keeps the protocol's count,
/// which is the same for every line with nothing astral in it. The
/// project's root is compared as the disk has it — a server names real
/// paths, and `/var` is `/private/var` — **and as it was opened**, for a
/// server started in a project opened through a link, which names its
/// files through that link.
fn from_servers(root: &Path, query: &str, answers: Vec<Value>, libraries: &[PathBuf]) -> Vec<Row> {
    let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let project = real(root);
    let real_libraries: Vec<PathBuf> = libraries.iter().map(|library| real(library)).collect();
    let mut texts: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut placed_symbols: Vec<(Where, Symbol)> = Vec::new();
    for (path, line, utf16, mut symbol) in answers.into_iter().flat_map(symbols::from_server) {
        let text = texts
            .entry(path.clone())
            .or_insert_with(|| std::fs::read_to_string(&path).ok());
        let column = text
            .as_deref()
            .map_or(utf16, |text| symbols::placed(text, line, utf16));
        symbol.line = line;
        symbol.column = column;
        let at = match symbols::where_of(&project, &real_libraries, &path, line, column) {
            Where::Outside { .. } => symbols::where_of(root, libraries, &path, line, column),
            placed => placed,
        };
        placed_symbols.push((at, symbol));
    }
    symbols::rank_server(query, &placed_symbols, symbols::LIMIT)
}

/// The thread: tag everything, say so, then answer asks until the window
/// lets go — everything waiting read before anything is done, and the last
/// query ranked again after a batch, under its own generation. **A burst
/// holding a batch that asks for a walk is one walk, and its other batches
/// are not applied**, as the file index's thread does and for its reason.
fn run(
    root: &Path,
    asks: &Receiver<Ask>,
    answers: &Sender<Answer>,
    stop: &AtomicBool,
    walk: &Walk<'_>,
) {
    let mut tags = Tags::default();
    tags.walk(root, stop, walk);
    if stop.load(Ordering::Relaxed) || answers.send(Answer::Tagged).is_err() {
        return;
    }
    let mut last: Option<(u64, String)> = None;
    // The servers' rows for the last ranking, as each server's come.
    let mut server: Vec<Row> = Vec::new();
    loop {
        let Ok(first) = asks.recv() else {
            return;
        };
        let mut waiting = vec![first];
        waiting.extend(asks.try_iter());
        let mut batches: Vec<(Vec<PathBuf>, Vec<PathBuf>)> = Vec::new();
        let mut again = false;
        let mut fresh = false;
        for ask in waiting {
            match ask {
                Ask::Heard {
                    files,
                    directories,
                    again: walks,
                } => {
                    again |= walks;
                    batches.push((files, directories));
                }
                Ask::Rank { generation, query } => {
                    last = Some((generation, query));
                    server.clear();
                    fresh = true;
                }
                Ask::Server {
                    generation,
                    answer,
                    libraries,
                } => {
                    if let Some((asked, query)) = &last
                        && *asked == generation
                    {
                        server.extend(from_servers(root, query, vec![answer], &libraries));
                        fresh = true;
                    }
                }
            }
        }
        let changed = !batches.is_empty();
        if again {
            tags.walk(root, stop, walk);
        } else if changed {
            let mut touched: HashSet<PathBuf> = HashSet::new();
            for (files, directories) in batches {
                file_index::apply(&mut tags.index, root, Under::Project, &files, &directories);
                touched.extend(files);
            }
            tags.follow(root, &touched, stop);
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if !fresh && !changed {
            continue;
        }
        let Some((generation, query)) = &last else {
            continue;
        };
        let mut rows = symbols::rank(query, &tags.placed(), symbols::LIMIT);
        if !server.is_empty() {
            rows = symbols::rerank(query, symbols::merge(rows, server.clone()), symbols::LIMIT);
        }
        let sent = answers.send(Answer::Ranked {
            generation: *generation,
            rows,
        });
        if sent.is_err() {
            return;
        }
    }
}

impl Root {
    /// Whether a tagger was started for a project.
    #[cfg(test)]
    pub(crate) fn has_symbol_index(&self, project: ProjectId) -> bool {
        self.symbols.by_project.contains_key(&project)
    }

    /// Rank a project's symbols for `query`: tags on the index's thread,
    /// and every ready server asked — none started.
    ///
    /// `libraries` does not walk a dependency with tags (task 0: minutes);
    /// a dependency's symbols are a ready server's, asked with `*` appended
    /// — rust-analyzer 1.98.0 answered `WalkBuilder` with nothing and
    /// `WalkBuilder*` with `ignore`'s. Nothing typed asks no server.
    #[allow(dead_code, reason = "asked by M8.9 group A's Symbols tab")]
    pub(crate) fn rank_symbols(
        &mut self,
        project: ProjectId,
        query: &str,
        libraries: bool,
        _cx: &mut Context<Self>,
    ) {
        // Shown, not indexed — GLD-055: no tagger over a group's folder.
        if self.desk.is_project_group_root(project) {
            return;
        }
        let Some(path) = self.desk.project(project).map(|found| found.path.clone()) else {
            return;
        };
        let generation = self
            .symbols
            .by_project
            .entry(project)
            .or_insert_with(|| Tagger::start(path))
            .ask(query);
        if query.trim().is_empty() {
            self.symbols.flights.forget(project);
            return;
        }
        let asked = if libraries {
            format!("{query}*")
        } else {
            query.to_owned()
        };
        let languages: Vec<&'static str> = self
            .servers
            .keys()
            .filter(|(of, _)| *of == project)
            .map(|(_, language)| *language)
            .collect();
        for language in languages {
            let key = (project, language);
            if !self.symbols.flights.wants(key, generation, &asked) {
                if let Some(flight) = self.symbols.flights.out.get_mut(&key) {
                    // Drop request metadata now; retain only the coalescing slot
                    // until the next poll can send the latest waiting query.
                    flight.id.take();
                }
                continue;
            }
            // `ask_server` answers `None` for a server that is not ready,
            // and nothing here starts one.
            if let Some(id) = self.ask_server(
                project,
                language,
                "workspace/symbol",
                serde_json::json!({ "query": asked }),
            ) {
                self.symbols.flights.sent(key, generation, id);
            }
        }
    }

    /// The last ranking that landed for a project.
    #[allow(dead_code, reason = "drawn by M8.9 group A's Symbols tab")]
    pub(crate) fn ranked_symbols(&self, project: ProjectId) -> &[Row] {
        self.symbols
            .by_project
            .get(&project)
            .map_or(&[], |tagger| tagger.rows.as_slice())
    }

    /// Whether a project's tags have been read once.
    #[allow(dead_code, reason = "read by M8.9 group A's Indexing… note")]
    pub(crate) fn symbols_ready(&self, project: ProjectId) -> bool {
        // A group's folder has no tags to wait for — GLD-055.
        self.desk.is_project_group_root(project)
            || self
                .symbols
                .by_project
                .get(&project)
                .is_some_and(|tagger| tagger.ready)
    }

    /// A watcher batch for a project's tags — nothing until something has
    /// asked, since the first ask tags the disk as it is.
    pub(crate) fn symbols_heard(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        if let Some(tagger) = self.symbols.by_project.get(&project) {
            tagger.heard(batch.files(), batch.directories());
        }
    }

    /// What the tagging threads have answered. Answers whether anything
    /// drawn changed.
    pub(crate) fn symbols_poll(&mut self) -> bool {
        self.symbols_take_servers();
        let mut changed = false;
        for tagger in self.symbols.by_project.values_mut() {
            changed |= tagger.take();
        }
        changed
    }

    /// Take what the servers have answered and hand each answer to its
    /// project's thread as it comes. An ask settles when its server has
    /// answered, refused, gone, or run out of patience, and then the ask
    /// that waited behind it goes out — if its ranking is still the one
    /// wanted. Superseded requests are revoked locally before the next
    /// poll; their payloads cannot survive in the client.
    fn symbols_take_servers(&mut self) {
        let keys: Vec<_> = self.symbols.flights.out.keys().copied().collect();
        for key in keys {
            let Some(flight) = self.symbols.flights.out.remove(&key) else {
                continue;
            };
            let (project, language) = key;
            let settled = match flight
                .id
                .as_ref()
                .and_then(|lease| self.server_answer(project, language, lease))
            {
                Some(Ok(answer)) => {
                    self.ensure_libraries(project);
                    let libraries = self.libraries.of(project).to_vec();
                    if let Some(tagger) = self.symbols.by_project.get(&project) {
                        tagger.found(flight.generation, answer, libraries);
                    }
                    true
                }
                Some(Err(refusal)) => {
                    tracing::info!(%refusal, language, "workspace/symbol was refused");
                    true
                }
                None => {
                    flight.id.is_none()
                        || flight.started.elapsed() >= SERVER_PATIENCE
                        || !self.server_alive(project, language)
                }
            };
            if !settled {
                self.symbols.flights.out.insert(key, flight);
                continue;
            }
            drop(flight);
            let Some(next) = self.symbols.flights.settled(key) else {
                continue;
            };
            let wanted = self
                .symbols
                .by_project
                .get(&project)
                .is_some_and(|tagger| tagger.generations.fresh(next.generation));
            if !wanted {
                continue;
            }
            if let Some(id) = self.ask_server(
                project,
                language,
                "workspace/symbol",
                serde_json::json!({ "query": next.query }),
            ) {
                self.symbols.flights.sent(key, next.generation, id);
            }
        }
    }

    /// A project closed: its tags go, and the thread with them.
    pub(crate) fn symbols_forget(&mut self, project: ProjectId) {
        self.symbols.by_project.remove(&project);
        self.symbols.flights.forget(project);
    }

    /// The window is going: every tagging thread stops.
    pub(crate) fn symbols_stop_all(&mut self) {
        self.symbols.by_project.clear();
        self.symbols.flights = Flights::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_late_answer_is_dropped() {
        let mut asks = Asks::default();
        let first = asks.ask();
        let second = asks.ask();
        assert!(
            !asks.fresh(first),
            "the third letter's answer after the fifth"
        );
        assert!(asks.fresh(second));
    }

    /// Take until `done` says so, or five seconds pass.
    fn wait(tagger: &mut Tagger, done: impl Fn(&Tagger) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            tagger.take();
            if done(tagger) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn a_burst_asks_a_server_once_and_then_the_last_query() {
        let project = ProjectId::new();
        let rust = (project, "rust");
        let mut flights = Flights::default();

        assert!(flights.wants(rust, 1, "g"), "nothing is out: sent at once");
        flights.sent(rust, 1, 10);
        assert!(!flights.wants(rust, 2, "gr"), "one is out: this waits");
        assert!(
            !flights.wants(rust, 3, "gre"),
            "and this waits in its place"
        );
        assert!(
            flights.wants((project, "toml"), 3, "gre"),
            "another server is not held back by this one"
        );

        assert_eq!(
            flights.settled(rust),
            Some(Waiting {
                generation: 3,
                query: "gre".to_owned()
            }),
            "the last letter goes next, and the ones between never"
        );
        assert!(flights.settled(rust).is_none());
        assert!(flights.wants(rust, 4, "gree"), "nothing out again");

        flights.sent(rust, 4, 11);
        flights.wants(rust, 5, "greet");
        flights.forget(project);
        assert!(flights.out.is_empty() && flights.waiting.is_empty());
    }

    /// A tree of this test's own, removed when the test lets go of it.
    struct Fixture {
        root: PathBuf,
        _folder: tempfile::TempDir,
    }

    impl Fixture {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let folder = tempfile::Builder::new()
                .prefix(&format!("guild-tagger-{name}-"))
                .tempdir()
                .expect("a scratch folder");
            let root = folder.path();
            for (file, text) in files {
                let full = root.join(file);
                std::fs::create_dir_all(full.parent().expect("a parent")).expect("a directory");
                std::fs::write(full, text).expect("a file");
            }
            Self {
                root: root.canonicalize().expect("canonical"),
                _folder: folder,
            }
        }
    }

    /// A server's answer naming one function at `path`, line 0, column 3.
    fn answer_naming(name: &str, path: &Path) -> Value {
        serde_json::json!([{
            "name": name, "kind": 12,
            "location": {"uri": crate::lsp::uri(path),
                         "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 8}}}
        }])
    }

    /// Everything queued, the sender let go, and `run` on this thread: the
    /// answers it gave and how many walks it made.
    fn run_through(root: &Path, queued: Vec<Ask>) -> (Vec<Answer>, usize) {
        let (asks, asked) = channel();
        let (answer, answers) = channel();
        for ask in queued {
            asks.send(ask).expect("sent");
        }
        drop(asks);
        let walks = std::sync::atomic::AtomicUsize::new(0);
        let counting = |root: &Path, under: Under, stop: &AtomicBool| {
            walks.fetch_add(1, Ordering::Relaxed);
            file_index::walk(root, under, stop)
        };
        run(root, &asked, &answer, &AtomicBool::new(false), &counting);
        (answers.try_iter().collect(), walks.load(Ordering::Relaxed))
    }

    fn ranked(said: &[Answer]) -> Vec<(u64, Vec<Row>)> {
        said.iter()
            .filter_map(|answer| match answer {
                Answer::Ranked { generation, rows } => Some((*generation, rows.clone())),
                Answer::Tagged => None,
            })
            .collect()
    }

    #[test]
    fn a_burst_of_batches_that_walk_again_is_one_walk() {
        let fixture = Fixture::new("burst", &[("src/lib.rs", "fn greet() {}\n")]);
        let again = || Ask::Heard {
            files: Vec::new(),
            directories: Vec::new(),
            again: true,
        };
        let (said, walks) = run_through(
            &fixture.root,
            vec![
                again(),
                again(),
                again(),
                Ask::Rank {
                    generation: 1,
                    query: "greet".to_owned(),
                },
            ],
        );
        assert_eq!(walks, 2, "the first tagging, and one for the burst");
        let ranked = ranked(&said);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1.len(), 1);
    }

    #[test]
    fn the_thread_drops_a_server_answer_to_an_older_ranking_and_keeps_each_server_s() {
        let fixture = Fixture::new("servers", &[("src/lib.rs", "fn greet() {}\n")]);
        let root = fixture.root.clone();
        let (said, _) = run_through(
            &root,
            vec![
                Ask::Rank {
                    generation: 2,
                    query: "greet".to_owned(),
                },
                // An answer to the ranking before: nobody is asking it.
                Ask::Server {
                    generation: 1,
                    answer: answer_naming("greet_stale", &root.join("stale.rs")),
                    libraries: Vec::new(),
                },
                // Two servers of this ranking, answering one at a time.
                Ask::Server {
                    generation: 2,
                    answer: answer_naming("greet_one", &root.join("one.rs")),
                    libraries: Vec::new(),
                },
                Ask::Server {
                    generation: 2,
                    answer: answer_naming("greet_two", &root.join("two.rs")),
                    libraries: Vec::new(),
                },
            ],
        );
        let ranked = ranked(&said);
        let (generation, rows) = ranked.last().expect("a ranking");
        assert_eq!(*generation, 2);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert!(!names.contains(&"greet_stale"), "{names:?}");
        assert!(
            names.contains(&"greet_one"),
            "the first server's rows stay: {names:?}"
        );
        assert!(names.contains(&"greet_two"), "{names:?}");
        assert!(names.contains(&"greet"), "the tag stands: {names:?}");
    }

    #[test]
    fn a_server_path_under_the_root_as_it_was_opened_is_in_the_project() {
        let fixture = Fixture::new("linked", &[("src/lib.rs", "fn greet() {}\n")]);
        let folder = tempfile::Builder::new()
            .prefix("guild-tagger-link-")
            .tempdir()
            .expect("a scratch folder");
        let link = folder.path().join("linked");
        std::os::unix::fs::symlink(&fixture.root, &link).expect("a link");
        // Opened through the link, and named by the server through it too.
        let rows = from_servers(
            &link,
            "greet",
            vec![answer_naming("greet", &link.join("src/lib.rs"))],
            &[],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].at,
            Where::Project {
                path: PathBuf::from("src/lib.rs"),
                line: 0,
                column: 3
            }
        );
    }

    #[test]
    fn the_thread_tags_the_project_and_follows_a_batch() {
        let fixture = Fixture::new(
            "thread",
            &[
                ("src/lib.rs", "fn greet() {}\n"),
                ("notes.txt", "fn greeting() {}\n"),
            ],
        );
        let root = fixture.root.clone();

        let mut tagger = Tagger::start(root.clone());
        assert!(wait(&mut tagger, |it| it.ready), "the first tagging lands");
        tagger.ask("greet");
        assert!(wait(&mut tagger, |it| !it.rows.is_empty()));
        assert_eq!(tagger.rows.len(), 1, "a text file has no tags");
        assert_eq!(
            tagger.rows[0].at,
            Where::Project {
                path: PathBuf::from("src/lib.rs"),
                line: 0,
                column: 3
            }
        );

        std::fs::write(
            root.join("src/lib.rs"),
            "\nfn greet() {}\nfn greeter() {}\n",
        )
        .expect("rewritten");
        tagger.heard(&[PathBuf::from("src/lib.rs")], &[PathBuf::from("src")]);
        assert!(
            wait(&mut tagger, |it| it.rows.len() == 2),
            "a file the batch names is tagged again"
        );
        assert!(
            tagger
                .rows
                .iter()
                .all(|row| matches!(row.at, Where::Project { line: 1 | 2, .. }))
        );

        // A server's answer for this ranking: its row wins at the tag's
        // place, and a symbol outside the project joins the rows.
        let uri = crate::lsp::uri(&root.join("src/lib.rs"));
        let answer = serde_json::json!([
            {"name": "greet", "kind": 12, "containerName": "lib",
             "location": {"uri": uri, "range": {"start": {"line": 1, "character": 3}, "end": {"line": 1, "character": 8}}}},
            {"name": "greet_outside", "kind": 12,
             "location": {"uri": "file:///elsewhere/x.rs", "range": {"start": {"line": 4, "character": 0}, "end": {"line": 4, "character": 5}}}}
        ]);
        tagger.found(0, answer.clone(), Vec::new());
        tagger.found(tagger.generations.asked, answer, Vec::new());
        assert!(
            wait(&mut tagger, |it| it.rows.len() == 3),
            "the server's rows arrive: {:?}",
            tagger.rows
        );
        let greet = tagger
            .rows
            .iter()
            .find(|row| row.name == "greet")
            .expect("greet");
        assert_eq!(greet.from, symbols::Source::Server);
        assert_eq!(greet.container.as_deref(), Some("lib"));
        assert!(tagger.rows.iter().any(|row| row.name == "greet_outside"
            && matches!(
                row.at,
                Where::Outside {
                    line: 4,
                    column: 0,
                    ..
                }
            )));
        assert_eq!(
            tagger.rows.iter().filter(|row| row.name == "greet").count(),
            1,
            "one row per place"
        );
    }
}
