//! Deterministic native fixture; never restores projects or starts services.
use crate::{Root, notify::Level};
use gpui::{Context, Entity, Render, Window, div, prelude::*, px, rgb};
use gpui_component::{
    button::Button,
    input::{Input, InputState},
};
use std::time::{Duration, Instant};

pub struct NoticePreview {
    root: Entity<Root>,
    input: Entity<InputState>,
    clicks: usize,
}
impl NoticePreview {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let root = cx.new(|cx| {
            Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                guild_session::Desk::default(),
                cx,
            )
        });
        cx.observe(&root, |_, _, cx| cx.notify()).detach();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Focus stays here while reading and copying notices")
        });
        let timer_root = root.downgrade();
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    if cx
                        .update(|window, cx| {
                            timer_root.update(cx, |root, cx| {
                                root.sync_notice_hover(window, Instant::now());
                                if root.notices.expire(Instant::now()) {
                                    cx.notify();
                                }
                            })
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        let mut result = Self {
            root,
            input,
            clicks: 0,
        };
        result.populate(3, Level::Warning, cx);
        result
    }
    fn populate(&mut self, count: usize, level: Level, cx: &mut Context<Self>) {
        self.root.update(cx, |root, cx| {
            root.notices = Default::default();
            for n in 0..count {
                let text = format!(
                    "Notice {} — FIRST LINE\n{}\n{}LAST LINE — конец 🌍  \n",
                    n + 1,
                    "Ж".repeat(400),
                    (0..300)
                        .map(|i| format!("Строка {i}: deterministic warning\n"))
                        .collect::<String>()
                );
                root.notices.push(level, text, Instant::now());
            }
            cx.notify();
        });
    }
}
impl Render for NoticePreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::text_field::input_placeholder(
            &self.input,
            "Focus stays here while reading and copying notices",
            false,
            window,
            cx,
        );
        window.set_rem_size(crate::scale::rem_size(13.));
        let theme = self.root.read(cx).theme;
        let toolbar = div()
            .flex()
            .gap(px(8.))
            .children((1..=3).map(|n| {
                Button::new(("count", n)).label(n.to_string()).on_click(
                    cx.listener(move |this, _, _, cx| this.populate(n, Level::Warning, cx)),
                )
            }))
            .child(
                Button::new("info")
                    .label("Info")
                    .on_click(cx.listener(|this, _, _, cx| this.populate(3, Level::Info, cx))),
            )
            .child(
                Button::new("error")
                    .label("Error")
                    .on_click(cx.listener(|this, _, _, cx| this.populate(3, Level::Error, cx))),
            )
            .child(
                Button::new("add")
                    .label("Add")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.root.update(cx, |root, cx| {
                            root.notices.push(
                                Level::Warning,
                                format!("Added {:?}", Instant::now()),
                                Instant::now(),
                            );
                            cx.notify();
                        })
                    })),
            );
        let background = div()
            .id("background")
            .size_full()
            .bg(rgb(theme.surface.panel))
            .on_click(cx.listener(|this, _, _, cx| {
                this.clicks += 1;
                cx.notify();
            }))
            .child(format!("Background clicks: {}", self.clicks));
        let notices = self
            .root
            .update(cx, |root, cx| root.render_notices(window, cx));
        div()
            .relative()
            .size_full()
            .bg(rgb(theme.surface.app))
            .text_color(rgb(theme.text.primary))
            .font_family(crate::fonts::DEFAULT.ui)
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .p(px(16.))
                    .gap(px(8.))
                    .child(toolbar)
                    .child(
                        Input::new(&self.input)
                            .aria_label("Focus stays here while reading and copying notices"),
                    )
                    .child(background),
            )
            .child(notices)
    }
}
