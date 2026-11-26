//! The socket `claude` connects to, and the threads behind it.
//!
//! A listener thread accepts connections; each connection is a thread
//! that reads frames and writes whatever the window hands it. The window
//! hears about all of it on the watch loop's clock — the git worker's
//! shape, for the fourth time in this crate.
//!
//! **A call may be answered much later.** `openDiff` does not return
//! until a person accepts or rejects, so the connection's thread cannot
//! be a request-answer loop: it reads without blocking, writes whatever
//! has been queued for it, and sleeps.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use serde_json::{Value, json};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{Message, WebSocket};

use super::{lock_json, lock_path};

/// How long a connection's thread sleeps between looks. A frame: nothing
/// here is faster than the window redraws.
const LOOK: Duration = Duration::from_millis(16);

/// How long a handshake may take before the connection is given up on.
/// Generous: it is a local socket, and the only thing this guards against
/// is a client that connects and then says nothing at all.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// What the window hears from the socket.
#[derive(Debug)]
pub enum Event {
    /// A CLI has connected, and says which process it is.
    Connected {
        /// Which connection, for the answer to go back to.
        connection: u64,
        /// The CLI's own pid, from its `ide_connected` notification.
        pid: Option<u32>,
    },
    /// It has asked for something. **Every one of these must be
    /// answered**, or the agent waits for ever.
    Ask {
        /// Which connection asked.
        connection: u64,
        /// The JSON-RPC id to answer with.
        id: Value,
        /// The tool's name.
        tool: String,
        /// Its arguments.
        arguments: Value,
    },
    /// It has gone.
    Gone {
        /// Which connection.
        connection: u64,
    },
}

/// What the window sends back.
#[derive(Debug)]
pub enum Answer {
    /// The result of a call, by its id.
    Result {
        /// Which connection.
        connection: u64,
        /// The id it asked with.
        id: Value,
        /// What it gets.
        result: Value,
    },
}

/// The listening socket, the lock file, and the connections.
pub struct Server {
    /// The port the lock names.
    pub port: u16,
    /// What the CLI must present in its header.
    token: String,
    lock: PathBuf,
    /// The projects the lock file currently names.
    folders: Vec<PathBuf>,
    /// What the connections have to say.
    events: Receiver<Event>,
    /// Where an answer goes, per connection.
    answers: Sender<Answer>,
    /// Cleared when the window stops the server, which is how the
    /// threads learn to end.
    running: Arc<AtomicBool>,
}

impl Server {
    /// Start listening, and announce it.
    ///
    /// The port is the OS's: a fixed one is a port already in use on
    /// somebody's machine, and the lock file is how it is found anyway.
    pub fn start(folders: &[PathBuf]) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;

        // Before ours is written: the directory is where `claude`
        // decides how many IDEs there are, and a dead one counts.
        super::sweep();

        let token = uuid::Uuid::new_v4().to_string();
        let lock = lock_path(port);
        if let Some(directory) = lock.parent() {
            std::fs::create_dir_all(directory)?;
        }
        std::fs::write(&lock, lock_json(port, folders, &token).to_string())?;

        let (to_window, events) = channel::<Event>();
        let (answers, from_window) = channel::<Answer>();
        let running = Arc::new(AtomicBool::new(true));

        let accepting = (
            token.clone(),
            to_window,
            running.clone(),
            std::sync::Mutex::new(from_window),
        );
        std::thread::Builder::new()
            .name("ide-accept".to_owned())
            .spawn(move || accept_loop(listener, accepting))?;

        tracing::info!(port, lock = %lock.display(), "Guild is announcing itself as an IDE");
        Ok(Self {
            port,
            token,
            lock,
            folders: folders.to_vec(),
            events,
            answers,
            running,
        })
    }

    /// Say which projects this window has open, when that has changed.
    ///
    /// **The lock file goes stale otherwise.** A `claude` started by hand
    /// with `--ide` matches its own directory against `workspaceFolders`,
    /// so a project added after the window opened would not find us —
    /// while our own tabs, pinned to the port, would. Two behaviours from
    /// one window is the kind of difference nobody reports as a bug.
    pub fn announce(&mut self, folders: &[PathBuf]) {
        if self.folders == folders {
            return;
        }
        self.folders = folders.to_vec();
        let lock = lock_json(self.port, folders, &self.token).to_string();
        if let Err(err) = std::fs::write(&self.lock, lock) {
            tracing::warn!(%err, lock = %self.lock.display(), "the IDE lock was not rewritten");
        }
    }

    /// The token a connection must present. Kept for the tests and for
    /// anything that wants to say what it announced.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Everything the connections have said since the last look.
    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Answer a call. **Any call, at any time** — that is the whole
    /// reason the connection's thread does not wait for one.
    pub fn answer(&self, connection: u64, id: Value, result: Value) {
        let _ = self.answers.send(Answer::Result {
            connection,
            id,
            result,
        });
    }
}

impl Drop for Server {
    /// **The lock file goes with the window.** It names a port nothing is
    /// listening on the moment we are gone, and `claude` then fails to
    /// connect with no explanation at all.
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Err(err) = std::fs::remove_file(&self.lock)
            && err.kind() != ErrorKind::NotFound
        {
            tracing::warn!(%err, lock = %self.lock.display(), "the IDE lock was not removed");
        }
    }
}

type Accepting = (
    String,
    Sender<Event>,
    Arc<AtomicBool>,
    std::sync::Mutex<Receiver<Answer>>,
);

/// The queue of each live connection, by its number: the one channel the
/// window writes to is fanned out into these.
type Fan = Arc<std::sync::Mutex<Vec<(u64, Sender<Answer>)>>>;

/// Take connections until the window stops.
fn accept_loop(listener: TcpListener, accepting: Accepting) {
    let (token, to_window, running, from_window) = accepting;
    let next = AtomicU64::new(1);
    // One queue per connection, fed by the one the window writes to.
    let fan: Fan = Arc::new(std::sync::Mutex::new(Vec::new()));

    {
        let fan = fan.clone();
        let running = running.clone();
        std::thread::Builder::new()
            .name("ide-fan".to_owned())
            .spawn(move || {
                while running.load(Ordering::Acquire) {
                    let answer = match from_window.lock() {
                        Ok(queue) => queue.try_recv(),
                        Err(_) => return,
                    };
                    match answer {
                        Ok(Answer::Result {
                            connection,
                            id,
                            result,
                        }) => {
                            if let Ok(fan) = fan.lock()
                                && let Some((_, sender)) =
                                    fan.iter().find(|(at, _)| *at == connection)
                            {
                                let _ = sender.send(Answer::Result {
                                    connection,
                                    id,
                                    result,
                                });
                            }
                        }
                        Err(TryRecvError::Empty) => std::thread::sleep(LOOK),
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
            })
            .ok();
    }

    while running.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let connection = next.fetch_add(1, Ordering::Relaxed);
                let (mine, ours) = channel::<Answer>();
                if let Ok(mut fan) = fan.lock() {
                    fan.push((connection, mine));
                }
                let token = token.clone();
                let to_window = to_window.clone();
                let running = running.clone();
                std::thread::Builder::new()
                    .name(format!("ide-{connection}"))
                    .spawn(move || {
                        serve(stream, connection, &token, &to_window, &ours, &running);
                        let _ = to_window.send(Event::Gone { connection });
                    })
                    .ok();
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => std::thread::sleep(LOOK),
            Err(err) => {
                tracing::warn!(%err, "the IDE socket stopped accepting");
                return;
            }
        }
    }
}

/// One connection, from the handshake to the end.
///
/// The `allow` is for the handshake callback below: its `Err` type is
/// `tungstenite`'s own `ErrorResponse`, which is a whole HTTP response
/// and is the only thing that signature accepts.
#[allow(clippy::result_large_err)]
fn serve(
    stream: TcpStream,
    connection: u64,
    token: &str,
    to_window: &Sender<Event>,
    answers: &Receiver<Answer>,
    running: &AtomicBool,
) {
    let expected = token.to_owned();
    let checked = tungstenite::accept_hdr(stream, |request: &Request, mut response: Response| {
        // **The subprotocol has to be echoed**, or the CLI closes the
        // connection the moment the handshake finishes — task 0 paid for
        // that once.
        if request
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.split(',').any(|one| one.trim() == "mcp"))
        {
            response
                .headers_mut()
                .insert("sec-websocket-protocol", "mcp".parse().expect("a header"));
        }
        let presented = request
            .headers()
            .get("x-claude-code-ide-authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if presented == expected {
            Ok(response)
        } else {
            tracing::warn!("an IDE connection presented the wrong token");
            Err(ErrorResponse::new(Some("unauthorised".to_owned())))
        }
    });

    // **An accepted socket inherits the listener's non-blocking flag** on
    // macOS, so the handshake reads what has arrived and asks to be
    // resumed for the rest. Treating that as a failure loses the
    // connection about one time in four — and what a person sees is a
    // session that drew its diff in the terminal instead of in a tab,
    // with nothing anywhere saying why.
    let until = std::time::Instant::now() + HANDSHAKE;
    let mut attempt = checked;
    let mut socket = loop {
        match attempt {
            Ok(socket) => break socket,
            Err(tungstenite::HandshakeError::Interrupted(midway)) => {
                if std::time::Instant::now() > until {
                    tracing::warn!("the IDE handshake never finished");
                    return;
                }
                std::thread::sleep(LOOK);
                attempt = midway.handshake();
            }
            Err(tungstenite::HandshakeError::Failure(err)) => {
                tracing::warn!(%err, "the IDE handshake failed");
                return;
            }
        }
    };
    if socket.get_ref().set_nonblocking(true).is_err() {
        return;
    }

    while running.load(Ordering::Acquire) {
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Some(event) = read(connection, &text, &mut socket)
                    && to_window.send(event).is_err()
                {
                    return;
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => {
                tracing::debug!(%err, "the IDE connection ended");
                return;
            }
        }

        while let Ok(Answer::Result { id, result, .. }) = answers.try_recv() {
            let message = json!({"jsonrpc": "2.0", "id": id, "result": result});
            tracing::debug!(%id, "answering the CLI");
            if let Err(err) = socket.send(Message::Text(message.to_string().into())) {
                tracing::warn!(%err, %id, "the answer did not reach the CLI");
                return;
            }
        }
        std::thread::sleep(LOOK);
    }
}

/// One message. What the connection can answer for itself it answers;
/// what the window has to answer becomes an event.
fn read(connection: u64, text: &str, socket: &mut WebSocket<TcpStream>) -> Option<Event> {
    let message: Value = serde_json::from_str(text)
        .inspect_err(|err| tracing::warn!(%err, "the CLI said something that is not JSON"))
        .ok()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();
    tracing::debug!(%method, "the CLI said something");

    let reply = |socket: &mut WebSocket<TcpStream>, result: Value| {
        if let Some(id) = id.clone() {
            let message = json!({"jsonrpc": "2.0", "id": id, "result": result});
            let _ = socket.send(Message::Text(message.to_string().into()));
        }
    };

    match method {
        // The opening two, which need nothing from the window.
        "initialize" => {
            reply(
                socket,
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "Guild", "version": env!("CARGO_PKG_VERSION")}
                }),
            );
            None
        }
        "tools/list" => {
            reply(socket, json!({"tools": super::tools()}));
            None
        }
        "ide_connected" => Some(Event::Connected {
            connection,
            pid: message
                .get("params")
                .and_then(|params| params.get("pid"))
                .and_then(Value::as_u64)
                .map(|pid| pid as u32),
        }),
        "tools/call" => {
            let params = message.get("params")?;
            Some(Event::Ask {
                connection,
                id: id?,
                tool: params.get("name").and_then(Value::as_str)?.to_owned(),
                arguments: params.get("arguments").cloned().unwrap_or(json!({})),
            })
        }
        "notifications/initialized" | "notifications/cancelled" => None,
        other => {
            // **Answered rather than ignored.** A request with an id that
            // never comes back is an agent waiting for ever.
            tracing::debug!(%other, "the CLI asked for something we do not have");
            if id.is_some() {
                reply(socket, json!({}));
            }
            None
        }
    }
}
