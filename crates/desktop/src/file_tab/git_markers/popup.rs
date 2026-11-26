//! One source-owned popup over an immutable, bounded HEAD comparison.
use super::*;
use crate::editor::{EditorReveal, Occurrences};
use crate::hunk::{Action, View};
use crate::view::tooltip::Hint as _;
use gpui::{AnyElement, Bounds, Pixels, Point, anchored, deferred, point};
use gpui_component::input::Position;
use std::rc::Rc;

pub(super) struct HunkPopup {
    comparison: Arc<line_changes::Comparison>,
    selected: usize,
    presentation: u64,
    at: Point<Pixels>,
    focus: FocusHandle,
    preview: Option<Entity<EditorState>>,
    occurrences: Option<Entity<Occurrences>>,
}
impl FileTab {
    fn comparison_current(&self, comparison: &Arc<line_changes::Comparison>, cx: &App) -> bool {
        self.marker_source_supported(cx)
            && self.drawn_at.get().is_some()
            && self
                .git_markers
                .comparison
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, comparison))
            && self.editor.as_ref().is_some_and(|editor| {
                editor.read(cx).is_editable()
                    && self.git_markers.source.as_ref().is_some_and(|source| {
                        ropey::extra::esoterica::ropes_are_instances(source, editor.read(cx).text())
                    })
            })
    }
    pub(in crate::file_tab) fn git_marker_click(
        &self,
        cx: &Context<Self>,
    ) -> Option<super::super::gutter::MarkerClick> {
        let comparison = self.git_markers.comparison.clone()?;
        let owner = cx.weak_entity();
        let font = self.look.font_size;
        let presentation = self.git_markers.presentation.get();
        let family = self.look.family.clone();
        Some(Rc::new(
            move |marker, bounds: Bounds<Pixels>, window, cx| {
                let _ = owner.update(cx, |file, cx| {
                    if file.git_markers.presentation.get() != presentation
                        || file.look.font_size != font
                        || file.look.family != family
                        || !file.comparison_current(&comparison, cx)
                    {
                        return;
                    }
                    let Some(&block) = comparison.marker_blocks.get(marker) else {
                        return;
                    };
                    file.show_git_hunk(
                        comparison.clone(),
                        block,
                        point(bounds.right(), bounds.bottom()),
                        window,
                        cx,
                    );
                });
            },
        ))
    }
    fn show_git_hunk(
        &mut self,
        comparison: Arc<line_changes::Comparison>,
        selected: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let block = &comparison.blocks[selected];
        let preview = (!block.old.is_empty()).then(|| {
            Self::editor_for(
                self.language,
                false,
                comparison.old[block.old.clone()].to_owned(),
                window,
                cx,
            )
        });
        let occurrences = preview
            .as_ref()
            .map(|editor| Occurrences::attach(editor, self.look.theme, cx));
        self.suppress_presentation_blur = self
            .editor
            .as_ref()
            .is_some_and(|e| e.read(cx).focus_handle(cx).contains_focused(window, cx));
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.git_markers.popup = Some(HunkPopup {
            comparison,
            selected,
            presentation: self.git_markers.presentation.get(),
            at,
            focus,
            preview,
            occurrences,
        });
        cx.notify();
    }
    fn close_git_hunk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.git_markers.popup = None;
        // A mode switch already gave focus to Preview. An old callback must
        // not send subsequent typing into a Source editor that is now hidden.
        if self.drawn_at.get().is_some() {
            window.focus(&self.focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn step_git_hunk(
        &mut self,
        snapshot: &Arc<line_changes::Comparison>,
        selected: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.comparison_current(snapshot, cx) {
            self.close_git_hunk(window, cx);
            return;
        }
        let Some(popup) = &self.git_markers.popup else {
            return;
        };
        if popup.presentation != self.git_markers.presentation.get()
            || !Arc::ptr_eq(&popup.comparison, snapshot)
            || selected >= snapshot.blocks.len()
        {
            return;
        }
        let at = popup.at;
        let row = snapshot.blocks[selected].rows.start;
        if let Some(editor) = &self.editor {
            editor.update(cx, |state, cx| {
                let row = row.min(state.text().lines_len().saturating_sub(1));
                state.set_cursor_position_centred(Position::new(row as u32, 0), window, cx);
            });
        }
        self.show_git_hunk(snapshot.clone(), selected, at, window, cx);
    }
    fn rollback_git_hunk(
        &mut self,
        snapshot: &Arc<line_changes::Comparison>,
        selected: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.comparison_current(snapshot, cx) {
            self.close_git_hunk(window, cx);
            return;
        }
        if !self.git_markers.popup.as_ref().is_some_and(|p| {
            p.presentation == self.git_markers.presentation.get()
                && Arc::ptr_eq(&p.comparison, snapshot)
                && p.selected == selected
        }) {
            return;
        }
        let Some(block) = snapshot.blocks.get(selected) else {
            return;
        };
        let editor = self.editor.clone().unwrap();
        let replacement = snapshot.old[block.old.clone()].to_owned();
        // Public selection is UTF-8; replace is an Atomic editor edit. Never
        // reset the component value/history or write Git/disk here.
        editor.update(cx, |state, cx| {
            state.set_selected_range(block.current_raw.clone(), cx);
            state.replace(replacement, window, cx);
        });
        self.close_git_hunk(window, cx);
    }
    /// The offer of the popup's block, when the offers are of this
    /// comparison: the action and whether it is available.
    fn hunk_offer_of(
        &self,
        snapshot: &Arc<line_changes::Comparison>,
        selected: usize,
    ) -> Option<(Action, Result<(), crate::i18n::Text>)> {
        let offers = self
            .git_markers
            .offers
            .as_ref()
            .filter(|o| Arc::ptr_eq(&o.comparison, snapshot))?;
        if let Err(why) = crate::diff_tab::hunk_hint(offers.eligibility) {
            return Some((Action::Stage, Err(why)));
        }
        crate::diff_tab::hunk_present(View::Head, *offers.per_block.get(selected)?)
    }

    /// The keymap's Stage Hunk or Unstage Hunk in an editor: the block under
    /// the caret, when it offers that action and it is available.
    pub(in crate::file_tab) fn hunk_action_at_caret(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(editor), Some(snapshot)) =
            (self.editor.clone(), self.git_markers.comparison.clone())
        else {
            return;
        };
        let row = editor.read(cx).cursor_position().line as usize;
        let block = snapshot
            .blocks
            .iter()
            .position(|b| b.rows.contains(&row) || (b.rows.is_empty() && b.rows.start == row));
        if let Some(block) = block
            && self.hunk_offer_of(&snapshot, block) == Some((action, Ok(())))
        {
            self.stage_git_hunk(&snapshot, block, window, cx);
        }
    }

    /// The popup's Stage or Unstage: the block's index content as a write
    /// the window sends. Nothing is edited here — the buffer is the shown
    /// working text, and its snapshot is checked as Rollback checks it.
    pub(in crate::file_tab) fn stage_git_hunk(
        &mut self,
        snapshot: &Arc<line_changes::Comparison>,
        selected: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.comparison_current(snapshot, cx) {
            self.close_git_hunk(window, cx);
            return;
        }
        // One write per file while it is unanswered, refused before the
        // buffer is copied — review r3 `arch+quality-5`.
        if crate::root::hunks::hunk_in_flight(cx, &self.path()) {
            self.close_git_hunk(window, cx);
            return;
        }
        let Some(offers) = self
            .git_markers
            .offers
            .clone()
            .filter(|o| Arc::ptr_eq(&o.comparison, snapshot))
        else {
            return;
        };
        let (Some(block), Some(comparisons), Some(index)) = (
            snapshot.blocks.get(selected),
            offers.comparisons.as_ref(),
            offers.index.as_ref(),
        ) else {
            return;
        };
        let Some((action, postimage)) = comparisons.postimage(View::Head, &super::shown(block))
        else {
            return;
        };
        let hunk = crate::git::Hunk {
            path: self.relative.clone(),
            preimage: Arc::from(index.as_bytes()),
            postimage: Arc::from(postimage.into_bytes()),
            disk: None,
            head: Arc::from(offers.head.as_bytes()),
        };
        cx.emit(super::HunkRequest(match action {
            Action::Stage => crate::git::Write::StageHunk(hunk),
            Action::Unstage => crate::git::Write::UnstageHunk(hunk),
        }));
        self.close_git_hunk(window, cx);
    }

    pub(in crate::file_tab) fn render_git_hunk(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.git_markers.popup.as_ref().is_some_and(|p| {
            p.presentation != self.git_markers.presentation.get()
                || !self.comparison_current(&p.comparison, cx)
        }) {
            self.close_git_hunk(window, cx);
        }
        let popup = self.git_markers.popup.as_ref()?;
        let theme = self.look.theme;
        let selected = popup.selected;
        let comparison = popup.comparison.clone();
        let mut toolbar = div()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .p(theme::space::MENU_PADDING);
        for (id, icon, enabled, target) in [
            (
                "git-hunk-previous",
                "ui/arrow-up.svg",
                selected > 0,
                selected.saturating_sub(1),
            ),
            (
                "git-hunk-next",
                "ui/arrow-down.svg",
                selected + 1 < comparison.blocks.len(),
                selected + 1,
            ),
        ] {
            let snapshot = comparison.clone();
            toolbar = toolbar.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .size(theme::space::HEAD_BUTTON)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(enabled, |b| {
                        b.cursor_pointer()
                            .hover(|s| s.bg(rgb(theme.surface.raised)))
                            .on_click(cx.listener(move |file, _, window, cx| {
                                file.step_git_hunk(&snapshot, target, window, cx)
                            }))
                    })
                    .child(
                        crate::icons::toolbar(icon)
                            .size(theme::space::MENU_ICON)
                            .text_color(rgb(if enabled {
                                theme.text.secondary
                            } else {
                                theme.text.faint
                            })),
                    ),
            );
        }
        let snapshot = comparison.clone();
        toolbar = toolbar.child(
            div()
                .id("git-hunk-rollback")
                .debug_selector(|| "git-hunk-rollback".into())
                .size(theme::space::HEAD_BUTTON)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme.surface.raised)))
                .hint(self.ui.text(Label::RollbackLines), theme)
                .child(crate::view::popup::icon("ui/tool-revert.svg", false, theme))
                .on_click(cx.listener(move |file, _, window, cx| {
                    file.rollback_git_hunk(&snapshot, selected, window, cx)
                })),
        );
        // GLD-026: Stage for a filled block, Unstage for an outlined one.
        if let Some((action, available)) = self.hunk_offer_of(&comparison, selected) {
            let snapshot = comparison.clone();
            let (id, icon, label) = match action {
                Action::Stage => ("git-hunk-stage", "ui/head-plus.svg", Label::StageHunk),
                Action::Unstage => ("git-hunk-unstage", "ui/head-hide.svg", Label::UnstageHunk),
            };
            let enabled = available.is_ok();
            let hint = self.ui.text(available.err().unwrap_or(label)).to_owned();
            toolbar = toolbar.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .size(theme::space::HEAD_BUTTON)
                    .flex()
                    .items_center()
                    .justify_center()
                    .hint(hint, theme)
                    .child(
                        crate::icons::toolbar(icon)
                            .size(theme::space::MENU_ICON)
                            .text_color(rgb(if enabled {
                                theme.text.secondary
                            } else {
                                theme.text.faint
                            })),
                    )
                    .when(enabled, |b| {
                        b.cursor_pointer()
                            .hover(|s| s.bg(rgb(theme.surface.raised)))
                            .on_click(cx.listener(move |file, _, window, cx| {
                                file.stage_git_hunk(&snapshot, selected, window, cx)
                            }))
                    }),
            );
        }
        if let Some(occurrences) = &popup.occurrences {
            occurrences.update(cx, |owner, cx| owner.restyle(theme, cx));
        }
        let source_bounds = self
            .drawn_at
            .get()
            .unwrap_or_else(|| Bounds::new(Point::default(), window.viewport_size()));
        let padding = theme::space::MENU_PADDING.to_pixels(window.rem_size());
        let toolbar_height = theme::space::HEAD_BUTTON.to_pixels(window.rem_size()) + padding * 4.;
        let width = theme::space::RUN_COMMAND
            .to_pixels(window.rem_size())
            .min((source_bounds.size.width - px(16.)).max(px(0.)));
        let code_height = (px(self.look.font_size)
            * theme::leading::CODE
            * (comparison.blocks[selected].old_rows.len() + 1) as f32)
            .min(theme::space::PREVIEW.to_pixels(window.rem_size()))
            .min((source_bounds.size.height - toolbar_height - px(16.)).max(px(0.)));
        let height = toolbar_height
            + if popup.preview.is_some() {
                code_height + px(1.)
            } else {
                px(0.)
            };
        let code = popup.preview.as_ref().map(|editor| {
            div()
                .id("git-hunk-source")
                .debug_selector(|| "git-hunk-source".into())
                .h(code_height)
                .min_h_0()
                .border_t_1()
                .border_color(rgb(theme.surface.divider))
                .child(
                    Editor::new(editor)
                        .readonly(true)
                        .appearance(false)
                        .h_full()
                        .font_family(self.look.family.clone())
                        .text_size(px(self.look.font_size))
                        .line_height(px(self.look.font_size) * theme::leading::CODE),
                )
        });
        let at = point(
            popup
                .at
                .x
                .max(source_bounds.left() + px(8.))
                .min((source_bounds.right() - width - px(8.)).max(source_bounds.left())),
            popup
                .at
                .y
                .max(source_bounds.top() + px(8.))
                .min((source_bounds.bottom() - height - px(8.)).max(source_bounds.top())),
        );
        let card = crate::view::popup::card(theme)
            .id("git-hunk-popup")
            .debug_selector(|| "git-hunk-popup".into())
            .track_focus(&popup.focus)
            .min_w_0()
            .w(width)
            .capture_action(
                cx.listener(|file, _: &gpui_component::input::Escape, window, cx| {
                    file.close_git_hunk(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_key_down(cx.listener(|file, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    file.close_git_hunk(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down_out(cx.listener(|file, _, window, cx| file.close_git_hunk(window, cx)))
            .child(toolbar)
            .children(code);
        Some(
            deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(px(8.))
                    .child(card),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    // Private owner state is intentionally tested here: no production API is
    // added to expose popup editor entities or presentation generations.
    #[gpui::test]
    fn hidden_source_revokes_popup_and_closing_releases_preview_owners(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.rs"), "let total = 2;\n").unwrap();
        let mut made = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new("file.rs"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            made = Some(file.clone());
            gpui_component::Root::new(file, window, cx)
        });
        visual.run_until_parked();
        let file = made.unwrap();
        let snapshot = Arc::new(
            line_changes::comparison(&"let total = 1;\n".repeat(100), "let total = 2;\n").unwrap(),
        );
        let (preview, occurrences) = visual.update(|window, cx| {
            file.update(cx, |file, cx| {
                file.git_markers.task = None;
                file.git_markers.running = false;
                file.git_markers.started = true;
                file.git_markers.comparison = Some(snapshot.clone());
                file.git_markers.source =
                    Some(file.editor.as_ref().unwrap().read(cx).text().clone());
                file.show_git_hunk(snapshot.clone(), 0, point(px(20.), px(20.)), window, cx);
                let popup = file.git_markers.popup.as_ref().unwrap();
                (
                    popup.preview.as_ref().unwrap().downgrade(),
                    popup.occurrences.as_ref().unwrap().downgrade(),
                )
            })
        });
        visual.run_until_parked();
        let shown = preview.upgrade().unwrap();
        let (position, before) = visual.update(|_, cx| {
            (
                shown.read(cx).input_bounds().center(),
                shown.read(cx).scroll_offset(),
            )
        });
        visual.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-200.))),
            ..Default::default()
        });
        visual.run_until_parked();
        visual.update(|_, cx| {
            assert_ne!(
                shown.read(cx).scroll_offset(),
                before,
                "long readonly HEAD code scrolls in its bounded popup"
            )
        });
        drop(shown);
        visual.update(|window, cx| {
            file.update(cx, |file, cx| {
                let preview_focus = cx.focus_handle();
                window.focus(&preview_focus, cx);
                file.hide_source(); // The same hook Markdown Preview calls.
                file.rollback_git_hunk(&snapshot, 0, window, cx);
                assert!(
                    preview_focus.is_focused(window),
                    "stale callback must not focus hidden Source"
                );
                assert_eq!(
                    file.editor.as_ref().unwrap().read(cx).text().to_string(),
                    "let total = 2;\n"
                );
            })
        });
        visual.run_until_parked();
        visual.update(|_, cx| {
            assert!(preview.upgrade().is_none());
            assert!(occurrences.upgrade().is_none());
            assert!(file.read(cx).git_markers.popup.is_none());
        });
        let owner = file.downgrade();
        visual.update(|window, _| window.remove_window());
        drop(file);
        visual.run_until_parked();
        assert!(owner.upgrade().is_none());
    }
}
