//! Configured external applications, using the existing Settings and field adapters.
use super::sections::setting_button;
use crate::view::tooltip::Hint as _;
use crate::{Root, i18n::Text, theme};
use gpui::{AppContext, Context, Entity, FocusHandle, Window, div, prelude::*, rgb};
use gpui_component::input::InputState;

fn settings_app_keys(
    config: &guild_session::open_in::Config,
    apps: Option<&[crate::open_in::Connection]>,
) -> Vec<String> {
    config
        .keys(&crate::open_in::default_keys())
        .into_iter()
        .filter(|key| {
            config.custom.iter().any(|row| row.id == *key)
                || apps.is_some_and(|apps| apps.iter().any(|app| app.key == *key))
        })
        .collect()
}

pub(crate) struct AppPicker {
    key: String,
    query: String,
    selected: usize,
    input: Entity<InputState>,
    previous: Option<FocusHandle>,
    scroll: gpui::UniformListScrollHandle,
}

impl Root {
    pub(crate) fn ensure_open_in_inventory(&mut self, cx: &mut Context<Self>) {
        if self.open_in_inventory.is_some() || self.open_in_inventory_task.is_some() {
            return;
        }
        let worker = cx
            .background_executor()
            .spawn(async { crate::system::installed_applications() });
        self.open_in_inventory_task = Some(cx.spawn(async move |root, cx| {
            let apps = worker.await;
            let _ = root.update(cx, |root, cx| {
                root.open_in_inventory = Some(std::sync::Arc::new(apps));
                root.open_in_inventory_task = None;
                cx.notify();
            });
        }));
    }

    pub(crate) fn open_in_settings_changed(&mut self, cx: &mut Context<Self>) {
        self.open_in_apps = None;
        self.ensure_open_in_apps();
        self.revalidate_open_in_target(cx);
        self.store_settings();
        cx.notify();
    }

    fn open_application_selector(
        &mut self,
        key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .open_in_picker
            .as_ref()
            .is_some_and(|picker| picker.key == key)
        {
            self.close_application_selector(window, cx);
            return;
        }
        self.ensure_open_in_inventory(cx);
        let input = cx.new(|cx| InputState::new(window, cx));
        let previous = window.focused(cx);
        input.update(cx, |input, cx| input.focus(window, cx));
        self.open_in_picker = Some(AppPicker {
            key,
            query: String::new(),
            selected: 0,
            input,
            previous,
            scroll: Default::default(),
        });
        cx.notify();
    }

    fn close_application_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.open_in_picker.take()
            && let Some(focus) = picker.previous
        {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn select_installed_application(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &self.open_in_picker else {
            return;
        };
        let Some(apps) = &self.open_in_inventory else {
            return;
        };
        let Some(app) = crate::open_in::matching_applications(apps, &picker.query)
            .get(index)
            .cloned()
            .cloned()
        else {
            return;
        };
        let key = picker.key.clone();
        self.open_in_new_custom = None;
        self.settings
            .open_in_apps
            .connect(&key, app.path, app.bundle_id, &app.name);
        self.close_application_selector(window, cx);
        self.open_in_settings_changed(cx);
    }

    fn application_selector_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self
            .open_in_picker
            .as_ref()
            .map(|picker| {
                crate::open_in::matching_applications(
                    self.open_in_inventory
                        .as_deref()
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                    &picker.query,
                )
                .len()
            })
            .unwrap_or(0);
        match event.keystroke.key.as_str() {
            "escape" => self.close_application_selector(window, cx),
            "enter" => {
                if let Some(picker) = &self.open_in_picker {
                    self.select_installed_application(picker.selected, window, cx);
                }
            }
            "up" | "down" | "pageup" | "pagedown" => {
                if let Some(picker) = self.open_in_picker.as_mut() {
                    picker.selected = crate::pick::step(
                        count,
                        Some(picker.selected),
                        matches!(event.keystroke.key.as_str(), "down" | "pagedown"),
                    )
                    .unwrap_or(0);
                    picker
                        .scroll
                        .scroll_to_item(picker.selected, gpui::ScrollStrategy::Nearest);
                    cx.notify();
                }
            }
            "tab" => {
                self.close_application_selector(window, cx);
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
            }
            _ => {}
        }
    }

    fn render_application_selector(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(picker) = &self.open_in_picker else {
            return div().into_any_element();
        };
        let theme = self.theme;
        let count = crate::open_in::matching_applications(
            self.open_in_inventory
                .as_deref()
                .map(Vec::as_slice)
                .unwrap_or_default(),
            &picker.query,
        )
        .len();
        let input = picker.input.clone();
        div()
            .id("open-in-application-picker")
            .debug_selector(|| "open-in-application-picker".into())
            .w_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(theme::settings::outline(theme))
            .rounded(theme::radius::CARD)
            .bg(rgb(theme.surface.card))
            .child(
                div().h(theme::settings::CONTROL).child(
                    crate::text_field::TextField::new(
                        "open-in-application-search",
                        picker.query.clone(),
                        cx.listener(|root, value: &String, _, cx| {
                            if let Some(picker) = root.open_in_picker.as_mut() {
                                picker.query = value.clone();
                                picker.selected = 0;
                                picker
                                    .scroll
                                    .scroll_to_item(0, gpui::ScrollStrategy::Nearest);
                            }
                            cx.notify();
                        }),
                    )
                    .input(input)
                    .placeholder(self.ui().text(Text::OpenInSearch))
                    .padded()
                    .list()
                    .on_key(cx.listener(|root, event, window, cx| {
                        root.application_selector_key(event, window, cx)
                    })),
                ),
            )
            .when(count == 0, |card| {
                card.child(
                    div()
                        .p(theme::space::CARD_PADDING)
                        .text_color(rgb(theme.text.tertiary))
                        .child(self.ui().text(if self.open_in_inventory.is_none() {
                            Text::OpenInLoading
                        } else {
                            Text::OpenInNoResults
                        })),
                )
            })
            .when(count > 0, |card| {
                card.child(
                    gpui::uniform_list(
                        "open-in-installed-list",
                        count,
                        cx.processor(|root, range: std::ops::Range<usize>, _, cx| {
                            let Some(picker) = &root.open_in_picker else {
                                return Vec::new();
                            };
                            let apps = root.open_in_inventory.as_ref().unwrap();
                            let filtered =
                                crate::open_in::matching_applications(apps, &picker.query);
                            let selected = picker.selected;
                            range
                                .filter_map(|index| {
                                    filtered.get(index).map(|app| {
                                        let icon = root.open_in_icon(
                                            &format!("path:{}", app.path.display()),
                                            &app.path,
                                        );
                                        div()
                                            .id(("open-in-installed", index))
                                            .debug_selector(move || {
                                                format!("open-in-installed-{index}")
                                            })
                                            .h(theme::space::BRANCH_ROW)
                                            .px(theme::space::CARD_GAP)
                                            .flex()
                                            .items_center()
                                            .gap(theme::space::CARD_GAP)
                                            .cursor_pointer()
                                            .when(index == selected, |row| {
                                                row.bg(rgb(root.theme.surface.raised))
                                            })
                                            .hover(|row| {
                                                row.bg(crate::theme::control_hover(
                                                    root.theme,
                                                    root.theme.surface.card,
                                                ))
                                            })
                                            .child(root.open_in_icon_element(icon))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .child(app.name.clone())
                                                    .child(
                                                        div()
                                                            .text_size(theme::size::META)
                                                            .text_color(rgb(root
                                                                .theme
                                                                .text
                                                                .tertiary))
                                                            .truncate()
                                                            .child(app.path.display().to_string()),
                                                    ),
                                            )
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                |_, window, _| window.prevent_default(),
                                            )
                                            .on_click(cx.listener(move |root, _, window, cx| {
                                                root.select_installed_application(index, window, cx)
                                            }))
                                    })
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .h(theme::space::BRANCH_LIST)
                    .w_full()
                    .track_scroll(&picker.scroll),
                )
            })
            .into_any_element()
    }

    pub(crate) fn open_in_icon_element(
        &self,
        icon: Option<std::sync::Arc<gpui::Image>>,
    ) -> gpui::AnyElement {
        match icon {
            Some(image) => gpui::img(image)
                .size(theme::space::ICON)
                .flex_none()
                .into_any_element(),
            None => crate::icons::toolbar("ui/title-open-in.svg")
                .text_color(rgb(self.theme.text.secondary))
                .into_any_element(),
        }
    }

    pub(super) fn open_in_add_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let content = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .debug_selector(|| "open-in-add-icon".into())
                    .size(theme::space::HEAD_ICON)
                    .flex_none()
                    .child(
                        crate::icons::toolbar("ui/head-plus.svg")
                            .text_color(rgb(self.theme.text.primary)),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "open-in-add-label".into())
                    .child(self.ui().text(Text::OpenInAdd)),
            );
        crate::view::controls::value_content(self.theme, "open-in-add-app", content, false)
            .bg(gpui::transparent_black())
            .border_color(theme::settings::outline(self.theme))
            .debug_selector(|| "open-in-add-app".into())
            .on_click(cx.listener(|root, _, _, cx| {
                root.open_in_new_custom = Some(root.settings.open_in_apps.add_custom());
                root.settings_scroll.scroll_to_bottom();
                root.open_in_settings_changed(cx);
            }))
            .into_any_element()
    }

    pub(super) fn render_open_in_apps_settings(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let keys = settings_app_keys(&self.settings.open_in_apps, self.open_in_apps.as_deref());
        let total = keys.len();
        let visible_keys = keys.clone();
        div()
            .id("open-in-apps-settings")
            .debug_selector(|| "open-in-apps-settings".into())
            .w_full()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .children(keys.into_iter().enumerate().map(|(index, key)| {
                let connection = self
                    .open_in_apps
                    .as_ref()
                    .and_then(|apps| apps.iter().find(|app| app.key == key));
                let custom = self
                    .settings
                    .open_in_apps
                    .custom
                    .iter()
                    .find(|row| row.id == key);
                let builtin = crate::open_in::APPS.iter().find(|app| app.ids[0] == key);
                let icon =
                    connection.and_then(|app| self.open_in_icons.borrow().get(&app.id).cloned());
                let label = connection
                    .map(|app| app.name.clone())
                    .or_else(|| builtin.map(|app| app.name.to_owned()))
                    .unwrap_or_default();
                let filename = connection
                    .and_then(|app| app.path.as_ref())
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.ui().text(Text::OpenInUnavailable).into());
                let enabled = self.settings.open_in_apps.enabled(&key);
                let mut row = div()
                    .id(gpui::SharedString::from(format!("open-in-row-{key}")))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .p(theme::space::CARD_GAP)
                    .border_t_1()
                    .border_color(theme::settings::rule(theme));
                let mut line = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .child(self.open_in_icon_element(icon));
                if let Some(custom) = custom {
                    let id = key.clone();
                    line = line.child(
                        div()
                            .flex_1()
                            .min_w(theme::settings::LABEL_MIN)
                            .h(theme::settings::CONTROL)
                            .debug_selector({
                                let key = key.clone();
                                move || format!("open-in-name-{key}")
                            })
                            .border_1()
                            .border_color(theme::settings::outline(theme))
                            .rounded(theme::radius::ROW)
                            .child(
                                crate::text_field::TextField::new(
                                    gpui::SharedString::from(format!("open-in-name-{key}")),
                                    custom.name.clone(),
                                    cx.listener(move |root, name: &String, _, cx| {
                                        root.open_in_new_custom = None;
                                        root.settings.open_in_apps.rename(&id, name.clone());
                                        root.open_in_settings_changed(cx);
                                    }),
                                )
                                .placeholder(self.ui().text(Text::OpenInName))
                                .padded(),
                            ),
                    );
                } else {
                    line = line.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(label)
                            .child(
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.tertiary))
                                    .child(filename.clone()),
                            ),
                    );
                }
                let id = key.clone();
                line = line.child(
                    crate::view::controls::toggle(
                        theme,
                        gpui::SharedString::from(format!("open-in-enabled-{key}")),
                        enabled,
                    )
                    .debug_selector(move || format!("open-in-enabled-{id}"))
                    .on_click({
                        let id = key.clone();
                        cx.listener(move |root, _, _, cx| {
                            let next = !root.settings.open_in_apps.enabled(&id);
                            root.settings.open_in_apps.set_enabled(&id, next);
                            root.open_in_settings_changed(cx);
                        })
                    }),
                );
                for down in [false, true] {
                    let id = key.clone();
                    let can_move = if down { index + 1 < total } else { index > 0 };
                    let neighbor = if down {
                        visible_keys.get(index + 1)
                    } else {
                        index.checked_sub(1).and_then(|at| visible_keys.get(at))
                    }
                    .cloned();
                    line = line.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "open-in-move-{key}-{down}"
                            )))
                            .debug_selector({
                                let key = key.clone();
                                move || format!("open-in-move-{key}-{down}")
                            })
                            .when(can_move, |button| button.tab_index(0))
                            .size(theme::space::STATUS_BUTTON)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme::radius::ROW)
                            .when(can_move, |button| {
                                button.cursor_pointer().hover(|style| {
                                    style.bg(crate::theme::control_hover(theme, theme.surface.card))
                                })
                            })
                            .child(
                                crate::icons::toolbar(if down {
                                    "ui/chevron-down.svg"
                                } else {
                                    "ui/chevron-up.svg"
                                })
                                .text_color(rgb(if can_move {
                                    theme.text.secondary
                                } else {
                                    theme.text.faint
                                })),
                            )
                            .hint(
                                self.ui().text(if down {
                                    Text::OpenInMoveDown
                                } else {
                                    Text::OpenInMoveUp
                                }),
                                theme,
                            )
                            .on_click(cx.listener(move |root, _, _, cx| {
                                if can_move {
                                    let defaults = crate::open_in::default_keys();
                                    let all = root.settings.open_in_apps.keys(&defaults);
                                    if let (Some(from), Some(to)) = (
                                        all.iter().position(|key| key == &id),
                                        all.iter().position(|key| Some(key) == neighbor.as_ref()),
                                    ) {
                                        // Move one visible row, preserving hidden catalog preferences.
                                        for _ in 0..from.abs_diff(to) {
                                            root.settings
                                                .open_in_apps
                                                .move_item(&id, down, &defaults);
                                        }
                                    }
                                    root.open_in_settings_changed(cx);
                                }
                            })),
                    );
                }
                if custom.is_some() {
                    let id = key.clone();
                    line = line.child(
                        setting_button(
                            theme,
                            gpui::SharedString::from(format!("open-in-remove-{key}")),
                            self.ui().text(Text::OpenInRemove).into(),
                            false,
                        )
                        .text_color(rgb(theme.accent.rust))
                        .debug_selector({
                            let key = key.clone();
                            move || format!("open-in-remove-{key}")
                        })
                        .on_click(cx.listener(
                            move |root, _, window, cx| {
                                if root
                                    .open_in_picker
                                    .as_ref()
                                    .is_some_and(|picker| picker.key == id)
                                {
                                    root.close_application_selector(window, cx);
                                }
                                root.settings.open_in_apps.remove_custom(&id);
                                root.open_in_settings_changed(cx);
                            },
                        )),
                    );
                }
                row = row.child(line);
                if custom.is_some() {
                    let id = key.clone();
                    row = row.child(
                        setting_button(
                            theme,
                            gpui::SharedString::from(format!("open-in-choose-{key}")),
                            connection
                                .and_then(|app| app.path.as_ref())
                                .and_then(|path| path.file_stem())
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| self.ui().text(Text::OpenInChoose).into()),
                            self.open_in_picker
                                .as_ref()
                                .is_some_and(|picker| picker.key == key),
                        )
                        .justify_between()
                        .child(
                            crate::icons::toolbar(
                                if self
                                    .open_in_picker
                                    .as_ref()
                                    .is_some_and(|picker| picker.key == key)
                                {
                                    "ui/chevron-up.svg"
                                } else {
                                    "ui/chevron-down.svg"
                                },
                            )
                            .text_color(rgb(theme.text.secondary)),
                        )
                        .debug_selector({
                            let id = key.clone();
                            move || format!("open-in-choose-{id}")
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(cx.listener(
                            move |root, _, window, cx| {
                                root.open_application_selector(id.clone(), window, cx)
                            },
                        )),
                    );
                    if connection.is_some() {
                        row = row.child(
                            div()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.tertiary))
                                .child(filename),
                        );
                    }
                    if self
                        .open_in_picker
                        .as_ref()
                        .is_some_and(|picker| picker.key == key)
                    {
                        row = row.child(self.render_application_selector(cx));
                    }
                }
                row.into_any_element()
            }))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "open_in_tests.rs"]
mod tests;
