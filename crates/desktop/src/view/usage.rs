//! The title bar's usage chip and the card it opens — M12.10, decision 46.
//!
//! In Guild's shapes (section 6.9): the chip is the Problems button's rule —
//! a figure, and a disc only when the figure matters — and the card is the
//! language servers' list (6.21), a head, one line per window over a rounded
//! bar, and a status/Refresh footer.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, MouseButton, anchored, deferred, div, px, rgb};

use crate::agents::Agent;
use crate::i18n::Text;
use crate::root::usage::{Failure, Source};
use crate::theme::{self, Theme};
use crate::usage::{self, Level};
use crate::{Root, view::popup};

/// A disc's colour for a level; `None` for plenty, which draws nothing.
fn disc(level: Level, theme: Theme) -> Option<u32> {
    match level {
        Level::Plenty => None,
        Level::Low => Some(theme.accent.brass),
        Level::Scarce => Some(theme.accent.rust),
    }
}

/// The share and bar use green, brass and rust at the existing thresholds.
fn bar(level: Level, theme: Theme) -> u32 {
    disc(level, theme).unwrap_or(theme.accent.green)
}

fn usage_bar(left: u8, theme: Theme) -> gpui::Div {
    div()
        .mt(px(3.0))
        .w_full()
        .h(theme::space::USAGE_BAR_HEIGHT)
        .rounded_full()
        .overflow_hidden()
        .bg(rgb(theme.surface.raised))
        .child(
            div()
                .h_full()
                .rounded_full()
                .w(gpui::relative(f32::from(left) / 100.0))
                .bg(rgb(bar(usage::level(left), theme))),
        )
}

/// Every provider uses the same window anatomy, including the expanded list.
fn usage_window(
    window: &usage::Window,
    ui: crate::i18n::Ui,
    theme: Theme,
    now: time::OffsetDateTime,
) -> gpui::Div {
    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(theme::space::MENU_ICON_GAP)
        .px(theme::space::MENU_INSET)
        .pt(theme::space::MENU_ICON_GAP)
        .pb(theme::space::CARD_GAP)
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(theme::space::MENU_ICON_GAP)
                .text_size(theme::size::META)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.text.secondary))
                        .child(ui.usage_window(&window.name)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(bar(usage::level(window.left), theme)))
                        .child(ui.usage_left(window.left)),
                )
                .children(resets_left(window.resets_at, now).map(|span| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.faint))
                        .child(ui.usage_resets_in(span))
                })),
        )
        .child(usage_bar(window.left, theme))
}

fn usage_frame(theme: Theme, viewport_height: gpui::Pixels) -> gpui::Div {
    popup::card_look(theme)
        .flex()
        .flex_col()
        .w(theme::space::USAGE_CARD)
        .max_h((viewport_height - px(2.0 * theme::space::USAGE_WINDOW_MARGIN_PX)).max(px(0.0)))
        .py(theme::space::MENU_PADDING)
        .text_size(theme::size::BODY)
        .text_color(rgb(theme.text.primary))
}

fn usage_chip_frame(theme: Theme, open: bool, missing: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id("titlebar-usage")
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .debug_selector(|| "titlebar-usage".into())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.0))
        .h(theme::space::STATUS_BUTTON)
        .px(theme::space::CHIP_PADDING)
        .rounded_full()
        .border_1()
        .border_color(theme::settings::outline(theme))
        .bg(rgb(theme.surface.card))
        .cursor_pointer()
        .text_size(theme::size::META)
        .text_color(rgb(if missing {
            theme.text.faint
        } else {
            theme.text.tertiary
        }))
        .when(open, |chip| chip.bg(rgb(theme.surface.raised)))
        .hover(|chip| {
            chip.bg(crate::theme::control_hover(theme, theme.surface.card))
                .text_color(rgb(theme.text.primary))
        })
}

fn usage_body() -> gpui::Stateful<gpui::Div> {
    div()
        .id("usage-content")
        .flex()
        .flex_col()
        .min_h(px(0.0))
        .overflow_y_scroll()
}

/// Why a provider shows no windows, by its last failure.
fn reason_text(failure: Option<&Failure>, answered: bool) -> Text {
    match failure {
        Some(Failure::NoCredentials) => Text::UsageNotSignedIn,
        Some(Failure::NotInstalled) => Text::UsageNotInstalled,
        Some(Failure::RateLimited(_)) => Text::UsageRateLimited,
        Some(_) => Text::UsageCouldNotRead,
        None if answered => Text::UsageNothingToRead,
        None => Text::UsageAsking,
    }
}

/// What the Refresh row speaks of: the provider in front, as it stands.
#[derive(Clone, Copy)]
struct Shown<'a> {
    asking: bool,
    /// The last failure, while it is the last word.
    failure: Option<&'a Failure>,
    fetched_ago: Option<std::time::Duration>,
    source: Source,
    outdated: bool,
}

/// The Refresh row's detail: asking; why the last try failed and how old
/// the figures kept are; that the figures are outdated; or where they
/// came from and how old they are.
fn refresh_detail(ui: crate::i18n::Ui, shown: &Shown) -> String {
    match shown.failure {
        _ if shown.asking => ui.text(Text::UsageAsking).to_owned(),
        Some(Failure::RateLimited(_)) => ui.usage_rate_limited(shown.fetched_ago),
        Some(_) => ui.usage_failed(shown.fetched_ago),
        None => match shown.fetched_ago {
            None => String::new(),
            Some(ago) if shown.outdated => ui.usage_outdated(ago),
            Some(ago) => match shown.source {
                Source::Network => ui.usage_updated(ago),
                Source::Session => ui.usage_from_session(ago),
            },
        },
    }
}

/// How long until a window resets, `None` when it gives no time or the
/// time has passed — a past reset is not drawn as one a minute away.
fn resets_left(
    at: Option<time::OffsetDateTime>,
    now: time::OffsetDateTime,
) -> Option<std::time::Duration> {
    let left = at? - now;
    left.is_positive()
        .then(|| std::time::Duration::from_secs(left.whole_seconds().max(0) as u64))
}

/// Keep the trigger above its own fullscreen dismiss layer, in its normal
/// layout slot. The card remains below it and all outside clicks stay blocked.
pub(crate) fn usage_chip_layer(chip: gpui::Stateful<gpui::Div>, open: bool) -> gpui::AnyElement {
    if open {
        deferred(chip.occlude()).priority(2).into_any_element()
    } else {
        chip.into_any_element()
    }
}

impl Root {
    /// The chip: the agent in front's mark, a disc when its lowest share
    /// is under half, and the share — or `-` when that agent has none.
    /// Always available, including before the first answer and after errors.
    pub(crate) fn render_usage_chip(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        usage_chip_layer(self.usage_chip_content(cx), self.showing_usage)
    }

    pub(crate) fn usage_chip_content(&self, cx: &mut Context<Root>) -> gpui::Stateful<gpui::Div> {
        let front = self.usage_front();
        let lowest = self
            .usage
            .of(front)
            .and_then(|provider| usage::lowest(provider.windows()));
        let theme = self.theme;
        let at = self.usage_at.clone();
        let open = self.showing_usage;
        let ui = self.ui();
        let hint = format!(
            "{} · {}",
            crate::view::kind::default_tab_label(ui, front.mark()),
            ui.text(Text::ActionBarUsage)
        );
        usage_chip_frame(theme, open, lowest.is_none())
            .child(crate::view::kind::harness_mark(
                front.mark(),
                theme,
                theme::size::META,
                crate::view::kind::Shade::Full,
            ))
            .when_some(
                lowest.and_then(|left| disc(usage::level(left), theme)),
                |chip, colour| {
                    chip.child(
                        div()
                            .size(theme::space::DISC)
                            .rounded_full()
                            .bg(rgb(colour)),
                    )
                },
            )
            .child(match lowest {
                Some(left) => div().child(format!("{left}%")),
                None => div().child("-"),
            })
            // Where it ended up, for the card that hangs under it.
            .relative()
            .child(
                gpui::canvas(move |bounds, _window, _cx| at.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .when(!open, |chip| {
                chip.hint_delay().tooltip(move |_, app| {
                    app.new(|_| {
                        super::tooltip::Tooltip::for_action(
                            hint.clone(),
                            crate::actions::keymap::Named::ToggleUsage,
                            theme,
                        )
                    })
                    .into()
                })
            })
            // A title bar's own click is the zoom gesture.
            .on_click(cx.listener(|root, _event, _window, cx| {
                cx.stop_propagation();
                root.toggle_usage(cx);
            }))
    }

    /// The card, when it is up: a layer that closes it, and the card
    /// hanging under the chip's right edge.
    pub(crate) fn render_usage(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        if !self.showing_usage {
            return None;
        }
        let theme = self.theme;
        let ui = self.ui();
        let viewport = self.viewport();
        let at = self.usage_at.get();
        let now = time::OffsetDateTime::now_utc();

        let backdrop = deferred(
            anchored().position(gpui::Point::default()).child(
                div()
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _event, _window, cx| {
                            cx.stop_propagation();
                            root.showing_usage = false;
                            cx.notify();
                        }),
                    ),
            ),
        );

        // A body-sized provider head with its harness mark.
        let head = |mark: guild_session::DefaultTab, name: &'static str, detail: Option<String>| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(theme::space::MENU_ICON_GAP)
                .h(theme::space::MENU_ROW)
                .px(theme::space::MENU_INSET)
                .text_size(theme::size::BODY)
                .text_color(rgb(theme.text.primary))
                .child(crate::view::kind::harness_mark(
                    mark,
                    theme,
                    theme::size::BODY,
                    crate::view::kind::Shade::Full,
                ))
                .child(div().flex_1().child(name))
                .children(detail.map(|detail| {
                    div()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(detail)
                }))
        };

        let front = self.usage_front();
        // Why a provider shows no windows, or `None` when it has some.
        let reason = |agent: Agent| -> Option<String> {
            let Some(provider) = self.usage.of(agent) else {
                return Some(ui.text(Text::UsageNothingToRead).to_owned());
            };
            if !provider.windows().is_empty() {
                return None;
            }
            let text = reason_text(provider.failure(), provider.answered());
            Some(ui.text(text).to_owned())
        };
        let name = |agent: Agent| crate::view::kind::default_tab_label(ui, agent.mark());

        let mut card = usage_body().child(
            head(front.mark(), name(front), None)
                .debug_selector(move || format!("usage-primary-{}", name(front))),
        );

        // The agent in front, in full — M12.10's rows.
        match reason(front) {
            Some(reason) => {
                card = card.child(
                    div()
                        .flex_none()
                        .px(theme::space::MENU_INSET)
                        .pt(px(6.0))
                        .pb(theme::space::CARD_GAP)
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(reason),
                );
            }
            None => {
                for window in self.usage.shown(front).windows() {
                    card = card.child(usage_window(window, ui, theme, now));
                }
            }
        }

        // All providers: a fold, open until pressed.
        let all_open = self.usage_all_open;
        card = card.child(popup::separator(theme)).child(
            div()
                .id("usage-all")
                .debug_selector(|| "usage-all".into())
                .flex_none()
                .text_color(rgb(theme.text.secondary))
                .flex()
                .items_center()
                .gap(theme::space::MENU_ICON_GAP)
                .h(theme::space::MENU_ROW)
                .mx(theme::radius::ROW)
                .px(popup::row_inset())
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.card)))
                .child(div().flex_1().child(ui.text(Text::UsageAllProviders)))
                .child(popup::icon(
                    if all_open {
                        "ui/chevron-up.svg"
                    } else {
                        "ui/chevron-down.svg"
                    },
                    false,
                    theme,
                ))
                .on_click(cx.listener(|root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.usage_all_open = !root.usage_all_open;
                    cx.notify();
                })),
        );
        if all_open {
            for kind in guild_session::HarnessKind::ALL {
                let agent = Agent::from_harness(kind);
                if agent == front {
                    continue;
                }
                card = card.child(
                    head(agent.mark(), name(agent), reason(agent))
                        .debug_selector(move || format!("usage-secondary-{}", name(agent))),
                );
                let windows = self
                    .usage
                    .of(agent)
                    .map(|provider| provider.windows())
                    .unwrap_or_default();
                for window in windows {
                    card = card.child(usage_window(window, ui, theme, now));
                }
            }
        }
        let mut frame = usage_frame(theme, viewport.height)
            .child(card)
            .child(popup::separator(theme));

        let shown = self.usage.shown(front);
        let detail = refresh_detail(
            ui,
            &Shown {
                asking: shown.asking(),
                failure: shown.failure(),
                fetched_ago: shown.fetched_ago(),
                source: shown.source(),
                outdated: shown.outdated(now),
            },
        );
        frame = frame.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(theme::space::MENU_ICON_GAP)
                .h(theme::space::MENU_ROW)
                .px(theme::space::MENU_INSET)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(detail),
                )
                .child(
                    div()
                        .id("usage-refresh")
                        .flex_none()
                        .cursor_pointer()
                        .text_color(rgb(theme.text.secondary))
                        .hover(|label| label.text_color(rgb(theme.text.primary)))
                        .child(ui.text(Text::UsageRefresh))
                        .on_click(cx.listener(|root, _event, _window, cx| {
                            cx.stop_propagation();
                            root.ask_usage();
                            cx.notify();
                        })),
                ),
        );

        let list = deferred(
            anchored()
                .anchor(gpui::Anchor::TopRight)
                .position(gpui::point(at.right(), at.bottom() + px(4.0)))
                .snap_to_window_with_margin(px(theme::space::USAGE_WINDOW_MARGIN_PX))
                .child(frame.occlude()),
        )
        .priority(1);
        Some(div().child(backdrop).child(list).into_any_element())
    }
}

/// What the card says about a provider's last try — the installed Guild
/// said *couldn't read* for every 429 from 2026-10-01, which sent the
/// person looking for a parsing bug that was not there.
#[cfg(test)]
mod tests {
    mod regression;

    use super::*;
    use crate::i18n::{Language, Ui};

    #[test]
    fn a_rate_limited_provider_says_so_rather_than_could_not_read() {
        let limited = Failure::RateLimited(None);
        assert_eq!(reason_text(Some(&limited), true), Text::UsageRateLimited);
        assert_eq!(
            reason_text(Some(&Failure::Request("http status: 500".into())), true),
            Text::UsageCouldNotRead
        );
        assert_eq!(
            reason_text(Some(&Failure::NoCredentials), false),
            Text::UsageNotSignedIn
        );
        assert_eq!(reason_text(None, true), Text::UsageNothingToRead);
        assert_eq!(reason_text(None, false), Text::UsageAsking);
    }

    #[test]
    fn usage_card_bounds_scroll_content_and_keeps_footer_space() {
        let mut frame = usage_frame(Theme::dark(), px(550.0));
        assert_eq!(frame.style().max_size.height, Some(px(534.0).into()));
        let mut body = usage_body();
        assert_eq!(body.style().overflow.y, Some(gpui::Overflow::Scroll));
        assert_eq!(body.style().min_size.height, Some(px(0.0).into()));
    }

    #[test]
    fn usage_bar_is_thick_and_fully_rounded() {
        let mut track = usage_bar(86, Theme::dark());
        let mut rounded = div().rounded_full();
        assert_eq!(track.style().size.height, Some(theme::from_px(5.0).into()));
        assert_eq!(track.style().corner_radii, rounded.style().corner_radii);
    }

    #[test]
    fn usage_card_colours_follow_remaining_share_boundaries() {
        for theme in Theme::all() {
            for (left, expected) in [
                (100, theme.accent.green),
                (86, theme.accent.green),
                (50, theme.accent.green),
                (49, theme.accent.brass),
                (35, theme.accent.brass),
                (20, theme.accent.brass),
                (19, theme.accent.rust),
                (10, theme.accent.rust),
                (0, theme.accent.rust),
            ] {
                assert_eq!(
                    bar(usage::level(left), theme),
                    expected,
                    "{}% in {:?}",
                    left,
                    theme.kind
                );
            }
            assert_eq!(
                disc(Level::Plenty, theme),
                None,
                "the chip keeps its quiet plenty state"
            );
        }
    }

    /// The figures as the network gave them, `ago`, with nothing wrong.
    fn shown(ago: Option<std::time::Duration>) -> Shown<'static> {
        Shown {
            asking: false,
            failure: None,
            fetched_ago: ago,
            source: Source::Network,
            outdated: false,
        }
    }

    #[test]
    fn the_refresh_row_says_a_rate_limit_beside_the_figures_it_kept() {
        let ui = Ui(Language::English);
        let ago = Some(std::time::Duration::from_secs(12 * 60));
        let limited = Failure::RateLimited(None);
        assert_eq!(
            refresh_detail(
                ui,
                &Shown {
                    failure: Some(&limited),
                    ..shown(ago)
                }
            ),
            "rate limited · from 12m ago"
        );
        assert_eq!(
            refresh_detail(
                ui,
                &Shown {
                    failure: Some(&Failure::Unreadable),
                    ..shown(ago)
                }
            ),
            "failed · from 12m ago"
        );
        assert_eq!(
            refresh_detail(
                ui,
                &Shown {
                    asking: true,
                    failure: Some(&limited),
                    ..shown(ago)
                }
            ),
            "asking…"
        );
        assert_eq!(refresh_detail(ui, &shown(ago)), "updated 12m ago");
        assert_eq!(
            refresh_detail(
                Ui(Language::Russian),
                &Shown {
                    failure: Some(&limited),
                    ..shown(None)
                }
            ),
            "запросы ограничены"
        );
    }

    /// The figures say where they came from and how old they are — a
    /// session's are not the network's, and old ones are called old.
    #[test]
    fn the_refresh_row_says_where_the_figures_came_from_and_when() {
        let ui = Ui(Language::English);
        let two = Some(std::time::Duration::from_secs(2 * 60));
        let session = Shown {
            source: Source::Session,
            ..shown(two)
        };
        assert_eq!(refresh_detail(ui, &session), "from a session · 2m ago");
        assert_eq!(
            refresh_detail(
                ui,
                &Shown {
                    fetched_ago: Some(std::time::Duration::from_secs(5)),
                    ..session
                }
            ),
            "from a session · just now"
        );
        let forty = Some(std::time::Duration::from_secs(40 * 60));
        for source in [Source::Session, Source::Network] {
            assert_eq!(
                refresh_detail(
                    ui,
                    &Shown {
                        source,
                        outdated: true,
                        ..shown(forty)
                    }
                ),
                "outdated · from 40m ago"
            );
        }
        assert_eq!(
            refresh_detail(Ui(Language::Russian), &session),
            "из сессии · 2м назад"
        );
        assert_eq!(
            refresh_detail(
                Ui(Language::Russian),
                &Shown {
                    outdated: true,
                    ..shown(forty)
                }
            ),
            "устарело · данные 40м назад"
        );
    }

    /// A reset already past is not *resets in 1m*: the share it would
    /// stand beside is not what it says any more.
    #[test]
    fn a_reset_already_past_is_not_drawn() {
        let now = time::macros::datetime!(2026-10-03 12:00:00 UTC);
        assert_eq!(
            resets_left(Some(now - time::Duration::minutes(1)), now),
            None
        );
        assert_eq!(resets_left(Some(now), now), None);
        assert_eq!(
            resets_left(Some(now + time::Duration::minutes(90)), now),
            Some(std::time::Duration::from_secs(90 * 60))
        );
        assert_eq!(resets_left(None, now), None);
    }
}
