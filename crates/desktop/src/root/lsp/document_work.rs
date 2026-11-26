//! Document-owned deadlines and view-owned lifecycle listeners.
use super::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    rc::Rc,
    time::Instant,
};
type Stamp = (Path_, u64);
#[derive(Default)]
struct Pending {
    next: u64,
    due: BTreeMap<Instant, HashSet<Stamp>>,
}
#[derive(Clone, Default)]
pub(crate) struct DocumentWork(Rc<RefCell<Pending>>);
impl DocumentWork {
    pub fn generation(&self) -> u64 {
        let mut w = self.0.borrow_mut();
        w.next = w.next.wrapping_add(1);
        w.next
    }
    fn remove(&self, key: &Stamp, at: Option<Instant>) {
        if let Some(at) = at {
            let mut w = self.0.borrow_mut();
            if let Some(keys) = w.due.get_mut(&at) {
                keys.remove(key);
                if keys.is_empty() {
                    w.due.remove(&at);
                }
            }
        }
    }
    fn insert(&self, key: Stamp, at: Instant) {
        self.0.borrow_mut().due.entry(at).or_default().insert(key);
    }
    pub fn due(&self, now: Instant) -> Vec<Stamp> {
        let mut w = self.0.borrow_mut();
        let mut due = Vec::new();
        while w.due.first_key_value().is_some_and(|(at, _)| *at <= now) {
            due.extend(w.due.pop_first().unwrap().1);
        }
        due
    }
}
impl Document {
    pub(super) fn changed(&mut self, cx: &gpui::App) {
        if self.editor.read(cx).text() == &self.sent {
            self.disarm();
        } else {
            self.arm();
        }
    }
    pub(super) fn arm(&mut self) {
        let stamp = (self.path.clone(), self.generation);
        self.work
            .remove(&stamp, self.dirty_at.map(|at| at + crate::autosave::PAUSE));
        let at = Instant::now();
        self.dirty_at = Some(at);
        self.work.insert(stamp, at + crate::autosave::PAUSE);
    }
    pub(super) fn disarm(&mut self) {
        self.work.remove(
            &(self.path.clone(), self.generation),
            self.dirty_at.take().map(|at| at + crate::autosave::PAUSE),
        );
    }
}
impl Drop for Document {
    fn drop(&mut self) {
        self.disarm();
    }
}
impl Root {
    /// Registered once per new FileTab; GPUI releases listeners with that view.
    pub(crate) fn watch_file_document(
        &mut self,
        project: ProjectId,
        path: &std::path::Path,
        file: &gpui::Entity<crate::file_tab::FileTab>,
        cx: &mut Context<Self>,
    ) {
        self.link_document_view(project, path, file, cx);
        cx.observe(file, |root, file, cx| {
            root.refresh_document_view(&file, false, cx)
        })
        .detach();
        cx.subscribe(
            file,
            |root, file, _: &crate::file_tab::BufferChanged, cx| {
                root.refresh_document_view(&file, true, cx)
            },
        )
        .detach();
    }
    fn refresh_document_view(
        &mut self,
        file: &gpui::Entity<crate::file_tab::FileTab>,
        changed: bool,
        cx: &mut Context<Self>,
    ) {
        // Live seats alone authorize rebinding: a retained closed view cannot
        // recreate a document, including after rename/project transfer.
        let keys: HashSet<Path_> = self
            .tabs
            .iter()
            .flat_map(|t| {
                t.bodies().filter_map(|b| match b {
                    crate::Body::File {
                        path,
                        tab,
                        origin: crate::Origin::Project,
                        ..
                    } if tab == file => Some((t.project, path.clone())),
                    _ => None,
                })
            })
            .collect();
        for (project, path) in keys {
            let key = (project, path.clone());
            let editor = file.read(cx).editor().cloned();
            let replaced = self.documents.get(&key).is_some_and(|d| {
                d.authority == Some(file.entity_id())
                    && editor
                        .as_ref()
                        .is_none_or(|e| e.entity_id() != d.editor.entity_id())
            });
            if replaced {
                self.close_document(project, &path);
            }
            if let Some(editor) = editor {
                if !self.documents.contains_key(&key) {
                    if self.integrations.external()
                        && let Some(language) = Language::of(&path)
                    {
                        self.ensure_server(project, language, cx);
                    }
                    self.open_document(project, &path, &editor, cx);
                }
                self.link_document_view(project, &path, file, cx);
                if changed
                    && let Some(doc) = self.documents.get_mut(&key)
                    && doc.editor == editor
                {
                    doc.changed(cx);
                }
            }
        }
    }
    pub(super) fn link_document_view(
        &mut self,
        project: ProjectId,
        path: &std::path::Path,
        file: &gpui::Entity<crate::file_tab::FileTab>,
        cx: &mut Context<Self>,
    ) {
        let key = (project, path.to_path_buf());
        let Some(doc) = self.documents.get_mut(&key) else {
            return;
        };
        if doc.views.contains_key(&file.entity_id()) {
            return;
        }
        if file.read(cx).editor().is_some_and(|e| e == &doc.editor) {
            doc.authority.get_or_insert(file.entity_id());
        }
        let generation = doc.generation;
        let saved = cx.subscribe(file, move |root, _, event: &crate::file_tab::Saved, cx| {
            if root
                .documents
                .get(&key)
                .is_none_or(|d| d.generation != generation)
            {
                return;
            }
            let expected = root.desk.project(project).map(|p| p.path.join(&key.1));
            if expected.as_ref() != Some(&event.path) {
                return;
            }
            // Preserve the exact write, even if later typing/reload has already
            // changed the editor by the time this success event is delivered.
            root.send_document_text(&key, &event.text, cx);
            if let Some(doc) = root.documents.get(&key) {
                let language = doc.language;
                if let Some(uri) = root.uri_of(project, &key.1)
                    && let Some(server) = root.servers.get(&(project, language)).filter(|s| s.ready)
                {
                    server.client.notify(
                        "textDocument/didSave",
                        serde_json::json!({"textDocument":{"uri":uri}}),
                    );
                }
            }
            if let Some(doc) = root.documents.get_mut(&key)
                && doc.editor.read(cx).text() != &event.text
            {
                doc.arm();
            }
        });
        doc.views.insert(file.entity_id(), saved);
    }
    pub(crate) fn retain_document_holder(
        &mut self,
        project: ProjectId,
        path: &std::path::Path,
        except: Option<crate::TabId>,
        cx: &mut Context<Self>,
    ) {
        let key = (project, path.to_path_buf());
        let Some(doc) = self.documents.get(&key) else {
            return;
        };
        let views: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| Some(t.id) != except && t.project == project)
            .flat_map(|t| t.bodies())
            .filter_map(|b| match b {
                crate::Body::File {
                    path: other,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if other == path => Some(tab.clone()),
                _ => None,
            })
            .collect();
        let keep = views
            .iter()
            .find(|v| v.read(cx).editor().is_some_and(|e| e == &doc.editor))
            .map(|v| v.entity_id());
        if let Some(authority) = keep {
            let alive: HashSet<_> = views.iter().map(|v| v.entity_id()).collect();
            let doc = self.documents.get_mut(&key).expect("retained");
            doc.views.retain(|id, _| alive.contains(id));
            doc.authority = Some(authority);
            return;
        }
        self.close_document(project, path);
        if let Some((file, editor)) = views
            .into_iter()
            .find_map(|v| v.read(cx).editor().cloned().map(|e| (v, e)))
        {
            self.open_document(project, path, &editor, cx);
            self.link_document_view(project, path, &file, cx);
        }
    }
    pub(super) fn send_document_text(
        &mut self,
        key: &Path_,
        text: &gpui_component::input::Rope,
        _cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.documents.get_mut(key) else {
            return;
        };
        doc.disarm();
        if &doc.sent == text {
            return;
        }
        doc.sent = text.clone();
        doc.version += 1;
        let (version, language) = (doc.version, doc.language);
        if let Some(uri) = self.uri_of(key.0, &key.1)
            && let Some(server) = self.servers.get(&(key.0, language)).filter(|s| s.ready)
        {
            server.client.notify(
                "textDocument/didChange",
                crate::lsp::document::changed(&uri, version, &text.to_string()),
            );
        }
    }
}
