//! A terminal tab: the grid on screen, the keyboard, the size.

#[cfg(feature = "adaptive-terminal-refresh")]
mod adaptive;
mod commands;
mod drawing;
mod find_bar;
mod hover;
mod input;
mod platform_input;
mod pointer;
mod prepared_rows;
mod restore;
mod scheduling;
#[cfg(test)]
mod scheduling_tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

use alacritty_terminal::index::Point;
use alacritty_terminal::index::Side;
use gpui::{
    Bounds, Context, FocusHandle, Focusable, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollWheelEvent, Window, canvas, div,
    prelude::*, px,
};
use guild_session::{Conversation, HarnessKind};
use portable_pty::CommandBuilder;

use alacritty_terminal::event::WindowSize;

use super::emulator::{Emulator, GridSize, TerminalError, TerminalEvent};
use super::palette::Palette;
use super::reply::{Formatter, Reply};
use super::scroll::Scroll;
use crate::blink::Blink;
use crate::theme;

/// Everything the cursor needs, read in one go from the grid's lock.
///
/// Gathered into a struct rather than fetched field by field on purpose:
/// each fetch from the emulator takes the grid's mutex, and painting
/// already holds it. A std mutex is not reentrant, so the second one
/// deadlocks — the whole window, on its very first frame.
#[derive(Clone, Copy)]
struct Cursor {
    point: Point,
    columns: usize,
    /// How far the view is scrolled into the history, and how many rows it
    /// shows: the cursor's row on the view is its line plus the first.
    offset: usize,
    screen_lines: usize,
    style: alacritty_terminal::vte::ansi::CursorStyle,
    visible: bool,
}

/// A coalesced admission refusal, separate from managed submission errors.
pub(crate) struct InputRefused(pub &'static str);
impl gpui::EventEmitter<InputRefused> for TerminalTab {}

/// A terminal tab.
pub struct TerminalTab {
    composition: platform_input::Composition,
    command_point: Option<Point>,
    pending_meta: Option<gpui::Keystroke>,
    dispatch_meta: bool,
    discarding_composition: bool,
    composition_blur: Option<gpui::Subscription>,
    #[cfg(feature = "adaptive-terminal-refresh")]
    adaptive: adaptive::Adaptive,
    watch_task: Option<gpui::Task<()>>,
    watch_deadline: Option<std::time::Instant>,
    redraw: scheduling::Redraw,
    #[cfg(test)]
    wake_count: usize,
    #[cfg(test)]
    reader_exit_count: usize,
    /// How many times the grid was painted, not replayed from a cached frame.
    #[cfg(test)]
    pub(crate) paints: usize,
    /// Whether the last real paint registered the platform's text client.
    #[cfg(test)]
    pub(crate) text_client: bool,
    prepared_rows: prepared_rows::Rows,
    painted: bool,
    painted_at: Option<std::time::Instant>,
    emulator: Emulator,
    input_refusal_subscription: Option<gpui::Subscription>,
    pending_input_refusal: Option<&'static str>,
    /// The window's start size, which every fit of a body tab moves.
    start: Option<StartSize>,
    palette: Palette,
    focus: FocusHandle,
    /// The cell size in pixels; computed from the font on the first
    /// paint.
    cell: Option<gpui::Size<Pixels>>,
    /// The cell in device pixels, `(width, height)` — what a program asking
    /// for the text area in pixels is told, written beside `cell` by the
    /// paint. M5.12 task 6.
    cell_px: Option<(u16, u16)>,
    /// Pixel sizes asked before the first paint, answered by the first tick
    /// after it — decision 8 of M5.12: until then there is no cell, and the
    /// pty is still the size it was spawned at.
    sizes_waiting: Vec<Formatter<WindowSize>>,
    /// The font chosen from what is installed; determined on the first
    /// paint, when the text system is available.
    font: Option<gpui::SharedString>,
    /// The bounds of the painted area: a mouse press is converted into a
    /// grid cell by them.
    bounds: Option<Bounds<Pixels>>,
    /// What the held button is doing: our selection, or a press the
    /// program is being told about — 2026-09-24.
    grab: crate::terminal::select::Grab,
    /// The cell the program was last told the pointer is on, so a move
    /// inside one cell is not reported twice.
    reported_at: Option<(usize, usize)>,
    /// A size the view has become and the program not yet been told —
    /// M5.14, `terminal::resize`.
    pending_size: Option<crate::terminal::resize::Pending>,
    /// Whether the tab has been fitted once: the first size is told at
    /// once, so a restored tab starts at its real size — the StartSize
    /// trap.
    fitted: bool,
    /// Where the pointer was last seen while a selection was dragged — what
    /// the clock past the edge aims at. M12.1.
    drag_at: Option<gpui::Point<Pixels>>,
    /// Whether that clock is running.
    edge_ticking: bool,
    /// Pixels of scroll that have not yet added up to a row. A trackpad
    /// sends a few per event, and throwing them away meant it did not
    /// scroll at all.
    scroll: Scroll,
    /// The cursor's blink: its phase, which typing restarts so the caret
    /// is solid under the hand, and whether the next phase needs a
    /// repaint — only when the last paint drew it blinking, which an
    /// unfocused tab or an inactive window never does. `paint_cursor`
    /// says what it drew.
    caret: Blink,
    /// When the history was last moved, for showing the scrollbar.
    ///
    /// The bar is shown while scrolling and fades afterwards, the way an
    /// overlay scrollbar does everywhere else: a permanent one takes a
    /// column of the terminal away for something that is only interesting
    /// while the wheel is turning.
    scrolled_at: Option<std::time::Instant>,
    /// Copy the selection immediately, with no separate action.
    copy_on_select: bool,
    /// The font size: changed on the fly with a shortcut.
    font_size: f32,
    /// The palette in force, kept in step by the window.
    theme: crate::theme::Theme,
    /// The window's language, kept in step by the window like the palette.
    ui: crate::i18n::Ui,
    /// The family a person asked for, if they asked for one.
    ///
    /// Separate from `font`, which caches whatever was resolved: this is
    /// the question and that is the answer, and a name that is not
    /// installed makes them differ.
    chosen_font: Option<String>,
    /// The title, whether the process exited — the tab is dead, but its
    /// content is still visible — and the agent in front: what the window
    /// reads without reading this entity. `terminal::facts` says why.
    facts: std::rc::Rc<crate::terminal::facts::Facts>,
    launched_harness: Option<HarnessKind>,
    /// Esc or Ctrl-C was typed since the window last asked — M5.10.
    ///
    /// An interrupt fires no hook at all, so this keystroke is the only
    /// witness that the person stopped a working or waiting agent.
    interrupted: Option<std::time::Instant>,
    /// Input in this process, distinct from persisted conversation history.
    prompt_input: bool,
    draft_input: bool,
    awaiting_prompt: bool,
    uncertain_input: bool,
    runtime_identity: Option<String>,
    input_epoch: u64,
    programmatic_epoch: Option<u64>,
    managed_codex_turn: Option<String>,
    managed_codex_completed: Option<String>,
    managed_codex_started: bool,
    /// The foreground process last asked about, so the command line is read
    /// once per process rather than once per probe.
    foreground: Option<i32>,
    /// When the foreground was last asked for.
    foreground_at: Option<std::time::Instant>,
    /// The find bar, open or not — M5.12, task 9.
    find: find_bar::Finding,
    /// The link under the pointer while `cmd` is held — M5.12, task 12.
    hover: hover::Hover,
    /// The directory the terminal started in: a relative path in its
    /// output is resolved against it first — M5.12's decision 12.
    cwd: PathBuf,
    codex_probe: Option<guild_session::codex_transcript::Probe>,
    pub(crate) discovery_diagnostics: Vec<String>,
    /// Saved before hangup, when the program still owns its last screen.
    screen_cache: Option<restore::Cache>,
    /// Paint-only startup content; never fed into the live terminal.
    restoring: Option<std::rc::Rc<restore::Preview>>,
}

impl TerminalTab {
    /// Synthetic fixture observation only; never reads/writes the clipboard.
    #[cfg(feature = "native-preview")]
    pub(crate) fn probe_interaction_state(&self) -> (usize, bool) {
        let term = self.emulator.term().lock().unwrap();
        (term.grid().display_offset(), term.selection.is_some())
    }

    /// A tab around an already running process.
    ///
    /// The process is started **before** the GPUI entity is created — see
    /// [`open`]. Otherwise a failed launch would have to be returned from
    /// inside `cx.new`, which must return a value, and we would get a live
    /// tab around a dead process.
    fn new(
        emulator: Emulator,
        look: Look,
        cwd: PathBuf,
        start: Option<StartSize>,
        cx: &mut Context<Self>,
    ) -> Self {
        #[cfg(feature = "adaptive-terminal-refresh")]
        emulator.set_adaptive_enabled(look.adaptive_terminal_refresh && cfg!(target_os = "macos"));
        let mut tab = Self {
            #[cfg(feature = "adaptive-terminal-refresh")]
            adaptive: adaptive::Adaptive::default(),
            watch_task: None,
            watch_deadline: None,
            redraw: Default::default(),
            #[cfg(test)]
            wake_count: 0,
            #[cfg(test)]
            reader_exit_count: 0,
            #[cfg(test)]
            paints: 0,
            #[cfg(test)]
            text_client: false,
            composition: Default::default(),
            command_point: None,
            pending_meta: None,
            dispatch_meta: false,
            discarding_composition: false,
            composition_blur: None,
            prepared_rows: Default::default(),
            painted: false,
            painted_at: None,
            emulator,
            input_refusal_subscription: None,
            pending_input_refusal: None,
            start,
            copy_on_select: look.copy_on_select,
            font_size: look.font_size,
            theme: look.theme,
            ui: look.ui,
            palette: Palette::new(look.theme),
            chosen_font: look.font,
            focus: cx.focus_handle(),
            cell: None,
            cell_px: None,
            sizes_waiting: Vec::new(),
            bounds: None,
            grab: crate::terminal::select::Grab::None,
            reported_at: None,
            pending_size: None,
            fitted: false,
            drag_at: None,
            edge_ticking: false,
            scroll: Scroll::default(),
            caret: Blink::new(std::time::Instant::now()),
            scrolled_at: None,
            font: None,
            facts: crate::terminal::facts::Facts::new(),
            launched_harness: None,
            interrupted: None,
            prompt_input: false,
            draft_input: false,
            awaiting_prompt: false,
            uncertain_input: false,
            runtime_identity: None,
            input_epoch: 0,
            programmatic_epoch: None,
            managed_codex_turn: None,
            managed_codex_completed: None,
            managed_codex_started: false,
            foreground: None,
            foreground_at: None,
            find: find_bar::Finding::default(),
            hover: hover::Hover::default(),
            cwd,
            codex_probe: None,
            discovery_diagnostics: Vec::new(),
            screen_cache: None,
            restoring: None,
        };
        tab.watch(cx);
        tab
    }

    /// Explicit rollout events for this terminal process, as a fallback to hooks.
    /// What the program reported of itself since the window last asked:
    /// its root OSC 7501 record's changes, in order, and how many a full
    /// queue dropped.
    pub(crate) fn take_status_changes(
        &self,
    ) -> (Vec<crate::terminal::program_status::RootChange>, u64) {
        self.emulator.take_status_changes()
    }

    /// The program's root OSC 7501 record as it stands now.
    pub(crate) fn status_root(&self) -> Option<crate::terminal::program_status::Report> {
        self.emulator.status_root()
    }

    pub(crate) fn codex_signals(
        &mut self,
        conversation: &str,
        human_wait: bool,
    ) -> Vec<guild_session::codex_transcript::Evidence> {
        if let Some(cache) = &mut self.screen_cache {
            cache.conversation = Some(conversation.to_owned());
        }
        // Legacy command terminals have no runtime provenance and cannot enter
        // managed live recovery. Keep their ordinary status policy unchanged.
        let protect_wait = human_wait && self.runtime_identity.is_some();
        let signals = self.codex_probe.as_mut().map_or_else(Vec::new, |probe| {
            probe.poll_runtime_evidence(conversation, protect_wait)
        });
        let starts = self
            .codex_probe
            .as_mut()
            .map_or_else(Vec::new, |probe| probe.take_runtime_starts());
        self.managed_codex_started |= starts
            .iter()
            .any(|turn| Some(turn.as_str()) == self.managed_codex_turn.as_deref());
        self.managed_codex_completed = self
            .codex_probe
            .as_ref()
            .and_then(|probe| probe.runtime_completion())
            .filter(|turn| Some(*turn) == self.managed_codex_turn.as_deref())
            .map(str::to_owned);
        signals
    }

    /// Reconcile a source hook with this process's exact rollout cursor.
    pub(crate) fn codex_hook_signal(
        &mut self,
        payload: &serde_json::Value,
        signal: guild_session::status::Signal,
    ) -> Option<guild_session::status::Signal> {
        let signal = self
            .codex_probe
            .as_mut()
            .map_or(Some(signal), |probe| probe.hook_signal(payload, signal));
        if matches!(
            payload["hook_event_name"].as_str(),
            Some(
                "UserPromptSubmit"
                    | "PreToolUse"
                    | "PermissionRequest"
                    | "PostToolUse"
                    | "PostToolUseFailure"
                    | "Stop"
            )
        ) && self
            .runtime_identity
            .as_deref()
            .is_some_and(|runtime| Some(runtime) == payload["guild_runtime"].as_str())
            && let Some(turn) = payload["turn_id"].as_str().filter(|s| !s.is_empty())
        {
            if let Some(probe) = &mut self.codex_probe {
                probe.pin_runtime_turn(turn);
            }
            if self.programmatic_epoch == Some(self.input_epoch)
                && self
                    .codex_probe
                    .as_ref()
                    .is_some_and(|probe| probe.owns_runtime_turn(turn))
            {
                self.managed_codex_turn = Some(turn.to_owned());
                self.managed_codex_completed = self
                    .codex_probe
                    .as_ref()
                    .and_then(|probe| probe.runtime_completion())
                    .filter(|completed| *completed == turn)
                    .map(str::to_owned);
            }
        }
        signal
    }

    pub(crate) fn take_managed_codex_start(&mut self) -> bool {
        std::mem::take(&mut self.managed_codex_started)
    }
    #[cfg(test)]
    pub(crate) fn attach_test_codex_probe(&mut self, home: std::path::PathBuf) {
        self.codex_probe = Some(guild_session::codex_transcript::Probe::new(home, None));
    }

    /// The plan's limits this tab's Codex recorded since the last look.
    pub(crate) fn take_codex_rate_limits(
        &mut self,
    ) -> Option<guild_session::codex_transcript::RateLimits> {
        self.codex_probe
            .as_mut()
            .and_then(guild_session::codex_transcript::Probe::take_rate_limits)
    }

    /// What the window reads about this tab without reading the tab —
    /// its title, the agent in front and whether it exited.
    ///
    /// `claude` writes the conversation's topic into the title and a shell
    /// writes where it is. The tab strip reads it — see `title::of` for
    /// what is kept of it.
    pub fn facts(&self) -> &std::rc::Rc<crate::terminal::facts::Facts> {
        &self.facts
    }

    /// Whether the process has ended while the tab is open — M5.12, for
    /// the close's question and `guild agent wait`.
    pub fn exited(&self) -> bool {
        self.facts.exited()
    }

    /// The pid of the program the terminal started, while it has one.
    pub fn process_id(&self) -> Option<u32> {
        self.emulator.process_id()
    }

    /// The directory the terminal started in — M5.12, what a relative path
    /// in its output is resolved against first.
    pub(crate) fn hold_shell_material(&mut self, material: Option<tempfile::TempDir>) {
        self.emulator.hold_shell_material(material);
    }

    pub(crate) fn live_shell_cwd(&self) -> Option<PathBuf> {
        self.emulator.live_cwd()
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Whether Claude Code is what this terminal runs in front.
    pub fn claude_in_front(&self) -> bool {
        self.facts.claude_in_front()
    }

    /// Which agent CLI this terminal runs in front, if any — M14.
    pub(crate) fn launched_harness(&self) -> Option<HarnessKind> {
        self.launched_harness
    }

    pub fn harness_in_front(&self) -> Option<HarnessKind> {
        self.facts.harness_in_front()
    }

    /// Ask what the terminal runs in front, when it is time to — M12.3 —
    /// and answer whether that changed which agent it is.
    ///
    /// **The foreground process group is how a terminal knows what it is
    /// running**, and the pty's master gives it. Its command line is read
    /// only when that process is a new one, by `ps`, which is the one
    /// program every Mac has for it; a shell sitting at its prompt is read
    /// once and then left alone.
    fn probe_foreground(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self
            .foreground_at
            .is_some_and(|at| now.duration_since(at) < crate::PROBE)
        {
            return false;
        }
        self.foreground_at = Some(now);
        let pid = self.emulator.foreground_process();
        if pid == self.foreground {
            return false;
        }
        self.foreground = pid;
        let args = pid.and_then(super::emulator::command_line);
        let harness = args.as_deref().and_then(crate::agents::which_in_front);
        // And which conversation it resumes, when it names one — M13.3: a
        // resume of a known session typed here hands the tab to it.
        let resumed = args.as_deref().zip(harness).and_then(|(args, kind)| {
            kind.impl_()
                .resumed_in(args)
                .map(|conversation| (kind, conversation))
        });
        let resumed_changed = self.facts.set_resumed(resumed);
        self.facts.set_harness(harness) || resumed_changed
    }

    /// Tag the terminal with the session it runs: `terminal::retiring`.
    pub fn set_retire_tag(&mut self, tag: String) {
        self.emulator.set_retire_tag(tag);
    }

    /// Hang the program up and say when it has gone — M13.3, the
    /// hand-over. The tab is left with nothing running.
    pub fn let_go(&mut self) -> Option<std::sync::mpsc::Receiver<()>> {
        // Intentional handoff transfers the child to the reaper; the later
        // EOF has no child status to publish and must not become a failure.
        self.launched_harness = None;
        self.facts.set_exited();
        self.save_screen();
        self.emulator.let_go()
    }

    /// Local time of the latest Esc or Ctrl-C since the last poll, consumed
    /// once and ordered with received hooks so it cannot cancel a newer prompt.
    pub fn take_interrupt(&mut self) -> Option<std::time::Instant> {
        std::mem::take(&mut self.interrupted)
    }

    /// Every actual input invalidates a previously captured managed readiness.
    pub(crate) fn runtime_identity(&self) -> Option<&str> {
        self.runtime_identity.as_deref()
    }
    pub(crate) fn managed_foreground(&self) -> Option<u32> {
        self.emulator
            .foreground_process()
            .and_then(|pid| u32::try_from(pid).ok())
    }
    pub(crate) fn managed_completion(&self) -> Option<&str> {
        self.managed_codex_completed.as_deref()
    }
    pub(crate) fn input_epoch(&self) -> u64 {
        self.input_epoch
    }
    fn raw_input(&mut self) {
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.programmatic_epoch = None;
        self.managed_codex_completed = None;
    }
    fn programmatic_input(&mut self) {
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.programmatic_epoch = Some(self.input_epoch);
        self.managed_codex_turn = None;
        self.managed_codex_completed = None;
        self.managed_codex_started = false;
    }
    /// Stop hooks never grant this witness; only exact fresh rollout completion.
    pub(crate) fn managed_codex_ready(&self) -> bool {
        !self.emulator.input_delivery_failed()
            && self.programmatic_epoch == Some(self.input_epoch)
            && self.managed_codex_completed.is_some()
            && self.managed_codex_completed == self.managed_codex_turn
            && !self.has_draft()
            && !self.input_pending()
            && self.accepts_submission()
    }
    pub(crate) fn recovery_leader(&self) -> Option<u32> {
        self.emulator.recovery_leader()
    }
    pub(crate) fn release_quiescent(&mut self, leader: u32) -> Result<(), String> {
        self.emulator.release_quiescent(leader)
    }

    /// Whether this process has received no prompt input, even when resuming history.
    pub(crate) fn initial_prompt(&self) -> bool {
        !self.prompt_input && !self.awaiting_prompt
    }

    /// Observed unsent input, distinct from a submitted prompt awaiting its event.
    pub(crate) fn has_draft(&self) -> bool {
        self.draft_input
    }
    /// Workflow-owned guard for unobserved submissions and unknown input effects.
    pub(crate) fn input_pending(&self) -> bool {
        self.awaiting_prompt || self.uncertain_input || self.emulator.input_delivery_failed()
    }
    pub(crate) fn prompt_submitted(&mut self) {
        self.awaiting_prompt = false;
    }

    /// Whether the live terminal can transport a bracketed prompt submission.
    pub(crate) fn accepts_submission(&self) -> bool {
        !self.exited()
            && !self.emulator.input_delivery_failed()
            && self
                .emulator
                .mode()
                .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
    }

    /// Type a message into the program's prompt and send it — M13 task 9,
    /// `guild agent send`.
    ///
    /// **Refused by answering `false` when the program is not in bracketed
    /// mode**: there a newline in the message would be a Return, and `claude`
    /// turns the mode on only once its prompt is up — so a terminal without
    /// it is not reading a prompt, whatever else says so.
    pub(crate) fn submission_refusal(&self) -> &'static str {
        if self.emulator.input_delivery_failed() {
            "Terminal input delivery failed; previously queued input may have reached the child"
        } else {
            self.emulator
                .input_refusal()
                .unwrap_or("Terminal is not reading a prompt")
        }
    }
    pub fn submit(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        self.submit_baselined(text, None, cx)
    }
    pub(crate) fn submit_baselined(
        &mut self,
        text: &str,
        baseline: Option<(&str, bool)>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.accepts_submission() {
            return false;
        }
        let Some(bytes) =
            crate::terminal::keys::submitted_bounded(text, super::emulator::INPUT_BYTES)
        else {
            return self.emulator.refuse_oversized_input();
        };
        let probe = &mut self.codex_probe;
        if !self.emulator.send_before(&bytes, || {
            if let (Some(probe), Some((conversation, managed))) = (probe, baseline) {
                if managed {
                    probe.baseline_managed(conversation);
                } else {
                    probe.baseline(conversation);
                }
            }
        }) {
            return false;
        }
        self.restoring = None;
        self.emulator.scroll_to_bottom();
        self.programmatic_input();
        self.prompt_input = true;
        self.awaiting_prompt = true;
        cx.notify();
        true
    }

    /// Collect the emulator's events for as long as the tab is alive.
    fn watch(&mut self, cx: &mut Context<Self>) {
        let signal = self.emulator.event_signal();
        self.watch_task = Some(cx.spawn(async move |tab, cx| {
            loop {
                let updated_at = std::time::Instant::now();
                let alive = tab.update(cx, |tab, cx| {
                    tab.refresh_find(cx);
                    // A synchronized frame never ended is let go here, on
                    // the tick — a repaint when one was.
                    #[cfg(feature = "native-preview")]
                    crate::native_preview::resource_probe::POLLS
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    #[cfg(test)]
                    {
                        tab.wake_count += 1;
                    }
                    let mut dirty = tab.emulator.flush_stale_sync();
                    // Retained screens do not retain completed job PTY resources.
                    let completed_job = tab.emulator.job_completion().is_some();
                    // Whether a fact the window reads changed — the one
                    // thing this tab tells the window about.
                    // `terminal::facts`.
                    // The supervisor can publish after the reader exit event. Its
                    // result (including cleanup failure) is final independently.
                    let mut heard = completed_job && tab.facts.set_exited();
                    dirty |= heard;
                    // A size the program has not been told, once the hand
                    // has paused or LONGEST has passed.
                    if let Some(pending) = tab.pending_size
                        && crate::terminal::resize::due(&pending, std::time::Instant::now())
                    {
                        tab.pending_size = None;
                        tab.emulator.resize(pending.size);
                        dirty = true;
                    }
                    let batch_started = std::time::Instant::now();
                    for _ in 0..128 {
                        let Some(event) = tab.emulator.next_event() else {
                            break;
                        };
                        match event {
                            TerminalEvent::Wakeup => dirty = true,
                            TerminalEvent::Title(title) => {
                                heard |= tab.facts.set_title(title);
                                dirty = true;
                            }
                            TerminalEvent::Exited => {
                                // No more reader bytes can close this frame. Publish
                                // it before the one-time failure classification.
                                tab.emulator.flush_completed_sync();
                                #[cfg(test)]
                                {
                                    tab.reader_exit_count += 1;
                                }
                                if let Some(status) = tab.emulator.exit_status() {
                                    heard |= tab.facts.set_exited();
                                    let tail = if tab.launched_harness.is_some()
                                        && status.exit_code() != 0
                                    {
                                        tab.emulator.failure_tail()
                                    } else {
                                        String::new()
                                    };
                                    heard |=
                                        tab.facts.set_failure(super::failure::Failure::derive(
                                            tab.launched_harness,
                                            Some(status.exit_code()),
                                            &tail,
                                        ));
                                }
                                dirty = true;
                            }
                            // OSC 52: a program copying, which is what a
                            // person asked of it. Pasting stays refused —
                            // decision 8, in `Emulator::spawn`.
                            TerminalEvent::ClipboardStore(text) => {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                            }
                            TerminalEvent::Reply(reply) => tab.reply(reply, cx),
                        }
                        if batch_started.elapsed() >= Duration::from_millis(2) {
                            break;
                        }
                    }
                    // A size asked before the first paint is answered by
                    // the first tick after it.
                    if tab.cell_px.is_some() && !tab.sizes_waiting.is_empty() {
                        for formatter in std::mem::take(&mut tab.sizes_waiting) {
                            tab.reply(Reply::Size(formatter), cx);
                        }
                    }

                    // The caret flips twice a second and nothing else
                    // happens meanwhile, so the repaint is asked for here
                    // rather than every frame. Its deadline is armed by paint, and
                    // the phase says exactly when the picture changed —
                    // when the last paint drew a caret that blinks at all.
                    if tab.facts.exited() {
                        tab.caret.seen(false, std::time::Instant::now());
                    }
                    if tab.caret.due(std::time::Instant::now()) {
                        dirty = true;
                    }

                    // The scrollbar fades, and a fade is only a fade if
                    // something redraws it on the way down.
                    if tab.painted
                        && scheduling::scrollbar_deadline(tab.scrolled_at, tab.painted_at)
                            .is_some_and(|deadline| deadline <= std::time::Instant::now())
                    {
                        dirty = true;
                    }

                    // What the terminal runs in front, on the tab's own
                    // clock — `probe_foreground` asks at most every PROBE.
                    // It was asked from the window's render, which is a
                    // read of every terminal on every frame.
                    if !tab.facts.exited() {
                        heard |= tab.probe_foreground();
                    }

                    if let Some(preview) = &tab.restoring {
                        let now = std::time::Instant::now();
                        if (dirty || preview.expired(now))
                            && tab
                                .emulator
                                .term()
                                .lock()
                                .is_ok_and(|term| preview.ready(&term, now, tab.facts.exited()))
                        {
                            tab.restoring = None;
                            dirty = true;
                        }
                    }
                    let now = std::time::Instant::now();
                    if dirty && tab.painted {
                        tab.redraw.request(now);
                    }
                    if tab.redraw.take(now) && std::mem::take(&mut tab.painted) {
                        cx.notify();
                    }
                    // **The window by id, and only for a fact** — output
                    // is this view's own business, and a notify of the
                    // window per printed line is the full redraw
                    // `terminal::facts` exists to stop.
                    if heard && let Some(listener) = tab.facts.listener() {
                        gpui::App::notify(cx, listener);
                    }
                    tab.watch_deadline = tab.next_deadline(std::time::Instant::now());
                    (tab.watch_deadline, tab.emulator.has_pending_events())
                });

                let Ok((deadline, pending_semantics)) = alive else {
                    return;
                };
                if pending_semantics {
                    // Keep draining bounded semantic batches without imposing a
                    // throughput cap; yield so input can run between them.
                    cx.background_executor().timer(Duration::ZERO).await;
                    continue;
                }
                let event = if let Some(deadline) = deadline {
                    let delay = deadline.saturating_duration_since(std::time::Instant::now());
                    tokio::select! {
                        _ = signal.notified() => true,
                        _ = cx.background_executor().timer(delay) => false,
                    }
                } else {
                    signal.notified().await;
                    true
                };
                if event {
                    // Preserve the old consumer's 16ms batching, but only after
                    // an event. Quiet views never arm this timer. Earlier known
                    // semantic deadlines win; protocol latency stays within the
                    // prior polling interval even during continuous output.
                    let coalesced = updated_at + scheduling::FRAME_INTERVAL;
                    let until = deadline.map_or(coalesced, |deadline| deadline.min(coalesced));
                    let delay = until.saturating_duration_since(std::time::Instant::now());
                    if !delay.is_zero() {
                        cx.background_executor().timer(delay).await;
                    }
                }
                // An already queued batch must yield before the next update.
                cx.background_executor().timer(Duration::ZERO).await;
            }
        }));
    }

    fn next_deadline(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        let mut deadlines = Vec::with_capacity(6);
        if !self.facts.exited() {
            deadlines.push(self.foreground_at.unwrap_or(now) + crate::PROBE);
        }
        if let Some(pending) = self.pending_size {
            deadlines.push(
                (pending.changed + crate::terminal::resize::QUIET)
                    .min(pending.since + crate::terminal::resize::LONGEST),
            );
        }
        deadlines.extend(self.redraw.deadline());
        deadlines.extend(self.emulator.sync_deadline());
        if !self.facts.exited() {
            deadlines.extend(self.caret.deadline(now));
        }
        if let Some(preview) = &self.restoring {
            deadlines.extend(preview.deadline());
        }
        if self.painted {
            deadlines.extend(scheduling::scrollbar_deadline(
                self.scrolled_at,
                self.painted_at,
            ));
        }
        deadlines.into_iter().min()
    }

    /// Mark the process ended, and tell the window if that is news.
    fn exit_heard(&mut self, cx: &mut Context<Self>) {
        if self.facts.set_exited()
            && let Some(listener) = self.facts.listener()
        {
            gpui::App::notify(cx, listener);
        }
    }

    /// Answer what the program asked — M5.12 task 6, decision 8.
    ///
    /// **On the tab's clock, never inside paint**: the grid's lock is taken
    /// here only for the colours a program set, let go, and then the answer
    /// is made and sent — the trap *Painting holds the grid's mutex for the
    /// whole frame*. A pixel size with no cell yet waits in
    /// `sizes_waiting`, eight at most — `reply::wait_for_cell`.
    // Pressure fixtures use the real passive response path without inventing
    // human/prompt facts. Bytes still traverse the actual bounded PTY writer.
    #[cfg(test)]
    pub(crate) fn protocol_text_for_test(&mut self, text: String, cx: &mut Context<Self>) {
        self.reply(Reply::Text(text), cx);
    }
    pub(crate) fn hear_input_refusals(&mut self, subscription: gpui::Subscription) {
        // Rebinding replaces the previous owner without a second listener.
        self.input_refusal_subscription = Some(subscription);
    }

    /// Only human gestures and passive protocol replies use this route. Managed
    /// submit returns its own error and must not also create a window notice.
    fn send_visible(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> bool {
        let admitted = self.emulator.send(bytes);
        if !admitted {
            self.report_input_refusal(cx);
        }
        admitted
    }
    fn report_input_refusal(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.emulator.input_refusal() {
            let reason = if self.emulator.input_delivery_failed() {
                "Terminal input was not queued; delivery of previously queued input is uncertain"
            } else {
                reason
            };
            // One slot and one weak deferred delivery per live source, even
            // when a gesture produces many refused reports in one effect cycle.
            if self.pending_input_refusal.replace(reason).is_none() {
                let source = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = source.update(cx, |tab, cx| {
                        if let Some(reason) = tab.pending_input_refusal.take() {
                            cx.emit(InputRefused(reason));
                        }
                    });
                });
            }
        }
    }

    fn reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        let overrides = match &reply {
            Reply::Colour(..) => self.emulator.term().lock().map(|term| *term.colors()).ok(),
            Reply::Text(_) | Reply::Size(_) => None,
        };
        let overrides = overrides.unwrap_or_default();
        let area = self
            .cell_px
            .map(|cell_px| super::reply::text_area(self.emulator.size(), cell_px));
        let palette = &self.palette;
        match super::reply::answer(
            &reply,
            |index| palette.answer_colour(index, &overrides),
            area,
        ) {
            Some(text) => {
                let _ = self.send_visible(text.as_bytes(), cx);
            }
            None => {
                if let Reply::Size(formatter) = reply {
                    super::reply::wait_for_cell(&mut self.sizes_waiting, formatter);
                }
            }
        }
    }

    /// Set the font size.
    ///
    /// The grid is recomputed on the next paint: the cell size is derived
    /// from the font rather than given as a number.
    /// Repaint with another palette.
    pub fn set_theme(&mut self, theme: crate::theme::Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.palette = Palette::new(theme);
        self.prepared_rows.clear();
        cx.notify();
    }

    /// Speak another language from now on — the find bar's words.
    pub fn set_ui(&mut self, ui: crate::i18n::Ui, cx: &mut Context<Self>) {
        self.ui = ui;
        cx.notify();
    }

    /// Draw with another family from now on.
    ///
    /// The cached resolution is dropped, not overwritten: what a person
    /// asked for and what the machine has are two different things, and
    /// only [`Self::font_family`] can tell them apart — it has the list.
    pub fn set_font(&mut self, family: Option<String>, cx: &mut Context<Self>) {
        self.chosen_font = family;
        self.font = None;
        cx.notify();
    }

    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.font_size = size;
        cx.notify();
    }

    /// Stop the process and keep what it printed. M8.4's Stop button;
    /// a tab has no such gesture — closing it is how a tab ends.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.launched_harness = None;
        self.emulator.kill();
        self.exit_heard(cx);
        cx.notify();
    }

    /// Hang the terminal up because the application is quitting — M5.14
    /// review: a hangup, so a shell passes it to its jobs, where Stop's
    /// SIGKILL left them outliving Guild.
    pub fn hang_up_for_quit(&mut self, cx: &mut Context<Self>) {
        self.launched_harness = None;
        self.save_screen();
        self.emulator.hang_up_for_quit();
        self.exit_heard(cx);
        cx.notify();
    }

    fn dismiss_restore(&mut self, cx: &mut Context<Self>) {
        if self.restoring.take().is_some() {
            cx.notify();
        }
    }

    fn save_screen(&mut self) {
        // Never replace a good cache with a half-loaded startup screen.
        if self.restoring.is_none()
            && let Some(cache) = self.screen_cache.take()
            && let Ok(term) = self.emulator.term().lock()
            && let Err(error) = cache.save(&term)
        {
            tracing::warn!(%error, "could not save the Codex terminal screen");
        }
    }

    #[cfg(test)]
    pub(crate) fn startup_diagnostics_for_test(&mut self) -> (Option<u32>, String) {
        (self.exit_status(), self.emulator.failure_tail())
    }

    #[cfg(test)]
    pub(crate) fn colors_for_test(
        &self,
    ) -> (
        crate::theme::Theme,
        alacritty_terminal::vte::ansi::Rgb,
        alacritty_terminal::vte::ansi::Rgb,
    ) {
        let colors = alacritty_terminal::term::color::Colors::default();
        (
            self.theme,
            self.palette.answer_colour(256, &colors),
            self.palette.answer_colour(257, &colors),
        )
    }

    /// What the process exited with, once it has — M8.4's Output panel
    /// asks, and a tab has no use for it.
    ///
    /// Polled rather than delivered: `TerminalEvent::Exited` says the pty
    /// closed, which is a different instant and carries no number. Task 0
    /// of M8.4 measured the gap between the two at 5 µs.
    pub fn exit_status(&mut self) -> Option<u32> {
        self.emulator.exit_status().map(|status| status.exit_code())
    }

    /// Stable completion published only after a terminal job's owned cleanup.
    pub(crate) fn job_completion_handle(&self) -> Option<super::emulator::JobCompletion> {
        self.emulator.job_completion_handle()
    }
    pub fn job_completion(&self) -> Option<Result<portable_pty::ExitStatus, String>> {
        self.emulator.job_completion()
    }

    /// The history and the screen as text, without touching the selection.
    #[cfg(test)]
    pub(crate) fn text_for_test(&self) -> String {
        let Ok(mut term) = self.emulator.term().lock() else {
            return String::new();
        };
        let kept = term.selection.take();
        term.selection = Some(crate::terminal::select::all(&term));
        let text = term.selection_to_string().unwrap_or_default();
        term.selection = kept;
        text
    }

    /// Close the program's input, so that every send is refused.
    #[cfg(test)]
    pub(crate) fn close_input_for_test(&mut self) {
        self.emulator.hang_up_for_quit();
    }

    /// Whether the last notify has been painted since.
    #[cfg(all(test, feature = "adaptive-terminal-refresh"))]
    pub(crate) fn painted_for_test(&self) -> bool {
        self.painted
    }

    /// Whether marked text is being composed.
    #[cfg(test)]
    pub(crate) fn composing_for_test(&self) -> bool {
        self.composition.active
    }

    /// Where the grid was last painted.
    #[cfg(test)]
    pub(crate) fn bounds_for_test(&self) -> Option<gpui::Bounds<Pixels>> {
        self.bounds
    }

    /// Whether a press is still selecting, heard past the grid's edge.
    #[cfg(test)]
    pub(crate) fn grabbing_for_test(&self) -> bool {
        self.grab != crate::terminal::select::Grab::None
    }

    /// Whether retained screen or scrollback contains displayable text.
    pub fn has_text(&self) -> bool {
        self.emulator.has_text()
    }

    /// Request job shutdown without announcing a premature terminal exit.
    pub fn cancel_job(&mut self) {
        self.emulator.kill();
    }

    /// Clear the screen and the history.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.dismiss_restore(cx);
        self.emulator.clear();
        cx.notify();
    }

    /// Recompute the grid for new area bounds.
    ///
    /// This is where "full screen does not stretch the terminal" is fixed:
    /// the size is told **both** to the grid and to the pty. Before, it
    /// only reached the grid, and the program kept drawing at the old
    /// width.
    fn fit(&mut self, bounds: Bounds<Pixels>, cell: gpui::Size<Pixels>) {
        let size = super::geometry::grid(bounds, cell);
        let (cols, rows) = (size.cols, size.rows);
        if let Some(start) = &self.start {
            let content = super::geometry::content(bounds);
            if content.size.width >= cell.width && content.size.height >= cell.height {
                start.remember(cols, rows);
            }
        }
        // **The first size at once, the rest when the hand pauses** —
        // M5.14. A held `cmd-alt-arrow` is thirty sizes a second, and
        // `claude` clears and repaints its screen for each one it is told;
        // the tick tells it once `resize::due`. A restored tab's first fit
        // is its real size and cannot wait — the StartSize trap.
        if !self.fitted {
            self.fitted = true;
            self.pending_size = None;
            self.emulator.resize(size);
            return;
        }
        if self.pending_size.is_none() && size == self.emulator.size() {
            return;
        }
        self.pending_size =
            crate::terminal::resize::want(self.pending_size, size, std::time::Instant::now());
    }
}

impl Drop for TerminalTab {
    fn drop(&mut self) {
        #[cfg(feature = "adaptive-terminal-refresh")]
        if let Some(demand) = self.emulator.visual_demand() {
            demand.close();
        }
        self.save_screen();
    }
}

impl Focusable for TerminalTab {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// How a terminal takes the place it is drawn in: all of it. The tab's own
/// root and the cached view a tab's body hands GPUI (`view::body`) are both
/// laid out by this — GPUI lays a cached view out from the style it is
/// handed, not from what it draws, so the two must not drift apart.
pub(crate) fn fill<S: gpui::Styled>(element: S) -> S {
    element.size_full()
}

impl Render for TerminalTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The tab takes focus itself when nothing has it. Without this a
        // freshly opened tab has to be hit with the mouse before it
        // accepts a key — and, worse, nothing in the window is focused, so
        // the window's own shortcuts have no path to dispatch along and
        // none of them work.
        //
        // *When nothing has it*, not *whenever it is not ours*: until
        // M8.1 the panels were empty and nothing but a tab could hold the
        // focus, so taking it back on every frame cost nothing. A tree in
        // the panel holds it now, and a terminal that took it back on the
        // next frame left the arrows with nobody. A click on the terminal
        // brings it back explicitly.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }

        if self.composition_blur.is_none() {
            self.composition_blur = Some(cx.on_blur(&self.focus, window, |tab, window, cx| {
                if !tab.focus.is_focused(window) {
                    tab.cancel_platform_composition(window, cx);
                }
            }));
        }
        let entity = cx.entity();
        let font_size = px(self.font_size);
        let line_height = font_size * theme::leading::TERMINAL;

        div()
            .track_focus(&self.focus)
            .key_context("terminal")
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|tab, event: &MouseDownEvent, _, _| {
                    tab.command_point = tab.link_point(event.position);
                }),
            )
            .on_key_down(cx.listener(|tab, event: &KeyDownEvent, window, cx| {
                if !gpui_component::WindowExt::has_focused_input(window, cx) {
                    tab.on_key(event, cx);
                }
            }))
            .on_scroll_wheel(
                cx.listener(|tab, event: &ScrollWheelEvent, _window, cx| tab.on_scroll(event, cx)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|tab, event: &MouseDownEvent, window, cx| {
                    tab.dismiss_restore(cx);
                    // A click is the way back from the tree: the terminal
                    // no longer takes the focus on its own while something
                    // else holds it.
                    window.focus(&tab.focus, cx);
                    tab.on_mouse_down(event, cx)
                }),
            )
            .on_mouse_move(
                cx.listener(|tab, event: &MouseMoveEvent, _window, cx| {
                    tab.on_mouse_move(event, cx)
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|tab, event: &MouseUpEvent, _window, cx| tab.on_mouse_up(event, cx)),
            )
            // M5.12: the keyboard's, the pointer's, the find's and the
            // links' own actions and listeners, each in its own file.
            .map(|el| self.bind_keys(el, cx))
            .map(|el| self.bind_select(el, cx))
            .map(|el| self.bind_find(el, cx))
            .map(|el| self.bind_links(el, cx))
            .relative()
            .map(fill)
            .bg(gpui::rgb(self.theme.surface.app))
            .child(
                canvas(
                    move |bounds, _window, _cx| bounds,
                    move |bounds, _, window, cx| {
                        entity.update(cx, |tab, _cx| {
                            #[cfg(feature = "adaptive-terminal-refresh")]
                            if !tab.refresh_policy(window, _cx) {
                                return;
                            }
                            let family = tab.font_family(window);
                            // The cell width is computed from the chosen
                            // font rather than given as a number:
                            // otherwise the grid drifts from what is
                            // actually drawn.
                            let cell = cell_size(window, &family, font_size, line_height);

                            tab.cell = Some(cell);
                            // In device pixels, for a program that asks
                            // for the text area in pixels.
                            let scale = window.scale_factor();
                            let device = |length: Pixels| {
                                (f32::from(length) * scale)
                                    .round()
                                    .clamp(0.0, f32::from(u16::MAX))
                                    as u16
                            };
                            tab.cell_px = Some((device(cell.width), device(cell.height)));
                            tab.bounds = Some(bounds);
                            tab.fit(bounds, cell);
                            tab.paint(bounds, cell, &family, font_size, window, _cx);
                            #[cfg(test)]
                            {
                                tab.paints += 1;
                            }
                            #[cfg(test)]
                            {
                                tab.text_client = tab.focus.is_focused(window);
                            }
                            if tab.focus.is_focused(window) {
                                window.handle_input(
                                    &tab.focus,
                                    gpui::ElementInputHandler::new(bounds, _cx.entity()),
                                    _cx,
                                );
                            } else {
                                // In a paint, which cannot notify; and an
                                // unfocused terminal draws no composition.
                                tab.composition.clear();
                            }
                            tab.paint_composition(&family, font_size, cell, window, _cx);
                            tab.redraw.painted();
                            tab.painted = true;
                            tab.painted_at = Some(std::time::Instant::now());
                            // Paint only wakes the consumer when it creates earlier
                            // semantic work. Ordinary output paints need no second
                            // PTY update after the event that requested them.
                            let now = std::time::Instant::now();
                            let earlier = tab.next_deadline(now).is_some_and(|next| {
                                tab.watch_deadline.is_none_or(|armed| next < armed)
                            });
                            if earlier || !tab.sizes_waiting.is_empty() {
                                tab.emulator.event_signal().notify_one();
                            }
                        });
                        // **A selection dragged out of the grid is heard by
                        // the window** — 2026-09-23. The element hears its
                        // moves only under the pointer, so past the edge
                        // the clock that scrolls the history never
                        // started. A window listener lasts one frame, so
                        // it is set again on each while a selection is
                        // being made; it never stops propagation, so what
                        // is under the pointer still hears its hover.
                        // Our selection and a drag the program is told
                        // about alike: both have to hear the pointer past
                        // the edge — 2026-09-24.
                        if entity.read(cx).grab != crate::terminal::select::Grab::None {
                            let heard = entity.downgrade();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, app| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !super::select::heard_by_window(
                                        true,
                                        bounds.contains(&event.position),
                                    )
                                {
                                    return;
                                }
                                let _ = heard.update(app, |tab, cx| tab.on_mouse_move(event, cx));
                            });
                            let released = entity.downgrade();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, app| {
                                if phase != gpui::DispatchPhase::Capture
                                    || event.button != MouseButton::Left
                                    || bounds.contains(&event.position)
                                {
                                    return;
                                }
                                let _ = released.update(app, |tab, cx| tab.on_mouse_up(event, cx));
                            });
                        }
                    },
                )
                .size_full(),
            )
            // Over the grid rather than above it: a bar that took a row
            // would resize the pty — section 6.30.
            .children(self.render_find_bar(window, cx))
    }
}

impl TerminalTab {
    /// The terminal font, chosen from what is installed.
    ///
    /// Chosen once: walking the font list every frame is noticeable work,
    /// and the list does not change.
    fn font_family(&mut self, window: &Window) -> gpui::SharedString {
        if let Some(family) = &self.font {
            return family.clone();
        }

        let chosen: gpui::SharedString = super::font::choose(
            &window.text_system().all_font_names(),
            self.chosen_font.as_deref(),
        )
        .into();
        self.font = Some(chosen.clone());
        tracing::info!(font = %chosen, "terminal font");
        chosen
    }
}

/// How a terminal is drawn.
///
/// Four values that always travelled together through four functions, which
/// is what clippy noticed when the fourth arrived: they are not four
/// arguments but one — the look. Every one of them is a person's setting,
/// and every one of them can change while a tab is open.
#[derive(Debug, Clone)]
pub struct Look {
    /// Copy the selection immediately, with no separate action.
    pub copy_on_select: bool,
    pub adaptive_terminal_refresh: bool,
    /// The size the grid is measured at.
    pub font_size: f32,
    /// The family a person asked for, if they asked for one.
    pub font: Option<String>,
    /// The palette.
    pub theme: crate::theme::Theme,
    /// The window's language, for the words a terminal draws of its own —
    /// the find bar's. Since M5.12's merge: before it a terminal knew none,
    /// and the bar read the language off the window's root.
    pub ui: crate::i18n::Ui,
}

impl Look {
    /// The look every terminal in the window takes, from the settings in
    /// force.
    ///
    /// One constructor rather than the same four fields assembled at each
    /// place a terminal is opened: a tab, a shell and M8.4's run are the
    /// same terminal, and a look built twice is two of them drifting.
    pub fn of(settings: &guild_session::Settings, theme: crate::theme::Theme) -> Self {
        Self {
            copy_on_select: settings.copy_on_select,
            adaptive_terminal_refresh: settings.adaptive_terminal_refresh,
            font_size: settings.terminal_font_size,
            font: settings.terminal_font.clone(),
            theme,
            ui: crate::i18n::Ui(settings.language),
        }
    }
}

/// Open a tab with an agent CLI in the given directory.
///
/// The process starts first: if it fails to start, no GPUI entity is
/// created at all, and the window has something to show instead.
#[allow(
    clippy::too_many_arguments,
    reason = "one argument per fact a launch needs; a struct to carry them would be \
    the same list with a name, built at one call site"
)]
pub fn open(
    harness: HarnessKind,
    // The record this tab is, which its process is told as `GUILD_SESSION`
    // — M13.3. Not the conversation's id: those differ for a resumed
    // OpenCode or Codex session.
    session: &str,
    cwd: &Path,
    conversation: &Conversation,
    permission_mode: guild_session::PermissionMode,
    prefix: &str,
    arguments: &str,
    ide: Option<u16>,
    hooks: Option<&guild_session::Hooks>,
    // The first thing the session is asked, when the command line started
    // it with a task — M13. `None` for every tab a person opens.
    first: Option<&str>,
    look: Look,
    start: &StartSize,
    cx: &mut gpui::App,
) -> Result<gpui::Entity<TerminalTab>, TerminalError> {
    open_with_selection(
        harness,
        session,
        cwd,
        conversation,
        permission_mode,
        prefix,
        arguments,
        ide,
        hooks,
        first,
        None,
        look,
        start,
        cx,
    )
}

/// Open with saved per-session role choices, without changing shared settings.
#[allow(
    clippy::too_many_arguments,
    reason = "same established terminal launch facts plus saved selection"
)]
pub fn open_with_selection(
    harness: HarnessKind,
    // The record this tab is, which its process is told as `GUILD_SESSION`
    // — M13.3. Not the conversation's id: those differ for a resumed
    // OpenCode or Codex session.
    session: &str,
    cwd: &Path,
    conversation: &Conversation,
    permission_mode: guild_session::PermissionMode,
    prefix: &str,
    arguments: &str,
    ide: Option<u16>,
    hooks: Option<&guild_session::Hooks>,
    // The first thing the session is asked, when the command line started
    // it with a task — M13. `None` for every tab a person opens.
    first: Option<&str>,
    selection: Option<&guild_session::harness::LaunchSelection>,
    look: Look,
    start: &StartSize,
    cx: &mut gpui::App,
) -> Result<gpui::Entity<TerminalTab>, TerminalError> {
    let resumed = match conversation {
        Conversation::Resume(id) => Some(id.as_str()),
        _ => None,
    };
    let cache = (harness == HarnessKind::Codex)
        .then(|| {
            restore::Cache::new(
                &crate::paths::state_dir().join("terminal-screens"),
                session,
                resumed,
            )
        })
        .flatten();
    let preview = cache.as_ref().and_then(|cache| {
        cache.preview(&guild_session::harness::CodexHarness::home(), start.get())
    });
    let probe = (harness == HarnessKind::Codex).then(|| {
        let resumed = match conversation {
            Conversation::Resume(id) => Some(id.as_str()),
            _ => None,
        };
        guild_session::codex_transcript::Probe::new(
            guild_session::harness::CodexHarness::home(),
            resumed,
        )
    });
    // The person's own status line, where Claude would find it for this
    // folder: with one, the session is given Guild's tap to chain to it.
    let hooks = hooks.map(|hooks| guild_session::Hooks {
        user_statusline: (harness == HarnessKind::Claude)
            .then(|| crate::hooks::user_statusline(cwd, &crate::hooks::claude_config_dir()))
            .flatten(),
        ..hooks.clone()
    });
    let mut command = harness_command(
        harness,
        session,
        conversation,
        permission_mode,
        ide,
        hooks.as_ref(),
        None,
        crate::paths::guild_dir().as_deref(),
        // The login shell's, before the one the window was started with —
        // decision 22 of M5.11.
        guild_session::login_path::path()
            .or_else(|| std::env::var_os("PATH"))
            .as_deref(),
    );
    let (material, diagnostics) = launch_discovery(
        &mut command,
        harness,
        &crate::paths::state_dir().join("discovery"),
        crate::paths::guild_dir().is_some() && hooks.is_some(),
        std::env::var_os("OPENCODE_CONFIG_CONTENT"),
    );
    if let Some(selection) = selection {
        let inherited = command
            .get_env("OPENCODE_CONFIG_CONTENT")
            .map(std::ffi::OsStr::to_os_string)
            .or_else(|| std::env::var_os("OPENCODE_CONFIG_CONTENT"));
        apply_selection(&mut command, harness, selection, inherited.as_deref())
            .map_err(TerminalError::Selection)?;
    }
    // A launch nonce distinguishes delayed hooks from a replaced runtime with
    // the same task/conversation. It is not a caller/session identity.
    let runtime = guild_session::SessionId::new().to_string();
    command.env("GUILD_RUNTIME", &runtime);
    if harness == HarnessKind::Codex {
        command.arg("-c");
        command.arg(format!(
            "shell_environment_policy.set.GUILD_RUNTIME={}",
            serde_json::to_string(&runtime).expect("runtime is JSON")
        ));
    }
    // Flags precede the user's untouched first prompt on every harness.
    if let Some(first) = first {
        if matches!(harness, HarnessKind::Codex | HarnessKind::Claude) {
            command.arg("--");
        }
        if harness == HarnessKind::OpenCode {
            command.arg("--prompt");
        }
        command.arg(first);
    }
    let extra = shell_words::split(arguments).map_err(|error| {
        TerminalError::Pty(anyhow::anyhow!("Invalid launch arguments: {error}"))
    })?;
    command
        .get_argv_mut()
        .splice(1..1, extra.into_iter().map(std::ffi::OsString::from));
    let network = crate::network::snapshot()
        .agent_environment(harness)
        .map_err(|e| TerminalError::Pty(anyhow::anyhow!("{e}")))?;
    crate::network::apply_pty_environment(&mut command, &network);
    let emulator = Emulator::spawn_harness(command, cwd, start.get(), prefix, &network)?;
    let start = Some(start.clone());
    let view = cx.new(|cx| TerminalTab::new(emulator, look, cwd.to_path_buf(), start, cx));
    view.update(cx, |view, _| {
        // An argv task is already input to this process, before its first hook.
        if let Some(material) = material {
            view.emulator.hold_launch_material(material);
        }
        view.launched_harness = Some(harness);
        view.runtime_identity = Some(runtime);
        view.discovery_diagnostics = diagnostics;
        view.prompt_input = first.is_some();
        if first.is_some() {
            view.programmatic_input();
        }
        view.codex_probe = probe;
        view.screen_cache = cache;
        view.restoring = preview.map(std::rc::Rc::new);
    });
    Ok(view)
}

/// The program a tab runs, as a command — what `open` launches, without
/// launching it.
///
/// `guild` is the folder this window's own `guild` sits in, and `path` the
/// `PATH` the window was started with; both are asked by the caller, so the
/// arguments can be checked without a process or an environment.
#[allow(
    clippy::too_many_arguments,
    reason = "a launch takes the same list at one call site"
)]
fn harness_command(
    harness: HarnessKind,
    session: &str,
    conversation: &Conversation,
    permission_mode: guild_session::PermissionMode,
    ide: Option<u16>,
    hooks: Option<&guild_session::Hooks>,
    first: Option<&str>,
    guild: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> CommandBuilder {
    let is_claude = harness == HarnessKind::Claude;
    let mut command = CommandBuilder::new(harness.program());
    // We assign the identifier: in terminal mode the output is
    // characters, and it cannot be learned from events. Claude takes
    // `--session-id` for a new conversation and `--resume` to come back;
    // the other two are resumed by id (and a *new* session generates its
    // own id, so nothing is passed on the command line).
    if is_claude {
        command.arg(conversation.flag());
        command.arg(conversation.id());
    } else if matches!(conversation, Conversation::Resume(_)) {
        for arg in harness.impl_().resume_args(conversation.id()) {
            command.arg(arg);
        }
    }
    // The shared Codex daemon retains another launch's Guild variables.
    // A private runtime keeps hook routing and conversation provenance local.
    if harness == HarnessKind::Codex {
        command.arg("--no-daemon");
    }
    // The status plugin needs this tab's environment. The shared OpenCode
    // service was started elsewhere and cannot inherit its session or socket.
    if harness == HarnessKind::OpenCode {
        command.arg("--standalone");
    }
    if is_claude {
        command.arg("--permission-mode");
        command.arg(permission_mode.as_cli_value());
    } else {
        for arg in harness.impl_().approval_args(permission_mode) {
            command.arg(arg);
        }
    }
    // M8.6: this window, by its port — Claude only.
    if is_claude && let Some(port) = ide {
        command.arg("--ide");
        command.env("CLAUDE_CODE_SSE_PORT", port.to_string());
    }
    // M5.10: the hooks that tell the window what this agent is doing,
    // beside the person's own rather than instead of them. For Claude the
    // hooks are named on its command line; for Codex and OpenCode they are
    // registered in their own config, and only the socket is given here.
    if let Some(hooks) = hooks {
        if is_claude {
            for arg in hooks.args() {
                command.arg(arg);
            }
            // What Guild's status line runs after the tap.
            if let Some(user) = &hooks.user_statusline {
                command.env(guild_session::USER_STATUSLINE_VAR, user);
            }
        }
        command.env(guild_session::HOOK_SOCKET_VAR, &hooks.socket);
        // The same socket, for the `guild` command a session may run —
        // M13. A session Guild did not start has neither variable, which
        // is what keeps a stray `guild` in somebody's login shell from
        // reaching a window they did not open.
        command.env(guild_session::SOCKET_VAR, &hooks.socket);
        // Who this session is, for the same command — M13. **The record,
        // not the conversation** — M13.3: a reopened OpenCode or Codex tab
        // resumes the harness's own id, and given that id its hooks named a
        // session the window has no record of and were dropped, and a
        // `guild` it ran was not recognised as a task's.
        command.env(guild_session::SESSION_VAR, session);
        if harness == HarnessKind::Codex {
            // A resumed TUI can retain these variables while its shell tools lose them.
            // Pin only this launch's provenance in Codex's subprocess policy; never
            // change the user's global config, environment filters or permissions.
            for (key, value) in [
                (guild_session::SESSION_VAR, session.to_owned()),
                (
                    guild_session::SOCKET_VAR,
                    hooks.socket.to_string_lossy().into_owned(),
                ),
                (
                    guild_session::HOOK_SOCKET_VAR,
                    hooks.socket.to_string_lossy().into_owned(),
                ),
            ] {
                command.arg("-c");
                command.arg(format!(
                    "shell_environment_policy.set.{key}={}",
                    serde_json::to_string(&value).expect("Guild provenance is serializable")
                ));
            }
        }
    }
    if harness == guild_session::HarnessKind::Codex && hooks.is_some() {
        command.env(
            crate::hooks::identity::DIRECTORY_VAR,
            crate::hooks::identity::directory(),
        );
    }
    // OpenCode's status plugin is registered in opencode's own config (the
    // plugins directory), not a command-line flag — opencode 2.0.10 has no
    // `--plugin-uri`. Only the socket above is given here; the plugin
    // no-ops outside a session Guild started.
    // M13: this window's own `guild`, first on the path, so a session told
    // to run it finds this window's command rather than another build.
    if let Some(dir) = guild {
        command.env("PATH", crate::paths::path_with(dir, path));
    }
    // Begin with the task. OpenCode's positional argument is a directory;
    // Claude and Codex accept the prompt itself as the last argument.
    if let Some(first) = first {
        if harness == HarnessKind::Codex {
            command.arg("--");
        }
        if harness == HarnessKind::OpenCode {
            command.arg("--prompt");
        }
        command.arg(first);
    }
    command
}

fn apply_selection(
    command: &mut CommandBuilder,
    harness: HarnessKind,
    selection: &guild_session::harness::LaunchSelection,
    inherited: Option<&std::ffi::OsStr>,
) -> Result<(), String> {
    if selection.model.is_none() && selection.effort.is_none() {
        return Ok(());
    }
    for arg in selection.args(harness)? {
        command.arg(arg);
    }
    if harness == HarnessKind::OpenCode {
        let inherited = inherited
            .map(|value| {
                value.to_str().ok_or(
                    "harness.opencode: inherited inline config is not UTF-8; selection not applied",
                )
            })
            .transpose()?;
        if let Some(content) = selection.opencode_inline(inherited)? {
            command.env("OPENCODE_CONFIG_CONTENT", content);
        }
    }
    Ok(())
}

/// Open a tab with an ordinary shell in the project directory.
///
/// There is no conversation here and cannot be: a shell is not a Claude
/// session, and neither an identifier nor `--resume` applies to it.
pub fn open_shell(
    cwd: &Path,
    // Who this tab is and where the window listens — M13, so a person can
    // type `guild status` in a terminal of Guild's the way an agent does.
    session: &str,
    hooks: Option<&guild_session::Hooks>,
    look: Look,
    start: &StartSize,
    cx: &mut gpui::App,
) -> Result<gpui::Entity<TerminalTab>, TerminalError> {
    let mut command = super::shell::command();
    super::shell::align_theme(&mut command, look.theme.kind);
    if let Some(hooks) = hooks {
        command.env(guild_session::SOCKET_VAR, &hooks.socket);
        command.env(guild_session::SESSION_VAR, session);
    }
    // A login shell rebuilds `PATH` from `/etc/paths` and the person's
    // profile, and `path_helper` moves the system's folders in front — so
    // this folder ends up after them rather than first. Nothing else is
    // called `guild`, so after is enough to be found.
    if let Some(dir) = crate::paths::guild_dir() {
        command.env(
            "PATH",
            crate::paths::path_with(
                &dir,
                guild_session::login_path::path()
                    .or_else(|| std::env::var_os("PATH"))
                    .as_deref(),
            ),
        );
    }
    crate::network::apply_pty(&mut command, guild_session::network::Scope::Terminal)
        .map_err(TerminalError::Pty)?;
    let material = super::shell_integration::prepare(&mut command)
        .map_err(|e| TerminalError::Pty(e.into()))?;
    let view = open_command(command, cwd, look, Some(start), cx)?;
    view.update(cx, |tab, _| tab.emulator.hold_shell_material(material));
    Ok(view)
}

/// Compose the launch's discovery context and retain actionable diagnostics.
fn launch_discovery(
    command: &mut CommandBuilder,
    harness: HarnessKind,
    directory: &Path,
    available: bool,
    inline: Option<std::ffi::OsString>,
) -> (Option<crate::discovery::Material>, Vec<String>) {
    let mut diagnostics = Vec::new();
    let material = if available {
        match crate::discovery::Material::prepare(directory) {
            Ok(mut material) => {
                let applied = if harness == HarnessKind::OpenCode
                    && inline
                        .as_ref()
                        .is_some_and(|value| value.to_str().is_none())
                {
                    Err(std::io::Error::other(
                        "OpenCode CLI discovery unavailable: inherited inline config is not UTF-8; preserved unchanged",
                    ))
                } else {
                    material.apply(
                        command,
                        harness,
                        inline.as_deref().and_then(|value| value.to_str()),
                    )
                };
                match applied {
                    Ok(()) => Some(material),
                    Err(error) => {
                        diagnostics.push(error.to_string());
                        None
                    }
                }
            }
            Err(error) => {
                diagnostics.push(format!("CLI discovery unavailable: {error}"));
                None
            }
        }
    } else {
        diagnostics.push(
            "CLI discovery unavailable: owning guild CLI or socket/session provenance is missing"
                .into(),
        );
        None
    };
    (material, diagnostics)
}

/// The size the window's tab terminals were last fitted to — what a new
/// tab is spawned at.
///
/// **A tab is spawned before it is drawn**, and every tab restored behind
/// another stays undrawn until it is chosen. The program reads its size
/// the moment it starts: `claude` printed its header and its first screens
/// at a made-up 80 × 24 and stretched only when the tab was first shown —
/// what was printed before that stayed narrow. Every tab of the window's
/// body has the body's bounds, so the last fit of any of them is the right
/// start for the next; the first of a run is seeded from the settings.
/// One cell shared by the window and its tabs; the Output panel's
/// terminal, whose area is a dock, is given none.
#[derive(Debug, Clone, Default)]
pub struct StartSize(std::rc::Rc<std::cell::Cell<Option<GridSize>>>);

impl StartSize {
    /// A start seeded from what the settings remember.
    pub fn seeded(grid: Option<guild_session::TabGrid>) -> Self {
        let start = Self::default();
        if let Some(grid) = grid {
            start.remember(grid.cols.into(), grid.rows.into());
        }
        start
    }

    /// The size to spawn at: the last fit, or 80 × 24 before there is one.
    pub fn get(&self) -> GridSize {
        self.0.get().unwrap_or(GridSize::new(80, 24))
    }

    /// Record a fit, in the cells the area holds before `GridSize` lifts a
    /// zero to one. An area collapsed to nothing is no size to start at.
    pub fn remember(&self, cols: usize, rows: usize) {
        if cols > 0 && rows > 0 {
            self.0.set(Some(GridSize::new(cols, rows)));
        }
    }

    /// The last fit, as the settings keep it.
    pub fn grid(&self) -> Option<guild_session::TabGrid> {
        self.0.get().map(|size| guild_session::TabGrid {
            cols: u16::try_from(size.cols).unwrap_or(u16::MAX),
            rows: u16::try_from(size.rows).unwrap_or(u16::MAX),
        })
    }
}

/// Open a tab with an arbitrary command.
///
/// `start` is the window's [`StartSize`] for a tab of its body, and `None`
/// for a terminal elsewhere — the Output panel's — which starts at 80 × 24
/// and is fitted by its first paint.
pub fn open_command(
    command: CommandBuilder,
    cwd: &Path,
    look: Look,
    start: Option<&StartSize>,
    cx: &mut gpui::App,
) -> Result<gpui::Entity<TerminalTab>, TerminalError> {
    let size = start.map_or(GridSize::new(80, 24), StartSize::get);
    let emulator = Emulator::spawn(command, cwd, size)?;
    let start = start.cloned();

    Ok(cx.new(|cx| TerminalTab::new(emulator, look, cwd.to_path_buf(), start, cx)))
}

/// Open a supervised job using the same emulator, grid and renderer as a tab.
pub fn open_job(
    command: CommandBuilder,
    cwd: &Path,
    look: Look,
    cx: &mut gpui::App,
) -> Result<gpui::Entity<TerminalTab>, TerminalError> {
    let emulator = Emulator::spawn_job(command, cwd, GridSize::new(80, 24))?;
    Ok(cx.new(|cx| TerminalTab::new(emulator, look, cwd.to_path_buf(), None, cx)))
}

/// The size of one cell.
fn cell_size(
    window: &Window,
    family: &str,
    font_size: Pixels,
    line_height: Pixels,
) -> gpui::Size<Pixels> {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TextRun};

    let font = Font {
        family: family.to_owned().into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };

    // Width measured on "M": in a monospace font every cell is equal, but
    // a space cannot be used — some fonts give it a different advance.
    let run = TextRun {
        len: 1,
        font,
        color: gpui::white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line("M".into(), font_size, &[run], None);

    gpui::Size {
        width: line.width.max(px(1.0)),
        height: line_height,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use super::*;

    #[cfg(feature = "native-preview")]
    #[gpui::test]
    fn synthetic_scroll_selection_and_clear_use_immediate_terminal_handlers(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.args(["-c", "i=0; while [ $i -lt 200 ]; do printf 'synthetic row %s\\r\\n' $i; i=$((i+1)); done; printf READY; sleep 2"]);
        let emulator =
            Emulator::spawn_job(command, std::path::Path::new("/"), GridSize::new(80, 24)).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let text: String = emulator
                .term()
                .lock()
                .unwrap()
                .grid()
                .display_iter()
                .map(|c| c.c)
                .collect();
            if text.contains("READY") {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        cx.update(|cx| {
            let mut look = Look::of(&Default::default(), Default::default());
            look.copy_on_select = false;
            let tab = cx.new(|cx| TerminalTab::new(emulator, look, PathBuf::from("/"), None, cx));
            tab.update(cx, |tab, cx| {
                let key = |key| gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                assert!(!tab.has_draft());
                let mut text = key("t");
                text.keystroke.key_char = Some("t".into());
                tab.on_key(&text, cx);
                assert!(tab.has_draft());
                tab.prompt_submitted();
                assert!(
                    tab.has_draft(),
                    "an unrelated/late event cannot erase a user draft"
                );
                tab.on_key(&key("enter"), cx);
                assert!(
                    !tab.has_draft(),
                    "submitted input must not be labelled an unsent draft"
                );
                assert!(
                    tab.awaiting_prompt,
                    "submission still waits for its actual adapter event"
                );
                assert!(
                    !tab.initial_prompt(),
                    "submitted input cannot admit another handoff"
                );
                tab.prompt_submitted();
                assert!(!tab.has_draft());
                tab.on_key(&key("shift-pageup"), cx);
                assert!(tab.probe_interaction_state().0 > 0);
                tab.select_all(cx);
                assert!(tab.probe_interaction_state().1);
                tab.on_key(&key("escape"), cx);
                assert_eq!(tab.probe_interaction_state(), (0, false));
                tab.cancel_job();
            });
        });
    }

    /// **A tab that was never drawn still answers the OSC 7501 probe**, and
    /// before the DA1 after it: a session restored into a tab that is not in
    /// front asks within its first two seconds, and Claude Code settles the
    /// answer for the session. The program echoes the first bytes it reads.
    #[gpui::test]
    fn a_never_painted_tab_answers_the_program_status_probe(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            r"stty raw -echo; printf '\033]7501;?\007\033[c'; dd bs=1 count=16 2>/dev/null | od -An -c | tr -d ' \n'; printf ' END'; sleep 5",
        ]);
        let tab = cx.update(|cx| {
            open_command(
                command,
                std::path::Path::new("/"),
                Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap()
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let text = loop {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            cx.run_until_parked();
            let text: String = tab.read_with(cx, |tab, _| {
                tab.emulator
                    .term()
                    .lock()
                    .unwrap()
                    .grid()
                    .display_iter()
                    .map(|cell| cell.c)
                    .collect()
            });
            if text.contains(" END") || std::time::Instant::now() >= until {
                break text;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(
            text.contains(r"033]7501;?:states="),
            "the program read the 7501 reply first: {text:?}"
        );
        tab.update(cx, |tab, _| tab.cancel_job());
    }

    // This lives beside the view because the grid is intentionally private;
    // retaining an Entity alone does not prove that its output survived.
    #[gpui::test]
    fn terminal_job_retains_completed_grid_without_an_open_view(cx: &mut gpui::TestAppContext) {
        use crate::terminal_job::LiveJobs;
        let temp = tempfile::tempdir().unwrap();
        let owner = guild_session::SessionId::new();
        let spec = serde_json::from_value(serde_json::json!({
            "version":1,"request_id":"grid", "program":"/bin/sh",
            "args":["-c","printf retained-grid-sentinel; exit 7"],"cwd":temp.path(),
            "target":{"kind":"session","id":owner}
        }))
        .unwrap();
        cx.update(|cx| {
            let mut jobs = LiveJobs::load(temp.path().join("jobs.json"));
            let receipt = jobs
                .start(
                    owner,
                    guild_session::ProjectId::new(),
                    &spec,
                    Look::of(&Default::default(), Default::default()),
                    cx,
                )
                .unwrap();
            let finished = jobs
                .terminal(&receipt.id)
                .unwrap()
                .read(cx)
                .emulator
                .transport_finished_for_test();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                jobs.poll(cx);
                let terminal = jobs.terminal(&receipt.id).unwrap();
                let text: String = terminal
                    .read(cx)
                    .emulator
                    .term()
                    .lock()
                    .unwrap()
                    .grid()
                    .display_iter()
                    .map(|cell| cell.c)
                    .collect();
                if jobs.receipt(&receipt.id).unwrap().state.is_terminal()
                    && text.contains("retained-grid-sentinel")
                    && finished.load(std::sync::atomic::Ordering::Acquire)
                {
                    assert_eq!(jobs.receipt(&receipt.id).unwrap().exit_code, Some(7));
                    break;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "completed output was not retained: {text:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            jobs.release(&receipt.id).unwrap();
            assert!(jobs.terminal(&receipt.id).is_none());
        });
    }

    #[test]
    fn saved_selection_reaches_a_controlled_real_pty_without_profile_or_permission_rewrites() {
        use guild_session::{PermissionMode, harness::LaunchSelection};
        let directory = tempfile::tempdir().unwrap();
        let selection = LaunchSelection {
            model: Some("provider/model".into()),
            effort: Some("high".into()),
        };
        let inline = r#"{"permissions":["retained"],"plugins":["fixture-plugin"]}"#;
        for harness in HarnessKind::ALL {
            let original = harness_command(
                harness,
                "fixture-record",
                &Conversation::Resume("same-conversation".into()),
                PermissionMode::Manual,
                None,
                None,
                None,
                None,
                None,
            );
            let baseline = argv(&original);
            let mut selected = harness_command(
                harness,
                "fixture-record",
                &Conversation::Resume("same-conversation".into()),
                PermissionMode::Manual,
                None,
                None,
                None,
                None,
                None,
            );
            selected.env("OPENCODE_CONFIG_CONTENT", inline);
            apply_selection(&mut selected, harness, &selection, Some(OsStr::new(inline))).unwrap();
            assert_eq!(
                &argv(&selected)[..baseline.len()],
                baseline.as_slice(),
                "original resume/permission options must be retained"
            );
            let output = directory.path().join(format!("{harness:?}.json"));
            // A Python fixture is always the executable, never the configured model CLI.
            let mut fixture = CommandBuilder::new("/usr/bin/python3");
            fixture.args(["-I","-c","import json,os,sys; f=open(os.environ['GUILD_SELECTION_WITNESS'],'w'); json.dump({'argv':sys.argv[1:],'inline':json.loads(os.environ['OPENCODE_CONFIG_CONTENT'])},f); f.close(); print('FIXTURE COMPLETE',flush=True)"]);
            fixture.args(selected.get_argv().iter().skip(1));
            fixture.env("GUILD_SELECTION_WITNESS", &output);
            fixture.env(
                "OPENCODE_CONFIG_CONTENT",
                selected.get_env("OPENCODE_CONFIG_CONTENT").unwrap(),
            );
            let mut emulator =
                Emulator::spawn_job(fixture, directory.path(), GridSize::new(120, 24)).unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(status) = emulator.exit_status() {
                    assert!(status.success());
                    break;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "owned fixture did not exit"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let observed: serde_json::Value =
                serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
            assert_eq!(observed["argv"], serde_json::json!(&argv(&selected)[1..]));
            assert_eq!(
                observed["inline"]["permissions"],
                serde_json::json!(["retained"])
            );
            assert_eq!(
                observed["inline"]["plugins"],
                serde_json::json!(["fixture-plugin"])
            );
            if harness == HarnessKind::OpenCode {
                assert_eq!(observed["inline"]["model"], "provider/model#high");
            }
        }
    }

    #[test]
    fn empty_role_selection_keeps_non_utf8_inherited_inline_bytes_untouched() {
        use std::os::unix::ffi::OsStringExt;
        let inherited = std::ffi::OsString::from_vec(vec![0xff, 0xfe]);
        let mut command = CommandBuilder::new("controlled-fixture-only");
        command.env("OPENCODE_CONFIG_CONTENT", &inherited);
        apply_selection(
            &mut command,
            HarnessKind::OpenCode,
            &Default::default(),
            Some(&inherited),
        )
        .unwrap();
        assert_eq!(
            command.get_env("OPENCODE_CONFIG_CONTENT"),
            Some(inherited.as_os_str())
        );
        assert_eq!(command.get_argv().len(), 1);
    }

    fn argv(command: &CommandBuilder) -> Vec<String> {
        command
            .get_argv()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// **A tab starts at the size the window's tabs were last fitted to** —
    /// 2026-09-16: a tab restored behind another is spawned before it is
    /// drawn, and `claude` printed its first screen at a made-up 80 × 24.
    /// Seeded from the settings, moved by every fit, and never to a size a
    /// collapsed area gives.
    #[test]
    fn a_tab_starts_at_the_size_the_window_s_tabs_were_last_fitted_to() {
        let fresh = StartSize::default();
        assert_eq!(fresh.get(), GridSize::new(80, 24), "nothing known yet");

        let seeded = StartSize::seeded(Some(guild_session::TabGrid {
            cols: 122,
            rows: 29,
        }));
        assert_eq!(seeded.get(), GridSize::new(122, 29));

        let shared = seeded.clone();
        shared.remember(140, 40);
        assert_eq!(seeded.get(), GridSize::new(140, 40), "one cell, shared");

        seeded.remember(0, 40);
        seeded.remember(140, 0);
        assert_eq!(seeded.get(), GridSize::new(140, 40), "zero is no size");

        assert_eq!(
            seeded.grid(),
            Some(guild_session::TabGrid {
                cols: 140,
                rows: 40
            })
        );
    }

    fn hooks() -> guild_session::Hooks {
        guild_session::Hooks {
            settings: PathBuf::from("/state/hooks/claude-settings.json"),
            statusline_settings: PathBuf::from("/state/hooks/claude-settings-statusline.json"),
            socket: PathBuf::from("/tmp/guild-hooks-1.sock"),
            user_statusline: None,
        }
    }

    #[test]
    fn resumed_claude_launch_is_quiet_and_retains_concrete_discovery_errors() {
        let root = tempfile::tempdir().unwrap();
        let installed = crate::hooks::install(root.path()).unwrap();
        let hooks = guild_session::Hooks {
            settings: installed.settings.clone(),
            statusline_settings: installed.statusline_settings.clone(),
            socket: root.path().join("synthetic-unbound.sock"),
            user_statusline: None,
        };
        let conversation = Conversation::Resume("synthetic-saved".into());
        for broken in [false, true] {
            let mut command = harness_command(
                HarnessKind::Claude,
                "synthetic-owned",
                &conversation,
                guild_session::PermissionMode::Manual,
                None,
                Some(&hooks),
                None,
                Some(Path::new("/synthetic/bin")),
                None,
            );
            if broken {
                std::fs::write(&installed.settings, r#"{"hooks":[]}"#).unwrap();
            }
            let (material, diagnostics) =
                launch_discovery(&mut command, HarnessKind::Claude, root.path(), true, None);
            if broken {
                assert!(material.is_none());
                assert_eq!(
                    diagnostics.len(),
                    1,
                    "only the concrete failure: {diagnostics:?}"
                );
                assert_eq!(diagnostics[0], "Claude hooks must be an object");
            } else {
                assert!(material.is_some());
                assert!(
                    diagnostics.is_empty(),
                    "ordinary resume must be quiet: {diagnostics:?}"
                );
            }
            // Execute only a fake child with the production launch's literal arguments.
            let args = argv(&command);
            let output = std::process::Command::new("/bin/sh")
                .args(["-c", "printf '%s\n' \"$@\"", "fake-claude"])
                .args(&args[1..])
                .output()
                .unwrap();
            assert!(output.status.success());
            let output = String::from_utf8(output.stdout).unwrap();
            assert!(output.contains("--resume\nsynthetic-saved"));
            println!("fake child broken={broken}: diagnostics={diagnostics:?}");
        }
    }

    #[test]
    fn round04_complete_claude_launch_retains_status_and_discovery_settings() {
        let fixture = std::env::var_os("GUILD_ROUND04_FIXTURE_DIRECTORY").map(PathBuf::from);
        let root = match &fixture {
            Some(path) => {
                std::fs::create_dir_all(path).unwrap();
                tempfile::tempdir_in(path).unwrap()
            }
            None => tempfile::tempdir().unwrap(),
        };
        let mut records = Vec::new();
        let installed = crate::hooks::install(root.path()).unwrap();
        for statusline in [false, true] {
            let hooks = guild_session::Hooks {
                settings: installed.settings.clone(),
                statusline_settings: installed.statusline_settings.clone(),
                socket: root.path().join("synthetic-unbound.sock"),
                user_statusline: statusline.then(|| "synthetic-user-statusline".into()),
            };
            let original: serde_json::Value = serde_json::from_slice(
                &std::fs::read(if statusline {
                    &hooks.statusline_settings
                } else {
                    &hooks.settings
                })
                .unwrap(),
            )
            .unwrap();
            for conversation in [
                Conversation::New("00000000-0000-4000-8000-000000000401".into()),
                Conversation::Resume("00000000-0000-4000-8000-000000000402".into()),
            ] {
                let mut command = harness_command(
                    HarnessKind::Claude,
                    "synthetic-owned",
                    &conversation,
                    guild_session::PermissionMode::Manual,
                    None,
                    Some(&hooks),
                    None,
                    Some(Path::new("/synthetic/bin")),
                    None,
                );
                let (material, diagnostics) =
                    launch_discovery(&mut command, HarnessKind::Claude, root.path(), true, None);
                assert!(diagnostics.is_empty(), "{diagnostics:?}");
                let material = material.unwrap();
                let args = argv(&command);
                assert_eq!(
                    args.iter().filter(|a| a.as_str() == "--settings").count(),
                    1,
                    "the actual Claude loader selects only its last --settings"
                );
                let at = args.iter().position(|a| a == "--settings").unwrap();
                let combined: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&args[at + 1]).unwrap()).unwrap();
                for (key, value) in original.as_object().unwrap() {
                    if key != "hooks" {
                        assert_eq!(&combined[key], value);
                    }
                }
                for (event, groups) in original["hooks"].as_object().unwrap() {
                    let combined_groups = combined["hooks"][event].as_array().unwrap();
                    assert_eq!(
                        &combined_groups[..groups.as_array().unwrap().len()],
                        groups.as_array().unwrap()
                    );
                }
                assert_eq!(
                    combined["hooks"]["SessionStart"].as_array().unwrap().len(),
                    2
                );
                assert_eq!(
                    command
                        .get_env(guild_session::USER_STATUSLINE_VAR)
                        .is_some(),
                    statusline
                );
                if fixture.is_some() {
                    records.push(serde_json::json!({"argv":args,"statusline":statusline,"resume":matches!(conversation,Conversation::Resume(_))}));
                    material.keep_fixture();
                }
            }
        }
        if let Some(path) = fixture {
            std::fs::write(
                path.join("complete-claude-commands.json"),
                serde_json::to_vec(&records).unwrap(),
            )
            .unwrap();
            let _ = root.keep();
        }
    }

    /// **A session Guild starts is told `guild` is there — M13 — and the
    /// task is still the last word.** `claude` reads its first prompt as
    /// the positional argument, so anything placed after it would be read
    /// as the prompt instead; and a window with no `guild` beside it, or no
    /// socket to answer on, must not send an agent to a command that cannot
    /// reach it.
    #[test]
    fn a_session_is_told_of_guild_only_when_guild_can_answer() {
        let conversation = Conversation::New("0f0e".to_owned());
        let told = |guild: Option<&Path>, hooks: Option<&guild_session::Hooks>| {
            argv(&harness_command(
                HarnessKind::Claude,
                "0f0e",
                &conversation,
                guild_session::PermissionMode::Auto,
                None,
                hooks,
                Some("fix the build"),
                guild,
                Some(OsStr::new("/usr/bin")),
            ))
        };

        let both = told(Some(Path::new("/app/bin")), Some(&hooks()));
        assert!(
            !both.iter().any(|arg| arg == "--append-system-prompt"),
            "recorded system text must not receive host mandates"
        );
        assert_eq!(both.last().map(String::as_str), Some("fix the build"));

        for argv in [
            told(None, Some(&hooks())),
            told(Some(Path::new("/app/bin")), None),
        ] {
            assert!(
                !argv.iter().any(|arg| arg == "--append-system-prompt"),
                "told of a command that cannot answer: {argv:?}"
            );
        }
    }

    /// OpenCode treats a positional argument as a directory, so a task
    /// must be passed with --prompt. Its private server also inherits the
    /// tab's hook environment, unlike a shared service started elsewhere.
    #[test]
    fn an_opencode_task_is_a_prompt_not_a_directory() {
        let task = "Reply only kiwi. Do not use tools.";
        let args = argv(&harness_command(
            HarnessKind::OpenCode,
            "record",
            &Conversation::New("conversation".to_owned()),
            guild_session::PermissionMode::Auto,
            None,
            None,
            Some(task),
            None,
            None,
        ));
        assert_eq!(args, ["opencode", "--standalone", "--prompt", task]);
    }

    #[test]
    fn codex_keeps_its_hooks_in_the_tabs_own_process_environment() {
        for conversation in [
            Conversation::New("new".into()),
            Conversation::Resume("known".into()),
        ] {
            let command = harness_command(
                HarnessKind::Codex,
                "record",
                &conversation,
                guild_session::PermissionMode::Auto,
                None,
                Some(&hooks()),
                None,
                None,
                None,
            );
            assert_eq!(
                command.get_env(crate::hooks::identity::DIRECTORY_VAR),
                Some(crate::hooks::identity::directory().as_os_str())
            );
            let args = argv(&command);
            assert!(args.iter().any(|arg| arg == "--no-daemon"), "{args:?}");
            assert!(
                args.iter()
                    .any(|arg| arg == "shell_environment_policy.set.GUILD_SESSION=\"record\""),
                "shell tools must receive this record, not only the TUI: {args:?}"
            );
            assert!(
                args.iter()
                    .any(|arg| arg.starts_with("shell_environment_policy.set.GUILD_SOCKET=")),
                "{args:?}"
            );
        }
    }

    /// **A tab tells its process which record it is, not which conversation
    /// it resumes** — M13.3. A reopened OpenCode or Codex tab resumes the
    /// harness's own id, and was given that id as `GUILD_SESSION`: its hooks
    /// named a session the window has no record of and were dropped, and a
    /// `guild` it ran was not recognised as a task's.
    #[test]
    fn the_process_is_told_its_record_whatever_it_resumes() {
        for harness in HarnessKind::ALL {
            let command = harness_command(
                harness,
                "11111111-2222-3333-4444-555555555555",
                &Conversation::Resume("theirs-0f0e".to_owned()),
                guild_session::PermissionMode::Auto,
                None,
                Some(&hooks()),
                None,
                None,
                None,
            );
            assert_eq!(
                command.get_env(guild_session::SESSION_VAR),
                Some(OsStr::new("11111111-2222-3333-4444-555555555555")),
                "{harness:?}"
            );
        }
    }

    /// **A new Claude session is started, not resumed** — the M14 bug this
    /// catches made `harness_command` pass `--resume <id>` for a fresh
    /// session, and `claude` refuses a newly assigned id as *no
    /// conversation found*. Each harness gets its own flag for its own
    /// modes.
    #[test]
    fn the_launch_uses_the_conversation_the_right_way() {
        let new = |harness| {
            argv(&harness_command(
                harness,
                "0f0e",
                &Conversation::New("0f0e".to_owned()),
                guild_session::PermissionMode::Auto,
                None,
                None,
                None,
                None,
                None,
            ))
        };
        let resume = |harness| {
            argv(&harness_command(
                harness,
                "0f0e",
                &Conversation::Resume("0f0e".to_owned()),
                guild_session::PermissionMode::Auto,
                None,
                None,
                None,
                None,
                None,
            ))
        };

        // Claude: --session-id to start, --resume to come back.
        let cn = new(HarnessKind::Claude);
        assert_eq!(cn.get(1).map(String::as_str), Some("--session-id"));
        assert_eq!(cn.get(2).map(String::as_str), Some("0f0e"));
        let cr = resume(HarnessKind::Claude);
        assert_eq!(cr.get(1).map(String::as_str), Some("--resume"));
        assert_eq!(cr.get(2).map(String::as_str), Some("0f0e"));

        // OpenCode: a new session passes nothing, a resumed one --session.
        let on = new(HarnessKind::OpenCode);
        assert!(
            !on.contains(&"--session".to_owned()),
            "fresh OpenCode has no id: {on:?}"
        );
        let or_ = resume(HarnessKind::OpenCode);
        assert_eq!(or_.get(1).map(String::as_str), Some("--session"));
        assert_eq!(or_.get(2).map(String::as_str), Some("0f0e"));

        // Codex: fresh passes nothing, resumed is `resume <id>`.
        let xn = new(HarnessKind::Codex);
        assert!(
            !xn.contains(&"resume".to_owned()),
            "fresh Codex has no id: {xn:?}"
        );
        let xr = resume(HarnessKind::Codex);
        assert_eq!(xr.get(1).map(String::as_str), Some("resume"));
        assert_eq!(xr.get(2).map(String::as_str), Some("0f0e"));
    }

    #[test]
    fn codex_resume_startup_prompt_is_literal_after_option_separator() {
        let prompt = "--help\nпродолжит $HOME; `touch nope`";
        let args = argv(&harness_command(
            HarnessKind::Codex,
            "guild-record",
            &Conversation::Resume("exact-conversation".into()),
            guild_session::PermissionMode::BypassPermissions,
            None,
            None,
            Some(prompt),
            None,
            None,
        ));
        assert_eq!(&args[1..3], ["resume", "exact-conversation"]);
        assert_eq!(&args[args.len() - 2..], ["--", prompt]);
        assert!(!args.iter().any(|arg| arg == "-s"));
    }

    #[test]
    fn codex_launch_and_resume_leave_permissions_to_native_codex() {
        for permission in [
            guild_session::PermissionMode::Auto,
            guild_session::PermissionMode::Manual,
            guild_session::PermissionMode::AcceptEdits,
            guild_session::PermissionMode::Plan,
            guild_session::PermissionMode::BypassPermissions,
            guild_session::PermissionMode::DontAsk,
        ] {
            let args = argv(&harness_command(
                HarnessKind::Codex,
                "guild-record",
                &Conversation::Resume("conversation".into()),
                permission,
                None,
                None,
                None,
                None,
                None,
            ));
            assert!(!args.iter().any(|arg| arg == "-s"), "{args:?}");
            let fresh = argv(&harness_command(
                HarnessKind::Codex,
                "guild-record",
                &Conversation::New("conversation".into()),
                permission,
                None,
                None,
                None,
                None,
                None,
            ));
            assert_eq!(fresh, ["codex", "--no-daemon"], "{permission:?}");
        }
    }
}
