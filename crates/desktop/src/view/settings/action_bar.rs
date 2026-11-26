//! Action Bar composition and ordering, shared by drag and keyboard-focused items.
use super::{SettingsHost, catalog::Setting};
use crate::view::menu::{Anchor, Choice, Deed, Entry, Label, Mark, Target};
use crate::{Root, i18n::Text, theme};
use gpui::{Context, Render, Window, div, prelude::*, rgb};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
};
use guild_session::action_bar::{self, Item};

#[derive(Clone)]
struct DragItem(Item);

#[derive(Default)]
pub(crate) struct OrderScroll {
    handle: gpui::ScrollHandle,
    pub(super) items:
        std::cell::RefCell<std::collections::HashMap<&'static str, gpui::FocusHandle>>,
    scope: std::cell::RefCell<Option<gpui::FocusHandle>>,
    add: std::cell::RefCell<Option<gpui::FocusHandle>>,
    revealed: std::rc::Rc<std::cell::Cell<Option<Revealed>>>,
}
#[derive(Clone, Copy, PartialEq)]
struct Revealed {
    item: &'static str,
    content_x: gpui::Pixels,
    width: gpui::Pixels,
    viewport_width: gpui::Pixels,
}

struct Preview {
    label: &'static str,
    theme: theme::Theme,
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(self.theme.surface.raised))
            .text_color(rgb(self.theme.text.primary))
            .text_size(theme::size::BODY)
            .child(self.label)
    }
}
fn label(item: Item) -> Text {
    match item {
        Item::Usage => Text::ActionBarUsage,
        Item::OpenIn => Text::OpenInButton,
        Item::TerminalJobs => Text::TerminalJobs,
    }
}

impl Root {
    /// The live Settings host must still draw the Action Bar row that opened Add.
    pub(crate) fn action_bar_settings_source(&self) -> Option<SettingsHost> {
        let chosen = self.settings_in_front()?;
        let host = match self.active_body() {
            Some(crate::Body::Settings { .. }) => SettingsHost::Tab(self.active_tab?),
            None => SettingsHost::Window,
            _ => return None,
        };
        if self.settings_query.trim().is_empty()
            && self.scoped_settings_target(host).is_some_and(|target| {
                !matches!(target, crate::root::repository_settings::Target::Global(_))
            })
        {
            return None;
        }
        self.visible_settings(chosen)
            .contains(&Setting::ActionBarOrder)
            .then_some(host)
    }

    pub(crate) fn settle_action_bar_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Target::ActionBarAdd { source, .. }) =
            self.context_menu.as_ref().map(|menu| &menu.target)
            && self.action_bar_settings_source() != Some(*source)
        {
            self.close_context_menu(window, cx);
        }
    }

    /// Keep native Settings item/menu focus out of the start-screen fallback.
    pub(crate) fn action_bar_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.settings_in_front().is_some()
            && self
                .action_bar_scroll
                .scope
                .borrow()
                .as_ref()
                .is_some_and(|focus| focus.contains_focused(window, cx))
    }

    pub(crate) fn move_action_bar_item(
        &mut self,
        item: Item,
        target: usize,
        cx: &mut Context<Self>,
    ) {
        if action_bar::move_to(
            &mut self.settings.action_bar_order,
            &self.settings.action_bar_excluded,
            item,
            target,
        ) {
            self.store_settings();
            cx.notify();
        }
    }
    fn focus_action_bar_item(&self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self
            .action_bar_scroll
            .items
            .borrow_mut()
            .entry(item.id())
            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
            .clone();
        let source = self.action_bar_settings_source();
        let owner = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = owner.update(cx, |root, cx| {
                if source.is_some()
                    && root.action_bar_settings_source() == source
                    && action_bar::visible(
                        &root.settings.action_bar_order,
                        &root.settings.action_bar_excluded,
                    )
                    .contains(&item)
                {
                    focus.focus(window, cx);
                    cx.notify();
                }
            });
        });
    }

    fn remove_action_bar_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let before = action_bar::visible(
            &self.settings.action_bar_order,
            &self.settings.action_bar_excluded,
        );
        let Some(index) = before.iter().position(|candidate| *candidate == item) else {
            return;
        };
        if !action_bar::remove(&mut self.settings.action_bar_excluded, item) {
            return;
        }
        if item == Item::Usage {
            self.showing_usage = false;
        }
        let after = action_bar::visible(
            &self.settings.action_bar_order,
            &self.settings.action_bar_excluded,
        );
        if let Some(next) = after.get(index.min(after.len().saturating_sub(1))) {
            self.focus_action_bar_item(*next, window, cx);
        } else if let Some(add) = self.action_bar_scroll.add.borrow().as_ref() {
            let add = add.clone();
            let source = self.action_bar_settings_source();
            let owner = cx.weak_entity();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |root, cx| {
                    if source.is_some() && root.action_bar_settings_source() == source {
                        add.focus(window, cx);
                        window.focus_next(cx);
                    }
                });
            });
        }
        self.store_settings();
        cx.notify();
    }

    pub(crate) fn add_action_bar_item(
        &mut self,
        item: Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action_bar::add(
            &mut self.settings.action_bar_order,
            &mut self.settings.action_bar_excluded,
            item,
        ) {
            self.focus_action_bar_item(item, window, cx);
            self.store_settings();
            cx.notify();
        }
    }

    pub(crate) fn action_bar_choices(&self) -> Vec<Entry> {
        Item::ALL
            .into_iter()
            .filter(|item| {
                self.settings
                    .action_bar_excluded
                    .iter()
                    .any(|id| id == item.id())
            })
            .map(|item| {
                Entry::Choice(Choice {
                    label: Label::Text(label(item)),
                    mark: match item {
                        Item::Usage => Mark::Harness(self.usage_front().mark()),
                        Item::OpenIn => Mark::Icon("ui/title-open-in.svg"),
                        Item::TerminalJobs => Mark::Icon("ui/workspace-terminal.svg"),
                    },
                    deed: Deed::ActionBarAdd(item),
                    check: None,
                    enabled: true,
                    detail: None,
                    trailing: None,
                })
            })
            .collect()
    }

    /// A passive sample of the existing titlebar control, using cached usage only.
    fn action_bar_sample(&self, item: Item) -> gpui::AnyElement {
        let theme = self.theme;
        let sample = div()
            .id(gpui::SharedString::from(format!(
                "action-bar-sample-{}",
                item.id()
            )))
            .debug_selector(move || format!("action-bar-sample-{}", item.id()))
            .flex()
            .flex_none()
            .items_center()
            .h(theme::space::STATUS_BUTTON)
            .rounded_full()
            .border_1()
            .border_color(theme::settings::outline(theme))
            .bg(rgb(theme.surface.card))
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.tertiary));
        match item {
            Item::Usage => {
                let front = self.usage_front();
                let reading = self
                    .usage
                    .of(front)
                    .and_then(|provider| crate::usage::lowest(provider.windows()));
                sample
                    .px(theme::space::CHIP_PADDING)
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .child(crate::view::kind::harness_mark(
                        front.mark(),
                        theme,
                        theme::size::META,
                        crate::view::kind::Shade::Full,
                    ))
                    .child(reading.map_or_else(|| "-".to_owned(), |left| format!("{left}%")))
                    .into_any_element()
            }
            Item::OpenIn | Item::TerminalJobs => sample
                .size(theme::space::STATUS_BUTTON)
                .justify_center()
                .child(
                    crate::icons::toolbar(if item == Item::OpenIn {
                        "ui/title-open-in.svg"
                    } else {
                        "ui/workspace-terminal.svg"
                    })
                    .text_color(rgb(theme.text.tertiary)),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_action_bar_order(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let order = action_bar::visible(
            &self.settings.action_bar_order,
            &self.settings.action_bar_excluded,
        );
        let length = order.len();
        let missing = Item::ALL
            .into_iter()
            .filter(|item| !order.contains(item))
            .collect::<Vec<_>>();
        let add_focus = self
            .action_bar_scroll
            .add
            .borrow_mut()
            .get_or_insert_with(|| cx.focus_handle().tab_stop(false))
            .clone();
        let scope = self
            .action_bar_scroll
            .scope
            .borrow_mut()
            .get_or_insert_with(|| cx.focus_handle().tab_stop(false))
            .clone();
        let add_at = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let measured_add = add_at.clone();
        div()
            .id("action-bar-order")
            .track_focus(&scope)
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .on_key_down(|event, window, cx| {
                if crate::view::controls::tab_navigation(event, window, cx) {
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id("action-bar-order-scroll")
                    .debug_selector(|| "action-bar-order-scroll".into())
                    .w_full()
                    .min_w_0()
                    .flex()
                    .gap(theme::space::CARD_GAP)
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.action_bar_scroll.handle)
                    .role(gpui::Role::List)
                    .aria_label(ui.text(Text::ActionBarItemOrder))
                    .when(order.is_empty(), |list| {
                        list.child(
                            div()
                                .debug_selector(|| "action-bar-empty".into())
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.secondary))
                                .child(ui.text(Text::ActionBarEmpty)),
                        )
                    })
                    .children(order.into_iter().enumerate().map(|(index, item)| {
                        let owner = cx.weak_entity();
                        let name = ui.text(label(item));
                        let focus = self
                            .action_bar_scroll
                            .items
                            .borrow_mut()
                            .entry(item.id())
                            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                            .clone();
                        let key_focus = focus.clone();
                        let click_focus = focus.clone();
                        let scroll = self.action_bar_scroll.handle.clone();
                        let revealed = self.action_bar_scroll.revealed.clone();
                        let entity = cx.entity_id();
                        div()
                            .id(gpui::SharedString::from(format!(
                                "action-bar-row-{}",
                                item.id()
                            )))
                            .debug_selector(move || format!("action-bar-row-{}", item.id()))
                            .track_focus(&focus)
                            .role(gpui::Role::ListItem)
                            .aria_label(name)
                            .aria_description(ui.text(Text::ActionBarKeyboardHint))
                            .aria_keyshortcuts("Alt+ArrowLeft Alt+ArrowRight")
                            .relative()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(theme::space::CARD_GAP)
                            .cursor(gpui::CursorStyle::OpenHand)
                            .p(theme::space::CARD_GAP)
                            .rounded(theme::radius::ROW)
                            .bg(rgb(theme.surface.inset))
                            .border_1()
                            .border_color(rgb(theme.surface.inset))
                            .focus_visible(|style| style.border_color(rgb(theme.accent.patina)))
                            .on_click(move |_, window, cx| click_focus.focus(window, cx))
                            .on_key_down(cx.listener(
                                move |root, event: &gpui::KeyDownEvent, window, cx| {
                                    let stroke = &event.keystroke;
                                    if !key_focus.is_focused(window)
                                        || !stroke.modifiers.alt
                                        || stroke.modifiers.platform
                                        || stroke.modifiers.control
                                        || stroke.modifiers.shift
                                    {
                                        return;
                                    }
                                    let target = match stroke.key.as_str() {
                                        "left" => index.saturating_sub(1),
                                        "right" => (index + 1).min(length - 1),
                                        _ => return,
                                    };
                                    root.move_action_bar_item(item, target, cx);
                                    cx.stop_propagation();
                                    window.prevent_default();
                                },
                            ))
                            .drag_over::<DragItem>(move |style, drag, _, _| {
                                if drag.0 != item {
                                    style.bg(rgb(theme.surface.raised))
                                } else {
                                    style
                                }
                            })
                            // Only a completed drop inside a visible item mutates settings.
                            .on_drop(cx.listener(move |root, drag: &DragItem, _, cx| {
                                root.move_action_bar_item(drag.0, index, cx)
                            }))
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!(
                                        "action-bar-handle-{}",
                                        item.id()
                                    )))
                                    .debug_selector(move || {
                                        format!("action-bar-handle-{}", item.id())
                                    })
                                    .flex()
                                    .items_center()
                                    .gap(theme::space::CARD_GAP)
                                    .child(div().text_color(rgb(theme.text.tertiary)).child("⠿"))
                                    .child(self.action_bar_sample(item))
                                    .child(div().text_color(rgb(theme.text.primary)).child(name)),
                            )
                            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                window.prevent_default()
                            })
                            .on_drag(DragItem(item), move |_, _, _, cx| {
                                let _ = owner.update(cx, |root, cx| {
                                    root.action_bar_dragging = true;
                                    cx.notify();
                                });
                                cx.new(|_| Preview { label: name, theme })
                            })
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!(
                                        "action-bar-remove-{}",
                                        item.id()
                                    )))
                                    .debug_selector(move || {
                                        format!("action-bar-remove-{}", item.id())
                                    })
                                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                                        window.prevent_default();
                                        cx.stop_propagation();
                                    })
                                    .child(
                                        gpui_kit::base::Button::new(gpui::SharedString::from(
                                            format!("action-bar-remove-button-{}", item.id()),
                                        ))
                                        .size(theme::space::STATUS_BUTTON)
                                        .p_0()
                                        .cursor_pointer()
                                        .rounded(theme::radius::ROW)
                                        .text_color(rgb(theme.text.secondary))
                                        .hover(|style| {
                                            style.bg(crate::theme::control_hover(
                                                theme,
                                                theme.surface.card,
                                            ))
                                        })
                                        .focus(|style| {
                                            style
                                                .bg(rgb(theme.surface.raised))
                                                .text_color(rgb(theme.accent.patina))
                                        })
                                        .accessibility_label(format!(
                                            "{}: {name}",
                                            ui.text(Text::ActionBarRemove)
                                        ))
                                        .child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("action-bar-remove-glyph-{}", item.id())
                                                })
                                                .size(theme::space::STATUS_ICON)
                                                .child(
                                                    gpui_component::Icon::new(
                                                        gpui_component::IconName::Close,
                                                    )
                                                    .size_full(),
                                                ),
                                        )
                                        .on_click(
                                            cx.listener(move |root, _, window, cx| {
                                                cx.stop_propagation();
                                                root.remove_action_bar_item(item, window, cx);
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                gpui::canvas(
                                    move |_, window, cx| {
                                        if !focus.contains_focused(window, cx) {
                                            if revealed
                                                .get()
                                                .is_some_and(|last| last.item == item.id())
                                            {
                                                revealed.set(None);
                                            }
                                            return;
                                        }
                                        // Like the tab strip, child bounds are current and unscrolled.
                                        let Some(mut bounds) = scroll.bounds_for_item(index) else {
                                            return;
                                        };
                                        bounds.origin += scroll.offset();
                                        let viewport = scroll.bounds();
                                        let next = Revealed {
                                            item: item.id(),
                                            content_x: bounds.left()
                                                - viewport.left()
                                                - scroll.offset().x,
                                            width: bounds.size.width,
                                            viewport_width: viewport.size.width,
                                        };
                                        // Reveal on focus/layout change; allow manual scrolling between changes.
                                        if revealed.replace(Some(next)) == Some(next) {
                                            return;
                                        }
                                        let delta = if bounds.left() < viewport.left() {
                                            viewport.left() - bounds.left()
                                        } else if bounds.right() > viewport.right() {
                                            viewport.right() - bounds.right()
                                        } else {
                                            gpui::px(0.)
                                        };
                                        if delta != gpui::px(0.) {
                                            let scroll = scroll.clone();
                                            // Use the reordered item's current prepaint bounds on the next layout.
                                            cx.defer(move |cx| {
                                                let mut offset = scroll.offset();
                                                offset.x += delta;
                                                scroll.set_offset(offset);
                                                cx.notify(entity);
                                            });
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(theme::space::CARD_GAP)
                    .child(
                        div()
                            .id("action-bar-add-focus")
                            .track_focus(&add_focus)
                            .debug_selector(|| "action-bar-add".into())
                            .relative()
                            .child(
                                gpui::canvas(
                                    move |bounds, _, _| measured_add.set(bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .aria_description(ui.text(if missing.is_empty() {
                                Text::ActionBarAllShown
                            } else {
                                Text::ActionBarAddHint
                            }))
                            .child(
                                Button::new("action-bar-add-button")
                                    .child(
                                        div()
                                            .text_size(theme::size::META)
                                            .debug_selector(|| "action-bar-add-label".into())
                                            .child(ui.text(Text::ActionBarAdd)),
                                    )
                                    .h(theme::space::HEAD_BUTTON)
                                    .px(theme::space::CARD_GAP)
                                    .custom(crate::view::controls::ghost(theme, false, cx))
                                    .border_1()
                                    .border_color(theme::settings::outline(theme))
                                    .rounded(gpui::px(self.rem_px(theme::radius::ROW)))
                                    .accessibility_label(format!(
                                        "{}: {}",
                                        ui.text(Text::ActionBarAdd),
                                        ui.text(if missing.is_empty() {
                                            Text::ActionBarAllShown
                                        } else {
                                            Text::ActionBarAddHint
                                        })
                                    ))
                                    .disabled(missing.is_empty())
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(source) = root.action_bar_settings_source()
                                            && !root.action_bar_choices().is_empty()
                                        {
                                            root.open_menu(
                                                Target::ActionBarAdd {
                                                    source,
                                                    return_focus: window.focused(cx),
                                                },
                                                Anchor::Below(add_at.get()),
                                                window,
                                                cx,
                                            );
                                        }
                                    })),
                            ),
                    )
                    .child(
                        Button::new("action-bar-reset")
                            .debug_selector(|| "action-bar-reset".into())
                            .child(
                                div()
                                    .text_size(theme::size::META)
                                    .debug_selector(|| "action-bar-reset-label".into())
                                    .child(ui.text(Text::ActionBarReset)),
                            )
                            .h(theme::space::HEAD_BUTTON)
                            .px(theme::space::CARD_GAP)
                            .accessibility_label(ui.text(Text::ActionBarReset))
                            .custom(crate::view::controls::ghost(theme, false, cx))
                            .border_1()
                            .border_color(theme::settings::outline(theme))
                            .rounded(gpui::px(self.rem_px(theme::radius::ROW)))
                            .on_click(cx.listener(|root, _, window, cx| {
                                action_bar::reset(
                                    &mut root.settings.action_bar_order,
                                    &mut root.settings.action_bar_excluded,
                                );
                                root.focus_action_bar_item(Item::Usage, window, cx);
                                root.store_settings();
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
