//! The `PATH` the person's login shell gives, for every process the window
//! starts.
//!
//! A `Guild.app` opened by LaunchServices inherits
//! `/usr/bin:/bin:/usr/sbin:/sbin`, where `claude`, `rust-analyzer` and a
//! Homebrew `git` are not. M5.11's decision 22 asks the login shell once,
//! on a thread started before the window, and gives its answer to every
//! spawn: the login shell's entries first, then whatever the window
//! inherited that they lack, so a tab, the Output panel and an agent mean
//! the same `git`.
//!
//! **`-l -i`, in a session of its own, read at the second mark.** `-i`
//! because `~/.zshrc` is where installers put `~/.local/bin`, and task 0.3
//! measured `-l` alone without it. A session of its own because an
//! interactive shell in a process group of its own under a controlling
//! terminal — `just desktop`, a check window started from a shell — is
//! stopped the moment it reaches for that terminal, and answers nothing.
//! The second mark rather than the end of the output, because an rc file
//! that starts something in the background hands it the shell's stdout and
//! the pipe stays open long after the answer.
//!
//! **Nothing here sets the process's own `PATH`.** Every spawn is given one;
//! `std::env::set_var` in a threaded process is the trap AGENTS.md names.

use std::ffi::{OsStr, OsString};
use std::io::Read as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

/// What the shell prints on both sides of its `PATH`.
pub const MARK: &str = "__GUILD_PATH__";

/// How long the login shell is given. Task 0.3 measured `zsh -l -i` at
/// 55–72 ms under LaunchServices' environment; three seconds is for a
/// slow rc file, and what a broken one costs a launch.
pub const PATIENCE: Duration = Duration::from_secs(3);

// std's `CommandExt::setsid` is unstable (`process_setsid`), and this crate
// links no `libc`: the one call is declared here.
unsafe extern "C" {
    fn setsid() -> std::ffi::c_int;
}

/// The bytes between the first two marks, or `None` when there are not two
/// or nothing lies between them.
pub fn parse(output: &[u8]) -> Option<OsString> {
    let mark = MARK.as_bytes();
    let first = find(output, mark)? + mark.len();
    let length = find(&output[first..], mark)?;
    let path = &output[first..first + length];
    (!path.is_empty()).then(|| OsStr::from_bytes(path).to_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The login shell's entries, then the inherited ones it lacks, none twice
/// and none empty — an empty entry means the working directory, which is
/// not a thing to hand every process.
pub fn merge(login: &OsStr, inherited: Option<&OsStr>) -> OsString {
    let mut parts: Vec<PathBuf> = Vec::new();
    let inherited = inherited.map(std::env::split_paths);
    for part in std::env::split_paths(login).chain(inherited.into_iter().flatten()) {
        if !part.as_os_str().is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    std::env::join_paths(parts).unwrap_or_else(|_| login.to_owned())
}

/// Ask `shell` for its `PATH` as a terminal tab's shell would have it, and
/// give up after `patience`.
///
/// The shell runs with `stdin` null and in a session of its own, so its
/// process group is its pid; on giving up that group is ended and the
/// shell reaped, so nothing it started outlives the question. A shell that
/// has answered is waited for only until the same deadline, and a
/// background child an rc file left holding the pipe is the rc file's, not
/// ours.
pub fn resolve(shell: &Path, patience: Duration) -> Result<OsString, String> {
    let deadline = Instant::now() + patience;
    let script = format!("printf \"\\n%s%s%s\\n\" {MARK} \"$PATH\" {MARK}");
    let mut command = Command::new(shell);
    command
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: `setsid` is async-signal-safe, touches no memory of ours, and
    // is the only thing done between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|err| format!("{} could not be started: {err}", shell.display()))?;
    let mut stdout = child.stdout.take().expect("stdout was piped");

    // The read is a thread of its own so that it can be given up on: a
    // blocking read has no deadline.
    let (said, heard) = mpsc::channel();
    std::thread::Builder::new()
        .name("guild-login-path".into())
        .spawn(move || {
            let mut output = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        output.extend_from_slice(&chunk[..read]);
                        if parse(&output).is_some() {
                            break;
                        }
                    }
                }
            }
            let _ = said.send(output);
        })
        .map_err(|err| format!("the login shell could not be read: {err}"))?;

    let output = heard.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let found = match &output {
        Ok(output) => parse(output),
        Err(_) => None,
    };
    // Reaped by the deadline, or ended: a shell that has printed its answer
    // exits at once, and one that has not is not going to.
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                end_group(child.id());
                let _ = child.wait();
                break;
            }
        }
    }
    match (found, output) {
        (Some(path), _) => Ok(path),
        (None, Ok(_)) => Err(format!(
            "{} ended without saying what PATH is",
            shell.display()
        )),
        (None, Err(_)) => Err(format!(
            "{} did not say what PATH is within {} ms",
            shell.display(),
            patience.as_millis()
        )),
    }
}

/// End a process group by its leader's pid — only ever called while that
/// leader is unreaped, so the group cannot be a stranger's.
fn end_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// What the thread found, and when it was asked.
struct Asked {
    since: Instant,
    answer: Mutex<Answer>,
    arrived: Condvar,
}

#[derive(Default)]
struct Answer {
    /// `None` until the shell has answered or been given up on.
    found: Option<Result<OsString, String>>,
    /// Whether the failure has been handed to the window.
    failure_taken: bool,
}

static ASKED: OnceLock<Asked> = OnceLock::new();

/// Start asking the login shell for its `PATH`, on a thread, once per
/// process. `$SHELL`, or `/bin/zsh` when it is unset — the shell macOS
/// gives a new account.
pub fn start() {
    let mut first = false;
    let asked = ASKED.get_or_init(|| {
        first = true;
        Asked {
            since: Instant::now(),
            answer: Mutex::new(Answer::default()),
            arrived: Condvar::new(),
        }
    });
    if !first {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("guild-login-shell".into())
        .spawn(move || {
            let shell = std::env::var_os("SHELL")
                .filter(|shell| !shell.is_empty())
                .map_or_else(|| PathBuf::from("/bin/zsh"), PathBuf::from);
            let found = resolve(&shell, PATIENCE).map(|login| {
                merge(&login, std::env::var_os("PATH").as_deref())
            });
            match &found {
                Ok(path) => tracing::debug!(shell = %shell.display(), path = ?path, "the login shell's PATH"),
                Err(err) => tracing::warn!(shell = %shell.display(), %err, "the login shell's PATH was not had"),
            }
            let mut answer = asked.answer.lock().unwrap_or_else(|e| e.into_inner());
            answer.found = Some(found);
            asked.arrived.notify_all();
        });
    if let Err(err) = spawned {
        let mut answer = asked.answer.lock().unwrap_or_else(|e| e.into_inner());
        answer.found = Some(Err(format!("the login shell could not be asked: {err}")));
    }
}

/// The `PATH` every spawned process runs with, or `None` to keep the one
/// inherited.
///
/// **Waits for the shell until `start`'s time plus [`PATIENCE`], and not a
/// moment past it**: once that instant has gone every call answers at once,
/// so the window's thread — which restores tabs and starts servers before
/// the answer can have come — waits at most once per process. Before
/// `start` has run, which is every test, it answers `None` at once.
pub fn path() -> Option<OsString> {
    let asked = ASKED.get()?;
    let deadline = asked.since + PATIENCE;
    let began = Instant::now();
    let mut answer = asked.answer.lock().unwrap_or_else(|e| e.into_inner());
    while answer.found.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        answer = asked
            .arrived
            .wait_timeout(answer, left)
            .map(|(guard, _)| guard)
            .unwrap_or_else(|e| e.into_inner().0);
    }
    let waited = began.elapsed();
    if waited >= Duration::from_millis(1) {
        tracing::debug!(?waited, "a spawn waited for the login shell's PATH");
    }
    answer
        .found
        .as_ref()
        .and_then(|found| found.as_ref().ok().cloned())
}

/// The `PATH` a harness is looked for on, as text: the login shell's, or
/// the window's own before that has answered — what
/// [`crate::HarnessKind::available_on`] and a floor check take. One
/// spelling, because the chevron menu, the start screen and the hooks'
/// install each asked it in their own words.
pub fn path_or_inherited() -> String {
    path()
        .or_else(|| std::env::var_os("PATH"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Why the login shell's `PATH` was not had, once — for the window's
/// notice. `None` while the shell is still being asked, when it answered,
/// and after the first time.
pub fn take_failure() -> Option<String> {
    let asked = ASKED.get()?;
    let mut answer = asked.answer.lock().unwrap_or_else(|e| e.into_inner());
    if answer.failure_taken {
        return None;
    }
    let reason = answer.found.as_ref()?.as_ref().err()?.clone();
    answer.failure_taken = true;
    Some(reason)
}
