//! The three panels around the tab, and the borders that size them.
//!
//! Empty on purpose. A file tree, a git view and a task board are the
//! milestones after this one, and a panel built around the first thing that
//! goes into it is a panel that fights the second.
//!
//! **Sizes are pixels, not rems.** Everything else in the interface scales
//! with the interface font; a panel does not, because a person set its width
//! with their hand and a width that moved when the font changed would fight
//! the hand that set it.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, CursorStyle, div, px, rgb};

use crate::Root;
use guild_session::{Dock, Tool};

use crate::i18n::Text;
use crate::theme;
use crate::view::drag::DragTool;
use crate::{Body, icons};

/// Which border is being dragged, and which panel it sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The left panel's right-hand border.
    Left,
    /// The right panel's left-hand border.
    Right,
    /// The bottom panel's top border.
    Bottom,
}

/// A border *inside* a tool, and which column it sizes.
///
/// The panels' own borders are `Side`; these are the ones between the
/// columns of a tool that has more than one — the Log's three, since
/// M8.3.2. Same gestures, same three pixels, same double click to reset:
/// a person who has learned one border in this window has learned them
/// all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Between the Log's branches and its commits.
    LogBranches,
    /// Between the Log's commits and the commit that is open.
    LogDetails,
    /// Between the opened commit's files and its message. **Across the
    /// column, not down it**: it sets a height, and the pointer moves up
    /// to give the message more room.
    LogMessage,
}

impl Column {
    /// A stable identifier for the grip's element.
    fn grip_id(self) -> &'static str {
        match self {
            Column::LogBranches => "grip-log-branches",
            Column::LogDetails => "grip-log-details",
            Column::LogMessage => "grip-log-message",
        }
    }

    /// Which way the pointer moves to make that column bigger.
    ///
    /// The branches grow as the pointer goes right, the details as it
    /// comes back left — each column grows away from its own edge.
    pub fn towards(self) -> f32 {
        match self {
            Column::LogBranches => 1.0,
            Column::LogDetails => -1.0,
            Column::LogMessage => -1.0,
        }
    }
}

impl Side {
    /// A stable identifier for the grip's element.
    fn grip_id(self) -> &'static str {
        match self {
            Side::Left => "grip-left",
            Side::Right => "grip-right",
            Side::Bottom => "grip-bottom",
        }
    }

    /// Which way the pointer moves to make the panel bigger.
    ///
    /// The left panel grows as the pointer goes right; the other two grow as
    /// it comes back towards them.
    pub fn towards(self) -> f32 {
        match self {
            Side::Left => 1.0,
            Side::Right | Side::Bottom => -1.0,
        }
    }

    /// The dock this panel is. The same three, named twice: a `Side` is
    /// where a border is, a `Dock` is where a tool lives.
    pub fn dock(self) -> Dock {
        match self {
            Side::Left => Dock::Left,
            Side::Right => Dock::Right,
            Side::Bottom => Dock::Bottom,
        }
    }
}

/// What a tool is called. **One place**, so that a head, a tab in a dock
/// head and a button in the status bar cannot say three things about one
/// tool — and so that a tool added to `Tool::ALL` without a name of its
/// own is caught by a test rather than by a person reading *Nothing here*
/// where a name should be.
pub fn title_of(tool: Tool) -> Text {
    match tool {
        Tool::Explorer => Text::PanelProject,
        Tool::Commit => Text::ToolCommit,
        Tool::Git => Text::ToolGit,
        Tool::Output => Text::PanelOutput,
        Tool::Terminal => Text::PanelTerminal,
        Tool::Search => Text::ToolSearch,
        Tool::Problems => Text::ToolProblems,
        Tool::Push => Text::PushTab,
        Tool::Find => Text::ToolFind,
    }
}

/// The existing toggle action that puts an open dock away.
pub(crate) fn dock_toggle_action(dock: Dock) -> crate::actions::keymap::Named {
    use crate::actions::keymap::Named;
    match dock {
        Dock::Left => Named::ToggleLeftPanel,
        Dock::Right => Named::ToggleRightPanel,
        Dock::Bottom => Named::ToggleBottomPanel,
    }
}

impl Root {
    /// What a tool draws in a panel: its title, the line for when it has
    /// nothing, its body, and what sits at the right of its head. A dock
    /// with no tool shown draws the line that says so.
    fn render_tool(
        &self,
        tool: Option<Tool>,
        dock: Dock,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> (
        Text,
        Text,
        Option<gpui::AnyElement>,
        Option<gpui::AnyElement>,
    ) {
        let title = Self::tool_title(tool);
        match tool {
            Some(Tool::Explorer) => (
                title,
                Text::PanelProjectEmpty,
                self.render_tree(cx),
                Some(self.render_reveal_button(cx)),
            ),
            Some(Tool::Commit) => (
                title,
                Text::PanelChangesEmpty,
                self.render_changes(cx),
                Some(self.render_commit_toolbar(dock, cx)),
            ),
            Some(Tool::Git) => (
                title,
                Text::PanelLogEmpty,
                self.render_git_tool(window, cx),
                // What is being read, when it is not every branch.
                // Nothing says it otherwise, and a log of one branch
                // looks exactly like the log of all of them.
                match self.log_scope() {
                    crate::git::Scope::All => None,
                    crate::git::Scope::Head => Some(self.ui().text(Text::LogHead).to_owned()),
                    crate::git::Scope::Named(name) => Some(name),
                }
                .map(|reference| {
                    div()
                        .flex_none()
                        .px(theme::space::CARD_GAP)
                        .text_size(theme::size::EYEBROW)
                        .text_color(rgb(self.theme.accent.brass))
                        .child(reference)
                        .into_any_element()
                }),
            ),
            Some(Tool::Output) => (title, Text::PanelOutputEmpty, self.render_run(cx), None),
            Some(Tool::Terminal) => (
                title,
                Text::PanelTerminalEmpty,
                self.render_terminal_panel(cx),
                None,
            ),
            Some(Tool::Search) => (title, Text::PanelSearchEmpty, self.render_search(cx), None),
            // **The note says which emptiness it is.** *Nothing is wrong
            // here* is a claim about the project and *nothing has
            // looked* is a claim about us, and on a cold start they are
            // two minutes apart.
            Some(Tool::Problems) => (
                title,
                crate::root::problems::label_of(self.problems_quiet()),
                self.render_problems(cx),
                None,
            ),
            Some(Tool::Find) => (title, Text::PanelFindEmpty, self.render_find(cx), None),
            // Retired by M8.3.6 and taken out of every dock that is read;
            // a dock never shows it.
            Some(Tool::Push) | None => (title, Text::DockEmptyHint, None, None),
        }
    }

    /// What a tool is called, in a head or on a tab — or what a dock with
    /// no tool says instead.
    fn tool_title(tool: Option<Tool>) -> Text {
        tool.map_or(Text::DockEmpty, title_of)
    }

    /// How wide a side dock's panel is drawn — by `render_panel` and by
    /// the cached region's own layout node, which must agree: GPUI lays a
    /// cached view out from the style it is handed.
    pub(crate) fn side_dock_width(&self, dock: Dock) -> gpui::Pixels {
        let panels = self.panels();
        px(match dock {
            Dock::Left => panels.left.size,
            Dock::Right => panels.right.size,
            Dock::Bottom => panels.bottom.size,
        })
    }

    /// The left panel: whatever its dock shows.
    pub(crate) fn render_left_panel(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let (title, empty, body, trailing) =
            self.render_tool(self.tool_shown(Dock::Left), Dock::Left, window, cx);
        self.render_panel(
            "panel-left",
            title,
            empty,
            body,
            trailing,
            self.side_dock_width(Dock::Left),
            Side::Left,
            cx,
        )
    }

    /// The right panel: whatever its dock shows — nothing by default,
    /// until M6 brings the sessions.
    pub(crate) fn render_right_panel(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let (title, empty, body, trailing) =
            self.render_tool(self.tool_shown(Dock::Right), Dock::Right, window, cx);
        self.render_panel(
            "panel-right",
            title,
            empty,
            body,
            trailing,
            self.side_dock_width(Dock::Right),
            Side::Right,
            cx,
        )
    }

    /// One of the two side panels.
    ///
    /// **No border on either edge.** Since M12.8 a side panel is a card of
    /// its own on the window's colour (`render_side_card`), and the step
    /// from `surface.panel` to the card's `surface.app` is what separates
    /// it from the areas beside it — rule 2 of the design system. A line
    /// as well would say the same thing twice.
    #[allow(clippy::too_many_arguments)]
    fn render_panel(
        &self,
        _id: &'static str,
        title: Text,
        empty: Text,
        body: Option<gpui::AnyElement>,
        trailing: Option<gpui::AnyElement>,
        width: gpui::Pixels,
        side: Side,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        // The body when there is one, the placeholder otherwise: a panel
        // with nothing to show says so rather than showing nothing.
        let body = body.unwrap_or_else(|| {
            div()
                .flex_1()
                .p(theme::space::PANEL_PADDING)
                .text_size(theme::size::BODY)
                .text_color(rgb(theme.text.faint))
                .child(ui.text(empty))
                .into_any_element()
        });

        let panel = self
            .dock_drop_target(self.stretchable(side, cx), side.dock(), cx)
            .flex()
            .flex_none()
            .flex_col()
            .w(width)
            .h_full()
            .overflow_hidden()
            // Inside its card, **on the card's own ground** — a ground of
            // its own would be a square inside a rounded card, which is the
            // notch M8.13 took out.
            .child(self.render_panel_head(side.dock(), title, trailing, cx))
            .child(body)
            .into_any_element();
        // Its grip is laid over the edge by `side_grips`.
        panel
    }

    /// How tall the bottom dock's panel is drawn — by `render_bottom_dock`
    /// and by the cached region's own layout node, which must agree: GPUI
    /// lays a cached view out from the style it is handed.
    pub(crate) fn bottom_dock_height(&self) -> gpui::Pixels {
        let available = (f32::from(self.viewport.height) - self.bottom_frame_height()).max(0.0);
        px(self
            .panels()
            .bottom
            .size
            .min((available - guild_session::Panel::MIN_WORK).max(0.0)))
    }

    /// Space the dock may never take: title, measured compact footer, both dock gaps and borders.
    pub(crate) fn bottom_frame_height(&self) -> f32 {
        // Reserve whole pixels so scaled layout rounding cannot consume MIN_WORK.
        (self.rem_px(theme::space::TITLEBAR + theme::space::WORK_GAP + theme::space::WORK_GAP)
            + f32::from(self.footer_height.get()).max(self.rem_px(theme::space::STATUS_BAR))
            + 2.0)
            .ceil()
    }

    /// The bottom panel: output that arrives, is read, and is then in the
    /// way. The panel alone — its card is `render_bottom_card`'s, so that
    /// the part drawn by a cached region has one size, its own.
    pub(crate) fn render_bottom_dock(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let height = self.bottom_dock_height();

        let (title, empty, body, trailing) =
            self.render_tool(self.tool_shown(Dock::Bottom), Dock::Bottom, window, cx);
        let body = body.unwrap_or_else(|| {
            div()
                .flex_1()
                .p(theme::space::PANEL_PADDING)
                .text_size(theme::size::BODY)
                .text_color(rgb(theme.text.faint))
                .child(ui.text(empty))
                .into_any_element()
        });
        self.dock_drop_target(self.stretchable(Side::Bottom, cx), Dock::Bottom, cx)
            .flex()
            .flex_none()
            .flex_col()
            .w_full()
            .h(height)
            .overflow_hidden()
            .child(self.render_panel_head(Dock::Bottom, title, trailing, cx))
            .child(body)
            .into_any_element()
    }

    /// The bottom dock's card around its panel, and the grip over the gap
    /// above it.
    pub(crate) fn render_bottom_card(
        &self,
        panel: impl IntoElement,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;

        // **A card of its own**, in the top card's shape — section 4.1,
        // M12.6. The standard gap separates upper cards from the bottom dock;
        // the resize grip occupies that gap, independently of the footer.
        div()
            .flex()
            .flex_none()
            .flex_col()
            .w_full()
            .child(
                div().relative().w_full().h(theme::space::WORK_GAP).child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        // Fit the hit target between controls without adding layout height.
                        .top(self.bottom_grip_bounds().0)
                        .child(self.render_grip(Side::Bottom, cx)),
                ),
            )
            .child(
                div()
                    .mr(theme::space::WORK_INSET)
                    .when(self.settings.column_folded, |card| {
                        card.ml(theme::space::WORK_INSET)
                    })
                    .mb(theme::space::WORK_GAP)
                    .rounded(theme::radius::WORK)
                    // The border is `card_corners`', drawn over the panel;
                    // this is the room it takes.
                    .p(px(1.0))
                    // The ground is the card's, as the work card's is —
                    // M8.13: a child's square ground paints over a rounded
                    // corner, because a child is clipped to the rectangle.
                    .bg(rgb(theme.surface.app))
                    .overflow_hidden()
                    .relative()
                    .child(panel)
                    .children(card_corners(&theme)),
            )
            .into_any_element()
    }

    /// A side dock's card around its panel — M12.8, decision 18. The side
    /// docks are the window's and not an area's, so they left the top
    /// card when it became one card per area: each is a card of its own
    /// **in the bottom dock's shape**, 6 from the nearest area, and the
    /// areas divide the room between them. The grip over the gap is
    /// `side_grips`', drawn last by the row that holds the cards.
    pub(crate) fn render_side_card(&self, panel: impl IntoElement) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_none()
            .h_full()
            .rounded(theme::radius::WORK)
            // The border is `card_corners`', drawn over the panel; this is
            // the room it takes.
            .p(px(1.0))
            .bg(rgb(theme.surface.app))
            .overflow_hidden()
            .relative()
            .child(panel)
            .children(card_corners(&theme))
            .into_any_element()
    }

    /// A panel's element before anything is in it: its focus handle, and
    /// the four strokes that stretch it — M12.3.
    ///
    /// The handlers are on the panel and not on the window, so they are
    /// on the focus path only while the focus is somewhere inside it: the
    /// tree, the Log, a field, or the panel itself when what was pressed
    /// takes no focus of its own.
    fn stretchable(&self, side: Side, cx: &mut Context<Root>) -> gpui::Div {
        use crate::actions::{StretchToBottom, StretchToLeft, StretchToRight, StretchToTop};
        use guild_session::Stretch;
        div()
            .track_focus(self.dock_focus(side))
            // Escape gives the focus back to the tab from a tool window.
            // Only what the tool did not take for itself
            // reaches here: a field it cancels, a program in the Output's
            // terminal, keep theirs.
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                let stroke = &event.keystroke;
                if stroke.key == "escape" && stroke.modifiers == gpui::Modifiers::none() {
                    cx.stop_propagation();
                    root.refocus(window, cx);
                }
            }))
            .on_action(cx.listener(move |root, _: &StretchToLeft, window, cx| {
                root.stretch_panel(side, Stretch::Left, window, cx)
            }))
            .on_action(cx.listener(move |root, _: &StretchToRight, window, cx| {
                root.stretch_panel(side, Stretch::Right, window, cx)
            }))
            .on_action(cx.listener(move |root, _: &StretchToTop, window, cx| {
                root.stretch_panel(side, Stretch::Top, window, cx)
            }))
            .on_action(cx.listener(move |root, _: &StretchToBottom, window, cx| {
                root.stretch_panel(side, Stretch::Bottom, window, cx)
            }))
    }

    /// How wide a tool's name is in its dock's head: the head's face —
    /// capitals, semibold, at `eyebrow` in the interface's family — added up
    /// from the font's advances and rounded up, so a budget made from it
    /// never leaves the name a pixel short. The head's toolbar is laid out
    /// after it, `dock_menu::head_tools`.
    pub(crate) fn head_title_width(&self, tool: Tool, cx: &gpui::App) -> f32 {
        let family = self
            .settings
            .ui_font
            .clone()
            .unwrap_or_else(|| crate::fonts::DEFAULT.ui.to_owned());
        let font = gpui::Font {
            weight: gpui::FontWeight::SEMIBOLD,
            ..gpui::font(family)
        };
        let size = px(self.rem_px(theme::size::EYEBROW));
        let text = cx.text_system();
        let id = text.resolve_font(&font);
        self.ui()
            .text(title_of(tool))
            .to_uppercase()
            .chars()
            .map(|ch| {
                text.advance(id, size, ch)
                    .map_or(f32::from(size), |advance| f32::from(advance.width))
            })
            .sum::<f32>()
            .ceil()
    }

    /// A panel's heading strip.
    fn render_panel_head(
        &self,
        dock: Dock,
        title: Text,
        trailing: Option<gpui::AnyElement>,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let shown = self.tool_shown(dock);
        let menu_open = self.dock_menu_open(dock);
        let head_at = self.dock_head_at.clone();

        div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .h(theme::space::PANEL_HEAD)
            .pl(theme::space::CARD_PADDING)
            .pr(theme::space::CARD_GAP)
            .text_size(theme::size::EYEBROW)
            .text_color(rgb(theme.text.faint))
            // Where the head itself ended up, for the `⋮`'s menu to hang
            // under — M12.7, §6.17's *four under the head* rather than
            // under the button. Recorded only while the menu is closed,
            // the strip's own triggers' rule: the menu hangs from the
            // place it was opened under.
            .child(
                gpui::canvas(
                    move |bounds, window, _cx| {
                        let mut places = head_at.get();
                        if !menu_open && places[dock as usize] != Some(bounds) {
                            places[dock as usize] = Some(bounds);
                            head_at.set(places);
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
            // **What gives way when the panel is narrow is the naming,
            // never the controls.** Every child of this row is `flex_none`
            // — a button that shrank would be a button nobody can hit — so
            // with nothing able to give, the row simply overflowed and the
            // two controls at its end went over the edge: a panel that
            // could not be put away, which is the *Branch: HEAD* shape
            // again, a state reached and not left. Seen in the window,
            // M12 task 13. The name is what shrinks now.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    // **The tool that is open, and no other.** A dock
                    // holding several used to name them all as tabs, so
                    // the row read *Explorer* beside a greyed *Commit* —
                    // two names over a panel that can only ever show one
                    // thing. What switches between them is the status
                    // bar's buttons, which carry the same names and are
                    // the same gesture wherever a tool lives. Asked for
                    // from the running window, M12 task 14.
                    // **And it is a handle, as its button is** — M12.2's
                    // person: a tool is taken by its name as well as by the
                    // 22-point icon in the status bar. The same drag, the
                    // same preview under the pointer.
                    // **In capitals, semibold, a rank up from faint** —
                    // M12.4: a head in the faint grey of everything else
                    // was one more thing the eye slid over. Upper-cased
                    // at the draw rather than in the label, because the
                    // tooltip, the menu and the status bar still say
                    // *Explorer*.
                    .child(match shown {
                        // Its own width, never more than the naming's: what
                        // comes after it gives way first, and only a head
                        // narrower than the name itself ends it in an
                        // ellipsis rather than mid-word.
                        Some(tool) => div()
                            .id(("dock-title", dock as usize))
                            .flex_none()
                            .max_w_full()
                            .min_w_0()
                            .truncate()
                            .cursor(CursorStyle::OpenHand)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text.tertiary))
                            .child(ui.text(title).to_uppercase())
                            .on_drag(DragTool(tool), move |drag, _point, _window, cx| {
                                cx.new(|_| crate::view::drag::DragPreview {
                                    tool: drag.0,
                                    theme,
                                })
                            })
                            .into_any_element(),
                        None => div()
                            .flex_none()
                            .max_w_full()
                            .min_w_0()
                            .truncate()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text.tertiary))
                            .child(ui.text(title).to_uppercase())
                            .into_any_element(),
                    })
                    // A tool with faces puts them **here**, after its
                    // name, not in a row of their own: two strips one
                    // above the other cost a panel eighty-four pixels of
                    // head before a line of content, where forty-two is
                    // enough. `Git  Log Console` on one row.
                    .when(shown == Some(Tool::Terminal), |naming| {
                        naming.children(self.render_terminal_panel_tabs(cx))
                    })
                    .when(shown == Some(Tool::Git), |naming| {
                        naming.child(self.render_git_tabs(cx))
                    })
                    // The Find tool's searches, a tab each with its cross —
                    // *feign.Contract in Project Files ×*. M8.15.
                    .when(shown == Some(Tool::Find), |naming| {
                        naming.children(self.render_find_tabs(cx))
                    }),
            )
            // A tool's own controls give way next, and only then: they
            // are the tool's, while the two after them belong to the
            // panel itself and must survive any width.
            .children(trailing.map(|trailing| {
                div()
                    .flex()
                    .flex_shrink(1.)
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .child(trailing)
                    .into_any_element()
            }))
            // The pair, in the head's corner: the ⋮ that opens the tool
            // window's own menu, and the bar that puts the panel away.
            // They come **after** whatever the tool put there — the
            // Explorer's crosshair keeps its place and gains these.
            //
            // **The Commit panel takes the bar and not the ⋮**, which is
            // the person's own decision: its head is a toolbar already.
            .when(shown != Some(Tool::Commit), |head| {
                head.child(self.render_dock_options(dock, &[], cx))
            })
            // **…until the panel is too narrow for its toolbar**, M12.1:
            // then the buttons that do not fit go into a ⋮ that appears for
            // them, with the dock's own items after — a toolbar that went
            // over the edge instead was a Commit panel that could no longer
            // refresh.
            .when(shown == Some(Tool::Commit), |head| {
                let folded = self.commit_tools_folded(dock, cx);
                head.when(!folded.is_empty(), |head| {
                    head.child(self.render_dock_options(dock, &folded, cx))
                })
            })
            .child(self.render_dock_hide(dock, cx))
            .into_any_element()
    }

    /// The bar at a head's right, which puts the panel away — the same
    /// thing the tool's own button does when it is pressed again.
    ///
    /// A bar rather than a cross: a cross says *close
    /// this*, and a dock is not closed but hidden — its tools are still
    /// there and its button in the status bar brings them back.
    fn render_dock_hide(&self, dock: Dock, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let label = ui.text(Text::PanelHide).to_owned();

        div()
            .id(("dock-hide", dock as usize))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .cursor_pointer()
            .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .child(icons::toolbar("ui/head-hide.svg").text_color(rgb(theme.text.secondary)))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| {
                    crate::view::tooltip::Tooltip::for_action(
                        label.clone(),
                        dock_toggle_action(dock),
                        theme,
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |root, _event, _window, cx| {
                cx.stop_propagation();
                root.hide_dock(dock, cx);
            }))
            .into_any_element()
    }

    /// *Reveal Active File*, at the right of the Explorer's heading — a
    /// crosshair. Dimmed when the tab in front is not a file: a control
    /// that says only its name leaves a person to guess why it does
    /// nothing, so the tooltip says so too.
    fn render_reveal_button(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let has_file = matches!(self.active_body(), Some(Body::File { .. }));
        let label = if has_file {
            ui.text(Text::RevealActiveFile).to_owned()
        } else {
            format!(
                "{} — {}",
                ui.text(Text::RevealActiveFile),
                ui.text(Text::NoFileInFront)
            )
        };
        let action = crate::actions::keymap::Named::RevealFile;

        div()
            .id("reveal-file")
            .debug_selector(|| "reveal-file".into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .child(icons::crosshair().text_color(rgb(if has_file {
                theme.text.secondary
            } else {
                theme.text.faint
            })))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| {
                    crate::view::tooltip::Tooltip::for_action(label.clone(), action, theme)
                })
                .into()
            })
            .on_click(cx.listener(|root, _event, window, cx| {
                cx.stop_propagation();
                root.reveal_active_file(window, cx);
            }))
            .into_any_element()
    }

    /// The side docks' grips, laid over the gaps between their cards and
    /// the areas — for the row that holds them, which draws them **last
    /// among its own children**.
    ///
    /// The grip takes no room: it is centred on the gap, reaching a point
    /// and a half into each card as the grip between the top cards and the
    /// bottom dock's does — since M12.8, when the side docks became cards
    /// of their own; before, it hung over the panel's edge inside the one
    /// card. It is drawn after the cards so that it can occlude them —
    /// laid under the editor it would hand a press to both, and the editor
    /// would begin a selection. **And not `deferred`**: a deferred grip was
    /// drawn after every popup too, and its line cut through Find in Files
    /// — the person's screenshot, 2026-09-18. As the row's last child it is
    /// above the work and below everything deferred.
    ///
    /// Nothing while a tool is being dragged: the drop zones take room
    /// then and move the edges, and nobody resizes a panel mid-drag.
    pub(crate) fn side_grips(&self, cx: &mut Context<Root>) -> Vec<gpui::AnyElement> {
        if self.tool_drag.is_some() {
            return Vec::new();
        }
        let panels = self.panels();
        // The card is the panel and a point of border either side; the
        // gap follows it, and the grip's middle is the gap's.
        let gap = self.rem_px(theme::space::WORK_BETWEEN);
        let beside = |size: f32| size + 2.0 + gap / 2.0 - GRIP_REACH / 2.0;
        let mut grips = Vec::new();
        if panels.left.open {
            let at = beside(panels.left.size);
            grips.push(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(at))
                    .child(self.render_grip(Side::Left, cx))
                    .into_any_element(),
            );
        }
        if panels.right.open {
            let at = beside(panels.right.size);
            grips.push(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(at))
                    .child(self.render_grip(Side::Right, cx))
                    .into_any_element(),
            );
        }
        grips
    }

    /// The border between two regions, which is also the handle.
    ///
    /// **Only the cursor under the pointer.** M12.4 drew a hairline here
    /// and lit it patina under the hand; the patina went on 2026-09-18 by
    /// the person's word, and the line went with M12.8, when the side
    /// docks left the top card and every dock's border became a gap
    /// between two cards. A grip hangs over its gap and takes no room —
    /// `side_grips` says why.
    ///
    /// **What one sees and what one may hit are not the same width.** The
    /// side gap is six; the hand is given up to [`GRIP_REACH`], because a narrow
    /// target is one a pointer crosses between two of its own samples —
    /// and a point is smaller on a big screen than on this laptop's, which
    /// is where it was noticed. The bottom grip shrinks to fit between
    /// the upper card and header buttons at small UI scales.
    fn bottom_grip_bounds(&self) -> (gpui::Pixels, gpui::Pixels) {
        let gap = self.rem_px(theme::space::WORK_GAP);
        let reach = GRIP_REACH
            .min(gap + self.rem_px(theme::space::PANEL_HEAD - theme::space::HEAD_BUTTON) / 2.0);
        (px(0.0), px(reach))
    }

    fn render_grip(&self, side: Side, cx: &mut Context<Root>) -> gpui::AnyElement {
        let vertical = side != Side::Bottom;

        // **Every dock's grip lies on the window's ground between two
        // cards** since M12.8 — the side docks' as the bottom one's did
        // since M12.6 — where the step of colour already separates them,
        // so it draws nothing: section 4.1. **Nothing lights under the
        // hand** either — the person, 2026-09-18: the cursor saying it can
        // be dragged is enough. The same on every handle that sizes
        // something.
        let grip = div()
            .id(side.grip_id())
            .debug_selector(move || side.grip_id().into())
            .flex_none()
            // A side grip is over the cards on both sides of its gap, and
            // takes the press from both. The reach is shared with them.
            .when(vertical, |grip| grip.occlude())
            .when(vertical, |grip| grip.w(px(GRIP_REACH)).h_full())
            .when(!vertical, |grip| {
                grip.h(self.bottom_grip_bounds().1).w_full()
            })
            .cursor(if vertical {
                CursorStyle::ResizeLeftRight
            } else {
                CursorStyle::ResizeUpDown
            });

        grip.on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |root, event: &gpui::MouseDownEvent, _window, cx| {
                // A second click on a border asks for its width back, and
                // must not also begin a drag: the hand that double-clicks
                // moves a little, and the reset would be undone at once.
                if event.click_count >= 2 {
                    root.reset_panel(side, cx);
                    return;
                }
                root.dragging = Some(side);
                root.drag_from = Some(event.position);
                root.last_panel = side;
                cx.notify();
            }),
        )
        .into_any_element()
    }
}

impl Root {
    /// Make an element a place a dragged tool may be dropped.
    ///
    /// The zones at the window's edges are 52 pixels wide, which is a
    /// small target for a hand and says nothing about where the tool
    /// will end up. **An open panel is the other half of the answer:**
    /// dropping a tool onto the Explorer puts it in the left dock,
    /// because that is where the pointer is pointing. The zones stay for
    /// the docks that are closed, which have no panel to drop on.
    fn dock_drop_target(
        &self,
        panel: gpui::Div,
        dock: Dock,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        panel
            .id(("dock-drop", dock as usize))
            .drag_over::<DragTool>(move |style, _, _, _| style.bg(rgb(theme.accent.brass_bg)))
            .on_drop(cx.listener(move |root, drag: &DragTool, _window, cx| {
                root.tool_drag = None;
                root.move_tool(drag.0, dock, cx);
            }))
    }

    /// The border between two columns of a tool.
    ///
    /// The same three pixels as `render_grip`, and the same two gestures:
    /// drag to size, double click to put it back where the design system
    /// had it. Drawn by the tool that has the columns.
    pub(crate) fn render_column_grip(
        &self,
        column: Column,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        // The message's border lies across the column rather than down
        // it: it sets a height, and a hand reaching for it comes from
        // above or below.
        let across = column == Column::LogMessage;

        // Nothing is drawn: the columns' own dividers are the border, and
        // the cursor is what says it can be dragged — 2026-09-18, when the
        // patina that lit under the hand was taken off every handle.
        div()
            .id(column.grip_id())
            .debug_selector(move || column.grip_id().into())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when(!across, |grip| grip.w(px(GRIP_REACH)).h_full())
            .when(across, |grip| grip.h(px(GRIP_REACH)).w_full())
            .cursor(if across {
                CursorStyle::ResizeUpDown
            } else {
                CursorStyle::ResizeLeftRight
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |root, event: &gpui::MouseDownEvent, _window, cx| {
                    // As on a panel's border: the second click asks for
                    // the width back and must not also start a drag.
                    if event.click_count >= 2 {
                        root.reset_column(column, cx);
                        return;
                    }
                    root.grab_column(column, event.position, cx);
                }),
            )
            .into_any_element()
    }
}

/// How wide the hand's target is, against the line's one.
///
/// A line is what one sees and not what one can aim at: a pointer moving
/// at any speed steps over a point between two samples. Nine is the
/// common order of size for such a handle. **It is laid over the
/// border and shared with the regions beside it**, not added to either —
/// since 2026-09-18 a side panel's grip hangs over its edge instead of
/// standing beside it, and nothing lights under the hand: the cursor is
/// the only sign.
pub(crate) const GRIP_REACH: f32 = 9.0;

/// **A card's own corners, drawn over whatever it holds.**
///
/// GPUI clips a card's children to its *rectangle*, not to its rounded
/// shape, so any child that paints a ground reaching the card's edge
/// paints the corner square — the file tab's `surface.app`, a terminal's
/// cells, a diff's rows. M8.13 took the ground off two children and the
/// person found the corner square again under a third. A rule every child
/// has to remember is a rule the next child forgets, so the card closes
/// its corners itself, last, over everything:
///
/// - **a band of the window's colour outside the arc** — a box one radius
///   larger than the card on every side, with a border one radius thick
///   and corners of twice the radius, so that the border's *inner* edge is
///   exactly the card's rounded outline. The card's `overflow_hidden`
///   clips the band to the card's rectangle, and what is left is the four
///   slivers between the arc and the rectangle's corners;
/// - **the card's border**, which is drawn here and only here. A border
///   the card drew itself would be under its children, and an absolute
///   child is placed inside that border — so the two outlines stood a
///   point apart, measured in the window. The card keeps a point of
///   padding instead, which is the room the border takes.
///
/// Neither takes the mouse: a div with no handlers registers no hitbox.
pub(crate) fn card_corners(theme: &theme::Theme) -> [gpui::AnyElement; 2] {
    card_corners_edged(theme, rgb(theme.surface.divider).into())
}

/// `card_corners` with the edge in a colour of the caller's: the focused
/// area of a divided seat is framed in the patina the focused pane's
/// outline is drawn in (M12.8, §6.36).
pub(crate) fn card_corners_edged(theme: &theme::Theme, edge: gpui::Hsla) -> [gpui::AnyElement; 2] {
    let radius = theme::radius::WORK;
    let mut band = div()
        .absolute()
        .top(-radius)
        .left(-radius)
        .right(-radius)
        .bottom(-radius)
        .rounded(radius * 2.0)
        .border_color(rgb(theme.surface.panel));
    {
        let edges = &mut band.style().border_widths;
        let width: gpui::AbsoluteLength = radius.into();
        edges.top = Some(width);
        edges.right = Some(width);
        edges.bottom = Some(width);
        edges.left = Some(width);
    }
    let outline = div()
        .absolute()
        .inset_0()
        .rounded(radius)
        .border_1()
        .border_color(edge);
    [band.into_any_element(), outline.into_any_element()]
}
