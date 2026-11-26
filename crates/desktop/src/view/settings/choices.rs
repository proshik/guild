//! Preference menus use the same keyboard and placement engine as the rest of Guild.
use crate::i18n::Text;
use crate::view::kind::{Shade, default_tab_label, harness_mark};
use crate::view::menu::{Anchor, Choice, Deed, Entry, Label, Mark, Target};
use crate::{Root, theme};
use gpui::prelude::*;
use gpui::{Context, div, rgb};
use guild_session::{DefaultTab, DiffView, Language, PermissionMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Network(guild_session::network::Mode),
    HarnessNetwork(
        guild_session::HarnessKind,
        guild_session::network::AgentRoute,
    ),
    Language(Language),
    Tab(DefaultTab),
    Permission(PermissionMode),
    Diff(DiffView),
    Markdown(guild_session::markdown::Mode),
    UpdateSource(guild_session::update::Source),
    UpdateChannel(guild_session::update::Channel),
}
impl Value {
    pub(crate) fn alternatives(self) -> Vec<Self> {
        match self {
            Self::HarnessNetwork(harness, _) => [
                guild_session::network::AgentRoute::Follow,
                guild_session::network::AgentRoute::Proxy,
                guild_session::network::AgentRoute::Direct,
            ]
            .into_iter()
            .map(|route| Self::HarnessNetwork(harness, route))
            .collect(),
            Self::Network(_) => vec![
                Self::Network(guild_session::network::Mode::System),
                Self::Network(guild_session::network::Mode::Custom),
            ],
            Self::UpdateSource(_) => [
                guild_session::update::Source::Remote,
                guild_session::update::Source::Local,
            ]
            .into_iter()
            .map(Self::UpdateSource)
            .collect(),
            Self::UpdateChannel(_) => [
                guild_session::update::Channel::Nightly,
                guild_session::update::Channel::Stable,
            ]
            .into_iter()
            .map(Self::UpdateChannel)
            .collect(),
            Self::Language(_) => Language::ALL.iter().copied().map(Self::Language).collect(),
            Self::Tab(_) => DefaultTab::ALL.into_iter().map(Self::Tab).collect(),
            Self::Permission(_) => [
                PermissionMode::Manual,
                PermissionMode::AcceptEdits,
                PermissionMode::Auto,
                PermissionMode::Plan,
                PermissionMode::DontAsk,
                PermissionMode::BypassPermissions,
            ]
            .into_iter()
            .map(Self::Permission)
            .collect(),
            Self::Markdown(_) => [
                guild_session::markdown::Mode::Editor,
                guild_session::markdown::Mode::Split,
                guild_session::markdown::Mode::Preview,
            ]
            .into_iter()
            .map(Self::Markdown)
            .collect(),
            Self::Diff(_) => [DiffView::SideBySide, DiffView::Unified]
                .into_iter()
                .map(Self::Diff)
                .collect(),
        }
    }
    fn label(self, ui: crate::i18n::Ui) -> &'static str {
        match self {
            Self::HarnessNetwork(_, route) => ui.text(match route {
                guild_session::network::AgentRoute::Follow => Text::NetworkFollow,
                guild_session::network::AgentRoute::Proxy => Text::NetworkUseProxy,
                guild_session::network::AgentRoute::Direct => Text::NetworkDirect,
                guild_session::network::AgentRoute::Unsupported => Text::NetworkInvalid,
            }),
            Self::Network(value) => ui.text(if value == guild_session::network::Mode::System {
                Text::NetworkSystem
            } else {
                Text::NetworkCustom
            }),
            Self::UpdateSource(value) => match value {
                guild_session::update::Source::Remote => "Remote",
                guild_session::update::Source::Local => "Local",
            },
            Self::UpdateChannel(value) => match value {
                guild_session::update::Channel::Nightly => "Nightly",
                guild_session::update::Channel::Stable => "Stable",
            },
            Self::Language(value) => value.endonym(),
            Self::Tab(value) => default_tab_label(ui, value),
            Self::Permission(value) => value.as_cli_value(),
            Self::Markdown(value) => ui.text(match value {
                guild_session::markdown::Mode::Editor => Text::MarkdownEditor,
                guild_session::markdown::Mode::Split => Text::MarkdownSplit,
                guild_session::markdown::Mode::Preview => Text::MarkdownPreview,
            }),
            Self::Diff(value) => ui.text(match value {
                DiffView::SideBySide => Text::DiffSideBySide,
                DiffView::Unified => Text::DiffUnified,
            }),
        }
    }
}
impl Root {
    pub(crate) fn settings_choices(&self, selected: Value) -> Vec<Entry> {
        selected
            .alternatives()
            .into_iter()
            .map(|value| {
                Entry::Choice(Choice {
                    label: Label::Named(value.label(self.ui()).to_owned()),
                    mark: match value {
                        Value::Tab(tab) => Mark::Harness(tab),
                        _ => Mark::None,
                    },
                    deed: Deed::Setting(value),
                    check: Some(value == selected),
                    enabled: !matches!(value, Value::UpdateSource(_) | Value::UpdateChannel(_))
                        || self.update_source_editable(),
                    detail: None,
                    trailing: None,
                })
            })
            .collect()
    }
    pub(crate) fn take_setting(&mut self, value: Value, cx: &mut Context<Self>) {
        match value {
            Value::Network(value) => self.settings.network.mode = value,
            Value::HarnessNetwork(harness, route) => {
                self.settings
                    .network
                    .agents
                    .insert(harness.program().into(), route);
            }
            Value::UpdateSource(value) => {
                let mut preferences = self.update_preferences();
                preferences.source = value;
                self.set_update_preferences(preferences, cx);
                return;
            }
            Value::UpdateChannel(value) => {
                let mut preferences = self.update_preferences();
                preferences.channel = value;
                self.set_update_preferences(preferences, cx);
                return;
            }
            Value::Language(value) => {
                if self.settings.language != value {
                    self.cycle_language(cx);
                }
            }
            Value::Tab(value) => self.settings.default_tab = value,
            Value::Permission(value) => self.settings.permission_mode = value,
            Value::Diff(value) => self.settings.diff_view = value,
            Value::Markdown(value) => self.settings.markdown_default_view = value,
        }
        self.store_settings();
        cx.notify();
    }
    pub(crate) fn settings_choice(
        &self,
        id: &'static str,
        value: Value,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let at = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let measured = at.clone();
        let theme = self.theme;
        super::sections::setting_button(theme, id, String::new(), false)
            .debug_selector(move || id.into())
            .relative()
            .gap(theme::space::CARD_GAP)
            .when_some(
                match value {
                    Value::Tab(tab) => Some(tab),
                    _ => None,
                },
                |button, tab| {
                    button.child(harness_mark(tab, theme, theme::size::BODY, Shade::Full))
                },
            )
            .child(div().flex_1().child(value.label(self.ui())))
            .child(crate::icons::chevron(true).text_color(rgb(theme.text.secondary)))
            .child(
                gpui::canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_click(cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                root.open_menu(Target::Setting(value), Anchor::Below(at.get()), window, cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext, px};
    use guild_session::{Settings, markdown::Mode};

    #[test]
    fn isolated_per_harness_network_choices() {
        let state = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "view::settings::choices::tests::real_per_harness_network_choices",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_NETWORK_CHOICES_TEST", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let saved: Settings =
            serde_json::from_slice(&std::fs::read(state.path().join("settings.json")).unwrap())
                .unwrap();
        for h in guild_session::HarnessKind::ALL {
            assert_eq!(
                saved.network.agent_route(h),
                guild_session::network::AgentRoute::Direct
            );
        }
    }
    #[gpui::test]
    #[ignore = "run through isolated_per_harness_network_choices"]
    fn real_per_harness_network_choices(cx: &mut TestAppContext) {
        use guild_session::{HarnessKind, network::AgentRoute};
        assert_eq!(
            std::env::var("GUILD_NETWORK_CHOICES_TEST").as_deref(),
            Ok("1")
        );
        cx.executor().allow_parking();
        let state = crate::paths::state_dir();
        cx.update(|cx| {
            crate::editor::init(cx);
            let root = cx.new(|cx| Root::diagnostic(Default::default(), state.clone(), cx));
            root.update(cx, |root, cx| {
                for language in Language::ALL {
                    root.settings.language = *language;
                    root.settings_query.clear();
                    assert_eq!(root.network_harnesses(), HarnessKind::ALL);
                    for harness in HarnessKind::ALL {
                        let value = Value::HarnessNetwork(harness, AgentRoute::Follow);
                        assert_eq!(root.settings_choices(value).len(), 3);
                        root.settings_query = harness.program().into();
                        assert_eq!(root.network_harnesses(), vec![harness]);
                        for route in [AgentRoute::Follow, AgentRoute::Proxy, AgentRoute::Direct] {
                            root.take_setting(Value::HarnessNetwork(harness, route), cx);
                            assert_eq!(root.settings.network.agent_route(harness), route);
                        }
                    }
                }
            });
        });
        guild_session::flush();
    }
    // The real choice handler persists globally. A subprocess gives it its own
    // paths cache and store worker, leaving the person's settings untouched.
    #[test]
    fn isolated_markdown_default_view_choices() {
        let state = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "view::settings::choices::tests::real_markdown_default_view_choices",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_MARKDOWN_SETTINGS_TEST", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let saved: Settings =
            serde_json::from_slice(&std::fs::read(state.path().join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(
            saved.markdown_default_view,
            Mode::Preview,
            "the ignored child must execute and persist its final choice inside wrapper-owned state"
        );
    }

    #[gpui::test]
    #[ignore = "run through isolated_markdown_default_view_choices"]
    fn real_markdown_default_view_choices_select_and_persist(cx: &mut TestAppContext) {
        assert_eq!(
            std::env::var("GUILD_MARKDOWN_SETTINGS_TEST").as_deref(),
            Ok("1")
        );
        cx.executor().allow_parking();
        let state = crate::paths::state_dir();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let root = cx.new(|cx| {
                let mut root = Root::diagnostic(Default::default(), state.clone(), cx);
                root.isolated_render = true;
                root.settings_page = Some(super::super::Section::Editor);
                root
            });
            made = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1100.), px(900.)));
        let root = made.unwrap();
        for (language, labels) in [
            (
                Language::English,
                ["Editor", "Editor and Preview", "Preview"],
            ),
            (
                Language::Russian,
                ["Редактор", "Редактор и просмотр", "Просмотр"],
            ),
        ] {
            root.update(cx, |root, cx| {
                root.settings.language = language;
                root.settings_query = root.ui().text(Text::MarkdownDefaultViewTitle).into();
                let rows = root.visible_settings(super::super::Section::General);
                assert_eq!(
                    rows.len(),
                    1,
                    "search finds the Editor setting from any category"
                );
                assert_eq!(rows[0].info().3, "markdown_default_view");
                let hint = root.setting_hint(rows[0]);
                assert!(hint.contains(if language == Language::English {
                    "newly opened"
                } else {
                    "вновь открываемых"
                }));
                let entries = root.settings_choices(Value::Markdown(Mode::Editor));
                let names: Vec<_> = entries
                    .iter()
                    .map(|e| match e {
                        Entry::Choice(c) => match &c.label {
                            Label::Named(s) => s.as_str(),
                            _ => panic!("localized label"),
                        },
                        _ => panic!("choice"),
                    })
                    .collect();
                assert_eq!(names, labels);
                cx.notify();
            });
            cx.run_until_parked();
            for (index, expected) in [Mode::Editor, Mode::Split, Mode::Preview]
                .into_iter()
                .enumerate()
            {
                let button = cx
                    .debug_bounds("setting-markdown-default-view")
                    .unwrap_or_else(|| {
                        panic!("rendered preference control: {language:?}, {index}")
                    });
                cx.simulate_click(button.center(), Default::default());
                cx.run_until_parked();
                let item = cx
                    .debug_bounds(["menu-item-0", "menu-item-1", "menu-item-2"][index])
                    .expect("rendered mode choice");
                cx.simulate_click(item.center(), Default::default());
                cx.run_until_parked();
                assert_eq!(
                    root.read_with(cx, |root, _| root.settings.markdown_default_view),
                    expected
                );
                guild_session::store::flush();
                let saved = guild_session::Store::<Settings>::new(crate::paths::settings_file())
                    .load()
                    .unwrap();
                assert_eq!(
                    saved.markdown_default_view, expected,
                    "real choice persists"
                );
            }
        }
        guild_session::store::flush();
    }
}
