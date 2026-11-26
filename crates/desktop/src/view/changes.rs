//! The Commit panel, drawn wherever its dock is.
//!
//! The Commit window, since M8.3.1: two sections — *Changes*, every
//! tracked file with a change, and *Unversioned files* — each a head
//! row with a checkbox, its name and its count, and under it one row
//! per file in section 6.4's shape with a checkbox first: the icon, the
//! name in `text.primary`, the directory it is in after it in
//! `text.faint`, and the mark at the right edge. A staged rename says
//! where it came from instead of its directory. Section 6.12.
//!
//! The checkbox is the panel's own, decision 4 of M8.3.1: a change is
//! ticked when it arrives, an unversioned file is not, and a tick is
//! what the commit takes — nothing is written to git until then. The
//! head's box does the whole section.
//!
//! Through `uniform_list`, like the tree: the map is bounded by the
//! changes, and a branch with two thousand of them is a list that still
//! costs what is on screen. A row looks and behaves as the Explorer's —
//! decision 54: one solid selection, a softer grey under the pointer, the
//! arrow, names in the interface face.

use crate::view::tooltip::Hint as _;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use crate::text_field::HintedTextarea;
use gpui::prelude::*;
use gpui::{Context, div, px, rgb, uniform_list};

use crate::actions::keymap::Named;
use crate::changes::Section;
use crate::git::GitError;
use crate::i18n::Text;
use crate::theme::Mark;
use crate::{Root, icons, theme};

/// One line of the list, copied out of the model so that the list's
/// closure owns it.
enum Line {
    Head {
        section: Section,
        count: usize,
        /// Every row of the section is ticked.
        ticked: bool,
        /// The rows are hidden under the head.
        folded: bool,
    },
    Row {
        section: Section,
        path: PathBuf,
        name: String,
        /// A directory's disclosure, open or not; `None` for a file.
        open: Option<bool>,
        /// What is shown after the name: the directory, or a rename's
        /// old path.
        beside: String,
        stem: String,
        mark: theme::Mark,
        is_dir: bool,
        selected: bool,
        ticked: bool,
        /// In the middle of a merge with both sides changed: a commit
        /// cannot take it, so its menu does not offer to tick it.
        unmerged: bool,
        partial: bool,
        depth: usize,
    },
}

impl Section {
    fn title(self) -> Text {
        match self {
            Section::Changes => Text::Changes,
            Section::Unversioned => Text::Unversioned,
        }
    }
}

/// A checkbox, section 6.12: 14 square at radius 3, a hairline in
/// `text.faint` when empty, brass with a tick when ticked.
pub(crate) fn checkbox(
    id: (&'static str, usize),
    ticked: bool,
    theme: theme::Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(theme::space::CHECKBOX)
        .rounded(theme::radius::CHECKBOX)
        .border_1()
        .border_color(rgb(if ticked {
            theme.accent.brass
        } else {
            theme.text.faint
        }))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(ticked, |this| {
            this.bg(rgb(theme.accent.brass))
                .child(icons::check().text_color(rgb(theme.surface.panel)))
        })
}

impl Root {
    /// The changes of the active project as the panel's body, or `None`
    /// when there is nothing to list — no project, no map yet, or a
    /// clean tree, which the panel says in its own words.
    pub(crate) fn render_changes(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let project = self.active_project?;
        match self.git_status.get(&project)? {
            Err(GitError::NoGit) => return Some(self.tree_note(Text::DiffNoGit, None)),
            Err(GitError::NoRepository) => {
                return Some(self.tree_note(Text::DiffNoRepository, None));
            }
            Err(GitError::Failed(words)) => {
                return Some(self.tree_note(Text::TreeCannotRead, Some(words.clone())));
            }
            Ok(_) => {}
        }
        let state = self.changes()?;
        let theme = self.theme;
        let palette = icons::Palette::from(theme.kind);
        let query = state
            .speed
            .as_ref()
            .map(|s| s.query.clone())
            .unwrap_or_default();
        let mut lines = Vec::new();
        for section in [Section::Changes, Section::Unversioned] {
            let rows: Vec<_> = state.list.iter().filter(|l| l.section == section).collect();
            if rows.is_empty() {
                continue;
            }
            let folded = state.folded.contains(&section);
            lines.push(Line::Head {
                section,
                count: rows.iter().filter(|l| !l.row.is_dir).count(),
                ticked: rows.iter().any(|l| !l.row.unmerged)
                    && rows
                        .iter()
                        .filter(|l| !l.row.unmerged)
                        // A partial row is in the commit too — GLD-026.
                        .all(|l| {
                            state.membership(&l.row.path).0 || state.partial.contains(&l.row.path)
                        }),
                folded,
            });
            if folded {
                continue;
            }
            for listed in rows {
                let row = &listed.row;
                if !state.row_visible(&row.path) {
                    continue;
                }
                let name = row
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        self.project_root()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                            .unwrap_or_default()
                    });
                let beside = if row.is_dir {
                    state.directory_label(&row.path, self.ui())
                } else {
                    match &row.from {
                        Some(from) => format!("\u{2190} {}", from.display()),
                        None => row
                            .path
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    }
                };
                lines.push(Line::Row {
                    section,
                    path: row.path.clone(),
                    stem: if row.is_dir {
                        icons::for_dir(&row.path, false, palette)
                    } else {
                        icons::for_file(&name, palette)
                    },
                    open: row.is_dir.then(|| state.expanded.contains(&row.path)),
                    name: if row.is_dir { name.to_string() } else { name },
                    beside,
                    mark: row.mark,
                    is_dir: row.is_dir,
                    unmerged: row.unmerged,
                    selected: state.selection.contains(&row.path),
                    ticked: state.membership(&row.path).0,
                    partial: state.membership(&row.path).1 || state.partial.contains(&row.path),
                    depth: row
                        .path
                        .ancestors()
                        .skip(1)
                        .filter(|p| state.directories.contains_key(*p))
                        .count(),
                });
            }
        }
        let commit_box = self.render_commit_box(cx);
        let count = lines.len();
        let lines = Rc::new(lines);
        let focus = state.focus.clone();
        let row_focus = focus.clone();
        let scroll = state.scroll.clone();

        let list = uniform_list(
            "changes-rows",
            count,
            cx.processor(move |root: &mut Root, range: Range<usize>, window, cx| {
                let ui = root.ui();
                // Blue while the panel or its own menu has the keys —
                // decision 54.
                // The message box sits inside the panel's focus and does
                // not count: typing a message is not the list's.
                let message = root
                    .changes()
                    .and_then(|state| state.message.clone())
                    .is_some_and(|message| {
                        gpui::Focusable::focus_handle(message.read(cx), cx)
                            .contains_focused(window, cx)
                    });
                let focused = (crate::view::tree::keys_in(&row_focus, window, cx) && !message)
                    || (window.is_window_active()
                        && root.menu_over(|target| {
                            matches!(
                                target,
                                crate::view::menu::Target::Change { .. }
                                    | crate::view::menu::Target::ChangeSection
                            )
                        }));
                range
                    .map(|index| match &lines[index] {
                        Line::Head {
                            section,
                            count,
                            ticked,
                            folded,
                        } => {
                            let section = *section;
                            div()
                                .id(("changes-head", index))
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener(|root: &mut Root, _event, _window, _cx| {
                                        root.note_menu_target(
                                            crate::view::menu::Target::ChangeSection,
                                        );
                                    }),
                                )
                                .w_full()
                                .h(theme::space::TREE_ROW)
                                .flex()
                                .items_center()
                                .gap(theme::space::TREE_GAP)
                                .px(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                                .text_size(theme::size::UI)
                                .child(
                                    div()
                                        .id(("changes-fold", index))
                                        .flex_none()
                                        .size(theme::space::CHEVRON)
                                        .cursor_pointer()
                                        .child(
                                            icons::chevron(!*folded)
                                                .text_color(rgb(theme.text.faint)),
                                        )
                                        .on_click(cx.listener(move |root, _event, _window, cx| {
                                            cx.stop_propagation();
                                            root.toggle_fold(section, cx);
                                        })),
                                )
                                .child(
                                    checkbox(("changes-head-box", index), *ticked, theme).on_click(
                                        cx.listener(move |root, _event, _window, cx| {
                                            cx.stop_propagation();
                                            root.changes_toggle_group(section, cx);
                                        }),
                                    ),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(theme.text.secondary))
                                        .child(ui.text(section.title())),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(theme.text.faint))
                                        .child(count.to_string()),
                                )
                        }
                        Line::Row {
                            section,
                            path,
                            name,
                            open,
                            beside,
                            stem,
                            mark,
                            is_dir,
                            selected,
                            ticked,
                            unmerged,
                            partial,
                            depth,
                        } => {
                            let (section, is_dir) = (*section, *is_dir);
                            let gone = *mark == Mark::Deleted;
                            let (ticked_now, unmerged) = (*ticked, *unmerged);
                            let (clicked, toggled) = (path.clone(), path.clone());
                            let aimed = path.clone();
                            let selector = format!("commit-row-{}", path.display());
                            let description = beside.clone();
                            let name_selector = format!("commit-name-{name}");
                            div()
                                .id(("changes-row", index))
                                .debug_selector(move || selector.clone())
                                .w_full()
                                .h(theme::space::TREE_ROW)
                                .flex()
                                .items_center()
                                .gap(theme::space::TREE_GAP)
                                .pl(theme::space::TREE_PADDING
                                    + theme::space::TREE_INSET
                                    + theme::space::TREE_INDENT * (*depth as f32 + 1.0))
                                .pr(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                                .map(|this| {
                                    crate::view::tree::ground(
                                        this,
                                        theme,
                                        Some(*mark),
                                        theme::Selected::of(*selected, focused),
                                        theme.surface.app,
                                    )
                                })
                                .child(
                                    checkbox(("changes-box", index), *ticked, theme)
                                        .when(*partial, |this| {
                                            this.child(
                                                icons::check_partial()
                                                    .text_color(rgb(theme.text.primary)),
                                            )
                                        })
                                        .on_click(cx.listener(move |root, _event, _window, cx| {
                                            cx.stop_propagation();
                                            root.changes_toggle_row(&toggled, cx);
                                        })),
                                )
                                .child(icons::icon(stem).flex_none())
                                .child(
                                    div()
                                        .debug_selector(move || name_selector.clone())
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap(theme::space::STATUS_BUTTON_GAP)
                                        .when_some(*open, |name, open| {
                                            name.child(
                                                icons::glyph(
                                                    if open {
                                                        "ui/chevron-down.svg"
                                                    } else {
                                                        "ui/chevron-right.svg"
                                                    },
                                                    theme::size::UI,
                                                )
                                                .text_color(rgb(theme.text.primary)),
                                            )
                                        })
                                        .whitespace_nowrap()
                                        .text_size(theme::size::UI)
                                        .text_color(rgb(theme.text.primary))
                                        .child(crate::view::speed_search::label(
                                            name.clone(),
                                            &query,
                                            theme,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_size(theme::size::UI)
                                        .text_color(rgb(theme.text.faint))
                                        .child(beside.clone()),
                                )
                                .when(is_dir, |this| {
                                    this.hint_delay().tooltip(move |_, app| {
                                        app.new(|_| {
                                            crate::view::tooltip::Tooltip::text(
                                                description.clone(),
                                                theme,
                                            )
                                        })
                                        .into()
                                    })
                                })
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener(move |root: &mut Root, _event, window, cx| {
                                        root.changes_context(&aimed, window, cx);
                                        root.note_menu_target(crate::view::menu::Target::Change {
                                            path: aimed.clone(),
                                            ticked: ticked_now,
                                            unmerged,
                                            is_dir,
                                            // Which of the two lists the
                                            // row is in decides whether
                                            // it is offered *Add to VCS*
                                            // or *Rollback…* — M8.8.4.
                                            unversioned: section == Section::Unversioned,
                                            // Already off the disk: the
                                            // Trash has nothing to take.
                                            gone,
                                        });
                                    }),
                                )
                                .on_click(cx.listener(
                                    move |root, event: &gpui::ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        root.changes_click(
                                            section,
                                            &clicked,
                                            is_dir,
                                            event.modifiers(),
                                            window,
                                            cx,
                                        );
                                    },
                                ))
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&scroll)
        .size_full()
        .into_any_element();
        let list = if count == 0 {
            self.tree_note(Text::PanelChangesEmpty, None)
        } else {
            list
        };

        Some(
            div()
                .id("changes")
                .track_focus(&focus)
                .key_context("changes")
                .on_action(cx.listener(
                    |root, _: &crate::actions::IncludeSelectedChanges, _, cx| {
                        root.changes_include_selected(Some(true), cx)
                    },
                ))
                .on_action(cx.listener(
                    |root, _: &crate::actions::ExcludeSelectedChanges, _, cx| {
                        root.changes_include_selected(Some(false), cx)
                    },
                ))
                .on_action(
                    cx.listener(|root, _: &crate::actions::SelectAll, _window, cx| {
                        if let Some(state) = root.changes_mut() {
                            state
                                .selection
                                .key("a", &state.visible_paths(), true, false);
                            state.settle_selection();
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_action(cx.listener(|root, _: &crate::actions::Copy, _window, cx| {
                    if let Some(state) = root.changes() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            state.selection.copied_paths(),
                        ));
                    }
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if root.changes_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .children(self.file_speed_field(crate::root::speed_search::FileHost::Commit, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .py(theme::space::CARD_GAP)
                        .id("commit-list-viewport")
                        .debug_selector(|| "commit-list-viewport".into())
                        .child(list),
                )
                .children(commit_box)
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|root, event: &gpui::MouseDownEvent, window, cx| {
                        root.open_context_menu(event.position, window, cx)
                    }),
                )
                .into_any_element(),
        )
    }

    /// The toolbar under the head: refresh, the selected row's
    /// diff, tick all, untick all, collapse and expand — icon buttons 24
    /// square at radius 6, each with a tooltip naming its action and
    /// stroke. Section 6.12.
    /// The Commit panel's buttons.
    ///
    /// **In the dock's head, not in a strip under it.** Two rows of
    /// chrome at forty-two each cost a panel eighty-four pixels before a
    /// line of content, where forty-two is enough: a tool window's own
    /// controls live in its header, at the right.
    pub(crate) fn render_commit_toolbar(
        &self,
        dock: guild_session::Dock,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let shown = self.commit_tools_shown(dock, cx);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .children(COMMIT_TOOLS.into_iter().take(shown).enumerate().map(
                |(index, (icon, label, action))| {
                    div()
                        .id(("commit-tool", index))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(theme::space::HEAD_BUTTON)
                        .rounded(theme::radius::ROW)
                        .cursor_pointer()
                        .hover(|button| {
                            button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                        })
                        .child(icons::toolbar(icon).text_color(rgb(theme.text.tertiary)))
                        .hint_delay()
                        .tooltip(move |_window, app| {
                            app.new(|_cx| {
                                crate::view::tooltip::Tooltip::for_action(
                                    ui.text(label),
                                    action,
                                    theme,
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |root, _event, window, cx| {
                            cx.stop_propagation();
                            root.run_commit_tool(action, window, cx);
                        }))
                },
            ))
            .into_any_element()
    }

    /// What a button of the Commit panel's toolbar does — from the toolbar,
    /// or from the ⋮ it folded into.
    pub(crate) fn run_commit_tool(
        &mut self,
        action: Named,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        match action {
            Named::RefreshChanges => self.refresh_changes_now(cx),
            Named::ShowSelectedDiff => self.show_selected_diff(window, cx),
            Named::TickAllChanges => self.tick_all(true, cx),
            Named::UntickAllChanges => self.tick_all(false, cx),
            Named::CollapseChanges => self.fold_all(true, cx),
            _ => self.fold_all(false, cx),
        }
    }

    /// How many of the toolbar's buttons fit in the dock's head — M12.1.
    ///
    /// **Worked out from the head's own measures**, the design system's
    /// rems at this window's scale, and from the name the head draws:
    /// [`crate::dock_menu::head_tools`] keeps the name whole and gives the
    /// buttons the rest. What does not fit goes into a ⋮.
    pub(crate) fn commit_tools_shown(&self, dock: guild_session::Dock, cx: &gpui::App) -> usize {
        let width = match dock {
            guild_session::Dock::Left => self.panels().left.size,
            guild_session::Dock::Right => self.panels().right.size,
            guild_session::Dock::Bottom => f32::from(self.viewport().width),
        };
        crate::dock_menu::head_tools(
            COMMIT_TOOLS.len(),
            width,
            f32::from(crate::scale::rem_size(self.settings.ui_font_size)),
            self.head_title_width(guild_session::Tool::Commit, cx),
        )
    }

    /// The toolbar's buttons that did not fit, in its order — what the ⋮
    /// holds first.
    pub(crate) fn commit_tools_folded(
        &self,
        dock: guild_session::Dock,
        cx: &gpui::App,
    ) -> Vec<Named> {
        COMMIT_TOOLS
            .iter()
            .skip(self.commit_tools_shown(dock, cx))
            .map(|(_, _, action)| *action)
            .collect()
    }

    /// The counter, the message box and the *Commit* button, under the
    /// list. The box is the editor component in plain
    /// mode, decision 7; the button is section 6.7's primary, and while
    /// nothing can be committed it is the secondary with the reason as
    /// its tooltip.
    fn render_commit_box(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let message = self.changes()?.message.clone()?;
        let theme = self.theme;
        let ui = self.ui();
        let blocked = self.commit_blocked_by(cx);
        let lit = blocked.is_none();
        // **The quiet shape** — 2026-09-18: the full
        // primary, a slab of brass in bold, was the loudest thing in a panel
        // of greys. Section 6.7.
        let button = div()
            .id("commit-button")
            .h(theme::space::BUTTON)
            .min_w(theme::space::BUTTON_MIN)
            .px(theme::space::BUTTON_SIDE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme::radius::CARD)
            .text_size(theme::size::UI)
            .when(lit, |this| {
                this.bg(rgb(theme.accent.brass_bg))
                    .text_color(rgb(theme.accent.brass))
                    .cursor_pointer()
                    .on_click(cx.listener(|root, _event, _window, cx| {
                        cx.stop_propagation();
                        root.commit(cx);
                    }))
            })
            .when_some(blocked, |this, why| {
                this.text_color(rgb(theme.text.faint))
                    .hint_delay()
                    .tooltip(move |_window, app| {
                        app.new(|_cx| crate::view::tooltip::Tooltip::text(ui.text(why), theme))
                            .into()
                    })
            })
            .child(ui.text(Text::CommitButton));
        let ticked = self
            .changes()
            .map_or(0, |s| s.ticked.len() + s.partial.len());
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(theme::space::CARD_GAP)
                .px(theme::space::TREE_PADDING - theme::space::TREE_INSET)
                .pt(theme::space::CARD_GAP)
                .pb(theme::space::CARD_GAP)
                .border_t_1()
                .border_color(rgb(theme.surface.divider))
                .child(
                    div()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(ui.commit_selection_count(
                            ticked,
                            self.changes().map_or(0, |s| s.selection.selected.len()),
                        )),
                )
                .child(
                    div()
                        .rounded(theme::radius::CARD)
                        .bg(rgb(theme.surface.inset))
                        .border_1()
                        .border_color(rgb(theme.surface.divider))
                        .px(theme::space::CARD_GAP)
                        .py(px(4.0))
                        .text_size(theme::size::UI)
                        .child(
                            HintedTextarea::new(&message, ui.text(Text::CommitPlaceholder))
                                .appearance(false),
                        ),
                )
                .child(div().flex().child(button))
                .into_any_element(),
        )
    }
}

/// The Commit panel's toolbar: refresh, the selected row's diff,
/// tick all, untick all, collapse and expand — the icon, what it is called
/// and the action it is. One table for the buttons and for the ⋮ they fold
/// into, so a folded button is the same button.
pub(crate) const COMMIT_TOOLS: [(&str, Text, Named); 6] = [
    ("ui/tool-refresh.svg", Text::Refresh, Named::RefreshChanges),
    (
        "ui/tool-diff.svg",
        Text::ShowSelectedDiff,
        Named::ShowSelectedDiff,
    ),
    ("ui/tool-tick-all.svg", Text::TickAll, Named::TickAllChanges),
    (
        "ui/tool-untick-all.svg",
        Text::UntickAll,
        Named::UntickAllChanges,
    ),
    (
        "ui/tool-collapse.svg",
        Text::CollapseAll,
        Named::CollapseChanges,
    ),
    ("ui/tool-expand.svg", Text::ExpandAll, Named::ExpandChanges),
];
