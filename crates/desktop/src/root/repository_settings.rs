//! Scoped preferences and repository identities owned by the window.
use crate::view::settings::{Section, SettingsHost};
use crate::{Root, repository_settings::RepositoryIdentity};
use gpui::Context;
use guild_session::{ProjectId, WorkspaceId};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Global(Section),
    GlobalLsp,
    Workspace(WorkspaceId),
    Repository {
        workspace: WorkspaceId,
        project: ProjectId,
        page: RepoPage,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepoPage {
    Overview,
    // Retain the former internal target in compatibility regressions.
    #[cfg(test)]
    Lsp,
}
type SaveResult = std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>;
#[derive(Default)]
pub(crate) struct State {
    pub identities: HashMap<ProjectId, Result<RepositoryIdentity, String>>,
    pub loading: HashSet<ProjectId>,
    pub wanted: HashSet<ProjectId>,
    pub targets: HashMap<SettingsHost, Target>,
    pub drafts: HashMap<String, String>,
    pub error: Option<String>,
    pub stopped: bool,
    pub retry: Option<PreferenceWrite>,
    pub generations: HashMap<super::lsp::Key, u64>,
    pub pending: std::collections::VecDeque<PreferenceWrite>,
    pub flight: Option<PreferenceWrite>,
    pub writer: Option<gpui::Task<()>>,
    pub disk_worker: Option<std::thread::JoinHandle<()>>,
    pub completion: Option<SaveResult>,
    pub ordinary_dirty: HashSet<PathBuf>,
    #[cfg(test)]
    pub test_settings_path: Option<PathBuf>,
    #[cfg(test)]
    pub write_gate: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreferenceWrite {
    edit: Edit,
    path: PathBuf,
    draft: Option<(String, String)>,
}
impl Root {
    pub(crate) fn repository_identity(&self, project: ProjectId) -> Option<&RepositoryIdentity> {
        self.scoped_settings.identities.get(&project)?.as_ref().ok()
    }
    pub(crate) fn require_repository_identity(
        &self,
        project: ProjectId,
    ) -> Result<&RepositoryIdentity, String> {
        match self.scoped_settings.identities.get(&project) {
            Some(Ok(identity)) => Ok(identity),
            Some(Err(error)) => Err(format!(
                "{}: {error}",
                self.ui().text(crate::i18n::Text::ScopedUnavailable)
            )),
            None => Err(self
                .ui()
                .text(crate::i18n::Text::ScopedIdentityPending)
                .to_owned()),
        }
    }
    pub(crate) fn resolve_project_identity(
        &mut self,
        project: ProjectId,
        wanted: bool,
        cx: &mut Context<Self>,
    ) {
        if self.scoped_settings.stopped || self.desk.is_project_group_root(project) {
            return;
        }
        if wanted {
            self.scoped_settings.wanted.insert(project);
        }
        if self.scoped_settings.identities.contains_key(&project)
            || !self.scoped_settings.loading.insert(project)
        {
            return;
        }
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            self.scoped_settings.loading.remove(&project);
            return;
        };
        cx.spawn(async move |root, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { crate::repository_settings::resolve_repository(&path) })
                .await;
            let _ = root.update(cx, |root, cx| {
                root.scoped_settings.loading.remove(&project);
                if root.scoped_settings.stopped || root.desk.project(project).is_none() {
                    return;
                }
                root.scoped_settings
                    .identities
                    .entry(project)
                    .or_insert(answer);
                if root.scoped_settings.wanted.remove(&project) {
                    root.ensure_documents(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn prepare_settings_scopes(&mut self, cx: &mut Context<Self>) {
        if !self.integrations.external() {
            return;
        }
        let hosts: Vec<_> = self.scoped_settings.targets.keys().copied().collect();
        for host in hosts {
            let present = match host {
                SettingsHost::Window => self.settings_page.is_some(),
                SettingsHost::Tab(id) => self.tabs.iter().any(|tab| {
                    tab.id == id
                        && tab
                            .bodies()
                            .any(|body| matches!(body, crate::Body::Settings { .. }))
                }),
            };
            if present && self.scoped_settings_target(host).is_some() {
                continue;
            }
            let prefix = format!("{host:?}:");
            self.scoped_settings
                .drafts
                .retain(|id, _| !id.starts_with(&prefix));
            if present {
                self.choose_section(host, Section::General, cx);
            } else {
                self.scoped_settings.targets.remove(&host);
            }
        }
        // Not a project group's folder: its identity is git's to say, and
        // nothing of git runs for it but its branch label — GLD-055.
        let projects: Vec<_> = self
            .desk
            .projects()
            .map(|p| p.id)
            .filter(|project| !self.desk.is_project_group_root(*project))
            .collect();
        for project in projects {
            self.resolve_project_identity(project, false, cx);
        }
    }
    pub(crate) fn lsp_permitted(&self, project: ProjectId, language: &str) -> bool {
        // A project group's folder runs no language server — GLD-055: its
        // members' files open in the members' roots, which have theirs. The
        // person's `LspPolicy` is not written for it.
        !self.desk.is_project_group_root(project)
            && !self.scoped_settings.stopped
            && self
                .repository_identity(project)
                .is_some_and(|r| self.settings.lsp_enabled(&r.key, language))
    }
    pub(crate) fn reconcile_lsp_preferences(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<_> = self
            .servers
            .keys()
            .copied()
            .chain(self.documents.iter().map(|((p, _), d)| (*p, d.language)))
            .chain(self.diagnostics.keys().filter_map(|(project, path)| {
                crate::highlight::Language::of(path).map(|language| (*project, language.name))
            }))
            .collect();
        let keys: HashSet<_> = keys.into_iter().collect();
        for (project, language) in keys {
            if self.lsp_permitted(project, language) {
                continue;
            }
            self.servers.remove(&(project, language));
            *self
                .scoped_settings
                .generations
                .entry((project, language))
                .or_default() += 1;
            self.documents.retain(|(owner, path), _| {
                *owner != project
                    || crate::highlight::Language::of(path).is_none_or(|l| l.name != language)
            });
            self.diagnostics.retain(|(owner, path), _| {
                *owner != project
                    || crate::highlight::Language::of(path).is_none_or(|l| l.name != language)
            });
            let editors: Vec<_> = self
                .tabs
                .iter()
                .filter(|t| t.project == project)
                .flat_map(crate::Tab::bodies)
                .filter_map(|body| match body {
                    crate::Body::File {
                        path,
                        tab,
                        origin: crate::Origin::Project,
                        ..
                    } if crate::highlight::Language::of(path)
                        .is_some_and(|l| l.name == language) =>
                    {
                        tab.read(cx).editor().cloned()
                    }
                    _ => None,
                })
                .collect();
            for editor in editors {
                editor.update(cx, |state, cx| {
                    state.lsp_mut().definition_provider = None;
                    state.lsp_mut().completion_provider = None;
                    let text = state.text().clone();
                    if let Some(set) = state.diagnostics_mut() {
                        set.reset(&text);
                    }
                    cx.notify();
                });
            }
        }
        self.ensure_documents(cx);
        cx.notify();
    }
    pub(crate) fn choose_scoped_settings(
        &mut self,
        host: SettingsHost,
        target: Target,
        cx: &mut Context<Self>,
    ) {
        if let Target::Repository { project, .. } = &target {
            self.resolve_project_identity(*project, false, cx);
        }
        self.scoped_settings.targets.insert(host, target);
        self.settings_query.clear();
        self.settings_scroll.set_offset(Default::default());
        self.stop_recording();
        cx.notify();
    }
    pub(crate) fn scoped_settings_target(&self, host: SettingsHost) -> Option<Target> {
        let target = self.scoped_settings.targets.get(&host)?;
        let valid = match target {
            Target::Workspace(id) => self.desk.workspace(*id).is_some(),
            Target::Repository {
                workspace, project, ..
            } => self
                .desk
                .workspace(*workspace)
                .is_some_and(|w| w.projects.contains(project)),
            _ => true,
        };
        valid.then(|| target.clone())
    }
    pub(crate) fn pick_worktree_directory(
        &mut self,
        host: SettingsHost,
        project: ProjectId,
        cx: &mut Context<Self>,
    ) {
        let Some(identity) = self
            .repository_identity(project)
            .filter(|i| i.git && i.main.is_some())
        else {
            return;
        };
        let key = identity.key.clone();
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |root, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = root.update(cx, |root, cx| {
                if root.desk.project(project).is_none()
                    || root
                        .repository_identity(project)
                        .is_none_or(|i| i.key != key)
                {
                    return;
                }
                let id = format!("{host:?}:repo-{project}-directory");
                let value = path.to_string_lossy().into_owned();
                root.scoped_settings
                    .drafts
                    .insert(id.clone(), value.clone());
                root.queue_preference(
                    PreferenceWrite {
                        edit: Edit::Directory(key, Some(path)),
                        path: root.scoped_preferences_file(),
                        draft: Some((id, value)),
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    fn scoped_preferences_file(&self) -> PathBuf {
        #[cfg(test)]
        if let Some(path) = &self.scoped_settings.test_settings_path {
            return path.clone();
        }
        self.integrations.state_dir().join("settings.json")
    }
    pub(crate) fn commit_scoped_preference(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.commit_scoped_preference_to(edit, &self.scoped_preferences_file(), cx);
    }
    pub(crate) fn commit_scoped_preference_to(
        &mut self,
        edit: Edit,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.queue_preference(
            PreferenceWrite {
                edit,
                path: path.to_path_buf(),
                draft: None,
            },
            cx,
        );
    }
    pub(crate) fn retry_scoped_preference(&mut self, cx: &mut Context<Self>) {
        if let Some(write) = self.scoped_settings.retry.clone() {
            self.queue_preference(write, cx);
        }
    }
    fn queue_preference(&mut self, write: PreferenceWrite, cx: &mut Context<Self>) {
        if self.scoped_settings.stopped {
            return;
        }
        if let Err(error) = write.edit.validate() {
            self.scoped_settings.error = Some(error);
            self.scoped_settings.retry = None;
            cx.notify();
            return;
        }
        self.scoped_settings.pending.push_back(write);
        self.start_preference_writer(cx);
        cx.notify();
    }
    fn start_preference_writer(&mut self, cx: &mut Context<Self>) {
        if self.scoped_settings.flight.is_some() || self.scoped_settings.stopped {
            return;
        }
        let Some(write) = self.scoped_settings.pending.pop_front() else {
            for path in std::mem::take(&mut self.scoped_settings.ordinary_dirty) {
                self.store_settings_to(&path);
            }
            return;
        };
        let mut candidate = self.settings.clone();
        write.edit.publish(&mut candidate);
        self.scoped_settings.flight = Some(write.clone());
        #[cfg(test)]
        let gate = self.scoped_settings.write_gate.take();
        let completion: SaveResult = Default::default();
        let answer = completion.clone();
        self.scoped_settings.disk_worker = match std::thread::Builder::new()
            .name("guild-settings".into())
            .spawn(move || {
                #[cfg(test)]
                if let Some((started, release)) = gate {
                    let _ = started.send(());
                    let _ = release.recv();
                }
                let result = guild_session::Store::new(write.path.clone())
                    .save(&candidate)
                    .map_err(|e| e.to_string());
                *answer.lock().unwrap() = Some(result);
            }) {
            Ok(worker) => Some(worker),
            Err(error) => {
                *completion.lock().unwrap() = Some(Err(error.to_string()));
                None
            }
        };
        self.scoped_settings.completion = Some(completion.clone());
        self.scoped_settings.writer = Some(cx.spawn(async move |root, cx| {
            let result = loop {
                if let Some(result) = completion.lock().unwrap().clone() {
                    break result;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(5))
                    .await;
            };
            let _ = root.update(cx, |root, cx| {
                let Some(write) = root.scoped_settings.flight.take() else {
                    return;
                };
                match result {
                    Ok(()) => {
                        // Publish only the acknowledged field, never the old
                        // whole-settings snapshot. Ordinary changes may be newer.
                        let superseded = root
                            .scoped_settings
                            .pending
                            .iter()
                            .any(|newer| newer.edit.same_field(&write.edit));
                        if !superseded {
                            write.edit.publish(&mut root.settings);
                        }
                        if !superseded
                            && root
                                .scoped_settings
                                .retry
                                .as_ref()
                                .is_none_or(|failed| failed.edit.same_field(&write.edit))
                        {
                            root.scoped_settings.error = None;
                            root.scoped_settings.retry = None;
                        }
                        if let Some((id, value)) = write.draft
                            && root.scoped_settings.drafts.get(&id) == Some(&value)
                        {
                            root.scoped_settings.drafts.remove(&id);
                        }
                        root.scoped_settings.ordinary_dirty.insert(write.path);
                        if !superseded && !root.scoped_settings.stopped {
                            root.reconcile_lsp_preferences(cx);
                        }
                    }
                    Err(error) => {
                        root.scoped_settings.error = Some(error);
                        root.scoped_settings.retry = Some(write);
                        root.fail(
                            root.ui()
                                .text(crate::i18n::Text::NoticeSettingsNotSaved)
                                .to_owned(),
                        );
                    }
                }
                root.start_preference_writer(cx);
                cx.notify();
            });
        }));
    }
    pub(crate) fn store_settings_to(&mut self, path: &std::path::Path) {
        if self.scoped_settings.flight.is_some() || !self.scoped_settings.pending.is_empty() {
            self.scoped_settings
                .ordinary_dirty
                .insert(path.to_path_buf());
            return;
        }
        if let Err(error) = guild_session::Store::new(path.to_path_buf()).save_soon(&self.settings)
        {
            tracing::warn!(%error, "settings were not queued");
            // A file this run could not read was named once when it was
            // read; a window move refused on every frame says nothing new.
            if matches!(error, guild_session::StoreError::NotRead { .. }) {
                return;
            }
            self.fail(
                self.ui()
                    .text(crate::i18n::Text::NoticeSettingsNotSaved)
                    .to_owned(),
            );
        }
    }
    pub(crate) fn finish_preferences(
        &mut self,
        cx: &mut Context<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        // Quit may poll futures while App is borrowed. Await only background
        // disk work here; cancel the UI acknowledgement task, never re-enter App.
        self.scoped_settings.writer.take();
        let completion = self.scoped_settings.completion.take();
        let disk_worker = self.scoped_settings.disk_worker.take();
        let flight = self.scoped_settings.flight.take();
        let pending = std::mem::take(&mut self.scoped_settings.pending);
        let mut paths = std::mem::take(&mut self.scoped_settings.ordinary_dirty);
        let mut settings = self.settings.clone();
        let executor = cx.background_executor().clone();
        Box::pin(async move {
            executor
                .spawn(async move {
                    if let Some(worker) = disk_worker {
                        let _ = worker.join();
                    }
                    if let Some(completion) = completion
                        && completion
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_some_and(Result::is_ok)
                        && let Some(write) = flight
                    {
                        write.edit.publish(&mut settings);
                        paths.insert(write.path);
                    }
                    for write in pending {
                        write.edit.publish(&mut settings);
                        paths.insert(write.path);
                    }
                    for path in paths {
                        if let Err(error) = guild_session::Store::new(path).save(&settings) {
                            tracing::warn!(%error, "settings did not finish before quit");
                        }
                    }
                    guild_session::flush();
                })
                .await;
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Edit {
    Default(String, bool),
    Policy(
        PathBuf,
        String,
        guild_session::repository_settings::LspPolicy,
    ),
    Base(PathBuf, Option<String>),
    Directory(PathBuf, Option<PathBuf>),
    Icon(PathBuf, Option<String>),
}
impl Edit {
    fn same_field(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Default(a, _), Self::Default(b, _)) => a == b,
            (Self::Policy(a, al, _), Self::Policy(b, bl, _)) => a == b && al == bl,
            (Self::Base(a, _), Self::Base(b, _))
            | (Self::Directory(a, _), Self::Directory(b, _))
            | (Self::Icon(a, _), Self::Icon(b, _)) => a == b,
            _ => false,
        }
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Base(_, Some(base)) => crate::repository_settings::validate_base(base),
            Self::Directory(_, Some(path)) => crate::repository_settings::validate_directory(path),
            Self::Icon(_, Some(name))
                if !guild_session::workspace::ICONS.contains(&name.as_str()) =>
            {
                Err("choose an installed icon".into())
            }
            _ => Ok(()),
        }
    }
    fn publish(&self, settings: &mut guild_session::Settings) {
        match self {
            Self::Default(language, value) => {
                settings.lsp_defaults.insert(language.clone(), *value);
            }
            Self::Policy(key, language, value) => {
                settings
                    .repository_settings
                    .entry(key.clone())
                    .or_default()
                    .lsp
                    .insert(language.clone(), *value);
            }
            Self::Base(key, value) => {
                settings
                    .repository_settings
                    .entry(key.clone())
                    .or_default()
                    .worktree_base = value.clone();
            }
            Self::Directory(key, value) => {
                settings
                    .repository_settings
                    .entry(key.clone())
                    .or_default()
                    .worktree_directory = value.clone();
            }
            Self::Icon(key, value) => {
                settings
                    .repository_settings
                    .entry(key.clone())
                    .or_default()
                    .icon = value.clone();
            }
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) enum Field {
    WorkspaceName(WorkspaceId),
    Base(PathBuf),
    Directory(PathBuf),
}
impl Root {
    pub(crate) fn commit_scoped_field(&mut self, id: &str, field: &Field, cx: &mut Context<Self>) {
        let Some(value) = self.scoped_settings.drafts.get(id).cloned() else {
            return;
        };
        let submitted = value.clone();
        let value = value.trim();
        match field {
            Field::WorkspaceName(workspace) => {
                if value.is_empty() {
                    self.scoped_settings.retry = None;
                    self.scoped_settings.error =
                        Some(self.ui().text(crate::i18n::Text::ScopedName).to_owned());
                    cx.notify();
                    return;
                }
                if self.desk.rename_workspace(*workspace, value) {
                    self.save();
                    self.rebuild_menu(cx);
                }
                self.scoped_settings.error = None;
                self.scoped_settings.retry = None;
            }
            Field::Base(key) => {
                self.queue_preference(
                    PreferenceWrite {
                        edit: Edit::Base(
                            key.clone(),
                            (!value.is_empty()).then(|| value.to_owned()),
                        ),
                        path: self.scoped_preferences_file(),
                        draft: Some((id.to_owned(), submitted.clone())),
                    },
                    cx,
                );
                return;
            }
            Field::Directory(key) => {
                self.queue_preference(
                    PreferenceWrite {
                        edit: Edit::Directory(
                            key.clone(),
                            (!value.is_empty()).then(|| PathBuf::from(value)),
                        ),
                        path: self.scoped_preferences_file(),
                        draft: Some((id.to_owned(), submitted.clone())),
                    },
                    cx,
                );
                return;
            }
        }
        if self.scoped_settings.error.is_none() {
            self.scoped_settings.drafts.remove(id);
        }
        cx.notify();
    }
}
