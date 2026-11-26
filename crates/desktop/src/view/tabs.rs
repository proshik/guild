//! The tab strip of the active project.

use crate::view::tooltip::Hint as _;
#[cfg(test)]
mod close_tests;
pub(crate) mod drag;
pub(crate) mod landing;

use gpui::prelude::*;
use gpui::{Context, Render, SharedString, div, px, rgb};

use crate::Body;
use crate::Kind;
use crate::Root;
use crate::TabId;
use crate::file_tab::Mark;
use crate::i18n::Text;
use crate::theme;
use crate::view::kind::Shade;
use crate::view::kind::kind_colour;
use crate::view::kind::kind_glyph;
use gpui_component::{Sizable, button::ButtonVariants};
use guild_session::status::{Status, Why};
use guild_session::{DefaultTab, Runs};

const PROCESS_ICON: &str = "ui/workspace-git-branch.svg";

/// What a tab is called in its strip: its own name, or for a session
/// without one its kind and its place in the strip.
fn strip_label(ui: crate::i18n::Ui, kind: Kind, named: Option<&String>, index: usize) -> String {
    match kind {
        Kind::Settings => ui.text(Text::Settings).to_owned(),
        Kind::File | Kind::Diff | Kind::Annotate | Kind::Process => {
            named.cloned().unwrap_or_default()
        }
        Kind::Session(runs) => named
            .cloned()
            .unwrap_or_else(|| ui.tab_label(runs.is_claude(), index + 1)),
    }
}

impl Root {
    /// What the strip draws for `body`, one of `tab`'s panes: its kind,
    /// its agent's status, its name when it has one of its own, its file
    /// icon and its unsaved mark. `None` for a session with no record.
    #[allow(clippy::type_complexity)]
    fn strip_face(
        &self,
        tab: &crate::Tab,
        face: &Body,
        cx: &Context<Root>,
    ) -> Option<(Kind, Status, Option<String>, Option<String>, Option<Mark>)> {
        let theme = self.theme;
        let ui = self.ui();
        if let Body::File {
            path, tab: view, ..
        } = face
        {
            // A file: its name, and its icon where the mark
            // goes — section 6.5 as M8.1 amended it.
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let stem = crate::icons::for_file(&name, crate::icons::Palette::from(theme.kind));
            return Some((
                Kind::File,
                Status::Idle,
                Some(name),
                Some(stem),
                view.read(cx).mark(),
            ));
        }
        if let Body::Annotate { path, .. } = face {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let name = format!("{name} \u{b7} {}", ui.text(Text::AnnotateTab));
            return Some((Kind::Annotate, Status::Idle, Some(name), None, None));
        }
        if let Body::Diff {
            path,
            base,
            tab: view,
        } = face
        {
            // A diff: the file's name, or the working tree's
            // count — the mockup's *Diff · 3 files*.
            let name = match path {
                Some(path) => path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                None => ui.diff_title(view.read(cx).files().len()),
            };
            let name = ui.diff_tab_title(&name, base);
            return Some((
                Kind::Diff,
                // A proposal is the one diff with somebody
                // waiting on it: the agent that opened it is
                // blocked until this tab is answered. Its shared
                // question mark says so, as on a waiting agent tab.
                self.pane_status(tab, face),
                Some(name),
                None,
                None,
            ));
        }
        if matches!(face, Body::Process { .. }) {
            return Some((
                Kind::Process,
                Status::Idle,
                Some(ui.text(Text::Process).into()),
                None,
                None,
            ));
        }

        let Some(session) = face.session() else {
            // A page about Guild. It has no session, no spending
            // and nothing to wait for — the name is all it needs.
            return Some((Kind::Settings, Status::Idle, None, None, None));
        };
        let record = self.desk.session(session)?;
        // What a person named it, or what the conversation is
        // about, or what its program wrote — `session_tab_name`.
        let named = self.session_tab_name(tab);
        // **A shell with `claude` typed into it holds Claude Code**,
        // and is drawn as one — M12.3. What the terminal runs in
        // front says so; the record still says shell, because the
        // tab is one and goes back to being one when `claude` ends.
        let runs = match (face, record.runs) {
            (Body::Terminal { facts, .. }, guild_session::Runs::Shell)
                if facts.claude_in_front() =>
            {
                guild_session::Runs::Claude(guild_session::SessionMode::Terminal)
            }
            (_, runs) => runs,
        };
        Some((
            Kind::Session(runs),
            self.status_of(session),
            named,
            None,
            None,
        ))
    }

    /// A tab as its strip draws it, for the column's list of a worktree's
    /// tabs: its kind, the most urgent pane's status, and its name — the
    /// strip's own, numbered by its place in its own area's strip, so the
    /// two never call one tab two things. `None` for a tab the strip would
    /// not draw.
    pub(crate) fn column_tab_face(
        &self,
        tab: &crate::Tab,
        cx: &Context<Root>,
    ) -> Option<(Kind, Status, String)> {
        let face = tab.face()?;
        let (kind, status, named, _, _) = self.strip_face(tab, face, cx)?;
        let status = match self.loudest_body(tab) {
            Some(loud) if !std::ptr::eq(loud, face) => self
                .strip_face(tab, loud, cx)
                .map_or(status, |(_, status, ..)| status),
            _ => status,
        };
        let index = self
            .tabs
            .iter()
            .filter(|other| {
                other.seat == tab.seat && other.area == tab.area && other.face().is_some()
            })
            .position(|other| other.id == tab.id)?;
        Some((
            kind,
            status,
            strip_label(self.ui(), kind, named.as_ref(), index),
        ))
    }

    /// The pane of `tab` whose mark the strip draws — `stage::loudest`
    /// over its panes, the focused one listed first. `None` for a tab of
    /// one pane, whose face is its mark.
    fn loudest_body<'a>(&self, tab: &'a crate::Tab) -> Option<&'a Body> {
        if tab.panes.ids().len() < 2 {
            return None;
        }
        let focused = tab.panes.focused();
        let bodies = tab.panes.bodies();
        let listed = bodies
            .iter()
            .filter(|(pane, _)| *pane == focused)
            .chain(bodies.iter().filter(|(pane, _)| *pane != focused));
        crate::stage::loudest(listed.map(|(_, body)| (*body, self.pane_status(tab, body))))
    }

    /// What one pane of `tab` asks of the person, as the strip draws it:
    /// its agent's status; **a proposal's diff the agent is blocked on**,
    /// which wears the shared waiting question mark and so lends it to
    /// a divided tab as an agent would (M12.8's second window check); and
    /// nothing for anything else.
    fn pane_status(&self, tab: &crate::Tab, body: &Body) -> Status {
        match body {
            Body::Diff {
                base: guild_session::Base::Proposal,
                ..
            } if self.is_proposal(tab.id) => Status::Waiting(Why::Permission),
            _ => body
                .session()
                .map_or(Status::Idle, |session| self.status_of(session)),
        }
    }

    /// One area's tab strip — M12.8: every area draws its own, and only
    /// the focused one's front tab is underlined in patina (decision 18).
    /// `None` is the one card drawn before the seat has areas.
    pub(crate) fn render_tabs(
        &self,
        area: Option<guild_session::AreaId>,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let focused = area == self.focused_area();
        // A tab is no longer always a session, so the strip asks each one
        // what it is rather than looking every tab up in the workspace.
        let tabs: Vec<_> = self
            .area_tabs(area)
            .filter_map(|tab| {
                // The strip names a tab by its face — its focused pane's
                // body, else its first that shows something (M12.8).
                let face = tab.face()?;
                let (kind, status, named, icon, mark) = self.strip_face(tab, face, cx)?;
                // **The mark is the most urgent pane's** — M12.8's second
                // window check, O7: an agent working or waiting beside a
                // focused shell keeps its status in the strip,
                // under the shell's name.
                let (marked, status, icon) = match self.loudest_body(tab) {
                    Some(loud) if !std::ptr::eq(loud, face) => {
                        match self.strip_face(tab, loud, cx) {
                            Some((kind, status, _, icon, _)) => (kind, status, icon),
                            None => (kind, status, icon),
                        }
                    }
                    _ => (kind, status, icon),
                };
                Some((tab.id, kind, marked, status, named, icon, mark))
            })
            .collect();
        // The tab in front of this area: the window's for the focused one,
        // the one the area remembers for every other.
        let active = self.area_front(area);
        let renaming = self
            .renaming()
            .map(|(id, typed)| (id, SharedString::from(typed.to_owned())));
        // What each tab is drawn with and called, worked out once: the
        // strip draws it and the chevron's list of what cannot be seen
        // reads the same row. A name computed in both places is a name
        // free to differ between them.
        let rows: Vec<crate::view::tab_list::StripRow> = tabs
            .iter()
            .enumerate()
            .map(|(index, (id, kind, _, _, named, icon, _))| {
                let label = strip_label(ui, *kind, named.as_ref(), index);
                (
                    *id,
                    icon.clone().map(SharedString::from),
                    SharedString::from(label),
                )
            })
            .collect();

        strip_row(theme)
            // **A click anywhere else keeps the name being typed** — M12.1.
            // A layer over the whole window, drawn the way the dock's menu
            // draws its own: `deferred` so it is over everything, `occlude`
            // so the press is not also a press on whatever is underneath.
            // Only in the strip that holds the tab being named: one layer
            // is the whole window's, and a second would be drawn over it.
            .children(
                renaming
                    .as_ref()
                    .is_some_and(|(renamed, _)| tabs.iter().any(|tab| tab.0 == *renamed))
                    .then(|| {
                        gpui::deferred(
                            gpui::anchored().position(gpui::Point::default()).child(
                                div()
                                    .w(self.viewport().width)
                                    .h(self.viewport().height)
                                    .occlude()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|root, _event, window, cx| {
                                            cx.stop_propagation();
                                            root.finish_renaming_tab(true, window, cx);
                                        }),
                                    ),
                            ),
                        )
                    }),
            )
            .child(
                scrolling_tabs(crate::view::areas::element_id("tabs", area))
                    .track_scroll(&self.strip_scroll(area))
                    .children(tabs.into_iter().enumerate().map(
                        |(index, (id, named_as, kind, status, _named, icon, mark))| {
                            let selected = active == Some(id);
                            let label = rows[index].2.clone();
                            let drag_name = label.clone();
                            let is_session = matches!(named_as, Kind::Session(_));
                            let hovered = self.hovered_tab == Some(id);
                            // A tab of more than one pane says so after
                            // its name — M12.8, decision 19.
                            let divided = self
                                .tabs
                                .iter()
                                .any(|tab| tab.id == id && tab.panes.ids().len() > 1);
                            // Mixed-pane tabs retain the ordinary label and split mark.
                            let compact = named_as == Kind::Process && !divided;
                            // A process says what it runs; a file from
                            // outside the project says where it is, which
                            // its name alone does not.
                            let tip = self.tabs.iter().find(|t| t.id == id).and_then(|t| {
                                match t.face() {
                                    Some(Body::Process { run }) => Some(self.process_tab_tip(*run)),
                                    Some(Body::File {
                                        path,
                                        origin: crate::Origin::Outside,
                                        ..
                                    }) => Some(crate::short_path(
                                        &std::path::Path::new(crate::Origin::OUTSIDE_ROOT)
                                            .join(path),
                                    )),
                                    _ => None,
                                }
                            });
                            let process_runs: Vec<_> = self
                                .tabs
                                .iter()
                                .find(|tab| tab.id == id)
                                .and_then(|tab| tab.session())
                                .into_iter()
                                .flat_map(|session| {
                                    self.process_for_session(session)
                                        .into_iter()
                                        .map(move |run| (session, run))
                                })
                                .collect();
                            // A trailing icon button already includes its own inset.
                            // Activity, when present, still keeps ordinary tab padding.
                            let reopen_at_end = status == Status::Idle
                                && process_runs.iter().any(|(session, run)| {
                                    self.process_navigation_visible(*session, *run)
                                });
                            // **Underlined, not filled** — M12.6 decision 4: no
                            // ground on any tab, the one in front in
                            // `text.primary` with a bar under it, the rest in
                            // `text.tertiary`.
                            div()
                                .id(SharedString::from(format!("tab-{id}")))
                                .debug_selector(|| "activity-tab".into())
                                .on_hover(cx.listener(move |root, over: &bool, _window, cx| {
                                    let was = root.hovered_tab;
                                    if *over {
                                        root.hovered_tab = Some(id);
                                    } else if root.hovered_tab == Some(id) {
                                        root.hovered_tab = None;
                                    }
                                    if root.hovered_tab != was {
                                        cx.notify();
                                    }
                                }))
                                .relative()
                                .flex()
                                .flex_none()
                                .h_full()
                                .max_w(theme::space::TAB_WIDTH)
                                .items_center()
                                .gap(theme::space::TAB_GAP)
                                .px(theme::space::TAB_PADDING)
                                .when(reopen_at_end, |tab| tab.pr_0())
                                .when(compact, |tab| {
                                    tab.w(theme::space::TABS + theme::space::CARD_GAP)
                                        .max_w(theme::space::TABS + theme::space::CARD_GAP)
                                        .px(theme::space::CARD_GAP / 2.0)
                                        .gap_0()
                                })
                                .when_some(tip, |tab, tip| {
                                    tab.aria_label(tip.clone()).hint_delay().tooltip(
                                        move |_, app| {
                                            app.new(|_| {
                                                crate::view::tooltip::Tooltip::text(
                                                    tip.clone(),
                                                    theme,
                                                )
                                            })
                                            .into()
                                        },
                                    )
                                })
                                .cursor_pointer()
                                .when(selected, |tab| {
                                    tab.child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .right_0()
                                            .bottom_0()
                                            .h(theme::space::TAB_UNDERLINE)
                                            // Patina says where the keys
                                            // are; every other area's
                                            // front tab keeps its bar in
                                            // the gutter's grey.
                                            .bg(rgb(if focused {
                                                theme.accent.patina
                                            } else {
                                                theme.text.gutter
                                            })),
                                    )
                                })
                                // The right button says which tab the menu is
                                // about, and does not bring it to the front:
                                // a person closing a background tab from its
                                // menu did not ask to look at it first.
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener(move |root: &mut Root, _event, _window, _cx| {
                                        // The place in the strip travels
                                        // with the target: *close the
                                        // others*, *to the left* and *to
                                        // the right* are one question
                                        // about a place in a row — **this
                                        // tab's own row**, which with two
                                        // areas need not be the focused
                                        // one's (M12.8).
                                        let of = root.strip_of(id).len();
                                        let path = root.tab_path(id);
                                        let session = root.tabs.iter().any(|tab| {
                                            tab.id == id
                                                && tab.face().and_then(Body::session).is_some()
                                        });
                                        let neighbours = root.tab_neighbours(id);
                                        root.note_menu_target(crate::view::menu::Target::Tab {
                                            id,
                                            path,
                                            session,
                                            index,
                                            of,
                                            neighbours,
                                        });
                                    }),
                                )
                                .on_click(cx.listener(
                                    move |root, event: &gpui::ClickEvent, window, cx| {
                                        root.note_place(cx);
                                        root.active_tab = Some(id);
                                        // The one in front is part of the state: a
                                        // restart shows it.
                                        root.save();
                                        // **And it takes the keys.** The press gave
                                        // them to the root, whose own focus tracking
                                        // wins where nothing focusable was pressed,
                                        // and a terminal takes the focus on render
                                        // only when nobody has it — so `cmd-f`, a
                                        // paste or a letter after a click on a
                                        // terminal's tab went nowhere. Seen in
                                        // M5.12's window check 5.
                                        root.refocus(window, cx);
                                        // A second click names a session's tab —
                                        // M12.1; the first has already put it in
                                        // front, which is where it is named.
                                        if event.click_count() >= 2 && is_session {
                                            root.start_renaming_tab(id, window, cx);
                                        }
                                        cx.notify();
                                    },
                                ))
                                // Drag a tab to another place in the strip —
                                // M14. GPUI starts the drag only after its
                                // move threshold, which is what keeps a click
                                // that simply selects a tab a click. The
                                // payload carries the tab's id and name; the
                                // strip reorders live as the pointer crosses
                                // a tab's middle.
                                .map(|tab| {
                                    draggable(
                                        tab,
                                        id,
                                        cx.listener(move |root, drag: &TabDrag, window, cx| {
                                            root.begin_tab_drag(drag, window, cx);
                                            root.tab_landing = None;
                                            root.note_place(cx);
                                            root.active_tab = Some(id);
                                            root.save();
                                            root.refocus(window, cx);
                                        }),
                                        move |cx| {
                                            cx.new(|_| TabGhost {
                                                name: drag_name.clone(),
                                                theme,
                                            })
                                        },
                                    )
                                })
                                .map(|tab| {
                                    reorderable(
                                        tab,
                                        self.strip_scroll(area),
                                        cx.listener(
                                            move |root, placement: &TabPlacement, window, cx| {
                                                root.reorder_tab(
                                                    placement.dragged,
                                                    id,
                                                    placement.before,
                                                    window,
                                                    cx,
                                                );
                                                if let Some(from) = placement.released_from {
                                                    root.land_tab(placement.dragged, from, cx);
                                                }
                                            },
                                        ),
                                    )
                                })
                                // A harness's own mark in its brand colour —
                                // the orange Claude, the blue OpenCode, the
                                // green Codex, M14.1; constant while the agent
                                // works. Every harness wears
                                // the SVG mark now, not the shared glyph.
                                // **The cross takes the mark's place, under
                                // the pointer**. It lay over
                                // the tab's end on an opaque ground until
                                // 2026-09-25 and hid the name's last letters;
                                // in the mark's slot it covers nothing and the
                                // tab keeps its width. The slot is one size
                                // whatever it holds, so nothing moves.
                                .child(if compact {
                                    compact_process_mark(
                                        id,
                                        hovered,
                                        theme,
                                        ui.text(Text::MenuCloseTab),
                                        cx.listener(move |root, _, window, cx| {
                                            cx.stop_propagation();
                                            root.close_tab_asking(id, window, cx);
                                        }),
                                    )
                                    .into_any_element()
                                } else {
                                    div()
                                        .relative()
                                        .flex_none()
                                        .size(px(TAB_MARK))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(if hovered {
                                            div().into_any_element()
                                        } else {
                                            match &kind {
                                                Kind::Session(
                                                    runs @ (Runs::Claude(_)
                                                    | Runs::OpenCode(_)
                                                    | Runs::Codex(_)),
                                                ) => crate::view::kind::harness_mark(
                                                    match runs {
                                                        Runs::OpenCode(_) => DefaultTab::OpenCode,
                                                        Runs::Codex(_) => DefaultTab::Codex,
                                                        _ => DefaultTab::Claude,
                                                    },
                                                    theme,
                                                    theme::space::STATUS_ICON,
                                                    Shade::Full,
                                                ),
                                                // The title bar's cog at a file icon's
                                                // size: the `⚙` glyph at the meta size
                                                // stood half as tall as its neighbours.
                                                Kind::Settings | Kind::Process => gpui::svg()
                                                    .path(if kind == Kind::Process {
                                                        PROCESS_ICON
                                                    } else {
                                                        "ui/title-settings.svg"
                                                    })
                                                    .flex_none()
                                                    .size(theme::from_px(16.0))
                                                    .text_color(rgb(kind_colour(kind, theme)))
                                                    .into_any_element(),
                                                _ => match icon {
                                                    Some(stem) => crate::icons::icon(&stem)
                                                        .flex_none()
                                                        .into_any_element(),
                                                    None => div()
                                                        .text_size(theme::size::META)
                                                        .text_color(rgb(kind_colour(kind, theme)))
                                                        .child(kind_glyph(kind))
                                                        .into_any_element(),
                                                },
                                            }
                                        })
                                        .child(
                                            tab_close_control(
                                                id,
                                                hovered,
                                                theme,
                                                ui.text(Text::MenuCloseTab),
                                                cx.listener(move |root, _, window, cx| {
                                                    root.close_tab_asking(id, window, cx);
                                                }),
                                            )
                                            .absolute()
                                            .left(-(theme::space::TAB_GAP / 2.0))
                                            .right(-(theme::space::TAB_GAP / 2.0))
                                            .top_1_2()
                                            .mt(-(theme::space::TABS / 2.0)),
                                        )
                                        .into_any_element()
                                })
                                .children((!compact).then(|| {
                                    match &renaming {
                                        // The name being typed, in the tab itself
                                        // — the field of Go to Line, small.
                                        Some((renamed, typed)) if *renamed == id => div()
                                            .flex()
                                            .items_center()
                                            .min_w(px(60.0))
                                            .px(px(4.0))
                                            .rounded(theme::radius::ROW)
                                            .border_1()
                                            .border_color(rgb(theme.accent.brass))
                                            .bg(rgb(theme.surface.card))
                                            .text_color(rgb(theme.text.primary))
                                            .child(self.rename_input(typed.to_string(), cx))
                                            .into_any_element(),
                                        _ => div()
                                            .debug_selector(|| "activity-tab-label".into())
                                            .min_w_0()
                                            .truncate()
                                            .text_size(theme::size::BODY)
                                            .text_color(rgb(if hovered || (selected && focused) {
                                                theme.text.primary
                                            } else if selected {
                                                theme.text.secondary
                                            } else {
                                                theme.text.tertiary
                                            }))
                                            .child(label)
                                            .into_any_element(),
                                    }
                                }))
                                // **The split glyph**, Lucide's `columns-2` at
                                // the mark's size in `text.secondary`: the
                                // tab holds more than it shows in the strip.
                                .when(divided, |tab| {
                                    tab.child(
                                        gpui::svg()
                                            .path("ui/columns-2.svg")
                                            .flex_none()
                                            .size(theme::space::STATUS_ICON)
                                            .text_color(rgb(theme.text.secondary)),
                                    )
                                })
                                // The unsaved dot, section 6.5: brass for text not
                                // yet written, rust when the disk moved meanwhile
                                // and a save would write over someone's change.
                                .when_some(mark, |tab, mark| {
                                    tab.child(
                                        div()
                                            .flex_none()
                                            .size(theme::space::UNSAVED_DOT)
                                            .rounded_full()
                                            .bg(rgb(match mark {
                                                Mark::Unsaved => theme.accent.brass,
                                                Mark::Conflict => theme.accent.rust,
                                            })),
                                    )
                                })
                                .children(process_runs.into_iter().filter_map(|(session, run)| {
                                    process_reopen_control(
                                        self.process_navigation_visible(session, run),
                                        || {
                                            gpui_component::button::Button::new(SharedString::from(
                                                format!("open-process-{run}"),
                                            ))
                                            .debug_selector(|| "process-reopen-button".into())
                                            .small()
                                            .size(theme::space::HEAD_BUTTON)
                                            .custom(super::controls::ghost(theme, false, cx))
                                            .icon(
                                                gpui_component::Icon::default().path(PROCESS_ICON),
                                            )
                                            .accessibility_label(ui.text(
                                                if self.process_for_origin(session).contains(&run) {
                                                    Text::ProcessOpen
                                                } else {
                                                    Text::ProcessRecover
                                                },
                                            ))
                                            .hint(
                                                format!(
                                                    "{} · {}",
                                                    ui.text(
                                                        if self
                                                            .process_for_origin(session)
                                                            .contains(&run)
                                                        {
                                                            Text::ProcessOpen
                                                        } else {
                                                            Text::ProcessRecover
                                                        }
                                                    ),
                                                    self.process_tab_tip(run)
                                                ),
                                                theme,
                                            )
                                            .on_click(
                                                cx.listener(move |root, _, _, cx| {
                                                    cx.stop_propagation();
                                                    root.navigate_process(session, run, cx);
                                                }),
                                            )
                                        },
                                    )
                                }))
                                // Activity is separate from steady harness identity.
                                .when(status != Status::Idle, |tab| {
                                    tab.child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .items_center()
                                            .debug_selector(|| "tab-activity-mark".into())
                                            .child(crate::view::activity::mark(
                                                status, theme, true, false,
                                            )),
                                    )
                                })
                                .map(|tab| {
                                    landing::animate(
                                        tab,
                                        self.tab_landing
                                            .filter(|(landed, _)| *landed == id)
                                            .map(|(_, landing)| landing),
                                        theme,
                                    )
                                })
                            // A settings tab has nothing to spend, so this
                            // simply never appears on one.
                            // **A tab carries its name and nothing
                            // else.** It held what the session had
                            // spent — money first, then tokens — and
                            // both were a number nobody asked to be
                            // reminded of on the thing they click
                            // dozens of times an hour. What was spent
                            // is on the session's row in the list,
                            // where it is asked for. M12 task 14.
                        },
                    ))
                    // The add button follows the last tab — M12.6.
                    .child(self.render_add_tab(!rows.is_empty(), area, cx))
                    // **Let go past the tabs, and the tab goes to the end
                    // of this strip** — its own or another area's, M12.8,
                    // decision 8. Only past them: a tab takes a drop on
                    // itself and places the dragged tab beside it. The live pass
                    // answers only on a tab, so a fast drag past a narrow
                    // last tab could stop one short of the end; this is
                    // what takes it the rest of the way, as M14's pass over
                    // every tab once did.
                    .when_some(area, |row, area| {
                        row.on_drop(cx.listener(move |root, drag: &TabDrag, window, cx| {
                            root.move_tab_to_area(drag.id, area, None, window, cx);
                            root.land_tab(drag.id, drag.origin(window), cx);
                        }))
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(|root, event: &gpui::MouseDownEvent, window, cx| {
                            root.open_context_menu(event.position, window, cx)
                        }),
                    ),
            )
            // Fixed order: hidden tabs, Markdown modes, then the actions menu.
            .child(self.render_strip_buttons(
                &rows,
                area,
                active.and_then(|id| {
                    let tab = self.tabs.iter().find(|tab| tab.id == id)?;
                    match tab.body()? {
                        Body::File {
                            presentation: Some(presentation),
                            ..
                        } => Some(presentation.controls.clone()),
                        _ => None,
                    }
                }),
                cx,
            ))
            .child(self.tab_drag.measure(area, self.strip_scroll(area)))
            .into_any_element()
    }
}

/// Omit the entire layout child while open, including its parent's flex gap.
fn process_reopen_control(
    show: bool,
    button: impl FnOnce() -> gpui_component::button::Button,
) -> Option<gpui_component::button::Button> {
    show.then(button)
}

/// The logo always selects; the reserved trailing slot closes only on its own
/// click. Swapping the only logo for a close cross would make an icon-only tab
/// impossible to select by its visible mark.
fn compact_process_mark(
    id: TabId,
    hovered: bool,
    theme: theme::Theme,
    close_label: &'static str,
    close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(theme::space::TAB_UNDERLINE)
        .child(
            gpui::svg()
                .path(PROCESS_ICON)
                .flex_none()
                .size(px(TAB_MARK))
                .text_color(rgb(theme.text.secondary)),
        )
        .child(
            div().relative().flex_none().size(px(TAB_CLOSE)).child(
                tab_close_control(id, hovered, theme, close_label, close)
                    .debug_selector(|| "compact-process-close".into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_1_2()
                    .mt(-(theme::space::TABS / 2.0)),
            ),
        )
}

/// A stable press target, even before the hover redraw replaces the mark.
/// Consume down as well as click: otherwise the ancestor records this press
/// and GPUI's two-point threshold turns ordinary click motion into a tab drag.
fn tab_close_control(
    id: TabId,
    hovered: bool,
    theme: theme::Theme,
    close_label: &'static str,
    close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(format!("close-{id}")))
        .group("tab-close")
        .aria_label(close_label)
        .h(theme::space::TABS)
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            close(event, window, cx);
        })
        .child(
            div()
                .size(px(TAB_CLOSE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .opacity(if hovered { 1.0 } else { 0.0 })
                .group_hover("tab-close", |circle| circle.bg(rgb(theme.accent.rust_bg)))
                .child(
                    crate::icons::glyph("icons/close.svg", theme::size::UI)
                        .text_color(rgb(theme.text.faint))
                        .group_hover("tab-close", |cross| {
                            cross.text_color(rgb(theme.accent.rust))
                        }),
                ),
        )
}

/// The scroller shrinks while trailing actions retain their natural width.
fn strip_row(theme: theme::Theme) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .min_w_0()
        .items_center()
        .h(theme::space::TABS)
        .px(theme::space::CARD_GAP)
        .relative()
        // Paint inside the existing height, behind the active tab's underline.
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(1.0))
                .bg(rgb(theme.surface.divider)),
        )
}

/// Only tabs scroll; the overflow menu and Markdown modes are fixed siblings.
fn scrolling_tabs(id: impl Into<gpui::ElementId>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_1()
        .h_full()
        .items_center()
        .min_w_0()
        .overflow_x_scroll()
}

/// The slot a tab's mark sits in — a file icon's size — and the circle of
/// the cross that takes its place under the pointer, a little larger and
/// centred on it.
const TAB_MARK: f32 = 16.0;
const TAB_CLOSE: f32 = 18.0;

/// Whether the tab drawn at `tab` answers a drag at `at`: the pointer is
/// on the part of it the hand can see, inside `strip` — its strip's
/// visible box. GPUI hands every `on_drag_move` listener every move, and
/// the bounds it gives are the tab's own, **not clipped by the strip that
/// scrolls it**; its own hit test intersects a hitbox with its content
/// mask, and this is that intersection. Without it a tab scrolled out past
/// one area's strip answered a drag over the next area's (traps.md).
pub fn answers_drag(
    tab: gpui::Bounds<gpui::Pixels>,
    strip: gpui::Bounds<gpui::Pixels>,
    at: gpui::Point<gpui::Pixels>,
) -> bool {
    tab.intersect(&strip).contains(&at)
}

/// Select at GPUI's drag threshold, before moving or releasing the tab.
fn draggable<V: Render>(
    tab: gpui::Stateful<gpui::Div>,
    id: TabId,
    select: impl Fn(&TabDrag, &mut gpui::Window, &mut gpui::App) + 'static,
    preview: impl Fn(&mut gpui::App) -> gpui::Entity<V> + 'static,
) -> gpui::Stateful<gpui::Div> {
    tab.on_drag(TabDrag::new(id), move |drag, cursor, window, cx| {
        drag.cursor.set(cursor);
        select(drag, window, cx);
        preview(cx)
    })
}

/// One destination within a tab, shared by movement and release.
struct TabPlacement {
    dragged: TabId,
    before: bool,
    released_from: Option<gpui::Point<gpui::Pixels>>,
}

/// Bind the strip's gesture separately from its contents: a terminal's
/// process and status do not participate in placing a tab.
fn reorderable(
    tab: gpui::Stateful<gpui::Div>,
    strip: gpui::ScrollHandle,
    listener: impl Fn(&TabPlacement, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let listener = std::rc::Rc::new(listener);
    let drop_listener = listener.clone();
    // Hitboxes and this geometry describe the same painted frame. ScrollHandle
    // may already contain the next offset when release arrives before drawing.
    let painted = std::rc::Rc::new(std::cell::Cell::new(None));
    let measure = painted.clone();
    let drop_bounds = painted.clone();
    tab.relative()
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let viewport =
                        gpui::Bounds::new(gpui::point(px(0.), px(0.)), window.viewport_size());
                    let visible = bounds
                        .intersect(&strip.bounds())
                        .intersect(&window.content_mask().bounds)
                        .intersect(&viewport);
                    measure.set(Some((bounds, visible)));
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .on_drop(move |drag: &TabDrag, window, cx| {
            let Some((bounds, visible)) = drop_bounds.get() else {
                return;
            };
            let at = window.mouse_position();
            if visible.contains(&at) {
                drop_listener(
                    &TabPlacement {
                        dragged: drag.id,
                        before: at.x < bounds.center().x,
                        released_from: Some(drag.origin(window)),
                    },
                    window,
                    cx,
                );
            }
        })
        // Every listener runs even when its tab is clipped or in another area.
        .on_drag_move::<TabDrag>(move |event, window, cx| {
            let Some((bounds, visible)) = painted.get() else {
                return;
            };
            let at = event.event.position;
            if visible.contains(&at) {
                listener(
                    &TabPlacement {
                        dragged: event.drag(cx).id,
                        before: at.x < bounds.center().x,
                        released_from: None,
                    },
                    window,
                    cx,
                );
            }
        })
}

/// What a strip drag carries: the tab being dragged. M14. An empty
/// area's start screen takes it too — M12.8.
#[derive(Clone)]
pub(crate) struct TabDrag {
    pub(crate) id: TabId,
    token: uuid::Uuid,
    cursor: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
}

impl TabDrag {
    fn new(id: TabId) -> Self {
        Self {
            id,
            token: uuid::Uuid::new_v4(),
            cursor: Default::default(),
        }
    }

    pub(crate) fn origin(&self, window: &gpui::Window) -> gpui::Point<gpui::Pixels> {
        window.mouse_position() - self.cursor.get()
    }
}

/// The ghost GPUI puts under the cursor while a tab is dragged — the
/// strip's own card, lifted by a patina border and a shadow of padding.
struct TabGhost {
    name: SharedString,
    theme: crate::theme::Theme,
}

impl Render for TabGhost {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<'_, Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::TAB_GAP)
            .px(theme::space::TAB_PADDING)
            .py(px(4.0))
            .rounded(theme::radius::ROW)
            .border_1()
            .border_color(rgb(theme.accent.patina))
            .bg(rgb(theme.surface.raised))
            .text_color(rgb(theme.text.primary))
            .child(self.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, ScrollHandle, TestAppContext, point};

    struct OriginStrip(gpui::Entity<Root>);
    impl Render for OriginStrip {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |root, cx| {
                div()
                    .w(px(600.))
                    .child(root.render_tabs(root.focused_area(), cx))
            })
        }
    }

    #[gpui::test]
    fn closed_process_origin_has_no_extra_trailing_space(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::closed_process_origin_has_no_extra_trailing_space"
        )) {
            return;
        }
        use gpui::AppContext;
        use guild_session::workflow::{OriginBinding, RunId};
        use guild_session::{Desk, Seat, SessionMode, SessionRecord};
        let dir = tempfile::tempdir().unwrap();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let session = desk.add_session(SessionRecord::new(
                "Origin".into(),
                seat,
                dir.path().into(),
                Runs::Claude(SessionMode::Terminal),
            ));
            let root = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root.active_workspace = workspace;
                root.active_project = Some(project);
                let parent = root.adopt(
                    seat,
                    project,
                    Body::Broken {
                        session,
                        reason: "free fixture".into(),
                    },
                    cx,
                );
                root.active_tab = Some(parent);
                let origin = OriginBinding {
                    session,
                    project,
                    workspace,
                    area: Some(root.tabs[0].area),
                    harness: guild_session::HarnessKind::Claude,
                    conversation: None,
                    cwd: dir.path().into(),
                };
                root.processes.runs.entry(RunId::new()).or_default().value =
                    Some(serde_json::json!({"origin": origin}));
                root
            });
            let strip = cx.new(|_| OriginStrip(root.clone()));
            made = Some((root, strip.clone(), session));
            gpui_component::Root::new(strip, window, cx)
        });
        cx.run_until_parked();
        let tab = cx.debug_bounds("activity-tab").unwrap();
        let button = cx.debug_bounds("process-reopen-button").unwrap();
        assert_eq!(
            tab.right() - button.right(),
            px(0.),
            "the last reopen hit target supplies its own inset; no extra trailing tab padding"
        );
        let (root, strip, session) = made.unwrap();
        cx.update(|_, cx| {
            root.update(cx, |root, _| {
                root.board
                    .report(session, Some(guild_session::status::Reported::Working));
            });
            strip.update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        let tab = cx.debug_bounds("activity-tab").unwrap();
        let activity = cx.debug_bounds("tab-activity-mark").unwrap();
        let button = cx.debug_bounds("process-reopen-button").unwrap();
        assert!(activity.left() >= button.right());
        assert_eq!(
            tab.right() - activity.right(),
            cx.update(|window, _| theme::space::TAB_PADDING.to_pixels(window.rem_size()))
        );
    }

    struct ReopenControl {
        open: bool,
        parent_clicks: usize,
        reopens: usize,
        built: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl Render for ReopenControl {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("reopen-test-parent")
                .flex()
                .items_center()
                .gap(theme::space::TAB_GAP)
                .w(px(120.0))
                .h(theme::space::TABS)
                .on_click(cx.listener(|this, _, _, _| this.parent_clicks += 1))
                .child(div().flex_none().w(px(40.0)))
                .children(process_reopen_control(!self.open, || {
                    self.built.set(self.built.get() + 1);
                    gpui_component::button::Button::new("reopen-test-button")
                        .small()
                        .size(theme::space::HEAD_BUTTON)
                        .accessibility_label("Open process")
                        .tooltip("Open process")
                        .child(
                            div()
                                .debug_selector(|| "reopen-control-content".into())
                                .child("x"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.reopens += 1;
                            this.open = true;
                            cx.notify();
                        }))
                }))
                .child(
                    div()
                        .flex_none()
                        .w(px(20.0))
                        .h_full()
                        .debug_selector(|| "reopen-neighbor".into()),
                )
        }
    }
    #[gpui::test]
    fn open_process_removes_reopen_child_and_gap_and_closed_view_reopens(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let built = std::rc::Rc::new(std::cell::Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, _| ReopenControl {
            open: true,
            parent_clicks: 0,
            reopens: 0,
            built: built.clone(),
        });
        cx.run_until_parked();
        let neighbor = cx.debug_bounds("reopen-neighbor").unwrap();
        assert_eq!(
            neighbor.left(),
            px(49.0),
            "open view contributes neither reopen width nor an extra flex gap"
        );
        assert_eq!(built.get(), 0);
        assert!(cx.debug_bounds("reopen-control-content").is_none());
        cx.simulate_mouse_down(
            point(px(10.0), px(18.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            point(px(10.0), px(18.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|_, cx| assert_eq!((view.read(cx).parent_clicks, view.read(cx).reopens), (1, 0)));
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.open = false;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let control = cx.debug_bounds("reopen-control-content").unwrap();
        assert_eq!(
            cx.debug_bounds("reopen-neighbor").unwrap().left(),
            neighbor.left() + px(33.0)
        );
        cx.simulate_mouse_down(control.center(), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(control.center(), MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!((view.read(cx).parent_clicks, view.read(cx).reopens), (1, 1)));
        assert!(cx.debug_bounds("reopen-control-content").is_none());
        assert_eq!(cx.debug_bounds("reopen-neighbor").unwrap(), neighbor);
    }

    struct CompactProcessHits {
        selected: usize,
        closed: usize,
        tab: TabId,
    }
    impl Render for CompactProcessHits {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .items_center()
                .w(theme::space::TABS + theme::space::CARD_GAP)
                .h(theme::space::TABS)
                .px(theme::space::CARD_GAP / 2.0)
                .id("compact-hit-parent")
                .on_click(cx.listener(|this, _, _, _| this.selected += 1))
                .child(compact_process_mark(
                    self.tab,
                    true,
                    theme::Theme::of(Default::default()),
                    "Close Process",
                    cx.listener(|this, _, _, _| this.closed += 1),
                ))
        }
    }
    #[gpui::test]
    fn compact_process_logo_and_close_have_distinct_click_targets(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| CompactProcessHits {
            selected: 0,
            closed: 0,
            tab: TabId::new(),
        });
        cx.run_until_parked();
        let close = cx.debug_bounds("compact-process-close").unwrap();
        assert_eq!(f32::from(close.size.width), TAB_CLOSE);
        assert!(close.left() > px(TAB_MARK));
        let click = |cx: &mut gpui::VisualTestContext, at| {
            cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
        };
        click(cx, point(px(10.0), px(18.0)));
        cx.update(|_, cx| assert_eq!((view.read(cx).selected, view.read(cx).closed), (1, 0)));
        click(cx, close.center());
        cx.update(|_, cx| assert_eq!((view.read(cx).selected, view.read(cx).closed), (1, 1)));
        click(cx, point(px(10.0), px(18.0)));
        cx.update(|_, cx| assert_eq!((view.read(cx).selected, view.read(cx).closed), (2, 1)));
    }

    struct MarkdownStrip {
        pane: crate::markdown::Presentation,
        scroll: ScrollHandle,
        selected: usize,
    }
    impl Render for MarkdownStrip {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    strip_row(theme::Theme::default())
                        .child(
                            scrolling_tabs("overflow-tabs")
                                .debug_selector(|| "overflow-tabs".into())
                                .track_scroll(&self.scroll)
                                .children((0..30usize).map(|i| {
                                    div()
                                        .id(("test-tab", i))
                                        .debug_selector(move || format!("test-tab-{i}"))
                                        .flex_none()
                                        .w(px(110.))
                                        .h_full()
                                        .child(format!("document-{i}.md"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.selected = i;
                                            cx.notify();
                                        }))
                                })),
                        )
                        .child(crate::view::tab_list::strip_separator(
                            theme::Theme::default(),
                        ))
                        .child(self.pane.controls.clone())
                        .child(crate::view::tab_list::strip_separator(
                            theme::Theme::default(),
                        ))
                        .child(
                            div()
                                .debug_selector(|| "tab-actions-test".into())
                                .flex_none()
                                .w(px(24.))
                                .child("⋮"),
                        ),
                )
                .child(self.pane.view.clone())
        }
    }
    #[gpui::test]
    fn markdown_modes_stay_pinned_beside_overflowing_selectable_tabs(cx: &mut TestAppContext) {
        use crate::{
            file_tab::{FileTab, Look},
            i18n::Ui,
            markdown::Presentation,
        };
        use gpui::AppContext;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "A paragraph.").unwrap();
        let mut strip = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    std::path::Path::new("README.md"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: theme::Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let pane =
                Presentation::for_file(&file, guild_session::markdown::Mode::Split, cx).unwrap();
            let view = cx.new(|_| MarkdownStrip {
                pane,
                scroll: ScrollHandle::new(),
                selected: 0,
            });
            strip = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        let strip = strip.unwrap();
        for width in [180., 360., 900.] {
            cx.simulate_resize(gpui::size(px(width), px(500.)));
            let tabs = cx.debug_bounds("overflow-tabs").unwrap();
            assert!(tabs.size.width > px(0.));
            let controls: Vec<_> = ["markdown-editor", "markdown-split", "markdown-preview"]
                .map(|id| cx.debug_bounds(id).unwrap())
                .into();
            for button in &controls {
                assert!(button.left() >= tabs.right(), "actions never cover a tab");
                assert!(button.right() <= px(width), "actions fit the narrow strip");
                assert!(
                    button.top() >= tabs.top() && button.bottom() <= tabs.bottom(),
                    "same row"
                );
            }
            assert!(
                cx.debug_bounds("tab-actions-test").unwrap().left() >= controls[2].right(),
                "the tab actions menu is always rightmost"
            );
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: tabs.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(-10000.), px(0.))),
                ..Default::default()
            });
            let last = cx.debug_bounds("test-tab-29").unwrap().intersect(&tabs);
            assert!(last.size.width > px(0.), "last tab remains reachable");
            cx.simulate_click(last.center(), Modifiers::default());
            cx.update(|_, cx| assert_eq!(strip.read(cx).selected, 29));
            assert_eq!(
                cx.debug_bounds("markdown-preview").unwrap(),
                controls[2],
                "modes never scroll away"
            );
            cx.simulate_click(controls[2].center(), Modifiers::default());
            cx.update(|_, cx| {
                assert_eq!(
                    strip.read(cx).pane.mode(),
                    guild_session::markdown::Mode::Preview
                )
            });
            cx.simulate_click(controls[0].center(), Modifiers::default());
            cx.update(|_, cx| {
                assert_eq!(
                    strip.read(cx).pane.mode(),
                    guild_session::markdown::Mode::Editor
                )
            });
        }
    }

    // The actual strip gesture with inert tab bodies. No sessions, disk
    // state or periodic agent updates can make the gesture pass by accident.
    struct Strip {
        order: Vec<(TabId, u8)>,
        scroll: ScrollHandle,
        width: f32,
        last_release: Option<TabId>,
        active: TabId,
    }

    impl Render for Strip {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("strip")
                .flex()
                .w(px(self.width))
                .h(px(36.0))
                .overflow_x_scroll()
                .track_scroll(&self.scroll)
                .children(self.order.iter().map(|&(id, _)| {
                    let tab = div()
                        .id(SharedString::from(format!("tab-{id}")))
                        .flex_none()
                        .w(px(100.0))
                        .h_full();
                    let tab = draggable(
                        tab,
                        id,
                        cx.listener(|strip, drag: &TabDrag, _, cx| {
                            strip.active = drag.id;
                            cx.notify();
                        }),
                        |cx| cx.new(|_| gpui::Empty),
                    );
                    reorderable(
                        tab,
                        self.scroll.clone(),
                        cx.listener(move |strip, placement: &TabPlacement, _, cx| {
                            if placement.released_from.is_some() {
                                strip.last_release = Some(placement.dragged);
                            }
                            if crate::stage::reorder(
                                &mut strip.order,
                                placement.dragged,
                                id,
                                placement.before,
                            ) {
                                cx.notify();
                            }
                        }),
                    )
                }))
        }
    }

    #[gpui::test]
    fn dragging_a_background_tab_selects_it_before_reordering(cx: &mut TestAppContext) {
        let ids: Vec<_> = (0..5).map(|_| TabId::new()).collect();
        let (view, cx) = cx.add_window_view(|_, _| Strip {
            order: ids.iter().map(|id| (*id, 0)).collect(),
            scroll: ScrollHandle::new(),
            width: 500.0,
            last_release: None,
            active: ids[4],
        });
        let at = |x| point(px(x), px(18.0));
        // A background tab's context menu does not select it.
        cx.simulate_mouse_down(at(50.0), MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(at(50.0), MouseButton::Right, Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).active, ids[4]));
        cx.simulate_mouse_down(at(50.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).active, ids[4]));
        cx.simulate_mouse_move(at(70.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).active, ids[0]);
            assert_eq!(view.read(cx).order[0].0, ids[0]);
            assert_eq!(view.read(cx).last_release, None);
        });
        // Selection survives a drop across four other tabs.
        cx.simulate_mouse_up(at(480.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).active, ids[0]);
            assert_eq!(view.read(cx).order[4].0, ids[0]);
        });
    }

    #[gpui::test]
    fn release_places_a_tab_beyond_the_last_drag_move(cx: &mut TestAppContext) {
        let ids: Vec<_> = (0..5).map(|_| TabId::new()).collect();
        let (view, cx) = cx.add_window_view(|_, _| Strip {
            order: ids.iter().map(|id| (*id, 0)).collect(),
            scroll: ScrollHandle::new(),
            width: 500.0,
            last_release: None,
            active: ids[4],
        });
        let at = |x| point(px(x), px(18.0));
        cx.simulate_mouse_down(at(50.0), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(70.0), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(180.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).order[1].0, ids[0]);
            assert_eq!(view.read(cx).last_release, None);
        });
        // The release is the final position, even with no intervening
        // move event there (a fast gesture or a return from outside).
        cx.simulate_mouse_up(at(480.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).last_release, Some(ids[0]));
            assert_eq!(
                view.read(cx)
                    .order
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>(),
                vec![ids[1], ids[2], ids[3], ids[4], ids[0]],
            )
        });
    }

    #[gpui::test]
    fn a_continuous_drag_crosses_several_tabs_and_can_reverse(cx: &mut TestAppContext) {
        let ids: Vec<_> = (0..5).map(|_| TabId::new()).collect();
        let (view, cx) = cx.add_window_view(|_, _| Strip {
            order: ids.iter().map(|id| (*id, 0)).collect(),
            scroll: ScrollHandle::new(),
            width: 500.0,
            last_release: None,
            active: ids[4],
        });
        let at = |x| point(px(x), px(18.0));
        cx.simulate_mouse_down(at(50.0), MouseButton::Left, Modifiers::default());
        for x in [70.0, 180.0, 280.0, 380.0, 480.0] {
            cx.simulate_mouse_move(at(x), MouseButton::Left, Modifiers::default());
        }
        cx.update(|_, cx| assert_eq!(view.read(cx).order[4].0, ids[0]));
        for x in [320.0, 220.0, 120.0, 20.0] {
            cx.simulate_mouse_move(at(x), MouseButton::Left, Modifiers::default());
        }
        cx.simulate_mouse_up(at(20.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx)
                    .order
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>(),
                ids,
            )
        });
    }

    #[gpui::test]
    fn release_after_unpainted_scroll_uses_painted_target(cx: &mut TestAppContext) {
        let ids: Vec<_> = (0..6).map(|_| TabId::new()).collect();
        let (view, cx) = cx.add_window_view(|_, _| Strip {
            order: ids.iter().map(|id| (*id, 0)).collect(),
            scroll: ScrollHandle::new(),
            width: 300.,
            last_release: None,
            active: ids[0],
        });
        let at = |x| point(px(x), px(18.));
        cx.simulate_mouse_down(at(30.), MouseButton::Left, Default::default());
        cx.simulate_mouse_move(at(60.), MouseButton::Left, Default::default());
        cx.update(|window, cx| {
            view.read(cx).scroll.set_offset(point(px(-150.), px(0.)));
            // Dispatch without drawing: the native receiver is the painted second tab.
            window.dispatch_event(
                gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                    position: at(195.),
                    button: MouseButton::Left,
                    click_count: 1,
                    modifiers: Default::default(),
                }),
                cx,
            );
            assert_eq!(
                view.read(cx).last_release,
                Some(ids[0]),
                "release must not be swallowed by a live-offset/painted-hitbox mismatch"
            );
            assert_eq!(view.read(cx).order[1].0, ids[0]);
        });
    }

    #[gpui::test]
    fn release_uses_the_visible_half_of_a_scrolled_tab(cx: &mut TestAppContext) {
        let ids: Vec<_> = (0..5).map(|_| TabId::new()).collect();
        let (view, cx) = cx.add_window_view(|_, _| Strip {
            order: ids.iter().map(|id| (*id, 0)).collect(),
            scroll: ScrollHandle::new(),
            width: 300.0,
            last_release: None,
            active: ids[4],
        });
        cx.update(|_, cx| {
            view.update(cx, |strip, cx| {
                strip.scroll.set_offset(point(px(-200.0), px(0.0)));
                cx.notify();
            })
        });
        cx.run_until_parked();
        let at = |x| point(px(x), px(18.0));
        // Last tab to before the third, with no move over the destination.
        cx.simulate_mouse_down(at(250.0), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(230.0), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(20.0), MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx)
                    .order
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>(),
                vec![ids[0], ids[1], ids[4], ids[2], ids[3]],
            )
        });
    }
}

#[cfg(test)]
mod drag_tests;
