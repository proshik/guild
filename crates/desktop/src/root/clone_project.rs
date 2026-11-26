//! The window owns one clone, independent of project workers and workspace switches.
use crate::{
    Root,
    clone_project::{Field, Form, Phase},
    git::clone::{Request, Worker},
    i18n::Text,
};
use gpui::{Context, FocusHandle, Window};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct CloneProject {
    pub form: Form,
    pub phase: Phase,
    pub field: Field,
    pub visible: bool,
    pub focus: FocusHandle,
    pub previous: Option<FocusHandle>,
    pub worker: Option<Worker>,
    pub workspace: guild_session::WorkspaceId,
    pub diagnostics: String,
    pub problem: Option<String>,
    pub details: bool,
}
impl Root {
    pub(crate) fn open_clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self
            .clone_project
            .as_ref()
            .filter(|open| open.visible)
            .and_then(|open| open.previous.clone())
            .or_else(|| {
                self.picker
                    .as_ref()
                    .and_then(|open| open.previous.clone())
                    .or_else(|| window.focused(cx))
            });
        self.put_fields_away();
        if self.clone_project.is_none() {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"));
            let active = self.active_project.and_then(|id| self.desk.project(id));
            let parent = crate::clone_project::default_parent(
                self.desk.recent().next(),
                active.map(|project| project.path.as_path()),
                &home,
            );
            self.clone_project = Some(CloneProject {
                form: Form::new(&parent),
                phase: Phase::Editing,
                field: Field::Url,
                visible: true,
                focus: cx.focus_handle(),
                previous: previous.clone(),
                worker: None,
                workspace: self.active_workspace,
                diagnostics: String::new(),
                problem: None,
                details: false,
            });
        }
        if let Some(open) = self.clone_project.as_mut() {
            open.previous = previous;
            open.visible = true;
            window.focus(&open.focus, cx);
        }
        cx.notify();
    }
    /// Hiding a busy form leaves its worker owned and available from the same action.
    pub(crate) fn hide_clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.clone_project.as_mut().and_then(|open| {
            open.visible = false;
            open.previous.take()
        });
        match previous {
            Some(focus) => window.focus(&focus, cx),
            None => self.refocus(window, cx),
        }
        cx.notify();
    }
    pub(crate) fn cancel_clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.clone_project.as_mut() else {
            return;
        };
        if let Some(worker) = &open.worker {
            worker.cancel();
            open.phase.cancel();
            cx.notify();
        } else {
            self.hide_clone_project(window, cx);
            self.clone_project = None;
        }
    }
    pub(crate) fn clone_project_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self
            .clone_project
            .as_ref()
            .filter(|open| !open.phase.busy() && !matches!(open.phase, Phase::Succeeded(_)))
        else {
            return;
        };
        let start = PathBuf::from(&open.form.directory);
        let parent = start
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(&open.form.parent);
        let chosen = cx.prompt_for_new_path(parent, start.file_name().and_then(|n| n.to_str()));
        cx.spawn_in(window, async move |root, cx| {
            if let Ok(Ok(Some(path))) = chosen.await {
                let _ = root.update_in(cx, |root, window, cx| {
                    if let Some(open) = root.clone_project.as_mut().filter(|open| {
                        !open.phase.busy() && !matches!(open.phase, Phase::Succeeded(_))
                    }) {
                        open.form.set_directory(path.display().to_string());
                        open.problem = None;
                        open.field = Field::Directory;
                        window.focus(&open.focus, cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(crate) fn start_clone_project(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.clone_project.as_mut() else {
            return;
        };
        if open.phase.busy() {
            return;
        }
        if let Phase::Succeeded(path) = &open.phase {
            let path = path.clone();
            // This is the explicit Open action after the original workspace vanished.
            if self.open_project_in_workspace(self.active_workspace, &path, cx) {
                self.clone_project = None;
                self.focusing_front = true;
                cx.notify();
            }
            return;
        }
        let request = match Request::new(&open.form.url, Path::new(&open.form.directory)) {
            Ok(request) => request,
            Err(message) => {
                open.problem = Some(message);
                cx.notify();
                return;
            }
        };
        if !open.phase.start() {
            return;
        }
        open.workspace = self.active_workspace;
        open.diagnostics.clear();
        open.problem = None;
        open.worker = Some(Worker::start(request));
        cx.notify();
        cx.spawn(async move |root, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let alive = root.update(cx, |root, cx| {
                    let Some(open) = root.clone_project.as_mut() else {
                        return false;
                    };
                    let Some(worker) = open.worker.as_mut() else {
                        return false;
                    };
                    let progress = worker.progress();
                    if progress != open.diagnostics {
                        open.diagnostics = progress;
                        cx.notify();
                    }
                    let Some(outcome) = worker.finish() else {
                        return true;
                    };
                    open.worker = None;
                    if !open.phase.complete(outcome) {
                        return false;
                    }
                    let finished = match &open.phase {
                        Phase::Succeeded(path) => Some((open.workspace, path.clone())),
                        _ => None,
                    };
                    if let Some((workspace, path)) = finished.as_ref() {
                        if root.open_project_in_workspace(*workspace, path, cx) {
                            root.clone_project = None;
                            root.focusing_front = true;
                        } else if let Some(open) = root.clone_project.as_mut() {
                            open.problem = Some(
                                crate::i18n::Ui(root.settings.language)
                                    .text(Text::CloneOpenMissing)
                                    .to_owned(),
                            );
                        }
                    }
                    if root
                        .clone_project
                        .as_ref()
                        .is_some_and(|open| !open.visible)
                    {
                        let text =
                            root.ui()
                                .text(if finished.is_some() {
                                    Text::CloneOpenMissing
                                } else if root.clone_project.as_ref().is_some_and(|open| {
                                    matches!(open.phase, Phase::Cancelled { .. })
                                }) {
                                    Text::CloneCancelled
                                } else {
                                    Text::CloneFailed
                                })
                                .to_owned();
                        root.inform(text);
                    }
                    cx.notify();
                    false
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }
    pub(crate) fn clone_project_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.clone_project.as_mut().filter(|open| open.visible) else {
            return false;
        };
        let stroke = &event.keystroke;
        match stroke.key.as_str() {
            "escape" => self.cancel_clone_project(window, cx),
            "tab" => {
                open.field = open.field.step(!stroke.modifiers.shift);
                window.focus(&open.focus, cx);
                cx.notify();
            }
            "enter" => match open.field {
                Field::Choose => self.clone_project_choose(window, cx),
                Field::Cancel => self.cancel_clone_project(window, cx),
                _ => self.start_clone_project(cx),
            },
            _ => {}
        }
        true
    }
}
