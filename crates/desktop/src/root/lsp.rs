//! A language server per project and language, and what it says.
//!
//! The client is [`crate::lsp::client`]; what lives here is when one is
//! started, what it is told, and what the window does with its answers.
//!
//! **Started when a file wants it, stopped with the project.** Task 0
//! measured rust-analyzer at 3.5 GB resident on this repository: one per
//! project and language, never eagerly, and never outliving what asked
//! for it.

use crate::editor::EditorReveal as _;
use std::path::Path;

use gpui::Context;
use guild_session::ProjectId;
use serde_json::{Value, json};

use crate::Root;
use crate::highlight::Language;
use crate::lsp::Incoming;
use crate::lsp::client::{Client, Request};
use crate::lsp::progress::{Fault, Line, Task, Work};

/// A server the window is talking to, and how far it has got.
pub(crate) struct Server {
    /// The process and the protocol.
    pub client: Client,
    /// The id of the `initialize` request while it is out.
    initialize: Option<Request>,
    /// Whether it has answered `initialize` and been told `initialized`.
    pub ready: bool,
    /// What it says it is doing, from `$/progress` and the two
    /// extensions M8.5.1 reads — *Indexing*, *cargo check*, *Importing*.
    /// **This is what the status bar shows** until the first diagnostics
    /// land: task 0 measured 124 seconds from a cold start to the first of
    /// them, and a window that says nothing meanwhile is telling a person
    /// nothing is wrong when nobody has looked.
    pub work: Work,
}

/// Which server: one per project and language.
pub(crate) type Key = (ProjectId, &'static str);

/// Window identity and transport lifetime travel together, with no token map.
#[derive(Debug)]
pub(crate) struct ServerRequest {
    key: Key,
    generation: u64,
    request: Request,
}

impl ServerRequest {
    pub(super) fn cancellation(&self) -> crate::lsp::client::Cancellation {
        self.request.cancellation()
    }
}

impl Root {
    /// The server for this language in this project, started if there is
    /// not one yet and the language names one.
    ///
    /// A command that will not start is remembered as *tried*: a project
    /// with no rust-analyzer must not spawn one per file opened.
    pub(crate) fn ensure_server(
        &mut self,
        project: ProjectId,
        language: &Language,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        if self.test_server.is_none() {
            return;
        }
        self.resolve_project_identity(project, true, cx);
        if !self.lsp_permitted(project, language.name) {
            return;
        }
        let Some(_command) = language.server.first() else {
            return;
        };
        let key: Key = (project, language.name);
        if self.servers.contains_key(&key) || self.serverless.contains_key(&key) {
            return;
        }
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        // Remembered like a missing command: until the project is closed,
        // and the status bar names the files that would start it.
        if !language.server_starts_in(&root) {
            tracing::info!(language = %language.name, "the project does not describe itself to this server");
            self.serverless
                .insert(key, Fault::Unprepared(language.server_needs));
            return;
        }

        #[cfg(not(test))]
        let args: Vec<String> = language.server[1..]
            .iter()
            .map(|a| (*a).to_owned())
            .collect();
        #[cfg(test)]
        let (command, args) = self
            .test_server
            .as_ref()
            .or(self.test_launch_fallback.as_ref())
            .map_or(("/usr/bin/false", Vec::new()), |(command, args)| {
                (command.as_str(), args.clone())
            });

        #[cfg(not(test))]
        let command = *_command;
        match Client::start(command, &args, &root) {
            Ok(mut client) => {
                *self.scoped_settings.generations.entry(key).or_default() += 1;
                let initialize = client.request("initialize", initialize_params(&root));
                tracing::info!(server = %command, project = %root.display(), "a language server started");
                self.servers.insert(
                    key,
                    Server {
                        client,
                        initialize: Some(initialize),
                        ready: false,
                        work: Work::default(),
                    },
                );
            }
            Err(err) => {
                // Once per project and language, not per file: the log is
                // where this belongs, and a notice about a server nobody
                // asked for is an editor nagging about what it could
                // install for you.
                tracing::info!(%err, language = %language.name, "no language server here");
                self.serverless.insert(key, Fault::Missing);
            }
        }
    }

    /// Ask every server what it has said, on the watch loop's clock.
    pub(crate) fn poll_servers(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<Key> = self.servers.keys().copied().collect();
        let mut changed = false;
        // Collected rather than applied in the loop: applying reaches
        // into the tabs, and the server is borrowed here.
        let mut published: Vec<(ProjectId, Value)> = Vec::new();
        // Servers that have just agreed to talk, and so are owed every
        // file already open in their language.
        let mut ready: Vec<Key> = Vec::new();
        for key in keys {
            let Some(server) = self.servers.get_mut(&key) else {
                continue;
            };
            if !server.client.is_alive() {
                let reason = server.client.failure_reason();
                tracing::warn!(server = %server.client.name(), reason = ?reason, "the language server has gone");
                self.servers.remove(&key);
                // **Three deaths and it is not started again.** The
                // window starts one whenever a file wants one, so a
                // server that dies as it starts would be spawned once a
                // frame for ever — a restart loop with a process in it.
                // Three is enough for a person restarting a toolchain
                // and few enough to notice.
                let deaths = self.server_deaths.entry(key).or_insert(0);
                *deaths += 1;
                if *deaths >= 3 {
                    tracing::warn!(language = %key.1, "its server keeps dying; not starting it again");
                    self.serverless.insert(
                        key,
                        reason
                            .filter(|r| r != "LSP output closed")
                            .map_or(Fault::Dying, Fault::Refused),
                    );
                }
                // **And its documents are forgotten with it.** A server
                // knows only what it has been told; the next one to start
                // must be told again, and `ensure_documents` does that
                // for every file whose buffer exists. Without this a
                // restarted server would sit there knowing nothing about
                // the files already open, which looks exactly like a
                // server that is not working.
                self.documents.retain(|(owner, path), _| {
                    *owner != key.0
                        || Language::of(path).is_none_or(|language| language.name != key.1)
                });
                changed = true;
                continue;
            }
            // The answer to `initialize` is the one thing that must be
            // taken before anything else is sent: a server told
            // `didOpen` before it has answered is a server that has not
            // agreed to anything yet.
            if let Some(request) = &server.initialize
                && let Some(answer) = request.take_answer()
            {
                server.initialize = None;
                match answer {
                    Ok(_) => {
                        server.client.notify("initialized", json!({}));
                        server.ready = true;
                        ready.push(key);
                        changed = true;
                    }
                    Err(refusal) => {
                        tracing::warn!(%refusal, "the language server refused to initialize");
                        // **Marked as tried**, M8.5.1: without it the next
                        // render wanted the server again and started it
                        // again, once a frame.
                        self.servers.remove(&key);
                        self.serverless.insert(key, Fault::Refused(refusal));
                        changed = true;
                        continue;
                    }
                }
            }

            for message in server.client.poll() {
                match message {
                    Incoming::Notification { method, params } => match method.as_str() {
                        "textDocument/publishDiagnostics" => {
                            published.push((key.0, params));
                            changed = true;
                        }
                        _ => {
                            if server.work.absorb(&method, &params) {
                                changed = true;
                            } else {
                                tracing::debug!(%method, "the server said something");
                            }
                        }
                    },
                    // **Answered, always.** A server whose request goes
                    // unanswered waits: `workspace/configuration` with no
                    // reply leaves rust-analyzer without settings and it
                    // says nothing about it. Null is a legal answer to
                    // every request we can receive without having asked
                    // for the capability behind it.
                    Incoming::Request { id, method, .. } => {
                        tracing::debug!(%method, "the server asked us something");
                        server.client.answer(id, Value::Null);
                    }
                    Incoming::Answer { .. } => {}
                }
            }
        }
        for key in ready {
            self.announce_documents(key, cx);
        }
        for (project, params) in published {
            self.diagnostics_arrived(project, params, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Stop every server of a project — it is closing.
    pub(crate) fn stop_servers_of(&mut self, project: ProjectId) {
        self.documents.retain(|(owner, _), _| *owner != project);
        self.servers.retain(|(owner, _), _| *owner != project);
        self.serverless.retain(|(owner, _), _| *owner != project);
        self.xcode_setups.remove(&project);
    }

    /// Stop every server there is: the window is going.
    pub(crate) fn stop_every_server(&mut self) {
        self.documents.clear();
        self.servers.clear();
    }
}

/// What we tell a server we can do.
///
/// Only what the window actually draws: the component paints diagnostics,
/// a hover popover, a completion menu and a definition link, the Find tool
/// lists references, and claiming
/// more than that is how a client comes to be sent things it drops.
fn initialize_params(root: &Path) -> Value {
    let uri = format!("file://{}", root.display());
    json!({
        "processId": std::process::id(),
        "rootUri": uri,
        "workspaceFolders": [{"uri": uri, "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()}],
        "clientInfo": {"name": "Guild"},
        "capabilities": {
            "textDocument": {
                // **`didSave` is sent**, and it has to be: rust-analyzer
                // runs `cargo check` on save, and a client that never
                // saves gets the diagnostics it computed at startup and
                // nothing after.
                "synchronization": {"didSave": true},
                "publishDiagnostics": {},
                "hover": {"contentFormat": ["markdown", "plaintext"]},
                "completion": {"completionItem": {"snippetSupport": false}},
                "definition": {"linkSupport": true},
                // M8.15's Find tool. Optional in the protocol and answered
                // without it by every server task 0 asked, but a client
                // says what it will send.
                "references": {}
            },
            "window": {"workDoneProgress": true}
        }
    })
}

/// A file the server has been told about.
#[cfg(any(test, feature = "native-preview"))]
pub(crate) mod document_trace {
    thread_local! {static COUNTS:std::cell::Cell<[usize;2]>=const{std::cell::Cell::new([0,0])};}
    pub fn note(slot: usize) {
        COUNTS.with(|c| {
            let mut v = c.get();
            v[slot] += 1;
            c.set(v);
        });
    }
    pub fn take() -> [usize; 2] {
        COUNTS.with(|c| c.replace([0, 0]))
    }
}

mod document_work;
pub(crate) use document_work::DocumentWork;

pub(crate) struct Document {
    /// Which language's server holds it — the key's other half.
    pub(super) language: &'static str,
    /// The version the protocol counts, one up per change sent.
    version: i32,
    sent: gpui_component::input::Rope,
    /// The buffer, to read when the pause ends.
    editor: gpui::Entity<gpui_component::input::EditorState>,
    /// When it last changed, while a change is waiting to be sent.
    dirty_at: Option<std::time::Instant>,
    path: Path_,
    generation: u64,
    work: DocumentWork,
    authority: Option<gpui::EntityId>,
    views: std::collections::HashMap<gpui::EntityId, gpui::Subscription>,
    /// The subscription that hears it change. Dropped with the document,
    /// which is how the window stops listening to a closed tab.
    _heard: gpui::Subscription,
}

/// Which document: a path in a project.
pub(crate) type Path_ = (ProjectId, std::path::PathBuf);

impl Root {
    /// Tell the server about a file that has just been opened.
    ///
    /// The **buffer**, not the disk — decision 6: the server must see
    /// what the person sees, or its diagnostics are about a file nobody
    /// is looking at.
    pub(crate) fn open_document(
        &mut self,
        project: ProjectId,
        relative: &std::path::Path,
        editor: &gpui::Entity<gpui_component::input::EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(language) = Language::of(relative) else {
            return;
        };
        if language.server.is_empty() || !self.lsp_permitted(project, language.name) {
            return;
        }
        let key: Path_ = (project, relative.to_path_buf());
        if self.documents.contains_key(&key) {
            return;
        }
        let Some(uri) = self.uri_of(project, relative) else {
            return;
        };

        let text = editor.read(cx).text().to_string();
        let generation = self.document_work.generation();
        let heard = {
            let key = key.clone();
            cx.subscribe(
                editor,
                move |root, _editor, event: &gpui_component::input::InputEvent, cx| {
                    if matches!(event, gpui_component::input::InputEvent::Change)
                        && let Some(document) = root.documents.get_mut(&key)
                        && document.generation == generation
                    {
                        // Marked rather than sent: the watch loop sends it
                        // after the pause, so that a fast typist does not
                        // hand a server a hundred whole files a second.
                        document.changed(cx);
                    }
                },
            )
        };

        self.documents.insert(
            key.clone(),
            Document {
                language: language.name,
                version: 1,
                sent: editor.read(cx).text().clone(),
                editor: editor.clone(),
                dirty_at: None,
                path: key,
                generation,
                work: self.document_work.clone(),
                authority: None,
                views: Default::default(),
                _heard: heard,
            },
        );

        // The slots the component leaves for a server, filled with one
        // provider: the same file, the same server.
        //
        // **Not the hover slot — 2026-09-18.** The component opens it on
        // any pointer resting on a symbol, 150 ms later, and a page of
        // documentation then covers the code being read; the person asked
        // for it not to appear unless asked for. A mistake's
        // own popover is the diagnostics' and is not this slot, so it
        // stays. `HoverProvider` is still implemented for the day the
        // documentation is asked for by a stroke.
        let provider = std::rc::Rc::new(crate::lsp::providers::Provider::of(
            cx.weak_entity(),
            project,
            relative.to_path_buf(),
            language.name,
        ));
        editor.update(cx, |state, _cx| {
            state.lsp_mut().definition_provider = Some(provider.clone());
            state.lsp_mut().completion_provider = Some(provider);
        });

        // Whatever the server has already said about this file: it
        // publishes for files nobody has open, and LSP has no way to ask
        // again.
        self.show_diagnostics(project, relative, cx);

        // **Only to a server that has answered `initialize`.** Anything
        // sent before that is a protocol violation, and rust-analyzer
        // does not shrug it off — it prints *expected initialized
        // notification, got …* on stderr and **exits**. The rest are
        // announced by `announce_documents` the moment the server is
        // ready.
        if self
            .servers
            .get(&(project, language.name))
            .is_some_and(|server| server.ready)
            && let Some(server) = self.servers.get(&(project, language.name))
        {
            server.client.notify(
                "textDocument/didOpen",
                crate::lsp::document::opened(&uri, language.name, 1, &text),
            );
        }
    }

    /// Tell a server that has just become ready about every file of its
    /// language that is already open.
    ///
    /// **This is what a restart needs.** A server started while files are
    /// open knows nothing about them until it is told, and it may not be
    /// told before it has answered `initialize`.
    fn announce_documents(&mut self, key: Key, cx: &mut Context<Self>) {
        let open: Vec<(std::path::PathBuf, i32, String)> = self
            .documents
            .iter_mut()
            .filter(|((project, _), document)| *project == key.0 && document.language == key.1)
            .map(|((_, path), document)| {
                let text = document.editor.read(cx).text().clone();
                document.disarm();
                document.sent = text.clone();
                (path.clone(), document.version, text.to_string())
            })
            .collect();
        for (path, version, text) in open {
            let Some(uri) = self.uri_of(key.0, &path) else {
                continue;
            };
            if let Some(server) = self.servers.get(&key) {
                server.client.notify(
                    "textDocument/didOpen",
                    crate::lsp::document::opened(&uri, key.1, version, &text),
                );
            }
        }
    }

    /// The tab has closed: the server stops hearing about the file.
    pub(crate) fn close_document(&mut self, project: ProjectId, relative: &std::path::Path) {
        let key: Path_ = (project, relative.to_path_buf());
        let Some(document) = self.documents.remove(&key) else {
            return;
        };
        let Some(uri) = self.uri_of(project, relative) else {
            return;
        };
        if let Some(server) = self.servers.get(&(project, document.language)) {
            server.client.cancel_document(&uri);
            server
                .client
                .notify("textDocument/didClose", crate::lsp::document::closed(&uri));
        }
    }

    /// Send what has changed, once the typing has paused.
    ///
    /// On the watch loop's clock, which is 100 ms, against the same
    /// 300 ms pause autosave waits for — one rule about when a person has
    /// stopped typing, not two.
    pub(crate) fn flush_documents(&mut self, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        for (key, generation) in self.document_work.due(now) {
            let Some(doc) = self
                .documents
                .get(&key)
                .filter(|d| d.generation == generation)
            else {
                continue;
            };
            #[cfg(any(test, feature = "native-preview"))]
            document_trace::note(1);
            let text = doc.editor.read(cx).text().clone();
            self.send_document_text(&key, &text, cx);
        }
    }

    /// The `file://` URI of a file of a project, as the protocol wants
    /// it.
    pub(crate) fn uri_of(&self, project: ProjectId, relative: &std::path::Path) -> Option<String> {
        let root = self.desk.project(project).map(|p| p.path.clone())?;
        Some(crate::lsp::uri(&root.join(relative)))
    }
}

impl Root {
    /// What a server says is wrong with a file.
    ///
    /// **Kept whether or not a tab is open for it.** A server publishes
    /// for every file it has an opinion about, and LSP has no way to ask
    /// again — a tab opened a second later would otherwise show nothing
    /// until the next edit.
    fn diagnostics_arrived(&mut self, project: ProjectId, params: Value, cx: &mut Context<Self>) {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return;
        };
        let Some(path) = crate::lsp::path_of(uri) else {
            return;
        };
        let found: Vec<lsp_types::Diagnostic> = params
            .get("diagnostics")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();

        // Keep the server's project identity, including overlapping checkouts.
        let Some(root) = self.desk.project(project) else {
            return;
        };
        let Ok(relative) = path
            .strip_prefix(&root.path)
            .map(std::path::Path::to_path_buf)
        else {
            return;
        };
        if !path.exists() && !self.documents.contains_key(&(project, relative.clone())) {
            return;
        }
        self.diagnostics.insert((project, relative.clone()), found);
        self.show_diagnostics(project, &relative, cx);
    }

    /// The project a path belongs to, and the path within it: the deepest
    /// open root containing it ([`crate::column::owning_root`]).
    pub(crate) fn project_of(
        &self,
        path: &std::path::Path,
    ) -> Option<(ProjectId, std::path::PathBuf)> {
        crate::column::owning_root(
            self.desk
                .projects()
                .map(|project| (project.id, project.path.as_path())),
            path,
        )
    }

    /// Give a file's diagnostics to the editor that is showing it.
    ///
    /// The component draws them as wavy underlines and resets them on
    /// every edit — which is right: a squiggle on a line that has moved
    /// is a lie, and the server answers again within a moment.
    pub(crate) fn show_diagnostics(
        &mut self,
        project: ProjectId,
        relative: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self
            .diagnostics
            .get(&(project, relative.to_path_buf()))
            .cloned()
        else {
            return;
        };
        let editors: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if path == relative => tab.read(cx).editor().cloned(),
                _ => None,
            })
            .collect();
        for editor in editors {
            editor.update(cx, |state, cx| {
                let text = state.text().clone();
                if let Some(set) = state.diagnostics_mut() {
                    set.reset(&text);
                    set.extend(found.clone());
                }
                cx.notify();
            });
        }
    }

    /// Tell the server about every file tab that has a buffer and has
    /// not been mentioned yet.
    ///
    /// **A restored tab has no editor until it is first drawn** — the
    /// same fact the autosave table met when it read "no editor" as "not
    /// text". So the telling cannot happen where the tab is made; it
    /// happens here, on the window's render, where the buffer either
    /// exists or does not yet.
    pub(crate) fn ensure_documents(&mut self, cx: &mut Context<Self>) {
        if !self.integrations.external() {
            return;
        }
        let waiting: Vec<(
            ProjectId,
            std::path::PathBuf,
            gpui::Entity<gpui_component::input::EditorState>,
            gpui::Entity<crate::file_tab::FileTab>,
        )> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.bodies().map(move |body| (tab.project, body)))
            .filter_map(|(project, body)| match body {
                // **A dependency's file is told to no server.** Its path
                // is relative to the crate's root, and a URI built from
                // the project's would name a file in this repository.
                // M8.8.3.
                crate::Body::File {
                    path,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } => {
                    let editor = view.read(cx).editor().cloned()?;
                    (!self.documents.contains_key(&(project, path.clone()))).then_some((
                        project,
                        path.clone(),
                        editor,
                        view.clone(),
                    ))
                }
                _ => None,
            })
            .collect();
        for (project, path, editor, view) in waiting {
            // The server first: a file whose language has one but whose
            // server has gone — killed from outside, or crashed — starts
            // it again here, which is the only place that sees both.
            if let Some(language) = Language::of(&path) {
                self.ensure_server(project, language, cx);
            }
            self.open_document(project, &path, &editor, cx);
            self.link_document_view(project, &path, &view, cx);
        }
    }

    /// A lease owns both the caller's metadata and the client's pending entry.
    pub(crate) fn ask_server(
        &mut self,
        project: ProjectId,
        language: &'static str,
        method: &str,
        params: Value,
    ) -> Option<ServerRequest> {
        let key = (project, language);
        let server = self.servers.get_mut(&key)?;
        if !server.ready {
            return None;
        }
        Some(ServerRequest {
            key,
            generation: self
                .scoped_settings
                .generations
                .get(&key)
                .copied()
                .unwrap_or_default(),
            request: server.client.request(method, params),
        })
    }

    /// Old generations cannot consume a replacement server's protocol IDs.
    pub(crate) fn server_answer(
        &self,
        project: ProjectId,
        language: &'static str,
        lease: &ServerRequest,
    ) -> Option<Result<Value, String>> {
        if lease.key != (project, language)
            || !self.servers.contains_key(&lease.key)
            || !self.lsp_permitted(project, language)
            || self
                .scoped_settings
                .generations
                .get(&lease.key)
                .copied()
                .unwrap_or_default()
                != lease.generation
        {
            lease.request.cancel();
        }
        lease.request.take_answer()
    }

    /// Whether that server is still there — a waiter has to find out.
    pub(crate) fn server_alive(&self, project: ProjectId, language: &'static str) -> bool {
        self.servers
            .get(&(project, language))
            .is_some_and(|server| server.client.is_alive())
    }

    /// The file the window is showing, if it is showing one: which
    /// project, and its path within it.
    pub(crate) fn file_in_front(&self) -> Option<(ProjectId, std::path::PathBuf)> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active_tab)?;
        match tab.body()? {
            crate::Body::File {
                path,
                origin: crate::Origin::Project,
                ..
            } => Some((tab.project, path.clone())),
            _ => None,
        }
    }

    // **The per-file counter went in M8.8.5.** It answered one question
    // — how many mistakes are in the file in front — for a number in the
    // status bar beside the language, and that number is now the
    // Problems button, which counts the whole project because that is
    // what pressing it opens. `problems::counts` is the one counter; a
    // second one over the same map is how two places come to disagree.

    /// What the server of this file's language is doing, if it has one —
    /// *Indexing*, *cargo check* — or that there is no server at all.
    pub(crate) fn server_state(
        &self,
        project: ProjectId,
        relative: &std::path::Path,
    ) -> ServerState {
        let Some(language) = Language::of(relative) else {
            return ServerState::None;
        };
        if language.server.is_empty() {
            return ServerState::None;
        }
        let key = (project, language.name);
        if let Some(state) = self.identity_server_state(project) {
            return state;
        }
        if self
            .repository_identity(project)
            .is_some_and(|identity| !self.settings.lsp_enabled(&identity.key, language.name))
        {
            return ServerState::Disabled;
        }
        match self.servers.get(&key) {
            Some(server) => state_of(server),
            None => match self.serverless.get(&key) {
                Some(fault) => ServerState::Failed(fault.clone()),
                None => ServerState::None,
            },
        }
    }

    fn identity_server_state(&self, project: ProjectId) -> Option<ServerState> {
        match self.scoped_settings.identities.get(&project) {
            None => Some(ServerState::IdentityPending),
            Some(Err(error)) => Some(ServerState::IdentityFailed(error.clone())),
            Some(Ok(_)) => None,
        }
    }

    /// Every server of a project, running or not, for the list the status
    /// bar opens. In the order of the language table, so the list does not
    /// shuffle between frames.
    pub(crate) fn servers_of(&self, project: ProjectId) -> Vec<ServerRow> {
        Language::ALL
            .iter()
            .filter_map(|language| {
                let command = *language.server.first()?;
                let key = (project, language.name);
                let (state, tasks) = if let Some(state) = self.identity_server_state(project) {
                    (state, Vec::new())
                } else if self.repository_identity(project).is_some_and(|identity| {
                    !self.settings.lsp_enabled(&identity.key, language.name)
                }) {
                    (ServerState::Disabled, Vec::new())
                } else {
                    match self.servers.get(&key) {
                        Some(server) => (state_of(server), server.work.tasks().cloned().collect()),
                        None => (
                            self.serverless
                                .get(&key)
                                .map(|f| ServerState::Failed(f.clone()))
                                .unwrap_or(ServerState::None),
                            Vec::new(),
                        ),
                    }
                };
                Some(ServerRow {
                    language: language.name,
                    command,
                    state,
                    tasks,
                })
            })
            .collect()
    }
}

fn state_of(server: &Server) -> ServerState {
    match server.work.headline() {
        Some(line) => ServerState::Doing(line),
        None if server.ready => ServerState::Ready,
        None => ServerState::Starting,
    }
}

/// One server in the list the status bar opens.
pub(crate) struct ServerRow {
    /// The language it reads.
    pub language: &'static str,
    /// What was run.
    pub command: &'static str,
    /// How it is.
    pub state: ServerState,
    /// What it is doing, newest first.
    pub tasks: Vec<Task>,
}

/// What the status bar says about a file's server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServerState {
    /// The language has none, or it has not been wanted yet.
    None,
    /// Deliberately disabled by effective policy.
    Disabled,
    /// Automatic launches await confirmed repository identity.
    IdentityPending,
    /// Discovery failed; no fallback policy key is invented.
    IdentityFailed(String),
    /// Started, and not yet agreed with.
    Starting,
    /// Working: the task in front, and how many are behind it.
    Doing(Line),
    /// It names a server, and none is running. M8.5.1.
    Failed(Fault),
    /// Listening.
    Ready,
}

impl Root {
    /// Go to where the name under the caret is defined.
    ///
    /// **The jump is ours** — decision 8: the component's own
    /// `go_to_definition` moves the caret inside the buffer it is in and
    /// opens a URL for `http`, so every jump into another file has to be
    /// made here, where tabs are.
    pub(crate) fn go_to_definition(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        // **The place Back returns to is noted at the ask**, M5.11's
        // decision 10. `f12` did not need it: the caret had rested on the
        // name, so the 100 ms sampler had already seen it there. A
        // `cmd`-click put the caret on the name milliseconds ago, and the
        // sampler's last place is wherever it was before the click — so
        // Back would skip the name. Noted here, both gestures leave the
        // same place behind.
        self.note_place(cx);
        let Some((project, path)) = self.file_in_front() else {
            return;
        };
        let Some(language) = Language::of(&path) else {
            return;
        };
        if language.server.is_empty() {
            // **A sentence, not the status bar's label.** *no server* is
            // a word for a bar with a language beside it; on its own, in
            // a card in the corner, it is two words about nothing. It
            // names the language, because that is the fact — Go has no
            // server here, this file is fine.
            let said = self.ui().no_server_for(language.name);
            self.inform(said);
            return;
        }
        let Some(editor) = self.editor_of(project, &path, cx) else {
            return;
        };
        let (text, offset) =
            editor.read_with(cx, |state, _cx| (state.text().clone(), state.cursor()));

        // **Not through the provider.** `Provider::ask` updates the root
        // to reach the client, and we are inside the root's own update
        // here — GPUI refuses that with *cannot update Root while it is
        // already being updated*, and the window goes down on the first
        // `f12`. The component may call a provider because the component
        // is a different entity; the window must ask for itself.
        let params = serde_json::json!({
            "textDocument": {"uri": self.uri_of(project, &path)},
            "position": crate::lsp::providers::position_at(&text, offset),
        });
        let Some(id) = self.ask_server(project, language.name, "textDocument/definition", params)
        else {
            return;
        };
        let name = language.name;
        cx.spawn_in(window, async move |root, cx| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let taken = root
                    .update(cx, |root, _cx| root.server_answer(project, name, &id))
                    .ok()
                    .flatten();
                match taken {
                    Some(Ok(answer)) => {
                        let links = crate::lsp::providers::links_in(answer);
                        let Some(link) = links.first().cloned() else {
                            return;
                        };
                        root.update_in(cx, |root, window, cx| {
                            if root.active_project == Some(project)
                                && root.editor_of(project, &path, cx).is_some_and(|current| {
                                    current.entity_id() == editor.entity_id()
                                })
                            {
                                root.land_on(&link, window, cx);
                            }
                        })
                        .ok();
                        return;
                    }
                    // The server refused, or would not answer in time:
                    // the gesture says nothing rather than lying about
                    // where a name is defined.
                    Some(Err(refusal)) => {
                        tracing::info!(%refusal, "the server would not say where that is");
                        return;
                    }
                    None if std::time::Instant::now() > until => return,
                    None => {}
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    }

    /// Open where a link points and put the caret there — `f12`'s
    /// landing, and since M8.15 a Find tab's usage's.
    pub(crate) fn land_on(
        &mut self,
        link: &lsp_types::LocationLink,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let uri = link.target_uri.to_string();
        let Some(path) = crate::lsp::path_of(&uri) else {
            // **A `jar:` is a dependency of Kotlin or Java** — M8.11 — and
            // the source is usually in the archive beside it. `https:` and
            // the rest are still a notice: a tab called
            // `jar:/rt.jar!/String.class` is worse than being told.
            self.land_in_jar(&uri, link.target_selection_range.start, window, cx);
            return;
        };
        let Some((project, relative)) = self.project_of(&path) else {
            // A definition in the standard library or in a crate under
            // `~/.cargo` — the commonest jump there is. **Opened, since
            // M8.8.3**, under the dependency's own root: a file tab is
            // still a path within *something that can move*, and for
            // this file that something is the crate's directory rather
            // than the project's.
            self.land_outside(
                &path,
                crate::root::history::Caret::Protocol(link.target_selection_range.start),
                window,
                cx,
            );
            return;
        };

        self.land_at(
            project,
            relative,
            link.target_selection_range.start,
            window,
            cx,
        );
    }

    /// Open the source of a class inside a jar — M8.11.
    ///
    /// **Under a root of its own**, as every dependency's file is since
    /// M8.8.3: the archive is opened under Guild's own state directory
    /// and External Libraries holds it. Nothing is written into the
    /// person's Gradle or Maven cache.
    fn land_in_jar(
        &mut self,
        uri: &str,
        at: lsp_types::Position,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = crate::jars::parse(uri) else {
            self.inform(self.ui().notice_definition_elsewhere(uri));
            return;
        };
        let Some(project) = self.active_project else {
            self.inform(self.ui().notice_definition_elsewhere(uri));
            return;
        };
        let named = entry
            .jar
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| entry.jar.display().to_string());
        let Some(landed) = crate::jars::open_source(uri, &crate::jars::sources_dir()) else {
            // **The true thing to say**: the class is there and its
            // source is not, because nobody downloaded it. *Defined
            // outside this project* sent a person looking for a setting
            // of ours.
            self.inform(self.ui().no_downloaded_sources(&named));
            return;
        };
        let origin = self.note_library(project, &landed.root);
        self.go_to_library(
            origin,
            &landed.relative,
            Some(crate::root::history::Caret::Protocol(at)),
            window,
            cx,
        );
    }

    /// Open a file of a project and put the caret at a place in it.
    ///
    /// The tail of `land_on`, named because the Problems panel takes the
    /// same route: a row there is a place a server named, exactly as a
    /// definition is.
    pub(crate) fn land_at(
        &mut self,
        project: ProjectId,
        relative: std::path::PathBuf,
        at: lsp_types::Position,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_file_landings();
        if Some(project) != self.active_project {
            self.select_project(project, cx);
        }
        self.open_file(&relative, window, cx);
        // The caret goes where the editor is, which for a tab being
        // opened is now and for a restored one is its first render.
        self.landing = Some((project, relative, at));
        self.place_caret(window, cx);
    }

    /// A definition outside every project: open it under the dependency
    /// it belongs to, or say why it cannot be opened.
    ///
    /// **The root is found, not guessed** — the nearest ancestor holding
    /// the language's manifest, `highlight::dependency_root`. Two
    /// answers are honest here and only two: a tab under a root a person
    /// would recognise by name (`serde-1.0.228`), or a sentence. A tab
    /// under a root invented from the parent directory would be neither.
    ///
    /// Where the language ships its dependencies compiled — a Kotlin or
    /// Java jar — there is no manifest above the file and nothing to
    /// open, and it says so in the shape M8.5 uses for a language with
    /// no server. **We decompile nothing**, and a node listing class
    /// files nobody can read would be a worse answer than the sentence.
    ///
    /// The file is opened, and the caret placed, by `go_to_library` — which
    /// leaves the place `f12` was pressed from first, so Back comes back to
    /// it.
    pub(crate) fn land_outside(
        &mut self,
        path: &std::path::Path,
        caret: crate::root::history::Caret,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let said = path.display().to_string();
        let Some(project) = self.active_project else {
            self.inform(self.ui().notice_definition_elsewhere(&said));
            return;
        };
        // A Swift module's interface, which sourcekit-lsp has just written
        // under a hash in the system's temporary directory: copied under
        // the module's name, as a jar's source is.
        if let Some(landed) = crate::sourcekit::interface(path, &crate::jars::sources_dir()) {
            let origin = self.note_library(project, &landed.root);
            self.go_to_library(origin, &landed.relative, Some(caret), window, cx);
            return;
        }
        let language = crate::highlight::Language::of(path);
        // The manifest's rule first; where it finds nothing, the bundle a
        // header is in — an Objective-C framework Xcode built, which
        // sourcekit-lsp answers a name from with its header.
        let root = language
            .and_then(|language| crate::highlight::dependency_root(path, language))
            .or_else(|| crate::sourcekit::framework_root(path));
        let Some(root) = root else {
            let text = match language {
                Some(language) => self.ui().no_sources_for(language.name),
                None => self.ui().notice_definition_elsewhere(&said),
            };
            self.inform(text);
            return;
        };
        let Ok(relative) = path.strip_prefix(&root) else {
            self.inform(self.ui().notice_definition_elsewhere(&said));
            return;
        };
        let relative = relative.to_path_buf();
        let origin = self.note_library(project, &root);
        self.go_to_library(origin, &relative, Some(caret), window, cx);
    }

    /// Put the caret on a landing, if its editor exists yet.
    pub(crate) fn place_caret(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some((project, path, at)) = self.landing.clone() else {
            return;
        };
        if !self.can_land_in_file(project, &path, cx) {
            self.landing = None;
            return;
        }
        let Some(editor) = self.editor_of(project, &path, cx) else {
            return;
        };
        self.landing = None;
        self.reveal_markdown_source(window, cx);
        editor.update(cx, |state, cx| {
            let text = state.text().to_string();
            let (line, column) = crate::lsp::providers::caret_for(&text, at);
            // Centred when off the screen — our
            // adapter over the component's public layout and scroll API.
            state.set_cursor_position_centred(
                gpui_component::input::Position::new(line, column),
                window,
                cx,
            );
        });
    }

    /// The editor of a file tab, if it has one yet.
    pub(crate) fn editor_of(
        &self,
        project: ProjectId,
        relative: &std::path::Path,
        cx: &Context<Self>,
    ) -> Option<gpui::Entity<gpui_component::input::EditorState>> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                crate::Body::File {
                    path,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } if path == relative => view.read(cx).editor().cloned(),
                _ => None,
            })
    }
}

#[cfg(test)]
mod relocation_tests {
    use super::*;
    #[gpui::test]
    fn late_diagnostics_for_moved_paths_are_discarded(cx: &mut gpui::TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::late_diagnostics_for_moved_paths_are_discarded"
        )) {
            return;
        }
        use gpui::AppContext;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.rs"), "fn main() {}").unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default(); desk.normalise();
            let project = desk.open_in(desk.active_workspace(), dir.path());
            let root = cx.new(|cx| Root::from_state(guild_session::Settings{panels:Some(Default::default()),..Default::default()},desk,cx));
            std::fs::rename(dir.path().join("old.rs"),dir.path().join("new.rs")).unwrap();
            root.update(cx, |r,cx| {
                r.diagnostics_arrived(project, serde_json::json!({"uri":crate::lsp::uri(&dir.path().join("old.rs")),"diagnostics":[]}),cx);
                assert!(!r.diagnostics.contains_key(&(project,"old.rs".into())),"late server messages must not restore the old Problems path");
            });
        });
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;
    use crate::lsp_fixture::LspFixture;
    use gpui::{AppContext, Entity, Focusable, TestAppContext};
    use std::{
        path::Path,
        time::{Duration, Instant},
    };

    // A subview or a failed test can retain its App. Cleanup must not depend on
    // the gpui::test macro's trailing quit, which unwinding skips.
    struct RootGuard {
        root: Option<Entity<Root>>,
        cx: TestAppContext,
    }
    impl Drop for RootGuard {
        fn drop(&mut self) {
            if let Some(root) = self.root.take() {
                self.cx
                    .update(|cx| root.update(cx, |r, cx| r.stop_every_process(cx)));
            }
            // Ordinary saves must finish before a fixture's directory is
            // removed, or their failure notice can cover the next test's UI.
            guild_session::flush();
        }
    }
    fn root(cx: &mut TestAppContext, path: &Path) -> RootGuard {
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), path);
            cx.new(|cx| {
                let mut root = Root::from_state(
                    guild_session::Settings {
                        panels: Some(Default::default()),
                        ..Default::default()
                    },
                    desk,
                    cx,
                );
                root.active_project = Some(project);
                root.scoped_settings.identities.insert(
                    project,
                    crate::repository_settings::resolve_repository(path),
                );
                root
            })
        });
        RootGuard {
            root: Some(root),
            cx: cx.clone(),
        }
    }
    fn start_fake(cx: &mut TestAppContext, guard: &RootGuard, fake: &mut LspFixture) -> ProjectId {
        let root = guard.root.as_ref().unwrap();
        let project = cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.test_server = Some(fake.command(true));
                let project = r.active_project.unwrap();
                r.ensure_server(project, Language::of(Path::new("source.rs")).unwrap(), cx);
                project
            })
        });
        fake.ready();
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let ready = cx.update(|cx| {
                root.update(cx, |r, cx| {
                    r.poll_servers(cx);
                    r.servers.get(&(project, "rust")).is_some_and(|s| s.ready)
                })
            });
            if ready {
                break;
            }
            assert!(Instant::now() < until, "fake LSP never initialized");
            std::thread::sleep(Duration::from_millis(5));
        }
        project
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_disabled_policy_stops_server_without_losing_failure_state(cx: &mut TestAppContext) {
        use guild_session::repository_settings::{LspPolicy, RepositorySettings};
        let dir = tempfile::tempdir().unwrap();
        let mut fake = LspFixture::new();
        let guard = root(cx, dir.path());
        let project = start_fake(cx, &guard, &mut fake);
        let (running, deaths, dying) = cx.update(|cx| {
            guard.root.as_ref().unwrap().update(cx, |r, cx| {
                let identity = crate::repository_settings::resolve_repository(dir.path()).unwrap();
                let key = identity.key.clone();
                r.scoped_settings.identities.insert(project, Ok(identity));
                r.server_deaths.insert((project, "rust"), 3);
                r.serverless.insert((project, "rust"), Fault::Dying);
                let mut prefs = RepositorySettings::default();
                prefs.lsp.insert("rust".into(), LspPolicy::Disabled);
                r.settings.repository_settings.insert(key, prefs);
                r.reconcile_lsp_preferences(cx);
                (
                    r.servers.contains_key(&(project, "rust")),
                    r.server_deaths.get(&(project, "rust")).copied(),
                    matches!(r.serverless.get(&(project, "rust")), Some(Fault::Dying)),
                )
            })
        });
        assert!(!running, "disabled policy left its server running");
        assert_eq!(deaths, Some(3));
        assert!(dying);
        fake.assert_stopped();
        for fault in [
            Fault::Missing,
            Fault::Unprepared(&["Cargo.toml"]),
            Fault::Dying,
        ] {
            let (saved, deaths, running) = cx.update(|cx| {
                guard.root.as_ref().unwrap().update(cx, |r, cx| {
                    let key = r.repository_identity(project).unwrap().key.clone();
                    r.serverless.insert((project, "rust"), fault.clone());
                    for policy in [LspPolicy::Disabled, LspPolicy::Enabled] {
                        r.settings
                            .repository_settings
                            .get_mut(&key)
                            .unwrap()
                            .lsp
                            .insert("rust".into(), policy);
                        r.reconcile_lsp_preferences(cx);
                        r.ensure_server(project, Language::of(Path::new("source.rs")).unwrap(), cx);
                    }
                    (
                        r.serverless.get(&(project, "rust")).cloned(),
                        r.server_deaths.get(&(project, "rust")).copied(),
                        r.servers.contains_key(&(project, "rust")),
                    )
                })
            });
            assert_eq!(saved, Some(fault));
            assert_eq!(deaths, Some(3));
            assert!(
                !running,
                "enabling must not reset remembered startup failures"
            );
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_identity_error_and_pending_are_not_disabled_or_not_started(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let states = cx.update(|cx| {
            entity.update(cx, |r, _| {
                let project = r.active_project.unwrap();
                r.scoped_settings.identities.remove(&project);
                let pending = r
                    .servers_of(project)
                    .into_iter()
                    .find(|s| s.language == "rust")
                    .unwrap()
                    .state;
                r.scoped_settings
                    .identities
                    .insert(project, Err("fixture git unavailable".into()));
                let failed = r
                    .servers_of(project)
                    .into_iter()
                    .find(|s| s.language == "rust")
                    .unwrap()
                    .state;
                (
                    pending,
                    failed,
                    r.lsp_permitted(project, "rust"),
                    r.settings.repository_settings.is_empty(),
                )
            })
        });
        assert!(matches!(states.0, ServerState::IdentityPending));
        assert!(
            matches!(states.1, ServerState::IdentityFailed(ref error) if error == "fixture git unavailable")
        );
        assert!(!states.2 && states.3);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_directory_picker_retains_target_validates_and_cancels(cx: &mut TestAppContext) {
        use crate::view::settings::SettingsHost;
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let destination = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let (project, key) = cx.update(|cx| {
            entity.read_with(cx, |r, _| {
                (
                    r.active_project.unwrap(),
                    r.repository_identity(r.active_project.unwrap())
                        .unwrap()
                        .key
                        .clone(),
                )
            })
        });
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.pick_worktree_directory(SettingsHost::Window, project, cx)
            })
        });
        assert!(
            cx.did_prompt_for_paths(),
            "native folder picker must be connected"
        );
        cx.simulate_path_prompt_response(|options| {
            assert!(options.directories && !options.files && !options.multiple);
            None
        });
        cx.run_until_parked();
        assert!(
            cx.update(|cx| entity.read_with(cx, |r, _| r.settings.repository_settings.is_empty()))
        );
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.pick_worktree_directory(SettingsHost::Window, project, cx);
                r.active_project = None;
            })
        });
        cx.simulate_path_prompt_response(|_| Some(vec![destination.path().to_path_buf()]));
        wait_preferences(cx, entity);
        let saved = cx.update(|cx| {
            entity.read_with(cx, |r, _| {
                r.settings.repository_settings[&key]
                    .worktree_directory
                    .clone()
            })
        });
        assert_eq!(saved.as_deref(), Some(destination.path()));
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.pick_worktree_directory(SettingsHost::Window, project, cx)
            })
        });
        cx.simulate_path_prompt_response(|_| Some(vec!["relative".into()]));
        wait_preferences(cx, entity);
        cx.update(|cx| {
            entity.read_with(cx, |r, _| {
                assert!(r.scoped_settings.error.is_some());
                assert_eq!(
                    r.settings.repository_settings[&key].worktree_directory,
                    saved
                );
                assert!(r.servers.is_empty());
            })
        });
    }

    fn wait_preferences(cx: &mut TestAppContext, root: &Entity<Root>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            cx.executor().advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if cx.update(|cx| {
                root.read_with(cx, |r, _| {
                    r.scoped_settings.flight.is_none() && r.scoped_settings.pending.is_empty()
                })
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "preference writer never acknowledged"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_quit_drains_pending_settings_without_reentering_app(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Edit;
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let (started, observed) = std::sync::mpsc::sync_channel(1);
        let (release, blocked) = std::sync::mpsc::channel();
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.scoped_settings.write_gate = Some((started, blocked));
                r.commit_scoped_preference(Edit::Default("rust".into(), false), cx);
                r.commit_scoped_preference(Edit::Default("go".into(), false), cx);
                r.settings.copy_on_select = false;
                r.store_settings();
            })
        });
        observed.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(()).unwrap();
        cx.quit();
        let saved: guild_session::Settings =
            serde_json::from_slice(&std::fs::read(crate::paths::settings_file()).unwrap()).unwrap();
        assert!(!saved.lsp_enabled(dir.path(), "rust"));
        assert!(!saved.lsp_enabled(dir.path(), "go"));
        assert!(!saved.copy_on_select);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_blocked_settings_write_keeps_ui_live_and_merges_later_edits(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Edit;
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let path = crate::paths::settings_file();
        let (started, observed) = std::sync::mpsc::sync_channel(1);
        let (release, blocked) = std::sync::mpsc::channel();
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.scoped_settings.write_gate = Some((started, blocked));
                r.commit_scoped_preference_to(Edit::Default("rust".into(), false), &path, cx);
                assert!(
                    r.settings.lsp_enabled(dir.path(), "rust"),
                    "effective policy must await writer acknowledgement"
                );
            })
        });
        cx.run_until_parked();
        observed.recv_timeout(Duration::from_secs(10)).unwrap();
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                // This ordinary global save must neither wait behind the blocked
                // writer nor write an old policy snapshot after it.
                r.settings.copy_on_select = false;
                r.store_settings();
                r.commit_scoped_preference(Edit::Default("go".into(), false), cx);
                r.commit_scoped_preference(
                    Edit::Icon(dir.path().to_path_buf(), Some("folder".into())),
                    cx,
                );
                assert!(r.settings.lsp_enabled(dir.path(), "rust"));
                assert!(r.settings.lsp_enabled(dir.path(), "go"));
            })
        });
        release.send(()).unwrap();
        wait_preferences(cx, entity);
        guild_session::flush();
        let saved = guild_session::Store::<guild_session::Settings>::new(path)
            .load()
            .unwrap();
        assert!(!saved.copy_on_select);
        assert!(!saved.lsp_enabled(dir.path(), "rust"));
        assert!(!saved.lsp_enabled(dir.path(), "go"));
        assert_eq!(
            saved.repository_settings[dir.path()].icon.as_deref(),
            Some("folder")
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_failed_save_retries_identically_and_preserves_unrelated_settings(
        cx: &mut TestAppContext,
    ) {
        use crate::root::repository_settings::Edit;
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let path = blocker.join("settings.json");
        let root = guard.root.as_ref().unwrap();
        cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.commit_scoped_preference_to(Edit::Default("rust".into(), false), &path, cx)
            })
        });
        wait_preferences(cx, root);
        let (enabled, error) = cx.update(|cx| {
            root.read_with(cx, |r, _| {
                (
                    r.settings.lsp_enabled(dir.path(), "rust"),
                    r.scoped_settings.error.is_some(),
                )
            })
        });
        assert!(
            enabled && error,
            "failed persistence must not publish policy"
        );
        std::fs::remove_file(&blocker).unwrap();
        std::fs::create_dir(&blocker).unwrap();
        cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.settings.copy_on_select = false;
                r.retry_scoped_preference(cx);
            })
        });
        wait_preferences(cx, root);
        let saved = guild_session::Store::<guild_session::Settings>::new(path)
            .load()
            .unwrap();
        assert!(!saved.lsp_enabled(dir.path(), "rust"));
        assert!(!saved.copy_on_select);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_find_usages_reply_survives_thirty_seconds_and_token_pruning(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let mut fake = LspFixture::new();
        let project = start_fake(cx, &guard, &mut fake);
        let entity = guard.root.as_ref().unwrap();
        let uri = crate::lsp::uri(&dir.path().join("source.rs"));
        for prune in [false, true] {
            let (references, definition, barrier) = cx.update(|cx| {
                entity.update(cx, |r, _| {
                    let params = json!({
                        "textDocument": {"uri": uri},
                        "position": {"line": 0, "character": 0},
                        "context": {"includeDeclaration": false},
                    });
                    let references = r
                        .ask_server(project, "rust", "textDocument/references", params.clone())
                        .unwrap();
                    let definition = r
                        .ask_server(project, "rust", "textDocument/definition", params)
                        .unwrap();
                    // Age the real owner rather than a disconnected Root token.
                    for lease in [&references, &definition] {
                        lease
                            .request
                            .age_for_test(super::super::usages::DEADLINE - Duration::from_secs(1));
                    }
                    let barrier = if prune {
                        r.ask_server(project, "rust", "fixture/probe", json!({}))
                            .unwrap()
                    } else {
                        // Witness ordered delivery without invoking ask_server's prune.
                        ServerRequest {
                            key: (project, "rust"),
                            generation: r.scoped_settings.generations[&(project, "rust")],
                            request: r
                                .servers
                                .get_mut(&(project, "rust"))
                                .unwrap()
                                .client
                                .request("fixture/probe", json!({})),
                        }
                    };
                    (references, definition, barrier)
                })
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let answer = cx.update(|cx| {
                    entity.read_with(cx, |r, _| {
                        if prune {
                            r.server_answer(project, "rust", &barrier)
                        } else {
                            barrier.request.take_answer()
                        }
                    })
                });
                if let Some(answer) = answer {
                    assert!(answer.is_ok());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "fake references barrier timed out"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            let replies = cx.update(|cx| {
                entity.read_with(cx, |r, _| {
                    (
                        r.server_answer(project, "rust", &references),
                        r.server_answer(project, "rust", &definition),
                    )
                })
            });
            let references = replies
                .0
                .expect("active Find Usages token was pruned")
                .expect("active Find Usages reply rejected before its 60-second deadline");
            let links = crate::lsp::providers::links_in(references);
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].target_uri.as_str(), uri);
            assert!(
                replies.1.unwrap().is_ok(),
                "Find Usages definition expired early"
            );
        }
        let abandoned = cx.update(|cx| {
            entity.update(cx, |r, _| {
                let lease = r
                    .ask_server(project, "rust", "fixture/probe", json!({}))
                    .unwrap();
                lease.request.age_for_test(Duration::from_secs(121));
                r.servers.get_mut(&(project, "rust")).unwrap().client.poll();
                assert_eq!(r.servers[&(project, "rust")].client.pending_count(), 0);
                lease
            })
        });
        assert!(
            cx.update(
                |cx| entity.read_with(cx, |r, _| r.server_answer(project, "rust", &abandoned))
            )
            .unwrap()
            .is_err()
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_old_request_cannot_take_a_reenabled_server_answer(cx: &mut TestAppContext) {
        use guild_session::repository_settings::{LspPolicy, RepositorySettings};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let mut first = LspFixture::new();
        let project = start_fake(cx, &guard, &mut first);
        let entity = guard.root.as_ref().unwrap();
        let old = cx.update(|cx| {
            entity.update(cx, |r, _| {
                r.ask_server(project, "rust", "fixture/probe", serde_json::json!({}))
                    .unwrap()
            })
        });
        cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let key = r.repository_identity(project).unwrap().key.clone();
                let mut prefs = RepositorySettings::default();
                prefs.lsp.insert("rust".into(), LspPolicy::Disabled);
                r.settings.repository_settings.insert(key, prefs);
                r.reconcile_lsp_preferences(cx);
            })
        });
        first.assert_stopped();
        cx.update(|cx| {
            entity.update(cx, |r, _| {
                let key = r.repository_identity(project).unwrap().key.clone();
                r.settings
                    .repository_settings
                    .get_mut(&key)
                    .unwrap()
                    .lsp
                    .insert("rust".into(), LspPolicy::Enabled);
            })
        });
        let mut second = LspFixture::new();
        start_fake(cx, &guard, &mut second);
        let (fresh, barrier) = cx.update(|cx| {
            entity.update(cx, |r, _| {
                (
                    r.ask_server(project, "rust", "fixture/probe", json!({}))
                        .unwrap(),
                    r.ask_server(project, "rust", "fixture/probe", json!({}))
                        .unwrap(),
                )
            })
        });
        // Taking the later ordered reply proves the replacement client's
        // reused protocol ID already has an answer waiting in its reader map.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let taken = cx.update(|cx| {
                entity.read_with(cx, |r, _| r.server_answer(project, "rust", &barrier))
            });
            if let Some(answer) = taken {
                answer.unwrap();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement client did not answer its barrier"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let answer =
            cx.update(|cx| entity.read_with(cx, |r, _| r.server_answer(project, "rust", &old)));
        assert!(
            matches!(answer, Some(Err(_))),
            "stale request must terminate instead of reading a reused LSP request id"
        );
        let fresh_answer =
            cx.update(|cx| entity.read_with(cx, |r, _| r.server_answer(project, "rust", &fresh)));
        assert!(
            matches!(fresh_answer, Some(Ok(ref value)) if value["pid"].as_i64().is_some()),
            "stale waiter consumed the replacement client's answer"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_settings_hosts_are_independent_and_do_not_start_servers(cx: &mut TestAppContext) {
        use crate::root::repository_settings::{RepoPage, Target};
        use crate::view::settings::SettingsHost;
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let tab = crate::TabId::new();
        let (window, tab_target, empty) = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                let workspace = r.active_workspace;
                r.choose_scoped_settings(SettingsHost::Window, Target::Workspace(workspace), cx);
                r.choose_scoped_settings(
                    SettingsHost::Tab(tab),
                    Target::Repository {
                        workspace,
                        project,
                        page: RepoPage::Lsp,
                    },
                    cx,
                );
                (
                    r.scoped_settings_target(SettingsHost::Window),
                    r.scoped_settings_target(SettingsHost::Tab(tab)),
                    r.servers.is_empty(),
                )
            })
        });
        assert!(matches!(window, Some(Target::Workspace(_))));
        assert!(matches!(
            tab_target,
            Some(Target::Repository {
                page: RepoPage::Lsp,
                ..
            })
        ));
        assert!(empty);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_global_policy_changes_only_inheritors_across_workspaces(cx: &mut TestAppContext) {
        use guild_session::repository_settings::{LspPolicy, RepositorySettings};
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-q"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "fixture",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir.path())
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let linked = dir.path().join("linked");
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["worktree", "add", "-qb", "fixture"])
                .arg(&linked)
                .status()
                .unwrap()
                .success()
        );
        let guard = root(cx, dir.path());
        let mut inherit = LspFixture::new();
        let first = start_fake(cx, &guard, &mut inherit);
        let entity = guard.root.as_ref().unwrap();
        let second = cx.update(|cx| {
            entity.update(cx, |r, _| {
                let workspace = r.desk.add_workspace("Another", None).unwrap();
                let second = r.desk.open_in(workspace, &linked);
                let identity = crate::repository_settings::resolve_repository(&linked).unwrap();
                assert_eq!(identity.key, r.repository_identity(first).unwrap().key);
                r.scoped_settings.identities.insert(second, Ok(identity));
                r.active_project = Some(second);
                second
            })
        });
        let mut shared = LspFixture::new();
        start_fake(cx, &guard, &mut shared);
        let other_dir = tempfile::tempdir().unwrap();
        let third = cx.update(|cx| {
            entity.update(cx, |r, _| {
                let third = r.desk.open_in(r.active_workspace, other_dir.path());
                r.scoped_settings.identities.insert(
                    third,
                    crate::repository_settings::resolve_repository(other_dir.path()),
                );
                let key = r.repository_identity(third).unwrap().key.clone();
                let mut prefs = RepositorySettings::default();
                prefs.lsp.insert("rust".into(), LspPolicy::Enabled);
                r.settings.repository_settings.insert(key, prefs);
                r.active_project = Some(third);
                third
            })
        });
        let mut explicit = LspFixture::new();
        start_fake(cx, &guard, &mut explicit);
        let (remaining, allowed) = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                r.settings.lsp_defaults.insert("rust".into(), false);
                r.reconcile_lsp_preferences(cx);
                (
                    r.servers.keys().copied().collect::<Vec<_>>(),
                    r.lsp_permitted(third, "rust"),
                )
            })
        });
        assert_eq!(remaining, vec![(third, "rust")]);
        assert!(allowed);
        assert_ne!(
            first, second,
            "workspace memberships must be distinct runtime owners"
        );
        inherit.assert_stopped();
        shared.assert_stopped();
        let alive = cx.update(|cx| entity.read_with(cx, |r, _| r.server_alive(third, "rust")));
        assert!(
            alive,
            "explicit-enabled server must survive global disabling"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_closed_hosts_and_removed_targets_release_drafts(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let (target, drafts) = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let workspace = r.desk.add_workspace("Removed", None).unwrap();
                r.settings_page = Some(Section::Editor);
                r.choose_scoped_settings(SettingsHost::Window, Target::Workspace(workspace), cx);
                r.scoped_settings
                    .drafts
                    .insert("Window:removed".into(), "unsaved".into());
                let tab = r.restore_settings_tab(r.active_project.unwrap(), cx);
                r.choose_scoped_settings(SettingsHost::Tab(tab), Target::GlobalLsp, cx);
                r.scoped_settings.drafts.insert(
                    format!("{:?}:closed", SettingsHost::Tab(tab)),
                    "unsaved".into(),
                );
                r.tabs.clear();
                r.desk.delete_workspace(workspace).unwrap();
                r.prepare_settings_scopes(cx);
                (
                    r.scoped_settings_target(SettingsHost::Window),
                    r.scoped_settings.drafts.len(),
                )
            })
        });
        assert_eq!(target, Some(Target::Global(Section::General)));
        assert_eq!(drafts, 0, "removed views must release their drafts");
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_disable_clears_diagnostics_only_and_keeps_other_languages(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let (before,after,other)=cx.update(|cx|entity.update(cx,|r,cx|{
            let project=r.active_project.unwrap();
            for (path,message) in [("closed.rs","rust-only"),("kept.go","go-control")] {
                let diagnostic=serde_json::from_value(json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":message,"severity":1})).unwrap();
                r.diagnostics.insert((project,path.into()),vec![diagnostic]);
            }
            let before=r.problems().0.len(); r.settings.lsp_defaults.insert("rust".into(),false); r.reconcile_lsp_preferences(cx);
            (before,r.problems().0,r.diagnostics.contains_key(&(project,"kept.go".into())))
        }));
        assert_eq!(before, 2);
        assert!(other, "unrelated Go diagnostics cleared");
        assert_eq!(
            after.len(),
            1,
            "diagnostics-only Disabled remains in Problems"
        );
        assert_eq!(after[0].message, "go-control");
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_superseded_policy_keeps_live_document_request_and_latest_failure(
        cx: &mut TestAppContext,
    ) {
        use crate::root::repository_settings::Edit;
        use guild_session::repository_settings::LspPolicy;
        for fail_latest in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("source.rs"), "fn main() {}\n").unwrap();
            let guard = root(cx, dir.path());
            let mut fake = LspFixture::new();
            let project = start_fake(cx, &guard, &mut fake);
            let entity = guard.root.as_ref().unwrap().clone();
            let (_, visual) = cx.add_window_view(|window, cx| {
                entity.update(cx, |r, cx| {
                    r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx);
                    r.ensure_documents(cx);
                });
                let host = cx.new(|cx| ScopeHost {
                    root: entity.clone(),
                    settings: false,
                    _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
                });
                gpui_component::Root::new(host, window, cx)
            });
            visual.run_until_parked();
            let (token, key, generation) = cx.update(|cx| {
                entity.update(cx, |r, _| {
                    (
                        r.ask_server(project, "rust", "fixture/probe", json!({}))
                            .unwrap(),
                        r.repository_identity(project).unwrap().key.clone(),
                        r.scoped_settings.generations[&(project, "rust")],
                    )
                })
            });
            let path = dir.path().join("saved.json");
            let blocker = dir.path().join("blocker");
            std::fs::write(&blocker, "not a directory").unwrap();
            let latest = if fail_latest {
                blocker.join("settings.json")
            } else {
                path.clone()
            };
            let (started, observed) = std::sync::mpsc::sync_channel(1);
            let (release, blocked) = std::sync::mpsc::channel();
            cx.update(|cx| {
                entity.update(cx, |r, cx| {
                    r.scoped_settings.write_gate = Some((started, blocked));
                    r.commit_scoped_preference_to(
                        Edit::Policy(key.clone(), "rust".into(), LspPolicy::Disabled),
                        &path,
                        cx,
                    );
                })
            });
            observed.recv_timeout(Duration::from_secs(10)).unwrap();
            cx.update(|cx| {
                entity.update(cx, |r, cx| {
                    r.settings.copy_on_select = false;
                    r.store_settings_to(&path);
                    r.commit_scoped_preference_to(
                        Edit::Policy(key.clone(), "rust".into(), LspPolicy::Enabled),
                        &latest,
                        cx,
                    );
                })
            });
            release.send(()).unwrap();
            wait_preferences(cx, &entity);
            let state = cx.update(|cx| {
                entity.read_with(cx, |r, _| {
                    (
                        r.documents.contains_key(&(project, "source.rs".into())),
                        r.scoped_settings.generations[&(project, "rust")],
                        r.settings.lsp_enabled(&key, "rust"),
                        r.scoped_settings.error.is_some() && r.scoped_settings.retry.is_some(),
                    )
                })
            });
            assert!(state.0, "superseded Disabled cleared the open document");
            assert_eq!(
                state.1, generation,
                "superseded Disabled invalidated requests"
            );
            assert!(
                state.2,
                "latest failed save must keep previous effective policy"
            );
            assert_eq!(state.3, fail_latest);
            let deadline = Instant::now() + Duration::from_secs(10);
            let answer = loop {
                let answer = cx.update(|cx| {
                    entity.read_with(cx, |r, _| r.server_answer(project, "rust", &token))
                });
                if let Some(answer) = answer {
                    break answer.unwrap();
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            };
            assert!(
                answer["pid"].as_i64().is_some(),
                "old request must reach the same live fake"
            );
            guild_session::flush();
            let saved = guild_session::Store::<guild_session::Settings>::new(path)
                .load()
                .unwrap();
            assert!(saved.lsp_enabled(&key, "rust"));
            assert!(!saved.copy_on_select);
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_retry_discards_submitted_draft_preserves_newer_typing_and_other_host(
        cx: &mut TestAppContext,
    ) {
        use crate::root::repository_settings::{Field, Target};
        use crate::view::settings::{Section, SettingsHost};
        for newer in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let guard = root(cx, dir.path());
            let entity = guard.root.as_ref().unwrap().clone();
            let blocker = dir.path().join("owned-blocker");
            std::fs::write(&blocker, "not a directory").unwrap();
            let path = blocker.join("settings.json");
            let (key, id, field) = cx.update(|cx| {
                entity.update(cx, |r, cx| {
                    let project = r.active_project.unwrap();
                    let key = r.repository_identity(project).unwrap().key.clone();
                    let id = format!("Window:repo-{project}-base");
                    r.scoped_settings.test_settings_path = Some(path.clone());
                    r.settings_page = Some(Section::General);
                    r.choose_scoped_settings(SettingsHost::Window, Target::GlobalLsp, cx);
                    r.scoped_settings.drafts.insert(id.clone(), "main".into());
                    let field = Field::Base(key.clone());
                    r.commit_scoped_field(&id, &field, cx);
                    (key, id, field)
                })
            });
            wait_preferences(cx, &entity);
            assert!(
                cx.update(|cx| entity.read_with(cx, |r, _| r.scoped_settings.error.is_some())),
                "write did not fail"
            );
            std::fs::remove_file(&blocker).unwrap();
            std::fs::create_dir(&blocker).unwrap();
            let (_, visual) = cx.add_window_view(|window, cx| {
                let host = cx.new(|cx| ScopeHost {
                    root: entity.clone(),
                    settings: true,
                    _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
                });
                gpui_component::Root::new(host, window, cx)
            });
            visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(950.)));
            visual.run_until_parked();
            if newer {
                visual.update(|_, cx| {
                    entity.update(cx, |r, cx| {
                        r.scoped_settings
                            .drafts
                            .insert(id.clone(), "newer-typing".into());
                        cx.notify();
                    })
                });
            }
            let retry = visual.debug_bounds("scoped-retry").unwrap();
            visual.simulate_click(retry.center(), gpui::Modifiers::default());
            wait_preferences(cx, &entity);
            let draft = cx.update(|cx| {
                entity.read_with(cx, |r, _| r.scoped_settings.drafts.get(&id).cloned())
            });
            if newer {
                assert_eq!(draft.as_deref(), Some("newer-typing"));
            } else {
                assert!(draft.is_none(), "successful Retry retained submitted draft");
                cx.update(|cx| {
                    entity.update(cx, |r, cx| {
                        r.scoped_settings
                            .drafts
                            .insert("Tab(other):base".into(), "develop".into());
                        r.commit_scoped_field("Tab(other):base", &field, cx);
                    })
                });
                wait_preferences(cx, &entity);
                cx.update(|cx| entity.update(cx, |r, cx| r.commit_scoped_field(&id, &field, cx)));
                wait_preferences(cx, &entity);
                let value = cx.update(|cx| {
                    entity.read_with(cx, |r, _| {
                        r.settings.repository_settings[&key].worktree_base.clone()
                    })
                });
                assert_eq!(
                    value.as_deref(),
                    Some("develop"),
                    "stale host blur overwrote new value"
                );
            }
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_workspace_field_tab_and_shift_tab_move_real_focus_and_save_blur(
        cx: &mut TestAppContext,
    ) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::actions::install(cx);
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Workspace(r.active_workspace),
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(850.)));
        visual.run_until_parked();
        for (keys, name) in [("cmd-a a tab", "a"), ("cmd-a b shift-tab", "b")] {
            let field = visual.debug_bounds("scoped-workspace-name").unwrap();
            visual.simulate_click(field.center(), gpui::Modifiers::default());
            let old = visual.update(|window, cx| window.focused(cx).unwrap());
            visual.simulate_keystrokes(keys);
            visual.run_until_parked();
            let (left, current, value) = visual.update(|window, cx| {
                (
                    !old.is_focused(window),
                    window.focused(cx).is_some(),
                    entity.read_with(cx, |r, _| {
                        r.desk.workspace(r.active_workspace).unwrap().name.clone()
                    }),
                )
            });
            assert!(
                left && current,
                "Tab did not transfer actual InputState focus"
            );
            assert_eq!(value, name, "Tab blur did not save field");
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_workspace_icon_is_compact_and_popup_is_anchored_to_control(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::actions::install(cx);
            entity.update(cx, |r, cx| {
                r.active_project = None;
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Workspace(r.active_workspace),
                    cx,
                );
            });
            gpui_component::Root::new(entity.clone(), window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1180.), gpui::px(1000.)));
        visual.run_until_parked();
        let button = visual.debug_bounds("scoped-workspace-icon").unwrap();
        assert!(
            button.size.width <= gpui::px(260.),
            "workspace icon control stretches across card"
        );
        visual.simulate_click(button.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        let popup = visual.debug_bounds("workspace-icon-popup").unwrap();
        assert!(
            (popup.right() - button.right()).abs() < gpui::px(8.),
            "popup is anchored to sidebar instead of control"
        );
        assert!(popup.origin.y >= button.bottom_left().y - gpui::px(8.));
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_repository_settings_are_one_page_for_both_legacy_targets(cx: &mut TestAppContext) {
        use crate::root::repository_settings::{RepoPage, Target};
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Repository {
                        workspace: r.active_workspace,
                        project: r.active_project.unwrap(),
                        page: RepoPage::Overview,
                    },
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1180.), gpui::px(1500.)));
        visual.run_until_parked();
        for page in [RepoPage::Overview, RepoPage::Lsp] {
            visual.update(|_, cx| {
                entity.update(cx, |r, cx| {
                    r.choose_scoped_settings(
                        SettingsHost::Window,
                        Target::Repository {
                            workspace: r.active_workspace,
                            project: r.active_project.unwrap(),
                            page,
                        },
                        cx,
                    )
                })
            });
            visual.run_until_parked();
            assert!(
                visual.debug_bounds("scoped-repository-base").is_some(),
                "repository base is absent from unified page"
            );
            assert!(
                visual
                    .debug_bounds("scoped-repo-lsp-rust-Disabled")
                    .is_some(),
                "repository language override still needs a separate page"
            );
        }
    }

    /// A repository's settings say which project group it is in: a Group
    /// row in the Repository card, and in the WORKSPACES list the group
    /// stands under its workspace with the repository nested under it.
    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_repository_settings_name_the_project_group(cx: &mut TestAppContext) {
        use crate::root::repository_settings::{RepoPage, Target};
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Repository {
                        workspace: r.active_workspace,
                        project: r.active_project.unwrap(),
                        page: RepoPage::Overview,
                    },
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1180.), gpui::px(1200.)));
        visual.run_until_parked();
        let (workspace, project) = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| (r.active_workspace, r.active_project.unwrap()))
        });
        let repo_row: &'static str = Box::leak(
            format!("{:?}-repo-{workspace}-{project}", SettingsHost::Window).into_boxed_str(),
        );
        let ungrouped = visual.debug_bounds(repo_row).expect("the repository's row");
        // A repository without an icon of its own is marked as the column
        // marks it — its letter tile, in the column's colour — not with the
        // folder a project group has.
        let tile_selector: &'static str =
            Box::leak(format!("scoped-repository-tile-{project}").into_boxed_str());
        let tile = visual
            .debug_bounds(tile_selector)
            .expect("the repository's letter tile");
        assert!(
            ungrouped.contains(&tile.center()),
            "the tile is in the repository's row"
        );
        let fill = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                let name = r
                    .repository_identity(project)
                    .and_then(|i| i.main.as_ref())
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| r.desk.project(project).unwrap().name.clone());
                gpui::Hsla::from(gpui::rgb(crate::view::column::tile_fill(r.theme, &name)))
            })
        });
        assert!(
            visual.update(|window, _| {
                window.painted_quads().into_iter().any(|quad| {
                    quad.bounds == tile.scale(window.scale_factor())
                        && quad.background.as_solid() == Some(fill)
                })
            }),
            "the tile is in the column's colour for the repository"
        );
        // An icon chosen for the repository is shown instead, as before.
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                let key = r.repository_identity(project).unwrap().key.clone();
                r.settings.repository_settings.entry(key).or_default().icon = Some("rocket".into());
                cx.notify();
            })
        });
        visual.run_until_parked();
        assert!(
            visual.debug_bounds(tile_selector).is_none(),
            "a chosen icon replaces the tile"
        );
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                let key = r.repository_identity(project).unwrap().key.clone();
                r.settings.repository_settings.get_mut(&key).unwrap().icon = None;
                cx.notify();
            })
        });
        visual.run_until_parked();
        assert!(
            visual.debug_bounds("scoped-repository-group").is_none(),
            "an ungrouped repository has no Group row"
        );

        let group = visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                let id = r
                    .desk
                    .create_project_group(workspace, "agent-review", dir.path())
                    .unwrap();
                cx.notify();
                id
            })
        });
        visual.run_until_parked();
        assert!(
            visual.debug_bounds("scoped-repository-group").is_some(),
            "the Repository card names the group"
        );
        let nav: &'static str = Box::leak(format!("scoped-nav-group-{group}").into_boxed_str());
        let group_row = visual
            .debug_bounds(nav)
            .expect("the group's row in the list");
        let nested = visual.debug_bounds(repo_row).unwrap();
        assert!(
            group_row.bottom() <= nested.top(),
            "the group is above its repository"
        );
        assert!(
            nested.left() > group_row.left() && nested.left() > ungrouped.left(),
            "the repository is nested under its group"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_settings_wheel_over_field_scrolls_page_but_horizontal_stays_owned(
        cx: &mut TestAppContext,
    ) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Workspace(r.active_workspace),
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1180.), gpui::px(580.)));
        visual.run_until_parked();
        let input = visual.debug_bounds("scoped-workspace-name-input").unwrap();
        let row = visual.debug_bounds("scoped-workspace-name").unwrap();
        for position in [
            gpui::point(row.left() + gpui::px(5.), input.center().y),
            input.center(),
        ] {
            visual.update(|_, cx| {
                entity.update(cx, |r, cx| {
                    r.settings_scroll.set_offset(Default::default());
                    cx.notify();
                })
            });
            visual.run_until_parked();
            visual.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-60.))),
                ..Default::default()
            });
            visual.run_until_parked();
            assert!(
                visual.update(|_, cx| entity.read_with(cx, |r, _| r.settings_scroll.offset().y))
                    < gpui::px(0.),
                "vertical page scroll was swallowed at {position:?}"
            );
        }
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.settings_scroll.set_offset(Default::default());
                cx.notify();
            })
        });
        visual.run_until_parked();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: input.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(-60.), gpui::px(-5.))),
            ..Default::default()
        });
        visual.run_until_parked();
        assert_eq!(
            visual.update(|_, cx| entity.read_with(cx, |r, _| r.settings_scroll.offset().y)),
            gpui::px(0.),
            "horizontal field gesture leaked to the page"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_settings_scroll_layout_fits_scaled_and_narrow_hosts(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Workspace(r.active_workspace),
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        for font in [13., 18.] {
            visual.update(|window, cx| {
                window.set_rem_size(crate::scale::rem_size(font));
                entity.update(cx, |r, cx| {
                    r.settings.ui_font_size = font;
                    cx.notify();
                });
            });
            for width in [1180., 560., 450.] {
                visual.simulate_resize(gpui::size(gpui::px(width), gpui::px(580.)));
                visual.run_until_parked();
                let page = visual.debug_bounds("settings-page").unwrap();
                let nav = visual.debug_bounds("settings-nav-region").unwrap();
                let done = visual.debug_bounds("settings-done").unwrap();
                assert_eq!(
                    nav.top(),
                    page.top(),
                    "navigation is interrupted by a full-width header"
                );
                assert!(
                    done.left() >= nav.left()
                        && done.right() <= nav.right()
                        && done.top() >= nav.top()
                        && done.bottom() <= nav.bottom(),
                    "exit control must belong to the continuous navigation region"
                );
                let content = visual.debug_bounds("settings-content").unwrap();
                assert!(
                    content.left() >= gpui::px(0.) && content.right() <= gpui::px(width),
                    "content {content:?}, viewport {width}, font {font}"
                );
                let before = visual.debug_bounds("scoped-workspace-name-input").unwrap();
                assert!(
                    before.right() <= gpui::px(width),
                    "input escaped width {width} at font {font}"
                );
                visual.simulate_event(gpui::ScrollWheelEvent {
                    position: gpui::point(content.left() + gpui::px(5.), gpui::px(500.)),
                    delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-40.))),
                    ..Default::default()
                });
                visual.run_until_parked();
                let after = visual.debug_bounds("settings-content").unwrap();
                assert_eq!(
                    content.size.width, after.size.width,
                    "scroll changed layout width"
                );
            }
        }
    }

    struct ScopeHost {
        root: Entity<Root>,
        settings: bool,
        _changed: gpui::Subscription,
    }
    impl gpui::Render for ScopeHost {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::prelude::*;
            self.root.update(cx, |r, cx| {
                if self.settings {
                    r.render_settings(
                        crate::view::settings::Section::General,
                        crate::view::settings::SettingsHost::Window,
                        cx,
                    )
                } else {
                    gpui::div().into_any_element()
                }
            })
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_native_workspace_field_preserves_edit_history_and_commits(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::actions::install(cx);
            entity.update(cx, |r, cx| {
                r.settings_page = Some(Section::General);
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Workspace(r.active_workspace),
                    cx,
                );
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(800.)));
        visual.run_until_parked();
        let field = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| format!("workspace-{}-name", r.active_workspace))
        });
        let bounds = visual
            .debug_bounds("scoped-workspace-name")
            .expect("native workspace name field");
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        visual.simulate_keystrokes("cmd-a x");
        visual.update(|_, cx| entity.update(cx, |_, cx| cx.notify()));
        visual.run_until_parked();
        visual.simulate_keystrokes("cmd-z");
        let value = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                r.scoped_settings
                    .drafts
                    .get(&format!("Window:{field}"))
                    .cloned()
            })
        });
        assert_eq!(
            value.as_deref(),
            Some("Default"),
            "unchanged notifications must retain undo"
        );
        visual.simulate_keystrokes("cmd-a x enter");
        let (name, no_servers) = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                (
                    r.desk.workspace(r.active_workspace).unwrap().name.clone(),
                    r.servers.is_empty(),
                )
            })
        });
        assert_eq!(name, "x");
        assert!(no_servers);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_disable_clears_editor_services_and_enable_supplies_open_buffer(
        cx: &mut TestAppContext,
    ) {
        use guild_session::repository_settings::LspPolicy;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("source.rs"), "fn main() {}\n").unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let mut first = LspFixture::new();
        let project = start_fake(cx, &guard, &mut first);
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx);
            });
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: false,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.run_until_parked();
        let editor = visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.ensure_documents(cx);
                r.editor_of(project, Path::new("source.rs"), cx).unwrap()
            })
        });
        let original = visual.update(|_, cx| editor.read(cx).text().to_string());
        visual.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(3..7, cx)));
        let (attached, closed, unchanged, selection) = visual.update(|_, cx| entity.update(cx, |r, cx| {
            let attached = editor.read(cx).lsp().completion_provider.is_some();
            r.diagnostics_arrived(project, json!({"uri": crate::lsp::uri(&dir.path().join("source.rs")), "diagnostics":[{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}},"message":"fake error","severity":1}]}), cx);
            let key = r.repository_identity(project).unwrap().key.clone();
            r.settings.repository_settings.entry(key).or_default().lsp.insert("rust".into(), LspPolicy::Disabled);
            r.reconcile_lsp_preferences(cx);
            let state = editor.read(cx);
            (attached, r.documents.is_empty() && r.diagnostics.is_empty() && state.lsp().completion_provider.is_none() && state.lsp().definition_provider.is_none(), *state.text() == original, state.selected_range())
        }));
        assert!(attached && closed && unchanged);
        assert_eq!(selection, 3..7);
        first.assert_stopped();
        let mut second = LspFixture::new();
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.test_server = Some(second.command(true));
                let key = r.repository_identity(project).unwrap().key.clone();
                r.settings
                    .repository_settings
                    .get_mut(&key)
                    .unwrap()
                    .lsp
                    .insert("rust".into(), LspPolicy::Enabled);
                r.reconcile_lsp_preferences(cx);
            })
        });
        second.ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        let id = loop {
            let id = visual.update(|_, cx| {
                entity.update(cx, |r, cx| {
                    r.poll_servers(cx);
                    r.ask_server(project, "rust", "fixture/probe", json!({}))
                })
            });
            if let Some(id) = id {
                break id;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let answer = loop {
            let answer = visual
                .update(|_, cx| entity.read_with(cx, |r, _| r.server_answer(project, "rust", &id)));
            if let Some(answer) = answer {
                break answer.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(answer["opened"][0]["text"], original);
        let restored = visual.update(|_, cx| {
            entity.read_with(cx, |r, cx| {
                r.documents.contains_key(&(project, "source.rs".into()))
                    && editor.read(cx).lsp().completion_provider.is_some()
            })
        });
        assert!(restored);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_workspace_delete_control_keeps_confirmation_and_preferences(cx: &mut TestAppContext) {
        use crate::root::repository_settings::Target;
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), "keep repository files").unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let mut fake = LspFixture::new();
        let project = start_fake(cx, &guard, &mut fake);
        let workspace = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let workspace = r.active_workspace;
                r.desk.add_workspace("Survives", None).unwrap();
                r.settings_page = Some(Section::General);
                let key = r.repository_identity(project).unwrap().key.clone();
                r.settings.repository_settings.entry(key).or_default().icon = Some("folder".into());
                r.choose_scoped_settings(SettingsHost::Window, Target::Workspace(workspace), cx);
                workspace
            })
        });
        let (_, visual) = cx.add_window_view(|window, cx| {
            let host = cx.new(|cx| ScopeHost {
                root: entity.clone(),
                settings: true,
                _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            });
            gpui_component::Root::new(host, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(900.)));
        visual.run_until_parked();
        let bounds = visual
            .debug_bounds("scoped-delete-workspace")
            .expect("connected delete control");
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        assert!(
            visual.has_pending_prompt(),
            "settings deletion must retain confirmation"
        );
        visual.simulate_prompt_answer("Cancel");
        visual.run_until_parked();
        let survived = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                r.desk.workspace(workspace).is_some() && r.server_alive(project, "rust")
            })
        });
        assert!(
            survived,
            "Cancel must preserve the workspace and its owned service"
        );
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        visual.simulate_prompt_answer("Delete");
        visual.run_until_parked();
        fake.assert_stopped();
        let (removed, icon) = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                (
                    r.desk.workspace(workspace).is_none(),
                    r.settings
                        .repository_settings
                        .values()
                        .next()
                        .unwrap()
                        .icon
                        .clone(),
                )
            })
        });
        assert!(removed);
        assert_eq!(icon.as_deref(), Some("folder"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
            "keep repository files"
        );
        visual.update(|window, cx| {
            entity.update(cx, |r, cx| {
                r.delete_workspace_asking(r.active_workspace, window, cx)
            })
        });
        visual.run_until_parked();
        assert!(
            !visual.has_pending_prompt(),
            "the last workspace must not be deletable"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_full_window_policy_buttons_persist_without_service_launch(cx: &mut TestAppContext) {
        use crate::root::repository_settings::{RepoPage, Target};
        use crate::view::settings::{Section, SettingsHost};
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let project = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                r.active_project = None;
                r.settings_page = Some(Section::General);
                r.settings.lsp_defaults.insert("rust".into(), false);
                r.choose_scoped_settings(SettingsHost::Window, Target::GlobalLsp, cx);
                project
            })
        });
        let mut waiter = cx.clone();
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::actions::install(cx);
            gpui_component::Root::new(entity.clone(), window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(1180.), gpui::px(800.)));
        visual.run_until_parked();
        let bounds = visual
            .debug_bounds("scoped-global-lsp-rust-true")
            .expect("global Rust On control in full Root frame");
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        wait_preferences(&mut waiter, &entity);
        let enabled = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| r.settings.lsp_enabled(dir.path(), "rust"))
        });
        assert!(enabled, "native global control must commit the preference");
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.choose_scoped_settings(
                    SettingsHost::Window,
                    Target::Repository {
                        workspace: r.active_workspace,
                        project,
                        page: RepoPage::Lsp,
                    },
                    cx,
                )
            })
        });
        visual.run_until_parked();
        let bounds = visual
            .debug_bounds("scoped-repo-lsp-rust-Disabled")
            .expect("repository Disabled control in full Root frame");
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        wait_preferences(&mut waiter, &entity);
        let (disabled, no_servers) = visual.update(|_, cx| {
            entity.read_with(cx, |r, _| {
                (!r.lsp_permitted(project, "rust"), r.servers.is_empty())
            })
        });
        assert!(disabled && no_servers);
        let saved =
            guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
                .load()
                .unwrap();
        let key = crate::repository_settings::resolve_repository(dir.path())
            .unwrap()
            .key;
        assert!(!saved.lsp_enabled(&key, "rust"));
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_find_close_rerun_and_early_references_release_both_requests(cx: &mut TestAppContext) {
        for mode in ["close", "rerun", "references-first", "forget"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("source.rs"), "fn example() {}\n").unwrap();
            let guard = root(cx, dir.path());
            let entity = guard.root.as_ref().unwrap().clone();
            let project = cx.update(|cx| {
                entity.update(cx, |r, _| {
                    let project = r.active_project.unwrap();
                    r.test_server = Some(("/usr/bin/false".into(), Vec::new()));
                    r.servers.insert(
                        (project, "rust"),
                        Server {
                            client: crate::lsp::client::lifetime_tests::fake(),
                            initialize: None,
                            ready: true,
                            work: Work::default(),
                        },
                    );
                    project
                })
            });
            let (_, visual) = cx.add_window_view(|window, cx| {
                entity.update(cx, |r, cx| {
                    r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx);
                });
                let host = cx.new(|cx| ScopeHost {
                    root: entity.clone(),
                    settings: false,
                    _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
                });
                gpui_component::Root::new(host, window, cx)
            });
            visual.run_until_parked();
            visual.update(|window, cx| {
                entity.update(cx, |r, cx| {
                    let editor = r.editor_of(project, Path::new("source.rs"), cx).unwrap();
                    editor.update(cx, |e, cx| {
                        e.set_cursor_position(
                            gpui_component::input::Position::new(0, 5),
                            window,
                            cx,
                        )
                    });
                    r.find_usages(window, cx);
                })
            });
            let (before, after) = visual.update(|window, cx| {
                entity.update(cx, |r, cx| {
                    let before = r.servers[&(project, "rust")].client.pending_count();
                    match mode {
                        "close" => r.close_find_tab_at(project, 0, cx),
                        "rerun" => r.rerun_find(window, cx),
                        "forget" => r.forget_finds(project),
                        _ => {}
                    }
                    (before, r.servers[&(project, "rust")].client.pending_count())
                })
            });
            assert_eq!(
                before, 2,
                "{mode}: references and auxiliary definition were sent"
            );
            assert_eq!(
                after,
                if mode == "rerun" || mode == "references-first" {
                    2
                } else {
                    0
                },
                "{mode}: old operation retained requests"
            );
            visual.update(|_, cx| {
                entity.update(cx, |r, _| {
                    let client = &mut r.servers.get_mut(&(project, "rust")).unwrap().client;
                    crate::lsp::client::lifetime_tests::release_method(
                        client,
                        (mode == "references-first").then_some("textDocument/references"),
                    );
                })
            });
            visual.executor().advance_clock(Duration::from_millis(32));
            visual.run_until_parked();
            let retained = visual.update(|_, cx| {
                entity.read_with(cx, |r, _| {
                    r.servers[&(project, "rust")].client.pending_count()
                })
            });
            assert_eq!(
                retained, 0,
                "{mode}: operation completion retained an auxiliary payload"
            );
            let after_late = visual.update(|_, cx| {
                entity.update(cx, |r, _| {
                    let client = &mut r.servers.get_mut(&(project, "rust")).unwrap().client;
                    crate::lsp::client::lifetime_tests::released(client);
                    client.pending_count()
                })
            });
            assert_eq!(after_late, 0, "{mode}: late auxiliary answer was admitted");
        }
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_symbol_supersede_releases_payload_and_does_not_cancel_other_requests(
        cx: &mut TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap();
        let project = cx.update(|cx| {
            entity.update(cx, |r, _| {
                let project = r.active_project.unwrap();
                r.servers.insert(
                    (project, "rust"),
                    Server {
                        client: crate::lsp::client::lifetime_tests::fake(),
                        initialize: None,
                        ready: true,
                        work: Work::default(),
                    },
                );
                project
            })
        });
        let counts = cx.update(|cx| {
            entity.update(cx, |r, cx| {
                let control = r
                    .ask_server(project, "rust", "independent", json!({}))
                    .unwrap();
                r.rank_symbols(project, "a", false, cx);
                crate::lsp::client::lifetime_tests::released(
                    &mut r.servers.get_mut(&(project, "rust")).unwrap().client,
                );
                let before = r.servers[&(project, "rust")].client.pending_count();
                r.rank_symbols(project, "ab", false, cx);
                let cancelled = r.servers[&(project, "rust")].client.pending_count();
                r.symbols_poll(); // Settle cancelled flight and send only latest query.
                let replaced = r.servers[&(project, "rust")].client.pending_count();
                r.symbols_forget(project);
                let forgotten = r.servers[&(project, "rust")].client.pending_count();
                assert!(r.server_answer(project, "rust", &control).unwrap().is_ok());
                (
                    before,
                    cancelled,
                    replaced,
                    forgotten,
                    r.servers[&(project, "rust")].client.pending_count(),
                )
            })
        });
        assert_eq!(counts, (2, 1, 2, 1, 0));
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_provider_releases_timeout_close_and_dropped_futures(cx: &mut TestAppContext) {
        use gpui_component::input::{HoverProvider, Rope};
        use std::sync::{Arc, Mutex};
        for mode in ["drop-unpolled", "drop-pending", "close", "timeout"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("source.rs"), "fn example() {}\n").unwrap();
            let guard = root(cx, dir.path());
            let entity = guard.root.as_ref().unwrap().clone();
            let project = cx.update(|cx| {
                entity.update(cx, |r, _| {
                    let project = r.active_project.unwrap();
                    r.test_server = Some(("/usr/bin/false".into(), Vec::new()));
                    r.servers.insert(
                        (project, "rust"),
                        Server {
                            client: crate::lsp::client::lifetime_tests::fake(),
                            initialize: None,
                            ready: true,
                            work: Work::default(),
                        },
                    );
                    project
                })
            });
            let (_, visual) = cx.add_window_view(|window, cx| {
                entity.update(cx, |r, cx| {
                    r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx);
                });
                let host = cx.new(|cx| ScopeHost {
                    root: entity.clone(),
                    settings: false,
                    _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
                });
                gpui_component::Root::new(host, window, cx)
            });
            visual.run_until_parked();
            visual.update(|_, cx| {
                entity.update(cx, |r, cx| {
                    r.ensure_documents(cx);
                    assert!(r.documents.contains_key(&(project, "source.rs".into())));
                })
            });
            let provider = crate::lsp::providers::Provider::of(
                entity.downgrade(),
                project,
                "source.rs".into(),
                "rust",
            );
            let tasks = visual.update(|window, cx| {
                (0..100)
                    .map(|_| provider.hover(&Rope::from("fn example() {}"), 0, window, cx))
                    .collect::<Vec<_>>()
            });
            let completed = Arc::new(Mutex::new(None));
            let mut waiter = None;
            if mode.starts_with("drop") {
                if mode == "drop-pending" {
                    visual.run_until_parked();
                }
                drop(tasks);
            } else {
                let recorded = completed.clone();
                waiter = Some(visual.update(|_, cx| {
                    cx.spawn(async move |_| {
                        let mut results = Vec::new();
                        for task in tasks {
                            results.push(task.await.map_err(|e| e.to_string()));
                        }
                        *recorded.lock().unwrap() = Some(results);
                    })
                }));
                visual.run_until_parked();
                if mode == "close" {
                    visual.update(|_, cx| {
                        entity.update(cx, |r, _| r.close_document(project, Path::new("source.rs")))
                    });
                } else {
                    // Provider uses a real monotonic deadline. The virtual clock
                    // only wakes its next check; it cannot replace elapsed time.
                    std::thread::sleep(Duration::from_millis(5050));
                }
                visual.executor().advance_clock(Duration::from_millis(32));
                visual.run_until_parked();
                assert!(
                    completed.lock().unwrap().as_ref().is_some_and(
                        |results| results.len() == 100 && results.iter().all(Result::is_err)
                    ),
                    "{mode}: provider must finish with its existing failure"
                );
            }
            visual.run_until_parked();
            let retained = cx.update(|cx| {
                entity.update(cx, |r, _| {
                    let client = &mut r.servers.get_mut(&(project, "rust")).unwrap().client;
                    crate::lsp::client::lifetime_tests::released(client);
                    client.pending_count()
                })
            });
            assert_eq!(
                retained, 0,
                "{mode}: late reply retained without another ask"
            );
            drop(waiter);
        }
    }

    #[test]
    fn root_lsp_lifetimes_are_owned() {
        let state = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::lsp::lifetime_tests::owned_",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated Root LSP lifetime regressions failed"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_unrendered_root_quit_stops_server_and_descendant(cx: &mut TestAppContext) {
        let project = tempfile::tempdir().unwrap();
        let mut fake = LspFixture::new();
        let guard = root(cx, project.path());
        start_fake(cx, &guard, &mut fake);
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        let mut unrelated = guild_session::process::ProcessGroup::spawn(&mut command).unwrap();
        // Keep a strong reference to Root: quit must stop processes explicitly,
        // even when entity Drop is not delivered and Root was never rendered.
        cx.quit();
        fake.assert_stopped();
        assert!(unrelated.try_wait().unwrap().is_none());
        drop(guard);
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_project_close_stops_server_and_descendant(cx: &mut TestAppContext) {
        let project = tempfile::tempdir().unwrap();
        let mut fake = LspFixture::new();
        let guard = root(cx, project.path());
        let id = start_fake(cx, &guard, &mut fake);
        cx.update(|cx| {
            guard
                .root
                .as_ref()
                .unwrap()
                .update(cx, |r, cx| r.close_project_now(id, cx))
        });
        fake.assert_stopped();
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_root_release_stops_server_and_descendant(cx: &mut TestAppContext) {
        let project = tempfile::tempdir().unwrap();
        let mut fake = LspFixture::new();
        let mut guard = root(cx, project.path());
        start_fake(cx, &guard, &mut fake);
        // Remove the guard's handle without invoking explicit cleanup, so this
        // exercises Client/ProcessGroup Drop via GPUI's actual entity release.
        let root = guard.root.take().unwrap();
        cx.update(|_| drop(root));
        cx.run_until_parked();
        fake.assert_stopped();
    }

    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_fixture_unwind_stops_server_and_descendant(cx: &mut TestAppContext) {
        let project = tempfile::tempdir().unwrap();
        let mut fake = LspFixture::new();
        let guard = root(cx, project.path());
        start_fake(cx, &guard, &mut fake);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = guard;
            panic!("deliberate fixture assertion failure");
        }));
        assert!(failure.is_err());
        fake.assert_stopped();
    }

    #[test]
    fn root_fixtures_do_not_launch_installed_language_servers() {
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::lsp::lifetime_tests::default_root_launch_is_disabled",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", dir.path().join("state"))
            .status()
            .unwrap();
        assert!(
            status.success(),
            "ordinary Root fixture default launch guard failed"
        );
    }

    #[gpui::test]
    #[ignore = "run through root_fixtures_do_not_launch_installed_language_servers"]
    fn default_root_launch_is_disabled(cx: &mut TestAppContext) {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("source.rs"), "fn main() {}").unwrap();
        let guard = root(cx, project.path());
        let mut fake = LspFixture::new();
        let (_, args) = fake.command(true);
        let command = ("/usr/bin/python3".to_owned(), args);
        let root = guard.root.as_ref().unwrap();
        // Positive control proves the absolute executable seam is selected;
        // Client's login PATH cannot redirect it to an installed LSP.
        cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.test_launch_fallback = Some(command.clone());
                r.test_server = Some(command.clone());
                r.ensure_server(
                    r.active_project.unwrap(),
                    Language::of(Path::new("source.rs")).unwrap(),
                    cx,
                );
            })
        });
        fake.ready();
        cx.update(|cx| {
            root.update(cx, |r, _| {
                r.stop_every_server();
                r.test_server = None;
            })
        });
        fake.assert_stopped();
        let mut forbidden = LspFixture::new();
        let (_, args) = forbidden.command(true);
        let started = cx.update(|cx| {
            root.update(cx, |r, cx| {
                r.test_launch_fallback = Some(("/usr/bin/python3".into(), args));
                r.ensure_server(
                    r.active_project.unwrap(),
                    Language::of(Path::new("source.rs")).unwrap(),
                    cx,
                );
                !r.servers.is_empty()
            })
        });
        if started {
            forbidden.ready();
        }
        drop(guard);
        if started {
            forbidden.assert_stopped();
        }
        assert!(
            !started,
            "ordinary Root must not launch even its safe absolute fake fallback"
        );
    }
    #[gpui::test]
    fn clean_document_flush_does_not_rediscover_all_buffers(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                entity.update(cx, |r, cx| {
                    let project = r.active_project.unwrap();
                    for n in 0..20 {
                        let path = std::path::PathBuf::from(format!("clean-{n}.rs"));
                        std::fs::write(dir.path().join(&path), "fn clean() {}\n").unwrap();
                        let file = cx.new(|cx| {
                            crate::file_tab::FileTab::open(
                                dir.path(),
                                &path,
                                r.file_look(),
                                r.ui(),
                                window,
                                cx,
                            )
                        });
                        r.adopt(
                            guild_session::Seat {
                                workspace: r.active_workspace,
                                project,
                            },
                            project,
                            crate::Body::File {
                                path,
                                tab: file,
                                presentation: None,
                                origin: crate::Origin::Project,
                            },
                            cx,
                        );
                    }
                });
            })
            .unwrap();
        cx.run_until_parked();
        let counts = window
            .update(cx, |_, window, cx| {
                entity.update(cx, |r, cx| {
                    let project = r.active_project.unwrap();
                    let files: Vec<_> = r
                        .tabs
                        .iter()
                        .flat_map(crate::Tab::bodies)
                        .filter_map(|body| {
                            if let crate::Body::File {
                                path,
                                tab,
                                origin: crate::Origin::Project,
                                ..
                            } = body
                            {
                                Some((path.clone(), tab.clone()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    for (path, file) in files {
                        file.update(cx, |file, cx| file.realise(window, cx));
                        let editor = file.read(cx).editor().unwrap().clone();
                        r.open_document(project, &path, &editor, cx);
                    }
                    assert_eq!(r.documents.len(), 20);
                    r.flush_documents(cx);
                    document_trace::take();
                    for _ in 0..5 {
                        r.flush_documents(cx);
                    }
                    document_trace::take()
                })
            })
            .unwrap();
        assert_eq!(
            counts,
            [0, 0],
            "clean idle flush must neither observe every document's tabs nor rediscover all due candidates"
        );
    }
    fn document_events(
        cx: &mut TestAppContext,
        root: &Entity<Root>,
        project: ProjectId,
    ) -> Vec<Value> {
        let lease = cx.update(|cx| {
            root.update(cx, |r, _| {
                r.ask_server(project, "rust", "fixture/probe", json!({}))
                    .unwrap()
            })
        });
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(answer) =
                cx.update(|cx| root.read_with(cx, |r, _| r.server_answer(project, "rust", &lease)))
            {
                return answer.unwrap()["documentEvents"]
                    .as_array()
                    .unwrap()
                    .clone();
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_document_events_preserve_real_write_proofs_debounce_and_disposal(
        cx: &mut TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.rs");
        std::fs::write(&path, "fn original() {}\n").unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        cx.update(|cx| entity.update(cx, |r, _| r.settings.autosave = false));
        let mut fake = LspFixture::new();
        fake.record_documents = true;
        let project = start_fake(cx, &guard, &mut fake);
        let mut control = cx.clone();
        let mut file = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx)
            });
            let view = entity
                .read(cx)
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|b| {
                    if let crate::Body::File { tab, .. } = b {
                        Some(tab.clone())
                    } else {
                        None
                    }
                })
                .unwrap();
            file = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        let file = file.unwrap();
        visual.run_until_parked();
        let editor = control.update(|cx| file.read(cx).editor().unwrap().clone());
        let edit = |text: &str, visual: &mut gpui::VisualTestContext| {
            visual.update(|window, cx| {
                editor.update(cx, |e, cx| {
                    e.set_selected_range(0..0, cx);
                    e.replace_all(text.to_owned(), window, cx);
                })
            });
            visual.run_until_parked();
        };
        let saves = |events: &[Value]| {
            events
                .iter()
                .filter(|v| v["method"] == "textDocument/didSave")
                .count()
        };
        edit("fn edited() { /* 🦀 */ }\n", visual);
        control.update(|cx| entity.update(cx, |r, cx| r.flush_documents(cx)));
        assert!(
            !document_events(&mut control, &entity, project)
                .iter()
                .any(|v| v["method"] == "textDocument/didChange"),
            "typing must retain debounce"
        );
        std::thread::sleep(crate::autosave::PAUSE + Duration::from_millis(20));
        control.update(|cx| entity.update(cx, |r, cx| r.flush_documents(cx)));
        assert_eq!(
            document_events(&mut control, &entity, project)
                .iter()
                .filter(|v| v["method"] == "textDocument/didChange")
                .count(),
            1
        );
        visual.update(|_, cx| file.update(cx, |f, cx| f.save(cx).unwrap()));
        visual.run_until_parked();
        assert_eq!(saves(&document_events(&mut control, &entity, project)), 1);
        edit("fn temporary() {}\n", visual);
        visual.update(|window, cx| {
            editor.read(cx).focus_handle(cx).focus(window, cx);
            window.dispatch_action(Box::new(gpui_component::input::Undo), cx);
        });
        visual.run_until_parked();
        assert!(
            !control.update(|cx| file.read(cx).is_dirty()),
            "real Undo must return to saved content"
        );
        control.update(|cx| entity.update(cx, |r, cx| r.flush_documents(cx)));
        assert_eq!(
            saves(&document_events(&mut control, &entity, project)),
            1,
            "undo is not write proof"
        );
        std::fs::write(&path, "fn reloaded() {}\n").unwrap();
        visual.update(|window, cx| file.update(cx, |f, cx| f.reload(window, cx)));
        visual.run_until_parked();
        assert_eq!(
            saves(&document_events(&mut control, &entity, project)),
            1,
            "reload is not write proof"
        );
        edit("fn retry() {}\n", visual);
        std::fs::rename(&path, dir.path().join("backup")).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            visual
                .update(|_, cx| file.update(cx, |f, cx| f.save(cx)))
                .is_err()
        );
        visual.run_until_parked();
        assert!(control.update(|cx| file.read(cx).is_dirty()));
        assert_eq!(
            saves(&document_events(&mut control, &entity, project)),
            1,
            "failed IO cannot invent didSave"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(dir.path().join("backup"), &path).unwrap();
        visual.update(|_, cx| file.update(cx, |f, cx| f.save(cx).unwrap()));
        visual.run_until_parked();
        let events = document_events(&mut control, &entity, project);
        assert_eq!(saves(&events), 2);
        let last_save = events
            .iter()
            .rposition(|v| v["method"] == "textDocument/didSave")
            .unwrap();
        assert_eq!(events[last_save - 1]["method"], "textDocument/didChange");
        assert_eq!(
            events[last_save - 1]["params"]["contentChanges"][0]["text"],
            "fn retry() {}\n"
        );
        let versions: Vec<_> = events
            .iter()
            .filter(|v| v["method"] == "textDocument/didChange")
            .map(|v| v["params"]["textDocument"]["version"].as_i64().unwrap())
            .collect();
        assert!(versions.windows(2).all(|v| v[0] < v[1]));
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.settings.autosave = true;
                let look = r.file_look();
                file.update(cx, |f, cx| f.restyle(look, r.ui(), cx));
            })
        });
        edit("fn autosaved() {}\n", visual);
        visual
            .executor()
            .advance_clock(crate::autosave::PAUSE + Duration::from_millis(20));
        visual.run_until_parked();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "fn autosaved() {}\n"
        );
        assert_eq!(saves(&document_events(&mut control, &entity, project)), 3);
        // A newly initialized server opens the current buffer before its debounce.
        // Returning to the earlier text must still notify that server.
        edit("fn announced() {}\n", visual);
        control.update(|cx| entity.update(cx, |r, cx| r.announce_documents((project, "rust"), cx)));
        edit("fn autosaved() {}\n", visual);
        std::thread::sleep(crate::autosave::PAUSE + Duration::from_millis(20));
        control.update(|cx| entity.update(cx, |r, cx| r.flush_documents(cx)));
        let announced = document_events(&mut control, &entity, project);
        assert!(
            announced
                .iter()
                .rposition(|v| v["method"] == "textDocument/didChange")
                .unwrap()
                > announced
                    .iter()
                    .rposition(|v| v["method"] == "textDocument/didOpen")
                    .unwrap(),
            "returning to pre-announcement text must notify the newly initialized server"
        );
        assert_eq!(
            announced
                .iter()
                .rev()
                .find(|v| v["method"] == "textDocument/didChange")
                .unwrap()["params"]["contentChanges"][0]["text"],
            "fn autosaved() {}\n"
        );
        edit("fn abandoned() {}\n", visual);
        control.update(|cx| {
            entity.update(cx, |r, _| {
                r.close_document(project, Path::new("source.rs"));
                r.tabs.clear();
                assert!(
                    r.document_work
                        .due(Instant::now() + Duration::from_secs(1))
                        .is_empty()
                );
            })
        });
        edit("fn late() {}\n", visual);
        control.update(|cx| {
            entity.update(cx, |r, cx| {
                r.flush_documents(cx);
                assert!(r.documents.is_empty());
            })
        });
        assert!(
            document_events(&mut control, &entity, project)
                .iter()
                .any(|v| v["method"] == "textDocument/didClose")
        );
    }
    #[gpui::test]
    #[ignore = "run through root_lsp_lifetimes_are_owned"]
    fn owned_document_holders_retarget_and_revoke_generations(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("source.rs"), "fn original() {}\n").unwrap();
        let guard = root(cx, dir.path());
        let entity = guard.root.as_ref().unwrap().clone();
        cx.update(|cx| entity.update(cx, |r, _| r.settings.autosave = false));
        let mut first = LspFixture::new();
        first.record_documents = true;
        let project = start_fake(cx, &guard, &mut first);
        let mut control = cx.clone();
        let mut state = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            entity.update(cx, |r, cx| {
                r.open_file_from(crate::Origin::Project, Path::new("source.rs"), window, cx);
                let old = r.active_tab.unwrap();
                let original = r
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find_map(|b| {
                        if let crate::Body::File { tab, .. } = b {
                            Some(tab.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let second = cx.new(|cx| {
                    crate::file_tab::FileTab::open(
                        dir.path(),
                        Path::new("source.rs"),
                        r.file_look(),
                        r.ui(),
                        window,
                        cx,
                    )
                });
                r.watch_file_document(project, Path::new("source.rs"), &second, cx);
                let seat = guild_session::Seat {
                    workspace: r.active_workspace,
                    project,
                };
                for _ in 0..2 {
                    r.adopt(
                        seat,
                        project,
                        crate::Body::File {
                            path: "source.rs".into(),
                            tab: second.clone(),
                            presentation: None,
                            origin: crate::Origin::Project,
                        },
                        cx,
                    );
                }
                state = Some((original, second, old));
            });
            let file = state.as_ref().unwrap().1.clone();
            gpui_component::Root::new(file, window, cx)
        });
        let (original, file, old) = state.unwrap();
        visual.run_until_parked();
        visual.update(|_, cx| {
            entity.update(cx, |r, cx| {
                r.close_tab(old, cx);
                assert_eq!(
                    r.documents[&(project, "source.rs".into())].editor,
                    file.read(cx).editor().unwrap().clone()
                );
            })
        });
        let obsolete = control.update(|cx| original.read(cx).editor().unwrap().clone());
        let editor = control.update(|cx| file.read(cx).editor().unwrap().clone());
        visual.run_until_parked();
        visual.update(|window, cx| {
            obsolete.update(cx, |e, cx| e.replace_all("fn ghost() {}\n", window, cx))
        });
        visual.run_until_parked();
        control.update(|cx| {
            entity.update(cx, |r, _| {
                assert!(
                    r.document_work
                        .due(Instant::now() + Duration::from_secs(1))
                        .is_empty(),
                    "closed independent holder must not schedule work"
                )
            })
        });
        visual.update(|window, cx| {
            editor.update(cx, |e, cx| e.replace_all("fn sibling() {}\n", window, cx))
        });
        visual.run_until_parked();
        visual.update(|_, cx| file.update(cx, |f, cx| f.save(cx).unwrap()));
        visual.run_until_parked();
        assert_eq!(
            document_events(&mut control, &entity, project)
                .iter()
                .filter(|v| v["method"] == "textDocument/didSave")
                .count(),
            1,
            "two shared seats must not duplicate a write notification"
        );
        std::fs::rename(dir.path().join("source.rs"), dir.path().join("moved.rs")).unwrap();
        visual.update(|window, cx| {
            entity.update(cx, |r, cx| {
                r.relocate_project_paths(
                    project,
                    Path::new("source.rs"),
                    Path::new("moved.rs"),
                    window,
                    cx,
                )
            })
        });
        visual.run_until_parked();
        control.update(|cx| {
            entity.update(cx, |r, _| {
                assert!(!r.documents.contains_key(&(project, "source.rs".into())));
                assert!(r.documents.contains_key(&(project, "moved.rs".into())));
            })
        });
        std::fs::write(dir.path().join("moved.rs"), [0, 1, 2]).unwrap();
        visual.update(|window, cx| file.update(cx, |f, cx| f.reload(window, cx)));
        visual.run_until_parked();
        control.update(|cx| entity.update(cx, |r, _| assert!(r.documents.is_empty())));
        std::fs::write(dir.path().join("moved.rs"), "fn reborn() {}\n").unwrap();
        visual.update(|window, cx| file.update(cx, |f, cx| f.reload(window, cx)));
        visual.run_until_parked();
        let fresh = control.update(|cx| file.read(cx).editor().unwrap().clone());
        assert_ne!(fresh.entity_id(), editor.entity_id());
        visual.update(|window, cx| {
            editor.update(cx, |e, cx| {
                e.replace_all("fn obsolete_buffer() {}\n", window, cx)
            })
        });
        visual.run_until_parked();
        control.update(|cx| {
            entity.update(cx, |r, _| {
                assert_eq!(r.documents[&(project, "moved.rs".into())].editor, fresh);
                assert!(
                    r.document_work
                        .due(Instant::now() + Duration::from_secs(1))
                        .is_empty()
                );
            })
        });
        visual.update(|window, cx| {
            fresh.update(cx, |e, cx| {
                e.replace_all("fn disable_pending() {}\n", window, cx)
            })
        });
        visual.run_until_parked();
        control.update(|cx| {
            entity.update(cx, |r, cx| {
                r.settings.lsp_defaults.insert("rust".into(), false);
                r.reconcile_lsp_preferences(cx);
                assert!(r.documents.is_empty());
                assert!(
                    r.document_work
                        .due(Instant::now() + Duration::from_secs(1))
                        .is_empty()
                );
            })
        });
        first.assert_stopped();
        visual.update(|window, cx| {
            fresh.update(cx, |e, cx| {
                e.replace_all("fn while_disabled() {}\n", window, cx)
            })
        });
        visual.run_until_parked();
        control.update(|cx| entity.update(cx, |r, _| assert!(r.documents.is_empty())));
        let mut second = LspFixture::new();
        control.update(|cx| {
            entity.update(cx, |r, cx| {
                r.test_server = Some(second.command(true));
                r.settings.lsp_defaults.insert("rust".into(), true);
                r.reconcile_lsp_preferences(cx);
            })
        });
        start_fake(&mut control, &guard, &mut second);
        visual.update(|window, cx| {
            fresh.update(cx, |e, cx| {
                e.replace_all("fn project_close_pending() {}\n", window, cx)
            })
        });
        visual.run_until_parked();
        control.update(|cx| {
            entity.update(cx, |r, cx| {
                r.close_project_now(project, cx);
                assert!(r.documents.is_empty());
                assert!(
                    r.document_work
                        .due(Instant::now() + Duration::from_secs(1))
                        .is_empty()
                );
            })
        });
        second.assert_stopped();
    }
}
