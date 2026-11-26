//! What a control says about itself when the pointer rests on it.
//!
//! GPUI takes any view here and ships none, so this is ours: a label and,
//! when there is one, the shortcut that does the same thing. Three icon
//! buttons appeared in the tab strip with no words at all, and the only way
//! to learn what they did was the keymap screen.
//!
//! The shortcut is part of the tooltip rather than a separate discovery.
//! Somebody who reaches for the mouse twice is somebody who would rather
//! have pressed a key, and this is where they find out they could have.

use gpui::prelude::*;
use gpui::{Context, IntoElement, Render, Window, div, px, rgb};

use crate::actions::keymap::Named;
use crate::theme::Theme;
use gpui_component::ActiveTheme as _;

/// Public interaction adapter for Guild-owned hints, including component buttons.
pub(crate) trait Hint: gpui::InteractiveElement {
    fn hint_delay(mut self) -> Self {
        self.interactivity()
            .tooltip_show_delay(crate::theme::tooltip::SHOW_DELAY);
        self
    }

    fn hint(mut self, text: impl Into<String>, palette: Theme) -> Self {
        let text = text.into();
        self.interactivity()
            .tooltip(move |_, cx| cx.new(|_| Tooltip::text(text.clone(), palette)).into());
        self.hint_delay()
    }
}
impl<T: gpui::InteractiveElement> Hint for T {}

/// A label, and the stroke that does the same thing.
pub struct Tooltip {
    text: String,
    stroke: Option<String>,
    theme: Theme,
    details: bool,
    worktree: bool,
}

impl Tooltip {
    /// A tooltip naming an action, with whatever stroke is bound to it now.
    ///
    /// Now, rather than the default: a person who rebound it is exactly the
    /// person who would be misled by being told the default.
    pub fn for_action(text: impl Into<String>, action: Named, theme: Theme) -> Self {
        Self {
            text: text.into().replace(['\r', '\n'], " "),
            stroke: crate::actions::stroke_for(action),
            theme,
            details: false,
            worktree: false,
        }
    }
}

impl Tooltip {
    /// Multi-line informational content is not a compact control hint.
    pub fn details(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            text: text.into(),
            stroke: None,
            theme,
            details: true,
            worktree: false,
        }
    }

    /// A tooltip that only says something — why a control is disabled,
    /// which no stroke would fix.
    pub fn text(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            text: text.into().replace(['\r', '\n'], " "),
            stroke: None,
            theme,
            details: false,
            worktree: false,
        }
    }
}

impl Tooltip {
    /// Anchored worktree controls use the same Guild ink and current keymap.
    pub(crate) fn worktree(text: String, action: Option<Named>, theme: Theme) -> Self {
        let mut tooltip = match action {
            Some(action) => Self::for_action(text, action, theme),
            None => Self::text(text, theme),
        };
        tooltip.worktree = true;
        tooltip
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;

        div()
            .id("guild-tooltip")
            .debug_selector(|| "guild-control-tooltip".into())
            .flex()
            .items_center()
            .flex_none()
            .w_auto()
            .h(crate::theme::tooltip::HEIGHT)
            .whitespace_nowrap()
            .gap(crate::theme::tooltip::GAP)
            .px(crate::theme::tooltip::INSET)
            .border_1()
            .border_color(crate::theme::card_outline(theme))
            .rounded(crate::theme::radius::ROW)
            .bg(rgb(theme.surface.raised))
            .text_size(crate::theme::size::BODY)
            .when(self.details, |tip| {
                tip.h_auto()
                    .whitespace_normal()
                    .px(crate::theme::space::CARD_PADDING)
                    .py(px(5.0))
                    .text_size(crate::theme::size::META)
            })
            .when(self.worktree, |tip| {
                tip.debug_selector(|| "tooltip-popup".into())
                    .role(gpui::Role::Tooltip)
                    .h_auto()
                    .py(px(5.0))
                    .gap(crate::theme::space::CARD_GAP)
                    .text_size(crate::theme::size::META)
                    .mb(crate::theme::space::WORKTREE_HINT_GAP)
                    .px(crate::theme::space::CARD_GAP)
                    .line_height(crate::theme::space::WORKTREE_HINT_LINE)
                    .bg(rgb(theme.surface.panel))
                    .border_1()
                    .border_color(rgb(theme.text.gutter))
                    .rounded(crate::theme::radius::STATUS_BUTTON)
            })
            // GPUI lays tooltips out as roots, outside the triggering
            // control's inherited font family.
            .font_family(cx.theme().font_family.clone())
            .text_color(rgb(theme.text.primary))
            .child(self.text.clone())
            .when_some(self.stroke.clone(), |tip, stroke| {
                tip.child(
                    div()
                        .text_color(rgb(theme.text.muted))
                        .child(crate::actions::printed(&stroke)),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, size};

    struct Host(gpui::Entity<Tooltip>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().items_start().child(self.0.clone())
        }
    }

    #[gpui::test]
    fn control_hints_keep_one_line_and_content_width(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let mut tip = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let view = cx.new(|_| Tooltip::text("Open In…", Theme::of(crate::theme::Kind::Dark)));
            tip = Some(view.clone());
            gpui_component::Root::new(cx.new(|_| Host(view)), window, cx)
        });
        let tip = tip.unwrap();
        cx.simulate_resize(size(px(1000.), px(200.)));
        let mut widths = Vec::new();
        for (label, stroke) in [
            ("Open In…", None),
            ("Commands", Some("cmd-shift-k")),
            ("Открыть выбранный файл в другом приложении", None),
        ] {
            tip.update(cx, |tip, cx| {
                tip.text = label.into();
                tip.stroke = stroke.map(str::to_owned);
                cx.notify();
            });
            cx.run_until_parked();
            let bounds = cx.debug_bounds("guild-control-tooltip").unwrap();
            assert_eq!(bounds.size.height, px(28.), "single-line hint: {label}");
            widths.push(bounds.size.width);
        }
        assert!(
            widths[1] > widths[0],
            "shortcut contributes to content width"
        );
        assert!(widths[2] > widths[1], "long localized label grows width");
        tip.update(cx, |tip, cx| {
            let diagnostic = grep_regex::RegexMatcher::new("a(")
                .err()
                .unwrap()
                .to_string();
            assert!(diagnostic.contains('\n') && diagnostic.contains('^'));
            *tip = Tooltip::details(diagnostic.clone(), tip.theme);
            assert_eq!(
                tip.text, diagnostic,
                "retain line breaks and caret indentation"
            );
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("guild-control-tooltip")
                .unwrap()
                .size
                .height
                > px(28.),
            "multi-line regex diagnostics retain their layout"
        );
    }
}

#[cfg(test)]
mod dock_shortcut_tests {
    use super::*;
    use guild_session::Dock;

    #[test]
    fn isolated_dock_hide_shortcuts_follow_current_remapping() {
        let state = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "view::tooltip::dock_shortcut_tests::dock_hide_shortcut_fixture",
                "--ignored",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_TOOLTIP_KEYMAP_TEST", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    #[ignore = "isolated_dock_hide_shortcuts_follow_current_remapping owns the keymap"]
    fn dock_hide_shortcut_fixture() {
        assert_eq!(
            std::env::var("GUILD_TOOLTIP_KEYMAP_TEST").as_deref(),
            Ok("1")
        );
        let theme = Theme::of(crate::theme::Kind::Dark);
        assert_eq!(
            Tooltip::for_action("Open", Named::OpenSelectedApp, theme)
                .stroke
                .as_deref(),
            Some("cmd-o")
        );
        assert_eq!(
            Tooltip::for_action("Choose", Named::SelectOpenApp, theme)
                .stroke
                .as_deref(),
            Some("cmd-shift-o")
        );
        crate::actions::keymap::save(
            &[
                (Named::AddProject, "cmd-o".into()),
                (Named::SelectOpenApp, "cmd-alt-7".into()),
            ]
            .into(),
        )
        .unwrap();
        assert_eq!(
            Tooltip::for_action("Open", Named::OpenSelectedApp, theme).stroke,
            None
        );
        assert_eq!(
            Tooltip::for_action("Choose", Named::SelectOpenApp, theme)
                .stroke
                .as_deref(),
            Some("cmd-alt-7")
        );
        let keymap = gpui::Keymap::new(crate::actions::ordered(crate::actions::effective(), &[]));
        let contexts = [gpui::KeyContext::parse("guild").unwrap()];
        for (stroke, action) in [
            ("cmd-o", Named::AddProject),
            ("cmd-alt-7", Named::SelectOpenApp),
        ] {
            let actual = keymap
                .bindings_for_input(&[gpui::Keystroke::parse(stroke).unwrap()], &contexts)
                .0
                .into_iter()
                .next()
                .unwrap();
            assert_eq!(actual.action().name(), action.action().name());
        }
        crate::actions::keymap::save(&Default::default()).unwrap();
        let bindings = [
            (Dock::Left, Named::ToggleLeftPanel, "cmd-b", "cmd-alt-b"),
            (Dock::Right, Named::ToggleRightPanel, "cmd-l", "cmd-alt-l"),
            (Dock::Bottom, Named::ToggleBottomPanel, "cmd-j", "cmd-alt-j"),
        ];
        for (dock, action, default, _) in bindings {
            assert_eq!(crate::view::panels::dock_toggle_action(dock), action);
            assert_eq!(
                Tooltip::for_action("Hide", action, theme).stroke.as_deref(),
                Some(default)
            );
        }
        crate::actions::keymap::save(
            &bindings
                .iter()
                .map(|(_, action, _, custom)| (*action, custom.to_string()))
                .collect(),
        )
        .unwrap();
        for (dock, _, _, custom) in bindings {
            let hint =
                Tooltip::for_action("Hide", crate::view::panels::dock_toggle_action(dock), theme);
            assert_eq!(
                hint.stroke.as_deref(),
                Some(custom),
                "dock hint follows the stored remapping"
            );
        }
        crate::actions::keymap::save(&Default::default()).unwrap();
        let toggles = crate::actions::keymap::parse(
            r#"{"ctrl-cmd-j":"ToggleTerminalJobs","ctrl-cmd-u":"ToggleUsage"}"#,
        )
        .unwrap();
        for (stroke, action) in &toggles {
            assert_eq!(
                Tooltip::for_action("provider · Usage", *action, theme)
                    .stroke
                    .as_ref(),
                Some(stroke)
            );
        }
        crate::actions::keymap::save(
            &toggles
                .iter()
                .map(|(stroke, action)| (*action, stroke.replace("ctrl-cmd-", "ctrl-cmd-alt-")))
                .collect(),
        )
        .unwrap();
        for (stroke, action) in toggles {
            let hint = Tooltip::for_action("provider · Usage", action, theme);
            assert_eq!(hint.text, "provider · Usage");
            assert_eq!(
                hint.stroke,
                Some(stroke.replace("ctrl-cmd-", "ctrl-cmd-alt-"))
            );
        }
    }
}
