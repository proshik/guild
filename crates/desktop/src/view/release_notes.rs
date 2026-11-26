//! Shared release-note composition for Settings, Update and What's New.
use crate::{
    Root,
    i18n::{Text, Ui},
    theme::{self, Theme},
    view::popup,
};
use gpui::{Context, Entity, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb};
use gpui_kit::base::FocusTrapElement as _;
use guild_session::changelog::{Group, Notes};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

#[derive(Default)]
pub(crate) struct Presentation {
    pub(crate) overlay: OnceCell<Entity<gpui_kit::base::TooltipOverlay>>,
    card: RefCell<Option<Entity<NotesCard>>>,
    greeting_category: Cell<Option<Group>>,
    popover_focus: OnceCell<gpui::FocusHandle>,
    greeting_focus: OnceCell<gpui::FocusHandle>,
    trigger_focus: OnceCell<gpui::FocusHandle>,
    greeting_continue: OnceCell<gpui::FocusHandle>,
    greeting_active: Cell<bool>,
    previous_focus: RefCell<Option<gpui::FocusHandle>>,
}
impl Presentation {
    pub(crate) fn holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        // The first greeting frame has no recorded ancestor path yet.
        (self.greeting_active.get()
            && self
                .greeting_continue
                .get()
                .is_some_and(|focus| focus.is_focused(window)))
            || self
                .trigger_focus
                .get()
                .is_some_and(|focus| focus.is_focused(window))
            || self
                .popover_focus
                .get()
                .is_some_and(|focus| focus.contains_focused(window, cx))
            || self
                .greeting_focus
                .get()
                .is_some_and(|focus| focus.contains_focused(window, cx))
    }
    pub(crate) fn hide(&self, cx: &mut gpui::App) {
        if let Some(overlay) = self.overlay.get() {
            overlay.update(cx, |overlay, cx| overlay.hide(cx));
        }
    }
}

/// Parse only real Keep a Changelog headings/bullets, retaining continuation text.
/// An unstructured body is displayed verbatim by the caller.
pub(crate) fn parse_body(body: &str) -> Notes {
    let mut entries: Vec<(Group, String)> = Vec::new();
    let mut group = None;
    let mut open = None;
    for line in body.lines() {
        if let Some(name) = line.strip_prefix("### ") {
            group = Group::ALL
                .into_iter()
                .find(|group| group.name() == name.trim());
            open = None;
        } else if let Some(entry) = line.strip_prefix("- ") {
            open = group.map(|group| {
                entries.push((group, entry.into()));
                entries.len() - 1
            });
        } else if let Some(index) = open {
            entries[index].1.push('\n');
            entries[index].1.push_str(line);
        }
    }
    Group::ALL
        .into_iter()
        .filter_map(|group| {
            let lines: Vec<_> = entries
                .iter()
                .filter(|(g, _)| *g == group)
                .map(|(_, line)| line.trim_end_matches('\n').to_owned())
                .collect();
            (!lines.is_empty()).then_some((group, lines))
        })
        .collect()
}
/// Display grouping only; source Keep a Changelog categories remain untouched.
pub(crate) fn display_bucket(group: Group) -> Group {
    match group {
        Group::Added => Group::Added,
        Group::Changed | Group::Deprecated | Group::Removed => Group::Changed,
        Group::Fixed | Group::Security => Group::Fixed,
    }
}
pub(crate) fn display_notes(notes: &Notes) -> Notes {
    [Group::Added, Group::Changed, Group::Fixed]
        .into_iter()
        .filter_map(|bucket| {
            let entries: Vec<_> = notes
                .iter()
                .filter(|(group, _)| display_bucket(*group) == bucket)
                .flat_map(|(_, entries)| entries.iter().cloned())
                .collect();
            (!entries.is_empty()).then_some((bucket, entries))
        })
        .collect()
}
pub(crate) fn selected(notes: &Notes, choice: Option<Group>) -> Option<Group> {
    let notes = display_notes(notes);
    choice
        .map(display_bucket)
        .filter(|group| notes.iter().any(|(g, _)| g == group))
        .or_else(|| notes.first().map(|(group, _)| *group))
}

pub(crate) fn button(
    theme: Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .min_h(theme::space::HEAD_BUTTON)
        .px(theme::space::CARD_GAP)
        .rounded(theme::radius::ROW)
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(theme::size::UI)
        .bg(rgb(theme.accent.brass_bg))
        .text_color(rgb(theme.accent.brass))
        .hover(|style| style.bg(theme::control_hover(theme, theme.accent.brass_bg)))
        .focus(|style| style.border_color(rgb(theme.accent.patina)))
        .child(label)
}

/// Three balanced display buckets, wrapping only when genuinely constrained.
type CategoryClick = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App)>;

pub(crate) fn categories(
    notes: &Notes,
    choice: Option<Group>,
    theme: Theme,
    ui: Ui,
    prefix: &'static str,
    pick: impl Fn(Group) -> CategoryClick,
) -> gpui::Div {
    let notes = display_notes(notes);
    div()
        .flex()
        .flex_wrap()
        .w_full()
        .min_w_0()
        .p(theme::space::STATUS_BUTTON_GAP)
        .rounded(theme::radius::ROW)
        .bg(rgb(theme.surface.inset))
        .children(notes.iter().map(|(group, entries)| {
            let group = *group;
            let on = choice == Some(group);
            let label = format!("{} {}", ui.change_group(group), entries.len());
            let callback = pick(group);
            // Native buttons own their focus and Enter/Space activation.
            gpui_kit::base::Button::new(SharedString::from(format!("{prefix}-{}", group.name())))
                .accessibility_label(label)
                .debug_selector(move || format!("{prefix}-{}", group.name()))
                .flex_1()
                .min_h(theme::space::HEAD_BUTTON)
                .px(theme::space::CARD_GAP)
                .rounded(theme::radius::ROW)
                .border_1()
                .border_color(gpui::transparent_black())
                .bg(if on {
                    rgb(theme.surface.raised).into()
                } else {
                    gpui::transparent_black()
                })
                .text_size(theme::size::UI)
                .text_color(rgb(if on {
                    theme.text.primary
                } else {
                    theme.text.secondary
                }))
                .focus(|style| style.border_color(rgb(theme.accent.patina)))
                .hover(|style| style.bg(theme::control_hover(theme, theme.surface.inset)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::space::TREE_INSET)
                        .child(ui.change_group(group))
                        .child(
                            div()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.tertiary))
                                .child(entries.len().to_string()),
                        ),
                )
                .on_click(move |event, window, cx| callback(event, window, cx))
        }))
}
fn group_icon(group: Group) -> &'static str {
    match group {
        Group::Added => "ui/head-plus.svg",
        Group::Changed => "ui/menu-rename.svg",
        Group::Deprecated => "ui/job-info.svg",
        Group::Removed => "ui/menu-remove.svg",
        Group::Fixed => "ui/check.svg",
        Group::Security => "ui/workspace-shield.svg",
    }
}
pub(crate) fn entries(group: Group, lines: &[String], theme: Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(theme::space::CARD_PADDING)
        .children(lines.iter().map(|line| {
            div()
                .flex()
                .items_start()
                .gap(theme::space::CARD_INNER_GAP)
                .w_full()
                .min_w_0()
                .text_size(theme::size::BODY)
                .line_height(gpui::relative(theme::leading::TEXT))
                .text_color(rgb(theme.text.secondary))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(theme::space::HEAD_BUTTON)
                        .rounded(theme::radius::ROW)
                        .bg(rgb(theme.surface.inset))
                        .child(
                            crate::icons::toolbar(group_icon(group))
                                .size(theme::space::ICON)
                                .text_color(rgb(theme.text.tertiary)),
                        ),
                )
                .child(div().flex_1().min_w_0().child(line.clone()))
        }))
}
pub(crate) fn content(
    notes: &Notes,
    choice: Option<Group>,
    raw: &str,
    theme: Theme,
    prefix: &'static str,
) -> gpui::AnyElement {
    let notes = display_notes(notes);
    if let Some((group, lines)) = notes.iter().find(|(group, _)| Some(*group) == choice) {
        let group = *group;
        div()
            .debug_selector(move || format!("{prefix}-content-{}", group.name()))
            .w_full()
            .min_w_0()
            .child(entries(group, lines, theme))
            .into_any_element()
    } else {
        div()
            .debug_selector(move || format!("{prefix}-raw"))
            .w_full()
            .min_w_0()
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.secondary))
            .line_height(gpui::relative(theme::leading::TEXT))
            .child(raw.to_owned())
            .into_any_element()
    }
}

pub(crate) fn scroll_content(
    id: &'static str,
    scroll: &gpui::ScrollHandle,
    child: gpui::AnyElement,
    theme: Theme,
    rem: gpui::Pixels,
    inset: gpui::Rems,
) -> gpui::Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .min_h_0()
        .min_w_0()
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .p(inset)
                .child(child),
        )
        .child(
            gpui_component::scroll::Scrollbar::vertical(scroll)
                .id(SharedString::from(format!("{id}-bar")))
                .mode(gpui_component::scroll::ScrollbarMode::Always)
                .styles(|s| {
                    s.track(|s| {
                        s.width(theme::space::SCROLLBAR.to_pixels(rem))
                            .bg(gpui::transparent_black())
                            .border_color(gpui::transparent_black())
                    })
                    .thumb(|s| s.bg(rgb(theme.text.faint)))
                }),
        )
}

/// Immutable presentation ownership; an old action cannot target a newer artifact.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    epoch: Option<uuid::Uuid>,
    identity: Option<crate::update::control::Identity>,
    build: crate::update::Build,
    ready: bool,
    raw: String,
    notes: Notes,
}
impl Root {
    pub(crate) fn notes_offer(&self) -> Option<Offer> {
        let build = self.update.staged()?.clone();
        let epoch = self.update_source_epoch();
        let view = self.selected_update();
        if epoch.is_some()
            && (self.update_failure().is_some()
                || view.is_none()
                || !self.update_ready_to_check()
                || self.update_checking()
                || self.update_downloading()
                || self.update_installing())
        {
            return None;
        }
        let raw = view.map(|view| view.notes.clone()).unwrap_or_default();
        Some(Offer {
            epoch,
            identity: view.map(|view| view.identity.clone()),
            build,
            ready: epoch.is_none() || self.update_download_ready(),
            notes: if view.is_some() {
                parse_body(&raw)
            } else {
                self.update.staged_notes().clone()
            },
            raw,
        })
    }
    pub(crate) fn apply_notes_offer(
        &mut self,
        offer: &Offer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.notes_offer().as_ref() != Some(offer) {
            self.update.presentation.hide(cx);
            return false;
        }
        self.update.presentation.hide(cx);
        self.apply_update(window, cx);
        true
    }
    pub(crate) fn settle_notes_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let presentation = &self.update.presentation;
        let showing = self.update.greeting().is_some();
        if showing && !presentation.greeting_active.replace(true) {
            *presentation.previous_focus.borrow_mut() = window.focused(cx);
            presentation
                .greeting_continue
                .get_or_init(|| cx.focus_handle())
                .focus(window, cx);
            presentation.hide(cx);
        } else if !showing && presentation.greeting_active.replace(false) {
            if presentation.holds_focus(window, cx) {
                if let Some(previous) = presentation.previous_focus.borrow_mut().take() {
                    previous.focus(window, cx);
                }
            } else {
                presentation.previous_focus.borrow_mut().take();
            }
        }
    }
    pub(crate) fn notes_trigger_focus(&self, cx: &mut Context<Self>) -> gpui::FocusHandle {
        self.update
            .presentation
            .trigger_focus
            .get_or_init(|| cx.focus_handle())
            .clone()
    }
    pub(crate) fn notes_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Entity<gpui_kit::base::TooltipOverlay> {
        if self
            .update
            .presentation
            .card
            .borrow()
            .as_ref()
            .is_some_and(|card| self.notes_offer().as_ref() != Some(&card.read(cx).offer))
        {
            self.update.presentation.hide(cx);
        }
        self.update
            .presentation
            .overlay
            .get_or_init(|| cx.new(|_| gpui_kit::base::TooltipOverlay::new()))
            .clone()
    }
    pub(crate) fn notes_trigger(
        &self,
        bounds: gpui::Bounds<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.update.greeting().is_some() {
            return;
        }
        let Some(offer) = self.notes_offer() else {
            return;
        };
        let overlay = self.notes_overlay(cx);
        let mut card = self.update.presentation.card.borrow_mut();
        if card
            .as_ref()
            .is_none_or(|card| card.read(cx).offer != offer)
        {
            let root = cx.entity().downgrade();
            let previous_focus = window.focused(cx);
            *card = Some(cx.new(|_| NotesCard {
                root,
                offer,
                category: None,
                scroll: gpui::ScrollHandle::new(),
                bounds,
                previous_focus,
                pointer_outside: Default::default(),
            }));
        } else {
            let previous_focus = window.focused(cx);
            let from_card = self
                .update
                .presentation
                .popover_focus
                .get()
                .is_some_and(|focus| focus.contains_focused(window, cx));
            card.as_ref().unwrap().update(cx, |card, cx| {
                card.bounds = bounds;
                card.pointer_outside.set(false);
                if !from_card {
                    card.previous_focus = previous_focus;
                }
                // Refresh current theme/language and opening context, preserving
                // the selected bucket and scroll during internal hover transfer.
                cx.notify();
            });
        }
        let card = card.as_ref().unwrap().clone();
        overlay.update(cx, |overlay, cx| {
            overlay.request_show(request(bounds, card), window, cx)
        });
    }
}
fn request(
    bounds: gpui::Bounds<gpui::Pixels>,
    card: Entity<NotesCard>,
) -> gpui_kit::base::TooltipRequest {
    gpui_kit::base::TooltipRequest::new(bounds, move |_, _| card.clone().into())
        .placement(gpui_kit::base::Placement::Bottom)
}

pub(crate) struct NotesCard {
    root: gpui::WeakEntity<Root>,
    offer: Offer,
    category: Option<Group>,
    scroll: gpui::ScrollHandle,
    bounds: gpui::Bounds<gpui::Pixels>,
    previous_focus: Option<gpui::FocusHandle>,
    pointer_outside: Rc<Cell<bool>>,
}
impl Render for NotesCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.root.upgrade() else {
            return div().into_any_element();
        };
        let owner = root.read(cx);
        if owner.notes_offer().as_ref() != Some(&self.offer) {
            return div().into_any_element();
        }
        let theme = owner.theme;
        let ui = owner.ui();
        let focus = owner
            .update
            .presentation
            .popover_focus
            .get_or_init(|| cx.focus_handle().tab_stop(false))
            .clone();
        let overlay = owner.update.presentation.overlay.get().unwrap().clone();
        let show_overlay = overlay.clone();
        let card = cx.entity();
        let bounds = self.bounds;
        let previous_focus = self.previous_focus.clone();
        let leave_focus = previous_focus.clone();
        let card_focus = focus.clone();
        let pointer_outside = self.pointer_outside.clone();
        let selected = selected(&self.offer.notes, self.category);
        let pick = cx.entity().downgrade();
        let offer = self.offer.clone();
        let click_root = self.root.clone();
        let title = ui.text(Text::UpdateButton);
        let margin = theme::space::CARD_PADDING.to_pixels(window.rem_size());
        let width = theme::space::NOTES_CARD
            .to_pixels(window.rem_size())
            .min((window.viewport_size().width - margin * 2.).max(px(0.)));
        let available = bounds
            .top()
            .max(window.viewport_size().height - bounds.bottom());
        let height = theme::space::WHATS_NEW_LIST
            .to_pixels(window.rem_size())
            .min((available - margin * 2.).max(px(0.)));
        popup::card_look(theme)
            .id("update-notes-popover")
            .debug_selector(|| "update-notes-popover".into())
            .track_focus(&focus)
            // Guild masks the outer toolkit Root's Tab binding for terminals.
            // These native controls give that binding a deeper local context.
            .key_context("Root")
            .occlude()
            .w(width)
            .h(height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |card_bounds, _, window, _| {
                        let overlay = show_overlay.clone();
                        let card = card.clone();
                        let previous_focus = leave_focus.clone();
                        let focus = card_focus.clone();
                        let exit_overlay = overlay.clone();
                        let exit_focus = focus.clone();
                        let exit_previous = previous_focus.clone();
                        let outside = pointer_outside.clone();
                        let exit_outside = outside.clone();
                        window.on_mouse_event(
                            move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Capture {
                                    return;
                                }
                                let inside = card_bounds.contains(&event.position);
                                let over_trigger = bounds.contains(&event.position);
                                let overlay = overlay.clone();
                                let card = card.clone();
                                let previous_focus = previous_focus.clone();
                                let focus = focus.clone();
                                let outside = outside.clone();
                                // Run after the trigger's leave event. Descendant buttons can
                                // occlude the parent's hover hitbox without leaving its bounds.
                                window.defer(cx, move |window, cx| {
                                    overlay.update(cx, |overlay, cx| {
                                        if inside {
                                            outside.set(false);
                                            overlay.request_show(request(bounds, card), window, cx);
                                        } else if over_trigger {
                                            if outside.replace(false) {
                                                overlay.request_show(
                                                    request(bounds, card),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        } else if !outside.replace(true) {
                                            // Kit restarts its grace timer on every hide request.
                                            // Own the departure transition, not each outside move.
                                            if focus.contains_focused(window, cx)
                                                && let Some(previous) = &previous_focus
                                            {
                                                previous.focus(window, cx);
                                            }
                                            overlay.request_hide(window, cx);
                                        }
                                    })
                                });
                            },
                        );
                        window.on_mouse_event(
                            move |_: &gpui::MouseExitEvent, phase, window, cx| {
                                if phase == gpui::DispatchPhase::Capture
                                    && !exit_outside.replace(true)
                                {
                                    if exit_focus.contains_focused(window, cx)
                                        && let Some(previous) = &exit_previous
                                    {
                                        previous.focus(window, cx);
                                    }
                                    exit_overlay
                                        .update(cx, |overlay, cx| overlay.request_hide(window, cx));
                                }
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .on_key_down(move |event, window, cx| {
                if event.keystroke.key == "escape" {
                    overlay.update(cx, |overlay, cx| overlay.hide(cx));
                    if let Some(previous) = &previous_focus {
                        previous.focus(window, cx);
                    }
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_start()
                    .gap(theme::space::CARD_GAP)
                    .p(theme::space::CARD_PADDING)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme::size::BODY)
                            .text_color(rgb(theme.text.primary))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title)
                            .child(
                                div()
                                    .pt(theme::space::TREE_INSET)
                                    .text_size(theme::size::META)
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(rgb(theme.text.tertiary))
                                    .child(
                                        self.offer
                                            .build
                                            .version()
                                            .unwrap_or(self.offer.build.short())
                                            .to_owned(),
                                    ),
                            ),
                    )
                    .child(
                        button(
                            theme,
                            "update-notes-action",
                            ui.text(if self.offer.ready {
                                Text::UpdaterRestart
                            } else {
                                Text::UpdaterDownload
                            })
                            .into(),
                        )
                        .debug_selector(|| "update-notes-action".into())
                        .on_click(move |_, window, cx| {
                            let _ = click_root
                                .update(cx, |root, cx| root.apply_notes_offer(&offer, window, cx));
                        }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .px(theme::space::CARD_PADDING)
                    .pb(theme::space::CARD_GAP)
                    .child(categories(
                        &self.offer.notes,
                        selected,
                        theme,
                        ui,
                        "popover",
                        move |group| {
                            let pick = pick.clone();
                            Box::new(move |_, _, cx| {
                                let _ = pick.update(cx, |card, cx| {
                                    card.category = Some(group);
                                    card.scroll.set_offset(gpui::point(px(0.), px(0.)));
                                    cx.notify();
                                });
                            })
                        },
                    )),
            )
            .child(
                scroll_content(
                    "popover-notes-scroll",
                    &self.scroll,
                    content(
                        &self.offer.notes,
                        selected,
                        &self.offer.raw,
                        theme,
                        "popover",
                    ),
                    theme,
                    window.rem_size(),
                    theme::space::CARD_PADDING,
                )
                .flex_1(),
            )
            .into_any_element()
    }
}

impl Root {
    pub(crate) fn render_whats_new(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let greeting = self.update.greeting()?;
        let theme = self.theme;
        let ui = self.ui();
        let margin = theme::space::CARD_PADDING.to_pixels(window.rem_size());
        let viewport = window.viewport_size();
        let width = theme::space::WHATS_NEW
            .to_pixels(window.rem_size())
            .min((viewport.width - margin * 2.).max(px(0.)));
        let height = (viewport.height - margin * 2.).max(px(0.));
        let merged = guild_session::changelog::merged(
            &greeting
                .releases
                .iter()
                .map(|section| guild_session::changelog::Release {
                    version: section.version.clone(),
                    date: section.date.clone(),
                    entries: section
                        .notes
                        .iter()
                        .flat_map(|(group, entries)| {
                            entries.iter().map(|entry| (*group, entry.clone()))
                        })
                        .collect(),
                })
                .collect::<Vec<_>>()
                .iter()
                .collect::<Vec<_>>(),
        );
        let selected = selected(&merged, self.update.presentation.greeting_category.get());
        let root = cx.entity().downgrade();
        let sub = match &greeting.since {
            Some(since) => ui.whats_new_since(since, greeting.releases.len()),
            None => greeting.version.clone(),
        };
        let blocks = greeting.releases.iter().filter_map(|section| {
            let notes = display_notes(&section.notes);
            let (group, lines) = notes.iter().find(|(group, _)| Some(*group) == selected)?;
            let version = section.version.clone();
            Some(
                div()
                    .debug_selector(move || format!("whats-new-release-{version}"))
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_INNER_GAP)
                    .pb(theme::space::CARD_PADDING)
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(theme::space::CARD_GAP)
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.tertiary))
                            .child(section.version.clone())
                            .children(section.date.clone()),
                    )
                    .child(entries(*group, lines, theme)),
            )
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
                .p(theme::space::CARD_PADDING)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id("whats-new-backdrop")
                        .debug_selector(|| "whats-new-backdrop".into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(crate::view::goto_line::dim(theme.surface.backdrop))
                        .on_click(cx.listener(away)),
                )
                .child(
                    popup::card_look(theme)
                        .id("whats-new")
                        .debug_selector(|| "whats-new".into())
                        .track_focus(
                            self.update
                                .presentation
                                .greeting_focus
                                .get_or_init(|| cx.focus_handle().tab_stop(false)),
                        )
                        .key_context("Root")
                        .focus_trap(
                            "whats-new-focus",
                            self.update.presentation.greeting_focus.get().unwrap(),
                        )
                        .role(gpui::Role::Dialog)
                        .occlude()
                        .rounded(theme::radius::CARD)
                        .w(width)
                        .max_h(height)
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(theme::space::CARD_GAP)
                                .p(theme::space::CARD_PADDING)
                                .child(
                                    div()
                                        .text_size(theme::size::META)
                                        .text_color(rgb(theme.text.tertiary))
                                        .child(sub),
                                )
                                .child(
                                    div()
                                        .text_size(theme::size::TITLE)
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(theme.text.primary))
                                        .child(ui.text(Text::WhatsNewTitle)),
                                )
                                .child(categories(
                                    &merged,
                                    selected,
                                    theme,
                                    ui,
                                    "whats-new",
                                    move |group| {
                                        let root = root.clone();
                                        Box::new(move |_, _, cx| {
                                            let _ = root.update(cx, |root, cx| {
                                                root.update
                                                    .presentation
                                                    .greeting_category
                                                    .set(Some(group));
                                                root.update
                                                    .greeting_scroll()
                                                    .set_offset(gpui::point(px(0.), px(0.)));
                                                cx.notify();
                                            });
                                        })
                                    },
                                )),
                        )
                        .child(
                            scroll_content(
                                "whats-new-list",
                                self.update.greeting_scroll(),
                                div().flex().flex_col().children(blocks).into_any_element(),
                                theme,
                                window.rem_size(),
                                theme::space::CARD_PADDING,
                            )
                            .h(theme::space::WHATS_NEW_LIST)
                            .flex_shrink(1.),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .justify_end()
                                .p(theme::space::CARD_PADDING)
                                .child(
                                    button(
                                        theme,
                                        "whats-new-continue",
                                        ui.text(Text::WhatsNewContinue).into(),
                                    )
                                    .track_focus(
                                        self.update
                                            .presentation
                                            .greeting_continue
                                            .get_or_init(|| cx.focus_handle()),
                                    )
                                    .debug_selector(|| "whats-new-continue".into())
                                    .on_click(cx.listener(away)),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
    pub(crate) fn dismiss_whats_new(&mut self, cx: &mut Context<Self>) {
        if self.update.dismiss_greeting() {
            cx.notify();
        }
    }
}
fn away(root: &mut Root, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Root>) {
    cx.stop_propagation();
    root.dismiss_whats_new(cx);
}
