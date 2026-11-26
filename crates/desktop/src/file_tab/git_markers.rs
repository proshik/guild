//! One coalesced executor job per FileTab; no per-file thread or polling loop.
use super::*;
use crate::hunk::{Comparisons, Offer, View};
use crate::line_changes::{self, Marker};
use gpui_component::RopeExt;
use std::sync::Arc;
mod popup;

#[derive(Default)]
pub(super) struct Tracking {
    epoch: u64,
    serial: u64,
    started: bool,
    running: bool,
    again: bool,
    baseline: Option<Result<Arc<str>, ()>>,
    /// What git says about the file for its hunks, read with the baseline
    /// and dropped with it — GLD-026.
    facts: Option<Arc<crate::git::hunk::Facts>>,
    /// What each block of `comparison` offers, for the popup and the
    /// outlined markers.
    pub(super) offers: Option<Arc<Offers>>,
    pub marks: Arc<[Marker]>,
    task: Option<gpui::Task<()>>,
    comparison: Option<Arc<line_changes::Comparison>>,
    source: Option<gpui_component::Rope>,
    popup: Option<popup::HunkPopup>,
    pub(super) presentation: std::cell::Cell<u64>,
}
impl Tracking {
    pub(super) fn invalidate(&mut self) {
        self.epoch += 1;
        self.serial += 1;
        self.baseline = None;
        self.facts = None;
        self.offers = None;
        self.comparison = None;
        self.source = None;
        self.marks = Arc::from([]);
        self.started = false;
    }
}
/// A hunk write the marker popup asks the window to send — GLD-026.
pub struct HunkRequest(pub crate::git::Write);
impl gpui::EventEmitter<HunkRequest> for FileTab {}

/// What a comparison's blocks offer — GLD-026, decision 55.
pub(super) struct Offers {
    pub comparison: Arc<line_changes::Comparison>,
    pub per_block: Vec<Offer>,
    /// Parallel to the markers: the marker's block is wholly in the index,
    /// drawn as an outline.
    pub staged: Arc<[bool]>,
    pub comparisons: Option<Arc<Comparisons>>,
    pub head: Arc<str>,
    pub index: Option<Arc<str>>,
    pub eligibility: crate::git::hunk::Eligibility,
}

impl Offers {
    fn of(
        comparison: Arc<line_changes::Comparison>,
        head: Arc<str>,
        facts: &crate::git::hunk::Facts,
        working: &str,
    ) -> Self {
        let comparisons = facts
            .index
            .as_ref()
            .filter(|_| facts.eligibility == crate::git::hunk::Eligibility::Yes)
            .and_then(|index| Comparisons::new(&head, index, working))
            .map(Arc::new);
        // Whether a block is wholly staged is read before the conversion
        // check disables its action: a staged block keeps its outline when
        // its Unstage cannot be written — review r4 `bugs+impl-8`.
        let (per_block, wholly_staged): (Vec<Offer>, Vec<bool>) = comparison
            .blocks
            .iter()
            .map(|b| match &comparisons {
                Some(c) => (
                    crate::diff_tab::hunk_converted(c, View::Head, &shown(b), facts.endings),
                    c.offer(View::Head, &shown(b)) == Offer::Unstage,
                ),
                None => (Offer::Nothing, false),
            })
            .unzip();
        let staged = comparison
            .marker_blocks
            .iter()
            .map(|b| wholly_staged.get(*b).copied().unwrap_or(false))
            .collect();
        Self {
            comparison,
            per_block,
            staged,
            comparisons,
            head,
            index: facts.index.clone(),
            eligibility: facts.eligibility,
        }
    }
}

/// A marker block as the comparisons name one: HEAD rows, then buffer rows.
pub(super) fn shown(block: &line_changes::Block) -> crate::diff::document::Block {
    crate::diff::document::Block {
        old: block.old_rows.clone(),
        new: block.rows.clone(),
    }
}

impl FileTab {
    /// Git/watch callers invalidate the immutable baseline, even for an unsaved
    /// buffer. A stale response may neither restore marks nor become its base.
    pub fn refresh_git_baseline(&mut self, cx: &mut Context<Self>) {
        self.git_markers.invalidate();
        self.request_git_markers(cx);
    }
    fn marker_source_supported(&self, cx: &App) -> bool {
        !self.read_only
            && !self.without_git
            && !self.relocating
            && (self.image.is_none() || self.image_source_visible())
            && self.editor.as_ref().is_some_and(|editor| {
                let text = editor.read(cx).text();
                text.len() <= line_changes::BYTE_LIMIT
                    && text.lines_len() <= line_changes::LINE_LIMIT
            })
    }
    pub(super) fn request_git_markers(&mut self, cx: &mut Context<Self>) {
        self.git_markers.serial += 1;
        self.git_markers.marks = Arc::from([]);
        self.git_markers.comparison = None;
        self.git_markers.source = None;
        self.git_markers.offers = None;
        if !self.marker_source_supported(cx) {
            // Do not retain a huge Rope or issue Git reads for unsupported views.
            self.git_markers.started = false;
            cx.notify();
            return;
        }
        self.git_markers.started = true;
        if self.git_markers.running {
            self.git_markers.again = true;
            cx.notify();
            return;
        }
        self.git_markers.running = true;
        self.git_markers.again = false;
        self.git_markers.task = Some(cx.spawn(async move |owner, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(75))
                .await;
            let input = owner
                .update(cx, |file, cx| {
                    if !file.marker_source_supported(cx) {
                        return None;
                    }
                    // All intents received during the debounce are represented by
                    // this latest snapshot. Only edits arriving after it need a rerun.
                    file.git_markers.again = false;
                    Some((
                        file.git_markers.epoch,
                        file.git_markers.serial,
                        file.root.clone(),
                        file.relative.clone(),
                        file.git_markers.baseline.clone(),
                        file.editor.as_ref().unwrap().read(cx).text().clone(),
                        (file.git_markers.facts.clone(), file.line_ending),
                    ))
                })
                .ok()
                .flatten();
            if let Some((epoch, serial, root, path, cached, text, (facts, ending))) = input {
                let answer = cx
                    .background_executor()
                    .spawn(async move {
                        let baseline = cached.unwrap_or_else(|| {
                            crate::git::bounded_head_text(&root, &path, line_changes::BYTE_LIMIT)
                                .map_err(|_| ())
                                .and_then(|bytes| {
                                    let bytes = bytes.unwrap_or_default();
                                    if bytes.contains(&0) {
                                        return Err(());
                                    }
                                    String::from_utf8(bytes)
                                        .map(Arc::<str>::from)
                                        .map_err(|_| ())
                                })
                        });
                        // The Rope was checked before cloning; flatten only on the
                        // worker, and never above the byte/line budget.
                        let flat = text.to_string();
                        let comparison = baseline
                            .as_ref()
                            .ok()
                            .and_then(|base| line_changes::comparison(base, &flat))
                            .map(Arc::new);
                        // The facts go with the baseline: read once, never
                        // per keystroke. The working text is the buffer in
                        // the file's own line endings, what a save writes.
                        let facts = facts.unwrap_or_else(|| {
                            Arc::new(crate::git::hunk::facts(
                                &root,
                                &path,
                                line_changes::BYTE_LIMIT,
                                line_changes::LINE_LIMIT,
                            ))
                        });
                        let offers = baseline.as_ref().ok().zip(comparison.clone()).map(
                            |(head, comparison)| {
                                Arc::new(Offers::of(
                                    comparison,
                                    head.clone(),
                                    &facts,
                                    &ending.apply(&flat),
                                ))
                            },
                        );
                        (baseline, comparison, text, facts, offers)
                    })
                    .await;
                let _ = owner.update(cx, |file, cx| {
                    let marks = answer
                        .1
                        .as_ref()
                        .map(|c| c.markers.clone())
                        .unwrap_or_default();
                    file.git_markers_landed(epoch, serial, answer.0, marks, cx);
                    if file.git_markers.epoch == epoch && file.marker_source_supported(cx) {
                        file.git_markers.facts = Some(answer.3);
                    }
                    if file.git_markers.epoch == epoch
                        && file.git_markers.serial == serial
                        && file.marker_source_supported(cx)
                    {
                        file.git_markers.comparison = answer.1;
                        file.git_markers.source = Some(answer.2);
                        file.git_markers.offers = answer.4;
                    }
                });
            }
            let _ = owner.update(cx, |file, cx| {
                file.git_markers.running = false;
                file.git_markers.task = None;
                // Leaving Source can skip snapshot capture or answer installation.
                // Returning must start again, even if an older base was cached.
                if !file.marker_source_supported(cx) {
                    file.git_markers.started = false;
                }
                if std::mem::take(&mut file.git_markers.again) {
                    file.request_git_markers(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn git_markers_landed(
        &mut self,
        epoch: u64,
        serial: u64,
        baseline: Result<Arc<str>, ()>,
        marks: Vec<Marker>,
        cx: &mut Context<Self>,
    ) {
        if self.git_markers.epoch == epoch && self.marker_source_supported(cx) {
            self.git_markers.baseline = Some(baseline);
            if self.git_markers.serial == serial {
                self.git_markers.marks = marks.into();
            }
        }
    }
    pub(super) fn ensure_git_markers(&mut self, cx: &mut Context<Self>) {
        if !self.git_markers.started && self.marker_source_supported(cx) {
            self.request_git_markers(cx);
        }
    }
    pub(super) fn git_marker_lane(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<super::gutter::MarkerLane> {
        if !self.marker_source_supported(cx) || !matches!(self.git_markers.baseline, Some(Ok(_))) {
            return None;
        }
        let staged = self
            .git_markers
            .offers
            .as_ref()
            .filter(|o| {
                self.git_markers
                    .comparison
                    .as_ref()
                    .is_some_and(|c| Arc::ptr_eq(c, &o.comparison))
            })
            .map(|o| o.staged.clone())
            .unwrap_or_else(|| Arc::from([]));
        Some(super::gutter::MarkerLane {
            staged,
            items: self.git_markers.marks.clone(),
            width: theme::space::GIT_MARKER_LANE.to_pixels(window.rem_size()),
            stripe: theme::space::GIT_MARKER_STRIPE.to_pixels(window.rem_size()),
            deleted: theme::space::GIT_MARKER_DELETED.to_pixels(window.rem_size()),
            colors: [
                self.look.theme.accent.green,
                self.look.theme.accent.brass,
                self.look.theme.accent.rust,
            ],
        })
    }
}

#[cfg(test)]
mod tests;
