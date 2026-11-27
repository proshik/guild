//! The start screen, drawn — M14.5, section 6.35 of the design system.
//!
//! What a worktree shows with no tab open: the heading, the default
//! harness's card, a tile per kind of tab, the recent sessions with their
//! chips, and the foot. The model — the rows, the chips, the one
//! highlight, what a key does — is `crate::start`; the keys, the reading
//! and the deeds are `root::start`. This is only the picture of them.
//!
//! **One highlight on the whole screen** (6.32's rules 1 and 2, and
//! `docs/traps.md`'s "A hover tint in the highlight's colour is a second
//! highlight"): the card's outline or a row's `surface.raised`, as
//! `start.lit` says, moved by the pointer on a *move* rather than on
//! hover. A tile under the pointer is raised too, but a tile is never
//! lit: `enter` does not take one.

use std::time::SystemTime;

use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Hsla, MouseMoveEvent, Window, div, px, rgb,
};
use guild_session::history::Past;
use guild_session::{DefaultTab, HarnessKind};

use crate::Root;
use crate::i18n::Text;
use crate::start::{Deed, Filter, Fits, Lit, harness_of, tile_key};
use crate::theme;
use crate::view::kind::{Shade, default_tab_text, harness_mark};
use crate::view::settings::Section;

impl Root {
    /// The start screen, in place of the hints whenever a project is in
    /// front with no tab open.
    ///
    /// **Its focus and keys are the screen's element's own**: `refocus`
    /// and the render's rule put the focus on `start_focus` whenever the
    /// screen is up, and a handle nobody draws is a dispatch path with no
    /// window on it — not one of its keys would work.
    ///
    /// **Drawn in every empty area since M12.8, and `focused` in one** —
    /// decision 16: only the focused place's copy tracks `start_focus` and
    /// `start_scroll`, so `1`, `t` and `enter` act there and nowhere else,
    /// and one handle is never measured by two lists. A press on another
    /// copy focuses its area first (`view::areas`), and a press in another
    /// pane its pane (`view::panes`).
    ///
    /// **Compact in a small place** — decision 20, as its second window
    /// check amended it: `fits` says which parts are drawn; the default
    /// card always is, and the tiles wrap in a narrow place.
    pub(crate) fn render_start(
        &self,
        focused: bool,
        fits: Fits,
        _window: &Window,
        cx: &mut Context<Root>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // **Clipped by the card**, the hints' rule, for what does not
            // fit down; across, the column narrows (below).
            .overflow_hidden()
            .flex_col()
            .items_center()
            .justify_center()
            .py(theme::space::START_GAP)
            // **Never flush with a card's edge** — M12.8's window check:
            // in a divided area the column ran to both of them. The
            // parts' own gap stands at either side.
            .px(theme::space::START_GAP)
            .when(focused, |screen| {
                screen
                    .track_focus(&self.start_focus)
                    .key_context("start")
                    .on_key_down(cx.listener(Root::start_key))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    // **560 at most** — M12.8: an area can be narrower than
                    // the column, and the column narrows with it rather
                    // than hanging over the card's edges.
                    .w_full()
                    .max_w(theme::space::START_COLUMN)
                    // The list gives way when the room is short, and only
                    // the list: every other part is `flex_none`.
                    .min_h_0()
                    .max_h_full()
                    .gap(theme::space::START_GAP)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .gap(theme::space::PANEL_PADDING)
                            .when(fits.heading, |group| group.child(self.start_heading()))
                            .child(self.start_card(focused, cx)),
                    )
                    // Gone rather than cut in a short place — `start::fits`;
                    // their keys still work, only the tiles are not drawn.
                    .when(fits.tiles, |column| {
                        column.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_none()
                                .gap(theme::space::CARD_GAP)
                                .child(self.start_rubric(Text::StartAllTools, None))
                                .child(self.start_tiles(focused, fits.two_rows, cx)),
                        )
                    })
                    .map(|column| {
                        if fits.recent {
                            column.child(self.start_recent(focused, cx))
                        } else {
                            self.note_start_ages();
                            column
                        }
                    })
                    .when(fits.foot, |column| column.child(self.start_foot(cx))),
            )
            .into_any_element()
    }

    /// The folder every copy of the screen is about: the project in
    /// front's, whichever area the copy is drawn in — `start_showing` is
    /// the focused area's question, and an empty area beside a tab asks
    /// this one.
    fn start_folder(&self) -> Option<std::path::PathBuf> {
        let project = self.active_project?;
        self.desk
            .project(project)
            .map(|project| project.path.clone())
    }

    /// The folder's name, and under it the branch and what the screen is
    /// for — one line of text, centred as one.
    fn start_heading(&self) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let name = self
            .start_folder()
            .and_then(|folder| folder.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let choose = ui.text(Text::StartChoose);
        let line = match &self.branch {
            Some(branch) => format!("{branch} · {choose}"),
            None => choose.to_owned(),
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(theme::size::SECTION)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text.primary))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .whitespace_nowrap()
                    .text_center()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.tertiary))
                    .when(self.branch.is_some(), |line| {
                        line.child(
                            crate::icons::glyph("ui/workspace-git-branch.svg", theme::size::META)
                                .text_color(rgb(theme.text.tertiary)),
                        )
                    })
                    .child(line),
            )
            .into_any_element()
    }

    /// What `enter` starts: the default tab, lit by its outline while the
    /// highlight is on it.
    ///
    /// **Lit only in the focused copy** — M12.8: every empty area draws
    /// the screen, and §6.35's one highlight is the focused place's.
    fn start_card(&self, focused: bool, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let tab = self.default_tab();
        let lit = focused && self.start.lit == Lit::Card;
        let mut outline: Hsla = rgb(theme.accent.brass).into();
        outline.a = if lit { theme::alpha::LIT_OUTLINE } else { 0.0 };

        div()
            .id("start-card")
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .h(theme::space::START_CARD)
            .px(theme::space::CARD_PADDING)
            .rounded(theme::radius::CARD)
            .bg(rgb(theme.surface.card))
            // Always a point of border, clear when not lit, so lighting it
            // moves nothing.
            .border_1()
            .border_color(outline)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(theme::space::START_DISC)
                    .rounded_full()
                    .bg(rgb(theme.surface.raised))
                    .child(harness_mark(
                        tab,
                        theme,
                        theme::space::START_MARK,
                        Shade::Full,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_size(theme::size::CARD_TITLE)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text.primary))
                            .child(tab_name(ui, tab)),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.tertiary))
                            .child(ui.text(Text::StartDefault)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(self.code_font.clone())
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.faint))
                    .child("↵"),
            )
            // The pointer moves the highlight of the copy it is over, and
            // only the focused copy has one: a move over another copy would
            // move the focused one's from afar.
            .when(focused, |card| {
                card.on_mouse_move(cx.listener(|root, _event: &MouseMoveEvent, _window, cx| {
                    root.start_point(Lit::Card, cx);
                }))
            })
            .on_click(cx.listener(|root, _event: &ClickEvent, window, cx| {
                root.start_deed(Deed::Default, window, cx);
            }))
            .into_any_element()
    }

    /// A part's heading in a menu heading's voice (6.17), its rule running
    /// to the column's edge — or to 10 before `right`, the chips.
    fn start_rubric(&self, label: Text, right: Option<AnyElement>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .h(theme::space::HEAD_TAB)
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::EYEBROW)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text.faint))
                    .child(self.ui().text(label)),
            )
            .child(div().flex_1().h(px(1.0)).bg(rgb(theme.surface.divider)))
            .children(right)
            .into_any_element()
    }

    /// A tile per kind of tab, in the harness menu's order.
    ///
    /// The tile under the pointer is one field for every copy, so only
    /// the focused copy raises one and hears the hover — M12.8: a tile in
    /// another copy would otherwise raise its twin in the focused one.
    fn start_tiles(&self, focused: bool, two_rows: bool, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let mut tiles: Vec<AnyElement> = DefaultTab::ALL
            .into_iter()
            .map(|tab| {
                // §6.5's rule: a harness the `PATH` does not have is drawn
                // and never taken; the terminal needs nothing on it.
                let enabled = harness_of(tab).is_none_or(|kind| self.available.contains(&kind));
                let under = focused && enabled && self.start_tile == Some(tab);
                let tile = div()
                    .id(("start-tile", tile_key(tab).as_bytes()[0] as usize))
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .gap(theme::space::CARD_INNER_GAP)
                    .h(theme::space::START_TILE)
                    .pt(theme::space::PANEL_PADDING)
                    // **Its name clear of its edges, its mark clear of its
                    // key** — window check 3: at the floor the names
                    // touched the edges and Codex's mark ran under its
                    // `3`. The mark has the same air at its sides as above
                    // it, which the key, 10 in from the corner, sits in.
                    .px(theme::space::CARD_INNER_GAP)
                    .min_w(theme::space::START_TILE_MARK + theme::space::PANEL_PADDING * 2.0)
                    .rounded(theme::radius::CARD)
                    .bg(rgb(if under {
                        theme.surface.raised
                    } else {
                        theme.surface.card
                    }))
                    .child(
                        div()
                            .absolute()
                            .top(theme::space::CARD_INNER_GAP)
                            .right(theme::space::CARD_INNER_GAP)
                            .font_family(self.code_font.clone())
                            .text_size(theme::size::EYEBROW)
                            .text_color(rgb(theme.text.faint))
                            .child(tile_key(tab).to_uppercase()),
                    )
                    .child(harness_mark(
                        tab,
                        theme,
                        theme::space::START_TILE_MARK,
                        if enabled {
                            Shade::Full
                        } else {
                            Shade::Disabled
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .whitespace_nowrap()
                                    .text_size(theme::size::UI)
                                    .text_color(rgb(if !enabled {
                                        theme.text.faint
                                    } else if under {
                                        theme.text.primary
                                    } else {
                                        theme.text.secondary
                                    }))
                                    .child(tab_name(ui, tab)),
                            )
                            .when(!enabled, |names| {
                                names.child(
                                    div()
                                        .whitespace_nowrap()
                                        .text_size(theme::size::EYEBROW)
                                        .text_color(rgb(theme.text.faint))
                                        .child(ui.text(Text::StartNotOnPath)),
                                )
                            }),
                    );
                if !enabled {
                    // No pointer and no click: a tile drawn, not a place to
                    // stand.
                    return tile.into_any_element();
                }
                tile.when(focused, |tile| {
                    tile.on_hover(cx.listener(move |root, hovered: &bool, _window, cx| {
                        let now = if *hovered {
                            Some(tab)
                        } else if root.start_tile == Some(tab) {
                            None
                        } else {
                            return;
                        };
                        if root.start_tile != now {
                            root.start_tile = now;
                            cx.notify();
                        }
                    }))
                })
                .on_click(cx.listener(move |root, _event: &ClickEvent, window, cx| {
                    root.start_deed(Deed::Tile { tab }, window, cx);
                }))
                .into_any_element()
            })
            .collect();
        let row = |tiles: Vec<AnyElement>| div().flex().gap(theme::space::CARD_GAP).children(tiles);
        if !two_rows {
            // **They wrap in a narrow place** — decision 20. A tile gives
            // way no further than its name (no `min_w_0`), so four that
            // cannot stand side by side go to a second line rather than
            // cutting their names; `fits` counts one row only where four
            // stand, so this is a guard, not a layout.
            return row(tiles).flex_wrap().into_any_element();
        }
        // Two rows of two, as `start::fits` counted them from the same
        // width — a wrap left to the layout drew three and one, or one
        // row where the height was counted for two.
        let second = tiles.split_off(tiles.len().div_ceil(2));
        div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(row(tiles))
            .child(row(second))
            .into_any_element()
    }

    /// *Recent*: its heading with the chips, and the rows — which alone
    /// give way, and scroll, when the room is short.
    fn start_recent(&self, focused: bool, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        // The previous folder's rows never stand under this one's name.
        let here = self
            .start_folder()
            .is_some_and(|folder| self.start_rows_for(&folder));
        let ages = self.note_start_ages();

        let chips = if here { self.start.chips() } else { vec![] };
        let chips = (!chips.is_empty()).then(|| self.start_chips(chips, cx));

        let body: Option<AnyElement> = if !here {
            // Still reading: the room stays empty.
            None
        } else if self.start.rows.is_empty() {
            Some(
                div()
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(Text::StartNoSessions))
                    .into_any_element(),
            )
        } else {
            let rows = self
                .start
                .shown()
                .into_iter()
                .zip(ages)
                .enumerate()
                .map(|(index, (past, age))| self.start_row(index, past, age, focused, cx))
                .collect::<Vec<_>>();
            let list = div()
                .id("start-rows")
                .flex()
                .flex_col()
                .min_h_0()
                .overflow_y_scroll()
                // One list measures the handle: the focused copy's.
                .when(focused, |list| list.track_scroll(&self.start_scroll))
                .children(rows);
            Some(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    // 8 wider than the column on each side, so a row's
                    // mark stands on the headings' line.
                    .mx(-theme::space::CARD_GAP)
                    .child(list)
                    .when_some(
                        focused
                            .then(|| crate::view::thumb(&self.start_scroll))
                            .flatten(),
                        |area, (top, length)| {
                            area.child(
                                div()
                                    .absolute()
                                    .right(px(2.0))
                                    .top(px(top))
                                    .w(theme::space::SCROLLBAR)
                                    .h(px(length))
                                    .rounded_full()
                                    .bg(rgb(theme.text.faint)),
                            )
                        },
                    )
                    .into_any_element(),
            )
        };

        div()
            .flex()
            .flex_col()
            .min_h_0()
            .gap(theme::space::CARD_GAP)
            .child(self.start_rubric(Text::StartRecent, chips))
            .children(body)
            .into_any_element()
    }

    /// The rows' age words as they read now, recorded as drawn — what
    /// `poll_start` compares with to repaint once a minute at most. A
    /// compact screen draws no rows and records them all the same: words
    /// never recorded would read as moved on every tick, and a clock that
    /// notifies redraws the whole window (traps.md).
    fn note_start_ages(&self) -> Vec<String> {
        let ui = self.ui();
        let here = self
            .start_folder()
            .is_some_and(|folder| self.start_rows_for(&folder));
        let ages = if here {
            self.start
                .ages(SystemTime::now(), |seconds| ui.ago(seconds))
        } else {
            Vec::new()
        };
        *self.start_ages.borrow_mut() = ages.clone();
        ages
    }

    /// The Git dock's tabs (6.13), as a filter: *All*, then a harness with
    /// sessions, each with its count.
    fn start_chips(&self, chips: Vec<(Filter, usize)>, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .children(
                chips
                    .into_iter()
                    .enumerate()
                    .map(|(index, (filter, count))| {
                        let chosen = self.start.filter == filter;
                        let name = match filter {
                            Filter::All => ui.text(Text::StartAll),
                            Filter::Only(kind) => ui.text(default_tab_text(tab_of(kind))),
                        };
                        div()
                            .id(("start-chip", index))
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(theme::space::CARD_INNER_GAP)
                            .h(theme::space::HEAD_TAB)
                            .px(theme::space::CARD_GAP)
                            .rounded(theme::radius::ROW)
                            .text_size(theme::size::EYEBROW)
                            .font_weight(FontWeight::MEDIUM)
                            .when(chosen, |chip| chip.bg(rgb(theme.surface.raised)))
                            .child(
                                div()
                                    .text_color(rgb(if chosen {
                                        theme.text.primary
                                    } else {
                                        theme.text.faint
                                    }))
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_color(rgb(if chosen {
                                        theme.text.tertiary
                                    } else {
                                        theme.text.faint
                                    }))
                                    .child(count.to_string()),
                            )
                            .on_click(cx.listener(move |root, _event: &ClickEvent, _window, cx| {
                                root.start.choose(filter);
                                root.start_scroll.scroll_to_item(0);
                                cx.notify();
                            }))
                    }),
            )
            .into_any_element()
    }

    /// A recent session: its mark, its title, its age — or *Resume ↵*
    /// while it is the one lit.
    fn start_row(
        &self,
        index: usize,
        past: &Past,
        age: String,
        focused: bool,
        cx: &mut Context<Root>,
    ) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        // The focused copy's alone, as the card's is.
        let lit = focused && self.start.lit == Lit::Row(index);
        // OpenCode names an untitled session *New session* in English;
        // that is its data, and this is where it becomes a word of ours.
        let title = if past.harness == HarnessKind::OpenCode && past.title == "New session" {
            ui.text(Text::StartNewSession).to_owned()
        } else {
            past.title.clone()
        };
        let right = if lit {
            format!("{} ↵", ui.text(Text::StartResume))
        } else {
            age
        };
        div()
            .id(("start-row", index))
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .h(theme::space::MENU_ROW)
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .child(harness_mark(
                tab_of(past.harness),
                theme,
                theme::space::STATUS_ICON,
                Shade::Full,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme::size::BODY)
                    .text_color(rgb(if lit {
                        theme.text.primary
                    } else {
                        theme.text.secondary
                    }))
                    .child(title),
            )
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::META)
                    .text_color(rgb(if lit {
                        theme.text.tertiary
                    } else {
                        theme.text.faint
                    }))
                    .child(right),
            )
            // The pointer moves the one highlight, on a move: hover is
            // recomputed when the arrows scroll the list under a still
            // pointer and would drag the highlight straight back.
            .when(focused, |row| {
                row.on_mouse_move(
                    cx.listener(move |root, _event: &MouseMoveEvent, _window, cx| {
                        root.start_point(Lit::Row(index), cx);
                    }),
                )
            })
            .on_click(cx.listener(move |root, _event: &ClickEvent, window, cx| {
                if let Some(deed) = root.start.take(Lit::Row(index)) {
                    root.start_deed(deed, window, cx);
                }
            }))
            .into_any_element()
    }

    /// The rule and the way to where the defaults are set.
    fn start_foot(&self, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(theme::space::CARD_INNER_GAP)
            .child(div().h(px(1.0)).bg(rgb(theme.surface.divider)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .gap(theme::space::CARD_INNER_GAP)
                    .child(
                        div()
                            .id("start-settings")
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(theme::space::MENU_ICON_GAP)
                            .cursor_pointer()
                            .child(
                                gpui::svg()
                                    .path("ui/title-settings.svg")
                                    .flex_none()
                                    .size(theme::space::START_COG)
                                    .text_color(rgb(theme.text.faint)),
                            )
                            .child(ui.text(Text::StartSettings))
                            .on_click(cx.listener(|root, _event: &ClickEvent, _window, cx| {
                                root.open_settings_at(Section::Sessions, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The pointer on the card or a row: the one highlight goes there, and
    /// the screen repaints only when it moved.
    fn start_point(&mut self, lit: Lit, cx: &mut Context<Root>) {
        if self.start.lit != lit {
            self.start.lit = lit;
            cx.notify();
        }
    }
}

/// A tile's and the card's name: the harness's, or *Terminal* — the
/// strip's menu says the lower-case mode word, which as a title beside
/// *Claude Code* reads as a typo.
fn tab_name(ui: crate::i18n::Ui, tab: DefaultTab) -> &'static str {
    match tab {
        DefaultTab::Terminal => ui.text(Text::StartTerminalName),
        harness => ui.text(default_tab_text(harness)),
    }
}

/// The tab kind a harness's rows and chips are marked with.
fn tab_of(kind: HarnessKind) -> DefaultTab {
    match kind {
        HarnessKind::Claude => DefaultTab::Claude,
        HarnessKind::OpenCode => DefaultTab::OpenCode,
        HarnessKind::Codex => DefaultTab::Codex,
    }
}
