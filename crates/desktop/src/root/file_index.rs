//! A file-name index per project, on a thread — M8.9, decision 9.
//!
//! The git worker's shape for the seventh time: a thread per project, made
//! when [`Root::rank_files`] first asks, fed the watcher's batches by
//! [`Root::index_batch`] and the queries by `rank_files`, answering on the
//! watch loop's clock through [`Root::poll_indexes`]. **Ranked on the
//! thread**, with a generation per ask so a late answer is dropped, and
//! the rows on screen stay until the new answer lands — M8.3.4's rule.
//! **Stopped when its [`Indexer`] is dropped**: `forget_indexes` drops a
//! project's and `stop_every_index` drops them all, because GPUI's quit
//! drops nothing.
//!
//! `desktop::file_index` is the walk, the batch and the ranking, and knows
//! nothing about any of this.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use gpui::Context;
use guild_session::ProjectId;

use crate::Root;
use crate::file_index::{self, Entry, Index, LIMIT, Row, Under};

/// Every project's index.
#[derive(Default)]
pub(crate) struct Indexes {
    by_project: HashMap<ProjectId, Indexer>,
}

/// What the window asks of an index's thread.
enum Ask {
    /// A watcher batch: apply it, or walk again.
    Heard {
        files: Vec<PathBuf>,
        directories: Vec<PathBuf>,
        again: bool,
    },
    /// The dependencies' roots as `Libraries::of` lists them; the thread
    /// walks the ones it has not.
    Libraries(Vec<PathBuf>),
    /// A ranking.
    Rank(Ranking),
}

/// One ask for a ranking, kept by the thread so that a batch ranks the
/// last query again.
#[derive(Clone)]
struct Ranking {
    generation: u64,
    query: String,
    recent: Vec<PathBuf>,
    libraries: bool,
}

/// What an index's thread says.
enum Answer {
    /// The first walk has landed.
    Walked,
    /// The rows for an ask.
    Ranked { generation: u64, rows: Vec<Row> },
}

/// One project's index: the thread, and what it has answered.
pub(crate) struct Indexer {
    asks: Sender<Ask>,
    answers: Receiver<Answer>,
    stop: Arc<AtomicBool>,
    /// The generation of the last ask, which is the only one whose answer
    /// is drawn.
    asked: u64,
    /// How many of `Libraries::of`'s roots the thread has been told of.
    libraries: usize,
    ready: bool,
    rows: Vec<Row>,
}

impl Drop for Indexer {
    /// **A dropped indexer is a stopped thread.** The walk reads the flag
    /// before every entry, and the loop ends when the channel closes.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Indexer {
    /// Start walking `root` on a thread of its own.
    pub(crate) fn start(root: PathBuf) -> Self {
        let (asks, asked) = channel();
        let (answer, answers) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let started = std::thread::Builder::new()
            .name("guild-file-index".into())
            .spawn(move || run(&root, &asked, &answer, &stopping, &file_index::walk));
        if let Err(err) = started {
            tracing::warn!(%err, "the file index's thread could not be started");
        }
        Self {
            asks,
            answers,
            stop,
            asked: 0,
            libraries: 0,
            ready: false,
            rows: Vec::new(),
        }
    }

    /// Ask for a ranking; the answer is taken by [`Indexer::take`].
    ///
    /// `libraries` is `Some` with every root `Libraries::of` knows when a
    /// dependency's files are to be ranked too; the roots not yet told of
    /// are walked first.
    pub(crate) fn ask(&mut self, query: &str, recent: Vec<PathBuf>, libraries: Option<&[PathBuf]>) {
        if let Some(roots) = libraries
            && roots.len() > self.libraries
        {
            self.libraries = roots.len();
            let _ = self.asks.send(Ask::Libraries(roots.to_vec()));
        }
        self.asked += 1;
        let _ = self.asks.send(Ask::Rank(Ranking {
            generation: self.asked,
            query: query.to_owned(),
            recent,
            libraries: libraries.is_some(),
        }));
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
                Ok(Answer::Walked) => {
                    changed |= !self.ready;
                    self.ready = true;
                }
                Ok(Answer::Ranked { generation, rows }) => {
                    // A late answer is a query nobody is asking any more.
                    if generation == self.asked {
                        self.rows = rows;
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return changed,
            }
        }
    }
}

/// How a thread walks a root: `file_index::walk`, and a counting one in a
/// test.
pub(crate) type Walk<'a> = dyn Fn(&std::path::Path, Under, &AtomicBool) -> Vec<Entry> + 'a;

/// The thread: walk, say so, then answer asks until the window lets go.
///
/// Everything waiting is read before anything is done, so that a person
/// typing five letters is one ranking and a burst of batches is applied
/// before it rather than after. **A burst that holds a batch asking for a
/// walk is one walk**, and the other batches of it are not applied: the
/// walk reads the disk after every one of them was written. A checkout
/// queues a batch every 100 ms while a 1.8 s walk runs, and walking once
/// per batch was a backlog with no end while the writes went on.
fn run(
    root: &std::path::Path,
    asks: &Receiver<Ask>,
    answers: &Sender<Answer>,
    stop: &AtomicBool,
    walk: &Walk<'_>,
) {
    let mut index = Index::default();
    index.replace(Under::Project, walk(root, Under::Project, stop));
    if stop.load(Ordering::Relaxed) || answers.send(Answer::Walked).is_err() {
        return;
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut last: Option<Ranking> = None;
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
                Ask::Libraries(listed) => roots = listed,
                Ask::Rank(ranking) => {
                    last = Some(ranking);
                    fresh = true;
                }
            }
        }
        let mut changed = !batches.is_empty();
        if again {
            index.replace(Under::Project, walk(root, Under::Project, stop));
        } else {
            for (files, directories) in &batches {
                file_index::apply(&mut index, root, Under::Project, files, directories);
            }
        }
        if let Some(ranking) = &last
            && ranking.libraries
        {
            for (at, library) in roots.iter().enumerate() {
                let under = Under::Library(at);
                if !index.has(under) {
                    index.replace(under, walk(library, under, stop));
                    changed = true;
                }
            }
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if !fresh && !changed {
            continue;
        }
        let Some(ranking) = &last else {
            continue;
        };
        let rows = file_index::rank(
            &ranking.query,
            &index,
            &ranking.recent,
            ranking.libraries,
            LIMIT,
        );
        let sent = answers.send(Answer::Ranked {
            generation: ranking.generation,
            rows,
        });
        if sent.is_err() {
            return;
        }
    }
}

impl Root {
    /// Rank a project's files for `query`, on the index's thread. The
    /// answer lands through [`Root::poll_indexes`] and is read with
    /// [`Root::ranked_files`].
    ///
    /// The thread is made by the first ask, and an empty query answers
    /// the project's recent files (decision 10). `libraries` walks the
    /// roots `Libraries::of` knows the first time it is asked — which are
    /// only the dependencies a person has already gone into.
    #[allow(dead_code, reason = "asked by M8.9 group A's Files tab")]
    pub(crate) fn rank_files(
        &mut self,
        project: ProjectId,
        query: &str,
        libraries: bool,
        _cx: &mut Context<Self>,
    ) {
        // A project group's folder is shown, not indexed — GLD-055: no
        // index is started over it, however it is asked.
        if self.desk.is_project_group_root(project) {
            return;
        }
        let Some(path) = self.desk.project(project).map(|found| found.path.clone()) else {
            return;
        };
        let recent = self.recent_files_of(project);
        let roots = if libraries {
            self.ensure_libraries(project);
            Some(self.libraries.of(project).to_vec())
        } else {
            None
        };
        self.file_indexes
            .by_project
            .entry(project)
            .or_insert_with(|| Indexer::start(path))
            .ask(query, recent, roots.as_deref());
    }

    /// Whether an index thread was started for a project.
    #[cfg(test)]
    pub(crate) fn has_file_index(&self, project: ProjectId) -> bool {
        self.file_indexes.by_project.contains_key(&project)
    }

    /// The last ranking that landed for a project.
    #[allow(dead_code, reason = "drawn by M8.9 group A's Files tab")]
    pub(crate) fn ranked_files(&self, project: ProjectId) -> &[Row] {
        self.file_indexes
            .by_project
            .get(&project)
            .map_or(&[], |indexer| indexer.rows.as_slice())
    }

    /// Whether a project's first walk has landed.
    #[allow(dead_code, reason = "read by M8.9 group A's Indexing… note")]
    ///
    /// A project group's folder has no index and never will, so there is
    /// nothing to wait for: it answers ready, and empty — GLD-055.
    pub(crate) fn index_ready(&self, project: ProjectId) -> bool {
        self.desk.is_project_group_root(project)
            || self
                .file_indexes
                .by_project
                .get(&project)
                .is_some_and(|indexer| indexer.ready)
    }

    /// A watcher batch for a project's index — nothing until something
    /// has asked for one, since the first ask walks the disk as it is.
    pub(crate) fn index_batch(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        if batch.files().is_empty() && batch.directories().is_empty() {
            return;
        }
        // The symbols follow the same batches, by the same rules.
        self.symbols_heard(project, batch);
        if let Some(indexer) = self.file_indexes.by_project.get(&project) {
            indexer.heard(batch.files(), batch.directories());
        }
    }

    /// What the index threads have answered, on the watch loop's clock.
    pub(crate) fn poll_indexes(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for indexer in self.file_indexes.by_project.values_mut() {
            changed |= indexer.take();
        }
        changed |= self.symbols_poll();
        if changed {
            cx.notify();
        }
    }

    /// A project closed: its index goes, and its thread with it — and its
    /// symbols' thread, which this door stands for too.
    pub(crate) fn forget_indexes(&mut self, project: ProjectId) {
        self.file_indexes.by_project.remove(&project);
        self.symbols_forget(project);
    }

    /// The window is going: every index thread stops, the symbols' with
    /// them — GPUI's quit drops nothing.
    pub(crate) fn stop_every_index(&mut self) {
        self.file_indexes.by_project.clear();
        self.symbols_stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    /// A tree of this test's own under the temporary directory, removed
    /// when the test lets go of it — a run leaves nothing in `$TMPDIR`.
    struct Fixture {
        root: PathBuf,
        _folder: tempfile::TempDir,
    }

    impl Fixture {
        fn new(name: &str, files: &[&str]) -> Self {
            let folder = tempfile::Builder::new()
                .prefix(&format!("guild-indexer-{name}-"))
                .tempdir()
                .expect("a scratch folder");
            let root = folder.path();
            for file in files {
                let full = root.join(file);
                std::fs::create_dir_all(full.parent().expect("a parent")).expect("a directory");
                std::fs::write(full, "x").expect("a file");
            }
            Self {
                root: root.canonicalize().expect("canonical"),
                _folder: folder,
            }
        }
    }

    /// Take until `done` says so, or five seconds pass — a clock, never a
    /// count of tries.
    fn wait(indexer: &mut Indexer, done: impl Fn(&Indexer) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            indexer.take();
            if done(indexer) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn ranking(generation: u64, query: &str) -> Ask {
        Ask::Rank(Ranking {
            generation,
            query: query.to_owned(),
            recent: Vec::new(),
            libraries: false,
        })
    }

    #[test]
    fn the_thread_walks_ranks_and_follows_a_batch() {
        let fixture = Fixture::new("thread", &["src/alpha.rs"]);
        let root = fixture.root.clone();

        let mut indexer = Indexer::start(root.clone());
        assert!(wait(&mut indexer, |it| it.ready), "the first walk lands");

        indexer.ask("alpha", Vec::new(), None);
        assert!(wait(&mut indexer, |it| it.rows.len() == 1));
        assert_eq!(indexer.rows[0].path, PathBuf::from("src/alpha.rs"));

        // A batch ranks the last query again, without being asked.
        std::fs::write(root.join("src/alphabet.rs"), "x").expect("a file");
        indexer.heard(&[PathBuf::from("src/alphabet.rs")], &[PathBuf::from("src")]);
        assert!(
            wait(&mut indexer, |it| it.rows.len() == 2),
            "the new file joins the rows on screen"
        );
    }

    #[test]
    fn an_answer_to_an_older_ask_is_not_drawn() {
        // No thread: the answers are put on the channel by hand, so that
        // what is dropped is dropped by `take` and not by timing.
        let (asks, _asked) = channel();
        let (answer, answers) = channel();
        let mut indexer = Indexer {
            asks,
            answers,
            stop: Arc::new(AtomicBool::new(false)),
            asked: 2,
            libraries: 0,
            ready: true,
            rows: Vec::new(),
        };
        let row = |path: &str| Row {
            under: Under::Project,
            path: PathBuf::from(path),
            ignored: false,
            marks: Vec::new(),
            recent: false,
        };

        answer
            .send(Answer::Ranked {
                generation: 1,
                rows: vec![row("src/beta.rs")],
            })
            .expect("sent");
        assert!(!indexer.take(), "nothing drawn changed");
        assert!(
            indexer.rows.is_empty(),
            "the first letter's answer after the second"
        );

        answer
            .send(Answer::Ranked {
                generation: 2,
                rows: vec![row("src/alpha.rs")],
            })
            .expect("sent");
        assert!(indexer.take());
        assert_eq!(indexer.rows, vec![row("src/alpha.rs")]);
    }

    #[test]
    fn a_burst_of_batches_that_walk_again_is_one_walk_and_one_ranking() {
        let fixture = Fixture::new("burst", &["src/alpha.rs", "src/beta.rs"]);
        // Everything is queued before the thread reads any of it, and the
        // sending side is let go, so `run` answers the lot and returns —
        // on this thread, with no clock.
        let (asks, asked) = channel();
        let (answer, answers) = channel();
        for _ in 0..3 {
            asks.send(Ask::Heard {
                files: Vec::new(),
                directories: Vec::new(),
                again: true,
            })
            .expect("sent");
        }
        asks.send(ranking(1, "beta")).expect("sent");
        asks.send(ranking(2, "alpha")).expect("sent");
        drop(asks);

        let walks = AtomicUsize::new(0);
        let counting = |root: &std::path::Path, under: Under, stop: &AtomicBool| {
            walks.fetch_add(1, Ordering::Relaxed);
            file_index::walk(root, under, stop)
        };
        run(
            &fixture.root,
            &asked,
            &answer,
            &AtomicBool::new(false),
            &counting,
        );

        assert_eq!(
            walks.load(Ordering::Relaxed),
            2,
            "the first walk, and one for three batches that each asked for one"
        );
        let said: Vec<Answer> = answers.try_iter().collect();
        assert_eq!(said.len(), 2, "walked, and one ranking");
        assert!(matches!(said[0], Answer::Walked));
        let Answer::Ranked { generation, rows } = &said[1] else {
            panic!("a ranking");
        };
        assert_eq!(*generation, 2, "the last letter typed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, PathBuf::from("src/alpha.rs"));
    }
}
