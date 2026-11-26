//! The find bar over a terminal's grid — M5.12, task 9.
//!
//! `cmd-f` on the terminal that holds the focus, or *Find…* from its
//! right-button menu, opens a bar at the grid's top right: section 6.30 of
//! the design system. The rules it runs on are `terminal::find`'s; this is
//! the field, its buttons and what they do to the grid.
//!
//! **The bar names no key context** — decision 10: under one of its own,
//! `terminal` would match a level shallower and `cmd-g` would go to
//! `GoToLine`, which does nothing in a terminal.
//!
//! **The bar holds a focus of its own and stops what it takes.** It is a
//! child of the terminal's element, so a key it let through would reach the
//! terminal's `on_key` — where Escape is M5.10's interrupt witness and a
//! byte to `claude` — and a press it did not occlude would begin a
//! selection underneath.

use crate::view::tooltip::Hint as _;
use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};
use gpui::prelude::*;
use gpui::{
    ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    ScrollWheelEvent, SharedString, Window, div, px, rgb,
};

use super::TerminalTab;
use crate::actions::{Copy, Find, FindNext, FindPrevious, Paste, SelectAll};
use crate::i18n::Text;
use crate::terminal::emulator::Listener;
use crate::terminal::find::{self, BarKey};
use crate::terminal::paint;
use crate::terminal::select;
use crate::theme;

/// What the find bar holds while a terminal has one.
///
/// `Default`, so that `TerminalTab::new` — group B's — never has to name a
/// field of it: every field here has a default of its own, the focus
/// handle by being made at the first open rather than here.
#[derive(Default)]
pub(super) struct Finding {
    /// Whether the bar is drawn. Closing keeps the query, so `cmd-f` brings
    /// it back selected, as a browser's bar does.
    open: bool,
    matches: Vec<Match>,
    origin: Option<Point>,
    pub(super) revision: u64,
    task: Option<gpui::Task<()>>,
    cancel: Option<SearchCancel>,
    /// What is typed and how it is read.
    query: find::Query,
    /// The query compiled, or why it will not; `None` while nothing is
    /// typed.
    search: Option<Result<RegexSearch, String>>,
    /// The match *next* was last pressed to, or the first one typing found.
    pub(super) current: Option<Match>,
    /// The bar's own focus, made by the first `open_find`: `FocusHandle`
    /// has no `Default`, and a field that forced one out of this struct
    /// would reach into `TerminalTab::new`.
    focus: Option<FocusHandle>,
    /// The query is selected whole, after `cmd-f` or Select All over it:
    /// the next edit replaces it. M5.11's decision 5.
    selected: bool,
}

impl TerminalTab {
    /// The proxy is focused before the new shared input joins the focus tree.
    pub(crate) fn find_takes_focus(&self, window: &Window) -> bool {
        self.find.open
            && self
                .find
                .focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
    }

    /// Open the find bar and give it the keys; in an open bar, select its
    /// query. The door `Find` and the right button's *Find…* go through.
    ///
    /// **A query left from the last time is searched again from the bottom
    /// of what is shown**: the output has moved on since.
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_before(window, cx, |tab, window, cx| tab.open_find(window, cx)) {
            return;
        }
        self.dismiss_restore(cx);
        let focus = self
            .find
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if !self.find.open {
            self.find.open = true;
            self.restart_find(cx);
        }
        self.find.selected = !self.find.query.text.is_empty();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Close the bar, take every highlight with it, and give the keys back
    /// to the terminal.
    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_search();
        self.find.open = false;
        self.find.current = None;
        self.find.selected = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The find actions on the terminal: `Find`, `FindNext`,
    /// `FindPrevious`.
    ///
    /// **With the bar closed, `cmd-g` is not ours**: the handler hands the
    /// stroke on, and GPUI offers it to the next binding that matched —
    /// `GoToLine`, which has nothing to do in a terminal either, but a
    /// stroke taken and dropped here would be one nobody else can have.
    pub(super) fn bind_find(&self, el: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        el.on_action(cx.listener(|tab, _: &Find, window, cx| tab.open_find(window, cx)))
            .on_action(cx.listener(|tab, _: &FindNext, _window, cx| {
                if tab.find.open {
                    tab.step_find(true, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|tab, _: &FindPrevious, _window, cx| {
                if tab.find.open {
                    tab.step_find(false, cx);
                } else {
                    cx.propagate();
                }
            }))
    }

    /// A key that reached the bar. **Every one of them stops here**: the
    /// terminal behind the bar is not typing into it while the bar has the
    /// focus, and Escape there would be read as stopping an agent.
    fn find_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        match find::bar_key(&event.keystroke) {
            Some(BarKey::Older) => self.step_find(true, cx),
            Some(BarKey::Newer) => self.step_find(false, cx),
            Some(BarKey::Close) => self.close_find(window, cx),
            None => {}
        }
    }

    /// Compile the query again and find its first match, from the bottom of
    /// what is shown. Typing, a toggle and reopening all come here.
    fn restart_find(&mut self, cx: &mut Context<Self>) {
        self.find.current = None;
        self.find.search =
            (!self.find.query.text.is_empty()).then(|| find::compile(&self.find.query));
        self.start_search(cx);
    }

    fn cancel_search(&mut self) {
        if let Some(cancel) = self.find.cancel.take() {
            cancel.0.store(true, std::sync::atomic::Ordering::Release);
        }
        self.find.task = None;
    }
    fn start_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_search();
        self.find.matches.clear();
        self.find.origin = self.emulator.term().lock().ok().map(|term| {
            Point::new(
                alacritty_terminal::index::Line(
                    term.screen_lines() as i32 - 1 - term.grid().display_offset() as i32,
                ),
                term.last_column(),
            )
        });
        if !matches!(self.find.search, Some(Ok(_))) {
            cx.notify();
            return;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.find.cancel = Some(SearchCancel(cancel.clone()));
        let live = self.emulator.term().clone();
        let revision = self.emulator.search_revision();
        let query = self.find.query.clone();
        let executor = cx.background_executor().clone();
        let work = cx.background_executor().spawn(async move {
            executor.timer(std::time::Duration::from_millis(30)).await;
            crate::terminal::find_worker::search(live, revision, cancel, query)
        });
        self.find.task = Some(cx.spawn(async move |tab, cx| {
            let result = work.await;
            let _ = tab.update(cx, |tab, cx| {
                tab.find.task = None;
                if let Some(result) = result
                    && tab
                        .emulator
                        .search_revision()
                        .load(std::sync::atomic::Ordering::Acquire)
                        == result.revision
                {
                    tab.find.matches = result.matches;
                    tab.find.revision = result.revision;
                    tab.step_find(true, cx);
                }
                if tab.find.open
                    && tab.find.revision
                        != tab
                            .emulator
                            .search_revision()
                            .load(std::sync::atomic::Ordering::Acquire)
                {
                    tab.refresh_find(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn refresh_find(&mut self, cx: &mut Context<Self>) {
        if self.find.open
            && self.find.task.is_none()
            && self.find.revision
                != self
                    .emulator
                    .search_revision()
                    .load(std::sync::atomic::Ordering::Acquire)
        {
            self.find.current = None;
            self.start_search(cx);
        }
    }

    /// Navigate the published background match list, holding the live grid only
    /// to update its display offset after checking the snapshot revision.
    fn step_find(&mut self, older: bool, cx: &mut Context<Self>) {
        self.find.selected = false;
        if self.find.revision
            != self
                .emulator
                .search_revision()
                .load(std::sync::atomic::Ordering::Acquire)
            && self.find.task.is_none()
        {
            self.find.current = None;
            self.start_search(cx);
            return;
        }
        let Some(Ok(search)) = self.find.search.as_mut() else {
            cx.notify();
            return;
        };
        let _ = search;
        if self.find.task.is_some() {
            cx.notify();
            return;
        }
        let found = if self.find.matches.is_empty() {
            None
        } else {
            let current = self
                .find
                .current
                .as_ref()
                .and_then(|current| self.find.matches.iter().position(|m| m == current));
            let index = match current {
                Some(i) if older => (i + self.find.matches.len() - 1) % self.find.matches.len(),
                Some(i) => (i + 1) % self.find.matches.len(),
                None => self
                    .find
                    .matches
                    .iter()
                    .rposition(|m| self.find.origin.is_none_or(|origin| *m.end() <= origin))
                    .unwrap_or(self.find.matches.len() - 1),
            };
            Some(self.find.matches[index].clone())
        };
        match find::navigate_snapshot(
            self.emulator.term(),
            &self.emulator.search_revision(),
            self.find.revision,
            found.as_ref(),
        ) {
            Ok(true) => {
                #[cfg(feature = "adaptive-terminal-refresh")]
                self.emulator.grid_changed();
                self.scrolled_at = Some(std::time::Instant::now());
            }
            Ok(false) => {}
            Err(()) => {
                // The helper has released the grid before restarting extraction.
                self.find.current = None;
                self.start_search(cx);
                return;
            }
        }
        self.find.current = found;
        cx.notify();
    }

    /// The bar, while it is open.
    pub(super) fn render_find_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.find.open {
            return None;
        }
        let focus = self.find.focus.clone()?;
        let theme = self.theme;
        let ui = self.ui;
        // The grid's modes, asked outside paint: this is render, and no lock
        // is held here.
        let screen_only = self.emulator.mode().contains(TermMode::ALT_SCREEN);
        let taking_keys = focus.is_focused(window);
        let error = match &self.find.search {
            Some(Err(error)) => Some(error.clone()),
            _ => None,
        };
        let nothing = matches!(self.find.search, Some(Ok(_))) && self.find.current.is_none();
        let empty = self.find.query.text.is_empty();

        // The shared text input is bordered brass **only
        // while its keys go to it** — decision 18 — and rust while its
        // pattern will not compile.
        let query = div()
            .id("find-query")
            .flex_none()
            .w(theme::space::LOG_FIELD)
            .h(theme::space::HEAD_TAB)
            .flex()
            .items_center()
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.inset))
            .overflow_hidden()
            .whitespace_nowrap()
            .when(self.find.selected && !empty, |field| {
                field.bg(rgb(theme.accent.patina_bg))
            })
            .when(taking_keys || error.is_some(), |field| {
                field.border_1().border_color(rgb(if error.is_some() {
                    theme.accent.rust
                } else {
                    theme.accent.brass
                }))
            })
            .text_size(theme::size::META)
            .text_color(rgb(if empty {
                theme.text.faint
            } else {
                theme.text.primary
            }))
            .child(
                crate::text_field::TextField::new(
                    "terminal-find-input",
                    self.find.query.text.clone(),
                    cx.listener(|tab, value: &String, _, cx| {
                        tab.find.query.text = value.clone();
                        tab.find.selected = false;
                        tab.restart_find(cx);
                        cx.notify();
                    }),
                )
                .placeholder(ui.text(Text::FindPlaceholder))
                .focus(&focus)
                .selected(self.find.selected)
                .on_key(cx.listener(|tab, event, window, cx| tab.find_key(event, window, cx))),
            );

        let toggle = |id: &'static str, label: &'static str, on: bool, hint: Text| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(theme::space::HEAD_BUTTON)
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .when(on, |button| button.bg(rgb(theme.accent.brass_bg)))
                .when(!on, |button| {
                    button.hover(|button| {
                        button.bg(crate::theme::control_hover(theme, theme.surface.sunken))
                    })
                })
                .text_size(theme::size::META)
                .text_color(rgb(if on {
                    theme.accent.brass
                } else {
                    theme.text.tertiary
                }))
                .child(label)
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::text(ui.text(hint), theme))
                        .into()
                })
        };
        let button = |id: &'static str, hint: Text| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(theme::space::HEAD_BUTTON)
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.sunken)))
                .text_color(rgb(theme.text.tertiary))
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::text(ui.text(hint), theme))
                        .into()
                })
        };
        let chevron = |path: &'static str| {
            gpui::svg()
                .path(path)
                .size(theme::space::HEAD_ICON)
                .text_color(rgb(theme.text.tertiary))
        };

        let what_was_not_found = match (&error, nothing) {
            (Some(error), _) => Some((SharedString::from(error.clone()), theme.accent.rust)),
            (None, true) => Some((
                SharedString::from(ui.text(Text::SearchNothingFound)),
                theme.text.faint,
            )),
            (None, false) => None,
        };

        let controls = div()
            .flex()
            // Wrap the controls without moving close from the top right.
            .flex_1()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .gap(theme::space::TREE_INSET)
            .child(query)
            .child(
                toggle(
                    "find-match-case",
                    "Aa",
                    self.find.query.match_case,
                    Text::SearchMatchCase,
                )
                .on_click(cx.listener(|tab, _event, _window, cx| {
                    cx.stop_propagation();
                    tab.find.query.match_case = !tab.find.query.match_case;
                    tab.restart_find(cx);
                })),
            )
            .child(
                toggle("find-regex", ".*", self.find.query.regex, Text::SearchRegex).on_click(
                    cx.listener(|tab, _event, _window, cx| {
                        cx.stop_propagation();
                        tab.find.query.regex = !tab.find.query.regex;
                        tab.restart_find(cx);
                    }),
                ),
            )
            .child(
                button("find-older", Text::FindOlder)
                    .child(chevron("ui/chevron-up.svg"))
                    .on_click(cx.listener(|tab, _event, _window, cx| {
                        cx.stop_propagation();
                        tab.step_find(true, cx);
                    })),
            )
            .child(
                button("find-newer", Text::FindNewer)
                    .child(chevron("ui/chevron-down.svg"))
                    .on_click(cx.listener(|tab, _event, _window, cx| {
                        cx.stop_propagation();
                        tab.step_find(false, cx);
                    })),
            );
        let header = div()
            .flex()
            .items_start()
            .gap(theme::space::CARD_GAP)
            .child(controls)
            .child(
                button("find-close", Text::FindClose)
                    .child(chevron("icons/close.svg"))
                    .on_click(cx.listener(|tab, _event, window, cx| {
                        cx.stop_propagation();
                        tab.close_find(window, cx);
                    })),
            );

        // No wider than the terminal it sits on, less the air on both sides.
        let widest = self.bounds.map(|bounds| {
            (bounds.size.width - window.rem_size() * theme::space::CARD_GAP.0 * 2.0).max(px(0.))
        });

        Some(
            div()
                .id("find-bar")
                .absolute()
                .top(theme::space::CARD_GAP)
                .right(theme::space::CARD_GAP)
                .when_some(widest, |card, widest| card.max_w(widest))
                // **The whole card takes the focus when it is pressed**, the
                // Search panel's rule: an element that tracks focus takes it
                // on mouse-down, and the pressed thing — a toggle as much as
                // the field — has to be inside the focusable one, or the
                // terminal behind it would take the keys back.
                .track_focus(&focus)
                // And it hides the terminal from the pointer where it is: a
                // press on a button must not begin a selection underneath.
                .occlude()
                // **And so it carries the gesture it hides**: a selection
                // dragged across the bar has its moves and its release here,
                // not on the terminal, whose handlers stop firing wherever
                // the card is — the trap *…and occluding takes the mouse
                // away from the window underneath*. The wheel too, so the
                // history scrolls under the bar as it does beside it.
                // The rule is `select::drag`, the grid's own since the merge
                // of groups C and D.
                .on_mouse_move(cx.listener(|tab, event: &MouseMoveEvent, _window, cx| {
                    let left_held = event.pressed_button == Some(MouseButton::Left);
                    // A drag the program is told about goes on under the bar
                    // too, and ends there — M5.14 review.
                    if matches!(tab.grab, select::Grab::Report(_)) {
                        tab.on_mouse_move(event, cx);
                        return;
                    }
                    match select::drag(tab.selecting(), left_held) {
                        select::Drag::Idle => {}
                        select::Drag::Stretch => tab.on_mouse_move(event, cx),
                        // The platform says the button is up and no
                        // release arrived: the drag is over all the same.
                        select::Drag::Release => tab.finish_selection(cx),
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|tab, event: &MouseUpEvent, _window, cx| {
                        if tab.grab != select::Grab::None {
                            tab.on_mouse_up(event, cx);
                        }
                    }),
                )
                .on_scroll_wheel(cx.listener(|tab, event: &ScrollWheelEvent, _window, cx| {
                    tab.on_scroll(event, cx)
                }))
                .cursor_default()
                .on_key_down(cx.listener(|tab, event: &KeyDownEvent, window, cx| {
                    if !gpui_component::WindowExt::has_focused_input(window, cx) {
                        tab.find_key(event, window, cx);
                    }
                }))
                // Its own Paste, or `cmd-v` would paste into the pty through
                // the terminal's; its own Copy, or `cmd-c` would copy the
                // terminal's selection behind the bar — both handlers are
                // the card's ancestors (task 7), and an action the card does
                // not take bubbles to them; its own Select All and Find,
                // which select the query so the next letter replaces it.
                .on_action(cx.listener(|tab, _: &Copy, _window, cx| {
                    if let Some(text) = copied(&tab.find) {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }))
                .on_action(cx.listener(|_tab, _: &Paste, window, cx| {
                    if gpui_component::WindowExt::has_focused_input(window, cx) {
                        window.dispatch_action(Box::new(gpui_component::input::Paste), cx);
                    }
                }))
                .on_action(cx.listener(|tab, _: &SelectAll, window, cx| {
                    if let Some(focus) = &tab.find.focus {
                        focus.focus(window, cx);
                    }
                    tab.find.selected = !tab.find.query.text.is_empty();
                    cx.notify();
                }))
                .on_action(cx.listener(|tab, _: &Find, window, cx| {
                    if let Some(focus) = &tab.find.focus {
                        focus.focus(window, cx);
                    }
                    tab.find.selected = !tab.find.query.text.is_empty();
                    cx.notify();
                }))
                .flex()
                .flex_col()
                .gap(theme::space::CARD_GAP)
                .p(theme::space::CARD_PADDING)
                .rounded(theme::radius::CARD)
                .bg(rgb(theme.surface.card))
                .border_1()
                .border_color(theme::card_outline(theme))
                .child(header)
                .child(
                    div()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.tertiary))
                        .child(if self.find.task.is_some() {
                            "…".to_owned()
                        } else {
                            let index = self
                                .find
                                .current
                                .as_ref()
                                .and_then(|m| self.find.matches.iter().position(|v| v == m))
                                .map_or(0, |i| i + 1);
                            format!("{index}/{}", self.find.matches.len())
                        }),
                )
                // **What was not found is a line under the controls, not
                // an item among them.** In the wrapping row it was measured
                // at the width left over beside the buttons, GPUI's text
                // keeps the first width it was measured at for the rest of
                // the frame, and *Nothing matches* stood one letter to a
                // line down a card grown as tall as the words — seen in
                // M5.12's window check 6. Here it has the card's width,
                // and a pattern's error breaks inside the card in a
                // 300-pixel dock, as the line below does.
                .children(what_was_not_found.map(|(words, colour)| {
                    div()
                        .max_w(theme::space::NOTES_CARD)
                        .text_size(theme::size::META)
                        .text_color(rgb(colour))
                        .child(words)
                }))
                .when(screen_only, |card| {
                    card.child(
                        div()
                            .max_w(theme::space::NOTES_CARD)
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.faint))
                            .child(ui.text(Text::FindScreenOnly)),
                    )
                })
                .into_any_element(),
        )
    }
}

/// The matches on the rows shown, read from the grid's guard the paint
/// already holds — never through an accessor that locks again. Nothing
/// while the bar is closed or its pattern will not compile.
pub(super) fn visible_matches(finding: &mut Finding, term: &Term<Listener>) -> Vec<Match> {
    if !finding.open {
        return Vec::new();
    }
    match finding.search.as_mut() {
        Some(Ok(search)) => find::visible(term, search),
        _ => Vec::new(),
    }
}

/// How a cell is marked by the find: not at all, a match, or the current
/// one.
///
/// **Halved rather than walked**: `visible` comes back in order and without
/// overlaps, paint asks for every cell on screen, and `.` is a match per
/// cell. **The current match is marked only while it is among what was
/// found**, so output that moved the grid since *next* was pressed does not
/// leave brass on cells that no longer hold it.
pub(super) fn found(finding: &Finding, visible: &[Match], point: Point) -> paint::Found {
    if !finding.open {
        return paint::Found::No;
    }
    let after = visible.partition_point(|found| *found.start() <= point);
    let Some(found) = after.checked_sub(1).map(|index| &visible[index]) else {
        return paint::Found::No;
    };
    if point > *found.end() {
        return paint::Found::No;
    }
    if finding.current.as_ref() == Some(found) {
        paint::Found::Current
    } else {
        paint::Found::Match
    }
}

/// What Copy takes from the bar: the query while it is selected whole, and
/// nothing otherwise. This fallback handles focus on the bar controls;
/// the focused InputState handles its own partial selection.
fn copied(finding: &Finding) -> Option<String> {
    (finding.selected && !finding.query.text.is_empty()).then(|| finding.query.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line};

    /// **Copy in the bar is the bar's**, as its Paste is: the terminal's
    /// Copy is an ancestor of the card, and an action nobody handles on the
    /// card bubbles to it — so `cmd-c` in the query copied the terminal's
    /// selection behind the bar. With focus on its controls, the fallback
    /// copies only an explicitly selected whole query, and
    /// with nothing selected nothing is copied and the clipboard stays.
    #[test]
    fn copy_in_the_bar_takes_the_query_selected_and_nothing_else() {
        let mut finding = Finding::default();
        assert_eq!(copied(&finding), None, "nothing typed");

        finding.query.text = "panic".into();
        assert_eq!(copied(&finding), None, "typed and not selected");

        finding.selected = true;
        assert_eq!(copied(&finding).as_deref(), Some("panic"));

        finding.query.text.clear();
        assert_eq!(copied(&finding), None, "selected and empty");
    }

    fn at(line: i32, column: usize) -> Point {
        Point::new(Line(line), Column(column))
    }

    /// **A cell is marked by the match it is in, found by halving the
    /// list** — paint asks for every cell on screen, and a pattern like `.`
    /// is a match per cell — and **the current match is marked only where
    /// it still is among what was found**: output arriving moves the grid
    /// under a match found a keystroke ago, and brass painted on the cells
    /// it moved to would be a plausible lie.
    #[test]
    fn a_cell_is_marked_by_the_match_it_is_in_and_the_current_only_where_it_still_is() {
        let visible = vec![
            at(-1, 3)..=at(-1, 5),
            at(0, 8)..=at(1, 1),
            at(2, 0)..=at(2, 0),
        ];
        let mut finding = Finding {
            open: true,
            current: Some(at(0, 8)..=at(1, 1)),
            ..Finding::default()
        };

        assert_eq!(found(&finding, &visible, at(-1, 2)), paint::Found::No);
        assert_eq!(found(&finding, &visible, at(-1, 3)), paint::Found::Match);
        assert_eq!(found(&finding, &visible, at(-1, 5)), paint::Found::Match);
        assert_eq!(found(&finding, &visible, at(-1, 6)), paint::Found::No);
        assert_eq!(found(&finding, &visible, at(0, 9)), paint::Found::Current);
        assert_eq!(
            found(&finding, &visible, at(1, 0)),
            paint::Found::Current,
            "wrapped"
        );
        assert_eq!(found(&finding, &visible, at(1, 2)), paint::Found::No);
        assert_eq!(found(&finding, &visible, at(2, 0)), paint::Found::Match);

        // The current match moved off what was found: nothing is current.
        finding.current = Some(at(0, 7)..=at(1, 0));
        assert_eq!(found(&finding, &visible, at(0, 9)), paint::Found::Match);

        // A closed bar marks nothing, whatever it still remembers.
        finding.open = false;
        assert_eq!(found(&finding, &visible, at(-1, 3)), paint::Found::No);
    }
}

struct SearchCancel(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for SearchCancel {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(test)]
mod asynchronous_tests {
    #[gpui::test]
    fn replacement_query_discards_old_count_and_close_releases_owner(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.executor().allow_parking();
        let mut command = portable_pty::CommandBuilder::new("/bin/sleep");
        command.arg("30");
        let tab = cx.update(|cx| {
            super::super::open_command(
                command,
                std::path::Path::new("/"),
                super::super::Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap()
        });
        tab.update(cx, |tab, cx| {
            let mut parser: alacritty_terminal::vte::ansi::Processor<
                alacritty_terminal::vte::ansi::StdSyncHandler,
            > = Default::default();
            parser.advance(
                &mut *tab.emulator.term().lock().unwrap(),
                b"OLD OLD OLD\r\nneedle\r\nneedle",
            );
            tab.find.open = true;
            tab.find.query.text = "OLD".into();
            tab.restart_find(cx);
            tab.find.query.text = "needle".into();
            tab.restart_find(cx);
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(40));
            cx.run_until_parked();
            if tab.read_with(cx, |tab, _| tab.find.task.is_none()) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        tab.update(cx, |tab, cx| {
            assert_eq!(tab.find.matches.len(), 2);
            assert!(tab.find.current.is_some());
            tab.find.query.text = "OLD".into();
            tab.restart_find(cx);
            tab.cancel_search();
            tab.find.open = false;
        });
        let weak = tab.downgrade();
        drop(tab);
        cx.run_until_parked();
        assert!(
            weak.upgrade().is_none(),
            "cancelled search retained its terminal"
        );
    }
}
