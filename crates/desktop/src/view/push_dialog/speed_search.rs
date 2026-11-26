//! The two Push lists share transient search, but retain independent focus.
use super::*;
use crate::speed_search::{Candidate, Search};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PushList {
    Commits,
    Files,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Key {
    Commit(String),
    File(PathBuf),
}
pub(super) struct PushSearch {
    pub list: PushList,
    pub search: Search<Key>,
    inspection: Option<String>,
}

impl PushDialog {
    fn speed_rows(&self, list: PushList, seen: &Seen) -> Vec<Candidate<Key>> {
        match list {
            PushList::Commits => seen
                .commits
                .iter()
                .enumerate()
                .map(|(i, (sha, summary))| Candidate::new(Key::Commit(sha.clone()), summary, i + 1))
                .collect(),
            PushList::Files if !self.repository_folded => seen
                .files
                .as_ref()
                .and_then(|f| f.as_ref().ok())
                .map(|files| {
                    let rows = files_tree::of(files);
                    files_tree::visible(&rows, &self.folded)
                        .into_iter()
                        .enumerate()
                        .filter_map(|(i, r)| match r {
                            Row::File { path, name, .. } => {
                                Some(Candidate::new(Key::File(path.clone()), name, i + 1))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
    pub(super) fn speed_update(
        &mut self,
        backwards: Option<bool>,
        reveal: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(speed) = self.speed.as_ref() else {
            return;
        };
        let Some(seen) = self.seen(cx) else {
            return;
        };
        if speed.list == PushList::Files && speed.inspection != seen.opened {
            self.speed = None;
            cx.notify();
            return;
        }
        let list = speed.list;
        let rows = self.speed_rows(list, &seen);
        let search = &mut self.speed.as_mut().unwrap().search;
        let previous = search.current.clone();
        if let Some(back) = backwards {
            search.step(&rows, back);
        } else {
            search.reconcile(&rows);
        }
        if let Some(row) = search.matched(&rows) {
            match &row.key {
                Key::Commit(sha) => {
                    if reveal || previous.as_ref() != Some(&row.key) {
                        self.commit_scroll.scroll_to_item(row.row);
                    }
                    if seen.selected.as_slice() != [sha.clone()] {
                        self.choose_commit(Some(sha), false, false, cx);
                    }
                }
                Key::File(path) => {
                    // Only a query edit, a step or a new match moves the
                    // selection: a redraw leaves the latest click in charge.
                    if reveal || previous.as_ref() != Some(&row.key) {
                        self.file_scroll.scroll_to_item(row.row);
                        // The match is the selection — decision 54.
                        let order: Vec<_> = rows
                            .iter()
                            .filter_map(|r| match &r.key {
                                Key::File(p) => Some(p.clone()),
                                Key::Commit(_) => None,
                            })
                            .collect();
                        self.file_selection.click(path, &order, false, false);
                        self.selected_file = self.file_selection.action_target().cloned();
                        cx.notify();
                    }
                }
            }
        }
    }
    pub(super) fn speed_query(&self, list: PushList) -> &str {
        self.speed
            .as_ref()
            .filter(|s| s.list == list)
            .map(|s| s.search.query.as_str())
            .unwrap_or("")
    }
    pub(super) fn speed_file(&self, seen: &Seen) -> Option<PathBuf> {
        let speed = self.speed.as_ref().filter(|s| s.list == PushList::Files)?;
        let rows = self.speed_rows(PushList::Files, seen);
        match &speed.search.matched(&rows)?.key {
            Key::File(p) => Some(p.clone()),
            _ => None,
        }
    }
    pub(super) fn speed_field(
        &self,
        list: PushList,
        seen: &Seen,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let speed = self.speed.as_ref().filter(|s| s.list == list);
        Some(crate::view::speed_search::field(
            speed.map(|s| s.search.query.as_str()),
            match list {
                PushList::Commits => &self.commit_focus,
                PushList::Files => &self.file_focus,
            },
            speed.is_some_and(|s| {
                !s.search.query.is_empty()
                    && s.search.matched(&self.speed_rows(list, seen)).is_none()
            }),
            self.theme,
            cx.listener(move |this, query: &String, _, cx| {
                if this.popup.is_some() {
                    return;
                }
                let Some(seen) = this.seen(cx) else {
                    return;
                };
                let current = match list {
                    PushList::Commits => seen.selected.first().cloned().map(Key::Commit),
                    PushList::Files => this.file_selection.focus.clone().map(Key::File),
                };
                let speed = this.speed.get_or_insert_with(|| PushSearch {
                    list,
                    search: Search::new("", current),
                    inspection: seen.opened,
                });
                speed.search.query = query.clone();
                this.speed_update(None, true, cx);
                cx.notify();
            }),
            cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        this.speed_update(Some(event.keystroke.key == "up"), true, cx);
                        cx.notify();
                    }
                    "enter" | "escape" | "tab" => {
                        if event.keystroke.key == "enter"
                            && list == PushList::Files
                            && let Some(seen) = this.seen(cx)
                            && let Some(path) = this.speed_file(&seen)
                        {
                            this.file_selection.focus = Some(path);
                        }
                        this.speed = None;
                        let focus = match list {
                            PushList::Commits => &this.commit_focus,
                            PushList::Files => &this.file_focus,
                        };
                        window.focus(focus, cx);
                        cx.notify();
                    }
                    _ => {}
                }
            }),
            cx.listener(move |this, _: &(), _, cx| {
                if this.speed.as_ref().is_some_and(|s| s.list == list) {
                    this.speed = None;
                    cx.notify();
                }
            }),
        ))
    }
}
