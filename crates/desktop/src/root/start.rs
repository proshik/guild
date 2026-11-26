//! The start screen's half in the window — M14.5: its focus and keys,
//! the history read on a thread, and what a press does.
//!
//! **Every arm of a press is a deed the window already has.** The default
//! card and a tile are the strip's `+` and its chevron menu, through
//! `open_harness_tab`, so OpenCode's floor holds here without being
//! written twice; a row is `go_to_session_in_workspace`, `reopen_with` or
//! `open_record`, the same halves the Sessions panel and the command line
//! reach. What is decided without a window — which of the three a row
//! comes to, which strokes are the screen's, when a read is due and
//! whether its answer changes the picture — is `crate::start`, and tested
//! there.
//!
//! **The history is read on a thread and posted back on the watch loop's
//! clock**, the way `root::usage` posts its figures: a read opens files
//! and runs `opencode`, which the window's thread must never wait on.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use gpui::{Context, KeyDownEvent, Window};
use guild_session::history::{History, LIMIT, Past, Stores};
use guild_session::{Conversation, HarnessKind, ProjectId, Runs, SessionRecord};

use crate::Root;
use crate::i18n::Text;
use crate::start::{Deed, Resuming, Start, harness_of, hears, read_due, resuming, running};

/// The history being read, and whose folder the screen is for.
#[derive(Default)]
pub(crate) struct StartReading {
    /// The folder the screen was last drawn for; `Root::start` is its.
    folder: Option<PathBuf>,
    /// When the last read began.
    asked_at: Option<Instant>,
    /// The readers' caches between reads — on the thread while one runs.
    /// Kept across folders: they hold each file's facts, not a folder's
    /// answer, and a read drops only what it saw is gone — Claude's, the
    /// asked folder's vanished transcripts; Codex's, what fell out of its
    /// one listing — so a worktree switch and back costs no re-reading.
    history: Option<History>,
    answer: Option<Receiver<Read>>,
}

/// A thread's answer, with the folder it was asked for: the person may
/// have moved to another worktree while it read.
struct Read {
    folder: PathBuf,
    history: History,
    rows: Vec<Past>,
    available: Vec<HarnessKind>,
}

impl Root {
    /// The folder the start screen is drawn for — the active project's —
    /// or `None` when something else fills the middle: a tab, the
    /// window's Settings page, the Sessions panel, or no project at all.
    /// The one predicate the drawing, the focus and the reading share.
    pub(crate) fn start_showing(&self) -> Option<PathBuf> {
        if self.active_body().is_some() || self.settings_page.is_some() {
            return None;
        }
        let project = self.active_project?;
        self.desk
            .project(project)
            .map(|project| project.path.clone())
    }

    /// The history's answer, if one has come, and a new read when one is
    /// due — from the watch loop, beside `poll_usage`.
    ///
    /// **A repaint only when the picture changed** (`docs/traps.md`: a
    /// clock that notifies redraws the whole window): an answer of the
    /// same rows every ten seconds is the common case and draws nothing.
    /// An answer for a folder the person has left is dropped, its caches
    /// kept.
    pub(crate) fn poll_start(&mut self, cx: &mut Context<Root>) {
        let mut changed = self.settle_start();
        let showing = self.start_showing();
        if let Some(folder) = &showing
            && self.start_reading.folder.as_ref() != Some(folder)
        {
            // Another folder: its screen starts empty and on the card,
            // and is read at once — the previous folder's rows must not
            // stand under this one's name while it reads.
            self.start = Start::default();
            self.start_reading.folder = Some(folder.clone());
            self.start_reading.asked_at = None;
            changed = true;
        }
        // An idle screen's ages move on without an answer changing them:
        // a minute turning *just now* into *1m ago* is a changed picture
        // too (R6), and costs one repaint a minute at most.
        if !changed && showing.is_some() && self.start_ages_moved() {
            changed = true;
        }
        if changed && showing.is_some() {
            cx.notify();
        }
        let Some(folder) = showing else {
            return;
        };
        let since = self.start_reading.asked_at.map(|at| at.elapsed());
        if !read_due(
            since,
            self.start_reading.answer.is_some(),
            self.window_focused,
        ) {
            return;
        }
        self.start_reading.asked_at = Some(Instant::now());
        let mut history = self.start_reading.history.take().unwrap_or_default();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            // Asked here, not on the window's thread: the login shell's
            // `PATH` may still be on its way, and `path` waits for it.
            let path = guild_session::login_path::path_or_inherited();
            let stores = Stores::home(path.clone().into());
            let rows = history.recent(&stores, &folder, LIMIT);
            let available = HarnessKind::available_on(&path);
            let _ = send.send(Read {
                folder,
                history,
                rows,
                available,
            });
        });
        self.start_reading.answer = Some(receive);
    }

    /// Whether the rows are this folder's to draw: an answer has come,
    /// and for the folder in front. Between a worktree switch and the
    /// next frame of the watch loop `start` still holds the previous
    /// folder's rows, and they must not stand under this one's name.
    pub(crate) fn start_rows_for(&self, folder: &std::path::Path) -> bool {
        self.start.read && self.start_reading.folder.as_deref() == Some(folder)
    }

    /// The age words the rows would draw now — nothing while they are not
    /// drawn, which is what `render_start` records then too.
    pub(crate) fn start_ages_now(&self) -> Vec<String> {
        match self.start_showing() {
            Some(folder) if self.start_rows_for(&folder) => {
                let ui = self.ui();
                self.start
                    .ages(std::time::SystemTime::now(), |seconds| ui.ago(seconds))
            }
            _ => Vec::new(),
        }
    }

    /// Whether the rows' age words would read differently from the last
    /// drawn.
    fn start_ages_moved(&self) -> bool {
        *self.start_ages.borrow() != self.start_ages_now()
    }

    /// Take in a thread's answer, if it has come; whether the picture
    /// changed.
    fn settle_start(&mut self) -> bool {
        let Some(answer) = &self.start_reading.answer else {
            return false;
        };
        let read = match answer.try_recv() {
            Ok(read) => read,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                // The thread died without a word, and the caches with it:
                // the next read starts cold rather than never.
                tracing::warn!("start screen: the history read ended without an answer");
                self.start_reading.answer = None;
                return false;
            }
        };
        self.start_reading.answer = None;
        self.start_reading.history = Some(read.history);
        let mut changed = false;
        if self.available != read.available {
            self.available = read.available;
            changed = true;
        }
        if self.start_reading.folder.as_ref() == Some(&read.folder) {
            changed |= self.start.answered(read.rows);
        }
        changed
    }

    /// A key on the start screen, from its own element's `on_key_down` —
    /// the pattern of the tree, the Changes panel and the Log: what it
    /// handles it stops, and everything else goes on to the window.
    /// `cmd-1` and `cmd-t` among them: `crate::start::hears` says which.
    pub(crate) fn start_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let stroke = &event.keystroke;
        let modifiers = stroke.modifiers;
        let key = stroke.key.as_str();
        let cmd = modifiers.platform;
        if !hears(
            key,
            cmd,
            modifiers.control || modifiers.alt || modifiers.shift,
        ) {
            return;
        }
        cx.stop_propagation();
        let available = self.available.clone();
        match self.start.key(key, &available) {
            Some(deed) => self.start_deed(deed, window, cx),
            // An arrow moved the highlight or the chip; a dead tile's key
            // moved nothing and costs one frame.
            None => {
                // The lit row into view: the list scrolls, and a
                // highlight walked past its edge would be lit unseen.
                if let crate::start::Lit::Row(index) = self.start.lit {
                    self.start_scroll.scroll_to_item(index);
                }
                cx.notify()
            }
        }
    }

    /// Carry out what a press on the start screen means. Nothing without a
    /// project in front: the screen is drawn only under one.
    ///
    /// **From an empty pane, what it opens fills that pane** — M12.8,
    /// decision 13. The pane is noted before anything opens, the screen
    /// opens its tab the one way it always does, and the tab it opened is
    /// poured into the pane: one path for opening, whatever the place. A
    /// row that brings forward a tab already open pours nothing — that tab
    /// was not opened here.
    pub(crate) fn start_deed(&mut self, deed: Deed, window: &mut Window, cx: &mut Context<Root>) {
        let into = self.empty_pane_in_front();
        // A resume reads the disk on a thread first, and pours what it
        // opened when it comes back.
        if let Deed::Resume { past } = deed {
            let Some(project) = self.active_project else {
                return;
            };
            let from = ResumeFrom {
                into,
                select: false,
            };
            return self.resume_past(project, past, from, window, cx);
        }
        let before = self.tab_ids_if(into);
        self.start_open(deed, window, cx);
        self.pour_opened(into, &before, window, cx);
    }

    /// The tabs there are now, when something opened next may be poured.
    fn tab_ids_if(
        &self,
        into: Option<(crate::TabId, guild_session::PaneId)>,
    ) -> std::collections::HashSet<crate::TabId> {
        match into {
            Some(_) => self.tabs.iter().map(|tab| tab.id).collect(),
            None => Default::default(),
        }
    }

    /// Pour the tab opened since `before` into the empty pane `into`.
    fn pour_opened(
        &mut self,
        into: Option<(crate::TabId, guild_session::PaneId)>,
        before: &std::collections::HashSet<crate::TabId>,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(into) = into else {
            return;
        };
        let Some(opened) = self.active_tab.filter(|id| !before.contains(id)) else {
            return;
        };
        self.pour(opened, into, cx);
        // The body was made this moment and now stands in the pane: the
        // keys go to it, now and at the next frame (`open_tab`'s rule).
        self.focusing_front = true;
        self.refocus(window, cx);
    }

    /// [`Root::start_deed`]'s opening, the same for every place.
    fn start_open(&mut self, deed: Deed, window: &mut Window, cx: &mut Context<Root>) {
        let Some(project) = self.active_project else {
            return;
        };
        match deed {
            Deed::Default => {
                let runs = self.runs_for_default_tab();
                if let Some(kind) = Self::harness_missing(runs) {
                    return self.say_missing(kind, cx);
                }
                self.open_harness_tab(project, runs, window, cx);
            }
            Deed::Tile { tab } => {
                let runs = match harness_of(tab) {
                    Some(kind) => self.runs_for_pick(kind),
                    None => Runs::Shell,
                };
                // A greyed tile takes no click, but the deed is safe on
                // its own: a key or a stale press says why, too.
                if let Some(kind) = Self::harness_missing(runs) {
                    return self.say_missing(kind, cx);
                }
                self.open_harness_tab(project, runs, window, cx);
            }
            Deed::Resume { past } => {
                let from = ResumeFrom {
                    into: None,
                    select: false,
                };
                self.resume_past(project, past, from, window, cx)
            }
        }
    }

    /// Resume a row: bring forward the tab that holds it, reopen the closed
    /// record that is it, or make a new record resuming it — the design's
    /// "Resuming a row".
    ///
    /// **A tab anywhere is brought forward, whatever the `PATH` or the
    /// floor says** — and so is a record left open in a workspace not
    /// restored since the start, whose tab that restore will make
    /// (`crate::start::running`): it is running already, and two processes
    /// appending to one transcript is the one outcome worse than no button. Only what
    /// would start a process is refused — a harness not on the `PATH`, an
    /// OpenCode below its floor — because `reopen_with` and `open_record`
    /// do not pass through `open_harness_tab`, where the strip's check is.
    ///
    /// **What it asks of the disk is asked on a thread** — the harness on
    /// the `PATH`, OpenCode's floor, whether Claude's transcript is there —
    /// and so is the wait for the conversation's last process: a tab closed
    /// a moment ago hangs its program up and gives it a grace to go
    /// (`terminal::retiring`), and a second process on its transcript is
    /// what this path exists to prevent. When the answer comes back the
    /// row is resolved again, and anything that changed meanwhile wins.
    pub(crate) fn resume_past(
        &mut self,
        project: ProjectId,
        past: Past,
        from: ResumeFrom,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let runs = self.runs_for_pick(past.harness);
        let mut resolved = resuming(&past, &self.resume_records());
        if let Resuming::Front(id) = resolved {
            // Selecting its seat restores its workspace first, when that
            // has not happened yet, and then its tab is there to find.
            if !self.go_to_session_in_workspace(id, cx) {
                // No tab, even now: the restore could not bring it back,
                // or its seat is gone. Asked again, a restored workspace's
                // record no longer counts as running — reopened, or a new
                // record when the restore forgot it.
                resolved = resuming(&past, &self.resume_records());
            }
        }
        match resolved {
            // Brought forward above — or its seat cannot be reached, and
            // a second process on its transcript is worse than nothing.
            Resuming::Front(_) => return self.focus_resumed(window, cx),
            // A Claude row whose id no record can carry: nothing to make.
            Resuming::New {
                id: None,
                harness_session: None,
            } => return,
            _ => {}
        }
        // The record whose terminal may still be closing, and Claude's
        // record whose transcript decides how it comes back.
        let held_by = match &resolved {
            Resuming::Reopen { record, .. } => Some(record.to_string()),
            Resuming::New { id: Some(id), .. } => Some(id.to_string()),
            _ => None,
        };
        let transcript = match &resolved {
            Resuming::Reopen {
                record,
                conversation: None,
            } if past.harness == HarnessKind::Claude => Some(record.to_string()),
            _ => None,
        };
        let kind = past.harness;
        let before = self.tab_ids_if(from.into);
        let workspace = self.active_workspace;
        let tag = held_by.clone();
        let work = cx.background_executor().spawn(async move {
            let gone = held_by
                .as_deref()
                .is_none_or(|tag| crate::terminal::retiring::wait_gone(tag, RETIRING_WAIT));
            let missing = Root::harness_missing(runs);
            let floor_missed =
                missing.is_none() && kind == HarnessKind::OpenCode && Root::opencode_floor_missed();
            let claude = transcript.map(|key| {
                let projects = crate::paths::claude_projects_dir();
                guild_session::continuation(
                    guild_session::transcript::has_conversation(&projects, &key),
                    guild_session::transcript::is_id_taken(&projects, &key),
                )
            });
            Prepared {
                gone,
                missing,
                floor_missed,
                claude,
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let prepared = work.await;
            let _ = this.update_in(cx, |root, window, cx| {
                let resume = Resume {
                    workspace,
                    project,
                    past,
                    resolved,
                    from,
                    before,
                    tag,
                };
                root.finish_resume(resume, prepared, window, cx);
            });
        })
        .detach();
    }

    /// [`Root::resume_past`] with the thread's answer.
    fn finish_resume(
        &mut self,
        resume: Resume,
        prepared: Prepared,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Resume {
            workspace,
            project,
            past,
            resolved,
            from,
            before,
            tag,
        } = resume;
        // Asked in one workspace, answered in another: the seat it would
        // open in is not the one the person is looking at, so the press
        // is dropped and a press there reads again.
        if self.active_workspace != workspace
            || !self
                .desk
                .workspace(workspace)
                .is_some_and(|shown| shown.projects.contains(&project))
        {
            return;
        }
        let now = resuming(&past, &self.resume_records());
        if now != resolved {
            // Something opened it, or changed its record, while the disk
            // was read: a tab holding it is brought forward, and anything
            // else is left for the next press to read again.
            if let Resuming::Front(id) = now
                && self.go_to_session_in_workspace(id, cx)
            {
                self.focus_resumed(window, cx);
            }
            return;
        }
        // Gone when the thread looked — and not closed again since, by a
        // tab reopened and shut while it was looking.
        let gone = prepared.gone
            && tag
                .as_deref()
                .is_none_or(|tag| !crate::terminal::retiring::alive(tag));
        if !gone {
            let text = self.ui().text(Text::ResumeStillClosing).to_owned();
            self.warn(text);
            cx.notify();
            return;
        }
        if let Some(kind) = prepared.missing {
            return self.say_missing(kind, cx);
        }
        if prepared.floor_missed {
            let text = self.ui().text(Text::OpenCodeNeedsV2).to_owned();
            self.warn(text);
            cx.notify();
            return;
        }
        // Its own project in front only now that it can start: a refusal
        // says why and leaves the person where they were.
        if from.select && self.active_project != Some(project) {
            self.select_project(project, cx);
        }
        let runs = self.runs_for_pick(past.harness);
        match resolved {
            Resuming::Front(_) => {}
            Resuming::Reopen {
                record,
                conversation,
            } => {
                use guild_session::Continuation;
                // `continue_or_forget`'s answer for Claude, from the
                // transcript the thread looked at.
                let conversation = match (conversation, prepared.claude) {
                    (Some(theirs), _) => Some(Conversation::Resume(theirs)),
                    (None, Some(Continuation::Resume)) => {
                        Some(Conversation::Resume(record.to_string()))
                    }
                    (None, Some(Continuation::Start)) => {
                        Some(Conversation::New(record.to_string()))
                    }
                    (None, Some(Continuation::Impossible)) => {
                        tracing::info!(session = %record, "session cannot be restored, forgetting it");
                        self.desk.remove_session(record);
                        cx.notify();
                        return;
                    }
                    (None, None) => None,
                };
                self.reopen_with(record, conversation, cx);
            }
            Resuming::New {
                id,
                harness_session,
            } => {
                let Some(cwd) = self.desk.project(project).map(|p| p.path.clone()) else {
                    return;
                };
                let mut record = SessionRecord::new(past.title.clone(), project, cwd, runs);
                // What the tab is called: `session_tab_name` reads the
                // topic, and the transcript probe fills it only while it
                // is empty — so the row's title is what the tab shows, now
                // and after a restart.
                record.topic = Some(past.title.clone());
                let conversation = match id {
                    // The record's id *is* Claude's conversation id.
                    Some(id) => {
                        record.id = id;
                        Conversation::Resume(id.to_string())
                    }
                    None => {
                        let theirs = harness_session.unwrap_or_else(|| past.id.clone());
                        record.harness_session = Some(theirs.clone());
                        Conversation::Resume(theirs)
                    }
                };
                self.open_record(project, record, Some(conversation), None, true, cx);
            }
        }
        self.pour_opened(from.into, &before, window, cx);
        self.focus_resumed(window, cx);
    }

    /// A person's gesture, so the tab in front takes the keys — now, and
    /// again at the next frame for a body made this moment, which is
    /// `open_tab`'s rule (`docs/traps.md`: opening a tab focuses it).
    fn focus_resumed(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        self.focusing_front = true;
        self.refocus(window, cx);
    }

    /// Every record as [`resuming`] takes it: `(id, harness_session,
    /// running)`, with [`running`]'s rule over the tabs, the record's own
    /// word and whether its workspace has been restored.
    fn resume_records(&self) -> Vec<(guild_session::SessionId, Option<String>, bool)> {
        self.desk
            .sessions()
            .map(|record| {
                let in_a_tab = self
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::sessions)
                    .any(|shown| shown == record.id);
                let restored = self.restored_workspaces.contains(&record.workspace);
                (
                    record.id,
                    record.harness_session.clone(),
                    running(in_a_tab, record.open, restored),
                )
            })
            .collect()
    }

    /// The harness `runs` needs when its program is not on the login
    /// shell's `PATH` — asked at the press, as the strip's chevron menu
    /// asks at its click. `None` for a shell, and for a harness that is
    /// there.
    pub(crate) fn harness_missing(runs: Runs) -> Option<HarnessKind> {
        let kind = runs.harness()?;
        let path = guild_session::login_path::path_or_inherited();
        (!HarnessKind::available_on(&path).contains(&kind)).then_some(kind)
    }

    /// Say why a press opened nothing: *codex is not on the PATH*.
    pub(crate) fn say_missing(&mut self, kind: HarnessKind, cx: &mut Context<Root>) {
        let text = self.ui().not_on_path(kind.program());
        self.warn(text);
        cx.notify();
    }
}

/// How long a resume waits for the conversation's last process to go: the
/// hang-up's grace, the settling after it and the reap fit well inside.
const RETIRING_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Where a resume was asked from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResumeFrom {
    /// The empty pane in front, which what opens fills (decision 13).
    pub(crate) into: Option<(crate::TabId, guild_session::PaneId)>,
    /// Put the row's project in front before opening — the Sessions
    /// column's row, which may belong to another project.
    pub(crate) select: bool,
}

/// A resume between its asking and the thread's answer.
struct Resume {
    workspace: guild_session::WorkspaceId,
    project: ProjectId,
    past: Past,
    resolved: Resuming,
    from: ResumeFrom,
    before: std::collections::HashSet<crate::TabId>,
    /// The record whose terminal may be closing, looked at again before
    /// anything starts.
    tag: Option<String>,
}

/// What a resume needed from the disk and the processes.
struct Prepared {
    /// The conversation's last process has gone, or there was none.
    gone: bool,
    /// The harness is not on the `PATH`.
    missing: Option<HarnessKind>,
    /// OpenCode is below its floor.
    floor_missed: bool,
    /// Claude's transcript, for a closed Claude record.
    claude: Option<guild_session::Continuation>,
}
