//! The tab's content, or what stands in for it.

use gpui::prelude::*;
use gpui::{Context, Window, div, rgb};

use crate::actions::keymap::Named;
use gpui_component::{Disableable, Sizable, button::Button};

use crate::Body;
use crate::Root;
use crate::i18n::Text;
use crate::theme;
use crate::view::settings::SettingsHost;

impl Root {
    /// The middle of the window: what the tab in front shows, **on the
    /// canvas**.
    ///
    /// M12 task 12 made the frame one surface, `surface.panel`, and left
    /// the canvas to what a tab draws — a terminal, an editor and a diff each
    /// paint `surface.app` themselves. What paints nothing was then the
    /// frame's colour: with no tab open the middle ran into the column and
    /// the docks with nothing between them, and Push and a tab that failed
    /// to open did the same. Seen by the person in the running window. The
    /// canvas is laid here, under every body; a page about Guild still
    /// draws its own ground over it.
    ///
    /// **One area's** since M12.8: the tab in front of `area` — the
    /// window's own for the focused area, the one each other area
    /// remembers — and the start screen in an area with none. Still the
    /// one view that binds a session's view (`a_frame_binds_no_session_view`):
    /// `view::areas` lays the areas out and calls this.
    pub(crate) fn render_body(
        &self,
        area: Option<guild_session::AreaId>,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            // **No ground of its own, since M8.13.** The work's card
            // paints `surface.app` and rounds its corners; GPUI clips a
            // child to the element's *rectangle*, not to its rounded one,
            // so a square ground here leaked past the rounding and the
            // person saw a notch at every corner.
            .child(self.render_body_content(area, window, cx))
            .into_any_element()
    }

    /// The tab in front of `area` — its panes, when it is divided — or,
    /// with no tab, the window's own Settings page or the start screen.
    fn render_body_content(
        &self,
        area: Option<guild_session::AreaId>,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let focused = area == self.focused_area();
        let tab = self
            .area_front(area)
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id));
        match tab {
            // **Divided, the pane view lays the panes out** — M12.8,
            // decision 19 — and calls `render_shown` for each that shows
            // something, so this file stays the one that binds a view.
            Some(tab) if tab.panes.ids().len() > 1 => {
                self.render_panes(tab, area, focused, window, cx)
            }
            Some(tab) => match tab.body() {
                Some(body) => self.render_shown(tab.id, tab.panes.focused(), body, cx),
                // A tab of one pane always shows something (invariant 3);
                // were it not to, its place is a start screen.
                None => {
                    let fits = self.place_fits(area);
                    self.render_start(focused, fits, window, cx)
                }
            },
            // **Settings with no project in front** is the window's own
            // page, drawn where the hints are — M5.11's decision 15.
            None => match self.settings_page {
                Some(section) => self.render_settings(section, SettingsHost::Window, cx),
                None => self.render_nothing_open(area, focused, window, cx),
            },
        }
    }

    /// What `area`'s body can show of the start screen, by its size as
    /// last measured — decision 20; everything before the first frame.
    fn place_fits(&self, area: Option<guild_session::AreaId>) -> crate::start::Fits {
        match self.body_room(area) {
            Some(room) => self.start_fits(room.width, room.height),
            None => crate::start::Fits::ALL,
        }
    }

    /// What one body is drawn as, in the tab `tab` — the whole of a tab
    /// of one pane, or one pane of a divided tab (`view::panes`). **The
    /// one place a session's view is bound** (`a_frame_binds_no_session_view`).
    pub(crate) fn render_shown(
        &self,
        tab: crate::TabId,
        pane: guild_session::PaneId,
        body: &Body,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let content = match body {
            Body::Process { run } => self.render_process(*run, pane, cx),
            // The right button records the terminal and opens its menu —
            // M5.12. The terminal itself hears only the left button.
            Body::Terminal { view, facts, .. } => {
                let at = crate::view::menu::TerminalAt::Tab(tab);
                let failure = facts.failure().map(|failure| {
                    let ui = self.ui();
                    div()
                        .flex()
                        .flex_none()
                        .flex_wrap()
                        .items_center()
                        .gap(theme::space::CARD_GAP)
                        .px(theme::space::MENU_INSET)
                        .py(theme::space::MENU_PADDING)
                        .bg(rgb(theme.surface.panel))
                        .border_b_1()
                        .border_color(rgb(theme.surface.divider))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.secondary))
                                .child(format!(
                                    "{} {}",
                                    ui.harness_failure(failure.harness, failure.code),
                                    ui.text(if failure.network {
                                        Text::HarnessFailureNetwork
                                    } else {
                                        Text::HarnessFailureDetails
                                    })
                                )),
                        )
                        .child(
                            Button::new(("harness-retry", view.entity_id()))
                                .small()
                                .label(ui.text(Text::HarnessRetry))
                                .disabled(facts.retrying())
                                .on_click(cx.listener(move |root, _, window, cx| {
                                    root.retry_harness(tab, pane, window, cx)
                                })),
                        )
                });
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .children(failure)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(
                                    move |root, event: &gpui::MouseDownEvent, window, cx| {
                                        root.note_terminal_menu(at, cx);
                                        root.open_context_menu(event.position, window, cx);
                                    },
                                ),
                            )
                            // **Cached** — reused from the last frame until
                            // the terminal itself is notified, its bounds
                            // move or the window refreshes: a scroll of the
                            // column, a ring's tick or any notify of the root
                            // dirties every view above it and used to repaint
                            // the whole grid with it. GPUI replays the
                            // reused frame's input handler, mouse listeners
                            // and cursor styles; `terminal::view::fill` is
                            // the layout both sides agree on. A render that
                            // moved the keys draws it whole: its last paint
                            // had them elsewhere, and registered no text
                            // client — `Root::focus_moved_in_draw`.
                            .child(if self.focus_moved_in_draw {
                                view.clone().into_any_element()
                            } else {
                                gpui::AnyView::from(view.clone())
                                    .cached(crate::terminal::view::fill(
                                        gpui::StyleRefinement::default(),
                                    ))
                                    .into_any_element()
                            }),
                    )
                    .into_any_element()
            }
            // **The line numbers' right button is Guild's** — M12.9: it
            // offers *Annotate with Git Blame*.
            // Heard on the way down, before the component's own menu
            // opens under the pointer, and stopped there; a press on the
            // text goes on to the component. A dependency's file has no
            // blame in this repository, so it keeps the component's menu.
            Body::File {
                tab: view,
                path,
                origin,
                presentation,
                ..
            } => {
                let file = view.clone();
                let path = path.clone();
                let blamed = *origin == crate::Origin::Project;
                div()
                    .flex_1()
                    .min_h_0()
                    .capture_any_mouse_down(cx.listener(
                        move |root, event: &gpui::MouseDownEvent, window, cx| {
                            if !blamed
                                || event.button != gpui::MouseButton::Right
                                || !file.read(cx).in_gutter(event.position)
                            {
                                return;
                            }
                            cx.stop_propagation();
                            root.note_gutter_menu(path.clone(), file.clone(), cx);
                            root.open_context_menu(event.position, window, cx);
                        },
                    ))
                    .child(presentation.as_ref().map_or_else(
                        || view.clone().into_any_element(),
                        |p| p.view.clone().into_any_element(),
                    ))
                    .into_any_element()
            }
            Body::Diff { tab: view, .. } => div()
                .flex_1()
                .min_h_0()
                .child(view.clone())
                .into_any_element(),
            Body::Annotate { tab: view, .. } => div()
                .flex_1()
                .min_h_0()
                .child(view.clone())
                .into_any_element(),
            Body::Broken { reason, session } => div()
                .flex()
                .flex_col()
                .items_start()
                .gap(theme::space::PANEL_PADDING)
                .flex_1()
                .p(theme::space::PANEL_PADDING)
                .text_color(rgb(theme.accent.rust))
                .child(reason.clone())
                .when(
                    self.desk.session(*session).is_some_and(|record| {
                        matches!(record.runs, guild_session::Runs::Codex(_))
                            && record.harness_session.as_deref().is_none_or(str::is_empty)
                    }),
                    |view| {
                        view.child(
                            Button::new(gpui::SharedString::from(format!(
                                "choose-conversation-{session}"
                            )))
                            .small()
                            .label(self.ui().text(Text::ChooseConversation))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.choose_conversation(tab, pane, window, cx);
                                },
                            )),
                        )
                    },
                )
                .into_any_element(),
            // A page about Guild rather than about the work. It draws in
            // the tab's own space, which is the whole point: it no longer
            // covers the session a person was in the middle of.
            Body::Settings { section } => {
                self.render_settings(*section, SettingsHost::Tab(tab), cx)
            }
        };
        let content = match body.session() {
            Some(session) => self.render_overlay_host(
                guild_session::overlay::Target::Session(session),
                content,
                cx,
            ),
            None => content,
        };
        self.render_overlay_host(guild_session::overlay::Target::Pane(pane), content, cx)
    }
}

impl Root {
    /// What the window says when there is nothing in it.
    ///
    /// **With a project in front, the start screen** — M14.5, section 6.35
    /// of the design system: its harnesses to start and its history to
    /// resume, which say more than any hint could. Without one there is no
    /// folder to read a history for and nothing to start in it, so the
    /// window keeps M5.11's hints — and not the start screen's focus: its
    /// bare keys belong to a screen that is drawn.
    ///
    /// An empty editor's hints: the few things a person would do
    /// next, each with the stroke that does it. An empty middle of a
    /// window is the one place where saying what the keyboard can do costs
    /// nothing — there is nothing else there to compete with.
    ///
    /// **The strokes are read from the keymap, not written into the
    /// sentence.** The line this replaces said "or ⌘T", which stopped
    /// being true for anyone who rebound it, and nothing would ever have
    /// caught that.
    ///
    /// **Every empty area draws the start screen** — M12.8, decision 16 —
    /// and only the focused one's copy takes its keys.
    fn render_nothing_open(
        &self,
        area: Option<guild_session::AreaId>,
        focused: bool,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        if self.active_project.is_some() {
            // An empty area meets a pane's thresholds (decision 20): a
            // full-height one passes them, a divided one may not.
            let fits = self.place_fits(area);
            return self.render_start(focused, fits, window, cx);
        }
        let theme = self.theme;
        let ui = self.ui();

        // **Clipped by the middle** — found after M12.3: a column and a
        // dock left the middle narrower than the list, and the list,
        // centred, hung over the column's edge. The hints are neither
        // wrapped nor shortened when the middle narrows: they stay
        // centred, on one line each, and the area's edges cut them.
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme::space::PANEL_PADDING)
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.muted))
                    .child(ui.text(Text::NoProjects)),
            )
            .child(
                div().flex().flex_none().flex_col().w(HINTS).children(
                    offered_rows().into_iter().enumerate().map(
                        |(index, (name, action, stroke))| {
                            // GLD-080: a row is a deed — the same action its
                            // menu item and its stroke dispatch — lit under
                            // the pointer as the start screen's history rows
                            // are, so a newcomer who reaches for the mouse is
                            // answered.
                            let group: gpui::SharedString = format!("start-hint-{index}").into();
                            let selector = format!("start-hint-{}", ui.text(name));
                            div()
                                .id(("start-hint", index))
                                .debug_selector(move || selector.clone())
                                .group(group.clone())
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(theme::space::CARD_PADDING)
                                .h(theme::space::MENU_ROW)
                                .px(theme::space::CARD_GAP)
                                .rounded(theme::radius::ROW)
                                .cursor_pointer()
                                .hover(|row| row.bg(rgb(theme.surface.raised)))
                                .on_click(cx.listener(move |_, _, window, cx| {
                                    window.dispatch_action(action.action(), cx);
                                }))
                                .child(
                                    div()
                                        .whitespace_nowrap()
                                        .text_size(theme::size::UI)
                                        .text_color(rgb(theme.text.secondary))
                                        .group_hover(group, |text| {
                                            text.text_color(rgb(theme.text.primary))
                                        })
                                        .child(ui.text(name)),
                                )
                                .children(stroke.map(|stroke| {
                                    div()
                                        .flex_none()
                                        .whitespace_nowrap()
                                        .font_family(self.code_font.clone())
                                        .text_size(theme::size::META)
                                        .text_color(rgb(theme.text.faint))
                                        .child(crate::actions::printed(&stroke))
                                }))
                        },
                    ),
                ),
            )
            .into_any_element()
    }
}

/// What to offer with no project in front.
///
/// Five at most. A list long enough to read as documentation is one nobody
/// reads. The project's
/// own five went with M14.5: a worktree with no tab open is the start
/// screen now.
const HINTS_OFFERED: &[(Text, Named)] = &[
    (Text::HintOpenProject, Named::AddProject),
    (Text::HintNewProject, Named::NewProject),
    (Text::HintRecentProjects, Named::RecentProjects),
    (Text::HintSettings, Named::OpenSettings),
];

/// Every row offered, with its stroke when the keymap has one — GLD-080.
///
/// A row is a deed to click, so one without a stroke stays: *Open…* has
/// none since `cmd-o` went to Open In, and it is the row a newcomer with a
/// repository on disk needs first.
fn offered_rows() -> Vec<(Text, Named, Option<String>)> {
    HINTS_OFFERED
        .iter()
        .map(|(text, action)| (*text, *action, crate::actions::stroke_for(*action)))
        .collect()
}

/// How wide the two columns of hints are.
///
/// The longest name at `UI` next to the longest stroke, with room between
/// them: wider and the two columns stop reading as one line, narrower and
/// a translated name runs into its own shortcut.
const HINTS: gpui::Rems = theme::from_px(300.0);

#[cfg(test)]
mod tests {
    use super::*;

    /// GLD-080: the empty screen's first row opens an existing folder —
    /// *File ▸ Open…*'s deed — even though it has no stroke (`cmd-o` is
    /// Open In's), and every row is a deed to click, not only a hint.
    #[test]
    fn the_empty_screen_offers_open_first_with_or_without_a_stroke() {
        assert_eq!(
            offered_rows(),
            vec![
                (Text::HintOpenProject, Named::AddProject, None),
                (
                    Text::HintNewProject,
                    Named::NewProject,
                    Some("cmd-shift-n".into())
                ),
                (
                    Text::HintRecentProjects,
                    Named::RecentProjects,
                    Some("cmd-alt-shift-o".into())
                ),
                (
                    Text::HintSettings,
                    Named::OpenSettings,
                    Some("cmd-,".into())
                ),
            ]
        );
    }
}
