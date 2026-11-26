//! The pseudo-terminal and the grid state.
//!
//! Everything that is not about drawing lives here: the process in a pty,
//! ANSI parsing through `alacritty_terminal` and the grid size. Painting
//! is in [`super::paint`], input in [`super::keys`].
//!
//! The split is not cosmetic: the state is covered by tests without a
//! window — you can feed the emulator bytes and look at what ended up in
//! the grid. The same trick as with `guild-session`, which deliberately
//! has no GPUI.

use std::cell::RefCell;
use std::ffi::OsStr;
mod events;
mod job;
use events::{Receiver, Sender, channel};
use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::ClipboardType;
use alacritty_terminal::term::Osc52;

use super::reply::{Formatter, Reply};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::viewport_to_point;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
#[cfg(test)]
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use portable_pty::{
    Child, CommandBuilder, ExitStatus, MasterPty, NativePtySystem, PtySize, PtySystem,
};

pub(crate) type JobCompletion = Arc<Mutex<Option<Result<ExitStatus, String>>>>;

type Teardown = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

pub(crate) const INPUT_BYTES: usize = 8 * 1024 * 1024;

type PreparedPty = (std::fs::File, Box<dyn std::io::Read + Send>);

fn prepare_descriptors(master: &dyn MasterPty) -> Result<PreparedPty, TerminalError> {
    // **Our own copy of the master's descriptor, not portable-pty's
    // writer** — M5.14 review. That writer writes a newline and VEOF
    // when it is dropped, and with closing a hangup rather than a
    // kill, a program that outlived the hangup was typed an Enter.
    let fd = master
        .as_raw_fd()
        .ok_or_else(|| TerminalError::Pty(anyhow::anyhow!("the pty has no master descriptor")))?;
    // SAFETY: master owns fd. Set CLOEXEC atomically so concurrent ordinary
    // subprocesses cannot inherit the terminal input writer.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if copy < 0 {
        return Err(TerminalError::Pty(std::io::Error::last_os_error().into()));
    }
    use std::os::fd::FromRawFd as _;
    // SAFETY: the fresh descriptor is owned exclusively by this File.
    let writer = unsafe { std::fs::File::from_raw_fd(copy) };
    crate::transport::nonblocking(copy).map_err(|e| TerminalError::Pty(e.into()))?;
    let reader = master.try_clone_reader().map_err(TerminalError::Pty)?;
    Ok((writer, reader))
}

fn pty_writer_anchor(master: &dyn MasterPty) -> Result<std::fs::File, TerminalError> {
    use std::os::fd::FromRawFd;
    let fd = master
        .as_raw_fd()
        .ok_or_else(|| TerminalError::Pty(anyhow::anyhow!("PTY master unavailable")))?;
    // SAFETY: fresh independently owned alias, atomically CLOEXEC.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if copy < 0 {
        return Err(TerminalError::Pty(std::io::Error::last_os_error().into()));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(copy) })
}

/// What prevented opening the terminal.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// A saved per-session selection cannot be safely applied to this harness.
    #[error("could not apply session selection: {0}")]
    Selection(String),
    /// The terminal child could not be assigned a supervisor.
    #[error("could not supervise the terminal child: {0}")]
    Supervise(#[source] std::io::Error),
    /// Could not allocate a pseudo-terminal.
    #[error("could not open a pty: {0}")]
    Pty(#[source] anyhow::Error),

    /// The command did not start.
    #[error("could not run {command} in a pty: {source}. Check that it is on the PATH")]
    Spawn {
        /// What we tried to run.
        command: String,
        /// Why it failed.
        #[source]
        source: anyhow::Error,
    },
}

/// The grid size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    /// Columns.
    pub cols: usize,
    /// Rows.
    pub rows: usize,
}

impl GridSize {
    /// A size that is never zero.
    ///
    /// A zero-width grid crashes `alacritty_terminal`, and getting one is
    /// easy: the window collapses for an instant when minimised.
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols: cols.max(1),
            rows: rows.max(1),
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }

    fn last_column(&self) -> Column {
        Column(self.cols.saturating_sub(1))
    }
}

/// An emulator event that has to reach the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    /// The grid changed — it needs repainting.
    Wakeup,
    /// The program changed the title.
    Title(String),
    /// The program asks to write to the clipboard — OSC 52, copy only, and
    /// only to the clipboard: a store aimed at the primary selection, which
    /// macOS does not have, is dropped as alacritty drops it there.
    ClipboardStore(String),
    /// The program asked the terminal something, and the answer goes back
    /// to it — M5.12 task 6. Answered on the tab's clock, never here on the
    /// reader thread: a colour needs the palette and a pixel size the cell.
    Reply(super::reply::Reply),
    /// The process exited.
    Exited,
}

/// The bridge between `alacritty_terminal` and the window.
///
/// Public even though it is only created here: it appears in the `Term`
/// type that the painting reads. Hiding it would hide the grid too.
#[derive(Clone)]
pub struct Listener(Sender);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let translated = match event {
            Event::Wakeup => TerminalEvent::Wakeup,
            Event::Title(title) => TerminalEvent::Title(title),
            Event::ClipboardStore(ClipboardType::Clipboard, data) => {
                TerminalEvent::ClipboardStore(data)
            }
            Event::Exit => TerminalEvent::Exited,
            Event::PtyWrite(text) => TerminalEvent::Reply(Reply::Text(text)),
            Event::ColorRequest(index, formatter) => {
                TerminalEvent::Reply(Reply::Colour(index, Formatter(formatter)))
            }
            Event::TextAreaSizeRequest(formatter) => {
                TerminalEvent::Reply(Reply::Size(Formatter(formatter)))
            }
            // The window does not need the rest right now, but a repaint
            // does no harm: an event means the state changed.
            _ => TerminalEvent::Wakeup,
        };
        let _ = self.0.send(translated);
    }
}

/// How long a closed tab's process has to go on its own after the
/// hangup before it is killed.
const HANG_UP_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// How long quitting waits for a shell to pass its hangup on.
const QUIT_GRACE: std::time::Duration = std::time::Duration::from_millis(100);

/// A terminal hung up, and the process groups the hangup went to —
/// **found once, at the hangup**: when the leader of the session dies the
/// kernel takes the terminal from the session, and a program asked about
/// later has no terminal to be found by. M5.14 review.
struct Hangup {
    ownership: std::sync::Arc<super::ownership::Cleanup>,
    leader: Option<libc::pid_t>,
    groups: Vec<libc::pid_t>,
}

impl Drop for Hangup {
    fn drop(&mut self) {
        self.ownership.finish();
    }
}

impl Hangup {
    /// Whether everything is gone: the leader ended, and every other
    /// group empty.
    fn gone(&self) -> bool {
        self.leader.is_none_or(ended_unreaped)
            && self
                .groups
                .iter()
                .filter(|group| Some(**group) != self.leader)
                // SAFETY: a signal 0 is a question, and sends nothing.
                .all(|group| {
                    // Darwin killpg excludes zombies. Completion must also
                    // wait for the captured foreground leader to disappear.
                    // These are observations only, never additional signals.
                    unsafe { libc::killpg(*group, 0) != 0 && libc::kill(*group, 0) != 0 }
                })
    }

    /// Kill what is left: the leader's group, and every other group still
    /// of the leader's session. Called while the leader is unreaped, so the
    /// session's number is still ours and nobody else's.
    fn kill_survivors(&self) {
        if self.gone() {
            return;
        }
        for group in &self.groups {
            // SAFETY: getsid asks; killpg goes to the leader's own group or
            // to a group of its session.
            let ours = Some(*group) == self.leader
                || self
                    .leader
                    .is_some_and(|leader| unsafe { libc::getsid(*group) } == leader);
            if ours {
                unsafe { libc::killpg(*group, libc::SIGKILL) };
            }
        }
    }
}

/// The process groups with a process whose controlling terminal is
/// `tty`, the leader's own pid left out: it is a zombie until reaped.
fn groups_on(tty: libc::dev_t, leader: Option<libc::pid_t>) -> Vec<libc::pid_t> {
    // libproc's PROC_TTY_ONLY, which `libc` does not name.
    const PROC_TTY_ONLY: u32 = 3;
    let mut pids: Vec<libc::pid_t> = vec![0; 1024];
    let size = std::mem::size_of_val(pids.as_slice()) as libc::c_int;
    // SAFETY: the buffer is `size` bytes of pids, and the call writes at
    // most that.
    let bytes =
        unsafe { libc::proc_listpids(PROC_TTY_ONLY, tty as u32, pids.as_mut_ptr().cast(), size) };
    if bytes <= 0 {
        return Vec::new();
    }
    let count = bytes as usize / std::mem::size_of::<libc::pid_t>();
    let mut groups: Vec<libc::pid_t> = pids[..count.min(pids.len())]
        .iter()
        .copied()
        .filter(|pid| *pid > 0 && Some(*pid) != leader)
        // SAFETY: getpgid only asks.
        .map(|pid| unsafe { libc::getpgid(pid) })
        .filter(|group| *group > 0)
        .collect();
    groups.sort_unstable();
    groups.dedup();
    groups
}

/// Whether a child has ended, asked without reaping it: a zombie keeps
/// its pid, and its session's, from being given to anybody else.
fn ended_unreaped(pid: libc::pid_t) -> bool {
    // SAFETY: `info` is written by `waitid`, which only looks at our own
    // child, and WNOWAIT leaves it to be reaped later.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        ) == 0
            && info.si_pid != 0
    }
}

/// Wait on the reader thread after EOF without reaping the owned leader.
/// Closing a tab still owns signalling/reaping; this observer only publishes
/// final completion. WNOWAIT also keeps the process identity from being reused.
fn wait_until_exit(pid: u32, stop: &crate::transport::Stop) {
    loop {
        if stop.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            )
        };
        // ECHILD means the terminal/job owner has already reaped it.
        if (result == 0 && unsafe { info.si_pid() } != 0)
            || (result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// The grid, shared with the reader thread.
pub type SharedTerm = Arc<Mutex<Term<Listener>>>;

/// A live terminal: the process, the grid and the channels to them.
pub struct Emulator {
    term: SharedTerm,
    shell_state: Arc<Mutex<super::shell_state::ShellState>>,
    search_revision: Arc<std::sync::atomic::AtomicU64>,
    #[cfg(feature = "native-preview")]
    pub(crate) frame_generation: Arc<crate::native_preview::frame_meter::Generation>,
    /// The parser, shared with the reader so the tab's tick can end a
    /// synchronized frame that never ended — M5.14. **Locked before the
    /// term, everywhere**: the reader holds both while it parses.
    parser: Arc<Mutex<super::semantic::ShellProcessor>>,
    master: RefCell<Option<Box<dyn MasterPty + Send>>>,
    /// Input for the program, written on a thread of its own — M5.14.
    writer: RefCell<Option<Arc<crate::transport::Queue<Vec<u8>>>>>,
    io_workers: RefCell<Option<crate::transport::Workers>>,
    input_failed: Arc<std::sync::atomic::AtomicBool>,
    io_stop: Arc<crate::transport::Stop>,
    input_refusal: Option<&'static str>,
    teardown: Teardown,
    /// The session this terminal runs, when it runs one: counted in
    /// `terminal::retiring` from its hang-up until its child is reaped.
    retire_tag: Option<String>,
    /// `None` once the tab has closed and handed it to the reaper.
    child: Option<Box<dyn Child + Send + Sync>>,
    job: Option<job::Job>,
    // Harness leaders stay unreaped until read-only quiescence or explicit cleanup.
    retain_exit: bool,
    observed_exit: Option<ExitStatus>,
    launch_material: Option<crate::discovery::Material>,
    shell_material: Option<tempfile::TempDir>,
    /// The pty's device, which the processes on it name as their
    /// terminal — what a hangup finds a shell's jobs by.
    tty: Option<libc::dev_t>,
    events: Receiver,
    size: GridSize,
}

/// What a program in a tab is told about the terminal it is in.
const IDENTITY: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// What describes another terminal — the one the window was started from —
/// and is not handed on. Guild names no program of its own here, so a
/// program keyed on a terminal's name treats it as a plain one.
const NOT_OURS: [&str; 2] = ["TERM_PROGRAM", "TERM_PROGRAM_VERSION"];

impl Emulator {
    /// Run a retained terminal job with independently supervised completion.
    pub fn spawn_job(
        command: CommandBuilder,
        cwd: &Path,
        size: GridSize,
    ) -> Result<Self, TerminalError> {
        Self::spawn_inner(command, cwd, size, true)
    }

    /// Run a command in a pseudo-terminal.
    pub fn spawn(
        command: CommandBuilder,
        cwd: &Path,
        size: GridSize,
    ) -> Result<Self, TerminalError> {
        Self::spawn_inner(command, cwd, size, false)
    }

    /// Apply a terminal harness preference after all PTY context is prepared.
    pub(super) fn spawn_harness(
        command: CommandBuilder,
        cwd: &Path,
        size: GridSize,
        prefix: &str,
        network: &guild_session::network::Environment,
    ) -> Result<Self, TerminalError> {
        let mut terminal = Self::spawn_prepared(
            command,
            cwd,
            size,
            false,
            Some((prefix, network)),
            prepare_descriptors,
        )?;
        terminal.retain_exit = true;
        Ok(terminal)
    }

    fn spawn_inner(
        command: CommandBuilder,
        cwd: &Path,
        size: GridSize,
        supervised: bool,
    ) -> Result<Self, TerminalError> {
        Self::spawn_prepared(command, cwd, size, supervised, None, prepare_descriptors)
    }

    fn spawn_prepared(
        command: CommandBuilder,
        cwd: &Path,
        size: GridSize,
        supervised: bool,
        prefix: Option<(&str, &guild_session::network::Environment)>,
        prepare: impl FnOnce(&dyn MasterPty) -> Result<PreparedPty, TerminalError>,
    ) -> Result<Self, TerminalError> {
        let program = command.get_argv().first().map_or_else(
            || "command".to_owned(),
            |arg| arg.to_string_lossy().into_owned(),
        );

        let pty = NativePtySystem::default()
            .openpty(pty_size(size))
            .map_err(TerminalError::Pty)?;

        let mut command = command;
        command.cwd(cwd);

        // Markers of another session are removed: Guild starts its own.
        // If Guild itself was started from a Claude Code session — and
        // agents work on this very code, so that happens often — the child
        // `claude` takes them for its own, disables transcript writing and
        // warns about it across the whole screen. Our "a conversation
        // happened" signal rests on that transcript.
        for marker in guild_session::markers_to_drop() {
            command.env_remove(marker);
        }

        // **The tab says what terminal it is**, as every terminal does for
        // what it starts, and nothing the window inherited says it instead.
        // Without it a window started from Finder — or from a bare
        // environment — has no `TERM` at all, and `claude` drew in
        // monochrome; and one started from another terminal handed on that
        // terminal's name. `xterm-256color` is what `alacritty_terminal`
        // speaks and every machine has a terminfo entry for; `truecolor` is
        // what it draws.
        for (name, value) in IDENTITY {
            command.env(name, value);
        }
        for name in NOT_OURS {
            command.env_remove(name);
        }
        // A locale when the window had none — M5.14.
        // `defaults` is asked only when there is no `LANG` to keep.
        let inherited = command.get_env("LANG").filter(|value| !value.is_empty());
        if inherited.is_none()
            && let Some(lang) = lang(None, apple_locale(), locale_exists)
        {
            command.env("LANG", lang);
        }
        // **The login shell's `PATH`, unless the caller gave one.** A
        // `Guild.app` opened from Finder inherits launchd's, where `claude`
        // is not; every pty goes through this door. Decision 22 of M5.11.
        if !caller_set_path(&command, std::env::var_os("PATH").as_deref())
            && let Some(path) = guild_session::login_path::path()
        {
            command.env("PATH", path);
        }

        if let Some((prefix, network)) = prefix {
            let shell =
                super::shell::program(command.get_env("SHELL").and_then(OsStr::to_str)).to_owned();
            command = super::launch_prefix::wrap_policy(command, prefix, cwd, &shell, network)
                .map_err(TerminalError::Pty)?;
        }

        // Fallible descriptor preparation must not leave an ordinary child
        // alive when construction returns an error. Local PTY owners drop here.
        let (pty_writer, mut reader) = prepare(pty.master.as_ref())?;
        let starting_child: Arc<Mutex<Option<Box<dyn Child + Send + Sync>>>> =
            Arc::new(Mutex::new(None));
        let failed_start = starting_child.clone();
        let teardown: Teardown = Arc::new(Mutex::new(None));
        let closing = teardown.clone();
        let mut io_workers = crate::transport::Workers::new(move || {
            if let Some(close) = closing.lock().unwrap().take() {
                close();
            }
            if let Some(mut child) = failed_start.lock().unwrap().take() {
                // This ordinary leader is still unreaped: only its original group.
                if let Some(pid) = child.process_id() {
                    unsafe {
                        libc::killpg(pid as i32, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
            }
        })
        .map_err(TerminalError::Supervise)?;
        // Readiness alias is independently owned and CLOEXEC. O_NONBLOCK is
        // shared with all master aliases; reader WouldBlock is not EOF.
        let readiness_file = pty_writer
            .try_clone()
            .map_err(|e| TerminalError::Pty(e.into()))?;
        let readiness =
            crate::transport::Interruptible::new(readiness_file, io_workers.stop.clone())
                .map_err(|e| TerminalError::Pty(e.into()))?;
        let mut pty_writer =
            crate::transport::Interruptible::new(pty_writer, io_workers.stop.clone())
                .map_err(|e| TerminalError::Pty(e.into()))?;

        // portable-pty otherwise silently falls back to HOME for a missing cwd.
        std::fs::read_dir(cwd).map_err(|error| {
            TerminalError::Pty(anyhow::anyhow!(
                "working directory {} is unavailable: {error}",
                cwd.display()
            ))
        })?;

        let job_anchor = if supervised {
            Some(pty_writer_anchor(pty.master.as_ref())?)
        } else {
            None
        };
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|source| TerminalError::Spawn {
                command: program,
                source,
            })?;

        let exit_pid = child.process_id();
        let (sender, events) = channel();
        // Install the owner before later descriptor/thread setup can fail.
        let (child, job) = if supervised {
            // The supervisor retains a master alias until group cleanup/reap;
            // stopping local IO must not synthesize an early EOF for the child.
            let anchor = job_anchor.expect("prepared supervisor anchor");
            let (job, supervisor) = job::Job::start(child, sender.signal(), anchor)
                .map_err(TerminalError::Supervise)?;
            io_workers.handles.push(supervisor);
            (None, Some(job))
        } else {
            (Some(child), None)
        };

        *starting_child.lock().unwrap() = child;

        let tty = pty.master.tty_name().and_then(|name| {
            use std::os::unix::fs::MetadataExt as _;
            std::fs::metadata(name)
                .ok()
                .map(|meta| meta.rdev() as libc::dev_t)
        });

        // `slave` is closed at once: while it is open in our process the
        // program will not see EOF when it exits, and reading from the
        // master would never end.
        drop(pty.slave);

        // **Input goes out on a thread of its own** — M5.14. A pty write
        // blocks while the program has the terminal raw and is not
        // reading; on the window's thread that hung every tab, with no
        // way left to send the `ctrl-c` that would free it. The thread
        // ends when the emulator is dropped or the pty refuses a write.
        let writer = crate::transport::Queue::<Vec<u8>>::new(256, INPUT_BYTES, &io_workers.stop);
        let input = writer.clone();
        let input_failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failed = input_failed.clone();
        let write_events = sender.clone();
        let write_stop = io_workers.stop.clone();
        io_workers.handles.push(std::thread::Builder::new().name("pty-input".into()).spawn(move || {
            while let Some(mut permit) = input.pop(true, &write_stop) {
                let bytes = permit.value.take().unwrap();
                if let Err(err) = pty_writer.write_all(&bytes) {
                    if !write_stop.load(std::sync::atomic::Ordering::Acquire) {
                        failed.store(true, std::sync::atomic::Ordering::Release);
                        let _ = write_events.send(TerminalEvent::Wakeup);
                        tracing::warn!(%err, "admitted terminal input delivery is uncertain");
                    }
                    input.close();
                    return;
                }
            }
        }).map_err(TerminalError::Supervise)?);

        // A blinking block is what every terminal people come from shows
        // by default. `Config::default()`
        // asks for a steady one, and a cursor that does not blink reads as
        // a picture rather than as the place typing will land. A program
        // can still ask for something else with DECSCUSR, and that wins:
        // this is only what applies until it does.
        //
        // **OSC 52 copies and never pastes** — M5.12's decision 8. A
        // program that writes the clipboard is doing what a person asked of
        // it (`claude`'s copy, `tmux`'s yank); a program that could read the
        // clipboard silently is a leak of whatever was copied last. It is
        // `alacritty_terminal`'s default, spelled out so it is a decision
        // and not an accident of one.
        let config = Config {
            default_cursor_style: CursorStyle {
                shape: CursorShape::Block,
                blinking: true,
            },
            osc52: Osc52::OnlyCopy,
            ..Config::default()
        };
        let term = Arc::new(Mutex::new(Term::new(
            config,
            &size,
            Listener(sender.clone()),
        )));

        // Reading happens in a separate thread: it blocks, and the GPUI
        // executor is single-threaded. Parsing happens there too, so the
        // window receives a ready grid rather than raw bytes.
        #[cfg(feature = "native-preview")]
        let frame_generation = Arc::new(crate::native_preview::frame_meter::Generation::default());
        #[cfg(feature = "native-preview")]
        let reader_generation = frame_generation.clone();
        let search_revision = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let reader_revision = search_revision.clone();
        let reader_term = Arc::clone(&term);
        let shell_state = Arc::new(Mutex::new(super::shell_state::ShellState::new(
            cwd,
            crate::system::local_hostname().unwrap_or_default(),
        )));
        let reader_shell = shell_state.clone();
        let parser: Arc<Mutex<super::semantic::ShellProcessor>> = Arc::new(Mutex::new(
            super::semantic::ShellProcessor::new(guild_session::SessionId::new().to_string()),
        ));
        let reader_parser = Arc::clone(&parser);
        #[cfg(test)]
        if FAIL_PARTIAL_INPUT.with(|f| f.replace(false)) {
            PARTIAL_INPUT_OWNER.with(|o| {
                *o.borrow_mut() = Some((
                    exit_pid.unwrap(),
                    io_workers.finished.clone(),
                    io_workers.handles.len(),
                ))
            });
            return Err(TerminalError::Supervise(std::io::Error::other(
                "owned fixture: output thread creation failed",
            )));
        }
        let read_stop = io_workers.stop.clone();
        let io_stop = io_workers.stop.clone();
        io_workers.handles.push(
            std::thread::Builder::new()
                .name("pty-output".into())
                .spawn(move || {
                    // The timeout type is spelled out: `Processor` is generic
                    // over it and inference does not converge without it.
                    // `StdSyncHandler` is the standard one, the same alacritty
                    // uses.
                    // XTVERSION is answered here, because vte has no handler for
                    // it and `claude` asks for synchronized output only once it
                    // is answered — M5.14.
                    let mut version = super::xtversion::Scanner::default();
                    // The OSC 7501 probe too, and for a stricter reason: the
                    // program sends `CSI c` after it and takes a DA1 reply with
                    // no 7501 reply before it as "no" — alacritty answers the
                    // DA1 while parsing, so this reply is queued first.
                    let mut status_probe = super::program_status::ProbeScanner::default();
                    let mut buffer = [0u8; 8192];

                    loop {
                        if read_stop.load(std::sync::atomic::Ordering::Acquire)
                            || !sender.wait_capacity()
                        {
                            break;
                        }
                        match reader.read(&mut buffer) {
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock
                                        | std::io::ErrorKind::Interrupted
                                ) =>
                            {
                                if readiness.wait_readable().is_err() {
                                    break;
                                }
                                continue;
                            }
                            Ok(0) | Err(_) => break,
                            Ok(count) => {
                                sender.begin_batch();
                                for _ in 0..version.feed(&buffer[..count]) {
                                    let answer =
                                        super::xtversion::answer(env!("CARGO_PKG_VERSION"));
                                    let _ = sender.send(TerminalEvent::Reply(Reply::Text(answer)));
                                }
                                for terminator in status_probe.feed(&buffer[..count]) {
                                    let answer = super::program_status::probe_answer(terminator);
                                    let _ = sender.send(TerminalEvent::Reply(Reply::Text(answer)));
                                }
                                // Parser, then term: the order `flush_stale_sync`
                                // takes them in too.
                                let buffering = match reader_parser.lock() {
                                    Ok(mut parser) => {
                                        #[cfg(feature = "native-preview")]
                                        let waited = std::time::Instant::now();
                                        if let Ok(mut term) = reader_term.lock() {
                                            #[cfg(feature = "native-preview")]
                                            crate::native_preview::resource_probe::grid_waited(
                                                waited.elapsed(),
                                            );
                                            let mut state = reader_shell.lock().unwrap();
                                            parser.advance(
                                                &mut *term,
                                                &buffer[..count],
                                                |term, event| state.observe(term, event),
                                            );
                                            reader_revision
                                                .fetch_add(1, std::sync::atomic::Ordering::Release);
                                            #[cfg(feature = "native-preview")]
                                            reader_generation.advance();
                                            // If every byte is still buffered, the visible grid did not advance.
                                            #[cfg(feature = "adaptive-terminal-refresh")]
                                            if parser.visible_advanced()
                                                && let Some(d) = sender.demand()
                                            {
                                                d.advance();
                                            }
                                        }
                                        parser.sync_timeout().sync_timeout().is_some()
                                    }
                                    Err(_) => false,
                                };
                                // Publish once, after parser/grid locks are released. A
                                // buffered frame still wakes the consumer to arm its timeout.
                                if !buffering {
                                    let _ = sender.send(TerminalEvent::Wakeup);
                                }
                                #[cfg(not(feature = "adaptive-terminal-refresh"))]
                                sender.publish_batch();
                                #[cfg(feature = "adaptive-terminal-refresh")]
                                {
                                    if sender.demand_enabled() {
                                        let deadline = reader_parser
                                            .lock()
                                            .ok()
                                            .and_then(|p| p.sync_timeout().sync_timeout());
                                        sender.publish_parsed(deadline);
                                    } else {
                                        sender.publish_batch();
                                    }
                                }
                                #[cfg(feature = "native-preview")]
                                crate::native_preview::resource_probe::parsed_bytes(count);
                            }
                        }
                    }

                    // EOF is not process completion. Observe it here without reaping,
                    // scanning output, blocking the UI or arming an idle polling timer.
                    if let Some(pid) = exit_pid {
                        wait_until_exit(pid, &read_stop);
                    }
                    // OSC 7501: nothing an exited program reported is still
                    // working or blocked. After the last byte, before the event —
                    // and after the last frame: a synchronized frame the program
                    // left open is replayed here first (parser, then term, then
                    // the shell state, `flush_sync`'s order), or the window's own
                    // flush on `Exited` would bring a buffered `working` back.
                    // The parser stays held until the records are cleared, so a
                    // flush from the window cannot slip in between.
                    let mut parser = reader_parser.lock().ok();
                    if let Some(parser) = parser.as_mut()
                        && parser.sync_timeout().sync_timeout().is_some()
                        && let Ok(mut term) = reader_term.lock()
                    {
                        parser.stop_sync(&mut *term, |term, event| {
                            if let Ok(mut state) = reader_shell.lock() {
                                state.observe(term, event);
                            }
                        });
                        reader_revision.fetch_add(1, std::sync::atomic::Ordering::Release);
                    }
                    if let Ok(mut state) = reader_shell.lock() {
                        state.process_exited();
                    }
                    drop(parser);
                    let _ = sender.send(TerminalEvent::Exited);
                })
                .map_err(TerminalError::Supervise)?,
        );

        Ok(Self {
            #[cfg(feature = "native-preview")]
            frame_generation,
            term,
            shell_state,
            search_revision,
            parser,
            master: RefCell::new(Some(pty.master)),
            writer: RefCell::new(Some(writer)),
            io_workers: RefCell::new(Some(io_workers)),
            input_failed,
            io_stop,
            input_refusal: None,
            teardown,
            retire_tag: None,
            child: starting_child.lock().unwrap().take(),
            job,
            retain_exit: false,
            observed_exit: None,
            launch_material: None,
            shell_material: None,
            tty,
            events,
            size,
        })
    }

    pub(crate) fn hold_shell_material(&mut self, material: Option<tempfile::TempDir>) {
        self.shell_material = material;
    }

    pub(crate) fn hold_launch_material(&mut self, material: crate::discovery::Material) {
        self.launch_material = Some(material);
    }

    /// Stop the process, keeping the grid.
    ///
    /// **Not `drop`.** Dropping the emulator takes the output with it, and
    /// the output is what a person pressed Stop while reading. The pty is
    /// closed by the child's death, the reader thread ends on `read`
    /// returning zero, and what has already been parsed stays where it
    /// is. Killing something already dead is not an error worth a notice:
    /// Stop can be pressed twice, and a run can finish between the press
    /// and the handler.
    pub fn kill(&mut self) {
        if let Some(job) = &self.job {
            job.cancel();
            return;
        }
        let Some(child) = self.child.as_mut() else {
            return;
        };
        // **The group, not the child.** `portable_pty` starts the child
        // as a session leader, so its pid is its process group's id — and
        // a run is `zsh -lc "just check"`, which is `just`, which is
        // `sh`, which is `cargo`. Killing the leader alone leaves the
        // rest of that chain alive: after `cmd-q` the window was gone and
        // `just slow` was still counting, which is how this was found.
        //
        // SIGKILL rather than the SIGHUP a real hangup sends: a run has
        // nothing to flush — its output is already in the grid — and a
        // person who pressed Stop meant now. `child.kill()` after it
        // covers whatever a group signal did not reach.
        //
        // **Only while it is running.** A reaped pid can be given to
        // somebody else's process, and killing a stranger's group would
        // be the worst bug in this file. Nothing but this reaps our
        // child, so `try_wait` answering `None` means the pid is still
        // ours — a zombie's pid is not reused either.
        let running = matches!(child.try_wait(), Ok(None));
        if running && let Some(pid) = child.process_id() {
            // SAFETY: `pid` is our own unreaped child, and it leads the
            // group `portable_pty` made for it.
            unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        }
        // `kill` signals the pid whether or not it has been reaped — and
        // `try_wait` above reaps one that had ended — so only while it had
        // not. M5.14 review.
        if running && let Err(err) = child.kill() {
            tracing::debug!(%err, "the process was already gone");
        }
        if let Err(err) = child.wait() {
            tracing::warn!(%err, "could not wait for the process");
        }
    }

    /// Hang the terminal up, as a terminal closing does: SIGHUP to the
    /// leader's group and to the group in front, and a shell passes it to
    /// its jobs. Answers the two groups, or `None` when the child is not
    /// running — **only while it is ours**: a reaped pid can be a
    /// stranger's, the rule `kill` states. M5.14.
    fn hang_up(&mut self) -> Option<Hangup> {
        let child = self.child.as_mut()?;
        if !matches!(child.try_wait(), Ok(None)) {
            return None;
        }
        let leader = child.process_id().map(|pid| pid as libc::pid_t);
        // The leader's group, the one in front, and **every group with a
        // process on this terminal** — a shell's background jobs among
        // them. Waiting for the shell to pass the hangup on lost the race
        // under load, and the job outlived the tab. M5.14 review.
        let mut groups: Vec<libc::pid_t> = leader
            .into_iter()
            .chain(self.foreground_process())
            .chain(
                self.tty
                    .map(|tty| groups_on(tty, leader))
                    .unwrap_or_default(),
            )
            .collect();
        groups.sort_unstable();
        groups.dedup();
        let ownership = super::ownership::Cleanup::retain(leader, &groups);
        for group in &groups {
            // SAFETY: groups of our own unreaped child's session, or of
            // processes on our own pty.
            unsafe { libc::killpg(*group, libc::SIGHUP) };
        }
        Some(Hangup {
            leader,
            groups,
            ownership,
        })
    }

    /// Tag this terminal with the session it runs, so that its process is
    /// counted as going from the hang-up until it is reaped.
    pub fn set_retire_tag(&mut self, tag: String) {
        self.retire_tag = Some(tag);
    }

    /// Request quit cleanup on the pre-owned reaper with a shorter grace.
    /// Admission and local IO stop immediately; application quit awaits the
    /// owner completion off-thread, including supervisor joins and group reap.
    pub fn hang_up_for_quit(&mut self) {
        if let Some(job) = &mut self.job {
            job.shutdown();
            self.master.borrow_mut().take();
            self.stop_io();
            return;
        }
        let _ = self.let_go_with_grace(QUIT_GRACE);
    }

    /// Revoke admission immediately; the pre-owned reaper keeps the original
    /// groups/leader reserved through hangup, bounded drain, kill and reap.
    pub fn let_go(&mut self) -> Option<std::sync::mpsc::Receiver<()>> {
        self.let_go_with_grace(HANG_UP_GRACE)
    }
    fn let_go_with_grace(
        &mut self,
        grace: std::time::Duration,
    ) -> Option<std::sync::mpsc::Receiver<()>> {
        let Some(hangup) = self.hang_up() else {
            self.stop_io();
            return None;
        };
        let mut child = self.child.take()?;
        // From here until the child is reaped the session's conversation is
        // still held by a process: `terminal::retiring`.
        let tag = self.retire_tag.clone();
        if let Some(tag) = &tag {
            super::retiring::begin(tag);
        }
        let material = self.launch_material.take();
        let shell_material = self.shell_material.take();
        let master_anchor = self.master.borrow_mut().take();
        let mut discard = master_anchor
            .as_ref()
            .and_then(|m| pty_writer_anchor(m.as_ref()).ok());
        let (done, over) = std::sync::mpsc::channel();
        *self.teardown.lock().unwrap() = Some(Box::new(move || {
            let deadline = std::time::Instant::now() + grace;
            // Darwin can wait for tty output to drain during leader exit. The
            // UI reader is already stopped: drain bounded chunks here without
            // parsing/painting. Never wait for EOF or expand group authority.
            while std::time::Instant::now() < deadline && !hangup.gone() && discard.is_some() {
                if let Some(file) = &mut discard {
                    let mut buffer = [0u8; 8192];
                    for _ in 0..8 {
                        if !matches!(std::io::Read::read(file, &mut buffer),Ok(n) if n>0) {
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            hangup.kill_survivors();
            // Keep the tty alive until signals are sent, then close it before
            // wait: a dying Darwin leader can itself wait on the master.
            drop(discard);
            drop(master_anchor);
            let settle = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while std::time::Instant::now() < settle && !hangup.gone() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            hangup.ownership.finish();
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
            drop(hangup);
            drop(material);
            drop(shell_material);
            if let Some(tag) = &tag {
                super::retiring::end(tag);
            }
            let _ = done.send(());
        }));
        self.stop_io();
        Some(over)
    }

    /// The child's pid, as the platform gave it.
    pub fn process_id(&self) -> Option<u32> {
        if let Some(job) = &self.job {
            return job.pid();
        }
        self.child.as_ref()?.process_id()
    }

    /// What the process exited with, if it has ended.
    ///
    /// `try_wait` rather than `wait`: this is asked on the window's clock
    /// and must never block it. It touches the child, not the grid, so it
    /// cannot deadlock the frame the way an accessor that takes the
    /// grid's lock would.
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        if self.job.is_some() {
            return self.job_completion().and_then(Result::ok);
        }
        if self.retain_exit {
            if self.observed_exit.is_none() {
                let pid = self.child.as_ref()?.process_id()?;
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                // Observation never releases the leader/session identity.
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result == 0 && info.si_pid != 0 {
                    self.observed_exit = Some(if info.si_code == libc::CLD_EXITED {
                        ExitStatus::with_exit_code(info.si_status as u32)
                    } else {
                        use std::os::unix::process::ExitStatusExt;
                        ExitStatus::from(std::process::ExitStatus::from_raw(info.si_status))
                    });
                }
            }
            return self.observed_exit.clone();
        }
        match self.child.as_mut()?.try_wait() {
            Ok(status) => status,
            Err(err) => {
                tracing::warn!(%err, "could not ask the process how it ended");
                None
            }
        }
    }

    /// Retained owner token for off-thread read-only session inspection.
    pub(crate) fn recovery_leader(&self) -> Option<u32> {
        (self.retain_exit && self.observed_exit.is_some())
            .then(|| self.process_id())
            .flatten()
    }
    /// Only a quiescence witness for this still-unreaped owner may release it.
    pub(crate) fn release_quiescent(&mut self, leader: u32) -> Result<(), String> {
        if self.recovery_leader() != Some(leader) || !ended_unreaped(leader as libc::pid_t) {
            return Err("ownership_unqualified: retained PTY leader changed".into());
        }
        // The caller has proved no live session members while this leader was
        // reserved. With nobody left able to fork/join, this fact stays true.
        let mut child = self.child.take().ok_or("Retained owner disappeared")?;
        child.wait().map_err(|e| e.to_string())?;
        if let Some(q) = self.writer.borrow_mut().take() {
            q.close();
        }
        self.master.borrow_mut().take();
        self.launch_material.take();
        self.shell_material.take();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn hold_supervised_publication(
        &mut self,
    ) -> Arc<Mutex<Option<Result<ExitStatus, String>>>> {
        self.job.as_mut().unwrap().hold_publication()
    }

    #[cfg(test)]
    pub(super) fn transport_finished_for_test(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.io_workers.borrow().as_ref().unwrap().finished.clone()
    }

    /// Supervised completion, including a cleanup failure, stable across polls.
    pub(crate) fn job_completion_handle(&self) -> Option<JobCompletion> {
        self.job.as_ref().map(job::Job::completion_handle)
    }
    pub fn job_completion(&self) -> Option<Result<ExitStatus, String>> {
        let completion = self.job.as_ref().and_then(job::Job::completion);
        if completion.as_ref().is_some_and(Result::is_ok) {
            // The supervisor has finished process-group cleanup. The reader owns
            // its own descriptor and drains trailing output independently; keep
            // the parsed grid, but no live input/master resources for saved screens.
            if let Some(q) = self.writer.borrow_mut().take() {
                q.close();
            }
            self.master.borrow_mut().take();
            if let Some(workers) = self.io_workers.borrow_mut().take() {
                // Joining is off-thread and does not interrupt the reader's tail.
                // Close/quit can still revoke escaped-pipe waits via io_stop.
                workers.drain();
            }
        }
        completion
    }

    /// The grid, for reading while painting.
    pub(crate) fn live_cwd(&self) -> Option<std::path::PathBuf> {
        self.shell_state.lock().ok()?.cwd().map(Path::to_path_buf)
    }
    pub(crate) fn output_cwd(&self, from: Point, to: Point) -> Option<std::path::PathBuf> {
        self.shell_state
            .lock()
            .ok()?
            .cwd_at(from, to)
            .map(Path::to_path_buf)
    }
    pub(crate) fn command_context(&self, point: Option<Point>) -> (bool, bool, bool, Option<i32>) {
        let Ok(term) = self.term.lock() else {
            return Default::default();
        };
        let Ok(state) = self.shell_state.lock() else {
            return Default::default();
        };
        let commands = state.commands();
        let offset = term.grid().display_offset();
        let top = Point::new(alacritty_terminal::index::Line(-(offset as i32)), Column(0));
        let anchor =
            if let Some((point, _)) = state.navigation.filter(|(_, saved)| *saved == offset) {
                point
            } else if offset == 0 {
                commands.last().map_or(top, |c| c.prompt)
            } else {
                top
            };
        let at = point.unwrap_or(anchor);
        let command = commands.iter().rev().find(|c| c.prompt <= at);
        (
            commands.iter().any(|c| c.prompt < anchor),
            commands.iter().any(|c| c.prompt > anchor),
            command.is_some_and(|c| c.output.zip(c.end).is_some_and(|(start, end)| end > start)),
            command.and_then(|c| c.status),
        )
    }
    pub(crate) fn navigate_command(&self, older: bool) -> bool {
        let Ok(mut term) = self.term.lock() else {
            return false;
        };
        let Ok(mut state) = self.shell_state.lock() else {
            return false;
        };
        let commands = state.commands();
        if commands.is_empty() || term.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        let offset = term.grid().display_offset();
        let top = Point::new(alacritty_terminal::index::Line(-(offset as i32)), Column(0));
        let anchor = if let Some((point, saved_offset)) = state
            .navigation
            .filter(|(_, saved_offset)| *saved_offset == offset)
        {
            let _ = saved_offset;
            point
        } else if offset == 0 {
            commands.last().unwrap().prompt
        } else {
            top
        };
        let target = if older {
            commands.iter().rev().find(|c| c.prompt < anchor)
        } else {
            commands.iter().find(|c| c.prompt > anchor)
        };
        if let Some(target) = target.cloned() {
            term.scroll_display(Scroll::Delta(top.line.0 - target.prompt.line.0));
            state.navigation = Some((target.prompt, term.grid().display_offset()));
            if let Some(end) = target.output {
                let mut selection =
                    Selection::new(SelectionType::Simple, target.prompt, Side::Left);
                selection.update(end, Side::Left);
                term.selection = Some(selection);
            }
        }
        #[cfg(feature = "adaptive-terminal-refresh")]
        self.grid_changed();
        true
    }
    pub(crate) fn select_command_output(&self, point: Option<Point>) -> bool {
        let Ok(mut term) = self.term.lock() else {
            return false;
        };
        let Ok(state) = self.shell_state.lock() else {
            return false;
        };
        let commands = state.commands();
        let at = point
            .or(state.navigation.map(|(point, _)| point))
            .unwrap_or(term.grid().cursor.point);
        let Some(command) = commands.iter().rev().find(|c| c.prompt <= at) else {
            return false;
        };
        let (Some(start), Some(end)) = (command.output, command.end) else {
            return false;
        };
        if end <= start {
            return false;
        }
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Left);
        term.selection = Some(selection);
        #[cfg(feature = "adaptive-terminal-refresh")]
        self.grid_changed();
        true
    }
    /// The root OSC 7501 record's changes since the last take, in order,
    /// and how many a full queue dropped meanwhile.
    pub fn take_status_changes(&self) -> (Vec<super::program_status::RootChange>, u64) {
        self.shell_state
            .lock()
            .map(|mut state| state.take_status_changes())
            .unwrap_or_default()
    }
    /// The session's own OSC 7501 record, if the program reported one.
    pub fn status_root(&self) -> Option<super::program_status::Report> {
        self.shell_state
            .lock()
            .ok()?
            .status_records()
            .root()
            .cloned()
    }
    #[cfg(test)]
    pub(crate) fn commands(&self) -> Vec<super::shell_state::Command> {
        self.shell_state
            .lock()
            .map(|s| s.commands())
            .unwrap_or_default()
    }
    pub(crate) fn search_revision(&self) -> Arc<std::sync::atomic::AtomicU64> {
        self.search_revision.clone()
    }

    pub fn term(&self) -> &SharedTerm {
        &self.term
    }

    /// At most 16 KiB from the last 64 parsed rows, only requested after failure.
    /// Extraction does not alter selection, scroll position or retained output.
    pub fn failure_tail(&self) -> String {
        let Ok(term) = self.term.try_lock() else {
            return String::new();
        };
        let grid = term.grid();
        // Blank viewport padding is not retained diagnostic text. Find the
        // last populated visible row first; never walk the scrollback to find
        // it. Discovery is bounded by the allocated viewport and 512 columns,
        // then extraction still visits at most 64 rows / keeps 16 KiB.
        let bottom = (0..grid.screen_lines() as i32)
            .rev()
            .find(|&line| {
                (0..grid.columns().min(512)).any(|column| {
                    let c =
                        grid[Point::new(alacritty_terminal::index::Line(line), Column(column))].c;
                    !c.is_whitespace() && !c.is_control()
                }) || grid[Point::new(
                    alacritty_terminal::index::Line(line),
                    Column(grid.columns() - 1),
                )]
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
            })
            .map_or(0, |line| line + 1);
        let top = (bottom - 64).max(grid.topmost_line().0);
        let mut rows = Vec::new();
        let mut bytes = 0;
        // Start at the end so a large retained screen cannot crowd the final
        // diagnostic out of the bounded tail.
        for line in (top..bottom).rev() {
            let mut row = String::new();
            for column in 0..grid.columns().min(512) {
                let c = grid[Point::new(alacritty_terminal::index::Line(line), Column(column))].c;
                if !c.is_control() {
                    row.push(c);
                }
            }
            let last = Point::new(
                alacritty_terminal::index::Line(line),
                Column(grid.columns() - 1),
            );
            if !grid[last]
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
            {
                // Only hard-ended rows have padding. A soft-wrapped row is
                // full, and its trailing spaces are real word separators.
                row.truncate(row.trim_end().len());
                row.push('\n');
            }
            let available = 16 * 1024 - bytes;
            if row.len() > available {
                let mut start = row.len() - available;
                while !row.is_char_boundary(start) {
                    start += 1;
                }
                row.drain(..start);
            }
            bytes += row.len();
            rows.push(row);
            if bytes >= 16 * 1024 {
                break;
            }
        }
        rows.reverse();
        rows.concat()
    }

    /// Displayable text in the retained screen or scrollback, without changing it.
    pub fn has_text(&self) -> bool {
        self.term.lock().is_ok_and(|term| {
            let grid = term.grid();
            let first = alacritty_terminal::index::Point::new(
                grid.topmost_line(),
                alacritty_terminal::index::Column(0),
            );
            let visible = |c: char| !c.is_whitespace() && !c.is_control();
            visible(grid[first].c) || grid.iter_from(first).any(|cell| visible(cell.c))
        })
    }

    /// The terminal modes — key encoding depends on them.
    pub fn mode(&self) -> TermMode {
        self.term
            .lock()
            .map(|term| *term.mode())
            .unwrap_or_else(|_| TermMode::default())
    }

    /// Scroll through the history.
    ///
    /// A positive number scrolls back, towards earlier output. The history
    /// is kept by `alacritty_terminal` itself; our part is to move the
    /// viewport and repaint.
    ///
    /// Programs that drive the screen themselves (`vim`, `less`) have no
    /// history: they draw in the alternate buffer. Scrolling does nothing
    /// there — and must not, otherwise the screen would move against the
    /// program's wishes.
    pub fn scroll(&mut self, lines: i32) {
        if let Ok(mut term) = self.term.lock() {
            term.scroll_display(Scroll::Delta(lines));
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// Move the viewport by a page or to an end of the history — M5.12
    /// task 5, the history keys. The same no-op in the alternate screen as
    /// [`Self::scroll`], though `scroll::history_move` never asks there.
    pub fn scroll_to(&mut self, to: Scroll) {
        if let Ok(mut term) = self.term.lock() {
            term.scroll_display(to);
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// The grid's size in cells, as last given to the grid and the pty —
    /// kept here, so reading it takes no lock.
    pub fn size(&self) -> GridSize {
        self.size
    }

    /// How far the viewport is shifted into the history.
    pub fn display_offset(&self) -> usize {
        self.term
            .lock()
            .map(|term| term.grid().display_offset())
            .unwrap_or(0)
    }

    /// Return to the latest output.
    ///
    /// Needed when typing: typing while looking at history is pointless —
    /// the answer appears where the person is not looking.
    pub fn scroll_to_bottom(&mut self) {
        if let Ok(mut term) = self.term.lock() {
            term.scroll_display(Scroll::Bottom);
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// Clear the screen and the history.
    ///
    /// Both: `Cmd+K` in familiar terminals removes everything, not only
    /// what is visible. Keeping the history would mean you "cleared" it
    /// yet scrolling still shows the old content.
    pub fn clear(&mut self) {
        // Clearing comes from the same trait `alacritty` interprets
        // escape sequences with: clearing the screen is the same thing as
        // a program sending the corresponding code.
        use alacritty_terminal::vte::ansi::Handler as _;

        if let Ok(mut term) = self.term.lock() {
            self.search_revision
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            if let Ok(mut state) = self.shell_state.lock() {
                state.invalidate();
            }
            term.clear_screen(alacritty_terminal::vte::ansi::ClearMode::All);
            term.grid_mut().clear_history();
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// The grid point under a window cell.
    ///
    /// The window knows a row and a column on screen, while the selection
    /// lives in buffer coordinates: scrolled history shifts one relative
    /// to the other.
    pub fn point_at(&self, line: usize, column: usize) -> Point {
        viewport_to_point(self.display_offset(), Point::new(line, Column(column)))
    }

    /// Begin a selection.
    pub fn begin_selection(&mut self, point: Point, side: Side) {
        if let Ok(mut term) = self.term.lock() {
            term.selection = Some(Selection::new(SelectionType::Simple, point, side));
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// Extend the selection to a point.
    pub fn extend_selection(&mut self, point: Point, side: Side) {
        if let Ok(mut term) = self.term.lock()
            && let Some(selection) = term.selection.as_mut()
        {
            selection.update(point, side);
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        if let Ok(mut term) = self.term.lock() {
            term.selection = None;
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
    }

    /// Is there anything to copy?
    pub fn selected_text(&self) -> Option<String> {
        let text = self.term.lock().ok()?.selection_to_string()?;
        (!text.is_empty()).then_some(text)
    }

    /// The selection bounds — for painting.
    pub fn selection_range(&self) -> Option<SelectionRange> {
        let term = self.term.lock().ok()?;
        term.selection.as_ref()?.to_range(&term)
    }

    /// Take the accumulated events without blocking.
    pub fn drain_events(&self) -> Vec<TerminalEvent> {
        self.events.try_iter().collect()
    }

    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(crate) fn set_adaptive_enabled(&self, enabled: bool) {
        if self.events.demand_enabled() == enabled {
            return;
        }
        let _grid = self.term.lock().unwrap();
        self.events.set_demand_enabled(enabled);
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(crate) fn visual_demand(&self) -> Option<Arc<super::refresh::Demand>> {
        self.events.demand()
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(crate) fn wake_current(&self) {
        self.events.wake_current();
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    /// Caller holds the grid guard; publication is separate from mutation.
    pub(super) fn grid_changed(&self) {
        if let Some(d) = self.visual_demand() {
            d.advance();
        }
    }

    pub(crate) fn has_pending_events(&self) -> bool {
        self.events.has_semantics()
    }

    pub(crate) fn next_event(&self) -> Option<TerminalEvent> {
        self.events.next()
    }

    /// Event/deadline wake signal; no UI thread or polling runtime required.
    pub(crate) fn event_signal(&self) -> Arc<tokio::sync::Notify> {
        self.events.signal()
    }

    pub(crate) fn sync_deadline(&self) -> Option<std::time::Instant> {
        self.parser.lock().ok()?.sync_timeout().sync_timeout()
    }

    /// The process the terminal is running in front — its foreground
    /// process group's leader — M12.3. A shell at its prompt is itself;
    /// `claude` typed at it is `claude`.
    pub fn foreground_process(&self) -> Option<i32> {
        self.master
            .borrow()
            .as_ref()
            .and_then(|master| master.process_group_leader())
    }

    /// Send bytes to the process — queued for the writer thread, so this
    /// never waits on the program.
    pub fn send(&mut self, bytes: &[u8]) -> bool {
        self.send_before(bytes, || {})
    }
    pub(crate) fn send_before(&mut self, bytes: &[u8], before: impl FnOnce()) -> bool {
        if bytes.len() > INPUT_BYTES {
            return self.refuse_oversized_input();
        }
        let admission = self
            .writer
            .borrow()
            .as_ref()
            .ok_or("terminal writer closed")
            .and_then(|q| q.try_push_before(bytes.to_vec(), bytes.len(), before));
        self.input_refusal = admission.as_ref().err().map(|_|"Terminal input writer is closed or its 256-operation/8-MiB capacity is full; nothing was queued");
        let accepted = admission.is_ok();
        if !accepted {
            tracing::warn!(
                "terminal input refused: writer closed or 256-operation/8-MiB capacity reached"
            );
        }
        accepted
    }
    pub(crate) fn refuse_oversized_input(&mut self) -> bool {
        self.input_refusal = Some("Terminal input operation exceeds 8 MiB; nothing was queued");
        tracing::warn!("terminal input operation exceeds 8 MiB limit");
        false
    }
    pub(crate) fn input_refusal(&self) -> Option<&'static str> {
        self.input_refusal
    }
    pub(crate) fn input_delivery_failed(&self) -> bool {
        self.input_failed.load(std::sync::atomic::Ordering::Acquire)
    }
    fn stop_io(&self) {
        self.io_stop
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(q) = self.writer.borrow_mut().take() {
            q.close();
        }
        self.events.close();
        self.io_workers.borrow_mut().take();
    }

    /// Let go of a synchronized frame whose program began it and never
    /// ended it, once its timeout has passed — what alacritty's own event
    /// loop does, and ours did not: a program killed mid-frame left the
    /// tab frozen on the frame before. Called on the tab's tick; answers
    /// whether a frame was let go, which is a repaint.
    pub fn flush_stale_sync(&self) -> bool {
        self.flush_sync(false)
    }

    /// Publish the reader's final frame before inspecting completed output.
    /// Only the reader completion event may bypass the live-frame timeout.
    pub(crate) fn flush_completed_sync(&self) -> bool {
        self.flush_sync(true)
    }

    fn flush_sync(&self, reader_completed: bool) -> bool {
        let Ok(mut parser) = self.parser.lock() else {
            return false;
        };
        let ready = parser
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|deadline| reader_completed || deadline <= std::time::Instant::now());
        if !ready {
            return false;
        }
        let Ok(mut term) = self.term.lock() else {
            return false;
        };
        parser.stop_sync(&mut *term, |term, event| {
            if let Ok(mut state) = self.shell_state.lock() {
                state.observe(term, event);
            }
        });
        self.search_revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        #[cfg(feature = "adaptive-terminal-refresh")]
        self.grid_changed();
        drop(term);
        drop(parser);
        #[cfg(feature = "adaptive-terminal-refresh")]
        if let Some(d) = self.visual_demand() {
            d.publish();
        }
        true
    }

    /// Change the grid size.
    ///
    /// **Both** sizes change: the grid inside and the pty outside. If only
    /// the grid is told, the program keeps drawing at the old width — the
    /// window expands while the content stays in the previous column. That
    /// is exactly what happened until the pty started receiving a
    /// `PtySize`.
    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;

        // **The grid first, and the pty under the same lock** — M5.14. The
        // pty's size is the program's SIGWINCH, and its redraw for the new
        // size could be parsed into the old grid in between: a diff
        // renderer like `claude`'s then keeps the rows that diverged.
        let mut term = self.term.lock().ok();
        if let Some(term) = term.as_mut() {
            self.search_revision
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            if let Ok(mut state) = self.shell_state.lock() {
                state.invalidate();
            }
            term.resize(size);
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.grid_changed();
        }
        if let Some(master) = self.master.borrow().as_ref()
            && let Err(err) = master.resize(pty_size(size))
        {
            tracing::warn!(%err, "the pty did not accept the new size");
        }
        drop(term);
    }
}

impl Drop for Emulator {
    /// The process lives exactly as long as the tab.
    ///
    /// Without this, closing a tab leaves an orphan: the same class of
    /// mistake as a zombie after `kill` without `wait`. To stop watching
    /// is not to stop.
    ///
    /// **A hangup, as a terminal closing gives, and then the kill** —
    /// M5.14. SIGKILL at once could pass nothing on: a shell's
    /// background job — `npm run dev &` — lived on holding its port, and
    /// `claude` never exited on its own terms. SIGHUP goes to the leader's
    /// group and to the group in front; a shell hangs up its jobs. What
    /// is still there a second later is killed, **on a thread**: the
    /// window never waits for a process to agree to go.
    fn drop(&mut self) {
        let _ = self.let_go();
    }
}

/// The `LANG` a child is given, when it has none — M5.14.
///
/// A Guild opened from the Dock inherits launchd's environment, which has
/// no `LANG`, and zsh then ran in the C locale: Cyrillic at the prompt
/// came out as escapes, and `ls` and `git log` as question marks. The
/// system's own locale (`AppleLocale`, a region override after `@` cut
/// off) when the machine has it as a UTF-8 locale, `en_US.UTF-8` when it
/// does not; `None` when one is already there.
fn lang(
    inherited: Option<&OsStr>,
    apple_locale: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    if inherited.is_some_and(|value| !value.is_empty()) {
        return None;
    }
    let own = apple_locale
        .map(|locale| locale.split('@').next().unwrap_or(locale))
        .map(|name| format!("{name}.UTF-8"))
        .filter(|name| exists(name));
    Some(own.unwrap_or_else(|| "en_US.UTF-8".to_owned()))
}

/// The system's locale, as `defaults` says it — asked once.
fn apple_locale() -> Option<&'static str> {
    static LOCALE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    LOCALE
        .get_or_init(|| {
            let out = std::process::Command::new("/usr/bin/defaults")
                .args(["read", "-g", "AppleLocale"])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
                .filter(|locale| !locale.is_empty())
        })
        .as_deref()
}

/// Whether the machine has a locale of this name.
fn locale_exists(name: &str) -> bool {
    Path::new("/usr/share/locale").join(name).is_dir()
}

/// Whether the caller gave the builder a `PATH` of its own.
///
/// `get_env("PATH")` alone cannot say: `CommandBuilder::new` copies the
/// whole inherited environment, so it is always there — and the builder's
/// own list of what was set skips any name or value that is not UTF-8. So
/// both are asked: named in that list, or different from what was
/// inherited, which a value the list cannot print still is. A removed
/// `PATH` differs too, and is the caller's to have removed.
fn caller_set_path(command: &CommandBuilder, inherited: Option<&OsStr>) -> bool {
    command
        .iter_extra_env_as_str()
        .any(|(name, _)| name == "PATH")
        || command.get_env("PATH") != inherited
}

fn pty_size(size: GridSize) -> PtySize {
    PtySize {
        rows: size.rows as u16,
        cols: size.cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// A process's command line, as `ps` prints it — M12.3, for what a
/// terminal runs in front. `ps` because it is the one program every Mac has
/// for it, and this is asked once per foreground process, not per frame.
pub fn command_line(pid: i32) -> Option<String> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_quiescence_rechecks_a_member_that_forks_then_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let script = r#"import os,sys,time,signal
from pathlib import Path
root=Path(sys.argv[1]);child=os.fork()
if child==0:
 signal.signal(signal.SIGHUP,signal.SIG_IGN);os.setpgid(0,0)
 for fd in [0,1,2]:os.close(fd)
 (root/'parent').write_text(str(os.getpid()))
 while not (root/'fork').exists():time.sleep(.001)
 if os.fork()==0:
  (root/'descendant').write_text(str(os.getpid()))
  while not (root/'finish').exists():time.sleep(.001)
  os._exit(0)
 os._exit(0)
while not (root/'parent').exists():time.sleep(.001)
os._exit(0)
"#;
        let mut command = CommandBuilder::new("/usr/bin/python3");
        command.args(["-c", script]);
        command.arg(dir.path());
        let mut terminal =
            Emulator::spawn_harness(command, dir.path(), GridSize::new(80, 24), "", &vec![])
                .unwrap();
        let leader = terminal.process_id().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while terminal.exit_status().is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let parent: i32 = std::fs::read_to_string(dir.path().join("parent"))
            .unwrap()
            .parse()
            .unwrap();
        let mut released = false;
        let result = super::super::ownership::quiescent_session_observed(leader, || {
            if released {
                return;
            }
            released = true;
            std::fs::write(dir.path().join("fork"), "").unwrap();
            while !dir.path().join("descendant").exists() || unsafe { libc::getsid(parent) } >= 0 {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });
        let descendant: i32 = std::fs::read_to_string(dir.path().join("descendant"))
            .unwrap()
            .parse()
            .unwrap();
        std::fs::write(dir.path().join("finish"), "").unwrap();
        while unsafe { libc::getsid(descendant) } >= 0 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            result.is_err(),
            "a stale PID snapshot admitted a live unseen descendant"
        );
        super::super::ownership::quiescent_session(leader).unwrap();
        terminal.release_quiescent(leader).unwrap();
    }

    #[test]
    fn recovery_quiescence_observes_live_descendants_without_stopping_them() {
        let dir = tempfile::tempdir().unwrap();
        let script = r#"import os,sys,time,signal
from pathlib import Path
root=Path(sys.argv[1]);child=os.fork()
if child==0:
 signal.signal(signal.SIGHUP,signal.SIG_IGN);os.setpgid(0,0)
 (root/'ready').write_text(str(os.getpid()))
 for fd in [0,1,2]:os.close(fd)
 while not (root/'release').exists():time.sleep(.01)
 os._exit(0)
while not (root/'ready').exists():time.sleep(.01)
os._exit(0)
"#;
        let mut command = CommandBuilder::new("/usr/bin/python3");
        command.args(["-c", script]);
        command.arg(dir.path());
        let mut terminal =
            Emulator::spawn_harness(command, dir.path(), GridSize::new(80, 24), "", &vec![])
                .unwrap();
        let leader = terminal.process_id().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while terminal.exit_status().is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let refused = super::super::ownership::quiescent_session(leader);
        std::fs::write(dir.path().join("release"), "").unwrap();
        assert!(
            refused.unwrap_err().contains("cleanup_pending"),
            "leader exit alone admitted recovery"
        );
        while super::super::ownership::quiescent_session(leader).is_err() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        terminal.release_quiescent(leader).unwrap();
        assert!(terminal.process_id().is_none());
    }

    #[test]
    fn recovery_harness_exit_observation_retains_the_owned_leader() {
        let dir = tempfile::tempdir().unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 7"]);
        let mut terminal =
            Emulator::spawn_harness(command, dir.path(), GridSize::new(80, 24), "", &vec![])
                .unwrap();
        let pid = terminal.process_id().unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let status = loop {
            if let Some(status) = terminal.exit_status() {
                break status;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(status.exit_code(), 7);
        assert!(
            ended_unreaped(pid as libc::pid_t),
            "recovery must retain the exited leader identity until quiescence is proved"
        );
    }

    #[test]
    fn launch_material_survives_until_the_closing_reaper_finishes() {
        let root = tempfile::tempdir().unwrap();
        let material = crate::discovery::Material::prepare(root.path()).unwrap();
        let path = material.path().to_owned();
        let ready = root.path().join("ready");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            &format!("trap '' HUP; touch '{}'; sleep 30", ready.display()),
        ]);
        let mut emulator = Emulator::spawn(command, root.path(), GridSize::new(80, 24)).unwrap();
        emulator.hold_launch_material(material);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !ready.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(ready.exists());
        let over = emulator.let_go().unwrap();
        drop(emulator);
        assert!(
            path.exists(),
            "the closing child still owns its instruction files"
        );
        over.recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert!(
            !path.exists(),
            "only the finished reaper releases launch files"
        );
    }

    #[cfg(feature = "adaptive-terminal-refresh")]
    #[test]
    fn runtime_preference_changes_during_output_preserve_protocol_eof_and_final_grid() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", r#"sleep .05; i=0; while [ $i -lt 300 ]; do printf '\033]2;toggle-%s\007' "$i"; printf x; i=$((i+1)); if [ $((i % 10)) -eq 0 ]; then sleep .003; fi; done; printf '\r\nFINAL-TOGGLE'; sleep .05"#]);
        let emulator = Emulator::spawn_job(command, Path::new("/"), GridSize::new(80, 24)).unwrap();
        assert!(emulator.visual_demand().is_none());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut titles = Vec::new();
        let mut exited = false;
        let mut round = 0_usize;
        while !exited {
            assert!(std::time::Instant::now() < until);
            emulator.set_adaptive_enabled(round.is_multiple_of(2));
            if let Some(demand) = emulator.visual_demand() {
                demand.policy(true, true);
            }
            for event in emulator.drain_events() {
                match event {
                    TerminalEvent::Title(title) => titles.push(title),
                    TerminalEvent::Exited => exited = true,
                    _ => {}
                }
            }
            round += 1;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        emulator.set_adaptive_enabled(false);
        assert!(round > 2);
        assert_eq!(
            titles,
            (0..300).map(|i| format!("toggle-{i}")).collect::<Vec<_>>()
        );
        let text: String = emulator
            .term()
            .lock()
            .unwrap()
            .grid()
            .display_iter()
            .map(|c| c.c)
            .collect();
        assert!(text.contains("FINAL-TOGGLE"));
        assert!(emulator.visual_demand().is_none());
    }

    #[cfg(feature = "adaptive-terminal-refresh")]
    #[test]
    fn adaptive_reader_keeps_ordered_protocol_and_sync_final_grid_without_output_wakes() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep .1; i=0; while [ $i -lt 300 ]; do printf '\\033]2;title-%s\\007' \"$i\"; i=$((i+1)); done; printf '\\033[?2026hFINAL'; sleep .1"]);
        let emulator = events::with_adaptive(|| {
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).unwrap()
        });
        let demand = emulator.visual_demand().unwrap();
        demand.policy(true, true);
        let mut titles = Vec::new();
        let mut exited = false;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !exited {
            assert!(std::time::Instant::now() < until);
            emulator.flush_stale_sync();
            let batch = emulator.drain_events();
            assert!(batch.len() <= 128);
            for event in batch {
                match event {
                    TerminalEvent::Title(title) => titles.push(title),
                    TerminalEvent::Exited => exited = true,
                    TerminalEvent::Wakeup => panic!("adaptive output entered semantic delivery"),
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(wait_for(|| {
            emulator.flush_stale_sync();
            emulator.has_text()
        }));
        assert_eq!(
            titles,
            (0..300).map(|i| format!("title-{i}")).collect::<Vec<_>>()
        );
        assert!(emulator.has_text());
        assert!(demand.capture().generation > 0);
        assert_eq!(
            demand.action(std::time::Instant::now()),
            crate::terminal::refresh::Action::Notify
        );
        // Current takes over pending dirt exactly once, even after EOF.
        assert!(demand.policy(true, false));
        emulator.wake_current();
        assert!(emulator.drain_events().contains(&TerminalEvent::Wakeup));
        assert!(!demand.policy(true, false));
    }
    #[test]
    fn semantic_flood_is_delivered_in_bounded_ordered_batches() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "i=0; while [ $i -lt 250 ]; do printf '\\033]2;title-%s\\007' \"$i\"; i=$((i+1)); done; printf DONE"]);
        let emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).unwrap();
        assert!(wait_for(|| emulator.has_text()));
        let titles = RefCell::new(Vec::new());
        assert!(wait_for(|| {
            let batch = emulator.drain_events();
            assert!(
                batch.len() <= 128,
                "UI event batch was unbounded: {}",
                batch.len()
            );
            titles
                .borrow_mut()
                .extend(batch.into_iter().filter_map(|event| match event {
                    TerminalEvent::Title(title) => Some(title),
                    _ => None,
                }));
            titles.borrow().len() == 250
        }));
        assert_eq!(
            titles.into_inner(),
            (0..250).map(|i| format!("title-{i}")).collect::<Vec<_>>()
        );
    }

    /// **A child has a UTF-8 locale when the window had none** — M5.14.
    /// A Guild opened from the Dock inherits launchd's environment, which
    /// has no `LANG`: zsh ran in the C locale and Cyrillic came out as
    /// escapes. The system's locale is used when the machine has it, and
    /// `en_US.UTF-8` when it does not; a `LANG` already there is left.
    #[test]
    fn a_child_is_given_a_utf8_locale_when_there_is_none() {
        let have = |name: &str| ["ru_RU.UTF-8", "en_US.UTF-8"].contains(&name);
        assert_eq!(
            lang(Some(OsStr::new("de_DE.UTF-8")), Some("ru_RU"), have),
            None
        );
        assert_eq!(
            lang(None, Some("ru_RU"), have).as_deref(),
            Some("ru_RU.UTF-8")
        );
        assert_eq!(
            lang(None, Some("en_RU"), have).as_deref(),
            Some("en_US.UTF-8")
        );
        assert_eq!(lang(None, None, have).as_deref(), Some("en_US.UTF-8"));
        assert_eq!(
            lang(None, Some("ru_RU@rg=ruzzzz"), have).as_deref(),
            Some("ru_RU.UTF-8"),
            "a region override after @ is not part of the name"
        );
    }

    /// **A caller's `PATH` is the caller's, whatever bytes it is.** The
    /// builder's own list of what was set answers only for names and values
    /// that are UTF-8, so a `PATH` with one directory named in Latin-1 was
    /// not seen as set and the login shell's replaced it. What differs from
    /// the inherited value was set, and so was what the list names. No pty
    /// is opened: this is the builder alone.
    #[test]
    fn a_path_the_caller_set_is_seen_whatever_its_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let inherited = std::env::var_os("PATH");
        let untouched = CommandBuilder::new("true");
        assert!(!caller_set_path(&untouched, inherited.as_deref()));

        let mut plain = CommandBuilder::new("true");
        plain.env("PATH", "/caller/bin");
        assert!(caller_set_path(&plain, inherited.as_deref()));

        let mut latin = CommandBuilder::new("true");
        latin.env("PATH", OsStr::from_bytes(b"/caf\xe9/bin:/usr/bin"));
        assert!(
            caller_set_path(&latin, inherited.as_deref()),
            "a PATH that is not UTF-8 is still the caller's"
        );

        let mut same = CommandBuilder::new("true");
        if let Some(inherited) = &inherited {
            same.env("PATH", inherited);
        }
        assert!(
            caller_set_path(&same, inherited.as_deref()),
            "set to the inherited value, and named in the list"
        );
    }

    /// **A program's questions reach the window as replies** — M5.12 task
    /// 6. A `Term` with our `Listener` and no pty, fed what programs ask:
    /// the formed answers arrive as text, a colour and a pixel size as
    /// formatters the view completes, and an OSC 52 copy as the clipboard's
    /// text. A formatter is equal only to itself, so each is compared by
    /// what `answer` makes of it with a fixed colour and area.
    #[test]
    fn a_program_s_questions_reach_the_window_as_replies() {
        use crate::terminal::reply::{Reply, answer};
        use alacritty_terminal::event::WindowSize;
        use alacritty_terminal::vte::ansi::Rgb;

        let (sender, events) = channel();
        let size = GridSize::new(80, 24);
        let mut term = Term::new(Config::default(), &size, Listener(sender));
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(
            &mut term,
            b"\x1b[c\x1b[>c\x1b[6n\x1b[?2004$p\x1b]11;?\x07\x1b[14t\x1b[18t\x1b]52;c;aGVsbG8=\x07",
        );

        let colour = |index: usize| {
            assert_eq!(index, 257, "OSC 11 asks for the background");
            Rgb {
                r: 0x12,
                g: 0x34,
                b: 0xab,
            }
        };
        let area = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 16,
            cell_height: 34,
        };
        let replies: Vec<TerminalEvent> = events
            .try_iter()
            .filter(|event| *event != TerminalEvent::Wakeup)
            .collect();
        let texts: Vec<String> = replies
            .iter()
            .map(|event| match event {
                TerminalEvent::Reply(reply) => {
                    answer(reply, colour, Some(area)).expect("an area is given")
                }
                TerminalEvent::ClipboardStore(text) => format!("clipboard: {text}"),
                other => panic!("not a reply: {other:?}"),
            })
            .collect();

        assert_eq!(texts.len(), 8, "{texts:?}");
        assert_eq!(texts[0], "\x1b[?6c");
        assert!(
            texts[1].starts_with("\x1b[>0;") && texts[1].ends_with(";1c"),
            "{:?}",
            texts[1]
        );
        assert_eq!(texts[2], "\x1b[1;1R");
        assert_eq!(
            texts[3], "\x1b[?2004;2$y",
            "bracketed paste is off: reset is 2"
        );
        assert_eq!(texts[4], "\x1b]11;rgb:1212/3434/abab\x07");
        assert!(matches!(
            replies[4],
            TerminalEvent::Reply(Reply::Colour(257, _))
        ));
        assert_eq!(texts[5], "\x1b[4;816;1280t");
        assert!(matches!(replies[5], TerminalEvent::Reply(Reply::Size(_))));
        assert_eq!(texts[6], "\x1b[8;24;80t");
        assert_eq!(texts[7], "clipboard: hello");
    }

    /// **A store aimed at the selection does not reach the clipboard.**
    /// OSC 52 names `c` for the clipboard and `p` or `s` for the primary
    /// selection, which macOS does not have; alacritty drops a selection
    /// store there, and so does Guild, rather than let a program that
    /// writes the selection overwrite what a person copied. An empty name
    /// is `c` — `vte` reads it so.
    #[test]
    fn a_store_aimed_at_the_selection_does_not_reach_the_clipboard() {
        let (sender, events) = channel();
        let size = GridSize::new(80, 24);
        let mut term = Term::new(Config::default(), &size, Listener(sender));
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(
            &mut term,
            b"\x1b]52;p;cHJpbWFyeQ==\x07\x1b]52;s;c2VsZWN0aW9u\x07\x1b]52;;Y2xpcGJvYXJk\x07",
        );

        let stores: Vec<TerminalEvent> = events
            .try_iter()
            .filter(|event| matches!(event, TerminalEvent::ClipboardStore(_)))
            .collect();
        assert_eq!(
            stores,
            vec![TerminalEvent::ClipboardStore("clipboard".into())]
        );
    }

    /// Wait for a real process to get somewhere.
    ///
    /// Bounded by the clock rather than by a count of iterations: a fixed
    /// number of 20 ms sleeps is a budget of two seconds only on an idle
    /// machine, and `just check` runs the whole workspace at once. That is
    /// how these tests came to fail in a full run and pass on their own —
    /// the classic look of a budget that is too tight rather than of a
    /// broken assertion.
    ///
    /// Ten seconds is far past anything the happy path needs: the wait
    /// ends the moment the condition holds, so a generous limit costs
    /// nothing when nothing is wrong.
    /// **The terminal says what it is, and then that it draws a frame
    /// whole** — M5.14. `claude` asks DECRQM 2026 only after XTVERSION is
    /// answered, and vte answers no XTVERSION: the reader does. Both
    /// halves of the handshake in one pty — the ptmx trap.
    #[test]
    fn it_answers_xtversion_and_then_mode_2026() {
        let command = {
            let mut command = CommandBuilder::new("sh");
            command.args([
                "-c",
                r"printf '\033[>q'; sleep 1; printf '\033[?2026$p'; sleep 2",
            ]);
            command
        };
        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
        let mut replies: Vec<String> = Vec::new();
        let mut answered = |prefix: &str| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                replies.extend(emulator_replies(&mut emulator));
                if replies.iter().any(|text| text.starts_with(prefix)) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        assert!(answered("\x1bP>|Guild "), "no XTVERSION answer");
        assert!(answered("\x1b[?2026;2$y"), "no DECRPM for 2026");
    }

    /// The first `count` text replies a program's output drew, waited for.
    fn replies_of(script: &str, count: usize) -> Vec<String> {
        let mut command = CommandBuilder::new("sh");
        command.args(["-c", script]);
        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
        let mut replies: Vec<String> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while replies.len() < count && std::time::Instant::now() < deadline {
            replies.extend(emulator_replies(&mut emulator));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        replies
    }

    /// **The OSC 7501 probe is answered before the DA1 that follows it** —
    /// Claude Code takes a DA1 reply with no 7501 reply before it as "no"
    /// for the session (issue 105534). One write, and the probe split over
    /// two writes with `CSI c` in the second; each reply ends the way its
    /// probe did.
    #[test]
    fn the_program_status_probe_is_answered_before_da1() {
        use crate::terminal::program_status::{Terminator, probe_answer};
        let da1 = "\x1b[?6c".to_string();

        let one = replies_of(r"printf '\033]7501;?\007\033[c'; sleep 2", 2);
        assert_eq!(one, vec![probe_answer(Terminator::Bel), da1.clone()]);

        let split = replies_of(
            r"printf '\033]75'; sleep 0.3; printf '01;?\033\\\033[c'; sleep 2",
            2,
        );
        assert_eq!(split, vec![probe_answer(Terminator::St), da1.clone()]);

        // The recorded caveat: the protocol sends the probe first, and a
        // program that sent `CSI c` first in the same read would get the
        // replies the other way round, because the probe is answered
        // before the read is parsed.
        let reversed = replies_of(r"printf '\033[c\033]7501;?\007'; sleep 2", 2);
        assert_eq!(reversed, vec![probe_answer(Terminator::Bel), da1]);
    }

    /// **What a program reported working ends when it exits**, and a
    /// finished turn it reported stays: OSC 7501's process-exit rule.
    #[test]
    fn a_working_record_ends_with_the_process() {
        use crate::terminal::program_status::State;
        let spawn = |script: &str| {
            let mut command = CommandBuilder::new("sh");
            command.args(["-c", script]);
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts")
        };
        let exited = |emulator: &mut Emulator| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while emulator.exit_status().is_none() {
                assert!(std::time::Instant::now() < deadline, "the process exits");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        let mut working = spawn(r"printf '\033]7501;state=working\007'; sleep 0.5");
        assert!(wait_for(
            || working.status_root().map(|r| r.state) == Some(State::Working)
        ));
        exited(&mut working);
        assert!(wait_for(|| working.status_root().is_none()));

        let mut done = spawn(r"printf '\033]7501;state=done\007'");
        exited(&mut done);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(done.status_root().map(|r| r.state), Some(State::Done));
    }

    /// **A frame left open by an exiting program cannot bring its status
    /// back** — review r1, finding 3. The last frame is replayed before the
    /// process-exit rule ends `working` and `blocked`, so the window's own
    /// flush of the completed frame afterwards finds nothing to restore.
    #[test]
    fn an_open_frame_at_exit_does_not_restore_an_active_record() {
        for state in ["working", "blocked:kind=question"] {
            let mut command = CommandBuilder::new("sh");
            command.args([
                "-c",
                &format!(r"printf '\033[?2026h\033]7501;state={state}\007frame'"),
            ]);
            let emulator =
                Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
            // The view publishes the last frame on the reader's `Exited` — not
            // when `exit_status` first answers, which can be before the reader
            // has read to the end.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !emulator
                .drain_events()
                .into_iter()
                .any(|event| event == TerminalEvent::Exited)
            {
                assert!(std::time::Instant::now() < deadline, "the reader finishes");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // What the view does on `Exited`: publish the reader's last frame.
            assert!(wait_for(|| {
                emulator.flush_completed_sync();
                text_of(&emulator).contains("frame")
            }));
            assert_eq!(emulator.status_root(), None, "{state} came back after exit");
        }
    }

    /// **A synchronized frame begun and never ended is let go** — M5.14.
    /// vte only stores the deadline; alacritty's own loop is what calls
    /// `stop_sync`, and ours never did, so a program killed mid-frame left
    /// the tab frozen on the frame before it until two megabytes more.
    #[test]
    fn a_frame_never_ended_is_let_go_after_the_timeout() {
        let mut command = CommandBuilder::new("sh");
        // `ready` is printed before the frame, so the test knows the frame
        // has begun rather than guessing how long `sh` takes to start —
        // M5.14 review: a fixed sleep failed under a loaded parallel run.
        command.args([
            "-c",
            r"printf ready; printf '\033[?2026h'; printf hi; sleep 3",
        ]);
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
        assert!(wait_for(|| text_of(&emulator).contains("ready")));
        assert!(
            !text_of(&emulator).contains("hi"),
            "held while the frame is open"
        );
        assert!(
            wait_for(|| emulator.flush_stale_sync()),
            "the frame was let go"
        );
        assert!(
            text_of(&emulator).contains("hi"),
            "let go once the timeout has passed"
        );
    }

    /// **Typing into a program that is not reading never waits** — M5.14.
    /// A pty write blocks once about a kilobyte waits unread, and `send`
    /// ran on the window's thread: a paste into a tab whose program had
    /// the terminal raw and was busy hung every tab, and `ctrl-c` could
    /// not be sent. Measured: 64 KB into `stty raw; sleep 5` waited 4.7 s.
    /// A program in cooked mode is no test — macOS drops what overflows
    /// its line rather than blocking.
    #[test]
    fn sending_to_a_program_that_is_not_reading_returns_at_once() {
        let mut command = CommandBuilder::new("sh");
        // `raw` is printed once the terminal is raw, so the send is never
        // made while it is still cooked — which would pass without proving
        // anything. M5.14 review.
        command.args(["-c", "stty raw -echo; printf raw; sleep 5"]);
        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
        assert!(wait_for(|| text_of(&emulator).contains("raw")));
        let started = std::time::Instant::now();
        emulator.send(&vec![b'x'; 64 * 1024]);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "send waited {:?}",
            started.elapsed()
        );
    }

    /// **Closing a tab hangs up its shell and its jobs** — M5.14. The
    /// shell was SIGKILLed, which cannot pass a hangup on, so a job it
    /// had put in the background — `npm run dev &` — lived on holding its
    /// port with nothing on screen owning it.
    #[test]
    fn closing_a_tab_ends_the_jobs_its_shell_started() {
        let folder = tempfile::Builder::new()
            .prefix("guild-hangup-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        let pid_file = dir.join("pid");
        let mut command = CommandBuilder::new("zsh");
        command.args([
            "-fic",
            &format!("sleep 300 & echo $! > {}; wait", pid_file.display()),
        ]);
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("zsh starts");
        assert!(wait_for(|| std::fs::read_to_string(&pid_file)
            .is_ok_and(|text| text.trim().parse::<i32>().is_ok())));
        let job: i32 = std::fs::read_to_string(&pid_file)
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(job), "the job runs while the tab is open");

        let closed = std::time::Instant::now();
        drop(emulator);
        assert!(
            closed.elapsed() < std::time::Duration::from_millis(100),
            "closing waited {:?}",
            closed.elapsed()
        );
        assert!(
            wait_for(|| !alive(job)),
            "the background job outlived its tab"
        );
    }

    /// **Closing a tab types nothing into what is still running** — M5.14
    /// review. portable-pty's writer writes a newline and VEOF when it is
    /// dropped; once closing became a hangup rather than a kill, those
    /// reached a program that outlived the hangup, and at a `[Y/n]` prompt
    /// a newline is a yes.
    #[test]
    fn closing_a_tab_types_nothing_into_a_program_that_outlives_the_hangup() {
        let folder = tempfile::Builder::new()
            .prefix("guild-noenter-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        let got = dir.join("got");
        let ready = dir.join("ready");
        let mut command = CommandBuilder::new("sh");
        command.args([
            "-c",
            &format!(
                "trap '' HUP; touch {ready}; read line; echo \"[$line]\" > {got}; sleep 5",
                ready = ready.display(),
                got = got.display()
            ),
        ]);
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("sh starts");
        assert!(wait_for(|| ready.exists()), "the program is reading");
        drop(emulator);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            !got.exists(),
            "the program was typed into: {:?}",
            std::fs::read_to_string(&got)
        );
    }

    /// **What ignores the hangup is killed, even when its shell went at
    /// once** — M5.14 review. The reaper stopped as soon as the shell was
    /// gone, so a program in front that ignores SIGHUP lived on holding
    /// the pty, and the tab's reader and scrollback with it.
    #[test]
    fn a_program_in_front_that_ignores_the_hangup_is_killed_after_the_grace() {
        let folder = tempfile::Builder::new()
            .prefix("guild-front-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        let pid_file = dir.join("pid");
        let mut command = CommandBuilder::new("zsh");
        command.args([
            "-fic",
            &format!(
                // `; true` keeps zsh from exec'ing the last command: sh
                // must be a job in front of a shell, not the tab's leader.
                "sh -c 'trap \"\" HUP; echo $$ > {}; while :; do sleep 1; done'; true",
                pid_file.display()
            ),
        ]);
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("zsh starts");
        assert!(wait_for(|| std::fs::read_to_string(&pid_file)
            .is_ok_and(|text| text.trim().parse::<i32>().is_ok())));
        let front: i32 = std::fs::read_to_string(&pid_file)
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(front));
        drop(emulator);
        assert!(
            wait_for(|| !alive(front)),
            "the program in front outlived its tab"
        );
    }

    /// **Letting go says when it is over** — M13.3. The hand-over starts
    /// the window's own resume on the conversation the typed program held,
    /// and must not start it while that program lives: two processes on
    /// one transcript. The word comes only once the program in front is
    /// gone, even one that ignores the hangup.
    #[test]
    fn letting_go_says_so_only_when_the_program_in_front_is_gone() {
        let folder = tempfile::Builder::new()
            .prefix("guild-letgo-")
            .tempdir()
            .expect("a scratch folder");
        let pid_file = folder.path().join("pid");
        let mut command = CommandBuilder::new("zsh");
        command.args([
            "-fic",
            &format!(
                "sh -c 'trap \"\" HUP; echo $$ > {}; while :; do sleep 1; done'; true",
                pid_file.display()
            ),
        ]);
        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("zsh starts");
        assert!(wait_for(|| std::fs::read_to_string(&pid_file)
            .is_ok_and(|text| text.trim().parse::<i32>().is_ok())));
        let front: i32 = std::fs::read_to_string(&pid_file)
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(front));

        let over = emulator.let_go().expect("a live terminal can be let go");
        over.recv_timeout(std::time::Duration::from_secs(5))
            .expect("told when it is over");
        assert!(!alive(front), "told before the program in front was gone");
        assert!(emulator.let_go().is_none(), "let go of once");
    }

    /// **Quitting hangs up too** — M5.14 review. `cmd-q` stopped every
    /// terminal with Stop's SIGKILL, which passes nothing on, so a job a
    /// shell had put in the background outlived Guild itself.
    #[test]
    fn quitting_hangs_up_the_jobs_a_shell_started() {
        let folder = tempfile::Builder::new()
            .prefix("guild-quit-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        let pid_file = dir.join("pid");
        let mut command = CommandBuilder::new("zsh");
        command.args([
            "-fic",
            &format!("sleep 300 & echo $! > {}; wait", pid_file.display()),
        ]);
        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 24)).expect("zsh starts");
        assert!(wait_for(|| std::fs::read_to_string(&pid_file)
            .is_ok_and(|text| text.trim().parse::<i32>().is_ok())));
        let job: i32 = std::fs::read_to_string(&pid_file)
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        let asked = std::time::Instant::now();
        emulator.hang_up_for_quit();
        assert!(
            asked.elapsed() < std::time::Duration::from_millis(300),
            "quitting waited {:?}",
            asked.elapsed()
        );
        assert!(
            wait_for(|| !alive(job)),
            "the background job outlived the quit"
        );
    }

    /// The text replies an emulator has queued.
    fn emulator_replies(emulator: &mut Emulator) -> Vec<String> {
        emulator
            .drain_events()
            .into_iter()
            .filter_map(|event| match event {
                TerminalEvent::Reply(Reply::Text(text)) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn wait_for<F: Fn() -> bool>(condition: F) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

        while std::time::Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        condition()
    }

    /// The cursor's style, read the way painting reads it: through the
    /// grid's lock. There is no accessor on `Emulator` on purpose — one
    /// would be called from inside a frame that already holds the lock,
    /// and that is the deadlock this pair of helpers exists to avoid.
    fn cursor_style(emulator: &Emulator) -> CursorStyle {
        emulator
            .term()
            .lock()
            .map(|term| term.cursor_style())
            .unwrap_or_default()
    }

    fn cursor_visible(emulator: &Emulator) -> bool {
        emulator
            .term()
            .lock()
            .map(|term| term.mode().contains(TermMode::SHOW_CURSOR))
            .unwrap_or(true)
    }

    fn text_of(emulator: &Emulator) -> String {
        let term = emulator.term().lock().unwrap();
        term.grid()
            .display_iter()
            .map(|cell| cell.c)
            .collect::<String>()
    }

    /// Bytes from the process reach the grid. Without that everything
    /// else is pointless.
    #[test]
    fn output_reaches_the_grid() {
        let mut command = CommandBuilder::new("printf");
        command.arg("guild-check");

        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).expect("printf starts");

        assert!(
            wait_for(|| text_of(&emulator).contains("guild-check")),
            "the output did not reach the grid: {:?}",
            text_of(&emulator).trim()
        );
    }

    /// **A program in a tab is told what terminal it is in, by the tab**,
    /// whatever the window inherited. A window started without `TERM` —
    /// from Finder, or with a bare environment — ran `claude` in monochrome,
    /// because nothing else says a terminal has colour; and a window started
    /// from another terminal handed its programs that terminal's name,
    /// `TERM_PROGRAM=OtherTerminal` among them. Both are the tab's to say.
    #[test]
    fn a_program_is_told_the_terminal_it_is_in_and_not_the_one_before() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-c");
        command.arg(r#"printf '%s|%s|%s|%s' "${TERM:-none}" "${COLORTERM:-none}" "${TERM_PROGRAM:-none}" "${TERM_PROGRAM_VERSION:-none}""#);
        // What a window started from another terminal carries.
        command.env("TERM", "xterm-other");
        command.env("TERM_PROGRAM", "OtherTerminal");
        command.env("TERM_PROGRAM_VERSION", "0.1.0");
        command.env_remove("COLORTERM");

        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 5)).expect("sh starts");

        assert!(
            wait_for(|| text_of(&emulator).contains("xterm-256color|truecolor|none|none")),
            "the program was told: {:?}",
            text_of(&emulator).trim()
        );
    }

    /// **What a terminal runs in front is Claude Code when it is** — M12.3.
    /// A program called `claude` started from the shell in a pty becomes
    /// the pty's foreground process group, and its command line says what
    /// it is; before it starts, the shell in front is not Claude Code.
    #[test]
    fn a_program_called_claude_in_front_is_recognised() {
        let folder = tempfile::Builder::new()
            .prefix("gfg-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        let claude = dir.join("claude");
        std::fs::write(&claude, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();

        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-c");
        // A shell that waits, then runs the program in the foreground the
        // way a person's shell does when `claude` is typed.
        command.arg(format!("sleep 1; exec {}", claude.display()));
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(80, 5)).expect("sh starts");

        let early = emulator.foreground_process().and_then(command_line);
        assert!(
            !early
                .as_deref()
                .is_some_and(crate::agents::is_claude_command),
            "the shell is not Claude Code: {early:?}"
        );
        assert!(
            wait_for(|| emulator
                .foreground_process()
                .and_then(command_line)
                .is_some_and(|args| crate::agents::is_claude_command(&args))),
            "what runs in front: {:?}",
            emulator.foreground_process().and_then(command_line)
        );
    }

    /// The size changes on the grid. This checks the grid specifically
    /// rather than the pty: that the pty received the size is only visible
    /// from inside the process — there is a separate live check for
    /// that.
    #[test]
    fn resize_changes_the_grid() {
        let mut command = CommandBuilder::new("sleep");
        command.arg("30");

        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).expect("sleep starts");

        emulator.resize(GridSize::new(100, 30));

        assert_eq!(emulator.size, GridSize::new(100, 30));
        let term = emulator.term().lock().unwrap();
        assert_eq!(term.columns(), 100);
        assert_eq!(term.screen_lines(), 30);
    }

    /// A zero size crashes alacritty, and getting one is easy: the window
    /// collapses for an instant when minimised.
    #[test]
    fn a_zero_sized_grid_is_impossible() {
        let size = GridSize::new(0, 0);

        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    /// A missing command is an error with a message, not a panic: the tab
    /// has to open and explain what happened.
    #[test]
    fn a_missing_command_is_an_error_naming_it() {
        let command = CommandBuilder::new("no-such-command-on-the-path");

        let err = match Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)) {
            Err(err) => err.to_string(),
            Ok(_) => panic!("a non-existent command must not start"),
        };

        assert!(err.contains("no-such-command-on-the-path"), "{err}");
        assert!(err.contains("PATH"), "{err}");
    }

    /// The process lives exactly as long as the tab — and while it lives
    /// it has no exit status, which is asked here rather than in a pty of
    /// its own: `kern.tty.ptmx_max` is 511 and shared with every terminal
    /// on the machine, so a test that needs a live process asks
    /// everything it can of the one it already has.
    #[test]
    fn dropping_the_emulator_kills_the_process() {
        let mut command = CommandBuilder::new("sleep");
        command.arg("300");

        let mut emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).unwrap();
        let pid = emulator.process_id().expect("there is a pid");

        // Polled on the window's clock, so it must answer at once rather
        // than wait for the process it is asking about.
        let asked = std::time::Instant::now();
        assert!(emulator.exit_status().is_none(), "it is still running");
        assert!(
            asked.elapsed() < std::time::Duration::from_millis(200),
            "asking must not wait for the process"
        );

        drop(emulator);

        let alive = |pid: u32| {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };

        assert!(
            wait_for(|| !alive(pid)),
            "process {pid} outlived the tab closing — that is an orphan"
        );
    }

    /// The size reaches **the process**, not only the grid.
    ///
    /// This was the second complaint: maximise the window and find the
    /// content still sitting in the old column. The grid was recomputed
    /// correctly — what was missing was the `PtySize` going outward.
    ///
    /// Checked through `stty size`, because there is no other way: what
    /// the process believes its terminal to be is only visible from inside
    /// the process.
    #[test]
    fn the_process_learns_the_new_size() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        command.arg("while true; do stty size; sleep 0.2; done");

        let mut emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(40, 10))
            .expect("the shell starts");

        assert!(
            wait_for(|| text_of(&emulator).contains("10 40")),
            "the process did not see the initial size: {:?}",
            text_of(&emulator).trim()
        );

        emulator.resize(GridSize::new(132, 43));

        assert!(
            wait_for(|| text_of(&emulator).contains("43 132")),
            "the process did not learn the new size — the pty got no PtySize. Seen: {:?}",
            text_of(&emulator).trim()
        );
    }

    /// An end-to-end check of what the milestone started from: dim on the
    /// way in means dim on the way out.
    ///
    /// The bytes take the real path: command → pty → ANSI parsing → grid →
    /// paint runs. The unit tests in `paint` check that the flag is
    /// interpreted, but not that it arrives at all; arriving is exactly
    /// what the previous component lost.
    #[test]
    fn a_dim_escape_sequence_ends_up_dimmer_on_screen() {
        use crate::terminal::paint::{Mark, runs_of_row};
        use crate::terminal::palette::Palette;

        let mut command = CommandBuilder::new("printf");
        // \e[2m turns dim on, \e[22m turns it off. Exactly what `claude`
        // draws hints with.
        command.arg("\\033[2mdim\\033[22mbright");

        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 4)).expect("printf starts");

        assert!(
            wait_for(|| text_of(&emulator).contains("dimbright")),
            "the output did not arrive: {:?}",
            text_of(&emulator).trim()
        );

        let term = emulator.term().lock().unwrap();
        let palette = Palette::default();
        let columns = term.grid().columns();
        let cells: Vec<_> = term.grid().display_iter().collect();
        let first_row: Vec<&alacritty_terminal::term::cell::Cell> = cells
            .iter()
            .take(columns)
            .map(|indexed| indexed.cell)
            .collect();

        let runs = runs_of_row(
            first_row.into_iter().map(|cell| (cell, Mark::default())),
            &palette,
            term.colors(),
        );
        let dim = runs
            .iter()
            .find(|run| run.text.starts_with("dim"))
            .expect("there is a run of dim text");
        let bright = runs
            .iter()
            .find(|run| run.text.starts_with("bright"))
            .expect("there is a run of bright text");

        let dim_r = gpui::Rgba::from(dim.foreground).r;
        let bright_r = gpui::Rgba::from(bright.foreground).r;

        assert!(
            dim_r < bright_r - 0.1,
            "the dim text does not differ from the bright one: {dim_r} against {bright_r}. \
             That is exactly what was visible in the window: claude's hints at full brightness"
        );
    }

    /// Markers of another session do not reach the process.
    ///
    /// Guild is often started from a Claude Code session — agents work on
    /// this very code. The child `claude` then sees
    /// `CLAUDE_CODE_CHILD_SESSION` in the environment, takes it for its
    /// own and disables transcript writing: `⚠ Transcript saving is off —
    /// inherited CLAUDE_CODE_CHILD_SESSION marker`. And our "a
    /// conversation happened" signal rests on that transcript.
    ///
    /// The marker is set on the command explicitly rather than taken from
    /// the test's environment: otherwise on a clean machine the check
    /// would pass having checked nothing.
    #[test]
    fn markers_of_a_parent_session_do_not_reach_the_process() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        // The variable name is Latin: sh does not accept non-Latin names.
        command
            .arg("echo marker=${CLAUDE_CODE_CHILD_SESSION:-none}; echo own=${GUILD_PROBE:-none}");
        command.env("CLAUDE_CODE_CHILD_SESSION", "someone-elses-session");
        // An unrelated variable must arrive: what is removed are session
        // markers, not the whole environment.
        command.env("GUILD_PROBE", "arrived");

        let emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(60, 6))
            .expect("the shell starts");

        assert!(
            wait_for(|| text_of(&emulator).contains("own=arrived")),
            "the output did not arrive: {:?}",
            text_of(&emulator).trim()
        );

        let screen = text_of(&emulator);
        assert!(
            screen.contains("marker=none"),
            "a marker of another session reached the process: {:?}",
            screen.trim()
        );
    }

    /// How many bytes of output per second the terminal manages to parse.
    ///
    /// The measurement follows the real path: pty → read → ANSI parse →
    /// grid. Painting is not included, and that must be kept in mind when
    /// comparing with ready-made terminals: their number includes
    /// drawing.
    ///
    /// Marked `ignore`: this is a measurement rather than a check, and it
    /// has no place in `just check`.
    #[test]
    #[ignore = "a measurement, run by hand"]
    fn measure_throughput() {
        let folder = tempfile::Builder::new()
            .prefix("guild-throughput-")
            .tempdir()
            .expect("a scratch folder");
        let file = folder.path().join("output.txt");
        // Lines of 200 characters: similar to build output or logs.
        let line = "x".repeat(200);
        let text = (0..200_000)
            .map(|_| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file, &text).unwrap();
        let bytes = text.len();

        let mut command = CommandBuilder::new("cat");
        command.arg(file.to_string_lossy().as_ref());

        let started = std::time::Instant::now();
        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(120, 34)).expect("cat starts");

        // Wait for the end of the output: the process exits and the reader
        // thread sends
        // Exited.
        let mut finished = false;
        for _ in 0..600 {
            if emulator.drain_events().contains(&TerminalEvent::Exited) {
                finished = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let elapsed = started.elapsed();
        assert!(finished, "the output was not read within the time allowed");

        let mb = bytes as f64 / 1_048_576.0;
        println!(
            "parsed {mb:.1} MB in {:.2} s — {:.0} MB/s",
            elapsed.as_secs_f64(),
            mb / elapsed.as_secs_f64()
        );

        let _ = std::fs::remove_file(&file);
    }

    /// Scrolling through history shows what has already left the
    /// screen.
    #[test]
    fn scrolling_back_shows_earlier_output() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        command.arg("for i in $(seq 1 60); do echo line-$i; done; sleep 30");

        let mut emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(40, 10))
            .expect("the shell starts");

        // Wait until the first lines scroll past the top edge.
        assert!(
            wait_for(|| text_of(&emulator).contains("line-60")),
            "the output did not arrive"
        );
        assert!(
            !text_of(&emulator).contains("line-1 "),
            "the start of the output must leave the screen, otherwise there is nothing to scroll"
        );

        emulator.scroll(50);

        assert!(emulator.display_offset() > 0, "the viewport moved");
        assert!(
            text_of(&emulator).contains("line-1"),
            "scrolling back must show what left: {:?}",
            text_of(&emulator).trim()
        );

        emulator.scroll_to_bottom();

        assert_eq!(emulator.display_offset(), 0, "back to the latest output");
        assert!(text_of(&emulator).contains("line-60"));
    }

    /// Selecting and copying: what a terminal is unusable without.
    #[test]
    fn a_selection_can_be_copied() {
        let mut command = CommandBuilder::new("printf");
        command.arg("abcdef");

        let mut emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).expect("printf starts");
        assert!(wait_for(|| text_of(&emulator).contains("abcdef")));

        // The first three cells of the first row.
        emulator.begin_selection(emulator.point_at(0, 0), Side::Left);
        emulator.extend_selection(emulator.point_at(0, 2), Side::Right);

        assert_eq!(
            emulator.selected_text().as_deref(),
            Some("abc"),
            "exactly what was selected is copied"
        );
        assert!(
            emulator.selection_range().is_some(),
            "painting needs the bounds, otherwise the selection is invisible"
        );

        emulator.clear_selection();
        assert_eq!(emulator.selected_text(), None);
        assert_eq!(emulator.selection_range(), None);
    }

    /// The default cursor, and why painting must not ask for it.
    ///
    /// Two claims in one test on purpose. Each of these spawns a real
    /// pseudo-terminal, and ptys are a system-wide resource: a run of the
    /// whole workspace alongside a person's own terminals exhausts them,
    /// and every pty test starts failing with `failed to openpty`. Adding
    /// one test per assertion is what made the suite flaky.
    ///
    /// **The default blinks.** `Config::default()` asks for a steady
    /// cursor, and one that does not blink reads as a picture rather than
    /// as the place typing will land — that was the complaint.
    ///
    /// **The accessors take the grid's lock.** Painting holds that lock
    /// for the whole frame and reads the cursor out of the guard it
    /// already has. Reaching for these accessors there takes the same
    /// mutex a second time, and a std mutex is not reentrant: the window
    /// deadlocks on its first frame, never appears, and the application
    /// shows up in Cmd+Tab with nothing behind it. That is exactly what
    /// happened.
    #[test]
    fn the_default_cursor_blinks_and_reading_it_takes_the_grid_lock() {
        let mut command = CommandBuilder::new("sleep");
        command.arg("5");

        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).expect("sleep starts");

        let style = cursor_style(&emulator);
        assert!(style.blinking, "the default cursor must blink");
        assert_eq!(style.shape, CursorShape::Block);
        assert!(cursor_visible(&emulator), "and be drawn at all");

        // No threads: `try_lock` shows the mutex is already held, which is
        // the whole reason a second `lock` in the same frame never
        // returns.
        let held = emulator.term().lock().expect("the grid locks");
        assert!(
            emulator.term().try_lock().is_err(),
            "the grid mutex is already held: a second lock in the same \
             frame is what never comes back"
        );

        drop(held);
        assert!(
            emulator.term().try_lock().is_ok(),
            "and is free again once painting lets go"
        );
    }

    /// The cursor is the program's to choose: `vim` asks for a beam in
    /// insert mode with DECSCUSR, and a full-screen program hides the
    /// cursor with DECTCEM while it repaints. Neither was handled.
    ///
    /// Both sequences in one process, for the pty reason above.
    #[test]
    fn a_program_chooses_the_cursor_shape_and_can_hide_it() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        // 6 is "steady bar" in DECSCUSR, `?25l` hides the cursor. The
        // escapes are real ESC bytes from Rust rather than `\033` for
        // printf to expand: Rust reads `\033` as NUL followed by "33",
        // and the sequence never arrives.
        command.arg("printf '\x1b[6 q'; printf '\x1b[?25l'; sleep 5");

        let emulator =
            Emulator::spawn(command, Path::new("/"), GridSize::new(40, 5)).expect("sh starts");

        assert!(
            wait_for(|| cursor_style(&emulator).shape == CursorShape::Beam),
            "the program asked for a beam and must get one"
        );
        assert!(!cursor_style(&emulator).blinking, "and asked for it steady");
        assert!(
            wait_for(|| !cursor_visible(&emulator)),
            "and the cursor must go when the program hides it"
        );
    }

    /// A selection lives in buffer coordinates while the window knows
    /// screen coordinates. Scrolled history shifts one relative to the
    /// other — without the conversion the selection would land somewhere
    /// other than where it was pointed.
    #[test]
    fn a_point_follows_the_scrolled_view() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        command.arg("for i in $(seq 1 60); do echo line-$i; done; sleep 30");

        let mut emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(40, 10))
            .expect("the shell starts");
        assert!(wait_for(|| text_of(&emulator).contains("line-60")));

        let without_scroll = emulator.point_at(0, 0);
        emulator.scroll(20);
        let with_scroll = emulator.point_at(0, 0);

        assert_ne!(
            without_scroll, with_scroll,
            "the same screen cell after scrolling is a different buffer row"
        );
    }
    /// The exit code reaches the window, and the program believes it is
    /// looking at a terminal.
    ///
    /// **Three things in one pty**, because `kern.tty.ptmx_max` is 511 and
    /// shared with every terminal on the machine: the code is the
    /// process's own, it arrives after the pty closes rather than with it,
    /// and `test -t 1` is true — which is what makes `cargo` and `just`
    /// colour their output at all, and so what M8.4's Output panel rests
    /// on.
    #[test]
    fn a_finished_process_says_what_it_exited_with() {
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        command.arg("test -t 1 && printf TTY-YES; exit 7");

        let mut emulator = Emulator::spawn(command, Path::new("/"), GridSize::new(40, 10))
            .expect("the shell starts");

        assert!(
            wait_for(|| emulator.drain_events().contains(&TerminalEvent::Exited)),
            "the process did not end"
        );
        // `wait_for` takes a `Fn`, so the poll cannot keep the answer;
        // the status is asked for again once the wait says it is there,
        // which is what a reaped child gives every time it is asked.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let status = loop {
            if let Some(status) = emulator.exit_status() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the status never arrived"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(status.exit_code(), 7);
        assert!(!status.success());
        assert!(
            text_of(&emulator).contains("TTY-YES"),
            "a command in a pty must believe it is on a terminal, or it turns its colours off"
        );
    }
}

#[cfg(test)]
mod descriptor_failure_tests {
    use super::*;
    #[test]
    fn failed_preparation_starts_no_command() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "terminal::emulator::descriptor_failure_tests::owned_preparation_failure",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[test]
    #[ignore = "run through failed_preparation_starts_no_command"]
    fn owned_preparation_failure() {
        for supervised in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("started");
            let mut command = CommandBuilder::new("/bin/sh");
            command.args(["-c", "echo $$ > \"$1\"", "fixture"]);
            command.arg(&marker);
            let prepared = std::cell::Cell::new(None);
            let result = Emulator::spawn_prepared(
                command,
                dir.path(),
                GridSize::new(80, 24),
                supervised,
                None,
                |master| {
                    use std::os::fd::AsRawFd as _;
                    let (writer, reader) = prepare_descriptors(master)?;
                    prepared.set(Some((master.as_raw_fd().unwrap(), writer.as_raw_fd())));
                    drop((writer, reader));
                    let until = std::time::Instant::now() + std::time::Duration::from_millis(300);
                    while !marker.exists() && std::time::Instant::now() < until {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(TerminalError::Pty(
                        std::io::Error::from_raw_os_error(libc::EMFILE).into(),
                    ))
                },
            );
            // The baseline command is bounded and exits itself. Reap only its
            // recorded PID if the ordinary pre-construction owner dropped it.
            if !supervised && let Ok(pid) = std::fs::read_to_string(&marker) {
                unsafe {
                    libc::waitpid(pid.trim().parse().unwrap(), std::ptr::null_mut(), 0);
                }
            }
            assert!(matches!(result, Err(TerminalError::Pty(_))));
            assert!(!marker.exists(), "descriptor failure launched a command");
            let (master, writer) = prepared
                .get()
                .expect("descriptors were prepared before the injected error");
            for fd in [master, writer] {
                // SAFETY: querying a descriptor in this isolated, thread-free child.
                assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EBADF)
                );
            }
        }
    }
}

#[cfg(test)]
mod failure_lifecycle_tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn parsed_wrapped_network_error(columns: usize) {
        parsed_network_error(columns, 24);
    }
    fn parsed_network_error(columns: usize, rows: usize) {
        use guild_session::HarnessKind;
        const ERROR: &str = "Error: account/read failed during TUI bootstrap: account/read failed: workspace routing discovery failed (code -32603)";
        struct Fragmented(std::io::Cursor<Vec<u8>>);
        impl std::io::Read for Fragmented {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = buffer.len().min(3);
                std::io::Read::read(&mut self.0, &mut buffer[..count])
            }
        }
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 7"]);
        let mut emulator = Emulator::spawn_prepared(
            command,
            Path::new("/"),
            GridSize::new(columns, rows),
            false,
            None,
            |master| {
                let (writer, reader) = prepare_descriptors(master)?;
                drop(reader);
                Ok((
                    writer,
                    Box::new(Fragmented(std::io::Cursor::new(
                        format!("\x1b[31m{ERROR}\x1b[0m").into_bytes(),
                    ))),
                ))
            },
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(4);
        loop {
            if emulator.next_event() == Some(TerminalEvent::Exited) {
                break;
            }
            assert!(Instant::now() < until, "fragmented reader did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
        {
            let term = emulator.term().lock().unwrap();
            let last = Point::new(alacritty_terminal::index::Line(0), Column(columns - 1));
            assert!(
                term.grid()[last]
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
            );
            assert_eq!(term.grid()[last].c, ERROR.as_bytes()[columns - 1] as char);
            if columns == 80 || columns == 88 {
                assert_eq!(
                    term.grid()[last].c,
                    ' ',
                    "separator must exist in the real parsed row"
                );
            }
        }
        let tail = emulator.failure_tail();
        let failure = super::super::failure::Failure::derive(
            Some(HarnessKind::Codex),
            emulator.exit_status().map(|s| s.exit_code()),
            &tail,
        )
        .unwrap();
        assert_eq!(
            tail.trim_end(),
            ERROR,
            "columns={columns}, rows={rows}, network={}: retained parsed diagnostic changed",
            failure.network
        );
        assert!(
            failure.network,
            "columns={columns}: exact reported connectivity failure must retain guidance"
        );
    }
    #[test]
    fn tall_terminal_keeps_short_network_error() {
        parsed_network_error(80, 120);
    }
    #[test]
    fn wrapped_network_error_preserves_workspace_separator_at_80_columns() {
        parsed_wrapped_network_error(80);
    }
    #[test]
    fn wrapped_network_error_preserves_routing_separator_at_88_columns() {
        parsed_wrapped_network_error(88);
    }
    #[test]
    fn wrapped_network_error_joins_mid_word_at_81_columns() {
        parsed_wrapped_network_error(81);
    }

    #[test]
    fn missing_cwd_never_launches_in_home() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("removed");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        assert!(Emulator::spawn(command, &missing, GridSize::new(80, 24)).is_err());
    }
    #[test]
    fn cwd_removed_during_launch_preparation_never_falls_back_home() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("removed-during-prepare");
        std::fs::create_dir(&cwd).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        let opened = Emulator::spawn_prepared(
            command,
            &cwd,
            GridSize::new(80, 24),
            false,
            None,
            |master| {
                let descriptors = prepare_descriptors(master)?;
                std::fs::remove_dir(&cwd).unwrap();
                Ok(descriptors)
            },
        );
        assert!(
            opened.is_err(),
            "cwd must be checked after preparation, immediately before spawn"
        );
    }

    #[test]
    fn eof_before_delayed_nonzero_exit_publishes_final_completion() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 0.3; exit 7"]);
        // A real PTY child with a deterministically early EOF from the reader
        // fixture. Darwin retains the controlling tty after closing stdio, so
        // closing fd 0/1/2 alone does not reproduce this timing reliably.
        let mut emulator = Emulator::spawn_prepared(
            command,
            Path::new("/"),
            GridSize::new(80, 24),
            false,
            None,
            |master| {
                let (writer, reader) = prepare_descriptors(master)?;
                drop(reader);
                Ok((
                    writer,
                    Box::new(std::io::Cursor::new(
                        b"\x1b[31mDNS lookup failed\x1b[0m".to_vec(),
                    )),
                ))
            },
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(4);
        let mut observed = None;
        while Instant::now() < until {
            if emulator.next_event() == Some(TerminalEvent::Exited) {
                observed = emulator.exit_status().map(|status| status.exit_code());
                if observed.is_some() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            observed,
            Some(7),
            "EOF must not be the final notification before exit: {}",
            emulator.failure_tail()
        );
        assert!(emulator.failure_tail().contains("DNS lookup failed"));
        assert!(!emulator.failure_tail().contains('\u{1b}'));
        assert!(emulator.failure_tail().len() <= 16 * 1024);
    }
}

#[cfg(test)]
mod input_bounds_tests {
    use super::*;
    #[test]
    fn oversized_input_is_refused_before_any_prefix_reaches_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let release = dir.path().join("release");
        let captured = dir.path().join("captured");
        let mut cmd = CommandBuilder::new("/usr/bin/python3");
        cmd.args([
            "-I",
            "-u",
            "-c",
            r#"
import os,sys,tty,time,signal,select,pathlib
signal.alarm(15)
r=pathlib.Path(sys.argv[1]);tty.setraw(0);(r/'ready').touch()
while not (r/'release').exists():time.sleep(.005)
data=os.read(0,1) if select.select([0],[],[],.3)[0] else b''
# Only the final name announces a complete, closed capture.
(r/'captured.pending').write_bytes(data)
(r/'captured.pending').replace(r/'captured')
"#,
        ]);
        cmd.arg(dir.path());
        let mut e = Emulator::spawn(cmd, dir.path(), GridSize::new(80, 24)).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready.exists() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        e.send(&vec![b'x'; 8 * 1024 * 1024 + 1]);
        std::fs::write(release, "").unwrap();
        while !captured.exists() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            std::fs::read(captured).unwrap().is_empty(),
            "oversized operation leaked a prefix before refusal"
        );
    }
}

#[cfg(test)]
mod input_pressure_tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn until(mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < end, "owned PTY pressure deadline");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    #[test]
    fn retained_completed_grid_releases_workers_after_trailing_output() {
        // Companion to terminal_jobs' retained descriptor fixture: observe the
        // actual transport reaper while completed screens remain owned.
        struct GatedReader {
            reader: Box<dyn std::io::Read + Send>,
            gate: Option<std::sync::mpsc::Receiver<()>>,
        }
        impl std::io::Read for GatedReader {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                let count = self.reader.read(bytes)?;
                // Drain the kernel before holding parser publication: Darwin's
                // exiting child may wait for its pending tty output to drain.
                if count > 0
                    && let Some(gate) = self.gate.take()
                {
                    gate.recv_timeout(Duration::from_secs(5))
                        .map_err(std::io::Error::other)?;
                }
                Ok(count)
            }
        }
        let mut retained = Vec::new();
        for _ in 0..4 {
            let (release, gate) = std::sync::mpsc::channel();
            let mut command = CommandBuilder::new("/bin/sh");
            command.args([
                "-c",
                "printf 'retained-output\\n'; printf '\\033[31mFINAL-TAIL\\033[0m'",
            ]);
            let e = Emulator::spawn_prepared(
                command,
                Path::new("/"),
                GridSize::new(80, 24),
                true,
                None,
                |master| {
                    let (writer, reader) = prepare_descriptors(master)?;
                    Ok((
                        writer,
                        Box::new(GatedReader {
                            reader,
                            gate: Some(gate),
                        }),
                    ))
                },
            )
            .unwrap();
            let done = e.io_workers.borrow().as_ref().unwrap().finished.clone();
            retained.push(e);
            let e = retained.last_mut().unwrap();
            until(|| e.job_completion().is_some());
            assert!(
                !e.failure_tail().contains("FINAL-TAIL"),
                "fixture must hold trailing output until after supervisor completion"
            );
            release.send(()).unwrap();
            // Completion may be observed before the reader has parsed its tail.
            // Retain the emulator without polling completion again: the owner
            // must finish independently of the consumer's next status request.
            until(|| done.load(std::sync::atomic::Ordering::Acquire));
            let mut exited = false;
            while let Some(event) = e.next_event() {
                if event == TerminalEvent::Exited {
                    exited = true;
                }
            }
            assert!(exited, "reader completion must follow trailing output");
            assert!(e.job_completion().unwrap().unwrap().success());
            assert!(e.failure_tail().contains("FINAL-TAIL"));
            assert!(e.has_text());
        }
        assert_eq!(retained.len(), 4);
    }
    #[test]
    fn active_full_write_counts_bytes_and_items_then_fifo_drains() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = CommandBuilder::new("/usr/bin/python3");
        cmd.args([
            "-I",
            "-u",
            "-c",
            r#"
import os,tty,pathlib,time,sys,signal
signal.alarm(20);r=pathlib.Path(sys.argv[1]);tty.setraw(0);(r/'ready').touch()
while not (r/'release').exists():time.sleep(.003)
data=bytearray()
while len(data)<8*1024*1024+4:data.extend(os.read(0,65536))
# Only the final name announces a complete, closed capture.
(r/'captured.pending').write_bytes(data)
(r/'captured.pending').replace(r/'captured')
"#,
        ]);
        cmd.arg(dir.path());
        let mut e = Emulator::spawn(cmd, dir.path(), GridSize::new(80, 24)).unwrap();
        until(|| dir.path().join("ready").exists());
        let text = "🦀".repeat((8 * 1024 * 1024 - 16) / 4);
        let mut bytes = super::super::keys::submitted(&text);
        bytes.extend_from_slice(b"xyz");
        assert_eq!(bytes.len(), 8 * 1024 * 1024);
        assert!(e.send(&bytes));
        let q = e.writer.borrow().as_ref().unwrap().clone();
        assert_eq!(q.usage(), (1, 8 * 1024 * 1024));
        assert!(!e.send(b"rejected"));
        for _ in 0..255 {
            assert!(e.send(b""));
        }
        assert_eq!(q.usage(), (256, 8 * 1024 * 1024));
        assert!(!e.send(b""));
        std::fs::write(dir.path().join("release"), "").unwrap();
        until(|| q.usage() == (0, 0));
        assert!(e.send(b"tail"));
        until(|| dir.path().join("captured").exists());
        bytes.extend_from_slice(b"tail");
        assert_eq!(std::fs::read(dir.path().join("captured")).unwrap(), bytes);
        let finished = e.io_workers.borrow().as_ref().unwrap().finished.clone();
        let start = Instant::now();
        drop(e);
        assert!(start.elapsed() < Duration::from_millis(100));
        until(|| finished.load(std::sync::atomic::Ordering::Acquire));
    }
}

#[cfg(test)]
mod admitted_failure_tests {
    use super::*;
    #[test]
    fn later_child_exit_is_uncertain_delivery_not_safe_unsent() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = CommandBuilder::new("/usr/bin/python3");
        cmd.args([
            "-I",
            "-u",
            "-c",
            r#"
import tty,time,pathlib,sys,signal
signal.alarm(15);r=pathlib.Path(sys.argv[1]);tty.setraw(0);(r/'ready').touch()
while not (r/'exit').exists():time.sleep(.003)
"#,
        ]);
        cmd.arg(dir.path());
        let mut e = Emulator::spawn(cmd, dir.path(), GridSize::new(80, 24)).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !dir.path().join("ready").exists() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        assert!(
            e.send(&vec![b'x'; 8 * 1024 * 1024]),
            "whole operation admitted before failure"
        );
        std::fs::write(dir.path().join("exit"), "").unwrap();
        while !e.input_delivery_failed() {
            assert!(
                std::time::Instant::now() < until,
                "admitted write failure not surfaced"
            );
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        assert!(!e.send(b"later operation"));
        let finished = e.io_workers.borrow().as_ref().unwrap().finished.clone();
        e.stop_io();
        while !finished.load(std::sync::atomic::Ordering::Acquire) {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
    }
}

#[cfg(test)]
thread_local! {
 static FAIL_PARTIAL_INPUT:std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
 static PARTIAL_INPUT_OWNER:std::cell::RefCell<Option<(u32,Arc<std::sync::atomic::AtomicBool>,usize)>> = const {std::cell::RefCell::new(None)};
}
#[cfg(test)]
mod partial_input_tests {
    use super::*;
    #[test]
    fn started_writer_and_supervisor_are_owned_when_reader_setup_fails() {
        for supervised in [false, true] {
            FAIL_PARTIAL_INPUT.with(|f| f.set(true));
            let mut cmd = CommandBuilder::new("/bin/sleep");
            cmd.arg("15");
            let result = Emulator::spawn_prepared(
                cmd,
                Path::new("/"),
                GridSize::new(80, 24),
                supervised,
                None,
                prepare_descriptors,
            );
            assert!(matches!(result, Err(TerminalError::Supervise(_))));
            let (pid, finished, count) =
                PARTIAL_INPUT_OWNER.with(|o| o.borrow_mut().take().unwrap());
            assert_eq!(count, if supervised { 2 } else { 1 });
            let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !finished.load(std::sync::atomic::Ordering::Acquire) {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
    }
}

#[cfg(test)]
mod idle_wait_tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    #[test]
    fn silent_emulator_has_no_periodic_transport_timeouts() {
        let mut cmd = CommandBuilder::new("/bin/sleep");
        cmd.arg("10");
        let e = Emulator::spawn(cmd, Path::new("/"), GridSize::new(80, 24)).unwrap();
        let (stop, done) = {
            let w = e.io_workers.borrow();
            let w = w.as_ref().unwrap();
            (w.stop.clone(), w.finished.clone())
        };
        let end = Instant::now() + Duration::from_secs(3);
        while stop.waiting.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        stop.timeouts.store(0, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        let count = stop.timeouts.load(Ordering::SeqCst);
        drop(e);
        while !done.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            count, 0,
            "silent Emulator periodically timed out IO/capacity waits"
        );
    }
}
