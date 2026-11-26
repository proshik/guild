//! The Log's toolbar: a field and three dropdowns.
//!
//! In this order — *Text or hash*, then *Branch*, *User*
//! and *Date*. What each of them does is an argument of `git log`
//! (decision 3 of M8.3.4), so a filter narrows the whole history rather
//! than the two hundred commits that happen to be loaded.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

use crate::Root;
use crate::i18n::Text;
use crate::theme;
use crate::view::menu::log_filter::{FilterMenu, Since};
use crate::view::menu::{Anchor, Target};

impl Root {
    /// The strip across the top of the Log.
    ///
    /// **One row, and it follows the columns:** *Branch or tag* over the
    /// branch column and *Text or hash* over the commits, on the same
    /// line. Ours had the branch field inside the column below, so
    /// the two sat at different heights and the filter strip started at
    /// the panel's left edge whatever the columns were doing.
    ///
    /// `branches` is that column's width when there are three columns,
    /// and `None` in a side dock, where there is no branch column to sit
    /// over.
    pub(crate) fn render_log_toolbar(
        &self,
        branches: Option<f32>,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;

        div()
            .flex_none()
            .flex()
            .items_center()
            .h(theme::space::LOG_TOOLBAR)
            .children(branches.map(|width| {
                div()
                    .flex_none()
                    .w(px(width))
                    .px(theme::space::TREE_PADDING)
                    .border_r_1()
                    .border_color(rgb(theme.surface.divider))
                    .child(self.render_branch_field(cx))
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(theme::space::TREE_GAP)
                    .px(theme::space::TREE_PADDING)
                    .child(self.render_log_field(cx))
                    .child(self.render_filter_button(FilterMenu::Branch, cx))
                    .child(self.render_filter_button(FilterMenu::User, cx))
                    .child(self.render_filter_button(FilterMenu::Date, cx))
                    .child(div().flex_1()),
            )
            .into_any_element()
    }

    /// *Branch or tag*: it narrows the column under it and asks git
    /// nothing — the branches are already loaded, and re-walking the log
    /// to find a name among twelve would be absurd.
    fn render_branch_field(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let query = self.branch_query();
        let empty = query.is_empty();

        div()
            .id("log-branch-field")
            .h(theme::space::HEAD_TAB)
            .flex()
            .items_center()
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.inset))
            .cursor_pointer()
            .when(
                self.log_typing() == crate::root::log::Typing::Branch,
                |field| field.border_1().border_color(rgb(theme.accent.brass)),
            )
            .text_size(theme::size::META)
            .text_color(rgb(if empty {
                theme.text.faint
            } else {
                theme.text.primary
            }))
            .child(
                crate::text_field::TextField::new(
                    "log-branch-input",
                    query,
                    cx.listener(|root, value: &String, _, cx| {
                        if let Some(project) = root.active_project
                            && let Some(state) = root.logs.get_mut(&project)
                        {
                            state.branch_query = value.clone();
                        }
                        cx.notify();
                    }),
                )
                .placeholder(ui.text(Text::LogBranchSearch))
                .list()
                .when(
                    self.log_typing() == crate::root::log::Typing::Branch,
                    |input| input.focus(&self.log().expect("log state").focus),
                )
                .on_focus(cx.listener(|root, _, _, cx| {
                    root.type_into_log(crate::root::log::Typing::Branch, cx)
                }))
                .on_key(cx.listener(|root, event, window, cx| {
                    root.log_key(event, window, cx);
                })),
            )
            .into_any_element()
    }

    /// The field. It types into the filter and asks git again.
    fn render_log_field(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let text = self.log_filter().text;
        let empty = text.is_empty();

        div()
            .flex_none()
            .w(theme::space::LOG_FIELD)
            .h(theme::space::HEAD_TAB)
            .flex()
            .items_center()
            .px(theme::space::CARD_GAP)
            .id("log-filter-field")
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.inset))
            .cursor_pointer()
            .when(
                self.log_typing() == crate::root::log::Typing::Filter,
                |field| field.border_1().border_color(rgb(theme.accent.brass)),
            )
            .text_size(theme::size::META)
            .text_color(rgb(if empty {
                theme.text.faint
            } else {
                theme.text.primary
            }))
            .child(
                crate::text_field::TextField::new(
                    "log-log-input",
                    text,
                    cx.listener(|root, value: &String, _, cx| {
                        root.set_log_text(value.clone(), cx);
                    }),
                )
                .placeholder(ui.text(Text::LogSearch))
                .list()
                .when(
                    self.log_typing() == crate::root::log::Typing::Filter,
                    |input| input.focus(&self.log().expect("log state").focus),
                )
                .on_focus(cx.listener(|root, _, _, cx| {
                    root.type_into_log(crate::root::log::Typing::Filter, cx)
                }))
                .on_key(cx.listener(|root, event, window, cx| {
                    root.log_key(event, window, cx);
                })),
            )
            .into_any_element()
    }

    /// Whether `which` dropdown's menu is the one up — M12.7 task 6.
    pub(crate) fn log_filter_open(&self, which: FilterMenu) -> bool {
        matches!(
            self.context_menu.as_ref().map(|open| &open.target),
            Some(Target::LogFilter(list)) if list.kind() == which
        )
    }

    /// One dropdown: its name and what it is set to.
    ///
    /// **Its list is a menu of the engine** — M12.7 task 6: the button
    /// gathers what the list shows and asks `open_menu` to hang it under
    /// itself, section 6.17. Its bounds are recorded only while its menu
    /// is not up, the tab strip's trigger rule — the menu hangs from the
    /// place it was opened under — and it stays lit while the menu is up.
    fn render_filter_button(&self, which: FilterMenu, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let filter = self.log_filter();
        let open = self.log_filter_open(which);
        let at = self.log_filter_at.clone();
        let slot = which as usize;

        // **The name and the value, always both.** *Branch* alone is a
        // control that hides its own state; *Branch: HEAD* says what the
        // list is showing without being opened.
        let (name, value, narrowed) = match which {
            FilterMenu::Branch => match self.log_scope() {
                // Every branch is the unnarrowed log, so it wears no
                // cross; `HEAD` is a choice like any branch and does.
                crate::git::Scope::All => (
                    Text::LogBranchFilter,
                    ui.text(Text::LogAllBranches).to_owned(),
                    false,
                ),
                crate::git::Scope::Head => (
                    Text::LogBranchFilter,
                    ui.text(Text::LogHead).to_owned(),
                    true,
                ),
                crate::git::Scope::Named(branch) => (Text::LogBranchFilter, branch, true),
            },
            FilterMenu::User => match &filter.author {
                Some(author) => (Text::LogUserFilter, author.clone(), true),
                None => (
                    Text::LogUserFilter,
                    ui.text(Text::LogAnyone).to_owned(),
                    false,
                ),
            },
            FilterMenu::Date => match filter.since.as_deref() {
                Some(since) => (
                    Text::LogDateFilter,
                    ui.text(Since::from_arg(since).map_or(Text::LogDateFilter, Since::text))
                        .to_owned(),
                    true,
                ),
                None => (
                    Text::LogDateFilter,
                    ui.text(Text::LogAnyTime).to_owned(),
                    false,
                ),
            },
        };

        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(match which {
                        FilterMenu::Branch => "log-filter-branch",
                        FilterMenu::User => "log-filter-user",
                        FilterMenu::Date => "log-filter-date",
                    })
                    .relative()
                    .h(theme::space::HEAD_TAB)
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .pl(theme::space::CARD_GAP)
                    .pr(px(if narrowed { 18.0 } else { 8.0 }))
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .text_size(theme::size::META)
                    .when(open, |button| button.bg(rgb(theme.surface.raised)))
                    .hover(|button| {
                        button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                    })
                    .child(
                        div()
                            .text_color(rgb(theme.text.secondary))
                            .child(format!("{}:", ui.text(name))),
                    )
                    .child(
                        div()
                            .text_color(rgb(if narrowed {
                                theme.accent.brass
                            } else {
                                theme.text.primary
                            }))
                            .child(value),
                    )
                    // A cross where the chevron is, once the filter is
                    // set: taking a narrowing off is a gesture of its own,
                    // and opening a list to find *All* is a gesture and a
                    // half.
                    .when(!narrowed, |button| {
                        button.child(crate::icons::inline_chevron(
                            true,
                            theme::size::META,
                            theme.text.faint,
                        ))
                    })
                    // Where it ended up, for the menu that hangs under it.
                    .child(
                        gpui::canvas(
                            move |bounds, window, _cx| {
                                let mut places = at.get();
                                if !open && places[slot] != Some(bounds) {
                                    places[slot] = Some(bounds);
                                    at.set(places);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .on_click(
                        cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            // A second press on a lit button lands on the
                            // menu's backdrop, which closes it; this is a
                            // press with no menu of its own up.
                            let anchor = match root.log_filter_at.get()[slot] {
                                Some(bounds) => Anchor::Below(bounds),
                                None => Anchor::Pointer(event.position()),
                            };
                            let list = root.filter_list(which);
                            root.open_menu(Target::LogFilter(list), anchor, window, cx);
                        }),
                    ),
            )
            .children(narrowed.then(|| {
                div()
                    .id("log-filter-clear")
                    .absolute()
                    .right(px(2.0))
                    .top(px(4.0))
                    .px(px(3.0))
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .group("log-filter-clear")
                    .child(
                        crate::icons::glyph("icons/close.svg", theme::size::META)
                            .text_color(rgb(theme.text.faint))
                            .group_hover("log-filter-clear", |cross| {
                                cross.text_color(rgb(theme.accent.rust))
                            }),
                    )
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        cx.stop_propagation();
                        root.clear_log_filter(which, cx);
                    }))
            }))
            .into_any_element()
    }
}
