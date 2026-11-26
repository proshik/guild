//! A file in a tab: what is behind `Body::File`.
//!
//! The editor inside is the component's; what is ours is around it — the
//! file's state on disk, the language it is given, the face and the
//! leading it is set in, the dirt and the save. A tab whose file is not
//! text, is too large, or is gone shows one quiet line and stays: a
//! person may be reading what the agent is about to rewrite.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window, div, px, rgb,
};
use gpui_component::input::{Editor, EditorState, InputEvent, TabSize};

use crate::autosave::{self, Facts, Refusal, Trigger, Verdict};
use crate::highlight::{self, Indent, Language, LineEnding, PLAIN_INDENT, Text};
use crate::i18n::{Text as Label, Ui};
use crate::theme;
mod blame;
mod git_markers;
pub(crate) mod gutter;
pub mod image;
mod launch;
mod read;
pub use blame::{BlameRequest, RevealCommit};
pub use git_markers::HunkRequest;
pub use launch::LaunchRun;

/// Past this the file is shown plain: tree-sitter on twenty megabytes is
/// a wait, and a person asked to read.
pub const HIGHLIGHT_LIMIT: u64 = 2 * 1024 * 1024;

/// Past this the file is not opened at all.
pub const OPEN_LIMIT: u64 = 20 * 1024 * 1024;

/// What the file turned out to be when it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileState {
    /// Text, in the editor.
    Text,
    /// Not text: a NUL among the first bytes, or not UTF-8.
    NotText,
    /// Bigger than [`OPEN_LIMIT`].
    TooLarge,
    /// The path is not on disk any more.
    Gone,
    /// The read failed, with the system's words.
    CannotRead(String),
}

/// How a file tab is set: the terminal's face and size, the palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The code face, already resolved against what is installed: the
    /// terminal's rule, applied once by the window rather than per frame.
    pub family: SharedString,
    /// The terminal's size, which is the code size.
    pub font_size: f32,
    /// The palette.
    pub theme: theme::Theme,
    /// Whether the buffer writes itself — the setting.
    pub autosave: bool,
}

impl Look {
    /// The code size, a pixel count.
    pub fn text_size(&self) -> gpui::Pixels {
        px(self.font_size)
    }

    /// A line of code: the code size at [`theme::leading::CODE`] — the
    /// editor's line, and a popup preview's.
    pub fn line_height(&self) -> gpui::Pixels {
        self.text_size() * theme::leading::CODE
    }
}

/// A tab's mark in the strip: the unsaved dot, and its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Edited here and not yet written: `accent.brass`.
    Unsaved,
    /// Edited here **and** changed on disk meanwhile: `accent.rust`. A
    /// save would write over someone else's change.
    Conflict,
}

/// The mark for a tab in this state. A clean tab has none whatever the
/// disk did: a clean tab is reloaded rather than marked.
pub fn mark(dirty: bool, changed_on_disk: bool) -> Option<Mark> {
    match (dirty, changed_on_disk) {
        (false, _) => None,
        (true, false) => Some(Mark::Unsaved),
        (true, true) => Some(Mark::Conflict),
    }
}

/// What a save of ours left on disk, so that the watcher's report of it
/// is recognised as ours and not taken for someone else's change.
///
/// Length and modification time, not a hash: they are what the file
/// system hands over for free, and a change that keeps both to the
/// nanosecond is one nobody made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRecord {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl SaveRecord {
    fn of(path: &Path) -> std::io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        Ok(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }

    /// Whether the file on disk is still what we wrote.
    pub fn matches(&self, path: &Path) -> bool {
        Self::of(path).is_ok_and(|now| now == *self)
    }
}

/// Write a file the way the state file is written: beside it, synced,
/// then renamed over it — a crash leaves the old file or the new one,
/// never half of either. The old file's permissions go onto the new
/// one, the lines end the way they ended on disk, and nothing is added:
/// a save is not an edit, and a file that had no final newline keeps
/// not having one.
pub fn save_atomically(
    path: &Path,
    text: &str,
    line_ending: LineEnding,
) -> std::io::Result<SaveRecord> {
    save_atomically_with_directory(path, text, line_ending, |dir| std::fs::File::open(dir))
}

fn save_atomically_with_directory(
    path: &Path,
    text: &str,
    line_ending: LineEnding,
    open_directory: impl FnOnce(&Path) -> std::io::Result<std::fs::File>,
) -> std::io::Result<SaveRecord> {
    use std::io::Write;

    // Reading follows links; saving must address the same file. In particular,
    // a dangling link is an error, not permission to replace the link itself.
    let resolved;
    let path = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            resolved = path.canonicalize()?;
            resolved.as_path()
        }
        Ok(_) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path,
        Err(error) => return Err(error),
    };
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Hidden, and named after the process: two windows saving one file
    // must not write into each other's draft.
    let draft = dir.join(format!(".{name}.guild-{}~", std::process::id()));
    let permissions = match std::fs::metadata(path) {
        Ok(meta) => Some(meta.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    // Refuse before publishing any bytes when the directory cannot be opened
    // for the required durability sync. Keep the handle across the rename.
    let directory = open_directory(dir)?;
    let written = (|| {
        let mut file = std::fs::File::create(&draft)?;
        file.write_all(line_ending.apply(text).as_bytes())?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        // Sync permissions and bytes before publishing the replacement.
        file.sync_all()?;
        drop(file);
        std::fs::rename(&draft, path)?;
        // The directory too: that is what survives a power cut rather
        // than only a dead process.
        directory.sync_all()?;
        SaveRecord::of(path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&draft);
    }
    written
}

/// A file in a tab.
pub struct FileTab {
    git_markers: git_markers::Tracking,
    image: Option<image::Preview>,
    loading: read::Loading,
    root: PathBuf,
    relative: PathBuf,
    language: Option<&'static Language>,
    state: FileState,
    line_ending: LineEnding,
    /// Presentation state belongs to this open file, never its saved bytes.
    soft_wrap: bool,
    editor: Option<Entity<EditorState>>,
    /// Text read with no window to make an editor in yet — a tab
    /// restored at start-up. The first render makes the editor.
    pending: Option<String>,
    /// The text as it stands on disk — read, or written by us. What
    /// the editor holds is compared with it on every change.
    clean: String,
    dirty: bool,
    changed_on_disk: bool,
    /// Our last write, for the watcher to recognise.
    saved: Option<SaveRecord>,
    /// The last write failed, and how: the line above the editor says so,
    /// and the pause does not retry — the next change or a blur does.
    write_failed: Option<String>,
    /// The write armed by the last change, if the pause has not passed.
    /// Dropping it is cancelling it.
    pending_write: Option<gpui::Task<()>>,
    relocating: bool,
    look: Look,
    ui: Ui,
    /// A dependency's source: shown, never written. Two guards, because
    /// one of them is somebody else's — the component's `readonly`, and
    /// the refusal in [`FileTab::save`] and the autosave table, which
    /// would outlast a change upstream. M5.11's decision 7.
    read_only: bool,
    /// A file of a project group's folder: no git markers are asked for,
    /// since nothing of git runs for a group root but its branch label —
    /// GLD-055 decision 7.
    without_git: bool,
    /// For the states with no editor to focus.
    focus: FocusHandle,
    _changes: Option<Subscription>,
    occurrences: Option<Entity<crate::editor::Occurrences>>,
    smart_selection: Option<Entity<crate::editor::SmartSelection>>,
    /// The selections the editing actions leave, for undo and redo.
    journal: crate::editor::editing::Journal,
    suppress_presentation_blur: bool,
    /// Where the editor was drawn last frame, in the window — written by a
    /// `canvas` beside it after paint. Hidden source clears this geometry.
    drawn_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The text's left edge, including padding, measured from public editor
    /// geometry. With `drawn_at`, this locates the gutter's Annotate menu.
    gutter_width: std::rc::Rc<std::cell::Cell<Option<gpui::Pixels>>>,
    task_generation: u64,
    tasks: Vec<crate::run::source::Task>,
    gutter_snapshot: gutter::Snapshot,
    scroll: crate::file_scroll::Scroll,
    launch: Option<launch::Form>,
    launch_arguments: std::collections::HashMap<crate::run::source::Identity, launch::Arguments>,
    blame: blame::Inline,
    local_search: Option<Entity<crate::local_search::SearchBar>>,
}

/// Read a file into what a tab shows: the state, and the text when it
/// is text.
pub fn read(path: &Path) -> (FileState, Option<(String, LineEnding)>) {
    match read_bytes(path) {
        Ok(bytes) => match Text::of(&bytes) {
            Text::Source { text, line_ending } => (FileState::Text, Some((text, line_ending))),
            Text::Binary => (FileState::NotText, None),
        },
        Err(state) => (state, None),
    }
}

/// Validate both the path and the opened object: a FIFO can replace a file
/// between stat and open. NONBLOCK prevents that race waiting for a writer;
/// fstat then refuses it without reading. Symlinks to regular files still work.
fn read_bytes(path: &Path) -> Result<Vec<u8>, FileState> {
    let (file, meta) = open_regular(path)?;
    if meta.len() > OPEN_LIMIT {
        return Err(FileState::TooLarge);
    }
    read_limited(file)
}

/// Open a path for reading only while it is a regular file, and answer the
/// opened handle's own metadata — the one object every later read of it
/// must come from, so nothing swapped in at the path is opened again.
pub(crate) fn open_regular(path: &Path) -> Result<(std::fs::File, std::fs::Metadata), FileState> {
    use std::os::unix::fs::OpenOptionsExt;
    let error = |err: std::io::Error| {
        if err.kind() == std::io::ErrorKind::NotFound {
            FileState::Gone
        } else {
            FileState::CannotRead(err.to_string())
        }
    };
    let regular = |meta: &std::fs::Metadata| {
        if meta.is_file() {
            Ok(())
        } else {
            Err(FileState::CannotRead("Not a regular file".into()))
        }
    };
    regular(&std::fs::metadata(path).map_err(error)?)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
        .map_err(error)?;
    let meta = file.metadata().map_err(error)?;
    regular(&meta)?;
    Ok((file, meta))
}

fn read_limited(reader: impl std::io::Read) -> Result<Vec<u8>, FileState> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .take(OPEN_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| FileState::CannotRead(e.to_string()))?;
    if bytes.len() as u64 > OPEN_LIMIT {
        return Err(FileState::TooLarge);
    }
    Ok(bytes)
}

impl FileTab {
    /// Open a file of a project.
    pub fn open(
        root: &Path,
        relative: &Path,
        look: Look,
        ui: Ui,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tab = Self::restore(root, relative, look, ui, cx);
        tab.realise(window, cx);
        tab
    }

    /// Schedule a bounded background read. The first render after completion
    /// makes the editor; both opening and startup restoration use this path.
    pub fn restore(
        root: &Path,
        relative: &Path,
        look: Look,
        ui: Ui,
        cx: &mut Context<Self>,
    ) -> Self {
        let language = Language::of(relative);
        let image_format = image::format(relative);
        let mut tab = Self {
            git_markers: Default::default(),
            image: image_format.map(image::Preview::new),
            loading: Default::default(),
            root: root.to_path_buf(),
            relative: relative.to_path_buf(),
            language,
            state: FileState::NotText,
            line_ending: LineEnding::Lf,
            soft_wrap: language.is_some_and(|l| l.name == "markdown"),
            editor: None,
            pending: None,
            clean: String::new(),
            dirty: false,
            changed_on_disk: false,
            saved: None,
            local_search: None,
            write_failed: None,
            pending_write: None,
            relocating: false,
            look,
            ui,
            read_only: false,
            without_git: false,
            focus: cx.focus_handle(),
            _changes: None,
            occurrences: None,
            smart_selection: None,
            journal: Default::default(),
            suppress_presentation_blur: false,
            drawn_at: Default::default(),
            gutter_width: Default::default(),
            task_generation: 0,
            tasks: Vec::new(),
            gutter_snapshot: Default::default(),
            scroll: Default::default(),
            launch: None,
            launch_arguments: Default::default(),
            blame: Default::default(),
        };
        if tab.image.is_some() {
            Self::own_image_atlas(cx);
        }
        if tab.image.is_some() {
            tab.load_image(true, cx);
        } else {
            tab.load_text(false, None, cx);
        }
        tab.discover_tasks(cx);
        tab
    }

    /// Mark the tab as a dependency's source, read and never written.
    /// Stop or start asking git for line markers on an open tab — its root
    /// became a project group's folder, or stopped being one.
    pub fn set_without_git(&mut self, without_git: bool, cx: &mut Context<Self>) {
        if self.without_git == without_git {
            return;
        }
        self.without_git = without_git;
        self.git_markers.invalidate();
        if !without_git {
            self.ensure_git_markers(cx);
        }
        cx.notify();
    }

    /// Ask git for no line markers — a project group folder's file.
    pub fn without_git(mut self, without_git: bool) -> Self {
        self.without_git = without_git;
        if without_git {
            self.git_markers.invalidate();
        }
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        if read_only {
            self.git_markers.invalidate();
        }
        self
    }

    /// Make the editor from the text read, now that there is a window.
    /// If the tab was given the focus meanwhile, the editor takes it:
    /// the handle it went to is the one for tabs with nothing to edit.
    fn discard_local_search(&mut self, cx: &mut Context<Self>) {
        if let Some(search) = self.local_search.take() {
            search.update(cx, |search, cx| search.dispose(cx));
        }
    }
    pub(crate) fn realise(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.loading.window = Some(window.window_handle());
        self.realise_text(window, cx);
        self.realise_image_source(window, cx);
        let Some(text) = self.pending.take() else {
            return;
        };
        let editor = Self::editor_for(self.language, self.soft_wrap, text, window, cx);
        self._changes = Some(Self::watch(&editor, cx));
        self.occurrences = Some(crate::editor::Occurrences::attach(
            &editor,
            self.look.theme,
            cx,
        ));
        if self.focus.is_focused(window) && !self.image.as_ref().is_some_and(|p| !p.source) {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        }
        self.discard_local_search(cx);
        self.smart_selection = Some(crate::editor::SmartSelection::attach(&editor, cx));
        self.editor = Some(editor);
        cx.emit(BufferChanged);
        // **Whatever waits for this buffer is asked on the next frame, and
        // there has to be one.** The root lands a jump, tells a server and
        // listens for edits at its own render, which already ran this
        // frame; a `notify` from inside a render is swallowed (GPUI's
        // `invalidate_view` does not mark a window dirty mid-draw), so a
        // restored tab opened by a search hit stood on line 1 until the
        // pointer happened to move — and the hit's selection, which waits
        // five frames for the caret, had given up by then. M5.11's window
        // check 5. Not `request_animation_frame`: that reads the view being
        // rendered and panics outside a draw, and `open` realises from the
        // constructor.
        let tab = cx.entity_id();
        window.on_next_frame(move |_, cx| cx.notify(tab));
    }

    /// An editor holding this text in this language.
    fn editor_for(
        language: Option<&'static Language>,
        soft_wrap: bool,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        // Plain past the limit: the grammar is what decides whether
        // tree-sitter runs, and `text` is the grammar with no query.
        let id = if text.len() as u64 > HIGHLIGHT_LIMIT {
            highlight::PLAIN
        } else {
            language.map_or(highlight::PLAIN, |language| {
                crate::editor::grammar_or_plain(language.grammar)
            })
        };
        cx.new(|cx| {
            EditorState::new(window, cx)
                .language(id)
                .tab_size(tab_size_for(
                    language.map_or(PLAIN_INDENT, |language| language.indent),
                ))
                .line_number(false)
                .soft_wrap(soft_wrap)
                .searchable(false)
                .default_value(text)
        })
    }

    fn semantic_selection(&mut self, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &self.editor
            && editor.read(cx).focus_handle(cx).is_focused(window)
            && let Some(selection) = &self.smart_selection
        {
            selection.update(cx, |selection, cx| selection.select(editor, extend, cx));
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }

    /// One of the line-editing actions, on the focused buffer of an editable
    /// file. The comment tokens are the file's grammar's, even past the
    /// highlighting limit, where the editor itself is handed plain text.
    fn edit(
        &mut self,
        op: crate::editor::editing::Op,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = &self.editor
            && !self.read_only
            && editor.read(cx).focus_handle(cx).is_focused(window)
        {
            let grammar = self.language.map_or(highlight::PLAIN, |l| l.grammar);
            crate::editor::editing::apply(editor, op, grammar, &mut self.journal, window, cx);
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }

    /// Around the component's own undo or redo of the focused buffer: once
    /// it has run, give back the selection an editing action had there.
    fn replay_selection(&mut self, undo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.clone()
            && editor.read(cx).focus_handle(cx).is_focused(window)
            && self.journal.replaying(&editor, undo, cx)
        {
            cx.defer_in(window, move |tab, _, cx| {
                tab.journal.replayed(&editor, undo, cx);
            });
        }
    }

    /// Hear the editor's changes, and its losing the focus.
    fn watch(editor: &Entity<EditorState>, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe(editor, |this, editor, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.journal.changed(&editor, cx);
                this.changed(&editor, cx)
            }
            InputEvent::Blur if !std::mem::take(&mut this.suppress_presentation_blur) => {
                this.autosave(Trigger::Blur, cx);
            }
            _ => {}
        })
    }

    /// The facts the autosave table decides on, at this moment.
    fn facts(&self, trigger: Trigger) -> Facts {
        Facts {
            enabled: self.look.autosave,
            dirty: self.dirty,
            is_text: self.editor.is_some(),
            changed_on_disk: self.changed_on_disk,
            last_write_failed: self.write_failed.is_some(),
            writable: !self.read_only && !self.relocating && !self.loading.pending(),
            trigger,
        }
    }

    /// Write the buffer if the table says so. The mechanism is here; the
    /// decision is `autosave::should_write`, which is a test.
    pub fn autosave(&mut self, trigger: Trigger, cx: &mut Context<Self>) -> Verdict {
        let verdict = autosave::should_write(self.facts(trigger));
        if verdict == Verdict::Write {
            match self.save(cx) {
                Ok(()) => {}
                Err(reason) => {
                    tracing::warn!(file = %self.name(), %reason, "autosave failed");
                    self.write_failed = Some(reason);
                    cx.notify();
                }
            }
        }
        verdict
    }

    /// Arm the write for [`autosave::PAUSE`] after this change; the next
    /// change disarms it and arms its own.
    fn arm_autosave(&mut self, cx: &mut Context<Self>) {
        if !self.look.autosave {
            return;
        }
        self.pending_write = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(autosave::PAUSE).await;
            let _ = this.update(cx, |tab, cx| {
                tab.pending_write = None;
                tab.autosave(Trigger::Pause, cx);
            });
        }));
    }

    /// Why the last autosave did not write, if it did not.
    pub fn write_failure(&self) -> Option<&str> {
        self.write_failed.as_deref()
    }

    /// Whether the buffer is refused by the autosave table for a reason
    /// other than being clean — a conflict — and so has to be asked about.
    pub fn refuses_autosave(&self) -> Option<Refusal> {
        match autosave::should_write(self.facts(Trigger::Close)) {
            Verdict::Refuse(Refusal::Clean) | Verdict::Write => None,
            Verdict::Refuse(refusal) => Some(refusal),
        }
    }

    /// The editor changed. Dirt is the buffer differing from the disk,
    /// not a count of edits: undo all the way back is a clean tab, and
    /// the dot goes with it — what a person expects of `cmd-z`.
    fn changed(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        self.request_git_markers(cx);
        self.edited_during_read();
        self.invalidate_image_source(cx);
        self.remap_blame(cx);
        cx.emit(BufferChanged);
        self.discover_tasks(cx);
        let dirty = editor.read(cx).text() != self.clean.as_str();
        // A new change is a new chance for a write that failed.
        self.write_failed = None;
        if dirty != self.dirty {
            self.dirty = dirty;
            cx.notify();
        }
        if dirty {
            self.arm_autosave(cx);
        }
    }

    /// The file's name, for a question about it.
    pub fn name(&self) -> String {
        self.relative
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.relative.display().to_string())
    }

    /// Whether the buffer differs from the disk.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The dot for the tab strip, if any.
    pub fn mark(&self) -> Option<Mark> {
        mark(self.dirty, self.changed_on_disk)
    }

    /// Our last write, for the watcher to tell from someone else's.
    pub fn last_save(&self) -> Option<&SaveRecord> {
        self.saved.as_ref()
    }

    /// Write the buffer to disk. A failure comes back in the OS's words
    /// and leaves the tab as it was: dirty, with the text intact.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self.read_only || self.relocating {
            return Err(self.ui.text(Label::FileReadOnly).to_owned());
        }
        let Some(editor) = &self.editor else {
            return Ok(());
        };
        let snapshot = editor.read(cx).text().clone();
        let text = snapshot.to_string();
        let record = save_atomically(&self.path(), &text, self.line_ending)
            .map_err(|err| err.to_string())?;
        self.loading.cancel();
        self.saved = Some(record);
        self.clean = text;
        let proof = Saved {
            path: self.path(),
            text: snapshot,
        };
        self.dirty = false;
        self.changed_on_disk = false;
        self.write_failed = None;
        self.pending_write = None;
        // A file that had gone is back: we just wrote it.
        self.state = FileState::Text;
        self.refresh_blame(cx);
        cx.emit(proof);
        cx.notify();
        Ok(())
    }

    /// Read the file again and show what it says now, dropping the
    /// buffer. The caret keeps its row, so a file the agent rewrote under
    /// a reader does not jump to its top.
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_git_baseline(cx);
        if self.image.is_some() {
            self.dirty = false;
            self.pending_write = None;
            self.load_image(true, cx);
            return;
        }
        self.load_text(false, Some(window), cx);
    }

    fn install_read(
        &mut self,
        state: FileState,
        text: Option<(String, LineEnding)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // This read supersedes restore's deferred snapshot, including when
        // the file is gone or no longer text. A later realise must not replay it.
        self.pending = None;
        match text {
            Some((text, ending)) => {
                self.line_ending = ending;
                self.clean = text.clone();
                match &self.editor {
                    Some(editor) => {
                        let position = editor.read(cx).cursor_position();
                        // Setting the caret focuses the editor, and a
                        // reload the watcher asked for must not take the
                        // focus from wherever a person is typing.
                        let focused = window.focused(cx);
                        editor.update(cx, |editor, cx| {
                            editor.set_value(text, window, cx);
                            editor.set_cursor_position(position, window, cx);
                        });
                        // Reload may be installed while drawing, when an
                        // editor notification alone need not wake its observer.
                        if let Some(occurrences) = &self.occurrences {
                            occurrences
                                .update(cx, |occurrences, cx| occurrences.refresh(editor, cx));
                        }
                        if let Some(search) = &self.local_search {
                            search.update(cx, |search, cx| search.source_reloaded(cx));
                        }
                        match focused {
                            Some(handle) => window.focus(&handle, cx),
                            None => window.blur(cx),
                        }
                    }
                    None => {
                        let editor =
                            Self::editor_for(self.language, self.soft_wrap, text, window, cx);
                        self.discard_local_search(cx);
                        self._changes = Some(Self::watch(&editor, cx));
                        self.occurrences = Some(crate::editor::Occurrences::attach(
                            &editor,
                            self.look.theme,
                            cx,
                        ));
                        self.smart_selection =
                            Some(crate::editor::SmartSelection::attach(&editor, cx));
                        self.editor = Some(editor);
                    }
                }
            }
            None => {
                self.discard_local_search(cx);
                self.smart_selection = None;
                self.editor = None;
                self._changes = None;
                self.occurrences = None;
                self.clean.clear();
            }
        }
        self.state = state;
        self.refresh_git_baseline(cx);
        self.dirty = false;
        self.changed_on_disk = false;
        self.discover_tasks(cx);
        cx.emit(BufferChanged);
        self.refresh_blame(cx);
        cx.notify();
    }

    /// The watcher says the file changed. Our own save is nothing; a
    /// clean tab follows the disk; a dirty one keeps its text and says
    /// so, and the save that would write over the change is a person's
    /// decision.
    pub fn disk_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.relocating {
            return;
        }
        let path = self.path();
        if self
            .saved
            .as_ref()
            .is_some_and(|record| record.matches(&path))
        {
            return;
        }
        if self.dirty {
            self.refresh_blame(cx);
            if !self.changed_on_disk {
                self.changed_on_disk = true;
                cx.notify();
            }
            return;
        }
        self.reload(window, cx);
    }

    /// Freeze disk writes and watcher reloads while a filesystem move is pending.
    pub(crate) fn relocating(&mut self, value: bool, window: &mut Window, cx: &mut Context<Self>) {
        let was_relocating = self.relocating;
        self.relocating = value;
        self.refresh_git_baseline(cx);
        self.refresh_blame(cx);
        if value {
            self.loading.cancel();
            self.pending_write = None;
        } else if was_relocating {
            self.load_text(true, Some(window), cx);
        }
    }

    /// Retarget the existing buffer without replacing its input state or history.
    pub(crate) fn relocate(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.loading.cancel();
        self.relative = relative.into();
        self.refresh_git_baseline(cx);
        self.refresh_blame(cx);
        self.discover_tasks(cx);
        let language = Language::of(relative);
        if self.language.map(|l| l.name) != language.map(|l| l.name) {
            self.language = language;
            if let Some(editor) = &self.editor {
                let grammar = if editor.read(cx).text().len() as u64 > HIGHLIGHT_LIMIT {
                    highlight::PLAIN
                } else {
                    language.map_or(highlight::PLAIN, |l| {
                        crate::editor::grammar_or_plain(l.grammar)
                    })
                };
                editor.update(cx, |editor, cx| {
                    editor.set_highlighter(grammar, cx);
                    editor.set_tab_size(
                        tab_size_for(language.map_or(PLAIN_INDENT, |l| l.indent)),
                        cx,
                    );
                });
            }
        }
        // Reconcile after the move freeze is released, or immediately for a
        // caller that retargets an unfrozen tab. Never read on the UI thread.
        if !self.relocating {
            self.load_text(true, Some(window), cx);
        }
        cx.notify();
    }

    /// The path relative to the project.
    pub fn relative(&self) -> &Path {
        &self.relative
    }

    pub fn tasks(&self) -> &[crate::run::source::Task] {
        &self.tasks
    }

    pub(crate) fn discover_tasks(&mut self, cx: &mut Context<Self>) {
        self.task_generation += 1;
        self.tasks.clear();
        cx.notify();
        if self.read_only || crate::run::source::Runner::of(&self.relative).is_none() {
            return;
        }
        let generation = self.task_generation;
        let path = self.relative.clone();
        let root = self.root.clone();
        let source = self
            .editor
            .as_ref()
            .map(|e| e.read(cx).text().to_string())
            .or_else(|| self.pending.clone())
            .unwrap_or_else(|| self.clean.clone());
        cx.spawn(async move |tab, cx| {
            let tasks = cx
                .background_executor()
                .spawn(async move {
                    let catalogue = crate::run::discover::catalogue(&root);
                    let allowed = catalogue.sources.contains(&path);
                    let mut tasks = if allowed {
                        crate::run::source::parse(&path, &source)
                    } else {
                        Vec::new()
                    };
                    for task in &mut tasks {
                        if task.identity.runner == crate::run::source::Runner::Package {
                            task.managers =
                                crate::run::source::managers(&root, &path, &task.managers);
                        }
                    }
                    tasks
                })
                .await;
            let _ = tab.update(cx, |tab, cx| {
                if tab.task_generation == generation && !tab.read_only {
                    tab.tasks = tasks;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The absolute path.
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.relative)
    }

    /// What the file turned out to be.
    pub fn state(&self) -> &FileState {
        &self.state
    }

    /// The language it was given, and `None` for a file no language
    /// claims — which is a real answer, not a missing one.
    pub fn language(&self) -> Option<&'static Language> {
        self.language
    }

    /// How its lines ended on disk.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn local_search(&self) -> Option<&Entity<crate::local_search::SearchBar>> {
        self.local_search.as_ref()
    }

    /// A loaded text snapshot may overlay disk even when it is empty. An
    /// initial/restored read with no snapshot must leave disk searchable.
    pub fn search_buffer(&self, cx: &gpui::App) -> Option<crate::project_search::Buffer> {
        let text = match &self.editor {
            Some(editor) => editor.read(cx).text().to_string(),
            None => self.pending.clone()?,
        };
        Some(crate::project_search::Buffer {
            clean: self.clean.clone(),
            text,
        })
    }
    /// Compare the current searchable text without cloning text or clean buffers.
    pub(crate) fn search_text_is(&self, expected: &str, cx: &gpui::App) -> bool {
        self.editor.as_ref().map_or_else(
            || self.pending.as_deref().unwrap_or_default() == expected,
            |editor| editor.read(cx).text() == expected,
        )
    }
    /// Apply a background-validated full snapshot; disk saving remains editor-owned.
    pub fn replace_snapshot(
        &mut self,
        snapshot: &crate::project_search::Snapshot,
        edits: Vec<crate::document_search::Match>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.read_only || self.relocating || self.changed_on_disk {
            return Err("File is read-only, moving, or changed on disk".into());
        }
        // Project replacement already validated these immutable disk bytes on
        // its worker. A newly opened tab can adopt them without a second read
        // or a synchronous wait; its pending open must not overwrite the edit.
        if !snapshot.buffer
            && self.relative == snapshot.path
            && self.editor.is_none()
            && self.pending.is_none()
            && self.loading.pending()
            && let Text::Source { text, line_ending } = Text::of(&snapshot.disk)
            && text == snapshot.text
        {
            self.loading.cancel();
            self.install_read(FileState::Text, Some((text, line_ending)), window, cx);
        }
        self.realise(window, cx);
        let Some(editor) = self.editor.clone() else {
            return Err("File has no editable buffer".into());
        };
        if !self.search_text_is(&snapshot.text, cx) {
            return Err("Buffer changed since this search; search again".into());
        }
        self.pending_write = None;
        if let Some(search) = &self.local_search {
            search.update(cx, |search, cx| {
                search.apply_document_edits(&edits, window, cx)
            });
        } else {
            editor.update(cx, |editor, cx| {
                crate::document_search::apply_matches(editor, &edits, window, cx)
            });
        }
        self.changed(&editor, cx);
        Ok(())
    }

    /// The editor, when there is text.
    pub fn editor(&self) -> Option<&Entity<EditorState>> {
        self.editor.as_ref()
    }

    pub fn soft_wrap(&self) -> bool {
        self.soft_wrap
    }

    /// Change only the current editor's visual layout.
    pub fn toggle_soft_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.soft_wrap = !self.soft_wrap;
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| {
                editor.set_soft_wrap(self.soft_wrap, window, cx);
            });
        }
        cx.notify();
    }

    /// Set the tab another way: the palette or the font changed.
    /// What the tab is drawn with.
    pub fn look(&self) -> &Look {
        &self.look
    }

    pub fn restyle(&mut self, look: Look, ui: Ui, cx: &mut Context<Self>) {
        self.look = look;
        self.ui = ui;
        if let Some(occurrences) = &self.occurrences {
            occurrences.update(cx, |occurrences, cx| {
                occurrences.restyle(self.look.theme, cx)
            });
        }
        if let Some(search) = &self.local_search {
            search.update(cx, |search, cx| search.restyle(self.look.theme, ui, cx));
        }
        if !self.look.autosave {
            self.pending_write = None;
        }
        cx.notify();
    }
}

impl Focusable for FileTab {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        if self.image.as_ref().is_some_and(|p| !p.source) {
            return self.focus.clone();
        }
        match &self.editor {
            Some(editor) => editor.read(cx).focus_handle(cx),
            None => self.focus.clone(),
        }
    }
}

impl Render for FileTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.realise(window, cx);
        self.ensure_git_markers(cx);
        let theme = self.look.theme;
        // The same rule as the terminal's: take the focus when nothing
        // has it, not whenever it is not ours.
        let handle = self.focus_handle(cx);
        if window.focused(cx).is_none() {
            window.focus(&handle, cx);
        }

        // One line above the editor when the disk moved under the buffer,
        // or when the last write failed: both are the tab's to say.
        let warning = self
            .changed_on_disk
            .then(|| self.ui.text(Label::FileChangedOnDisk).to_owned())
            .or_else(|| {
                self.write_failed
                    .as_ref()
                    .map(|why| format!("{}: {why}", self.ui.text(Label::FileNotSaved)))
            })
            .map(|text| {
                div()
                    .flex_none()
                    .px(theme::space::PANEL_PADDING)
                    .py(px(4.0))
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.accent.rust))
                    .bg(rgb(theme.accent.rust_bg))
                    .child(text)
            });

        let body: gpui::AnyElement = match (&self.editor, &self.state) {
            (Some(editor), _) => {
                let family = self.look.family.clone();
                let input = Editor::new(editor)
                    .readonly(self.read_only)
                    .h_full()
                    .appearance(false)
                    .font_family(family)
                    .text_size(self.look.text_size())
                    .line_height(self.look.line_height());
                if !jumps_to_definitions(self.read_only, self.language) {
                    // A dependency's file is told to no server, and a
                    // language with none has nobody to ask: there is no
                    // definition to go to from either.
                    input.into_any_element()
                } else {
                    // **Both of the component's routes to a definition
                    // are `f12`'s** — M5.11's decision 10. Its own jump
                    // moves the caret inside this buffer whatever file
                    // the definition is in; the provider holds every link
                    // where it was asked, so that jump moves nothing, and
                    // this box sends Guild's `GoToDefinition`, which opens
                    // the right tab and lands through `land_on`.
                    //
                    // The right button's item is the component's action,
                    // caught on the way down and not let reach the
                    // editor. A `cmd`-click is heard in the bubble phase,
                    // after the editor below has put the caret under the
                    // pointer — a child fires first. `dispatch_action`
                    // defers, so the root answers outside this tab's
                    // update: a provider called from the root's own
                    // update panics.
                    //
                    // `min_h_0`, because the box is a flex item under the
                    // warning line, and a flex item's least height is its
                    // content's: without it an editor taller than the tab
                    // would push the box past the bottom edge.
                    div()
                        .size_full()
                        .min_h_0()
                        .capture_action(|_: &gpui_component::input::GoToDefinition, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(crate::actions::GoToDefinition), cx);
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |event, window, cx| {
                            if event.modifiers.secondary() {
                                window
                                    .dispatch_action(Box::new(crate::actions::GoToDefinition), cx);
                            }
                        })
                        .child(input)
                        .into_any_element()
                }
            }
            (None, state) => {
                let (label, detail): (Label, Option<String>) = if self.loading.pending() {
                    (Label::LogLoading, None)
                } else {
                    match state {
                        FileState::NotText => (Label::FileNotText, None),
                        FileState::TooLarge => (Label::FileTooLarge, None),
                        FileState::Gone => (Label::FileGone, None),
                        FileState::CannotRead(why) => (Label::FileCannotRead, Some(why.clone())),
                        FileState::Text => (Label::FileCannotRead, None),
                    }
                };
                div()
                    .track_focus(&self.focus)
                    .p(theme::space::PANEL_PADDING)
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.faint))
                    .child(self.ui.text(label))
                    .when_some(detail, |this, detail| {
                        this.child(div().text_size(theme::size::META).child(detail))
                    })
                    .into_any_element()
            }
        };

        let body = self.image_body(cx).unwrap_or(body);
        let image_controls = self.image_controls(cx);
        let image_preview = self.image.as_ref().is_some_and(|p| !p.source);
        let body = if image_preview {
            body
        } else {
            self.task_gutter(body, window, cx)
        };
        let body = self.scroll.wrap(body);
        let launch = self.render_launch(window, cx);
        let git_hunk = self.render_git_hunk(window, cx);
        let drawn_at = self.drawn_at.clone();
        let gutter_width = self.gutter_width.clone();
        let editor = (!image_preview).then(|| self.editor.clone()).flatten();
        div()
            .key_context(if self.read_only {
                "guild-readonly-file"
            } else {
                "guild-file"
            })
            .capture_action(
                cx.listener(|tab, _: &gpui_component::input::SelectAll, _, cx| {
                    if let Some(selection) = &tab.smart_selection {
                        selection.update(cx, |selection, _| selection.reset());
                    }
                    cx.propagate();
                }),
            )
            .capture_action(
                cx.listener(|tab, _: &crate::editor::ExtendSelection, window, cx| {
                    tab.semantic_selection(true, window, cx);
                }),
            )
            .capture_action(
                cx.listener(|tab, _: &crate::editor::ShrinkSelection, window, cx| {
                    tab.semantic_selection(false, window, cx);
                }),
            )
            .when(!self.read_only, |this| {
                use crate::editor::editing::{self as e, Op};
                this.capture_action(cx.listener(|tab, _: &e::DeleteLine, window, cx| {
                    tab.edit(Op::DeleteLine, window, cx)
                }))
                .capture_action(cx.listener(|tab, _: &e::Duplicate, window, cx| {
                    tab.edit(Op::Duplicate, window, cx)
                }))
                .capture_action(cx.listener(|tab, _: &e::MoveLineUp, window, cx| {
                    tab.edit(Op::MoveLine { up: true }, window, cx)
                }))
                .capture_action(cx.listener(|tab, _: &e::MoveLineDown, window, cx| {
                    tab.edit(Op::MoveLine { up: false }, window, cx)
                }))
                .capture_action(cx.listener(|tab, _: &e::JoinLines, window, cx| {
                    tab.edit(Op::JoinLines, window, cx)
                }))
                .capture_action(
                    cx.listener(|tab, _: &e::CommentWithLineComment, window, cx| {
                        tab.edit(Op::LineComment, window, cx)
                    }),
                )
                .capture_action(
                    cx.listener(|tab, _: &e::CommentWithBlockComment, window, cx| {
                        tab.edit(Op::BlockComment, window, cx)
                    }),
                )
                .capture_action(cx.listener(|tab, _: &e::StartNewLine, window, cx| {
                    tab.edit(Op::StartNewLine { above: false }, window, cx)
                }))
                .capture_action(
                    cx.listener(|tab, _: &e::StartNewLineBeforeCurrent, window, cx| {
                        tab.edit(Op::StartNewLine { above: true }, window, cx)
                    }),
                )
                .capture_action(cx.listener(
                    |tab, _: &e::ToggleCase, window, cx| tab.edit(Op::ToggleCase, window, cx),
                ))
            })
            .capture_any_mouse_down(cx.listener(|tab, _, _, cx| {
                if let Some(selection) = &tab.smart_selection {
                    selection.update(cx, |selection, _| selection.reset());
                }
            }))
            .capture_key_down(cx.listener(|tab, event: &gpui::KeyDownEvent, _, cx| {
                let key = &event.keystroke;
                if matches!(
                    key.key.as_str(),
                    "left"
                        | "right"
                        | "up"
                        | "down"
                        | "home"
                        | "end"
                        | "pageup"
                        | "pagedown"
                        | "escape"
                ) && !(key.modifiers.platform
                    && !key.modifiers.alt
                    && !key.modifiers.control
                    && !key.modifiers.shift
                    && matches!(key.key.as_str(), "up" | "down"))
                    && let Some(selection) = &tab.smart_selection
                {
                    selection.update(cx, |selection, _| selection.reset());
                }
            }))
            .capture_action(
                cx.listener(|tab, _: &gpui_component::input::Undo, window, cx| {
                    tab.replay_selection(true, window, cx);
                    if tab
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
                        && let Some(search) = tab.local_search.clone()
                    {
                        search.update(cx, |search, cx| search.begin_history(cx));
                        cx.defer_in(window, move |_, _, cx| {
                            search.update(cx, |search, cx| search.finish_history(false, cx))
                        });
                    }
                    cx.propagate();
                }),
            )
            .capture_action(
                cx.listener(|tab, _: &gpui_component::input::Redo, window, cx| {
                    tab.replay_selection(false, window, cx);
                    if tab
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
                        && let Some(search) = tab.local_search.clone()
                    {
                        search.update(cx, |search, cx| search.begin_history(cx));
                        cx.defer_in(window, move |_, _, cx| {
                            search.update(cx, |search, cx| search.finish_history(true, cx))
                        });
                    }
                    cx.propagate();
                }),
            )
            .when(self.editor.is_some(), |this| {
                this.on_action(
                    cx.listener(|tab, _: &crate::actions::StageHunk, window, cx| {
                        tab.hunk_action_at_caret(crate::hunk::Action::Stage, window, cx);
                    }),
                )
                .on_action(cx.listener(
                    |tab, _: &crate::actions::UnstageHunk, window, cx| {
                        tab.hunk_action_at_caret(crate::hunk::Action::Unstage, window, cx);
                    },
                ))
            })
            .when(self.editor.is_some(), |this| {
                this.on_action(cx.listener(|tab, _: &crate::actions::Find, _window, cx| {
                    tab.reveal_image_source(_window, cx);
                    if let Some(editor) = &tab.editor {
                        let editor = editor.clone();
                        let search = tab
                            .local_search
                            .get_or_insert_with(|| {
                                cx.new(|cx| {
                                    crate::local_search::SearchBar::new(
                                        editor,
                                        tab.look.theme,
                                        tab.ui,
                                        tab.read_only,
                                        cx,
                                    )
                                })
                            })
                            .clone();
                        search.update(cx, |search, cx| search.show(false, _window, cx));
                        cx.stop_propagation();
                    }
                }))
            })
            .when(!self.read_only && self.editor.is_some(), |this| {
                this.on_action(
                    cx.listener(|tab, _: &crate::actions::Replace, _window, cx| {
                        tab.reveal_image_source(_window, cx);
                        if let Some(editor) = &tab.editor {
                            let editor = editor.clone();
                            let search = tab
                                .local_search
                                .get_or_insert_with(|| {
                                    cx.new(|cx| {
                                        crate::local_search::SearchBar::new(
                                            editor,
                                            tab.look.theme,
                                            tab.ui,
                                            tab.read_only,
                                            cx,
                                        )
                                    })
                                })
                                .clone();
                            search.update(cx, |search, cx| search.show(true, _window, cx));
                            cx.stop_propagation();
                        }
                    }),
                )
            })
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface.app))
            .children(
                (!image_preview)
                    .then(|| self.local_search.clone())
                    .flatten(),
            )
            .children(warning)
            .children(image_controls)
            .child(
                div().relative().flex_1().min_h_0().child(body).child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, _, cx| {
                            use gpui_component::RopeExt as _;
                            drawn_at.set((!image_preview).then_some(bounds));
                            let width = editor.as_ref().and_then(|editor| {
                                let state = editor.read(cx);
                                let first = state.visible_row_range()?.start;
                                let offset = state.text().line_start_offset(first);
                                let text = state.range_to_bounds(&(offset..offset))?;
                                // range_to_bounds includes the text's horizontal scroll.
                                Some(text.left() - state.scroll_offset().x - bounds.left())
                            });
                            gutter_width.set(width);
                        },
                    )
                    // Pinned to the corner: an absolute box with no inset sits
                    // where it would have flowed — after the body, at the bottom.
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                ),
            )
            .children(launch)
            .children(git_hunk)
    }
}

impl FileTab {
    /// Whether a window point is on the line numbers — the right button
    /// there is Guild's, for *Annotate with Git Blame*. `false` until
    /// the editor has been drawn.
    pub fn in_gutter(&self, at: gpui::Point<gpui::Pixels>) -> bool {
        if gutter::marker_contains(&self.gutter_snapshot, at) {
            return false;
        }
        let (Some(bounds), Some(width)) = (self.drawn_at.get(), self.gutter_width.get()) else {
            return false;
        };
        bounds.contains(&at)
            && crate::gutter::inside(f32::from(at.x - bounds.left()), 0.0, f32::from(width))
    }

    /// Where a range of the text is drawn in the window, when the editor
    /// has been drawn — the rectangle under the name a balloon points at.
    /// `range` is in bytes of the text.
    pub fn bounds_of(
        &self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &App,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.drawn_at.get()?;
        let state = self.editor.as_ref()?.read(cx);
        let text = state.text().to_string();
        let start = range.start.min(text.len());
        let end = range.end.clamp(start, text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return None;
        }
        state.range_to_bounds(&(start..end))
    }
}

/// Whether a tab sends Guild's Go to Definition from the component's two
/// routes to one — a `cmd`-click and the right button's item.
///
/// **Only a project's file whose language has a server.** A dependency's
/// file is told to no server, and a file with no server has nobody to ask:
/// sent from there, the jump answers *no server for markdown* on every
/// `cmd`-click, where a `cmd`-click used to do nothing. `f12` still says
/// it — a person who pressed the key for a definition is owed a reason.
fn jumps_to_definitions(read_only: bool, language: Option<&'static Language>) -> bool {
    !read_only && language.is_some_and(|language| !language.server.is_empty())
}

/// What the component writes for Tab, from what the language table says.
///
/// A hard tab is **drawn** four wide: `tab_size` is also what the
/// component counts a `\t` as when it measures a line's indent, and four
/// is what every terminal beside this tab shows one as.
fn tab_size_for(indent: Indent) -> TabSize {
    match indent {
        Indent::Spaces(width) => TabSize {
            tab_size: usize::from(width),
            hard_tabs: false,
        },
        Indent::Tabs => TabSize {
            tab_size: 4,
            hard_tabs: true,
        },
    }
}

/// The retained buffer changed, including reloads while its editor is hidden.
pub struct BufferChanged;
impl gpui::EventEmitter<BufferChanged> for FileTab {}

impl FileTab {
    pub(crate) fn presentation_style(&self) -> (Look, Ui) {
        (self.look.clone(), self.ui)
    }

    /// Clear geometry as soon as source is hidden, before another mouse event.
    pub(crate) fn hide_source(&self) {
        self.git_markers
            .presentation
            .set(self.git_markers.presentation.get().wrapping_add(1));
        self.scroll.reset();
        self.drawn_at.set(None);
        self.gutter_width.set(None);
    }

    pub(crate) fn presentation_blur(&mut self, window: &Window, cx: &App) {
        self.suppress_presentation_blur = self
            .editor
            .as_ref()
            .is_some_and(|e| e.read(cx).focus_handle(cx).contains_focused(window, cx));
        self.hide_source();
    }

    pub(crate) fn warning_text(&self) -> Option<String> {
        self.changed_on_disk
            .then(|| self.ui.text(Label::FileChangedOnDisk).to_owned())
            .or_else(|| {
                self.write_failed
                    .as_ref()
                    .map(|why| format!("{}: {why}", self.ui.text(Label::FileNotSaved)))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab is a hard tab, drawn four wide; spaces are a width and write
    /// that many. The component's default is two spaces for everything,
    /// which is what Tab wrote into a Makefile before M5.11.
    #[gpui::test]
    fn restore_defers_disk_reads(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello\r\n").unwrap();
        let file = cx.update(|cx| {
            cx.new(|cx| {
                FileTab::restore(
                    dir.path(),
                    Path::new("file.txt"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: theme::Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    cx,
                )
            })
        });
        cx.update(|cx| {
            assert!(
                file.read(cx).pending.is_none(),
                "restore must return before disk IO"
            )
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(file.read(cx).pending.as_deref(), Some("hello\n"));
            assert_eq!(file.read(cx).line_ending, LineEnding::Crlf);
        });
    }

    #[test]
    fn file_reads_follow_only_regular_targets_and_bound_actual_bytes() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("text"), "hello").unwrap();
        symlink("text", dir.path().join("link")).unwrap();
        assert_eq!(read(&dir.path().join("link")).1.unwrap().0, "hello");
        symlink("/dev/zero", dir.path().join("device")).unwrap();
        for path in [dir.path().to_owned(), dir.path().join("device")] {
            assert!(matches!(read(&path), (FileState::CannotRead(_), None)));
        }
        // Simulates growth after the handle metadata check, with no large fixture.
        assert_eq!(
            read_limited(std::io::repeat(b'x')),
            Err(FileState::TooLarge)
        );
    }

    #[test]
    fn special_file_read_never_waits_for_a_writer() {
        use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pipe");
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (tx, rx) = std::sync::mpsc::channel();
        let input = path.clone();
        let worker = std::thread::spawn(move || tx.send(read(&input)).unwrap());
        let result = rx.recv_timeout(std::time::Duration::from_millis(500));
        // Release an old blocking reader before failing; never leave a test thread behind.
        if result.is_err() {
            let writer = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap();
            drop(writer);
        }
        worker.join().unwrap();
        assert!(
            matches!(result, Ok((FileState::CannotRead(_), None))),
            "FIFO read waited or accepted a special file: {result:?}"
        );
    }

    #[test]
    fn a_tab_is_a_hard_tab_and_spaces_are_a_width() {
        let tabs = tab_size_for(Indent::Tabs);
        assert!(tabs.hard_tabs);
        assert_eq!(tabs.tab_size, 4, "a hard tab is drawn four wide");

        for width in [2, 4] {
            let spaces = tab_size_for(Indent::Spaces(width));
            assert!(!spaces.hard_tabs, "{width} spaces are spaces");
            assert_eq!(spaces.tab_size, usize::from(width));
        }
    }

    /// **A `cmd`-click is a jump only where a server can answer one.**
    /// Before this, a `cmd`-click in a Markdown file sent Guild's Go to
    /// Definition, which told the person *no server for markdown* on every
    /// press — where a `cmd`-click used to do nothing. A dependency's file
    /// is told to no server, so it has no jump either.
    #[test]
    fn only_a_projects_file_with_a_server_hears_a_jump() {
        let rust = Language::of(Path::new("src/lib.rs"));
        let markdown = Language::of(Path::new("README.md"));
        assert!(
            rust.is_some_and(|l| !l.server.is_empty()),
            "rust is the language with a server this test rests on"
        );
        assert!(
            markdown.is_some_and(|l| l.server.is_empty()),
            "markdown is the language without one"
        );

        assert!(jumps_to_definitions(false, rust));
        assert!(!jumps_to_definitions(false, markdown), "no server");
        assert!(!jumps_to_definitions(false, None), "no language");
        assert!(!jumps_to_definitions(true, rust), "a dependency's source");
    }

    /// A folder of the test's own, gone when the test lets go of it.
    fn temp(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("guild-file-{name}-"))
            .tempdir()
            .expect("a scratch folder")
    }

    /// The four ways a read can fail are four states, not four errors.
    #[test]
    fn a_read_names_what_it_found() {
        let folder = temp("states");
        let dir = folder.path();
        std::fs::write(dir.join("a.rs"), "fn main() {}\r\n").unwrap();
        std::fs::write(dir.join("b.png"), b"\x89PNG\x00\x00").unwrap();

        let (state, text) = read(&dir.join("a.rs"));
        assert_eq!(state, FileState::Text);
        assert_eq!(text, Some(("fn main() {}\n".to_owned(), LineEnding::Crlf)));

        assert_eq!(read(&dir.join("b.png")).0, FileState::NotText);
        assert_eq!(read(&dir.join("nowhere.rs")).0, FileState::Gone);
    }

    /// A save writes what was typed, the way the file's lines ended, and
    /// leaves nothing beside the file; a record of it says the change the
    /// watcher will report is ours.
    #[test]
    fn a_save_writes_beside_and_renames_keeping_the_line_ending() {
        let folder = temp("save");
        let dir = folder.path();
        let path = dir.join("a.txt");
        std::fs::write(&path, "one\r\ntwo\r\n").unwrap();

        let record = save_atomically(&path, "one\nthree\n", LineEnding::Crlf).expect("saved");
        assert_eq!(std::fs::read(&path).unwrap(), b"one\r\nthree\r\n");
        assert!(record.matches(&path), "our own write is recognised");
        let leftovers: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "nothing beside the file: {leftovers:?}");

        std::fs::write(&path, "someone else\n").unwrap();
        assert!(
            !record.matches(&path),
            "a change that is not ours is seen as one"
        );
    }

    /// A file that ended without a newline is written back without one:
    /// a save is not an edit.
    #[test]
    fn a_save_adds_no_trailing_newline() {
        let folder = temp("newline");
        let dir = folder.path();
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();

        save_atomically(&path, "one two", LineEnding::Lf).expect("saved");
        assert_eq!(std::fs::read(&path).unwrap(), b"one two");
    }

    /// The old file's permissions survive the rename.
    #[cfg(unix)]
    #[test]
    fn a_save_keeps_the_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let folder = temp("perms");
        let dir = folder.path();
        let path = dir.join("run.sh");
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        save_atomically(&path, "#!/bin/sh\necho hi\n", LineEnding::Lf).expect("saved");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    /// The dot says "unsaved"; its colour says whether the disk moved
    /// meanwhile. A clean tab shows nothing: a clean tab is reloaded
    /// rather than marked.
    #[test]
    fn the_mark_is_for_a_dirty_tab_only() {
        assert_eq!(mark(false, false), None);
        assert_eq!(mark(false, true), None);
        assert_eq!(mark(true, false), Some(Mark::Unsaved));
        assert_eq!(mark(true, true), Some(Mark::Conflict));
    }
    // Inject directory-open failure without relying on chmod, root privileges
    // or the host filesystem's permission policy. The hook stays private.
    #[test]
    fn directory_open_failure_does_not_publish_an_editor_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, "original").unwrap();
        let result = save_atomically_with_directory(&path, "replacement", LineEnding::Lf, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture directory refusal",
            ))
        });
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[cfg(test)]
mod occurrence_tests;
#[cfg(test)]
mod scroll_tests;

/// A completed atomic write, distinct from becoming clean by undo or reload.
#[derive(Clone)]
pub struct Saved {
    pub path: PathBuf,
    pub text: gpui_component::input::Rope,
}
impl gpui::EventEmitter<Saved> for FileTab {}
