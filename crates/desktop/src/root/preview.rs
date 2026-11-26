//! The preview's asking: the open buffer or the disk, parsed whole off the
//! window's thread — M8.9, decision 13.
//!
//! Search Everywhere (group A) and Find in Files (group C) both ask through
//! [`Root::ask_preview`] and draw with `Root::render_preview`. One preview
//! at a time, because one popup is up at a time.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{AppContext, Context};
use guild_session::ProjectId;

use crate::Root;
use crate::preview::{self, Excerpt};

/// How many lines a preview shows above and below its line: eleven in all,
/// what section 5's 220 holds at the code face's height.
pub(crate) const AROUND: usize = 5;

/// The preview being made, and the one on screen.
#[derive(Default)]
pub(crate) struct Previews {
    /// What was asked last — so the same ask on every look costs nothing.
    asked: Option<Asked>,
    /// The number of the ask last made. An answer carrying another number
    /// is an answer to a row the person has already left.
    serial: u64,
    /// The same number where the work can read it, so that an ask
    /// overtaken before its turn does not read or parse at all.
    latest: Arc<AtomicU64>,
    /// What has landed.
    shown: Option<Shown>,
    /// How many lines around the asked one the last ask wanted.
    around: usize,
}

/// A preview that has landed.
pub(crate) struct Shown {
    /// What it was asked for.
    pub asked: Asked,
    /// The lines and their spans.
    pub excerpt: Excerpt,
    pub source: gpui::Entity<crate::source_snippet::SourceSnapshot>,
    pub offsets: Vec<usize>,
}

/// What a popup wants previewed: a line of a file, and the hit on it when
/// there is one — its text and the byte ranges it matched at, so a match is
/// marked only on a line that is still the hit's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    /// Whose file.
    pub project: ProjectId,
    /// Which root `path` is relative to.
    pub origin: crate::Origin,
    /// The file.
    pub path: PathBuf,
    /// Zero-based.
    pub line: u32,
    /// The hit's line as it was found, and where it matched.
    pub hit: Option<(String, Vec<Range<usize>>)>,
}

/// Which preview is asked for: a popup's, or the Find tool's beside its
/// list. **Two, because both can be up at once** — Find in Files opened
/// over a Find tab — and one slot would have each answer overwrite the
/// other's. M8.15.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The popup's — Search Everywhere's, Find in Files'.
    Popup,
    /// The Find tool's.
    Find,
}

impl Root {
    /// The previews of one slot.
    fn previews_of(&mut self, slot: Slot) -> &mut Previews {
        match slot {
            Slot::Popup => &mut self.previews,
            Slot::Find => &mut self.find_previews,
        }
    }

    /// Preview this in a popup, or nothing.
    pub(crate) fn ask_preview(&mut self, asked: Option<Asked>, cx: &mut Context<Self>) {
        self.ask_preview_in(Slot::Popup, asked, AROUND, cx);
    }

    /// Preview this, or nothing.
    ///
    /// **The open buffer when a tab has the file, the disk otherwise** —
    /// the order a diff reads a working tree in, because what a person has
    /// typed and not saved is the file they are reading. The buffer is
    /// copied here, where the editor can be read; the disk is read, and the
    /// whole text parsed, on the background executor.
    ///
    /// **Only the newest ask is worked on**, the diff painting thread's rule
    /// met in another shape: every ask carries a number, work overtaken
    /// before its turn does nothing, and an answer carrying an old number is
    /// dropped. What is on screen stays until the new answer lands.
    ///
    /// `around` is how many lines above and below: a popup's eleven, or
    /// what fills the Find tool's pane.
    pub(crate) fn ask_preview_in(
        &mut self,
        slot: Slot,
        asked: Option<Asked>,
        around: usize,
        cx: &mut Context<Self>,
    ) {
        let previews = self.previews_of(slot);
        if previews.asked == asked && previews.around == around {
            return;
        }
        previews.serial += 1;
        previews.around = around;
        let serial = previews.serial;
        previews.latest.store(serial, Ordering::Relaxed);
        previews.asked = asked.clone();
        let Some(asked) = asked else {
            previews.shown = None;
            cx.notify();
            return;
        };
        let Some(root) = self.root_of(asked.project, asked.origin) else {
            return;
        };
        let full = root.join(&asked.path);
        let buffer = self.preview_buffer(&asked, cx);
        let grammar = crate::highlight::Language::of(&asked.path).map(|language| language.grammar);
        let latest = self.previews_of(slot).latest.clone();
        let line = asked.line;

        let work = cx.background_spawn(async move {
            if latest.load(Ordering::Relaxed) != serial {
                return None;
            }
            let text = match buffer {
                Some(text) => text,
                None => {
                    let size = std::fs::metadata(&full).ok()?.len();
                    if size > crate::file_tab::OPEN_LIMIT {
                        return None;
                    }
                    std::fs::read_to_string(&full).ok()?
                }
            };
            if latest.load(Ordering::Relaxed) != serial {
                return None;
            }
            Some(preview::excerpt(&text, grammar, line, around))
        });
        cx.spawn(async move |root, cx| {
            let Some(excerpt) = work.await else {
                return;
            };
            let _ = root.update(cx, |root, cx| {
                let previews = root.previews_of(slot);
                if previews.serial != serial {
                    return;
                }
                let (snapshot, offsets) = crate::source_snippet::SourceSnapshot::from_lines(
                    excerpt.lines.iter().map(String::as_str),
                );
                let source = cx.new(|_| snapshot);
                previews.shown = Some(Shown {
                    asked,
                    excerpt,
                    source,
                    offsets,
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// The popup's preview on screen, if one has landed.
    pub(crate) fn preview_shown(&self) -> Option<&Shown> {
        self.previews.shown.as_ref()
    }

    /// The Find tool's preview on screen, if one has landed.
    pub(crate) fn find_preview_shown(&self) -> Option<&Shown> {
        self.find_previews.shown.as_ref()
    }

    /// The text of an open tab's buffer for this file, if a tab has it.
    fn preview_buffer(&self, asked: &Asked, cx: &Context<Self>) -> Option<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == asked.project)
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                crate::Body::File {
                    path,
                    origin,
                    tab: view,
                    ..
                } if *path == asked.path && *origin == asked.origin => view
                    .read(cx)
                    .editor()
                    .map(|editor| editor.read(cx).value().to_string()),
                _ => None,
            })
    }
}

impl Previews {
    pub(crate) fn invalidate_path(&mut self, project: ProjectId, from: &std::path::Path) {
        let affected = |asked: &Asked| {
            asked.project == project
                && asked.origin == crate::Origin::Project
                && asked.path.starts_with(from)
        };
        if self.asked.as_ref().is_some_and(affected)
            || self
                .shown
                .as_ref()
                .is_some_and(|shown| affected(&shown.asked))
        {
            self.serial += 1;
            self.latest.store(self.serial, Ordering::Relaxed);
            self.asked = None;
            self.shown = None;
        }
    }
}
