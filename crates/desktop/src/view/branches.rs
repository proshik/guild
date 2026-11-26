//! The branch popup, hanging from the branch in the title bar.
//!
//! Section 6.18 of the design system: two tabs,
//! a field that both narrows the list and offers to make what is typed,
//! and rows of two lines — the name, then who wrote the tip, how long
//! ago and what they called it.
//!
//! **A picker card since M8.9** (section 6.32): hanging from the place the
//! button's canvas records, drawn `deferred` by `view::picker` over the
//! tabs and the body below, over a backdrop that closes it.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

use crate::Root;
use crate::branch_list::Row;
use crate::i18n::Text;
use crate::root::branches::Face;
use crate::theme;
use crate::view::picker::{Card, Place};

impl Root {
    /// The card the branch popup answers with.
    pub(crate) fn branches_card(&self, cx: &mut Context<Root>) -> Card {
        let face = self.branch_face().unwrap_or_default();

        let head = div()
            .flex()
            .flex_col()
            .when(face != Face::Sources, |head| {
                head.child(self.render_branch_tabs(face, cx))
            })
            .child(self.render_branch_query(cx))
            .into_any_element();

        let sources = face == Face::Sources;
        Card {
            id: "branch-card",
            place: Place::Hanging {
                from: if sources {
                    self.update_branch_at.get()
                } else {
                    self.branch_at.get()
                },
                width: if sources {
                    theme::space::UPDATE_SOURCES
                } else {
                    theme::space::BRANCH_POPUP
                },
            },
            backdrop: true,
            head,
            rows: self.render_branch_rows(face, cx),
            preview: None,
            foot: sources.then(|| self.render_update_sources_foot()).flatten(),
        }
    }

    /// Under the sources, only when there is something to say: auto
    /// fast-forward's last skip, which its spec puts here, and one line on
    /// what the last manual update of this root did. Recovery steps and
    /// hashes stay in the update's notice and the Console.
    fn render_update_sources_foot(&self) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let last = self
            .update_snapshot()
            .and_then(|snapshot| self.settings.branch_update_receipts.get(&snapshot.root))
            .and_then(|record| self.ui().branch_update_last(record));
        let lines: Vec<String> = self
            .auto_ff_skip_line(crate::root::auto_ff::Surface::Sources)
            .into_iter()
            .chain(last)
            .collect();
        if lines.is_empty() {
            return None;
        }
        Some(
            div()
                .id("branch-update-details")
                .debug_selector(|| "branch-update-details".into())
                .flex()
                .flex_col()
                .flex_none()
                .py(theme::space::MENU_PADDING)
                .px(theme::space::MENU_INSET)
                .border_t_1()
                .border_color(rgb(theme.surface.divider))
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.faint))
                .children(lines.into_iter().map(|line| div().truncate().child(line)))
                .into_any_element(),
        )
    }

    /// The two tabs at the top of the card.
    fn render_branch_tabs(&self, chosen: Face, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        let tab = |face: Face, label: Text, cx: &mut Context<Root>| {
            let chosen = face == chosen;
            div()
                .id(match face {
                    Face::Branches => "branch-tab-branches",
                    Face::Stashes => "branch-tab-stashes",
                    Face::Sources => "branch-tab-sources",
                })
                .flex()
                .items_center()
                .h(theme::space::BRANCH_TAB)
                .px(theme::space::CARD_PADDING)
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .text_size(theme::size::UI)
                .when(chosen, |t| {
                    t.bg(rgb(theme.surface.raised))
                        .text_color(rgb(theme.text.primary))
                })
                .when(!chosen, |t| {
                    t.text_color(rgb(theme.text.faint))
                        .hover(|t| t.bg(crate::theme::control_hover(theme, theme.surface.card)))
                })
                .child(self.ui().text(label))
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.show_branch_face(face, cx);
                }))
        };

        div()
            .flex()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .p(theme::space::CARD_GAP)
            .child(tab(Face::Branches, Text::BranchesTab, cx))
            .child(tab(Face::Stashes, Text::StashesTab, cx))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.gutter))
                    .child(ui.text(Text::BranchHint)),
            )
            .into_any_element()
    }

    /// The field: the picker's, under the tabs and over a hairline.
    fn render_branch_query(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .flex()
            .items_center()
            .h(theme::space::BRANCH_FIELD)
            .px(theme::space::CARD_PADDING)
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .text_size(theme::size::UI)
            .child(
                self.picker_field(
                    ui.text(if self.branch_face() == Some(Face::Sources) {
                        Text::UpdateSourceSearch
                    } else {
                        Text::BranchSearch
                    })
                    .to_owned(),
                    theme.surface.card,
                    cx,
                ),
            )
            .into_any_element()
    }

    /// The list: the create rows when there are some, the *Local* heading,
    /// then the branches — or the stashes.
    fn render_branch_rows(&self, face: Face, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let lit = self.picker_lit_row();
        let query = self.picker_query().trim().to_owned();

        let items: Vec<gpui::AnyElement> = match face {
            Face::Sources => {
                let chosen = self.update_source().map(str::to_owned);
                self.update_source_rows()
                    .iter()
                    .enumerate()
                    .map(|(row, source)| {
                        let checked = chosen.as_deref() == Some(source.reference.as_str());
                        self.render_update_source_row(source, row, lit == Some(row), checked, cx)
                    })
                    .collect()
            }
            Face::Branches => {
                let rows = self.branch_rows_shown();
                if rows.is_empty() {
                    return self.popup_note(Text::BranchesNone);
                }
                let branches = self.branches_here().to_vec();
                rows.iter()
                    .enumerate()
                    .filter_map(|(row, shown)| match shown {
                        Row::Create => {
                            Some(self.render_branch_create(row, &query, lit == Some(row), cx))
                        }
                        // And the same name as a worktree of its own: the
                        // popup offers both rather than the window growing
                        // a fourth field for the second. M12.
                        Row::Worktree => {
                            Some(self.render_worktree_create(row, &query, lit == Some(row), cx))
                        }
                        // **Only above branches there are**, and never lit:
                        // `branch_list::rows` draws it only then.
                        Row::Heading => Some(
                            div()
                                // Aligned with a row's own inset, as every
                                // heading in the picker family is — M12.7
                                // task 9.
                                .px(theme::space::MENU_INSET)
                                .py(px(4.0))
                                .text_size(theme::size::EYEBROW)
                                .text_color(rgb(theme.text.faint))
                                .child(ui.text(Text::BranchesLocal))
                                .into_any_element(),
                        ),
                        Row::Branch(index) => branches.get(*index).map(|branch| {
                            self.render_branch_row(branch, row, lit == Some(row), cx)
                        }),
                    })
                    .collect()
            }
            Face::Stashes => {
                let rows = self.stash_rows_shown();
                if rows.is_empty() {
                    return self.popup_note(Text::StashesNone);
                }
                let stashes = self.stashes_here().to_vec();
                rows.iter()
                    .enumerate()
                    .filter_map(|(row, index)| {
                        stashes
                            .get(*index)
                            .map(|stash| self.render_stash_row(stash, row, lit == Some(row), cx))
                    })
                    .collect()
            }
        };
        // The arrows move the highlight and the picker carries it into
        // view: thirteen branches do not fit in 320 pixels, and a
        // highlight below the fold is a branch switched to blind.
        let cap = if face == Face::Sources {
            theme::space::UPDATE_SOURCES_LIST
        } else {
            theme::space::BRANCH_LIST
        };
        self.picker_children("branch-rows", items, Some(cap), cx)
    }

    /// One Update branch source: the remembered one's check, the name as
    /// a person reads it, and — faint and smaller, at the end — whether it
    /// is a local branch with nowhere to fetch from, or which upstream a
    /// local branch is updated from.
    ///
    /// **The one highlight is a menu's**, `control_hover` over the card,
    /// for the row under the pointer and the row the arrows lit alike:
    /// `surface.raised` on `surface.card` was a step too small to see.
    fn render_update_source_row(
        &self,
        source: &crate::git::branch_update::Source,
        row: usize,
        lit: bool,
        checked: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let name = crate::branch_list::source_name(&source.reference).to_owned();
        let tag = if !source.upstream.is_empty() {
            Some(format!(
                "\u{2192} {}",
                crate::branch_list::source_name(&source.upstream)
            ))
        } else if source.reference.starts_with("refs/heads/") {
            Some(ui.text(Text::UpdateSourceLocal).to_owned())
        } else {
            None
        };
        self.picker_item(
            row,
            div().id(gpui::SharedString::from(format!("update-source-{row}"))),
            cx,
        )
        .debug_selector(move || format!("update-source-{row}"))
        .flex()
        .flex_none()
        .items_center()
        .gap(theme::space::MENU_ICON_GAP)
        .h(theme::space::MENU_ROW)
        .mx(theme::radius::ROW)
        .px(crate::view::popup::row_inset())
        .rounded(theme::radius::ROW)
        .cursor_pointer()
        .when(lit, |row| {
            row.bg(crate::theme::control_hover(theme, theme.surface.card))
        })
        .child(crate::view::popup::check_column(checked, theme))
        // The name keeps its width and gives way only when it alone is
        // wider than the row; the tag takes what is left, right-aligned,
        // and is the first to be cut.
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(theme::size::UI)
                .text_color(rgb(theme.text.primary))
                .child(name),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_right()
                .text_size(theme::size::EYEBROW)
                .text_color(rgb(crate::view::popup::trailing_ink(theme, lit)))
                .children(tag),
        )
        .into_any_element()
    }

    /// One branch: the mark, the name, and the tip under it.
    fn render_branch_row(
        &self,
        branch: &crate::git::branches::Branch,
        row: usize,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let name = branch.name.clone();
        let current = branch.current;
        let beside = ui.branch_tip(&branch.author, &branch.age, &branch.subject);

        self.picker_item(
            row,
            div().id(gpui::SharedString::from(format!("branch-row-{name}"))),
            cx,
        )
        .flex()
        .items_center()
        .gap(theme::space::TREE_GAP)
        .h(theme::space::BRANCH_ROW)
        .mx(theme::radius::ROW)
        .px(crate::view::popup::row_inset())
        .rounded(theme::radius::ROW)
        .cursor_pointer()
        .when(lit, |lit_row| lit_row.bg(rgb(theme.surface.raised)))
        // The pointer moves the one highlight and a click takes the
        // row — `picker_item`, and the take is `branches_take`.
        .child(
            div().flex_none().w(theme::space::CHEVRON).child(
                crate::icons::glyph(
                    if current {
                        "ui/check.svg"
                    } else {
                        "ui/workspace-git-branch.svg"
                    },
                    theme::size::UI,
                )
                .text_color(rgb(if current {
                    theme.accent.brass
                } else {
                    theme.text.faint
                })),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme::size::UI)
                        .text_color(rgb(theme.text.primary))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(beside),
                ),
        )
        // The bin, on the lit row and never on the branch one is on:
        // git refuses to delete that, and a control whose only answer
        // is a refusal should not be drawn.
        .when(lit && !current, |row| {
            row.child(self.render_branch_bin(name.clone(), cx))
        })
        .into_any_element()
    }

    /// The bin at the end of a row.
    ///
    /// A warning-coloured glyph whenever deletion is offered; the branch
    /// row still switches branches, while only this control deletes one.
    fn render_branch_bin(&self, name: String, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;

        div()
            .id(gpui::SharedString::from(format!("branch-bin-{name}")))
            .flex_none()
            .px(px(4.0))
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .text_size(theme::size::UI)
            .text_color(rgb(theme.accent.rust))
            .hover(|bin| bin.bg(crate::theme::control_hover(theme, theme.surface.card)))
            .child("\u{2326}")
            .on_click(cx.listener(move |root, _event, _window, cx| {
                // The row under it means "switch to this branch"; the bin
                // means something else entirely.
                cx.stop_propagation();
                root.delete_branch(name.clone(), cx);
            }))
            .into_any_element()
    }

    /// One stash: its reference, and what it is called under it.
    fn render_stash_row(
        &self,
        stash: &crate::git::stashes::Stash,
        row: usize,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;

        self.picker_item(
            row,
            div().id(gpui::SharedString::from(format!(
                "stash-row-{}",
                stash.reference
            ))),
            cx,
        )
        .flex()
        .items_center()
        .gap(theme::space::TREE_GAP)
        .h(theme::space::BRANCH_ROW)
        .mx(theme::radius::ROW)
        .px(crate::view::popup::row_inset())
        .rounded(theme::radius::ROW)
        .cursor_pointer()
        .when(lit, |lit_row| lit_row.bg(rgb(theme.surface.raised)))
        // A click applies; it does not drop. The bin is the only way
        // to lose an entry, and it is a separate thing to aim at.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme::size::UI)
                        .text_color(rgb(theme.text.primary))
                        .child(stash.reference.clone()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(format!("{} \u{b7} {}", stash.age, stash.subject)),
                ),
        )
        .when(lit, |lit_row| {
            lit_row.child(self.render_stash_bin(stash.clone(), cx))
        })
        .into_any_element()
    }

    /// The bin on a stash row.
    ///
    /// Dropping is the one gesture here that loses work git cannot get
    /// back — which is why applying does not do it: `stash apply` leaves
    /// the entry, and this is the only thing that removes one.
    ///
    /// It asks first, in a sheet naming the entry, and the whole entry goes
    /// with the click — its commit is what the drop is carried by.
    fn render_stash_bin(
        &self,
        stash: crate::git::stashes::Stash,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;

        div()
            .id(gpui::SharedString::from(format!(
                "stash-bin-{}",
                stash.reference
            )))
            .flex_none()
            .px(px(4.0))
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .text_size(theme::size::UI)
            .text_color(rgb(theme.accent.rust))
            .hover(|bin| bin.bg(crate::theme::control_hover(theme, theme.surface.card)))
            .child("\u{2326}")
            .on_click(cx.listener(move |root, _event, window, cx| {
                cx.stop_propagation();
                root.drop_stash_asking(stash.clone(), window, cx);
            }))
            .into_any_element()
    }

    /// The row that offers to make what was typed.
    fn render_branch_create(
        &self,
        row: usize,
        name: &str,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        self.picker_item(row, div().id("branch-create"), cx)
            .flex()
            .items_center()
            .h(theme::space::BRANCH_ROW)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .text_size(theme::size::UI)
            .text_color(rgb(theme.accent.brass))
            .child(ui.create_branch(name))
            .into_any_element()
    }

    /// The row under it: the same name as a worktree of its own — M12.
    ///
    /// Brass like the branch above it, because both make something; what
    /// tells them apart is the word, not the colour.
    fn render_worktree_create(
        &self,
        row: usize,
        name: &str,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        self.picker_item(row, div().id("worktree-create"), cx)
            .flex()
            .items_center()
            .h(theme::space::BRANCH_ROW)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .text_size(theme::size::UI)
            .text_color(rgb(theme.accent.brass))
            .child(ui.create_worktree(name))
            .into_any_element()
    }

    /// One quiet line where rows would be.
    fn popup_note(&self, text: Text) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .p(theme::space::CARD_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(self.ui().text(text))
            .into_any_element()
    }
}
