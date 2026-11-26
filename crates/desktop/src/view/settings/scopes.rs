//! Workspace and repository settings, drawn through the existing native host.
use super::SettingsHost;
use super::sections::{navigation_button, setting_button};
use crate::root::repository_settings::{Edit, Field, RepoPage, Target};
use crate::{Root, i18n::Text, theme};
use gpui::{Context, SharedString, div, prelude::*, rgb};
use guild_session::repository_settings::LspPolicy;

/// A repository's letter tile in the list, in points: the room of the
/// 16-point icon a group's row has, so their names line up.
const LIST_TILE: f32 = 16.0;

fn card(root: &Root, title: String, rows: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme::space::CARD_INNER_GAP)
        .p(theme::space::CARD_PADDING)
        .rounded(theme::radius::CARD)
        .border_1()
        .border_color(theme::settings::outline(root.theme))
        .bg(theme::settings::card(root.theme))
        .child(
            div()
                .text_size(theme::size::META)
                .text_color(rgb(root.theme.text.tertiary))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title.to_uppercase()),
        )
        .children(rows)
        .into_any_element()
}
fn detail_row(
    root: &Root,
    label: String,
    hint: String,
    control: gpui::AnyElement,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(theme::space::CARD_INNER_GAP)
        .py(theme::space::CARD_PADDING)
        .border_t_1()
        .border_color(theme::settings::rule(root.theme))
        .child(
            div()
                .flex_1()
                .min_w(theme::settings::LABEL_MIN)
                .child(label)
                .when(!hint.is_empty(), |row| {
                    row.child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(root.theme.text.tertiary))
                            .child(hint),
                    )
                }),
        )
        .child(control)
        .into_any_element()
}
impl Root {
    pub(crate) fn scoped_navigation(
        &self,
        host: SettingsHost,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        let target = self.scoped_settings_target(host);
        rows.push(
            navigation_button(
                self.theme,
                SharedString::from(format!("{host:?}-global-lsp")),
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .child(
                        crate::icons::toolbar("ui/workspace-globe.svg")
                            .text_color(rgb(self.theme.text.secondary)),
                    )
                    .child(self.ui().text(Text::LspSettings)),
                target == Some(Target::GlobalLsp),
            )
            .justify_start()
            .on_click(
                cx.listener(move |r, _, _, cx| {
                    r.choose_scoped_settings(host, Target::GlobalLsp, cx)
                }),
            )
            .into_any_element(),
        );
        rows.push(
            div()
                .pt(theme::space::CARD_PADDING)
                .px(theme::space::CARD_GAP)
                .text_size(theme::size::META)
                .text_color(rgb(self.theme.text.tertiary))
                .child(self.ui().text(Text::ScopedWorkspaces).to_uppercase())
                .into_any_element(),
        );
        for workspace in self.desk.workspaces() {
            let id = workspace.id;
            let workspace_icon = workspace
                .icon
                .as_deref()
                .and_then(crate::icons::workspace_path);
            rows.push(
                navigation_button(
                    self.theme,
                    SharedString::from(format!("{host:?}-workspace-{id}")),
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::space::CARD_GAP)
                        .when_some(workspace_icon, |row, path| {
                            row.child(
                                crate::icons::toolbar(path)
                                    .text_color(rgb(self.theme.text.secondary)),
                            )
                        })
                        .when(workspace_icon.is_none(), |row| {
                            row.child(
                                div()
                                    .w(theme::space::ICON)
                                    .text_color(rgb(self.theme.accent.patina))
                                    .child(
                                        workspace
                                            .name
                                            .chars()
                                            .next()
                                            .map(|c| c.to_uppercase().to_string())
                                            .unwrap_or_default(),
                                    ),
                            )
                        })
                        .child(workspace.name.clone()),
                    target == Some(Target::Workspace(id)),
                )
                .justify_start()
                .on_click(cx.listener(move |r, _, _, cx| {
                    r.choose_scoped_settings(host, Target::Workspace(id), cx)
                }))
                .into_any_element(),
            );
            // The workspace's repositories, one row each, in its order; a
            // project group stands where its first member does, with its
            // members nested a step further under it — the column's tree.
            let mut seen = std::collections::HashSet::new();
            let mut repositories = Vec::new();
            for project in self.desk.projects_of(id) {
                let key = self
                    .repository_identity(project.id)
                    .map(|r| r.key.clone())
                    .or_else(|| project.repository.clone())
                    .unwrap_or_else(|| project.path.clone());
                if seen.insert(key) {
                    let group = workspace
                        .project_group_of(guild_session::project_group::repository_key(project))
                        .map(|group| (group.id, group.name.clone()));
                    repositories.push((project, group));
                }
            }
            let mut placed = std::collections::HashSet::new();
            for at in 0..repositories.len() {
                let (project, group) = &repositories[at];
                let Some((group_id, name)) = group else {
                    rows.push(self.scoped_repository_row(
                        host,
                        id,
                        project,
                        false,
                        target.as_ref(),
                        cx,
                    ));
                    continue;
                };
                if !placed.insert(*group_id) {
                    continue;
                }
                let members: Vec<_> = repositories[at..]
                    .iter()
                    .filter(|(_, other)| other.as_ref().map(|(g, _)| g) == Some(group_id))
                    .collect();
                rows.push(self.scoped_group_row(*group_id, name, members.len()));
                for (member, _) in members {
                    rows.push(self.scoped_repository_row(
                        host,
                        id,
                        member,
                        true,
                        target.as_ref(),
                        cx,
                    ));
                }
            }
        }
        rows
    }
    /// A project group's row in the list: its folder, name and count under the
    /// workspace, at a repository's step; it is a heading, not a page.
    fn scoped_group_row(
        &self,
        group: guild_session::project_group::ProjectGroupId,
        name: &str,
        count: usize,
    ) -> gpui::AnyElement {
        div()
            .debug_selector(move || format!("scoped-nav-group-{group}"))
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .min_h(theme::settings::CONTROL)
            .px(theme::space::CARD_GAP)
            .ml(theme::space::CARD_PADDING)
            .border_1()
            .border_color(gpui::transparent_black())
            .text_color(rgb(self.theme.text.tertiary))
            // As in the column: a folder and how many projects it holds.
            .child(
                crate::icons::toolbar("ui/workspace-folder.svg")
                    .text_color(rgb(self.theme.text.muted)),
            )
            .child(name.to_owned())
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.faint))
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    /// A repository's row in the list, under its workspace — and a step
    /// further under its project group, when it is in one.
    fn scoped_repository_row(
        &self,
        host: SettingsHost,
        id: guild_session::WorkspaceId,
        project: &guild_session::Project,
        grouped: bool,
        target: Option<&Target>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let project_id = project.id;
        let selected = matches!(target, Some(Target::Repository { workspace, project, .. }) if *workspace == id && *project == project_id);
        let label = self
            .repository_identity(project_id)
            .and_then(|r| r.main.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| project.name.clone());
        let key = format!("{host:?}-repo-{id}-{project_id}");
        let icon = self
            .repository_identity(project_id)
            .and_then(|i| self.settings.repository_settings.get(&i.key))
            .and_then(|p| p.icon.as_deref())
            .and_then(crate::icons::workspace_path);
        // The icon chosen for it, or the column's letter tile at an icon's
        // size — a project group's row has the folder.
        let mark = match icon {
            Some(path) => crate::icons::toolbar(path)
                .text_color(rgb(self.theme.text.secondary))
                .into_any_element(),
            None => crate::view::column::letter_tile(self.theme, &label, LIST_TILE)
                .debug_selector(move || format!("scoped-repository-tile-{project_id}"))
                .into_any_element(),
        };
        navigation_button(
            self.theme,
            SharedString::from(key.clone()),
            div()
                .flex()
                .items_center()
                .gap(theme::space::CARD_GAP)
                .child(mark)
                .child(label),
            selected,
        )
        .debug_selector(move || key.clone())
        .justify_start()
        .ml(if grouped {
            theme::space::CARD_PADDING * 2.
        } else {
            theme::space::CARD_PADDING
        })
        .on_click(cx.listener(move |r, _, _, cx| {
            r.choose_scoped_settings(
                host,
                Target::Repository {
                    workspace: id,
                    project: project_id,
                    page: RepoPage::Overview,
                },
                cx,
            )
        }))
        .into_any_element()
    }

    pub(crate) fn scoped_heading(&self, host: SettingsHost) -> Option<(String, String)> {
        let target = self.scoped_settings_target(host)?;
        match target {
            Target::Global(_) => None,
            Target::GlobalLsp => Some((
                self.ui().text(Text::LspSettings).into(),
                self.ui().text(Text::ScopedDefaults).into(),
            )),
            Target::Workspace(id) => Some((
                self.desk.workspace(id)?.name.clone(),
                self.ui().text(Text::ScopedWorkspace).into(),
            )),
            Target::Repository {
                workspace, project, ..
            } => Some((
                format!(
                    "{} / {}",
                    self.desk.workspace(workspace)?.name,
                    self.desk.project(project)?.name
                ),
                self.ui().text(Text::ScopedShared).into(),
            )),
        }
    }
    pub(crate) fn scoped_rows(
        &self,
        host: SettingsHost,
        cx: &mut Context<Self>,
    ) -> Option<Vec<gpui::AnyElement>> {
        let target = self.scoped_settings_target(host)?;
        if matches!(target, Target::Global(_)) {
            return None;
        }
        let mut rows = Vec::new();
        if let Some(error) = &self.scoped_settings.error {
            let retry = self.scoped_settings.retry.clone();
            rows.push(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(theme::space::CARD_GAP)
                    .text_color(rgb(self.theme.accent.rust))
                    .child(format!(
                        "{}: {error}",
                        self.ui().text(Text::NoticeSettingsNotSaved)
                    ))
                    .when_some(retry, |row, _write| {
                        row.child(
                            setting_button(
                                self.theme,
                                "scoped-retry",
                                self.ui().text(Text::ScopedRetry).into(),
                                false,
                            )
                            .debug_selector(|| "scoped-retry".into())
                            .on_click(
                                cx.listener(move |r, _, _, cx| r.retry_scoped_preference(cx)),
                            ),
                        )
                    })
                    .into_any_element(),
            );
        }
        match target {
            Target::Global(_) => {}
            Target::GlobalLsp => rows.push(card(
                self,
                self.ui().text(Text::LspSettings).into(),
                self.scoped_lsp_rows(None, cx),
            )),
            Target::Workspace(workspace) => {
                let ws = self.desk.workspace(workspace)?;
                rows.push(card(
                    self,
                    self.ui().text(Text::ScopedWorkspace).into(),
                    vec![
                        self.scoped_field(
                            host,
                            format!("workspace-{workspace}-name"),
                            self.ui().text(Text::ScopedName).into(),
                            ws.name.clone(),
                            Field::WorkspaceName(workspace),
                            cx,
                        ),
                        self.scoped_icon_control(
                            crate::view::workspaces::IconTarget::Workspace(workspace),
                            ws.icon.as_deref(),
                            "scoped-workspace-icon",
                            cx,
                        ),
                    ],
                ));
                let mut projects = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for project in self.desk.projects_of(workspace) {
                    let key = self
                        .repository_identity(project.id)
                        .map(|r| r.key.clone())
                        .unwrap_or_else(|| project.path.clone());
                    if !seen.insert(key) {
                        continue;
                    }
                    let name = project.name.clone();
                    let project = project.id;
                    let path = self
                        .desk
                        .project(project)?
                        .path
                        .to_string_lossy()
                        .into_owned();
                    projects.push(detail_row(
                        self,
                        name,
                        path,
                        setting_button(
                            self.theme,
                            SharedString::from(format!("scoped-project-{project}")),
                            self.ui().text(Text::ScopedOpenSettings).into(),
                            false,
                        )
                        .on_click(cx.listener(move |r, _, _, cx| {
                            r.choose_scoped_settings(
                                host,
                                Target::Repository {
                                    workspace,
                                    project,
                                    page: RepoPage::Overview,
                                },
                                cx,
                            )
                        }))
                        .into_any_element(),
                    ));
                }
                rows.push(card(
                    self,
                    self.ui().text(Text::ScopedProjects).into(),
                    projects,
                ));
                rows.push(card(
                    self,
                    self.ui().text(Text::ScopedDeleteWorkspace).into(),
                    vec![detail_row(
                        self,
                        self.ui().text(Text::ScopedWorkspace).into(),
                        self.ui().text(Text::ScopedDeleteHint).into(),
                        setting_button(
                            self.theme,
                            "scoped-delete-workspace",
                            self.ui().text(Text::ScopedDeleteWorkspace).into(),
                            false,
                        )
                        .self_end()
                        .text_color(rgb(self.theme.accent.rust))
                        .debug_selector(|| "scoped-delete-workspace".into())
                        .on_click(cx.listener(move |r, _, window, cx| {
                            r.delete_workspace_asking(workspace, window, cx)
                        }))
                        .into_any_element(),
                    )],
                ));
            }
            Target::Repository {
                workspace,
                project,
                page: _,
            } => {
                let Some(identity) = self.repository_identity(project) else {
                    let message = self
                        .scoped_settings
                        .identities
                        .get(&project)
                        .and_then(|r| r.as_ref().err())
                        .cloned()
                        .unwrap_or_else(|| self.ui().text(Text::ScopedIdentityPending).into());
                    rows.push(
                        div()
                            .child(message)
                            .child(
                                setting_button(
                                    self.theme,
                                    "scoped-identity-retry",
                                    self.ui().text(Text::ScopedRetry).into(),
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |r, _, _, cx| {
                                        r.scoped_settings.identities.remove(&project);
                                        r.resolve_project_identity(project, false, cx);
                                    },
                                )),
                            )
                            .into_any_element(),
                    );
                    return Some(rows);
                };
                let key = identity.key.clone();
                let prefs = self
                    .settings
                    .repository_settings
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                let main = identity
                    .main
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.ui().text(Text::ScopedNoMain).into());
                let mut overview = vec![detail_row(
                    self,
                    self.ui().text(Text::ScopedMainPath).into(),
                    String::new(),
                    div()
                        .w(theme::settings::CONTENT / 2.)
                        .min_w_0()
                        .text_size(theme::size::META)
                        .text_color(rgb(self.theme.text.secondary))
                        .child(main)
                        .into_any_element(),
                )];
                // The project group it is in, in this workspace — named,
                // as in the column; it is changed from the column's menu.
                if let Some(group) = self.desk.project(project).and_then(|root| {
                    self.desk
                        .workspace(workspace)?
                        .project_group_of(guild_session::project_group::repository_key(root))
                }) {
                    overview.push(
                        div()
                            .debug_selector(|| "scoped-repository-group".into())
                            .child(detail_row(
                                self,
                                self.ui().text(Text::ScopedGroup).into(),
                                String::new(),
                                div()
                                    .min_w_0()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(self.theme.text.secondary))
                                    .child(group.name.clone())
                                    .into_any_element(),
                            ))
                            .into_any_element(),
                    );
                }
                if identity.git && identity.main.is_some() {
                    overview.push(self.scoped_field(
                        host,
                        format!("repo-{project}-base"),
                        self.ui().text(Text::ScopedBase).into(),
                        prefs.worktree_base.clone().unwrap_or_default(),
                        Field::Base(key.clone()),
                        cx,
                    ));
                    overview.push(
                        self.scoped_field(
                            host,
                            format!("repo-{project}-directory"),
                            self.ui().text(Text::ScopedDirectory).into(),
                            prefs
                                .worktree_directory
                                .as_ref()
                                .map(|p| p.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            Field::Directory(key.clone()),
                            cx,
                        ),
                    );
                }
                overview.push(self.scoped_icon_control(
                    crate::view::workspaces::IconTarget::Repository(key.clone()),
                    prefs.icon.as_deref(),
                    "scoped-repository-icon",
                    cx,
                ));
                rows.push(card(
                    self,
                    self.ui().text(Text::ScopedRepository).into(),
                    overview,
                ));
                rows.push(card(
                    self,
                    self.ui().text(Text::LspSettings).into(),
                    self.scoped_lsp_rows(Some(project), cx),
                ));
            }
        }
        Some(rows)
    }
    fn scoped_icon_control(
        &self,
        target: crate::view::workspaces::IconTarget,
        current: Option<&str>,
        selector: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let anchor = std::rc::Rc::new(std::cell::Cell::new(gpui::Point::default()));
        let clicked_anchor = anchor.clone();
        let button = setting_button(
            self.theme,
            selector,
            self.ui().text(Text::WorkspaceChooseIcon).into(),
            false,
        )
        .relative()
        .gap(theme::space::CARD_GAP)
        .when_some(
            current.and_then(crate::icons::workspace_path),
            |row, path| {
                row.child(crate::icons::toolbar(path).text_color(rgb(self.theme.text.secondary)))
            },
        )
        .child(crate::icons::chevron(true).text_color(rgb(self.theme.text.secondary)))
        .debug_selector(move || selector.into())
        .child(
            gpui::canvas(
                move |bounds, _, _| {
                    anchor.set(gpui::point(
                        bounds.right() - gpui::px(260.),
                        bounds.bottom(),
                    ))
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .on_click(cx.listener(move |r, _, window, cx| {
            r.workspace_icon_at.set(clicked_anchor.get());
            r.workspace_icon_picker = Some(target.clone());
            r.workspace_icon_lit = None;
            window.focus(&r.focus, cx);
            cx.notify();
        }));
        detail_row(
            self,
            self.ui().text(Text::ScopedIcon).into(),
            self.ui().text(Text::ScopedIconHint).into(),
            button.into_any_element(),
        )
    }
    fn scoped_lsp_rows(
        &self,
        project: Option<guild_session::ProjectId>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        crate::highlight::Language::ALL
            .iter()
            .filter(|l| !l.server.is_empty())
            .map(|language| {
                let id = language.name;
                let enabled = project
                    .and_then(|p| self.repository_identity(p))
                    .map_or_else(
                        || self.settings.lsp_defaults.get(id).copied().unwrap_or(true),
                        |r| self.settings.lsp_enabled(&r.key, id),
                    );
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::CARD_INNER_GAP)
                    .py(theme::space::CARD_PADDING)
                    .border_t_1()
                    .border_color(theme::settings::rule(self.theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w(theme::settings::LABEL_MIN)
                            .child(id)
                            .child(
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(self.theme.text.tertiary))
                                    .child(language.server.join(" ")),
                            ),
                    );
                if let Some(project) = project {
                    let identity = self.repository_identity(project).unwrap();
                    let key = identity.key.clone();
                    let policy = self
                        .settings
                        .repository_settings
                        .get(&key)
                        .map(|p| p.lsp_policy(id))
                        .unwrap_or_default();
                    let state = if !enabled {
                        Text::LspDisabled
                    } else {
                        match self.servers.get(&(project, id)) {
                            Some(server) if server.ready => Text::LspRunning,
                            Some(_) => Text::ServerStarting,
                            None => match self.serverless.get(&(project, id)) {
                                Some(crate::lsp::progress::Fault::Missing) => Text::LspNotInstalled,
                                Some(crate::lsp::progress::Fault::Unprepared(_)) => {
                                    Text::LspUnprepared
                                }
                                Some(_) => Text::LspFailed,
                                None => Text::LspNotStarted,
                            },
                        }
                    };
                    row = row.child(
                        div()
                            .text_size(theme::size::META)
                            .child(self.ui().text(state)),
                    );
                    for (value, text) in [
                        (LspPolicy::Inherit, Text::LspInherit),
                        (LspPolicy::Enabled, Text::LspEnabled),
                        (LspPolicy::Disabled, Text::LspDisabled),
                    ] {
                        let key = key.clone();
                        let label = if value == LspPolicy::Inherit {
                            format!(
                                "{} · {}",
                                self.ui().text(text),
                                self.ui().text(
                                    if self.settings.lsp_defaults.get(id).copied().unwrap_or(true) {
                                        Text::LspOn
                                    } else {
                                        Text::LspOff
                                    }
                                )
                            )
                        } else {
                            self.ui().text(text).into()
                        };
                        row = row.child(
                            setting_button(
                                self.theme,
                                SharedString::from(format!("repo-lsp-{project}-{id}-{value:?}")),
                                label,
                                policy == value,
                            )
                            .debug_selector(move || format!("scoped-repo-lsp-{id}-{value:?}"))
                            .on_click(cx.listener(
                                move |r, _, _, cx| {
                                    r.commit_scoped_preference(
                                        Edit::Policy(key.clone(), id.into(), value),
                                        cx,
                                    )
                                },
                            )),
                        );
                    }
                } else {
                    for (value, text) in [(true, Text::LspOn), (false, Text::LspOff)] {
                        row = row.child(
                            setting_button(
                                self.theme,
                                SharedString::from(format!("global-lsp-{id}-{value}")),
                                self.ui().text(text).into(),
                                enabled == value,
                            )
                            .debug_selector(move || format!("scoped-global-lsp-{id}-{value}"))
                            .on_click(cx.listener(
                                move |r, _, _, cx| {
                                    r.commit_scoped_preference(Edit::Default(id.into(), value), cx)
                                },
                            )),
                        );
                    }
                }
                row.into_any_element()
            })
            .collect()
    }
    fn scoped_field(
        &self,
        host: SettingsHost,
        name: String,
        label: String,
        committed: String,
        field: Field,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let directory_project = match (&field, self.scoped_settings_target(host)) {
            (Field::Directory(_), Some(Target::Repository { project, .. })) => Some(project),
            _ => None,
        };
        let id = format!("{host:?}:{name}");
        let change = id.clone();
        let submit = id.clone();
        let blur = id.clone();
        let save = id.clone();
        let keyboard = field.clone();
        let blur_field = field.clone();
        let value = self
            .scoped_settings
            .drafts
            .get(&id)
            .cloned()
            .unwrap_or(committed);
        let input = crate::text_field::TextField::new(
            SharedString::from(id),
            value,
            cx.listener(move |r, value: &String, _, cx| {
                r.scoped_settings
                    .drafts
                    .insert(change.clone(), value.clone());
                cx.notify();
            }),
        )
        .vertical_scroll()
        .placeholder(self.ui().text(Text::ScopedAutomatic))
        .on_key(
            cx.listener(move |r, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "enter" => {
                        r.commit_scoped_field(&submit, &keyboard, cx);
                        cx.stop_propagation();
                    }
                    "escape" => {
                        r.scoped_settings.drafts.remove(&submit);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    "tab" => {
                        r.commit_scoped_field(&submit, &keyboard, cx);
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                    }
                    _ => {}
                }
            }),
        )
        .on_blur(
            cx.listener(move |r, _: &(), _, cx| r.commit_scoped_field(&blur, &blur_field, cx)),
        );
        let selector = match &field {
            Field::WorkspaceName(_) => "scoped-workspace-name".to_owned(),
            Field::Base(_) => "scoped-repository-base".to_owned(),
            Field::Directory(_) => "scoped-repository-directory".to_owned(),
        };
        let input_selector = format!("{selector}-input");
        div()
            .debug_selector(move || selector.clone())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .py(theme::space::CARD_PADDING)
            .border_t_1()
            .border_color(theme::settings::rule(self.theme))
            .child(
                div()
                    .flex_1()
                    .min_w(theme::settings::LABEL_MIN)
                    .child(label)
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(self.theme.text.tertiary))
                            .child(self.ui().text(match &field {
                                Field::WorkspaceName(_) => Text::ScopedNameHint,
                                Field::Base(_) => Text::ScopedBaseHint,
                                Field::Directory(_) => Text::ScopedFutureOnly,
                            })),
                    ),
            )
            .child(
                crate::text_field::frame(self.theme)
                    .debug_selector(move || input_selector.clone())
                    .w(theme::settings::CONTENT / 2.)
                    .min_w(theme::settings::LABEL_MIN)
                    .child(input),
            )
            .child(
                setting_button(
                    self.theme,
                    SharedString::from(format!("{save}-save")),
                    self.ui().text(Text::ScopedSave).into(),
                    false,
                )
                .on_click(cx.listener(move |r, _, _, cx| r.commit_scoped_field(&save, &field, cx))),
            )
            .when_some(directory_project, |row, project| {
                row.child(
                    setting_button(
                        self.theme,
                        "scoped-directory-picker",
                        self.ui().text(Text::FindInFilesChooseDirectory).into(),
                        false,
                    )
                    .debug_selector(|| "scoped-directory-picker".into())
                    .on_click(
                        cx.listener(move |r, _, _, cx| {
                            r.pick_worktree_directory(host, project, cx)
                        }),
                    ),
                )
            })
            .into_any_element()
    }
}
