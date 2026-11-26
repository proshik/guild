//! Revision-qualified, source-bound attribution. Never owns an editor or a write.
use super::*;
use crate::git::blame::{self, Run};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthorIdentity {
    pub root: PathBuf,
    pub revision: String,
    pub path: PathBuf,
    pub repository_relative: bool,
    pub fetch_generation: u64,
}
#[derive(Clone, Debug)]
pub(crate) enum AuthorData {
    Loading,
    Unavailable(String),
    Ready(Vec<Option<blame::Line>>),
}
pub(crate) struct AuthorWidth {
    pub font: gpui::Font,
    pub size: gpui::Pixels,
    pub padding: gpui::Pixels,
    pub language: guild_session::Language,
    pub width: gpui::Pixels,
    pub shades: HashMap<i64, u8>,
}
pub(crate) struct AuthorEntry {
    pub identity: Option<AuthorIdentity>,
    pub source: Arc<str>,
    pub generation: u64,
    pub data: AuthorData,
    pub width: Option<AuthorWidth>,
}
#[derive(Default)]
pub(crate) struct Authors {
    pub enabled: [bool; 2],
    pub font: Option<gpui::Font>,
    pub widths: [gpui::Pixels; 2],
    pub ranks: HashMap<PathBuf, HashMap<i64, u8>>,
    pub generation: u64,
    pub entries: HashMap<(PathBuf, usize), AuthorEntry>,
}
impl DiffTab {
    pub(crate) fn toggle_authors(&mut self, side: usize, cx: &mut Context<Self>) {
        if side > 1 {
            return;
        }
        self.authors.enabled[side] = !self.authors.enabled[side];
        self.authors
            .entries
            .retain(|(_, entry_side), _| *entry_side != side);
        cx.notify();
    }
    pub(crate) fn author_identity(&self, file: usize, side: usize) -> Option<AuthorIdentity> {
        let file = self.files.get(file)?;
        if matches!(self.base, Base::Proposal) {
            return None;
        }
        let (revision, path, repository_relative) = if matches!(self.base, Base::Commit(_)) {
            let identity = self.revisions.get(&file.path)?.get(side)?.as_deref()?;
            let (revision, path) = identity.split_once(':')?;
            (revision.to_owned(), PathBuf::from(path), true)
        } else {
            // Index/working sources retain attribution only where their text is
            // equal to committed HEAD, using the same mapping as FileTab.
            ("HEAD".to_owned(), file.path.clone(), false)
        };
        Some(AuthorIdentity {
            root: self.root.clone(),
            revision,
            path,
            repository_relative,
            fetch_generation: self.fetch_generation.get(),
        })
    }
    pub(crate) fn ensure_authors_for(&mut self, file: usize, cx: &mut Context<Self>) {
        let Some(path) = self.files.get(file).map(|file| file.path.clone()) else {
            return;
        };
        let Some(doc) = self.documents.get(&path).cloned() else {
            return;
        };
        for side in 0..2 {
            if !self.authors.enabled[side] {
                continue;
            }
            let source = doc.sources[side].text.clone();
            let identity = self.author_identity(file, side);
            if self
                .authors
                .entries
                .get(&(path.clone(), side))
                .is_some_and(|entry| {
                    entry.identity == identity && Arc::ptr_eq(&entry.source, &source)
                })
            {
                continue;
            }
            self.authors.generation = self.authors.generation.wrapping_add(1);
            let generation = self.authors.generation;
            self.authors.entries.insert(
                (path.clone(), side),
                AuthorEntry {
                    identity: identity.clone(),
                    source: source.clone(),
                    generation,
                    width: None,
                    data: if identity.is_some() {
                        AuthorData::Loading
                    } else {
                        AuthorData::Unavailable(
                            self.ui.text(crate::i18n::Text::BlameUnavailable).to_owned(),
                        )
                    },
                },
            );
            // A newly visible secondary file also needs one settling frame for
            // its measured metadata width, even if no Git request is possible.
            cx.notify();
            let Some(identity) = identity else {
                continue;
            };
            let path = path.clone();
            cx.spawn(async move |tab, cx| {
                let request = identity.clone();
                let text = source.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let root = if request.repository_relative {
                            git::toplevel(&request.root)?
                        } else {
                            request.root.clone()
                        };
                        let bytes = git::blame_at(&root, &request.revision, &request.path)?;
                        let runs = blame::parse(&bytes)
                            .map_err(|error| GitError::Failed(error.to_string()))?;
                        Ok::<_, GitError>(map_source_authors(&runs, &text))
                    })
                    .await;
                let _ = tab.update(cx, |tab, cx| {
                    tab.authors_landed(path, side, generation, identity, source, result, cx)
                });
            })
            .detach();
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn authors_landed(
        &mut self,
        path: PathBuf,
        side: usize,
        generation: u64,
        identity: AuthorIdentity,
        source: Arc<str>,
        result: Result<Vec<Option<blame::Line>>, GitError>,
        cx: &mut Context<Self>,
    ) {
        if identity.fetch_generation != self.fetch_generation.get()
            || !self.authors.enabled[side]
            || !self
                .documents
                .get(&path)
                .is_some_and(|doc| Arc::ptr_eq(&doc.sources[side].text, &source))
        {
            return;
        }
        let Some(entry) = self.authors.entries.get_mut(&(path, side)) else {
            return;
        };
        if entry.generation != generation
            || entry.identity.as_ref() != Some(&identity)
            || !Arc::ptr_eq(&entry.source, &source)
        {
            return;
        }
        entry.width = None;
        entry.data = match result {
            Ok(lines) => AuthorData::Ready(lines),
            Err(error) => AuthorData::Unavailable(error.to_string()),
        };
        cx.notify();
    }
}

/// Adapt original CR/LF/CRLF display lines to the existing equal-line blame
/// mapper. Split lone-CR Git lines without inventing new original Git indexes.
pub(crate) fn map_source_authors(runs: &[Run], text: &str) -> Vec<Option<blame::Line>> {
    let expanded;
    let runs = if runs.iter().any(|run| {
        run.lines
            .iter()
            .any(|line| line.strip_suffix('\r').unwrap_or(line).contains('\r'))
    }) {
        expanded = runs
            .iter()
            .flat_map(|run| {
                run.lines.iter().enumerate().flat_map(move |(index, line)| {
                    line.strip_suffix('\r')
                        .unwrap_or(line)
                        .split('\r')
                        .map(move |part| Run {
                            sha: run.sha.clone(),
                            author: run.author.clone(),
                            time: run.time,
                            summary: run.summary.clone(),
                            first_line: run.first_line + index,
                            original_line: run.original_line + index,
                            filename: run.filename.clone(),
                            lines: vec![part.to_owned()],
                        })
                })
            })
            .collect::<Vec<_>>();
        expanded.as_slice()
    } else {
        runs
    };
    let source = crate::diff::document::Source::new(text);
    let mut normalized = String::with_capacity(text.len() + 1);
    for line in 0..source.lines.len() {
        normalized.push_str(source.line(line));
        normalized.push('\n');
    }
    let mut mapped = blame::map(runs, &normalized);
    mapped.truncate(source.lines.len());
    mapped
}
