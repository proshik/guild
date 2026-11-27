//! The two buttons at the end of the tab strip, and what hangs off them.
//!
//! A chevron next to the last visible tab when the tabs do not fit —
//! *Show Hidden Tabs* — and a `⋮` beside it for *Recent Files, Tab
//! Actions, and More*. Both are here, and
//! both are drawn **after** the scrolling box rather than inside it: a
//! control that scrolls away with the thing it controls is reachable
//! exactly when it is not needed.
//!
//! **The `⋮`'s items are actions, and that is deliberate.** Every other
//! menu of ours holds deeds aimed at the row under the pointer, because
//! an action reaches the thing in front and a background tab's *Close*
//! must close *that* tab — decision 20 of the spec. This menu hangs off
//! the **strip**, not off a row: *Recent Files*, *Reopen Closed Tab*,
//! *Close All Tabs*, *Close Other Tabs* and *Settings* are aimed at the
//! window and mean the same thing however they are reached. So they are
//! actions, they dispatch, and every one of them shows its stroke
//! honestly — which is the whole reason for this button rather than a
//! longer tab menu. Decision 6 of M8.8.
//!
//! **All three menus are Guild's own since M12.7** — `view::menu::strip`
//! builds them and the right button's engine draws them, hung under the
//! button that opened them. The component's menu lit a row under the
//! pointer by itself, and the harness menu's own pill inside it made two.
//!
//! The chevron's list is the tabs that **cannot be seen**, not every tab:
//! `ScrollHandle` gives the box and each child's place in the same space,
//! so *hidden* is a comparison rather than a guess — task 0 of M8.8.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Interactivity, SharedString, Stateful,
    StatefulInteractiveElement, StyleRefinement, Window, div, px, rgb,
};

use crate::Root;
use crate::TabId;
use crate::i18n::Text;
use crate::view::kind::default_tab_label;
use crate::view::menu::{Anchor, Target};
use crate::{icons, theme};
use guild_session::AreaId;

/// How far a tab may lie outside the box and still count as shown.
///
/// Layout arithmetic is floating point and the strip is measured against
/// its own edges: without a tolerance the last tab that fits exactly
/// could appear among the hidden ones for ever, and the chevron beside it
/// would never go away.
const SLACK: f32 = 0.5;

/// Which of these tabs the strip cannot show.
///
/// `view` is the scrolling box's own left and right edge and each slot is
/// a tab's, in the same space. Task 0 of M8.8 established that the space
/// exists: `ScrollHandle::bounds()` is the box and `bounds_for_item(ix)`
/// is each child, both in window coordinates — so this is a comparison,
/// and the chevron can list what is actually out of sight instead of
/// listing every tab and calling it a menu.
///
/// **A tab clipped by an edge counts as hidden.** Half a name is not a
/// tab a person can pick out, and the chevron exists to reach exactly the
/// ones the hand cannot; a rule that counted only the wholly invisible
/// would leave the tab under the button itself out of the list.
///
/// The same answer says whether the chevron is drawn at all: an empty
/// list is a strip that fits.
pub fn hidden(view: (f32, f32), slots: &[(f32, f32)]) -> Vec<usize> {
    slots
        .iter()
        .enumerate()
        .filter(|(_, (left, right))| *left < view.0 - SLACK || *right > view.1 + SLACK)
        .map(|(index, _)| index)
        .collect()
}

/// A button at the end of the strip, in our tokens.
///
/// `lit` is the background it wears while its menu is up — the same a
/// panel head's ⋮ wears — which since M12.7 `Root` decides, because the
/// menu is `Root`'s now rather than the element's.
#[derive(IntoElement)]
pub(crate) struct StripButton {
    base: Stateful<Div>,
    lit: u32,
    selected: bool,
}

impl StripButton {
    /// An icon in a square, in the shape the status bar's buttons have.
    fn new(id: &'static str, icon: &'static str, theme: crate::theme::Theme) -> Self {
        Self::holding(
            id,
            icons::toolbar(icon)
                .text_color(rgb(theme.text.secondary))
                .into_any_element(),
            theme,
        )
    }

    /// Anything in that square — the add button's small chevron.
    fn holding(id: &'static str, child: gpui::AnyElement, theme: crate::theme::Theme) -> Self {
        Self {
            base: div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .relative()
                .size(theme::space::STATUS_BUTTON)
                .rounded(theme::radius::STATUS_BUTTON)
                .cursor_pointer()
                .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.card)))
                .child(child),
            lit: theme.surface.raised,
            selected: false,
        }
    }

    /// Drawn lit, as a trigger is while its menu is up.
    fn lit(mut self, lit: bool) -> Self {
        self.selected = lit;
        self
    }
}

impl ParentElement for StripButton {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements);
    }
}

impl Styled for StripButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for StripButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

/// The base carries an id, so the button is as stateful as the div it
/// wraps — which is what a tooltip and a hover are asking for.
impl StatefulInteractiveElement for StripButton {}

impl RenderOnce for StripButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let lit = self.lit;
        self.base.when(self.selected, |button| button.bg(rgb(lit)))
    }
}

/// Which of the strip's buttons a menu hangs from — M12.7. Its number is
/// its slot in `Root::strip_at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StripTrigger {
    /// The add button's chevron.
    NewTab = 0,
    /// The chevron over the tabs that cannot be seen.
    HiddenTabs = 1,
    /// The `⋮`.
    TabActions = 2,
}

/// One row of the chevron's list: a tab that cannot be seen.
#[derive(Clone)]
struct Hidden {
    id: TabId,
    name: SharedString,
    /// The file icon's stem, for a tab that has a file behind it.
    icon: Option<SharedString>,
}

/// What the strip knows about each of its tabs by the time the buttons
/// are drawn: which tab, what it is drawn with, and what it is called.
///
/// The strip works this out once and both the tabs and the chevron's
/// list read it — a name computed twice is a name free to differ in the
/// two places it is shown.
pub(crate) type StripRow = (TabId, Option<SharedString>, SharedString);

pub(crate) fn strip_separator(theme: crate::theme::Theme) -> Div {
    div()
        .flex_none()
        .w(px(1.0))
        .h(theme::space::STRIP_RULE)
        .bg(rgb(theme.surface.divider))
}

impl Root {
    /// The hidden-tabs chevron, optional Markdown modes, and rightmost `⋮`.
    ///
    /// Drawn by the strip **outside** its scrolling box; see the module's
    /// own note on why that is not a detail.
    pub(crate) fn render_strip_buttons(
        &self,
        rows: &[StripRow],
        area: Option<AreaId>,
        markdown: Option<gpui::Entity<crate::markdown::Controls>>,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let hidden = self.hidden_tabs(rows, area);

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .pl(theme::space::CARD_INNER_GAP)
            .children((!hidden.is_empty()).then(|| {
                let tabs: Vec<(TabId, String, Option<String>)> = hidden
                    .iter()
                    .map(|tab| {
                        (
                            tab.id,
                            tab.name.to_string(),
                            tab.icon.as_ref().map(|stem| stem.to_string()),
                        )
                    })
                    .collect();
                self.strip_trigger(
                    StripButton::new("tab-list", "ui/chevron-down.svg", theme),
                    StripTrigger::HiddenTabs,
                    area,
                    ui.text(Text::TabsHidden).to_owned(),
                    move |_root| Target::HiddenTabs(tabs.clone()),
                    cx,
                )
            }))
            .when(markdown.is_some() && !hidden.is_empty(), |row| {
                row.child(strip_separator(theme))
            })
            .children(markdown)
            .child(strip_separator(theme))
            .child(self.strip_trigger(
                StripButton::new("tab-actions", "ui/head-options.svg", theme),
                StripTrigger::TabActions,
                area,
                ui.text(Text::TabActions).to_owned(),
                |root| Target::TabActions {
                    areas: root.area_count(),
                },
                cx,
            ))
            .into_any_element()
    }

    /// The tabs the strip cannot show, in strip order.
    ///
    /// Empty on the first frame of a project: the box and its children
    /// are measured during prepaint, so nothing is known until one has
    /// happened. That is the honest answer — a chevron drawn before
    /// anything was measured would be a guess — and the next frame has
    /// it.
    ///
    /// **Measured by the strip's own handle** — M12.8: each area's strip
    /// scrolls on its own, and the chevron pressed is the one whose tabs
    /// it lists.
    fn hidden_tabs(&self, rows: &[StripRow], area: Option<AreaId>) -> Vec<Hidden> {
        let Some(handle) = self.tab_scroll.get(&area) else {
            return Vec::new();
        };
        let viewport = handle.bounds();
        // `child_bounds` are recorded during prepaint, **before** the
        // scroll offset is applied to them — `ScrollHandle`'s own
        // `scroll_to_active_item` adds it the same way. The two are in
        // one space only once it has been.
        let offset: f32 = handle.offset().x.into();
        let slots: Option<Vec<(f32, f32)>> = rows
            .iter()
            .enumerate()
            .map(|(index, _)| {
                handle.bounds_for_item(index).map(|place| {
                    (
                        f32::from(place.left()) + offset,
                        f32::from(place.right()) + offset,
                    )
                })
            })
            .collect();
        let Some(slots) = slots else {
            return Vec::new();
        };
        hidden(
            (f32::from(viewport.left()), f32::from(viewport.right())),
            &slots,
        )
        .into_iter()
        .filter_map(|index| {
            let (id, icon, name) = rows.get(index)?;
            Some(Hidden {
                id: *id,
                name: name.clone(),
                icon: icon.clone(),
            })
        })
        .collect()
    }

    /// The scroll handle of `area`'s strip — made by `keep_strip_scrolls`
    /// before the strips are drawn; a fresh one if an area is drawn that
    /// it did not see, which only costs that strip its offset.
    pub(crate) fn strip_scroll(&self, area: Option<AreaId>) -> gpui::ScrollHandle {
        self.tab_scroll
            .get(&area)
            .cloned()
            .unwrap_or_else(gpui::ScrollHandle::new)
    }

    /// Whether the menu hanging from `trigger` of `area`'s strip is the
    /// one up — every strip draws the same three buttons since M12.8, and
    /// only the one that was pressed is lit.
    fn strip_menu_open(&self, trigger: StripTrigger, area: Option<AreaId>) -> bool {
        let target = self.context_menu.as_ref().map(|open| &open.target);
        self.strip_menu_area == area
            && matches!(
                (target, trigger),
                (Some(Target::NewTab { .. }), StripTrigger::NewTab)
                    | (Some(Target::HiddenTabs(_)), StripTrigger::HiddenTabs)
                    | (Some(Target::TabActions { .. }), StripTrigger::TabActions)
            )
    }

    /// A strip button that opens a menu of the engine's under itself —
    /// M12.7.
    ///
    /// **Lit while its menu is up, and with no tooltip then**: a tooltip
    /// on a button that opens a popup is drawn over the popup, which was
    /// seen again on 2026-09-22. `target` is asked at the click, so what
    /// the menu lists is what is true when it opens.
    ///
    /// The button's place is recorded at prepaint by a `canvas`, and
    /// **only while its menu is not up**, the submenu's rule: the menu
    /// hangs from the place it was opened under. Each button owns its own
    /// slot, so no two writers share one, and a place that moved asks for
    /// a frame.
    ///
    /// **By area since M12.8**: each strip's buttons write their own row
    /// of places, and the press says which strip the menu belongs to.
    fn strip_trigger(
        &self,
        button: StripButton,
        trigger: StripTrigger,
        area: Option<AreaId>,
        tip: String,
        target: impl Fn(&Root) -> Target + 'static,
        cx: &mut Context<Root>,
    ) -> StripButton {
        let theme = self.theme;
        let open = self.strip_menu_open(trigger, area);
        let at = self.strip_at.clone();
        let slot = trigger as usize;
        button
            .lit(open)
            .when(!open, |button| {
                button.hint_delay().tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::text(tip.clone(), theme))
                        .into()
                })
            })
            .on_click(cx.listener({
                let at = at.clone();
                move |root, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    // Drawn at least once before it can be clicked, so the
                    // place is there; the pointer is the answer otherwise.
                    let place = at.borrow().get(&area).and_then(|places| places[slot]);
                    let anchor = match place {
                        Some(bounds) => Anchor::Below(bounds),
                        None => Anchor::Pointer(event.position()),
                    };
                    root.strip_menu_area = area;
                    let target = target(root);
                    root.open_menu(target, anchor, window, cx);
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, window, _cx| {
                        let mut all = at.borrow_mut();
                        let places = all.entry(area).or_default();
                        if !open && places[slot] != Some(bounds) {
                            places[slot] = Some(bounds);
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                // The trap: an absolute box with no inset sits where it
                // would have flowed.
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// The add button, after the last tab — M14, decision 5
    /// of the milestone: a rule, then the mark of the **default tab** (the
    /// setting) with the chevron beside it that offers every tab.
    pub(crate) fn render_add_tab(
        &self,
        any_tab: bool,
        area: Option<AreaId>,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let tip = |label: String| {
            move |_window: &mut Window, app: &mut App| {
                app.new(|_cx| crate::view::tooltip::Tooltip::text(label.clone(), theme))
                    .into()
            }
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            // The rule parts the add button from the tabs, and with no
            // tab it parts it from nothing.
            .when(any_tab, |row| {
                row.child(
                    div()
                        .flex_none()
                        .w(px(1.0))
                        .h(theme::space::STRIP_RULE)
                        .mx(theme::space::CARD_INNER_GAP)
                        .bg(rgb(theme.surface.divider)),
                )
            })
            .when(!any_tab, |row| row.pl(theme::space::CARD_INNER_GAP))
            .child(
                StripButton::holding(
                    "tab-new",
                    crate::view::kind::harness_mark(
                        self.default_tab(),
                        theme,
                        theme::space::STATUS_ICON,
                        crate::view::kind::Shade::Full,
                    ),
                    theme,
                )
                .hint_delay()
                .tooltip(tip(default_tab_label(ui, self.default_tab()).to_owned()))
                .on_click(cx.listener(|root, _event, window, cx| {
                    if let Some(project) = root.active_project {
                        let runs = root.runs_for_default_tab();
                        root.open_harness_tab(project, runs, window, cx);
                    }
                })),
            )
            .child(
                self.strip_trigger(
                    StripButton::holding(
                        "tab-new-kinds",
                        gpui::svg()
                            .path("ui/chevron-down.svg")
                            .size(px(9.0))
                            .text_color(rgb(theme.text.tertiary))
                            .into_any_element(),
                        theme,
                    )
                    .w(px(14.0)),
                    StripTrigger::NewTab,
                    area,
                    ui.text(Text::NewTabKinds).to_owned(),
                    // Shared background availability snapshot, also used by the chord.
                    |root| Target::NewTab {
                        available: root.available.clone(),
                    },
                    cx,
                ),
            )
            .into_any_element()
    }

    /// Bring a tab to the front.
    ///
    /// The strip scrolls it into view by itself: `root::render` follows
    /// the tab in front whenever it changes, which is what makes a tab
    /// picked out of the chevron's list reachable at all.
    pub(crate) fn show_tab(&mut self, id: TabId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.active_tab = Some(id);
        // The one in front is part of the state: a restart shows it.
        self.save();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The add button** — M14: the mark makes the default tab (a
    /// setting), and the chevron beside it offers it and the terminal.
    #[test]
    fn the_chevron_offers_the_default_tab_and_the_terminal() {
        assert_eq!(
            guild_session::DefaultTab::ALL,
            [
                guild_session::DefaultTab::Claude,
                guild_session::DefaultTab::OpenCode,
                guild_session::DefaultTab::Codex,
                guild_session::DefaultTab::Terminal,
            ]
        );
        for tab in guild_session::DefaultTab::ALL {
            let _label = default_tab_label(crate::i18n::Ui(guild_session::Language::English), tab);
        }
    }
}
