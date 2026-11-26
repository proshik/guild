//! The Log, drawn wherever its dock is.
//!
//! One row per commit, newest first, 44 high with two lines: the summary
//! and, under it, the author and how long ago; the short hash at the
//! right edge. A click opens the commit below the list — its message
//! and its files, each in section 6.4's shape and a way to its diff in
//! that commit. Section 6.13. Through `uniform_list`, and the last row
//! drawn asks for the next page: two hundred commits at a time, one
//! process each, decision 8.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, SharedString, div, px, rgb, uniform_list};

use guild_session::{Dock, Tool};

use crate::git::GitError;
use crate::i18n::Text;
use crate::log_layout::{LogLayout, layout};
use crate::view::panels::Column;
use crate::view::tooltip::Hint as _;
use crate::{Root, icons, theme};

/// What a row needs, copied out of the model so that the list's closure
/// owns it.
struct RowData {
    sha: String,
    short: String,
    summary: String,
    author: String,
    date: String,
    refs: String,
    /// Two parents or more. The one fact about a commit's shape that a
    /// person scanning a log looks for.
    merge: bool,
    opened: bool,
}

impl Root {
    /// The log of the active project as the panel's body, or `None` when
    /// there is nothing to list yet.
    pub(crate) fn render_log(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> Option<gpui::AnyElement> {
        let project = self.active_project?;
        let state = self.log()?;
        // Not a repository at all: no log, and no filter to put on one.
        match &state.error {
            Some(GitError::NoGit) => return Some(self.tree_note(Text::DiffNoGit, None)),
            Some(GitError::NoRepository) => {
                return Some(self.tree_note(Text::DiffNoRepository, None));
            }
            _ => {}
        }
        // **A note where the columns would be, and the toolbar above it
        // either way.** Returning early took the toolbar with the list,
        // so a filter that matched nothing removed the control that
        // would have undone it: a *Date* of the last day in a repository
        // with no commit today emptied the panel and left no way back to
        // the log — the state could be reached and not left.
        let narrowed = state.scope != crate::git::Scope::All
            || !state.filter.text.trim().is_empty()
            || state.filter.author.is_some()
            || state.filter.since.is_some();
        let note: Option<(Text, Option<String>)> = match &state.error {
            Some(GitError::Failed(words)) => Some((Text::TreeCannotRead, Some(words.clone()))),
            _ if !state.commits.is_empty() => None,
            _ if state.in_flight => Some((Text::LogLoading, None)),
            _ if narrowed => Some((Text::LogNoMatches, None)),
            _ => Some((Text::PanelLogEmpty, None)),
        };
        let theme = self.theme;
        let ui = self.ui();
        let family: SharedString = self.code_font.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        // Asked once for the whole list rather than once per row: it is a
        // message to AppKit, and every row would get the same answer.
        let offset = crate::system::utc_offset();

        let rows: Vec<RowData> = state
            .commits
            .iter()
            .map(|commit| RowData {
                sha: commit.sha.clone(),
                short: commit.short().to_owned(),
                summary: commit.summary.clone(),
                author: commit.author.clone(),
                date: ui.commit_date(commit.time, now, offset),
                refs: crate::git::log::refs_label(&commit.refs, REFS_BUDGET),
                merge: commit.parents.len() >= 2,
                opened: state.selection.shas.contains(&commit.sha),
            })
            .collect();
        let count = rows.len();
        let rows = Rc::new(rows);
        let focus = state.focus.clone();
        let scroll = state.scroll.clone();
        let mono = family.clone();
        let graph = state.graph.clone();

        let list = uniform_list(
            "log-rows",
            count,
            cx.processor(move |root: &mut Root, range: Range<usize>, _window, cx| {
                // The last row drawn is the ask for the next page: the
                // person is at the end, and there may be more.
                if range.end >= count {
                    root.ask_log_page(project);
                }
                range
                    .map(|index| {
                        let row = &rows[index];
                        let sha = row.sha.clone();
                        div()
                            .id(("log-row", index))
                            .w_full()
                            .h(theme::space::LOG_ROW)
                            .flex()
                            .items_center()
                            .gap(theme::space::TREE_GAP)
                            .px(theme::space::TREE_PADDING)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .when(row.opened, |this| this.bg(rgb(theme.surface.raised)))
                            // The author, at a width of its own so the
                            // summaries start at one place down the whole
                            // column and the eye has a line to run down.
                            .child(
                                div()
                                    .flex_none()
                                    .w(gpui::relative(theme::space::LOG_AUTHOR_FRACTION))
                                    .max_w(theme::space::LOG_AUTHOR)
                                    .truncate()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.secondary))
                                    .child(row.author.clone()),
                            )
                            .child(graph_row(
                                graph.clone(),
                                index,
                                row.merge,
                                row.opened,
                                theme,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(theme::space::LOG_SUMMARY_MIN)
                                    .truncate()
                                    .text_size(theme::size::UI)
                                    .text_color(rgb(theme.text.primary))
                                    .child(row.summary.clone()),
                            )
                            .children((!row.refs.is_empty()).then(|| {
                                div()
                                    .flex_none()
                                    .max_w(gpui::relative(theme::space::LOG_REFS_FRACTION))
                                    .flex_shrink(1.0)
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(ref_chip(&row.refs, theme))
                            }))
                            // **The date at its own width, never cut** — in
                            // a fixed cell, right-aligned and clipped,
                            // *Yesterday 19:30* lost its first letters and
                            // *14.11.2026* read *4.11.2026*. The author, the
                            // summary and the chips give way instead, each
                            // ending in an ellipsis.
                            .child(
                                div()
                                    .flex_none()
                                    .whitespace_nowrap()
                                    .font_family(mono.clone())
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.faint))
                                    .child(row.date.clone()),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener({
                                    let sha = row.sha.clone();
                                    let short = row.short.clone();
                                    let summary = row.summary.clone();
                                    move |root: &mut Root, _event, _window, _cx| {
                                        root.note_menu_target(crate::view::menu::Target::Commit {
                                            sha: sha.clone(),
                                            short: short.clone(),
                                            summary: summary.clone(),
                                        });
                                    }
                                }),
                            )
                            .on_click(cx.listener(
                                move |root, event: &gpui::ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    root.log_click(&sha, event.modifiers(), window, cx);
                                },
                            ))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&scroll)
        .size_full();

        // Three columns when the panel is wide enough — the bottom dock is,
        // a side dock is not — and M8.3's stacked shape otherwise.
        // Decision 6 of M8.3.1.
        let panels = self.panels();
        let width = match self.docks().dock_of(Tool::Git) {
            Dock::Bottom => f32::from(self.viewport.width),
            Dock::Left => panels.left.size,
            Dock::Right => panels.right.size,
        };
        let wide = layout(width) == LogLayout::Wide;
        // What the hand left the two outer columns at, given the room.
        let (branches_width, details_width) = crate::log_layout::columns(width, panels.columns);
        let detail = self.render_opened_commit(wide, details_width, window, cx);
        // **The note stands in for the commits and for nothing else.**
        // A word that matches no commit says so where the commits are;
        // the branch column is filtered by its own field and the opened
        // commit belongs to neither, and emptying all three made a
        // typo blank the whole tool.
        let list = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .py(theme::space::CARD_GAP)
            .px(theme::space::TREE_INSET)
            .child(match note {
                Some((text, detail)) => self.tree_note(text, detail),
                None => list.into_any_element(),
            });

        // The columns, which the right button's menu belongs to: it is
        // about a row, and a right-click on the toolbar is not.
        let root = div().id("log-columns").flex_1().min_h_0().flex();
        let root = root.on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(|root, event: &gpui::MouseDownEvent, window, cx| {
                root.open_context_menu(event.position, window, cx)
            }),
        );
        let columns = if wide {
            root.flex_row()
                .child(self.render_branches(branches_width, cx))
                .child(self.render_column_grip(Column::LogBranches, cx))
                .child(list)
                .child(self.render_column_grip(Column::LogDetails, cx))
                .child(detail.unwrap_or_else(|| {
                    div()
                        .flex_none()
                        .w(px(details_width))
                        .border_l_1()
                        .border_color(rgb(theme.surface.divider))
                        .child(self.tree_note(Text::LogSelectCommit, None))
                        .into_any_element()
                }))
                .into_any_element()
        } else {
            root.flex_col()
                .child(list)
                .children(detail)
                .into_any_element()
        };

        // The toolbar above the columns, and whichever dropdown is open
        // over them: `relative` so the menu hangs from the strip rather
        // than from the window, and the columns are what is left.
        // **The focus goes on the panel, toolbar included.**
        //
        // It used to sit on the columns with the toolbar as their
        // sibling, and a press on a field focused the panel by hand —
        // which GPUI then undid: an element that tracks focus registers
        // its own mouse-down listener that focuses *itself* and calls
        // `prevent_default` so no ancestor takes over. Those listeners
        // run innermost first, and the root of the window tracks focus
        // too — so the root ran last, saw no `prevent_default` from a
        // press outside the columns, and took the focus back before the
        // first letter arrived. Measured, not guessed: the click landed
        // and `log_key` was never reached once.
        //
        // With the handle here the pressed element *is* the focusable
        // one, so GPUI focuses it and stops the root itself.
        Some(
            div()
                .id("log")
                .track_focus(&focus)
                .key_context("log")
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if root.log_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(self.render_log_toolbar(wide.then_some(branches_width), cx))
                .children(state.navigation_error.as_ref().map(|reason| {
                    div()
                        .flex_none()
                        .px(theme::space::PANEL_PADDING)
                        .py(theme::space::CARD_GAP)
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.accent.rust))
                        .child(reason.clone())
                }))
                .child(columns)
                .into_any_element(),
        )
    }

    /// The first column when there is the width: *Local* and the
    /// branches under it, the current one starred in brass.
    fn render_branches(&self, width: f32, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let all = self.branches_here().to_vec();
        let scope = self.log_scope();
        let query = self.branch_query();
        let open = self.local_branches_open();
        // The field narrows this column and asks git nothing: the
        // branches are already here, and re-walking the log to find a
        // name among twelve would be absurd.
        let names: Vec<String> = all.iter().map(|b| b.name.clone()).collect();
        let branches: Vec<crate::git::branches::Branch> =
            crate::branch_list::matching(&names, &query)
                .into_iter()
                .map(|index| all[index].clone())
                .collect();

        div()
            .id("log-branches")
            .flex_none()
            .w(px(width))
            .overflow_y_scroll()
            .border_r_1()
            .border_color(rgb(theme.surface.divider))
            .py(theme::space::CARD_GAP)
            .px(theme::space::TREE_INSET)
            // *All Branches* — the scope the log starts in. It has a
            // row because the column is the other way to set this
            // filter, and without it a person who clicked *HEAD* here
            // could not get back from the column they clicked in.
            .child(
                div()
                    .id("log-branch-all")
                    .h(theme::space::TREE_ROW)
                    .flex()
                    .items_center()
                    .px(theme::space::TREE_PADDING)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .text_size(theme::size::UI)
                    .when(scope == crate::git::Scope::All, |row| {
                        row.bg(rgb(theme.surface.raised))
                            .text_color(rgb(theme.text.primary))
                    })
                    .when(scope != crate::git::Scope::All, |row| {
                        row.text_color(rgb(theme.text.secondary))
                    })
                    .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                    .child(ui.text(Text::LogAllBranchesRow))
                    .on_click(cx.listener(|root, _event, _window, cx| {
                        cx.stop_propagation();
                        root.show_branch_log(crate::git::Scope::All, cx);
                    })),
            )
            // *HEAD (Current Branch)* — the log of wherever HEAD is,
            // which is one scope of three and has a row of its own to
            // say so.
            .child(
                div()
                    .id("log-branch-head")
                    .h(theme::space::TREE_ROW)
                    .flex()
                    .items_center()
                    .px(theme::space::TREE_PADDING)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .text_size(theme::size::UI)
                    .when(scope == crate::git::Scope::Head, |row| {
                        row.bg(rgb(theme.surface.raised))
                            .text_color(rgb(theme.text.primary))
                    })
                    .when(scope != crate::git::Scope::Head, |row| {
                        row.text_color(rgb(theme.text.secondary))
                    })
                    .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                    .child(ui.text(Text::LogHeadRow))
                    .on_click(cx.listener(|root, _event, _window, cx| {
                        cx.stop_propagation();
                        root.show_branch_log(crate::git::Scope::Head, cx);
                    })),
            )
            // *Local*, which opens and closes: a group with a chevron is
            // a tree, and a heading is a label.
            .child(
                div()
                    .id("log-branches-local")
                    .h(theme::space::TREE_ROW)
                    .flex()
                    .items_center()
                    .gap(theme::space::TREE_GAP)
                    .px(theme::space::TREE_PADDING)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .text_size(theme::size::UI)
                    .text_color(rgb(theme.text.secondary))
                    .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                    .child(
                        div().flex_none().w(theme::space::CHEVRON).child(
                            crate::icons::glyph(
                                if open {
                                    "ui/chevron-down.svg"
                                } else {
                                    "ui/chevron-right.svg"
                                },
                                theme::size::UI,
                            )
                            .text_color(rgb(theme.text.faint)),
                        ),
                    )
                    .child(ui.text(Text::LogLocal))
                    .on_click(cx.listener(|root, _event, _window, cx| {
                        cx.stop_propagation();
                        root.toggle_local_branches(cx);
                    })),
            )
            // A query of its own that matches nothing empties this
            // column and nothing else — the same rule the commits have,
            // and the popup's words for it.
            .children(
                (open && branches.is_empty() && !query.trim().is_empty())
                    .then(|| self.tree_note(Text::BranchesNone, None)),
            )
            .children(open.then_some(()).into_iter().flat_map(|()| {
                branches
                    .clone()
                    .into_iter()
                    .enumerate()
                    .map(|(index, branch)| {
                        // Which branch is being *listed*, which is not which one
                        // is checked out: the star says the second, the lit row
                        // the first, and nothing here moves HEAD.
                        let listed = match &scope {
                            crate::git::Scope::Named(name) => *name == branch.name,
                            crate::git::Scope::Head => branch.current,
                            crate::git::Scope::All => false,
                        };
                        let (chosen, aimed) = (branch.name.clone(), branch.name.clone());
                        div()
                            .id(("log-branch", index))
                            .w_full()
                            .h(theme::space::TREE_ROW)
                            .flex()
                            .items_center()
                            .gap(theme::space::TREE_GAP)
                            .pl(theme::space::TREE_PADDING + theme::space::TREE_INDENT)
                            .pr(theme::space::TREE_PADDING)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .when(listed, |row| row.bg(rgb(theme.surface.raised)))
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(move |root: &mut Root, _event, _window, _cx| {
                                    root.note_menu_target(crate::view::menu::Target::Branch {
                                        name: aimed.clone(),
                                        listed,
                                    });
                                }),
                            )
                            .on_click(cx.listener(move |root, _event, _window, cx| {
                                cx.stop_propagation();
                                root.show_branch_log(crate::git::Scope::Named(chosen.clone()), cx);
                            }))
                            .text_size(theme::size::UI)
                            .child(div().flex_none().w(theme::space::CHEVRON).when(
                                branch.current,
                                |slot| {
                                    slot.child(
                                        crate::icons::glyph("ui/star-filled.svg", theme::size::UI)
                                            .text_color(rgb(theme.accent.brass)),
                                    )
                                },
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_color(rgb(if branch.current {
                                        theme.text.primary
                                    } else {
                                        theme.text.secondary
                                    }))
                                    .child(branch.name),
                            )
                    })
            }))
            .into_any_element()
    }

    /// The opened commit — under the list, or in the third column when
    /// there is the width: its message, then its files, each a way to the
    /// diff of that file in that commit.
    fn render_opened_commit(
        &self,
        wide: bool,
        width: f32,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> Option<gpui::AnyElement> {
        let state = self.log()?;
        let sha = state.opened.clone()?;
        let commit = state
            .commits
            .iter()
            .find(|c| c.sha == sha.split(',').next().unwrap_or_default())?;
        let multiple = state.selection.shas.len() > 1;
        let theme = self.theme;
        let ui = self.ui();
        let palette = icons::Palette::from(theme.kind);

        // Blue while the files have the keys — decision 54.
        let focused = crate::view::tree::keys_in(&state.file_focus, window, cx);
        let query = state
            .speed
            .as_ref()
            .map(|s| s.query.clone())
            .unwrap_or_default();
        let files: gpui::AnyElement = match state.files.get(&sha) {
            None => self.tree_note(Text::LogLoading, None),
            Some(Err(err)) => self.tree_note(Text::TreeCannotRead, Some(err.to_string())),
            // **A tree, not a list.** Thirty files of one commit read as
            // thirty unrelated lines; the same thirty as a tree read as
            // *four directories, and here is which*. The chains of
            // single-child directories are collapsed —
            // `crates/desktop/tests/all` on one row rather than four rows
            // of nothing but a name.
            Some(Ok(files)) => div()
                .id("log-files-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&state.file_scroll)
                .flex()
                .flex_col()
                // The project itself, with everything the commit touched.
                // The first row: it answers *how big is this commit*
                // before the eye has read a single path.
                .child(
                    div()
                        .id("log-files-root")
                        .map(|row| {
                            crate::view::tree::ground(
                                row,
                                theme,
                                None,
                                theme::Selected::No,
                                theme.surface.app,
                            )
                        })
                        .on_click(cx.listener(|root, _, window, cx| {
                            cx.stop_propagation();
                            root.toggle_log_directory(None, window, cx);
                        }))
                        .w_full()
                        .h(theme::space::TREE_ROW)
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(theme::space::TREE_GAP)
                        .px(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                        .child(
                            icons::chevron(!state.file_tree.root_folded)
                                .text_color(rgb(theme.text.tertiary)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(theme::size::UI)
                                .text_color(rgb(theme.text.primary))
                                .child(self.active_project_name()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.faint))
                                .child(files.len().to_string()),
                        ),
                )
                .children(
                    state
                        .file_tree
                        .visible()
                        .into_iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, row)| match row {
                            crate::files_tree::Row::Directory {
                                path,
                                files,
                                depth,
                                key,
                            } => div()
                                .id(("log-directory", index))
                                .map(|row| {
                                    crate::view::tree::ground(
                                        row,
                                        theme,
                                        None,
                                        theme::Selected::No,
                                        theme.surface.app,
                                    )
                                })
                                .child(
                                    icons::chevron(!state.file_tree.folded.contains(&key))
                                        .text_color(rgb(theme.text.tertiary)),
                                )
                                .on_click(cx.listener(move |root, _, window, cx| {
                                    cx.stop_propagation();
                                    root.toggle_log_directory(Some(&key), window, cx);
                                }))
                                .w_full()
                                .h(theme::space::TREE_ROW)
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(theme::space::TREE_GAP)
                                .pl(theme::space::TREE_PADDING
                                    + theme::space::TREE_INSET
                                    + theme::space::TREE_INDENT * (depth as f32 + 1.0))
                                .pr(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_size(theme::size::UI)
                                        .text_color(rgb(theme.text.secondary))
                                        .child(path),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(theme::size::META)
                                        .text_color(rgb(theme.text.faint))
                                        .child(files.to_string()),
                                )
                                .into_any_element(),
                            crate::files_tree::Row::File {
                                name,
                                path,
                                from,
                                mark,
                                depth,
                            } => {
                                let stem = icons::for_file(&name, palette);
                                let sha = sha.clone();
                                let opens = path.clone();
                                let selected = state.file_selection.contains(&path);
                                let beside = from
                                    .map(|from| format!("\u{2190} {}", from.display()))
                                    .unwrap_or_default();
                                div()
                                    .id(("log-file", index))
                                    .map(|row| {
                                        crate::view::tree::ground(
                                            row,
                                            theme,
                                            Some(mark),
                                            theme::Selected::of(selected, focused),
                                            theme.surface.app,
                                        )
                                    })
                                    .w_full()
                                    .h(theme::space::TREE_ROW)
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(theme::space::TREE_GAP)
                                    .pl(theme::space::TREE_PADDING
                                        + theme::space::TREE_INSET
                                        + theme::space::TREE_INDENT * (depth as f32 + 1.0))
                                    .pr(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                                    .child(icons::icon(&stem).flex_none())
                                    .child(
                                        div()
                                            .flex_none()
                                            .whitespace_nowrap()
                                            .text_size(theme::size::UI)
                                            .text_color(rgb(theme.text.primary))
                                            .child(crate::view::speed_search::label(
                                                name, &query, theme,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_size(theme::size::META)
                                            .text_color(rgb(theme.text.faint))
                                            .child(beside),
                                    )
                                    .on_click(cx.listener(
                                        move |root, event: &gpui::ClickEvent, window, cx| {
                                            cx.stop_propagation();
                                            root.log_file_click(
                                                &sha,
                                                &opens,
                                                event.modifiers(),
                                                window,
                                                cx,
                                            );
                                        },
                                    ))
                                    .into_any_element()
                            }
                        }),
                )
                .into_any_element(),
        };

        let message_height = self.panels().columns.message;

        Some(
            div()
                .id("log-detail")
                .flex_none()
                .flex()
                .flex_col()
                .when(wide, |this| this.w(px(width)).h_full().border_l_1())
                .when(!wide, |this| {
                    this.max_h(theme::space::LOG_DETAIL).border_t_1()
                })
                .border_color(rgb(theme.surface.divider))
                // **The files first, the message under them** — the right
                // order: the question a person opens a
                // commit with is *what did it touch*, and the message
                // they have already read in the row above.
                .child(
                    div()
                        .id("log-detail-files")
                        .on_action(
                            cx.listener(|root, _: &crate::actions::ExpandChanges, window, cx| {
                                root.focus_and_fold_log_files(true, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_action(cx.listener(
                            |root, _: &crate::actions::CollapseChanges, window, cx| {
                                root.focus_and_fold_log_files(false, window, cx);
                                cx.stop_propagation();
                            },
                        ))
                        .track_focus(&state.file_focus)
                        .on_action(cx.listener(
                            |root, _: &crate::actions::SelectAll, _window, cx| {
                                if let Some(state) =
                                    root.active_project.and_then(|p| root.logs.get_mut(&p))
                                {
                                    state
                                        .file_selection
                                        .key("a", &state.file_paths(), true, false);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ))
                        .on_action(cx.listener(|root, _: &crate::actions::Copy, _window, cx| {
                            if let Some(state) = root.log() {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    state.file_selection.copied_paths(),
                                ));
                            }
                            cx.stop_propagation();
                        }))
                        .on_key_down(cx.listener(|root, event, window, cx| {
                            if root.log_files_key(event, window, cx) {
                                cx.stop_propagation();
                            }
                        }))
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .py(theme::space::CARD_GAP)
                        .flex()
                        .flex_col()
                        .children(
                            self.file_speed_field(crate::root::speed_search::FileHost::Log, cx)
                                .map(|field| div().px(theme::space::TREE_INSET).child(field)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .h(theme::space::LOG_TOOLBAR)
                                .children(
                                    [
                                        (true, "ui/tool-expand.svg", Text::ExpandAll),
                                        (false, "ui/tool-collapse.svg", Text::CollapseAll),
                                    ]
                                    .into_iter()
                                    .map(
                                        |(expand, icon, text)| {
                                            let label = ui.text(text).to_owned();
                                            div()
                                                .id(if expand {
                                                    "log-expand-all"
                                                } else {
                                                    "log-collapse-all"
                                                })
                                                .size(theme::space::HEAD_BUTTON)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(theme::radius::ROW)
                                                .cursor_pointer()
                                                .hover(|s| s.bg(rgb(theme.surface.raised)))
                                                .hint_delay()
                                                .tooltip(move |_, app| {
                                                    app.new(|_| crate::view::tooltip::Tooltip::for_action(label.clone(), if expand { crate::actions::keymap::Named::ExpandChanges } else { crate::actions::keymap::Named::CollapseChanges }, theme)).into()
                                                })
                                                .child(
                                                    icons::toolbar(icon)
                                                        .text_color(rgb(theme.text.secondary)),
                                                )
                                                .on_click(cx.listener(
                                                    move |root, _, window, cx| {
                                                        cx.stop_propagation();
                                                        root.focus_and_fold_log_files(expand, window, cx);
                                                    },
                                                ))
                                        },
                                    ),
                                ),
                        )
                        .child(files),
                )
                .child(self.render_column_grip(Column::LogMessage, cx))
                .child(
                    div()
                        .id("log-detail-message")
                        .flex_none()
                        .h(px(message_height))
                        .overflow_y_scroll()
                        .py(theme::space::CARD_GAP)
                        .px(theme::space::TREE_PADDING)
                        .flex()
                        .flex_col()
                        .gap(theme::space::TREE_GAP)
                        .child(
                            div()
                                .text_size(theme::size::UI)
                                .text_color(rgb(theme.text.primary))
                                .child(if multiple { ui.log_commit_count(state.selection.shas.len()) } else { commit.summary.clone() }),
                        )
                        .when(!multiple && !commit.body.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.secondary))
                                    .child(commit.body.clone()),
                            )
                        })
                        // The footer: the hash, who wrote it, and the
                        // date in full. *3 hours ago* is what the row
                        // says; a commit one has opened deserves the
                        // date one would paste into a message.
                        .child(
                            div()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.faint))
                                .when(!multiple, |this| this.child(format!(
                                    "{} \u{b7} {} <{}> \u{b7} {}",
                                    commit.short(),
                                    commit.author,
                                    commit.email,
                                    ui.commit_date(
                                        commit.time,
                                        now_seconds(),
                                        crate::system::utc_offset()
                                    )
                                ))),
                        )
                        // *In 2 branches: main, m8-3-4-git-tool.* The last
                        // line, and the one that answers "is this
                        // commit anywhere else" without a command.
                        .children(state.contains.get(&sha).filter(|b| !b.is_empty()).map(
                            |branches| {
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.faint))
                                    .child(ui.in_branches(branches))
                            },
                        )),
                )
                .into_any_element(),
        )
    }
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Cached ancestry, painted in row coordinates so virtualization keeps edges joined.
fn graph_row(
    graph: Rc<crate::log_graph::Graph>,
    index: usize,
    merge: bool,
    opened: bool,
    theme: theme::Theme,
) -> gpui::AnyElement {
    let width = theme::space::LOG_GRAPH_LANE * graph.lanes.max(1) as f32;
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let Some(row) = graph.rows.get(index) else {
                return;
            };
            let lane = bounds.size.width / graph.lanes.max(1) as f32;
            let color = |i: usize| {
                rgb([
                    theme.accent.patina,
                    theme.accent.violet,
                    theme.accent.brass,
                    theme.accent.green,
                    theme.accent.rust,
                ][i % 5])
            };
            let point = |x: usize, y: f32| {
                gpui::point(
                    bounds.left() + lane * (x as f32 + 0.5),
                    bounds.top() + bounds.size.height * y,
                )
            };
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                for edge in &row.edges {
                    let mut path = gpui::PathBuilder::stroke(
                        theme::space::LOG_GRAPH_STROKE.to_pixels(window.rem_size()),
                    );
                    let from = point(edge.from, edge.start);
                    let to = point(edge.to, edge.end);
                    path.move_to(from);
                    path.cubic_bezier_to(
                        to,
                        gpui::point(from.x, (from.y + to.y) / 2.0),
                        gpui::point(to.x, (from.y + to.y) / 2.0),
                    );
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color(edge.color));
                    }
                }
                let size = theme::space::LOG_DOT
                    .to_pixels(window.rem_size())
                    .min(lane / 2.0);
                let center = point(row.lane, 0.5);
                let bounds = gpui::Bounds {
                    origin: center - gpui::point(size / 2.0, size / 2.0),
                    size: gpui::size(size, size),
                };
                window.paint_quad(gpui::quad(
                    bounds,
                    size / 2.0,
                    if merge {
                        rgb(if opened {
                            theme.surface.raised
                        } else {
                            theme.surface.panel
                        })
                    } else {
                        color(row.color)
                    },
                    theme::space::LOG_GRAPH_STROKE.to_pixels(window.rem_size()),
                    color(row.color),
                    gpui::BorderStyle::default(),
                ));
            });
        },
    )
    .flex_none()
    .w(width)
    .max_w(gpui::relative(theme::space::LOG_GRAPH_FRACTION))
    .h_full()
    .into_any_element()
}

/// The branches and tags pointing at this commit, in one chip.
///
/// **One chip and a ceiling**, not one per name and no ceiling: two long
/// branch names covered the message entirely, and the message is what a
/// row is for. What does not fit is shortened by
/// [`crate::git::log::refs_label`], which drops a path's prefix before
/// its tail.
fn ref_chip(name: &str, theme: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .max_w(theme::space::LOG_REFS)
        .truncate()
        .px(px(5.0))
        .rounded(theme::radius::ROW)
        .bg(rgb(theme.accent.patina_bg))
        .text_size(theme::size::META)
        .text_color(rgb(theme.accent.patina))
        .child(name.to_owned())
        .into_any_element()
}

/// How many characters of refs a row will show.
///
/// Read against `LOG_REFS`: at `meta` a character is a little over six
/// pixels, so thirty of them is the hundred and eighty the chip is
/// allowed. The two are checked against each other by the eye, which is
/// why the number is here beside the width rather than in the parser.
const REFS_BUDGET: usize = 30;
