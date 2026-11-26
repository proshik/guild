//! Guild as `claude`'s IDE — the lock file it is found by, the words the
//! protocol answers in, and the tools it says it has.
//!
//! What a real `claude` says is in task 0 of the M8.6 plan, recorded
//! rather than assumed; these are the parts that can be checked without
//! one.

use std::path::PathBuf;

use crate::scratch::Scratch;
use guild_desktop::ide;

/// The lock file is how a CLI finds us at all: the port it opens, the
/// token it must present, and the folders it matches its own directory
/// against.
#[test]
fn the_lock_file_says_where_and_who() {
    let folders = vec![PathBuf::from("/work/one"), PathBuf::from("/work/two")];

    let lock = ide::lock_json(4242, &folders, "a-token");

    assert_eq!(lock["port"], 4242);
    assert_eq!(lock["ideName"], "Guild");
    assert_eq!(lock["transport"], "ws");
    assert_eq!(lock["authToken"], "a-token");
    assert_eq!(lock["runningInWindows"], false);
    assert_eq!(
        lock["workspaceFolders"],
        serde_json::json!(["/work/one", "/work/two"]),
        "a window is one IDE with as many projects as it has open"
    );
    assert!(
        lock["pid"].as_u64().is_some(),
        "the CLI checks that whoever wrote this is still alive"
    );
}

/// It is looked for by name in a directory the CLI reads.
#[test]
fn the_lock_is_named_by_its_port() {
    let path = ide::lock_path(4242);

    assert!(path.ends_with("4242.lock"));
    assert!(path.starts_with(ide::lock_directory()));
    assert!(ide::lock_directory().ends_with(".claude/ide"));
}

/// **The two words the protocol turns on**, recorded from a real
/// session: one says the person applied the edit — which the CLI then
/// writes itself, M8.6 task 5 — and the other that they refused it.
#[test]
fn the_two_answers_are_the_recorded_ones() {
    assert_eq!(ide::ACCEPTED, "FILE_SAVED");
    assert_eq!(ide::REJECTED, "DIFF_REJECTED");
}

/// **Apply is answered with the text it showed, as a second block** —
/// M5.12, decision 15. `claude` reads `e[1].text` of a `FILE_SAVED`
/// answer and re-derives the edit from it, so a one-block answer throws
/// inside the CLI and leaves its terminal asking the question the person
/// has already answered. The text goes back byte for byte: it *is* the
/// edit.
#[test]
fn an_accepted_diff_carries_the_text_it_showed() {
    for text in [
        "fn main() {}",
        "alpha\nbeta\n",
        "",
        "  indented\r\nwith CRLF and ünïcode\n",
    ] {
        let answer = ide::accepted(text);
        let blocks = answer["content"].as_array().expect("content blocks");
        assert_eq!(blocks.len(), 2, "the word and the text: {answer}");
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], ide::ACCEPTED, "the word comes first");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(
            blocks[1]["text"].as_str(),
            Some(text),
            "the text second, byte for byte — an emptied file is a proposal too"
        );
    }
}

/// **A proposal without its text is not applied as an empty file.** The
/// text Apply sends back *is* the edit, so a call that carried none, read
/// as `""`, would tell the CLI to empty the file. An emptied file is still
/// a proposal — the field is there and says so; a missing field, or one
/// that is not a string, is no text at all.
#[test]
fn a_proposal_is_read_only_from_the_text_it_carries() {
    use serde_json::json;
    assert_eq!(
        ide::proposed(&json!({"new_file_path": "/a", "new_file_contents": "x\n"})),
        Some("x\n")
    );
    assert_eq!(
        ide::proposed(&json!({"new_file_path": "/a", "new_file_contents": ""})),
        Some(""),
        "an emptied file is a proposal too"
    );
    assert_eq!(ide::proposed(&json!({"new_file_path": "/a"})), None);
    assert_eq!(
        ide::proposed(&json!({"new_file_path": "/a", "new_file_contents": null})),
        None
    );
    assert_eq!(
        ide::proposed(&json!({"new_file_path": "/a", "new_file_contents": 5})),
        None
    );
}

/// An answer is a content block, and ours are text — which is how the
/// CLI reads them, JSON included.
#[test]
fn an_answer_is_a_text_content_block() {
    let plain = ide::text_result("done");
    assert_eq!(plain["content"][0]["type"], "text");
    assert_eq!(plain["content"][0]["text"], "done");

    let carried = ide::json_result(&serde_json::json!({"a": 1}));
    let text = carried["content"][0]["text"].as_str().expect("text");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(text).expect("json"),
        serde_json::json!({"a": 1}),
        "JSON travels inside the text block, as the CLI's own tools do it"
    );
}

/// **Only what the window can do.** A tool named and then refused is
/// worse than one never offered: the agent plans around what it is told
/// it has.
#[test]
fn the_tools_are_the_ones_the_window_can_answer() {
    let tools = ide::tools();
    let names: Vec<&str> = tools
        .as_array()
        .expect("a list")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();

    for wanted in [
        "openDiff",
        "close_tab",
        "closeAllDiffTabs",
        "getDiagnostics",
        "getWorkspaceFolders",
        "getOpenEditors",
        "checkDocumentDirty",
        "saveDocument",
    ] {
        assert!(names.contains(&wanted), "{wanted} is not offered");
    }

    for tool in tools.as_array().expect("a list") {
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{} has no description, and the agent reads them",
            tool["name"]
        );
        assert_eq!(
            tool["inputSchema"]["type"], "object",
            "{}'s schema is not an object",
            tool["name"]
        );
    }
}

/// A file the agent names is one of the window's own, or it is nothing to
/// do with us.
#[test]
fn a_path_belongs_to_a_folder_or_it_does_not() {
    let folders = vec![PathBuf::from("/work/one")];

    assert!(ide::inside(
        &folders,
        std::path::Path::new("/work/one/src/lib.rs")
    ));
    assert!(!ide::inside(
        &folders,
        std::path::Path::new("/work/two/lib.rs")
    ));
    assert!(!ide::inside(&[], std::path::Path::new("/work/one/lib.rs")));
}

/// The socket, end to end and without a `claude`: the token is checked,
/// the opening two calls are answered by the connection itself, and a
/// tool call becomes something the window answers **later**.
///
/// The lateness is the point. `openDiff` is answered by a person minutes
/// after it arrives, so the connection cannot be a request-answer loop —
/// and a test that asks and reads in one breath would pass over a server
/// that could only ever answer at once.
#[test]
fn a_connection_is_checked_answered_and_forgotten() {
    // A directory nothing is working in: `claude` matches its own
    // against `workspaceFolders`, so a real session on this machine
    // cannot mistake this test's socket for its window.
    let folder = Scratch::new("guild-ide-");
    let server = ide::server::Server::start(&[folder.to_path_buf()]).expect("a socket and a lock");
    let lock = ide::lock_path(server.port);
    assert!(lock.is_file(), "the lock file is how the CLI finds us");

    assert!(
        speak(server.port, "not-the-token").is_err(),
        "a connection presenting the wrong token is refused"
    );

    let mut socket = speak(server.port, server.token()).expect("the handshake");
    let opened = ask(&mut socket, 1, "initialize", serde_json::json!({}));
    assert_eq!(opened["result"]["serverInfo"]["name"], "Guild");

    let listed = ask(&mut socket, 2, "tools/list", serde_json::json!({}));
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("a list of tools")
        .len();
    assert_eq!(
        tools,
        ide::tools().as_array().expect("a list").len(),
        "what the socket lists is the window's own table"
    );

    // A tool call reaches the window rather than being answered here.
    send(
        &mut socket,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "getWorkspaceFolders", "arguments": {}},
        }),
    );
    let (connection, id) = until(|| {
        server.poll().into_iter().find_map(|event| match event {
            ide::server::Event::Ask {
                connection,
                id,
                tool,
                ..
            } if tool == "getWorkspaceFolders" => Some((connection, id)),
            _ => None,
        })
    })
    .expect("the window hears the call");

    server.answer(connection, id, ide::text_result("two projects"));
    let answered = read(&mut socket).expect("the answer");
    assert_eq!(answered["id"], 3);
    assert_eq!(answered["result"]["content"][0]["text"], "two projects");

    drop(server);
    assert!(
        !lock.exists(),
        "a lock naming a port nothing listens on is a CLI that fails to connect and says nothing"
    );
}

/// How long a test waits for another thread. A clock rather than a count
/// of iterations: a hundred short sleeps is two seconds on an idle
/// machine and much less under a full `just check`.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Wait for something to happen on another thread.
fn until<T>(mut look: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(found) = look() {
            return Some(found);
        }
        if std::time::Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Connect as the CLI does: the `mcp` subprotocol and the token header.
fn speak(
    port: u16,
    token: &str,
) -> Result<tungstenite::WebSocket<std::net::TcpStream>, tungstenite::Error> {
    let stream =
        std::net::TcpStream::connect(("127.0.0.1", port)).expect("the socket is listening");
    stream
        .set_read_timeout(Some(PATIENCE))
        .expect("a read timeout");
    let request = tungstenite::http::Request::builder()
        .uri(format!("ws://127.0.0.1:{port}/"))
        .header("Host", format!("127.0.0.1:{port}"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Protocol", "mcp")
        .header("x-claude-code-ide-authorization", token)
        .body(())
        .expect("a request");
    tungstenite::client::client(request, stream)
        .map(|(socket, _response)| socket)
        .map_err(|err| match err {
            tungstenite::HandshakeError::Failure(err) => err,
            tungstenite::HandshakeError::Interrupted(_) => tungstenite::Error::ConnectionClosed,
        })
}

fn send(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, message: serde_json::Value) {
    socket
        .send(tungstenite::Message::Text(message.to_string().into()))
        .expect("the message goes");
}

fn read(socket: &mut tungstenite::WebSocket<std::net::TcpStream>) -> Option<serde_json::Value> {
    loop {
        match socket.read().ok()? {
            tungstenite::Message::Text(text) => return serde_json::from_str(&text).ok(),
            tungstenite::Message::Close(_) => return None,
            _ => {}
        }
    }
}

fn ask(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    send(
        socket,
        serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
    );
    let answer = read(socket).expect("an answer");
    assert_eq!(
        answer["id"], id,
        "an answer carries the id it was asked with"
    );
    answer
}

/// A window that was killed rather than quit leaves its lock behind, and
/// `--ide` means *if exactly one valid IDE is available* — so a pile of
/// dead locks is a live window nobody can connect to.
#[test]
fn a_lock_is_swept_only_when_it_is_ours_and_dead() {
    let mine = serde_json::json!({"ideName": "Guild", "pid": std::process::id()});
    assert!(!ide::stale(&mine), "this very process is running");

    // A pid that has certainly gone: one we started and waited for.
    let child = std::process::Command::new("true")
        .spawn()
        .expect("a short-lived process");
    let gone = child.id();
    let mut child = child;
    child.wait().expect("it ends at once");
    let dead = serde_json::json!({"ideName": "Guild", "pid": gone});
    assert!(ide::stale(&dead), "a window that is not there any more");

    let theirs = serde_json::json!({"ideName": "Another Editor", "pid": gone});
    assert!(
        !ide::stale(&theirs),
        "another editor's lock is not ours to remove, dead or alive"
    );
}
