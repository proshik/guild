//! Annotate in a tab: what is behind `Body::Annotate`.
//!
//! *Annotate* is usually a column beside the editor's lines, and task 0
//! of M8.3 found the component cannot hold one: nothing public says
//! which line is at the top of its view, so a column of ours would
//! drift on the first scroll. This is a tab of ours instead — the
//! file's lines as `git blame` reads them, plain, with the column at
//! the left: who wrote the run and when, dimmer the older, and a way
//! from a run to its commit in the Log. Decision 10.
//!
//! The blame is asked for on the project's git thread and shown when it
//! lands; nothing here runs git. `lines` and `shades` are pure, for
//! `tests/annotate.rs`.

use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, EventEmitter, FocusHandle, UniformListScrollHandle};

use crate::file_tab::Look;
use crate::git::GitError;
use crate::git::blame::Run;
use crate::highlight::Language;
use crate::i18n::Ui;
use crate::syntax::{self, Span};

/// What the tab asks of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotateEvent {
    /// A run's click: that commit, opened in the Log.
    OpenCommit(String),
}

/// A file annotated, in a tab.
pub struct AnnotateTab {
    pub(crate) source: gpui::Entity<crate::source_snippet::SourceSnapshot>,
    pub(crate) source_offsets: Vec<usize>,
    path: PathBuf,
    /// What git said, once it has: the runs, or why there are none.
    runs: Option<Result<Vec<Run>, GitError>>,
    /// The spans of every drawn line, in the order [`lines`] draws them.
    /// Empty until the blame lands, and one empty row per line for a file
    /// no language claims.
    spans: Vec<Vec<Span>>,
    look: Look,
    ui: Ui,
    focus: FocusHandle,
    pub(crate) scroll: UniformListScrollHandle,
}

impl EventEmitter<AnnotateEvent> for AnnotateTab {}

impl AnnotateTab {
    /// A tab for a file's annotation, empty until the blame lands.
    pub fn new(path: PathBuf, look: Look, ui: Ui, cx: &mut Context<Self>) -> Self {
        Self {
            source: cx.new(|_| crate::source_snippet::SourceSnapshot::new("")),
            source_offsets: Vec::new(),
            path,
            runs: None,
            spans: Vec::new(),
            look,
            ui,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        }
    }

    /// The blame landed, or landed again after a commit.
    pub fn set_runs(&mut self, runs: Result<Vec<Run>, GitError>, cx: &mut Context<Self>) {
        let (snapshot, offsets) = match &runs {
            Ok(runs) => crate::source_snippet::SourceSnapshot::from_lines(
                runs.iter()
                    .flat_map(|run| run.lines.iter())
                    .map(String::as_str),
            ),
            Err(_) => (crate::source_snippet::SourceSnapshot::new(""), Vec::new()),
        };
        self.source = cx.new(|_| snapshot);
        self.source_offsets = offsets;
        self.spans = match &runs {
            Ok(runs) => spans(&self.path, runs),
            Err(_) => Vec::new(),
        };
        self.runs = Some(runs);
        cx.notify();
    }

    pub(crate) fn relocate(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.path = path.into();
        self.runs = None;
        self.spans.clear();
        self.source = cx.new(|_| crate::source_snippet::SourceSnapshot::new(""));
        self.source_offsets.clear();
        cx.notify();
    }

    /// The spans of every drawn line.
    pub fn spans(&self) -> &[Vec<Span>] {
        &self.spans
    }

    /// The file this tab annotates, relative to the project.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What git said, once it has.
    pub fn runs(&self) -> Option<&Result<Vec<Run>, GitError>> {
        self.runs.as_ref()
    }

    pub fn look(&self) -> &Look {
        &self.look
    }

    pub fn ui(&self) -> Ui {
        self.ui
    }

    /// The theme, the font or the language changed.
    pub fn restyle(&mut self, look: Look, ui: Ui, cx: &mut Context<Self>) {
        self.look = look;
        self.ui = ui;
        cx.notify();
    }

    pub fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }

    /// A run was clicked: the window opens its commit in the Log.
    pub fn open_commit(&mut self, sha: String, cx: &mut Context<Self>) {
        cx.emit(AnnotateEvent::OpenCommit(sha));
    }
}

/// The file's text as the blame describes it: its runs' lines in order,
/// each with the newline git took off.
///
/// This is what the spans are taken from, and it is why Annotate needs no
/// guard of the diff's kind: the line drawn **is** the line parsed, out
/// of one list, so the two cannot come apart.
pub fn text(runs: &[Run]) -> String {
    let mut text = String::new();
    for run in runs {
        for line in &run.lines {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

/// The spans of every line the tab draws, in the order [`lines`] draws
/// them.
///
/// Computed once, when the blame lands, rather than on a thread of its
/// own: `git blame` is much the slower of the two and this follows it,
/// and unlike a diff nothing rebuilds it as a person types. Past the
/// tab's limit — the same one — it is one empty row per line, which
/// means *draw it plain*.
pub fn spans(path: &Path, runs: &[Run]) -> Vec<Vec<Span>> {
    let count = lines(runs).len();
    let Some(language) = Language::of(path) else {
        return vec![Vec::new(); count];
    };
    let text = text(runs);
    if text.len() as u64 > crate::file_tab::HIGHLIGHT_LIMIT {
        return vec![Vec::new(); count];
    }
    let mut rows = syntax::spans(&text, language.grammar);
    rows.resize_with(count, Vec::new);
    rows
}

/// Every line of the file as (run, offset within the run), in order —
/// what the list draws one row per.
pub fn lines(runs: &[Run]) -> Vec<(usize, usize)> {
    runs.iter()
        .enumerate()
        .flat_map(|(run, r)| (0..r.lines.len()).map(move |offset| (run, offset)))
        .collect()
}

/// How dim each run is drawn: 0 for the newest third of the commits, 1
/// for the middle, 2 for the oldest — by rank among the file's commits
/// rather than by age in days, so a file untouched for a year still
/// tells its newer lines from its older ones. A line not committed yet
/// is the newest of all.
pub fn shades(runs: &[Run]) -> Vec<u8> {
    let ranked = revision_shades(
        runs.iter()
            .filter(|run| !run.is_uncommitted())
            .map(|run| run.time),
    );
    runs.iter()
        .map(|run| {
            if run.is_uncommitted() {
                0
            } else {
                ranked.get(&run.time).copied().unwrap_or(0)
            }
        })
        .collect()
}

/// Shared chronological thirds for committed attribution, independent of the
/// current date. Equal timestamps receive the same shade on every line.
pub(crate) fn revision_shades(
    times: impl IntoIterator<Item = i64>,
) -> std::collections::HashMap<i64, u8> {
    let mut times = times.into_iter().collect::<Vec<_>>();
    times.sort_unstable();
    times.dedup();
    let distinct = times.len();
    times
        .into_iter()
        .enumerate()
        .map(|(rank, time)| {
            let shade = if distinct < 2 {
                0
            } else {
                (2 - ((rank * 3) / distinct).min(2)) as u8
            };
            (time, shade)
        })
        .collect()
}
