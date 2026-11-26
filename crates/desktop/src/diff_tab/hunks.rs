//! A diff's hunk actions — GLD-026, decision 55: what each shown block
//! offers, worked out off the window's thread, and the write a click makes.
//!
//! **Two inputs, apart from the diff's own machinery.** The facts — the
//! index text a HEAD diff needs and whether the file takes hunks at all —
//! are read by a thread of their own whenever the diff fetches its texts.
//! The offers are computed on the background executor once a document and
//! its facts are both in hand, and kept only while that same document is
//! shown: a newer document makes them stale, and nothing is offered until
//! the newer one has its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use gpui::Context;
use guild_session::Base;

use super::{DiffEvent, DiffTab};
use crate::diff::document::Document;
use crate::git::{self, Hunk, Write};
use crate::hunk::{Action, Comparisons, Offer, View};
use crate::i18n::Text;

/// What git says about a path, for its hunks, as the diff draws it.
#[derive(Debug, Clone)]
pub(crate) struct Facts {
    /// The index entry's text — what a HEAD diff compares against twice.
    index: Option<Arc<str>>,
    /// `HEAD`'s bytes, which a Stage carries when the diff does not show
    /// them — the Unstaged diff.
    head: Option<Arc<[u8]>>,
    /// Whether the file takes hunks, or the hint saying why not.
    eligible: Result<(), Text>,
    /// Line-ending conversion may run there.
    endings: bool,
}

/// The offers for one shown document.
pub(crate) struct Offers {
    /// What the offers were computed from: the document and the facts'
    /// fetch generation. **Both** are the freshness token — a refresh can
    /// bring the same document back with another index under it.
    document: Arc<Document>,
    generation: u64,
    comparisons: Option<Arc<Comparisons>>,
    index: Option<Arc<str>>,
    head: Option<Arc<[u8]>>,
    pub(crate) eligible: Result<(), Text>,
    pub(crate) offers: Vec<Offer>,
}

pub(crate) struct Hunks {
    facts: HashMap<PathBuf, (u64, Facts)>,
    offers: HashMap<PathBuf, Offers>,
    /// The job computing a path's offers, with its token and its stop.
    pending: HashMap<PathBuf, Pending>,
    asks: mpsc::Sender<(PathBuf, u64, Facts)>,
    answers: mpsc::Receiver<(PathBuf, u64, Facts)>,
    /// The fetch generation each path's facts are wanted for, shared with
    /// the threads reading them: a superseded thread skips a path before it
    /// spends a git read on it — review r2 `bugs+impl-10`.
    wanted: Arc<std::sync::Mutex<HashMap<PathBuf, u64>>>,
    /// Facts read so far, and a gate a test holds to order two batches.
    #[cfg(test)]
    reads: Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    gate: Arc<std::sync::Mutex<()>>,
}

pub(crate) struct Pending {
    document: Arc<Document>,
    generation: u64,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Pending {
    fn is(&self, document: &Arc<Document>, generation: u64) -> bool {
        Arc::ptr_eq(&self.document, document) && self.generation == generation
    }
}

impl Drop for Pending {
    /// A job superseded or forgotten stops at its next block.
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Hunks {
    /// Forget every path not in `keep`.
    pub(super) fn retain(&mut self, keep: &[PathBuf]) {
        self.facts.retain(|p, _| keep.contains(p));
        self.offers.retain(|p, _| keep.contains(p));
        self.pending.retain(|p, _| keep.contains(p));
        self.wanted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|p, _| keep.contains(p));
    }
}

impl Default for Hunks {
    fn default() -> Self {
        let (asks, answers) = mpsc::channel();
        Self {
            facts: HashMap::new(),
            offers: HashMap::new(),
            pending: HashMap::new(),
            asks,
            answers,
            wanted: Default::default(),
            #[cfg(test)]
            reads: Default::default(),
            #[cfg(test)]
            gate: Default::default(),
        }
    }
}

/// The comparison a base shows, when its blocks can be staged.
pub(crate) fn view_of(base: &Base) -> Option<View> {
    match base {
        Base::Head => Some(View::Head),
        Base::Unstaged => Some(View::Unstaged),
        Base::Index => Some(View::Staged),
        Base::Commit(_) | Base::Proposal => None,
    }
}

/// How an offer is drawn: the icon's action and whether it is enabled, or
/// the hint saying why not. One table for the diff and the marker popup.
pub(crate) fn present(view: View, offer: Offer) -> Option<(Action, Result<(), Text>)> {
    let shown = match view {
        View::Staged => Action::Unstage,
        View::Head | View::Unstaged => Action::Stage,
    };
    match offer {
        Offer::Stage => Some((Action::Stage, Ok(()))),
        Offer::Unstage => Some((Action::Unstage, Ok(()))),
        Offer::Wider => Some((shown, Err(Text::HunkWider))),
        Offer::Mixed => Some((shown, Err(Text::HunkMixed))),
        Offer::LineEndings => Some((shown, Err(Text::HunkLineEndings))),
        Offer::Converted => Some((shown, Err(Text::NoticeHunkConverted))),
        Offer::Nothing => None,
    }
}

/// The hint for a file that takes no hunks.
pub(crate) fn hint(eligibility: git::hunk::Eligibility) -> Result<(), Text> {
    match eligibility {
        git::hunk::Eligibility::Yes => Ok(()),
        git::hunk::Eligibility::WholeFile => Err(Text::HunkWholeFile),
        git::hunk::Eligibility::Converted => Err(Text::NoticeHunkConverted),
    }
}

/// A block's offer, disabled as converted when line-ending conversion may run
/// and the bytes it would write hold a carriage return. The index was
/// checked whole, so only the replacement can bring one.
pub(crate) fn converted(
    comparisons: &Comparisons,
    view: View,
    block: &crate::diff::document::Block,
    endings: bool,
) -> Offer {
    let offer = comparisons.offer(view, block);
    if endings
        && comparisons
            .replacement(view, block)
            .is_some_and(|with| with.contains('\r'))
    {
        return Offer::Converted;
    }
    offer
}

/// The facts of one path, read on a thread.
fn read_facts(root: &Path, path: &Path) -> Facts {
    let facts = git::hunk::facts(root, path, crate::diff::DIFF_LIMIT as usize, usize::MAX);
    Facts {
        index: facts.index,
        head: facts.head,
        eligible: hint(facts.eligibility),
        endings: facts.endings,
    }
}

impl DiffTab {
    /// Ask for the facts of these paths, with the fetch that asks for their
    /// texts.
    pub(super) fn fetch_hunk_facts(&mut self, paths: &[PathBuf], generation: u64) {
        if view_of(&self.base).is_none() || self.inspection.is_some() {
            return;
        }
        for path in paths {
            self.hunks.facts.remove(path);
            self.hunks.offers.remove(path);
            self.hunks.pending.remove(path);
        }
        {
            let mut wanted = self
                .hunks
                .wanted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for path in paths {
                wanted.insert(path.clone(), generation);
            }
        }
        let root = self.root.clone();
        let asks = self.hunks.asks.clone();
        let wanted = self.hunks.wanted.clone();
        #[cfg(test)]
        let (reads, gate) = (self.hunks.reads.clone(), self.hunks.gate.clone());
        let paths = paths.to_vec();
        let spawned = std::thread::Builder::new()
            .name("guild-hunk-facts".into())
            .spawn(move || {
                #[cfg(test)]
                drop(gate.lock());
                for path in paths {
                    let current = wanted
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get(&path)
                        == Some(&generation);
                    if !current {
                        continue;
                    }
                    #[cfg(test)]
                    reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let facts = read_facts(&root, &path);
                    if asks.send((path, generation, facts)).is_err() {
                        return;
                    }
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "the hunk facts thread could not be started");
        }
    }

    /// Take the facts that arrived and start the offers that can be
    /// computed now; true when something changed.
    pub(super) fn poll_hunks(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        while let Ok((path, generation, facts)) = self.hunks.answers.try_recv() {
            if self.expected_fetch.borrow().get(&path) != Some(&generation) {
                continue;
            }
            self.hunks.facts.insert(path, (generation, facts));
            changed = true;
        }
        let Some(view) = view_of(&self.base).filter(|_| self.inspection.is_none()) else {
            return changed;
        };
        let paths: Vec<PathBuf> = self.files.iter().map(|f| f.path.clone()).collect();
        for path in paths {
            let (Some(document), Some((generation, facts))) =
                (self.documents.get(&path), self.hunks.facts.get(&path))
            else {
                continue;
            };
            let generation = *generation;
            if self
                .hunks
                .offers
                .get(&path)
                .is_some_and(|o| Arc::ptr_eq(&o.document, document) && o.generation == generation)
                || self
                    .hunks
                    .pending
                    .get(&path)
                    .is_some_and(|p| p.is(document, generation))
            {
                continue;
            }
            let document = document.clone();
            let facts = facts.clone();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            // Replacing a pending job stops it.
            self.hunks.pending.insert(
                path.clone(),
                Pending {
                    document: document.clone(),
                    generation,
                    stop: stop.clone(),
                },
            );
            let job = cx.background_executor().spawn(async move {
                let [old, new] = [
                    document.sources[0].text.clone(),
                    document.sources[1].text.clone(),
                ];
                let (head, index, working) = match view {
                    View::Head => (old.clone(), facts.index.clone(), new.clone()),
                    View::Unstaged => (old.clone(), Some(old.clone()), new.clone()),
                    View::Staged => (old.clone(), Some(new.clone()), new.clone()),
                };
                let comparisons = index
                    .as_ref()
                    .filter(|_| facts.eligible.is_ok())
                    .and_then(|index| Comparisons::new(&head, index, &working))
                    .map(Arc::new);
                let mut offers = Vec::with_capacity(document.blocks.len());
                for block in &document.blocks {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        return None;
                    }
                    offers.push(match &comparisons {
                        Some(c) => converted(c, view, block, facts.endings),
                        None => Offer::Nothing,
                    });
                }
                Some(Offers {
                    document,
                    generation,
                    comparisons,
                    index,
                    head: facts.head,
                    eligible: facts.eligible,
                    offers,
                })
            });
            cx.spawn(async move |owner, cx| {
                let Some(offers) = job.await else {
                    return;
                };
                let _ = owner.update(cx, |tab, cx| {
                    let fresh = |tab: &DiffTab| {
                        tab.documents
                            .get(&path)
                            .is_some_and(|d| Arc::ptr_eq(d, &offers.document))
                            && tab
                                .hunks
                                .facts
                                .get(&path)
                                .is_some_and(|(g, _)| *g == offers.generation)
                    };
                    if tab
                        .hunks
                        .pending
                        .get(&path)
                        .is_some_and(|p| p.is(&offers.document, offers.generation))
                    {
                        tab.hunks.pending.remove(&path);
                    }
                    if fresh(tab) {
                        tab.hunks.offers.insert(path, offers);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        changed
    }

    /// What block `block` of file `file` offers, for drawing: `None` when
    /// this diff stages nothing or the offers are not in yet.
    pub(crate) fn hunk_offer(
        &self,
        file: usize,
        block: usize,
    ) -> Option<(Action, Result<(), Text>)> {
        let view = view_of(&self.base).filter(|_| self.inspection.is_none())?;
        let path = &self.files.get(file)?.path;
        let document = self.documents.get(path)?;
        let offers = self.hunks.offers.get(path).filter(|o| {
            Arc::ptr_eq(&o.document, document)
                && self
                    .hunks
                    .facts
                    .get(path)
                    .is_some_and(|(g, _)| *g == o.generation)
        })?;
        if let Err(why) = offers.eligible {
            let action = match view {
                View::Staged => Action::Unstage,
                View::Head | View::Unstaged => Action::Stage,
            };
            return Some((action, Err(why)));
        }
        present(view, *offers.offers.get(block)?)
    }

    /// The block the toolbar's button acts on: the current change.
    pub(crate) fn current_hunk(&self) -> Option<(usize, usize)> {
        let targets = self.change_targets();
        let current = self.current_change()?;
        targets.get(current).map(|(file, block, _)| (*file, *block))
    }

    /// The exact offer a control is drawn for: its file, the document and
    /// the facts generation it was computed from, the block and the action.
    /// A click carries it, and a different current offer makes the click
    /// stale — review r2 `bugs+impl-9`: GPUI dispatches a click through the
    /// last frame, which can show an offer a refresh has replaced.
    pub(crate) fn hunk_token(&self, file: usize, block: usize) -> Option<HunkToken> {
        let (action, available) = self.hunk_offer(file, block)?;
        available.ok()?;
        let path = self.files.get(file)?.path.clone();
        let offers = self.hunks.offers.get(&path)?;
        Some(HunkToken {
            path,
            document: Arc::downgrade(&offers.document),
            generation: offers.generation,
            block,
            action,
        })
    }

    /// The keymap's Stage Hunk or Unstage Hunk: the current change, when it
    /// offers that action and it is available.
    pub(crate) fn hunk_action_on_current(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Some((file, block)) = self.current_hunk()
            && let Some(token) = self.hunk_token(file, block)
            && token.action == action
        {
            self.stage_or_unstage(&token, cx);
        }
    }

    /// A click on a block's hunk action: the write, or the notice that what
    /// was drawn is no longer the offer or the buffer.
    pub(crate) fn stage_or_unstage(&mut self, token: &HunkToken, cx: &mut Context<Self>) {
        // One write per file while it is unanswered, refused before the
        // file is copied — review r3 `arch+quality-5`.
        if crate::root::hunks::hunk_in_flight(cx, &self.root.join(&token.path)) {
            return;
        }
        match self.hunk_write(token, cx) {
            Ok(write) => cx.emit(DiffEvent::Hunk(write)),
            Err(()) => {
                cx.emit(DiffEvent::HunkStale);
            }
        }
    }

    fn hunk_write(&self, token: &HunkToken, cx: &gpui::App) -> Result<Write, ()> {
        let view = view_of(&self.base)
            .filter(|_| self.inspection.is_none())
            .ok_or(())?;
        let file = self
            .files
            .iter()
            .position(|f| f.path == token.path)
            .ok_or(())?;
        // The offer drawn must be the offer now.
        let now = self.hunk_token(file, token.block).ok_or(())?;
        if !now.same(token) {
            return Err(());
        }
        let path = token.path.clone();
        let document = self.documents.get(&path).ok_or(())?;
        let offers = self.hunks.offers.get(&path).ok_or(())?;
        let comparisons = offers.comparisons.as_ref().ok_or(())?;
        let shown = document.blocks.get(token.block).ok_or(())?;
        let (action, postimage) = comparisons.postimage(view, shown).ok_or(())?;
        if action != token.action {
            return Err(());
        }
        // The working text on screen must be the one there is now: the disk
        // bytes the diff read, or the buffer serialized as a save would write
        // it — a reload replaces a buffer without a Change event.
        let working_shown = matches!(view, View::Head | View::Unstaged);
        let disk = if working_shown {
            let shown_bytes = document.sources[1].text.as_bytes();
            match self.buffers.get(&path) {
                Some(serialize) => {
                    if serialize(cx) != shown_bytes {
                        return Err(());
                    }
                    None
                }
                None => {
                    let held = self.working.get(&path).cloned().flatten();
                    if held.as_deref() != Some(shown_bytes) {
                        return Err(());
                    }
                    Some(Arc::from(held.unwrap_or_default()))
                }
            }
        } else {
            None
        };
        let index = offers.index.clone().ok_or(())?;
        // `HEAD` as the offer was made over it: the shown side where the
        // diff shows `HEAD`, the facts' bytes in the Unstaged diff.
        let head = match view {
            View::Head | View::Staged => Arc::from(document.sources[0].text.as_bytes()),
            View::Unstaged => offers.head.clone().ok_or(())?,
        };
        let hunk = Hunk {
            path,
            preimage: Arc::from(index.as_bytes()),
            postimage: Arc::from(postimage.into_bytes()),
            disk,
            head,
        };
        Ok(match action {
            Action::Stage => Write::StageHunk(hunk),
            Action::Unstage => Write::UnstageHunk(hunk),
        })
    }
}

/// The offer a hunk control was drawn for — see [`DiffTab::hunk_token`].
#[derive(Clone, Debug)]
pub(crate) struct HunkToken {
    path: PathBuf,
    document: std::sync::Weak<Document>,
    generation: u64,
    block: usize,
    pub(crate) action: Action,
}

impl HunkToken {
    fn same(&self, other: &HunkToken) -> bool {
        self.path == other.path
            && self.document.ptr_eq(&other.document)
            && self.generation == other.generation
            && self.block == other.block
            && self.action == other.action
    }
}

#[cfg(test)]
mod tests;
