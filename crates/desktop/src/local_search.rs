//! Guild's local document search. The source editor owns text and Undo; this view owns chrome.
use crate::view::tooltip::Hint as _;
use crate::{
    document_search::{self, Match},
    i18n::Ui,
    search::{Query, Toggle},
    theme::{self, Theme},
};
use gpui::EntityInputHandler;
use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Focusable, MouseButton, Window, div, rgb};
use gpui_component::input::{
    EditorState, InputEvent, RangeDecoration, RangeDecorationCollection, RangeDecorationStyle,
    Textarea, TextareaState,
};
use sha2::{Digest, Sha256};
use std::ops::Range;

#[derive(Clone)]
struct ScopeEdit {
    before: [u8; 32],
    after: [u8; 32],
    before_range: Range<usize>,
    after_range: Range<usize>,
}

pub struct SearchBar {
    source: Entity<EditorState>,
    pub open: bool,
    pub replace: bool,
    pub query: Query,
    pub replacement: String,
    pub preserve_case: bool,
    pub found: Vec<Match>,
    current: usize,
    excluded: Vec<Range<usize>>,
    selection: Range<usize>,
    in_selection: bool,
    snapshot: String,
    scope_undo: std::collections::VecDeque<ScopeEdit>,
    scope_redo: Vec<std::collections::VecDeque<ScopeEdit>>,
    replaying_scope: bool,
    error: Option<String>,
    replace_error: Option<String>,
    history: Vec<String>,
    recall: crate::find_in_files::Recall,
    history_open: bool,
    more_open: bool,
    scroll_to_results: bool,
    multiline: Option<Entity<TextareaState>>,
    query_focus: FocusHandle,
    replacement_focus: FocusHandle,
    select_query: bool,
    sync_multiline_query: bool,
    decorations: RangeDecorationCollection,
    region: RangeDecorationCollection,
    source_edges: RangeDecorationCollection,
    theme: Theme,
    ui: Ui,
    readonly: bool,
    _changes: Option<gpui::Subscription>,
    _multiline: Option<gpui::Subscription>,
}
impl SearchBar {
    pub fn new(
        source: Entity<EditorState>,
        theme: Theme,
        ui: Ui,
        readonly: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let decorations = source.update(cx, |source, cx| {
            source.create_range_decorations_collection(Vec::new(), cx)
        });
        let region = source.update(cx, |source, cx| {
            source.create_range_decorations_collection(Vec::new(), cx)
        });
        let source_edges = source.update(cx, |source, cx| {
            source.create_range_decorations_collection(Vec::new(), cx)
        });
        let changes = cx.subscribe(&source, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && this.open {
                this.excluded.clear();
                this.refresh(cx);
            }
        });
        cx.on_release(|this, cx| {
            this.decorations.dispose(cx);
            this.region.dispose(cx);
            this.source_edges.dispose(cx);
        })
        .detach();
        Self {
            source,
            open: false,
            replace: false,
            query: Query::default(),
            replacement: String::new(),
            preserve_case: false,
            found: Vec::new(),
            current: 0,
            excluded: Vec::new(),
            selection: 0..0,
            in_selection: false,
            snapshot: String::new(),
            scope_undo: Default::default(),
            scope_redo: Vec::new(),
            replaying_scope: false,
            error: None,
            replace_error: None,
            history: Vec::new(),
            recall: Default::default(),
            history_open: false,
            more_open: false,
            scroll_to_results: true,
            multiline: None,
            query_focus: cx.focus_handle(),
            replacement_focus: cx.focus_handle(),
            select_query: false,
            sync_multiline_query: false,
            decorations,
            region,
            source_edges,
            theme,
            ui,
            readonly,
            _changes: Some(changes),
            _multiline: None,
        }
    }
    fn tr(&self, en: &'static str, ru: &'static str) -> &'static str {
        if self.ui.0 == guild_session::Language::Russian {
            ru
        } else {
            en
        }
    }
    pub fn restyle(&mut self, theme: Theme, ui: Ui, cx: &mut Context<Self>) {
        self.theme = theme;
        self.ui = ui;
        self.refresh(cx);
    }
    pub fn show(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.scope_undo.clear();
            self.scope_redo.clear();
            self.snapshot = self.source.read(cx).text().to_string();
            self.selection = self.source.read(cx).selected_range();
            self.track_region(cx);
            let selected = self.source.read(cx).selected_text().to_string();
            if !selected.is_empty() {
                self.query.text = selected;
            }
        }
        self.open = true;
        self.replace = replace && !self.readonly;
        self.select_query = true;
        self.sync_multiline_query = true;
        self.focus_query(window, cx);
        if self.query.text.contains('\n') && self.multiline.is_none() {
            self.toggle_multiline(window, cx);
        }
        self.refresh(cx);
    }
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.remember();
        self.decorations.clear(cx);
        window.focus(&self.source.read(cx).focus_handle(cx), cx);
        cx.notify();
    }
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_open || self.more_open {
            self.history_open = false;
            self.more_open = false;
            cx.notify();
        } else {
            self.close(window, cx);
        }
    }
    fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = &self.multiline {
            input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.query_focus, cx);
        }
        cx.notify();
    }
    fn focus_after_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.replace {
            window.focus(&self.replacement_focus, cx);
        } else {
            window.focus(&self.source.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn remember(&mut self) {
        crate::find_in_files::remember(&mut self.history, &self.query.text, 20);
    }
    pub fn set_query(&mut self, query: Query, cx: &mut Context<Self>) {
        self.history_open = false;
        self.more_open = false;
        self.query = query;
        self.recall = Default::default();
        self.sync_multiline_query = true;
        self.excluded.clear();
        self.current = 0;
        self.refresh(cx);
    }
    fn track_region(&self, cx: &mut Context<Self>) {
        // Two edge probes distinguish an actual spanning source edit from a
        // deletion before a selection that legitimately becomes the whole text.
        let edges = if self.snapshot.is_empty() {
            Vec::new()
        } else {
            let first = self.snapshot.chars().next().unwrap().len_utf8();
            let last = self.snapshot.char_indices().next_back().unwrap().0;
            vec![
                RangeDecoration::new(0..first).with_color(gpui::transparent_black()),
                RangeDecoration::new(last..self.snapshot.len())
                    .with_color(gpui::transparent_black()),
            ]
        };
        self.source_edges.set(edges, cx);
        self.region.set(
            vec![
                RangeDecoration::new(self.selection.clone()).with_color(gpui::transparent_black()),
            ],
            cx,
        );
    }
    fn record_scope_edit(&mut self, text: String, mapped: Range<usize>) {
        if text != self.snapshot {
            self.scope_undo.push_back(ScopeEdit {
                before: Sha256::digest(self.snapshot.as_bytes()).into(),
                after: Sha256::digest(text.as_bytes()).into(),
                before_range: self.selection.clone(),
                after_range: mapped.clone(),
            });
            // The pinned editor retains at most 1000 transactions of 1000 changes.
            if self.scope_undo.len() > 1_000_000 {
                self.scope_undo.pop_front();
            }
            self.scope_redo.clear();
        }
        self.snapshot = text;
        self.selection = mapped;
    }
    pub(crate) fn begin_history(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
        self.replaying_scope = true;
    }
    pub(crate) fn finish_history(&mut self, redo: bool, cx: &mut Context<Self>) {
        self.replaying_scope = false;
        if !self.open {
            return;
        }
        let text = self.source.read(cx).text().to_string();
        if text != self.snapshot {
            let before: [u8; 32] = Sha256::digest(self.snapshot.as_bytes()).into();
            let after: [u8; 32] = Sha256::digest(text.as_bytes()).into();
            let restored = if redo {
                self.scope_redo.pop().and_then(|group| {
                    if group.front()?.before != before || group.back()?.after != after {
                        return None;
                    }
                    let range = group.back()?.after_range.clone();
                    self.scope_undo.extend(group);
                    Some(range)
                })
            } else {
                let mut cursor = before;
                let mut boundary = None;
                for (index, edit) in self.scope_undo.iter().enumerate().rev() {
                    if edit.after != cursor {
                        break;
                    }
                    cursor = edit.before;
                    if cursor == after {
                        boundary = Some(index);
                        break;
                    }
                }
                boundary.map(|index| {
                    let group = self.scope_undo.split_off(index);
                    let range = group[0].before_range.clone();
                    self.scope_redo.push(group);
                    range
                })
            };
            self.selection = restored.unwrap_or_else(|| {
                self.scope_undo.clear();
                self.scope_redo.clear();
                self.tracked_scope_after(&text, cx)
            });
            self.snapshot = text;
            self.track_region(cx);
        }
        self.refresh(cx);
    }
    pub(crate) fn source_reloaded(&mut self, cx: &mut Context<Self>) {
        let text = self.source.read(cx).text().to_string();
        self.selection = map_selection(&self.snapshot, &text, self.selection.clone());
        self.snapshot = text;
        self.scope_undo.clear();
        self.scope_redo.clear();
        self.track_region(cx);
        self.refresh(cx);
    }
    pub(crate) fn dispose(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self._changes.take();
        self._multiline.take();
        self.decorations.dispose(cx);
        self.region.dispose(cx);
        self.source_edges.dispose(cx);
    }
    fn tracked_scope_after(&self, text: &str, cx: &gpui::App) -> Range<usize> {
        let tracked = self.region.get_ranges(cx).into_iter().next();
        let edges = self.source_edges.get_ranges(cx);
        let spanning = !text.is_empty()
            && edges.len() == 2
            && edges.iter().all(|edge| *edge == (0..text.len()));
        if spanning {
            document_search::remap_range(
                self.selection.clone(),
                &[Match {
                    range: 0..self.snapshot.len(),
                    replacement: text.into(),
                }],
            )
        } else if let Some(range) = tracked {
            range
        } else {
            // An absent tracker means the selected span was erased. Only an
            // observed Undo transition may resurrect it; a text diff must not.
            let at = map_selection(&self.snapshot, text, self.selection.clone())
                .start
                .min(text.len());
            at..at
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            self.decorations.clear(cx);
            return;
        }
        if self.replaying_scope {
            return;
        }
        self.replace_error = None;
        let text = self.source.read(cx).text().to_string();
        if text != self.snapshot {
            let mapped = self.tracked_scope_after(&text, cx);
            self.record_scope_edit(text, mapped);
            self.track_region(cx);
        }
        match document_search::find(&self.snapshot, &self.query) {
            Ok(found) => {
                self.error = None;
                self.found = found
                    .into_iter()
                    .filter(|m| {
                        (!self.in_selection
                            || (m.range.start >= self.selection.start
                                && m.range.end <= self.selection.end))
                            && !self.excluded.contains(&m.range)
                    })
                    .collect();
            }
            Err(error) => {
                self.error = Some(error);
                self.found.clear();
            }
        }
        self.current = self.current.min(self.found.len().saturating_sub(1));
        let color = gpui::Hsla::from(rgb(self.theme.accent.highlight));
        self.decorations.set(
            self.found
                .iter()
                .map(|m| {
                    RangeDecoration::new(m.range.clone())
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(color)
                })
                .collect(),
            cx,
        );
        cx.notify();
    }
    fn navigate(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.remember();
        if self.found.is_empty() {
            return;
        }
        self.current = if backwards {
            (self.current + self.found.len() - 1) % self.found.len()
        } else {
            (self.current + 1) % self.found.len()
        };
        self.reveal(window, cx);
    }
    fn reveal(&self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(found) = self.found.get(self.current) {
            self.source.update(cx, |source, cx| {
                source.set_selected_range(found.range.clone(), cx)
            });
        }
    }
    pub fn replace_matches(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.readonly || self.error.is_some() {
            return;
        }
        self.remember();
        if self.source.read(cx).text() != self.snapshot.as_str() {
            self.refresh(cx);
            return;
        }
        let selected = if all {
            self.found.clone()
        } else {
            self.found.get(self.current).cloned().into_iter().collect()
        };
        if selected.is_empty() {
            return;
        }
        let selected = match document_search::selected_plan(
            &self.snapshot,
            &self.query,
            &self.replacement,
            self.preserve_case,
            &selected,
        ) {
            Ok(selected) => selected,
            Err(error) => {
                self.replace_error = Some(error);
                cx.notify();
                return;
            }
        };
        let next_offset = selected
            .last()
            .map(|edit| edit.range.start + edit.replacement.len());
        self.apply_document_edits(&selected, window, cx);
        self.excluded.clear();
        if !all && let Some(next_offset) = next_offset {
            self.current = self
                .found
                .iter()
                .position(|found| found.range.start >= next_offset)
                .unwrap_or(0);
            self.reveal(window, cx);
        }
    }
    pub(crate) fn apply_document_edits(
        &mut self,
        edits: &[Match],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open {
            self.refresh(cx);
        }
        let mapped = document_search::remap_range(self.selection.clone(), edits);
        self.source.update(cx, |source, cx| {
            document_search::apply_matches(source, edits, window, cx)
        });
        let text = self.source.read(cx).text().to_string();
        if self.open {
            self.record_scope_edit(text, mapped);
            self.track_region(cx);
        } else {
            self.snapshot = text;
        }
        self.refresh(cx);
    }
    fn toggle_multiline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline.take().is_some() {
            self._multiline = None;
            window.focus(&self.query_focus, cx);
        } else {
            let state =
                cx.new(|cx| TextareaState::new(window, cx).default_value(self.query.text.clone()));
            self._multiline = Some(cx.subscribe_in(
                &state,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur)
                        && !input.read(cx).focus_handle(cx).is_focused(window)
                        && (this.history_open || this.more_open)
                    {
                        this.history_open = false;
                        this.more_open = false;
                        cx.notify();
                    }
                    if matches!(event, InputEvent::Change) {
                        let mut query = this.query.clone();
                        query.text = input.read(cx).value().to_string();
                        this.set_query(query, cx);
                        if this.scroll_to_results {
                            this.reveal(window, cx);
                        }
                    }
                },
            ));
            state.update(cx, |state, cx| state.focus(window, cx));
            self.multiline = Some(state);
        }
        cx.notify();
    }
    fn button(
        &self,
        id: &'static str,
        label: impl Into<gpui::SharedString>,
        active: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let label = label.into();
        let hint = match id {
            "local-case" => self.tr("Match case (⌥C)", "Учитывать регистр (⌥C)"),
            "local-word" => self.tr("Whole words (⌥W)", "Слова целиком (⌥W)"),
            "local-regex" => self.tr("Regular expression (⌥X)", "Регулярное выражение (⌥X)"),
            "local-history" => self.tr("Search history (⌥↑ / ⌥↓)", "История запросов (⌥↑ / ⌥↓)"),
            "local-previous" => {
                self.tr("Previous match (⇧Enter)", "Предыдущее совпадение (⇧Enter)")
            }
            "local-next" => self.tr("Next match (Enter)", "Следующее совпадение (Enter)"),
            "local-expand" => self.tr("Toggle Replace", "Показать или скрыть замену"),
            "local-close" => self.tr("Close Find (Esc)", "Закрыть поиск (Esc)"),
            "local-more" => self.tr("More search options", "Другие параметры поиска"),
            "local-preserve" => self.tr(
                "Preserve case — literal search only",
                "Сохранять регистр — буквальный поиск",
            ),
            "local-multiline" => self.tr("Multiline query", "Многострочный запрос"),
            _ => label.as_ref(),
        }
        .to_owned();
        let icon = match id {
            "local-history" => Some("ui/rail-search.svg"),
            "local-case" | "local-preserve" => Some("icons/case-sensitive.svg"),
            "local-previous" => Some("ui/arrow-up.svg"),
            "local-next" => Some("ui/arrow-down.svg"),
            "local-expand" => Some(if active {
                "ui/chevron-down.svg"
            } else {
                "ui/chevron-right.svg"
            }),
            "local-close" => Some("icons/close.svg"),
            "local-more" => Some("ui/head-more.svg"),
            _ => None,
        };
        let content = match icon {
            Some(icon) => crate::icons::toolbar(icon)
                .text_color(rgb(self.theme.text.secondary))
                .flex_none()
                .into_any_element(),
            None => label.into_any_element(),
        };
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .capture_any_mouse_down(|_, window, _| window.prevent_default())
            .hint_delay()
            .tooltip(move |_, cx| {
                cx.new(|_| crate::view::tooltip::Tooltip::text(&hint, theme))
                    .into()
            })
            .flex_none()
            .min_w(theme::space::HEAD_BUTTON)
            .px(theme::space::CARD_GAP)
            .h(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(self.theme.text.secondary))
            .when(active, |this| this.bg(rgb(self.theme.accent.brass_bg)))
            .when(!active, |this| {
                this.hover(|this| {
                    this.bg(crate::theme::control_hover(
                        self.theme,
                        self.theme.surface.panel,
                    ))
                })
            })
            .child(content)
    }
}
impl Render for SearchBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        if std::mem::take(&mut self.sync_multiline_query)
            && let Some(input) = &self.multiline
        {
            input.update(cx, |input, cx| {
                if input.value().as_ref() != self.query.text {
                    input.set_value(self.query.text.clone(), window, cx);
                }
            });
        }
        let theme = self.theme;
        let weak = cx.entity().downgrade();
        let keys = weak.clone();
        let query = if let Some(input) = &self.multiline {
            if std::mem::take(&mut self.select_query) {
                input.update(cx, |input, cx| {
                    input.set_selected_range(0..self.query.text.len(), cx)
                });
            }
            div()
                .key_context("guild-search-multiline")
                .child(
                    Textarea::new(input)
                        .appearance(false)
                        .h(theme::space::HEAD_BUTTON * 3.),
                )
                .into_any_element()
        } else {
            let select = std::mem::take(&mut self.select_query);
            crate::text_field::TextField::new(
                "local-query",
                self.query.text.clone(),
                move |text, window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        let mut query = this.query.clone();
                        query.text = text.clone();
                        this.set_query(query, cx);
                        if this.scroll_to_results {
                            this.reveal(window, cx);
                        }
                    });
                },
            )
            .focus(&self.query_focus)
            .selected(select)
            .padded()
            .on_blur(cx.listener(|this, _: &(), _, cx| {
                if this.history_open || this.more_open {
                    this.history_open = false;
                    this.more_open = false;
                    cx.notify();
                }
            }))
            .placeholder(self.tr("Find", "Найти"))
            .on_key(move |event, window, cx| {
                let _ = keys.update(cx, |this, cx| match event.keystroke.key.as_str() {
                    "escape" => {
                        this.escape(window, cx);
                        cx.stop_propagation();
                    }
                    "enter" => {
                        this.navigate(event.keystroke.modifiers.shift, window, cx);
                        cx.stop_propagation();
                    }
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            window.focus(&this.source.read(cx).focus_handle(cx), cx);
                        } else {
                            this.focus_after_query(window, cx);
                        }
                    }
                    _ => {
                        if let Some(toggle) = crate::find_in_files::toggle(&event.keystroke) {
                            toggle.turn(&mut this.query);
                            this.refresh(cx);
                            cx.stop_propagation();
                        }
                    }
                });
            })
            .into_any_element()
        };
        let mut field = div()
            .text_align(gpui::TextAlign::Left)
            .debug_selector(|| "local-query-field".into())
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .border_1()
            .border_color(rgb(if self.error.is_some() {
                theme.accent.rust
            } else {
                theme.surface.divider
            }))
            .rounded(theme::radius::ROW)
            .child(
                self.button("local-history", "", self.history_open)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.history_open = !this.history_open;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .key_context("guild-local-query")
                    .track_focus(&self.query_focus)
                    .flex_1()
                    .min_w_0()
                    .child(query),
            )
            .child(
                self.button("local-multiline", "↵", self.multiline.is_some())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.toggle_multiline(window, cx)),
                    ),
            );
        for (id, label, toggle) in [
            ("local-case", "Aa", Toggle::MatchCase),
            ("local-word", "W", Toggle::WholeWord),
            ("local-regex", ".*", Toggle::Regex),
        ] {
            field = field.child(
                self.button(id, label, toggle.is_on(&self.query))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            toggle.turn(&mut this.query);
                            this.excluded.clear();
                            this.refresh(cx);
                        }),
                    ),
            );
        }
        let count = if self.query.is_empty() {
            String::new()
        } else if self.error.is_some() {
            self.tr("Search error", "Ошибка поиска").into()
        } else {
            if self.found.is_empty() {
                self.tr("No matches", "Нет совпадений").into()
            } else {
                format!("{} / {}", self.current + 1, self.found.len())
            }
        };
        let disclosure = self.button("local-expand", "", self.replace).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.replace = !this.replace;
                cx.notify();
            }),
        );
        // One shared field column determines both rows' width and left edge.
        let mut fields = div()
            .flex_1()
            .min_w_0()
            .max_w(theme::space::LOCAL_SEARCH_FIELD)
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(field);
        let first = div()
            .flex()
            .flex_wrap()
            .items_center()
            .min_h(theme::space::HEAD_TAB)
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(if self.error.is_some() {
                        theme.accent.rust
                    } else {
                        theme.text.faint
                    }))
                    .child(count),
            )
            .child(self.button("local-previous", "↑", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.navigate(true, window, cx)),
            ))
            .child(self.button("local-next", "↓", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.navigate(false, window, cx)),
            ))
            .child(
                self.button(
                    "local-selection",
                    self.tr("Selection", "Выделение"),
                    self.in_selection,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.in_selection = !this.in_selection;
                        this.refresh(cx);
                    }),
                ),
            )
            .child(
                self.button("local-more", "…", self.more_open)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.more_open = !this.more_open;
                            cx.notify();
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(self.button("local-close", "×", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close(window, cx)),
            ));
        let mut controls = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(first);
        let mut bar = div()
            .relative()
            .key_context("guild-local-search")
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(toggle) = crate::find_in_files::toggle(&event.keystroke) {
                    toggle.turn(&mut this.query);
                    this.excluded.clear();
                    this.refresh(cx);
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "tab"
                    && let Some(input) = &this.multiline
                    && input.read(cx).focus_handle(cx).is_focused(window)
                    && !input.update(cx, |input, cx| {
                        input.marked_text_range(window, cx).is_some()
                    })
                {
                    if event.keystroke.modifiers.shift {
                        window.focus(&this.source.read(cx).focus_handle(cx), cx);
                    } else {
                        this.focus_after_query(window, cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                let held = event.keystroke.modifiers;
                if held.alt
                    && this.query_focus.contains_focused(window, cx)
                    && !held.platform
                    && !held.control
                    && matches!(event.keystroke.key.as_str(), "up" | "down")
                {
                    let next = if event.keystroke.key == "up" {
                        this.recall.older(&this.history, &this.query.text)
                    } else {
                        this.recall.newer(&this.history)
                    };
                    if let Some(next) = next {
                        this.query.text = next;
                        this.sync_multiline_query = true;
                        this.select_query = true;
                        this.refresh(cx);
                    }
                    cx.stop_propagation();
                }
                if event.keystroke.key == "escape"
                    && let Some(input) = &this.multiline
                    && !input.update(cx, |input, cx| {
                        input.marked_text_range(window, cx).is_some()
                    })
                {
                    this.escape(window, cx);
                    cx.stop_propagation();
                }
            }))
            .flex_none()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .p(theme::space::CARD_GAP)
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .bg(rgb(theme.surface.panel))
            .text_size(theme::size::META);
        if self.replace {
            let weak = cx.entity().downgrade();
            let field = crate::text_field::TextField::new(
                "local-replacement",
                self.replacement.clone(),
                move |text, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.replacement = text.clone();
                        this.refresh(cx);
                    });
                },
            )
            .padded()
            .on_blur(cx.listener(|this, _: &(), _, cx| {
                if this.history_open || this.more_open {
                    this.history_open = false;
                    this.more_open = false;
                    cx.notify();
                }
            }))
            .focus(&self.replacement_focus)
            .placeholder(self.tr("Replace with", "Заменить на"))
            .on_key(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => this.escape(window, cx),
                    "enter" => this.replace_matches(false, window, cx),
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            this.focus_query(window, cx);
                        } else {
                            window.focus(&this.source.read(cx).focus_handle(cx), cx);
                        }
                    }
                    _ => {}
                }
            }));
            fields = fields.child(
                div()
                    .debug_selector(|| "local-replacement-field".into())
                    .w_full()
                    .min_w_0()
                    .text_align(gpui::TextAlign::Left)
                    .border_1()
                    .border_color(rgb(theme.surface.divider))
                    .rounded(theme::radius::ROW)
                    .flex()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(field))
                    .child(
                        self.button(
                            "local-preserve",
                            "aA",
                            self.preserve_case && !self.query.regular_expression,
                        )
                        .when(self.query.regular_expression, |this| this.opacity(0.4))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                if !this.query.regular_expression {
                                    this.preserve_case = !this.preserve_case;
                                    this.refresh(cx);
                                }
                            }),
                        ),
                    ),
            );
            controls = controls.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .min_h(theme::space::HEAD_TAB)
                    .gap(theme::space::CARD_GAP)
                    .child(
                        self.button("local-replace", self.tr("Replace", "Заменить"), false)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.replace_matches(false, window, cx)
                                }),
                            ),
                    )
                    .child(
                        self.button(
                            "local-replace-all",
                            self.tr("Replace All", "Заменить все"),
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.replace_matches(true, window, cx)
                            }),
                        ),
                    )
                    .child(
                        self.button("local-exclude", self.tr("Exclude", "Исключить"), false)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    if let Some(found) = this.found.get(this.current) {
                                        this.excluded.push(found.range.clone());
                                    }
                                    this.refresh(cx);
                                }),
                            ),
                    ),
            );
        }
        bar = bar.child(
            div()
                .flex()
                .items_start()
                .gap(theme::space::CARD_GAP)
                .when(!self.readonly, |row| {
                    row.child(
                        div()
                            .w(theme::space::HEAD_BUTTON)
                            .flex_none()
                            .child(disclosure),
                    )
                })
                .child(fields)
                .child(controls),
        );
        if let Some(error) = self.error.as_ref().or(self.replace_error.as_ref()) {
            bar = bar.child(
                div()
                    .w_full()
                    .text_color(rgb(theme.accent.rust))
                    .child(error.clone()),
            );
        }
        if self.history_open {
            let mut menu = crate::view::popup::card_look(theme)
                .id("local-history-menu")
                .absolute()
                .top_full()
                .left_0()
                .w(theme::space::LOCAL_SEARCH_FIELD)
                .max_h(theme::space::EVERYWHERE_LIST)
                .overflow_y_scroll()
                .p(theme::space::MENU_PADDING)
                .occlude();
            for (at, entry) in self.history.clone().into_iter().enumerate() {
                menu = menu.child(
                    div()
                        .id(("local-history-entry", at))
                        .cursor_pointer()
                        .p(theme::space::CARD_GAP)
                        .hover(|this| {
                            this.bg(crate::theme::control_hover(theme, theme.surface.card))
                        })
                        .child(entry.clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.query.text = entry.clone();
                                this.sync_multiline_query = true;
                                this.history_open = false;
                                this.select_query = true;
                                this.focus_query(window, cx);
                                this.refresh(cx);
                            }),
                        ),
                );
            }
            if self.history.is_empty() {
                menu = menu.child(self.tr("No recent searches", "Нет недавних запросов"));
            }
            bar = bar.child(gpui::deferred(menu));
        }
        if self.more_open {
            let menu = crate::view::popup::card_look(theme).absolute().top_full().right_0().w(theme::space::LOCAL_SEARCH_FIELD).p(theme::space::MENU_PADDING).occlude()
                .child(self.button("local-scroll", self.tr("Scroll to results while typing", "Прокручивать к результатам при вводе"), self.scroll_to_results).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {this.scroll_to_results = !this.scroll_to_results; this.more_open = false; cx.notify();})))
                .child(div().p(theme::space::CARD_GAP).text_color(rgb(theme.text.faint)).child(self.tr("Regex: Rust syntax; lookarounds and backreferences are unsupported. $1 / ${name} / $$ in replacements. Preserve case applies to literal search.", "Regex: синтаксис Rust; lookaround и обратные ссылки не поддерживаются. Замены: $1 / ${name} / $$. Регистр сохраняется при буквальном поиске.")));
            bar = bar.child(gpui::deferred(menu));
        }
        bar.into_any_element()
    }
}

fn map_selection(before: &str, after: &str, range: Range<usize>) -> Range<usize> {
    let mut start = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(start) || !after.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = before[start..]
        .bytes()
        .rev()
        .zip(after[start..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    document_search::remap_range(
        range,
        &[Match {
            range: start..before.len() - suffix,
            replacement: after[start..after.len() - suffix].into(),
        }],
    )
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[gpui::test]
    fn closed_search_restyle_keeps_decorations_empty(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                crate::editor::init(cx);
                let source = cx.new(|cx| EditorState::new(window, cx).default_value("cat cat"));
                let search = cx.new(|cx| {
                    SearchBar::new(
                        source,
                        Theme::default(),
                        Ui(guild_session::Language::English),
                        false,
                        cx,
                    )
                });
                search.update(cx, |search, cx| {
                    search.show(false, window, cx);
                    search.set_query(
                        Query {
                            text: "cat".into(),
                            ..Default::default()
                        },
                        cx,
                    );
                    assert_eq!(search.decorations.get_ranges(cx).len(), 2);
                    search.close(window, cx);
                    search.restyle(Theme::default(), Ui(guild_session::Language::English), cx);
                    assert!(search.decorations.get_ranges(cx).is_empty());
                });
            })
            .unwrap();
    }
}
