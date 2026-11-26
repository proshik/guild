//! An annotation intent resolves one immutable commit and its historical file.
use super::*;
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Navigation {
    pub epoch: u64,
    pub sha: String,
    pub path: Option<PathBuf>,
    pub original_line: Option<usize>,
}
pub(crate) struct RevealReply {
    pub intent: Navigation,
    pub commit: Result<Commit, GitError>,
    pub files: Result<Vec<FileChange>, GitError>,
}
pub(crate) struct PageReply {
    pub epoch: u64,
    pub skip: usize,
    pub commits: Result<Vec<Commit>, GitError>,
}
impl Root {
    pub(crate) fn reveal_commit(
        &mut self,
        project: ProjectId,
        sha: &str,
        path: Option<PathBuf>,
        original_line: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        if self.active_project != Some(project)
            || sha.len() != 40
            || !sha.bytes().all(|b| b.is_ascii_hexdigit())
            || sha.bytes().all(|b| b == b'0')
        {
            return;
        }
        self.reveal_tool(guild_session::Tool::Git, cx);
        self.show_git_face(guild_session::GitFace::Log, cx);
        self.ensure_log(cx);
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        state.epoch += 1;
        let intent = Navigation {
            epoch: state.epoch,
            sha: sha.to_owned(),
            path,
            original_line,
        };
        state.navigation = Some(intent.clone());
        state.filter = crate::git::Filter::text(sha);
        state.scope = crate::git::Scope::Named(sha.to_owned());
        state.opened = None;
        state.selection = Default::default();
        state.cursor = None;
        state.file_tree = Default::default();
        state.file_selection = Default::default();
        state.speed = None;
        state.error = None;
        state.navigation_error = None;
        state.in_flight = true;
        if let Some(worker) = self.status_workers.get(&project) {
            worker.reveal_commit(intent);
        }
        cx.notify();
    }
    pub(crate) fn reveal_commit_landed(
        &mut self,
        project: ProjectId,
        reply: RevealReply,
        cx: &mut Context<Self>,
    ) {
        let ui = self.ui();
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        if state.epoch != reply.intent.epoch || state.navigation.as_ref() != Some(&reply.intent) {
            return;
        }
        state.navigation = None;
        state.in_flight = false;
        state.exhausted = true;
        state.error = None;
        let commit = match reply.commit {
            Ok(commit) if commit.sha == reply.intent.sha => commit,
            other => {
                let reason = other.err().map(|e| e.to_string()).unwrap_or_else(|| {
                    ui.text(crate::i18n::Text::BlameCommitUnavailable)
                        .to_owned()
                });
                state.commits.clear();
                state.graph = Default::default();
                state.opened = None;
                state.selection = Default::default();
                state.cursor = None;
                state.file_tree = Default::default();
                state.file_selection = Default::default();
                state.error = Some(GitError::Failed(reason));
                cx.notify();
                return;
            }
        };
        state
            .selection
            .click(&commit.sha, &[commit.sha.as_str()], false, false);
        state.cursor = Some(commit.sha.clone());
        state.opened = Some(commit.sha.clone());
        state.commits = vec![commit];
        state.graph = std::rc::Rc::new(crate::log_graph::Graph::of(&state.commits));
        state.scroll.scroll_to_item(0, ScrollStrategy::Top);
        state.file_tree = reply
            .files
            .as_ref()
            .map(|f| crate::files_tree::Tree::new(f))
            .unwrap_or_default();
        state.files.insert(reply.intent.sha.clone(), reply.files);
        let mut unavailable = None;
        if let Some(path) = reply.intent.path {
            let selected = state
                .files
                .get(&reply.intent.sha)
                .and_then(|files| files.as_ref().ok())
                .and_then(|files| {
                    files
                        .iter()
                        .find(|file| file.path == path || file.from.as_ref() == Some(&path))
                })
                .map(|file| file.path.clone());
            if let Some(selected) = selected {
                if let Some(index) = state.file_tree.reveal(&selected) {
                    state.file_scroll.scroll_to_item(index);
                }
                let paths = state.file_paths();
                state.file_selection.click(&selected, &paths, false, false);
            } else {
                state.file_selection = Default::default();
                unavailable = Some(format!(
                    "{}: {} ({})",
                    ui.text(crate::i18n::Text::BlameFileUnavailable),
                    path.display(),
                    reply.intent.sha
                ));
            }
        }
        state.navigation_error = unavailable;
        cx.notify();
    }
    pub(crate) fn scoped_log_landed(
        &mut self,
        project: ProjectId,
        page: PageReply,
        cx: &mut Context<Self>,
    ) {
        if self
            .logs
            .get(&project)
            .is_some_and(|state| state.epoch == page.epoch && state.navigation.is_none())
        {
            self.log_landed(project, page.skip, page.commits, cx);
        }
    }
}
