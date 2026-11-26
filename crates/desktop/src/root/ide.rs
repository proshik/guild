//! What the window answers when `claude` asks — M8.6.
//!
//! The socket and its threads are [`crate::ide::server`]; here is what
//! the tools mean. Most of them are the window saying what it already
//! knows; one of them, `openDiff`, is a question that is not answered
//! until a person answers it.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{Context, Focusable, Window};
use serde_json::{Value, json};

use crate::ide::server::{Event, Server};
use crate::ide::{self, REJECTED};
use guild_session::{PaneId, ProjectId};

use crate::{Root, TabId};

/// A call the window has not answered yet.
///
/// **`openDiff` blocks the agent.** It waits until the person accepts or
/// rejects, so the call is parked with the tab it opened and answered by
/// the gesture that ends it. A tab closed any other way — `cmd-w`, the
/// cross — is a rejection: the person put the proposal away without
/// taking it.
pub(crate) struct Parked {
    /// Which connection asked.
    pub connection: u64,
    /// The id to answer with.
    pub id: Value,
    /// The name the agent gave the tab, which is how it closes it again.
    pub tab_name: String,
    /// The text the proposal tab shows on its right, kept for the answer:
    /// an accepted diff sends it back, and the CLI makes its edit from it.
    /// M5.12, decision 15.
    pub proposed: String,
    /// The pane of the tab that shows it — M12.8: a divided tab may lose
    /// that pane and keep the others, and closing it is the same answer as
    /// closing the tab.
    pub pane: PaneId,
}

impl Root {
    /// Start announcing this window as an IDE, once.
    ///
    /// Called from the render, like the watcher: the projects have to be
    /// loaded before the lock file can name them.
    pub(crate) fn start_ide(&mut self) {
        if !self.integrations.external() {
            return;
        }
        #[cfg(test)]
        self.integrations.assert_external_boundary();
        if self.ide.is_some() {
            return;
        }
        let folders: Vec<PathBuf> = self
            .desk
            .projects()
            .map(|project| project.path.clone())
            .collect();
        if folders.is_empty() {
            return;
        }
        match Server::start(&folders) {
            Ok(server) => self.ide = Some(server),
            Err(err) => {
                // Not a notice: a person who has not asked for an agent
                // does not need to hear that a socket would not open.
                tracing::warn!(%err, "Guild could not announce itself as an IDE");
            }
        }
    }

    /// The port to point a session at, when the window is listening.
    ///
    /// `None` when it is not, and then a `claude` tab is started without
    /// `--ide` at all: a flag naming a socket that does not exist is a
    /// session that spends its first seconds failing to connect.
    pub(crate) fn ide_port(&self) -> Option<u16> {
        self.ide.as_ref().map(|server| server.port)
    }

    /// Everything the socket has said, on the watch loop's clock.
    pub(crate) fn poll_ide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // What the lock file says this window has open, kept true: a
        // project opened after the window was is a directory a `claude`
        // started by hand would not find us from.
        let folders: Vec<PathBuf> = self
            .desk
            .projects()
            .map(|project| project.path.clone())
            .collect();
        if let Some(server) = self.ide.as_mut() {
            server.announce(&folders);
        }
        // Taken in one go rather than iterated from the borrow: answering
        // a call needs the whole window, and the server is part of it.
        let Some(events) = self.ide.as_ref().map(Server::poll) else {
            return;
        };
        for event in events {
            self.note_place(cx);
            match event {
                Event::Connected { connection, pid } => {
                    tracing::info!(connection, ?pid, "a session in this window is using it");
                }
                Event::Gone { connection } => {
                    // Whatever it was waiting for, it is not waiting any
                    // more; the tabs it opened stay, because a person may
                    // still be reading them.
                    self.parked.retain(|_, call| call.connection != connection);
                }
                Event::Ask {
                    connection,
                    id,
                    tool,
                    arguments,
                } => self.answer_tool(connection, id, &tool, &arguments, window, cx),
            }
        }
    }

    /// One tool call.
    fn answer_tool(
        &mut self,
        connection: u64,
        id: Value,
        tool: &str,
        arguments: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.answer_tool_without_history(connection, id, tool, arguments, window, cx);
        self.history.suspended -= 1;
    }

    fn answer_tool_without_history(
        &mut self,
        connection: u64,
        id: Value,
        tool: &str,
        arguments: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = match tool {
            "getWorkspaceFolders" => Some(ide::json_result(&self.workspace_folders())),
            "getOpenEditors" => Some(ide::json_result(&self.open_editors(cx))),
            "getDiagnostics" => Some(ide::json_result(&self.ide_diagnostics(arguments))),
            "checkDocumentDirty" => Some(ide::json_result(&self.document_dirty(arguments, cx))),
            "saveDocument" => Some(self.save_document(arguments, cx)),
            "closeAllDiffTabs" => Some(self.close_proposals(cx)),
            "close_tab" => Some(self.close_named_tab(arguments, cx)),
            // The one that waits.
            "openDiff" => {
                self.open_proposal(connection, id.clone(), arguments, window, cx);
                None
            }
            other => {
                tracing::info!(%other, "the agent asked for a tool the window does not have");
                Some(ide::text_result(format!(
                    "{other} is not something Guild does"
                )))
            }
        };
        if let (Some(answer), Some(server)) = (answer, self.ide.as_ref()) {
            server.answer(connection, id, answer);
        }
    }

    /// The projects this window has open.
    fn workspace_folders(&self) -> Value {
        json!(
            self.desk
                .projects()
                .map(|project| project.path.display().to_string())
                .collect::<Vec<_>>()
        )
    }

    /// The files in tabs, and whether each has unsaved text.
    fn open_editors(&self, cx: &Context<Self>) -> Value {
        let editors: Vec<Value> = self
            .tabs
            .iter()
            .flat_map(|tab| {
                tab.panes
                    .bodies()
                    .into_iter()
                    .map(move |(pane, body)| (tab, pane, body))
            })
            .filter_map(|(tab, pane, body)| match body {
                // **The project's files only.** A tab under a
                // dependency's root is a path relative to that root, and
                // the agent would be told a file of this repository that
                // is not open. M8.8.3.
                crate::Body::File {
                    path,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } => {
                    let root = self.desk.project(tab.project)?.path.clone();
                    Some(json!({
                        "uri": crate::lsp::uri(&root.join(path)),
                        "filePath": root.join(path).display().to_string(),
                        "isDirty": view.read(cx).is_dirty(),
                        // The file in front: its tab's, in its focused pane.
                        "isActive": Some(tab.id) == self.active_tab
                            && pane == tab.panes.focused(),
                    }))
                }
                _ => None,
            })
            .collect();
        json!(editors)
    }

    /// What the language servers say is wrong — for one file when the
    /// agent names one, for every file the window knows about otherwise.
    ///
    /// **This is M8.5's map**, which is why that pass came first.
    fn ide_diagnostics(&self, arguments: &Value) -> Value {
        let wanted = arguments
            .get("uri")
            .and_then(Value::as_str)
            .and_then(crate::lsp::path_of);
        let mut files: Vec<Value> = Vec::new();
        for ((project, path), found) in &self.diagnostics {
            if found.is_empty() {
                continue;
            }
            let Some(root) = self.desk.project(*project).map(|p| p.path.clone()) else {
                continue;
            };
            let full = root.join(path);
            if wanted.as_ref().is_some_and(|only| *only != full) {
                continue;
            }
            files.push(json!({
                "uri": crate::lsp::uri(&full),
                "diagnostics": found,
            }));
        }
        json!(files)
    }

    /// Whether a file has unsaved text in the window.
    fn document_dirty(&self, arguments: &Value, cx: &Context<Self>) -> Value {
        let Some(path) = arguments.get("filePath").and_then(Value::as_str) else {
            return json!({"isDirty": false});
        };
        let path = PathBuf::from(path);
        let dirty = self.tabs.iter().any(|tab| {
            tab.bodies().any(|body| match body {
                crate::Body::File {
                    path: relative,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } => {
                    self.desk
                        .project(tab.project)
                        .is_some_and(|project| project.path.join(relative) == path)
                        && view.read(cx).is_dirty()
                }
                _ => false,
            })
        });
        json!({"isDirty": dirty})
    }

    /// Write a file that has unsaved text.
    fn save_document(&mut self, arguments: &Value, cx: &mut Context<Self>) -> Value {
        let Some(path) = arguments.get("filePath").and_then(Value::as_str) else {
            return ide::text_result("no file was named");
        };
        let path = PathBuf::from(path);
        // Whichever pane shows it — not the tab's focused one.
        let view = self.tabs.iter().find_map(|tab| {
            tab.bodies().find_map(|body| match body {
                crate::Body::File {
                    path: relative,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } if self
                    .desk
                    .project(tab.project)
                    .is_some_and(|project| project.path.join(relative) == path) =>
                {
                    Some(view.clone())
                }
                _ => None,
            })
        });
        let Some(view) = view else {
            return ide::text_result("that file is not open here");
        };
        match self.save_view(view, cx) {
            Ok(()) => ide::text_result("saved"),
            Err(reason) => ide::text_result(format!("not saved: {reason}")),
        }
    }

    /// Show what the agent proposes, and **wait**.
    fn open_proposal(
        &mut self,
        connection: u64,
        id: Value,
        arguments: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = arguments
            .get("new_file_path")
            .or_else(|| arguments.get("old_file_path"))
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let tab_name = arguments
            .get("tab_name")
            .and_then(Value::as_str)
            .unwrap_or("proposal")
            .to_owned();

        let Some(path) = path else {
            self.answer_call(connection, id, ide::text_result("no file was named"));
            return;
        };
        // Refused rather than read as empty: the text is what Apply sends
        // back as the edit, and an empty one empties the file.
        let Some(proposed) = ide::proposed(arguments).map(str::to_owned) else {
            self.answer_call(connection, id, ide::text_result("no text was proposed"));
            return;
        };
        let Some((project, relative)) = self.project_of(&path) else {
            // A file outside every project: the agent is working
            // somewhere this window is not.
            self.answer_call(connection, id, ide::text_result(REJECTED));
            return;
        };

        let Some(tab) = self.open_proposal_tab(project, &relative, &proposed, window, cx) else {
            self.answer_call(connection, id, ide::text_result(REJECTED));
            return;
        };
        // The tab was made just now with one pane, and that pane is the
        // proposal's for as long as it lives.
        let Some(pane) = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| open.panes.focused())
        else {
            self.answer_call(connection, id, ide::text_result(REJECTED));
            return;
        };
        self.parked.insert(
            tab,
            Parked {
                connection,
                id,
                tab_name,
                proposed,
                pane,
            },
        );
        cx.notify();
    }

    /// Answer a parked call — accepted, and **nothing is written here**.
    ///
    /// `FILE_SAVED` with the text the tab showed closes the diff, and the
    /// CLI makes the edit from that text and writes the file itself, with
    /// no second question in its terminal. A window that wrote it first
    /// would make that edit fail on a file that has changed since the agent
    /// read it — see [`ide::ACCEPTED`], where both ways round are measured,
    /// and why the text is a block of its own.
    pub(crate) fn accept_proposal(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let moving = self.tabs.iter().find(|t| t.id == tab).is_some_and(|owner| {
            let Some(call) = self.parked.get(&tab) else {
                return false;
            };
            let Some(crate::Body::Diff {
                path: Some(path), ..
            }) = owner.panes.body_of(call.pane)
            else {
                return false;
            };
            self.file_operations.busy.as_ref().is_some_and(|pending| {
                pending.project == owner.project
                    && pending
                        .sources
                        .iter()
                        .any(|source| path.starts_with(source))
            }) || self.trees.iter().any(|(seat, tree)| {
                seat.project == owner.project
                    && tree.naming.as_ref().is_some_and(|n| {
                        n.what == super::tree::For::Rename
                            && n.from.as_ref().is_some_and(|from| path.starts_with(from))
                    })
            })
        });
        if moving {
            return;
        }
        let Some(call) = self.parked.remove(&tab) else {
            return;
        };
        self.answer_call(call.connection, call.id, ide::accepted(&call.proposed));
        // The proposal's pane, and the tab only when it is all of it: a
        // pane divided off beside it is the person's, not the agent's.
        self.close_pane(tab, call.pane, cx);
    }

    /// The person said no — or closed the tab, which is the same answer.
    pub(crate) fn reject_proposal(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let Some(pane) = self.parked.get(&tab).map(|call| call.pane) else {
            return;
        };
        self.proposal_closing(tab);
        self.close_pane(tab, pane, cx);
    }

    /// A tab that is closing may be a proposal nobody answered: closing
    /// it *is* the answer. What the agent does next is its own to report
    /// (OSC 7501); the refusal no longer moves any status.
    pub(crate) fn proposal_closing(&mut self, tab: TabId) {
        let Some(call) = self.parked.remove(&tab) else {
            return;
        };
        self.answer_call(call.connection, call.id, ide::text_result(REJECTED));
    }

    /// A pane that is closing may be the one a proposal is parked in —
    /// M12.8: its tab stays, but the proposal went with the pane, and that
    /// is the person's answer as a closed tab is.
    pub(crate) fn proposal_pane_closing(&mut self, tab: TabId, pane: PaneId) {
        if self.parked.get(&tab).is_some_and(|call| call.pane == pane) {
            self.proposal_closing(tab);
        }
    }

    /// Whether this tab is a proposal waiting for a person.
    pub(crate) fn is_proposal(&self, tab: TabId) -> bool {
        self.parked.contains_key(&tab)
    }

    fn answer_call(&self, connection: u64, id: Value, result: Value) {
        if let Some(server) = self.ide.as_ref() {
            server.answer(connection, id, result);
        }
    }

    /// Close every proposal the agent has open — its own
    /// `closeAllDiffTabs`, which it sends before proposing anything new.
    fn close_proposals(&mut self, cx: &mut Context<Self>) -> Value {
        let waiting: Vec<TabId> = self.parked.keys().copied().collect();
        for tab in waiting {
            // The agent putting its own diffs away is not a person
            // refusing them: it goes on, so nothing is witnessed.
            if let Some(call) = self.parked.remove(&tab) {
                self.answer_call(call.connection, call.id, ide::text_result(REJECTED));
                self.close_pane(tab, call.pane, cx);
            }
        }
        ide::text_result("closed")
    }

    /// Close the tab the agent named.
    fn close_named_tab(&mut self, arguments: &Value, cx: &mut Context<Self>) -> Value {
        let Some(name) = arguments.get("tab_name").and_then(Value::as_str) else {
            return ide::text_result("no tab was named");
        };
        let found = self
            .parked
            .iter()
            .find(|(_, call)| call.tab_name == name)
            .map(|(tab, _)| *tab);
        match found {
            Some(tab) => {
                // It is closing a diff it has already been answered
                // about: its pane goes, and no second answer is sent.
                if let Some(call) = self.parked.remove(&tab) {
                    self.close_pane(tab, call.pane, cx);
                }
                ide::text_result("closed")
            }
            None => ide::text_result("no such tab"),
        }
    }

    /// The tab for a proposal: the file as it is on the left, what the
    /// agent sent on the right.
    ///
    /// Not `open_diff`: that one looks for a tab already showing the same
    /// subject and goes to it, which is right for a diff of a revision and
    /// wrong here — two proposals about one file are two questions, and
    /// answering one must not answer the other.
    fn open_proposal_tab(
        &mut self,
        project: ProjectId,
        relative: &Path,
        proposed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        self.note_place(cx);
        self.history.suspended += 1;
        let result =
            self.open_proposal_tab_without_history(project, relative, proposed, window, cx);
        self.history.suspended -= 1;
        result
    }

    fn open_proposal_tab_without_history(
        &mut self,
        project: ProjectId,
        relative: &Path,
        proposed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let root = self.desk.project(project).map(|p| p.path.clone())?;
        // What the file says now, which is what the agent's edit is
        // against. `None` is a file it is creating.
        let old = std::fs::read(root.join(relative)).ok();
        let look = self.file_look();
        let ui = self.ui();
        let mode = self.settings.diff_view;
        let path = relative.to_path_buf();
        let proposed = proposed.as_bytes().to_vec();
        let view = cx.new(|cx| {
            crate::diff_tab::DiffTab::open_proposal(&root, path, old, proposed, mode, look, ui, cx)
        });
        self.hear_diff(&view, window, cx);

        let focus = view.read(cx).focus_handle(cx);
        let id = self.adopt(
            self.seat_for_project(project),
            project,
            crate::Body::Diff {
                path: Some(relative.to_path_buf()),
                base: guild_session::Base::Proposal,
                tab: view,
            },
            cx,
        );
        self.active_tab = Some(id);
        window.focus(&focus, cx);
        Some(id)
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[test]
    fn isolated_proposal_navigation() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::ide::navigation_tests::proposals_are_separate_steps",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[gpui::test]
    #[ignore = "run through isolated_proposal_navigation"]
    fn proposals_are_separate_steps(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "before").unwrap();
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), dir.path());
            let entity = cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.active_project = Some(project);
                root.isolated_render = true;
                root
            });
            entity.update(cx, |r, cx| {
                let first = r
                    .open_proposal_tab(project, Path::new("a.txt"), "one", window, cx)
                    .unwrap();
                let second = r
                    .open_proposal_tab(project, Path::new("a.txt"), "two", window, cx)
                    .unwrap();
                assert_ne!(first, second);
                r.go_back(window, cx);
                assert_eq!(r.active_tab, Some(first));
                r.go_forward(window, cx);
                assert_eq!(r.active_tab, Some(second));
                r.close_tab(first, cx);
                r.go_back(window, cx);
                assert_ne!(
                    r.active_tab,
                    Some(first),
                    "a closed proposal cannot be recreated"
                );
            });
            gpui_component::Root::new(entity, window, cx)
        });
        visual.run_until_parked();
    }
}
