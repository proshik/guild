//! The window's end of the hooks: a socket, a request parsed, a channel —
//! M5.10.
//!
//! A listener thread accepts; each connection is a thread that reads
//! until [`parse`] has a request, answers it, and hands the JSON to the
//! window, which takes it on the watch loop's clock with
//! [`Receiver::poll`] — the git worker's shape, and the IDE server's.
//!
//! **Only as much HTTP as the script's `curl` speaks.** One `POST` with a
//! `Content-Length`, one answer, the connection closed. Anything else is
//! refused with the status that names why, and the next connection is
//! heard as though nothing had happened: a window that stops listening
//! because one client said something odd is a window whose tabs go quiet
//! for the rest of its life.

use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use guild_session::command::{Answer, Command};
use serde_json::Value;

/// The door a session's hooks post to.
pub const HOOK_PATH: &str = "/hook";

/// The door the `guild` command asks at — M13 task 1.
pub const COMMAND_PATH: &str = "/command";

/// The door Guild's status line posts the plan's limits to — a channel of
/// its own, since a status line is not a hook and says nothing of what
/// its agent is doing.
pub const USAGE_PATH: &str = "/usage";

/// How long a connection waits for the window to answer a command.
///
/// The window answers on the watch loop's clock, which is a tick of a
/// few milliseconds — but it may be in the middle of a frame, a git walk
/// or a prompt. Ten seconds is far longer than any of those and short
/// enough that a shell never looks hung.
const ANSWER_PATIENCE: Duration = Duration::from_secs(10);

/// The most a hook's body may be. A `PostToolUse` carries the tool's whole
/// output — a file read, a build log — and sixteen MiB is more than any of
/// those and less than a runaway.
const MAX_BODY: usize = 16 * 1024 * 1024;

/// The most a request's head may be before it is refused.
const MAX_HEAD: usize = 64 * 1024;

/// How long a connection may take to say what it came to say.
const PATIENCE: Duration = Duration::from_secs(5);

/// How often the listener looks for a connection, and for being told to
/// stop.
const TICK: Duration = Duration::from_millis(50);

/// What the bytes read so far amount to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Not all of it has arrived yet.
    Incomplete,
    /// A whole request.
    Request {
        /// Which door it came to — `/hook` for a session's hooks,
        /// `/command` for the `guild` command. M13 task 1.
        ///
        /// **Read rather than guessed from the body.** One socket answers
        /// two kinds of caller and they want different things: a hook
        /// posts and wants nothing back, a command asks and waits. The
        /// path is what a request already carries to say so, and the
        /// parser used to read the method and the length and throw it
        /// away.
        path: String,
        /// Its body: the hook's JSON.
        body: Vec<u8>,
        /// How many bytes the request took.
        used: usize,
    },
    /// Refused, with the status that says why.
    Bad(u16),
}

/// Read a request out of what has arrived.
///
/// Pure, so that every way a request can be cut or malformed is a test
/// rather than a socket. The method is checked before the length, so a
/// `GET` is `405` whatever else it says.
pub fn parse(buf: &[u8]) -> Parsed {
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return if buf.len() > MAX_HEAD {
            Parsed::Bad(431)
        } else {
            Parsed::Incomplete
        };
    };
    let head = String::from_utf8_lossy(&buf[..head_end]);
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap_or_default().split(' ');
    if start.next() != Some("POST") {
        return Parsed::Bad(405);
    }
    // A request line is `POST <path> HTTP/1.1`; a client that sends no
    // path at all is answered as the root, which no door of ours is.
    let path = start.next().unwrap_or("/").to_owned();

    let mut length = None;
    for (name, value) in lines.filter_map(|line| line.split_once(':')) {
        if name.trim().eq_ignore_ascii_case("content-length") {
            match value.trim().parse::<usize>() {
                Ok(bytes) => length = Some(bytes),
                Err(_) => return Parsed::Bad(400),
            }
        }
    }
    let Some(length) = length else {
        return Parsed::Bad(411);
    };
    if length > MAX_BODY {
        return Parsed::Bad(413);
    }

    let body_at = head_end + 4;
    if buf.len() < body_at + length {
        return Parsed::Incomplete;
    }
    Parsed::Request {
        path,
        body: buf[body_at..body_at + length].to_vec(),
        used: body_at + length,
    }
}

/// Hand a command to the window and write back what it says — M13.
///
/// **The window answers, not this thread.** Everything a command asks
/// for is a thing the window does on its own clock — decision 4 of M13's
/// plan — so the work happens there and this holds the line. A window
/// that is gone, or that drops the sender without answering, becomes a
/// timeout: a shell that waits for ever is worse than one told nobody
/// replied.
fn answer_command(stream: &mut UnixStream, body: &[u8], commands: &mpsc::Sender<Asked>) {
    let command = match serde_json::from_slice::<Command>(body) {
        Ok(command) => command,
        // A command this window does not know is a newer `guild` talking
        // to an older window, and saying so is more use than `400`.
        Err(err) => {
            said(
                stream,
                400,
                &Answer::Refused {
                    because: format!("this window does not know that command: {err}"),
                },
            );
            return;
        }
    };

    let (answering, answered) = mpsc::channel();
    if commands
        .send(Asked {
            command,
            answer: answering,
        })
        .is_err()
    {
        said(
            stream,
            503,
            &Answer::Refused {
                because: "the window is not listening".to_owned(),
            },
        );
        return;
    }

    match answered.recv_timeout(ANSWER_PATIENCE) {
        Ok(answer) => said(stream, 200, &answer),
        // **Not *failed*: unanswered.** A command that waits on git — a
        // worktree on a large repository — may still finish after the
        // line is dropped, and a caller told it failed would try again
        // into a name that now exists.
        Err(_) => said(
            stream,
            504,
            &Answer::Refused {
                because: "the window did not answer in time — what was asked may still be \
                          happening; `guild worktree list` will show it"
                    .to_owned(),
            },
        ),
    }
}

/// Write an answer as JSON, with the status that frames it.
fn said(stream: &mut UnixStream, status: u16, answer: &Answer) {
    let body = serde_json::to_vec(answer).unwrap_or_else(|_| b"{}".to_vec());
    let head = format!(
        "HTTP/1.1 {status} \r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

/// Whether a complete head asks to be told to go on before the body.
fn expects_continue(buf: &[u8]) -> bool {
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return false;
    };
    String::from_utf8_lossy(&buf[..head_end])
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .any(|(name, value)| {
            name.trim().eq_ignore_ascii_case("expect")
                && value.trim().eq_ignore_ascii_case("100-continue")
        })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The socket the hooks post to, and what they have said.
///
/// Dropping it stops the listener and removes the socket file, so a
/// session started after the window has gone finds no socket and its
/// script succeeds at doing nothing.
pub struct Receiver {
    path: PathBuf,
    heard: mpsc::Receiver<(Instant, Value)>,
    usage: mpsc::Receiver<Value>,
    asked: mpsc::Receiver<Asked>,
    stop: Arc<AtomicBool>,
}

/// A command waiting for the window, and the way back to whoever asked.
///
/// **The connection is still open while this waits.** `guild` is a shell
/// command: it asked a question and is holding the line for the answer,
/// so the window must send one — dropping the sender is itself an answer
/// the connection can read, and it becomes a timeout rather than a
/// silence.
pub struct Asked {
    /// What was asked.
    pub command: Command,
    /// Where the answer goes.
    pub answer: mpsc::Sender<Answer>,
}

impl Receiver {
    /// Listen at `path`, readable and writable by the person alone.
    ///
    /// A file already there is replaced: it can only be a socket left by a
    /// window that crashed with this pid, and a window that will not start
    /// its hooks over a dead file is one whose tabs never say anything.
    pub fn start(path: &Path) -> io::Result<Self> {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;

        let (sender, heard) = mpsc::channel();
        let (usage_sender, usage) = mpsc::channel();
        let (commands, asked) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let senders = Senders {
            hooks: sender,
            usage: usage_sender,
            commands,
        };
        std::thread::Builder::new()
            .name("guild-hooks".into())
            .spawn(move || listen(&listener, &senders, &stopping))?;

        Ok(Self {
            path: path.to_path_buf(),
            heard,
            usage,
            asked,
            stop,
        })
    }

    /// Where it listens: what a session is given in `GUILD_HOOK_SOCKET`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every payload heard since the last look. Never waits.
    pub fn poll(&self) -> Vec<Value> {
        self.poll_observed()
            .into_iter()
            .map(|(_, payload)| payload)
            .collect()
    }

    /// Payloads with their local receive time, for ordering against terminal input.
    pub fn poll_observed(&self) -> Vec<(Instant, Value)> {
        self.heard.try_iter().collect()
    }

    /// Every status-line payload posted since the last look. Never waits.
    pub fn usage(&self) -> Vec<Value> {
        self.usage.try_iter().collect()
    }

    /// The commands waiting to be answered — M13 task 1.
    ///
    /// Taken on the same clock as the hooks, and each one carries the
    /// way back: whoever asked is holding an open connection, so every
    /// `Asked` that comes out of here must be answered or dropped, and
    /// dropping it is what the far end reads as a timeout.
    pub fn commands(&self) -> Vec<Asked> {
        self.asked.try_iter().collect()
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Where each door's requests go.
#[derive(Clone)]
struct Senders {
    hooks: mpsc::Sender<(Instant, Value)>,
    usage: mpsc::Sender<Value>,
    commands: mpsc::Sender<Asked>,
}

fn listen(listener: &UnixListener, senders: &Senders, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let senders = senders.clone();
                let spawned = std::thread::Builder::new()
                    .name("guild-hook".into())
                    .spawn(move || serve(stream, &senders));
                if let Err(err) = spawned {
                    tracing::warn!(%err, "a hook's connection could not be served");
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(TICK),
            Err(err) => {
                tracing::warn!(%err, "the hooks' socket failed to accept");
                std::thread::sleep(TICK);
            }
        }
    }
}

fn serve(mut stream: UnixStream, senders: &Senders) {
    // On macOS an accepted socket inherits the listener's non-blocking
    // flag, and a read on it answers with whatever has arrived — the trap
    // the IDE server's handshake fell into first.
    if stream.set_nonblocking(false).is_err() || stream.set_read_timeout(Some(PATIENCE)).is_err() {
        return;
    }
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut continued = false;
    loop {
        match parse(&buf) {
            Parsed::Request { path, body, .. } => {
                if path == COMMAND_PATH {
                    answer_command(&mut stream, &body, &senders.commands);
                    return;
                }
                match serde_json::from_slice::<Value>(&body) {
                    Ok(payload) => {
                        if path == USAGE_PATH {
                            let _ = senders.usage.send(payload);
                        } else {
                            let _ = senders.hooks.send((Instant::now(), payload));
                        }
                        reply(&mut stream, 204);
                    }
                    Err(_) => reply(&mut stream, 400),
                }
                return;
            }
            Parsed::Bad(status) => {
                reply(&mut stream, status);
                return;
            }
            Parsed::Incomplete => {
                // The script clears `Expect:`; a client that sent it anyway
                // would otherwise wait a second for nothing.
                if !continued && expects_continue(&buf) {
                    continued = true;
                    if stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
                        return;
                    }
                }
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => buf.extend_from_slice(&chunk[..read]),
        }
    }
}

fn reply(stream: &mut UnixStream, status: u16) {
    let reason = match status {
        204 => "No Content",
        400 => "Bad Request",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
}

/// Take away the sockets of Guild windows that are no longer running.
///
/// The IDE lock sweep's rule: only ours — `guild-hooks-<pid>.sock` — and
/// only those whose pid does not answer. `Drop` does not run for a window
/// that was killed, and a pile of dead sockets in `$TMPDIR` is litter a
/// person finds and wonders about.
pub fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("guild-hooks-"))
            .and_then(|rest| rest.strip_suffix(".sock"))
            .and_then(|pid| pid.parse::<i32>().ok())
        else {
            continue;
        };
        if crate::ide::alive(pid) {
            continue;
        }
        tracing::info!(socket = %entry.path().display(), "a socket left by a window that is gone");
        let _ = std::fs::remove_file(entry.path());
    }
}

#[cfg(test)]
pub(crate) fn command_batch_fixture(directory: &Path, batch: Vec<Asked>) -> Receiver {
    let mut receiver = Receiver::start(&directory.join("batch.sock")).unwrap();
    let (send, receive) = mpsc::channel();
    for asked in batch {
        send.send(asked).unwrap();
    }
    receiver.asked = receive;
    receiver
}
