//! Explorer file-operation ownership. Workers carry the originating project.
#[cfg(test)]
pub(crate) mod clipboard_probe {
    std::thread_local! { pub static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
}
use crate::{
    Origin, Root,
    file_operations::{self, Collision, Operation, Outcome},
    i18n::Text,
};
use gpui::{App, Context, Focusable, Window, div, prelude::*};
use guild_session::ProjectId;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(crate) struct State {
    clipboard: Option<FileClipboard>,
    pub form: Option<Form>,
    pub busy: Option<Pending>,
    pub drag: Option<super::tree_drag::Drag>,
}
struct FileClipboard {
    project: ProjectId,
    sources: Vec<PathBuf>,
    cut: bool,
    nonce: String,
    identities: Vec<(u64, u64)>,
}
struct Run {
    project: ProjectId,
    root_path: PathBuf,
    sources: TransferSources,
    destination: PathBuf,
    operation: Operation,
    cancel: Arc<AtomicBool>,
}
struct TransferSources {
    paths: Vec<PathBuf>,
    identities: std::collections::BTreeMap<PathBuf, (u64, u64)>,
    clipboard_nonce: Option<String>,
}

pub(crate) struct Form {
    project: ProjectId,
    sources: Vec<PathBuf>,
    typed: String,
}
pub(crate) struct Pending {
    pub project: ProjectId,
    pub sources: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
}
impl Pending {
    #[cfg(test)]
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Root {
    pub(crate) fn select_operation_target(&mut self, path: &Path) {
        if let Some(state) = self.tree_mut() {
            let key = (Origin::Project, path.to_path_buf());
            if super::tree::writable_row(path) {
                state.files.context_click(&key, std::slice::from_ref(&key));
            } else {
                state.files = Default::default();
            }
            state.selected = Some(path.into());
            state.library_selected = None;
        }
    }
    pub(crate) fn forget_file_operations(&mut self, project: ProjectId) {
        // Keep the gesture clock alive until it clears GPUI's native preview.
        if let Some(pending) = &self.file_operations.busy
            && pending.project == project
        {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        if self
            .file_operations
            .clipboard
            .as_ref()
            .is_some_and(|p| p.project == project)
        {
            self.file_operations.clipboard = None;
        }
        if self
            .file_operations
            .form
            .as_ref()
            .is_some_and(|p| p.project == project)
        {
            self.file_operations.form = None;
        }
    }
    pub(crate) fn operation_sources(&self) -> Vec<PathBuf> {
        let Some(state) = self.tree() else {
            return Vec::new();
        };
        let mut paths: Vec<_> = state
            .files
            .selected
            .iter()
            .filter(|(origin, path)| *origin == Origin::Project && super::tree::writable_row(path))
            .map(|(_, p)| p.clone())
            .collect();
        if paths.is_empty()
            && state.library_selected.is_none()
            && let Some(path) = &state.selected
            && super::tree::writable_row(path)
        {
            paths.push(path.clone());
        }
        file_operations::normalize(&paths)
    }
    pub(crate) fn operation_target(&self) -> Option<PathBuf> {
        let state = self.tree()?;
        if state.library_selected.is_some() {
            return None;
        }
        let path = state.selected.clone().unwrap_or_default();
        Some(if state.tree.root().join(&path).is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("")).into()
        })
    }
    fn owns_file_clipboard(&self, cx: &mut App) -> bool {
        self.file_operations
            .clipboard
            .as_ref()
            .is_some_and(|payload| {
                #[cfg(test)]
                clipboard_probe::READS.with(|reads| reads.set(reads.get() + 1));
                cx.read_from_clipboard()
                    .and_then(|item| item.metadata().cloned())
                    == serde_json::to_string(&payload.nonce).ok()
            })
    }
    /// Resolve native ownership once for the whole Explorer render.
    pub(crate) fn cut_sources(&self, cx: &mut App) -> &[PathBuf] {
        if let Some(payload) = &self.file_operations.clipboard
            && payload.cut
            && Some(payload.project) == self.active_project
            && self.owns_file_clipboard(cx)
        {
            &payload.sources
        } else {
            &[]
        }
    }
    pub(crate) fn file_clipboard(&mut self, cut: bool, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let sources = self.operation_sources();
        if sources.is_empty() {
            return;
        }
        let Some(root) = self.project_root() else {
            return;
        };
        // A copy does not write its source; a cut is a move — GLD-057.
        if cut && self.refuse_open_root(&root, &sources) {
            cx.notify();
            return;
        }
        let identities = match sources
            .iter()
            .map(|p| clipboard_identity(&root, p))
            .collect::<std::io::Result<Vec<_>>>()
        {
            Ok(identities) => identities,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        let text = sources
            .iter()
            .map(|p| root.join(p).display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let nonce = uuid::Uuid::new_v4().to_string();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_json_metadata(
            text, &nonce,
        ));
        self.file_operations.clipboard = Some(FileClipboard {
            project,
            sources,
            cut,
            nonce,
            identities,
        });
        cx.notify();
    }
    pub(crate) fn paste_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.owns_file_clipboard(cx) {
            self.file_operations.clipboard = None;
            cx.notify();
            return;
        }
        let Some(dest) = self.operation_target() else {
            return;
        };
        let Some(payload) = &self.file_operations.clipboard else {
            return;
        };
        if self.active_project != Some(payload.project) {
            return;
        }
        let valid = self.desk.project(payload.project).is_some_and(|project| {
            payload
                .sources
                .iter()
                .zip(&payload.identities)
                .all(|(path, identity)| {
                    clipboard_identity(&project.path, path).ok().as_ref() == Some(identity)
                })
        });
        if !valid {
            self.file_operations.clipboard = None;
            self.warn(self.ui().text(Text::FileClipboardChanged).into());
            cx.notify();
            return;
        }
        self.start_transfer_snapshot(
            payload.project,
            TransferSources {
                paths: payload.sources.clone(),
                identities: payload
                    .sources
                    .iter()
                    .cloned()
                    .zip(payload.identities.iter().copied())
                    .collect(),
                clipboard_nonce: Some(payload.nonce.clone()),
            },
            dest,
            if payload.cut {
                Operation::Move
            } else {
                Operation::Copy
            },
            window,
            cx,
        );
    }
    pub(crate) fn cancel_file_operation(&mut self, cx: &mut Context<Self>) {
        if let Some(busy) = &self.file_operations.busy {
            busy.cancel.store(true, Ordering::Relaxed);
        }
        self.file_operations.form = None;
        if self
            .file_operations
            .clipboard
            .as_ref()
            .is_some_and(|c| c.cut)
        {
            self.file_operations.clipboard = None;
        }
        cx.notify();
    }
    pub(crate) fn move_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_operations.busy.is_some() {
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        let sources = self.operation_sources();
        if sources.is_empty() {
            return;
        }
        if let Some(root) = self.project_root()
            && self.refuse_open_root(&root, &sources)
        {
            cx.notify();
            return;
        }
        self.file_operations.form = Some(Form {
            project,
            typed: sources
                .first()
                .and_then(|p| p.parent())
                .unwrap_or(Path::new(""))
                .display()
                .to_string(),
            sources,
        });
        if let Some(state) = self.tree() {
            window.focus(&state.focus, cx);
        }
        cx.notify();
    }
    /// A running transfer and its Cancel, drawn below the rows.
    pub(crate) fn render_transfer_busy(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let ui = self.ui();
        let busy = self.file_operations.busy.as_ref()?;
        (Some(busy.project) == self.active_project).then(|| {
            div()
                .p(crate::theme::space::CARD_INNER_GAP)
                .child(ui.text(Text::FileTransferBusy).to_owned())
                .child(
                    div()
                        .id("cancel-file-transfer")
                        .debug_selector(|| "explorer-transfer-cancel".into())
                        .cursor_pointer()
                        .child(ui.text(Text::Cancel).to_owned())
                        .on_click(cx.listener(|root, _, _, cx| {
                            cx.stop_propagation();
                            root.cancel_file_operation(cx)
                        })),
                )
                .into_any_element()
        })
    }
    /// The Move destination field, drawn above the rows like the naming
    /// field; not while a transfer runs.
    pub(crate) fn render_transfer_form(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let ui = self.ui();
        if self
            .file_operations
            .busy
            .as_ref()
            .is_some_and(|busy| Some(busy.project) == self.active_project)
        {
            return None;
        }
        let form = self
            .file_operations
            .form
            .as_ref()
            .filter(|f| Some(f.project) == self.active_project)?;
        Some(
            div()
                .p(crate::theme::space::CARD_INNER_GAP)
                .flex()
                .flex_col()
                .gap(crate::theme::space::CARD_INNER_GAP)
                .child(format!(
                    "{}: {}",
                    ui.text(Text::MenuMove),
                    form.sources
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .child(
                    div()
                        .bg(gpui::rgb(theme.surface.inset))
                        .rounded(crate::theme::radius::ROW)
                        .child(
                            crate::text_field::TextField::new(
                                "file-move-destination",
                                form.typed.clone(),
                                cx.listener(|root, value: &String, _, cx| {
                                    if let Some(form) = root.file_operations.form.as_mut() {
                                        form.typed = value.clone();
                                    }
                                    cx.notify();
                                }),
                            )
                            .autofocus()
                            .initial_selection(0..form.typed.len())
                            .placeholder(ui.text(Text::FileTransferDestination))
                            .on_key(cx.listener(
                                |root, event: &gpui::KeyDownEvent, window, cx| {
                                    match event.keystroke.key.as_str() {
                                        "escape" => {
                                            root.file_operations.form = None;
                                            if let Some(state) = root.tree() {
                                                window.focus(&state.focus, cx);
                                            }
                                            cx.notify();
                                        }
                                        "enter" => {
                                            let Some(form) = root.file_operations.form.take()
                                            else {
                                                return;
                                            };
                                            if let Some(state) = root.tree() {
                                                window.focus(&state.focus, cx);
                                            }
                                            root.start_transfer(
                                                form.project,
                                                form.sources,
                                                PathBuf::from(form.typed),
                                                Operation::Move,
                                                window,
                                                cx,
                                            );
                                        }
                                        _ => {}
                                    }
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }
    pub(crate) fn start_transfer(
        &mut self,
        project: ProjectId,
        sources: Vec<PathBuf>,
        destination: PathBuf,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_operations.busy.is_some() || sources.is_empty() {
            return;
        }
        let Some(root_path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let sources = file_operations::normalize(&sources);
        let identities = match sources
            .iter()
            .map(|path| clipboard_identity(&root_path, path).map(|id| (path.clone(), id)))
            .collect::<std::io::Result<std::collections::BTreeMap<_, _>>>()
        {
            Ok(identities) => identities,
            Err(error) => {
                self.fail(error.to_string());
                cx.notify();
                return;
            }
        };
        self.start_transfer_snapshot(
            project,
            TransferSources {
                paths: sources,
                identities,
                clipboard_nonce: None,
            },
            destination,
            operation,
            window,
            cx,
        );
    }

    /// Copy what another application dropped, by absolute path, through the
    /// same worker, collision questions, cancel and notices as any transfer.
    /// The dropped paths may be on any volume, so they are read — checked
    /// against the destination and their identities taken — off the window's
    /// thread first. The import is a transfer from the moment it is accepted:
    /// `busy` holds it with its cancel flag while the check runs, so Cancel
    /// reaches it and nothing else starts in between; a failed check says why.
    pub(crate) fn start_import(
        &mut self,
        project: ProjectId,
        sources: Vec<PathBuf>,
        destination: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_operations.busy.is_some() || sources.is_empty() {
            return;
        }
        let Some(root_path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let sources = file_operations::normalize(&sources);
        let cancel = Arc::new(AtomicBool::new(false));
        self.file_operations.busy = Some(Pending {
            project,
            sources: sources.clone(),
            cancel: cancel.clone(),
        });
        cx.notify();
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let (paths, dest, at) = (sources.clone(), destination.clone(), root_path.clone());
                let checked = cx
                    .background_executor()
                    .spawn(async move { file_operations::inbound(&at, &paths, &dest) })
                    .await;
                let _ = cx.update(|window, cx| {
                    root.update(cx, |root, cx| {
                        let ours = root
                            .file_operations
                            .busy
                            .as_ref()
                            .is_some_and(|busy| Arc::ptr_eq(&busy.cancel, &cancel));
                        if !ours {
                            return;
                        }
                        match checked {
                            _ if cancel.load(Ordering::Relaxed) => {
                                root.file_operations.busy = None;
                            }
                            Err(error) => {
                                root.file_operations.busy = None;
                                root.fail(error.to_string());
                            }
                            Ok(dropped) => root.run_transfer(
                                Run {
                                    project,
                                    root_path,
                                    sources: TransferSources {
                                        paths: sources,
                                        identities: dropped
                                            .into_iter()
                                            .map(|d| (d.path, d.id))
                                            .collect(),
                                        clipboard_nonce: None,
                                    },
                                    destination,
                                    operation: Operation::Copy,
                                    cancel,
                                },
                                window,
                                cx,
                            ),
                        }
                        cx.notify();
                    })
                });
            })
            .detach();
    }

    fn start_transfer_snapshot(
        &mut self,
        project: ProjectId,
        sources: TransferSources,
        destination: PathBuf,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_operations.busy.is_some() || sources.paths.is_empty() {
            return;
        }
        let Some(root_path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        // Every move meets here — Paste after Cut, the Move field, a drop
        // — so the refusal does too. GLD-057.
        if operation == Operation::Move && self.refuse_open_root(&root_path, &sources.paths) {
            cx.notify();
            return;
        }
        let TransferSources {
            paths: sources,
            identities,
            clipboard_nonce,
        } = sources;
        let cancel = Arc::new(AtomicBool::new(false));
        if operation == Operation::Move {
            self.freeze_moving_editors(project, &sources, true, window, cx);
        }
        self.file_operations.busy = Some(Pending {
            project,
            sources: sources.clone(),
            cancel: cancel.clone(),
        });
        self.run_transfer(
            Run {
                project,
                root_path,
                sources: TransferSources {
                    paths: sources,
                    identities,
                    clipboard_nonce,
                },
                destination,
                operation,
                cancel,
            },
            window,
            cx,
        );
    }

    /// The worker of a transfer whose `busy` is already recorded, with the
    /// cancel flag that record holds.
    fn run_transfer(&mut self, run: Run, window: &mut Window, cx: &mut Context<Self>) {
        let Run {
            project,
            root_path,
            sources:
                TransferSources {
                    paths: sources,
                    identities,
                    clipboard_nonce,
                },
            destination,
            operation,
            cancel,
        } = run;
        let root = cx.weak_entity();
        let ui = self.ui();
        window
            .spawn(cx, async move |cx| {
                let mut remaining = sources;
                let mut completed = Vec::new();
                let mut errors = Vec::new();
                let mut cancelled = false;
                let mut policy = Collision::Cancel;
                loop {
                    let (path, items, dest, stop) = (
                        root_path.clone(),
                        remaining.clone(),
                        destination.clone(),
                        cancel.clone(),
                    );
                    let identities = identities.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            file_operations::execute_with_identities(
                                &path,
                                &items,
                                &dest,
                                operation,
                                policy,
                                &stop,
                                Some(&identities),
                            )
                        })
                        .await;
                    remaining = result.remaining.clone();
                    completed.extend(result.completed.iter().cloned());
                    errors.extend(result.errors.iter().cloned());
                    let collisions = result.collisions.clone();
                    let alive = cx
                        .update(|window, cx| {
                            root.update(cx, |root, cx| {
                                if root
                                    .desk
                                    .project(project)
                                    .is_none_or(|p| p.path != root_path)
                                {
                                    return false;
                                }
                                root.apply_transfer(project, &result, operation, window, cx);
                                true
                            })
                        })
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(false);
                    if !alive
                        || cancel.load(Ordering::Relaxed)
                        || collisions.is_empty()
                        || policy == Collision::Skip
                    {
                        break;
                    }
                    let title = ui.text(Text::FileTransferCollision);
                    let detail = collisions
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let Ok(answer) = cx.update(|window, cx| {
                        window.prompt(
                            gpui::PromptLevel::Warning,
                            title,
                            Some(&detail),
                            &[
                                gpui::PromptButton::new(ui.text(Text::FileTransferSkip)),
                                gpui::PromptButton::new(ui.text(Text::FileTransferKeepBoth)),
                                gpui::PromptButton::cancel(ui.text(Text::Cancel)),
                            ],
                            cx,
                        )
                    }) else {
                        break;
                    };
                    policy = match answer.await {
                        Ok(0) => Collision::Skip,
                        Ok(1) => Collision::KeepBoth,
                        _ => {
                            cancelled = true;
                            break;
                        }
                    };
                }
                let _ = cx.update(|window, cx| {
                    root.update(cx, |root, cx| {
                        if (cancelled || cancel.load(Ordering::Relaxed) || !errors.is_empty())
                            && root
                                .file_operations
                                .clipboard
                                .as_ref()
                                .is_some_and(|p| Some(&p.nonce) == clipboard_nonce.as_ref())
                        {
                            root.file_operations.clipboard = None;
                        }
                        if !remaining.is_empty() || !errors.is_empty() {
                            let done = completed
                                .iter()
                                .map(|(from, to)| format!("{} → {}", from.display(), to.display()))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let left = remaining
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join("\n");
                            let failures = errors
                                .iter()
                                .map(|(p, e)| format!("{}: {e}", p.display()))
                                .collect::<Vec<_>>()
                                .join("\n");
                            root.warn(format!(
                                "{}\n{}:\n{done}\n{}:\n{left}\n{failures}",
                                root_path.display(),
                                ui.text(Text::FileTransferCompleted),
                                ui.text(Text::FileTransferRemaining)
                            ));
                        }
                        if let Some(pending) = root.file_operations.busy.take() {
                            root.freeze_moving_editors(
                                pending.project,
                                &pending.sources,
                                false,
                                window,
                                cx,
                            );
                        }
                        cx.notify();
                    })
                });
            })
            .detach();
        cx.notify();
    }
    fn apply_transfer(
        &mut self,
        project: ProjectId,
        result: &Outcome,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if operation == Operation::Move {
            for (from, to) in &result.completed {
                self.relocate_project_paths(project, from, to, window, cx);
            }
        }
        for (seat, state) in &mut self.trees {
            if seat.project != project {
                continue;
            }
            let expanded: Vec<_> = state.tree.expanded().map(Path::to_path_buf).collect();
            for dir in expanded {
                state.tree.refresh(&dir);
            }
            for (_, to) in &result.completed {
                state.tree.reveal(to);
            }
            state.settle_selection();
        }
        if operation == Operation::Move
            && self.owns_file_clipboard(cx)
            && let Some(clipboard) = &mut self.file_operations.clipboard
            && clipboard.project == project
            && clipboard.cut
        {
            (clipboard.sources, clipboard.identities) = clipboard
                .sources
                .iter()
                .zip(&clipboard.identities)
                .filter(|(p, _)| !result.completed.iter().any(|(from, _)| p.starts_with(from)))
                .map(|(p, identity)| (p.clone(), *identity))
                .unzip();
            if clipboard.sources.is_empty() {
                self.file_operations.clipboard = None;
            }
        }
        cx.notify();
    }
}

impl Root {
    pub(crate) fn freeze_moving_editors(
        &self,
        project: ProjectId,
        sources: &[PathBuf],
        frozen: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        for tab in self.tabs.iter().filter(|t| t.project == project) {
            for (_, body) in tab.panes.bodies() {
                if let crate::Body::File {
                    path,
                    tab,
                    origin: Origin::Project,
                    ..
                } = body
                    && sources.iter().any(|source| path.starts_with(source))
                {
                    tab.update(cx, |file, cx| file.relocating(frozen, window, cx));
                }
            }
        }
    }
    pub(crate) fn relocate_project_paths(
        &mut self,
        project: ProjectId,
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            self.freeze_moving_editors(project, &[from.into()], false, window, cx);
            return;
        }
        let stale: Vec<_> = self.tabs.iter().filter(|t| t.project == project).filter_map(|t| {
            let call = self.parked.get(&t.id)?;
            matches!(t.panes.body_of(call.pane), Some(crate::Body::Diff { path: Some(path), base: guild_session::Base::Proposal, .. }) if path.starts_with(from)).then_some(t.id)
        }).collect();
        for tab in stale {
            self.reject_proposal(tab, cx);
        }
        let mut documents = Vec::new();
        let mut editors = std::collections::HashSet::new();
        for owner in self.tabs.iter_mut().filter(|t| t.project == project) {
            for pane in owner.panes.ids() {
                let Some(mut body) = owner.panes.take(pane) else {
                    continue;
                };
                if let crate::Body::File {
                    path,
                    tab,
                    origin: Origin::Project,
                    presentation,
                } = &mut body
                    && let Some(next) = file_operations::remap(path, from, to)
                {
                    let old = path.clone();
                    tab.update(cx, |file, cx| {
                        file.relocate(&next, window, cx);
                        file.relocating(false, window, cx);
                    });
                    if let Some(editor) = tab.read(cx).editor()
                        && editors.insert(editor.entity_id())
                    {
                        documents.push((old, next.clone(), editor.clone()));
                    }
                    let markdown = guild_session::markdown::has_preview(&next);
                    if markdown && presentation.is_none() {
                        *presentation = Self::markdown_presentation(
                            tab,
                            self.settings.markdown_default_view,
                            &self.diagrams,
                            cx,
                        );
                        if let Some(p) = presentation {
                            p.restore(guild_session::markdown::Mode::Editor, cx);
                        }
                    } else if !markdown
                        && let Some(p) = presentation.take()
                        && p.view
                            .read(cx)
                            .focus_handle(cx)
                            .contains_focused(window, cx)
                    {
                        window.focus(&tab.read(cx).focus_handle(cx), cx);
                    }
                    *path = next;
                }
                match &mut body {
                    crate::Body::Diff {
                        path,
                        base: guild_session::Base::Head | guild_session::Base::Unstaged,
                        tab,
                    } => {
                        if let Some(path) = path
                            && let Some(next) = file_operations::remap(path, from, to)
                        {
                            *path = next;
                        }
                        tab.update(cx, |diff, cx| diff.relocate_paths(from, to, cx));
                    }
                    crate::Body::Annotate { path, tab } => {
                        if let Some(next) = file_operations::remap(path, from, to) {
                            *path = next;
                            tab.update(cx, |view, cx| view.relocate(path, cx));
                        }
                    }
                    _ => {}
                }
                assert!(
                    owner.panes.fill(pane, body).is_ok(),
                    "the existing pane remains present"
                );
            }
        }
        for (old, new, editor) in documents {
            self.close_document(project, &old);
            self.history.forget_editor(editor.entity_id());
            editor.update(cx, |state, _| {
                state.lsp_mut().definition_provider = None;
                state.lsp_mut().completion_provider = None;
            });
            self.open_document(project, &new, &editor, cx);
            self.follow_in_diffs(project, &new, cx);
        }
        for (seat, state) in &mut self.trees {
            if seat.project != project {
                continue;
            }
            let expanded: Vec<_> = state
                .tree
                .expanded()
                .map(|p| file_operations::remap(p, from, to).unwrap_or_else(|| p.into()))
                .collect();
            state.tree = crate::tree::Tree::with_expanded(state.tree.root(), &expanded);
            if let Some(path) = &mut state.selected
                && let Some(next) = file_operations::remap(path, from, to)
            {
                *path = next;
            }
            state.files.remap(|(origin, path)| {
                if *origin == Origin::Project
                    && let Some(next) = file_operations::remap(path, from, to)
                {
                    *path = next;
                }
            });
        }
        self.refresh_blames(project, cx);
        // File/symbol/search popups hold snapshots and background asks. Closing
        // the originating project's popup drops both without touching another seat.
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.project == Some(project))
        {
            self.close_picker(super::picker::Focus::Back, window, cx);
        }
        self.previews.invalidate_path(project, from);
        self.find_previews.invalidate_path(project, from);
        self.relocate_search(project, from, to);
        self.relocate_usages(project, from, to);
        self.declarations.invalidate_subtree(project, from);
        self.history.relocate(project, from, to);
        self.recent_files.relocate(project, from, to);
        self.closed.relocate(project, from, to);
        if let Some((owner, path, _)) = &mut self.landing
            && *owner == project
            && let Some(next) = file_operations::remap(path, from, to)
        {
            *path = next;
        }
        self.diagnostics
            .retain(|(owner, path), _| *owner != project || !path.starts_with(from));
        self.forget_indexes(project);
        self.hear_edits(cx);
        if let Some(root) = self.desk.project(project).map(|p| p.path.clone()) {
            let mut state = guild_session::project_state::load(&root);
            relocate_saved(&mut state, from, to);
            if let Err(error) = guild_session::project_state::save(&root, &state) {
                self.fail(format!("Could not persist moved paths: {error}"));
            }
        }
        self.store_tabs();
        cx.notify();
    }
}

fn relocate_saved(state: &mut guild_session::ProjectState, from: &Path, to: &Path) {
    use guild_session::{Base, OpenTab, Shows};
    let path = |path: &mut PathBuf| {
        if let Some(next) = file_operations::remap(path, from, to) {
            *path = next;
        }
    };
    let shows = |shows: &mut Shows| match shows {
        Shows::File(p) | Shows::Annotate(p) => path(p),
        Shows::Diff {
            path: Some(p),
            base: Base::Head | Base::Unstaged,
        } => path(p),
        _ => {}
    };
    let tabs = |tabs: &mut Vec<OpenTab>| {
        for tab in tabs {
            shows(&mut tab.shows);
            if let Some(panes) = &mut tab.panes {
                for pane in panes.ids() {
                    if let Some(mut value) = panes.take(pane) {
                        shows(&mut value);
                        assert!(panes.fill(pane, value).is_ok());
                    }
                }
            }
        }
    };
    tabs(&mut state.tabs);
    for p in &mut state.expanded {
        path(p);
    }
    for p in &mut state.recent_files {
        path(p);
    }
    for seat in state.seats.values_mut() {
        tabs(&mut seat.tabs);
        for p in &mut seat.expanded {
            path(p);
        }
        if let Some(areas) = &mut seat.areas {
            for strip in areas.strips.values_mut() {
                tabs(&mut strip.tabs);
            }
        }
    }
}

fn clipboard_identity(root: &Path, path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    file_operations::validate_source(root, path)?;
    let metadata = std::fs::symlink_metadata(root.join(path))?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_seats_recent_paths_and_panes_follow_component_boundaries() {
        use guild_session::{OpenTab, SeatState, Shows, WorkspaceId};
        let mut state = guild_session::ProjectState {
            recent_files: vec!["src/file.rs".into(), "src-copy/file.rs".into()],
            expanded: vec!["src".into()],
            tabs: vec![OpenTab::showing(Shows::File("src/file.rs".into()))],
            ..Default::default()
        };
        let workspace = WorkspaceId::new();
        state.seats.insert(
            workspace,
            SeatState {
                tabs: state.tabs.clone(),
                expanded: state.expanded.clone(),
                ..Default::default()
            },
        );
        relocate_saved(&mut state, Path::new("src"), Path::new("dest/src"));
        assert_eq!(
            state.recent_files,
            vec![PathBuf::from("dest/src/file.rs"), "src-copy/file.rs".into()]
        );
        assert_eq!(state.expanded, vec![PathBuf::from("dest/src")]);
        assert_eq!(
            state.seats[&workspace].tabs[0].shows,
            Shows::File("dest/src/file.rs".into())
        );
        assert_eq!(
            state.seats[&workspace].expanded,
            vec![PathBuf::from("dest/src")]
        );
    }
}
