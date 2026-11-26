//! What goes down the pipe to a language server, and what comes back.
//!
//! The framing is a pure function over bytes and is checked here without
//! a process; the client itself is checked against
//! `tests/fixtures/fake-lsp.py`, which answers in milliseconds and
//! indexes nothing.

use crate::child_lifetime;

use std::io::Cursor;

use guild_desktop::lsp;

/// A message is its body with a `Content-Length` in front, and what is
/// written is what is read.
#[test]
fn a_message_goes_out_and_comes_back() {
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
    let framed = lsp::frame(body);

    assert!(
        framed.starts_with(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()),
        "the header counts the body's bytes"
    );

    let mut stream = Cursor::new(framed);
    let body = lsp::read_message(&mut stream)
        .expect("read")
        .expect("a message");

    assert_eq!(body, br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
}

/// Servers put other headers in — `Content-Type` is in the protocol — and
/// a reader that insists on exactly one header is a reader that works
/// until it meets a server.
#[test]
fn other_headers_are_allowed() {
    let mut stream = Cursor::new(
        b"Content-Length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}"
            .to_vec(),
    );

    assert_eq!(
        lsp::read_message(&mut stream)
            .expect("read")
            .expect("a message"),
        b"{}"
    );
}

/// Answers arrive in one read as often as not, and the reader must take
/// exactly one message and leave the next where it is.
#[test]
fn two_messages_in_one_breath_are_read_one_at_a_time() {
    let mut both = lsp::frame(b"{\"a\":1}");
    both.extend(lsp::frame(b"{\"b\":2}"));
    let mut stream = Cursor::new(both);

    assert_eq!(
        lsp::read_message(&mut stream).unwrap().unwrap(),
        b"{\"a\":1}"
    );
    assert_eq!(
        lsp::read_message(&mut stream).unwrap().unwrap(),
        b"{\"b\":2}"
    );
    assert!(
        lsp::read_message(&mut stream).unwrap().is_none(),
        "and then the stream is over"
    );
}

/// **355 KB in one message** is not a hypothetical: task 0 measured
/// rust-analyzer's answer to a completion at that size. A reader written
/// for small bodies would work until the first completion.
#[test]
fn a_very_large_message_is_read_whole() {
    let body = format!(r#"{{"items":"{}"}}"#, "x".repeat(400_000));
    let mut stream = Cursor::new(lsp::frame(body.as_bytes()));

    let read = lsp::read_message(&mut stream).unwrap().unwrap();

    assert_eq!(read.len(), body.len());
}

/// A server that closes is not an error — it is a server that stopped.
/// A server that closes **mid-message** is, because half a message is
/// not something to carry on from.
#[test]
fn the_end_of_the_stream_is_told_from_a_half_message() {
    let mut clean = Cursor::new(Vec::new());
    assert!(lsp::read_message(&mut clean).unwrap().is_none());

    let mut cut = Cursor::new(b"Content-Length: 10\r\n\r\n{}".to_vec());
    assert!(
        lsp::read_message(&mut cut).is_err(),
        "half a body is an error"
    );

    let mut headless = Cursor::new(b"Content-Type: nonsense\r\n\r\n{}".to_vec());
    assert!(
        lsp::read_message(&mut headless).is_err(),
        "a header with no length says nothing about where the body ends"
    );
}

/// The three shapes that come back, told apart by what they carry: an
/// answer has an id and a result, a request has an id and a method, a
/// notification has a method and no id.
#[test]
fn what_comes_back_is_one_of_three_things() {
    let answer = lsp::parse(br#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#);
    assert!(matches!(answer, Some(lsp::Incoming::Answer { id: 7, .. })));

    let failure = lsp::parse(br#"{"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"no"}}"#);
    match failure {
        Some(lsp::Incoming::Answer { id, result }) => {
            assert_eq!(id, 7);
            assert!(result.is_err(), "an error is an answer that failed");
        }
        other => panic!("an error is an answer: {other:?}"),
    }

    let notification =
        lsp::parse(br#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{}}"#);
    assert!(matches!(
        notification,
        Some(lsp::Incoming::Notification { .. })
    ));

    let request = lsp::parse(
        br#"{"jsonrpc":"2.0","id":1,"method":"window/workDoneProgress/create","params":{}}"#,
    );
    assert!(matches!(
        request,
        Some(lsp::Incoming::Request { id: 1, .. })
    ));
}

/// A body that is not JSON is dropped rather than taking the client down
/// with it: the server is somebody else's program, and half a protocol
/// is not a reason to lose the window.
#[test]
fn nonsense_is_dropped_rather_than_fatal() {
    assert!(lsp::parse(b"not json at all {").is_none());
    assert!(lsp::parse(br#"{"jsonrpc":"2.0"}"#).is_none());
}

/// What we send: a request carries an id, a notification does not, and
/// both name the version the protocol asks for.
#[test]
fn what_we_send_has_the_shape_the_protocol_asks_for() {
    // Framed, as it goes down the pipe, so the test reads it back the
    // way the server would.
    let request = lsp::request(3, "textDocument/hover", serde_json::json!({"a": 1}));
    let body = lsp::read_message(&mut Cursor::new(request))
        .unwrap()
        .unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(sent["jsonrpc"], "2.0");
    assert_eq!(sent["id"], 3);
    assert_eq!(sent["method"], "textDocument/hover");

    let note = lsp::notification("initialized", serde_json::json!({}));
    let body = lsp::read_message(&mut Cursor::new(note)).unwrap().unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(sent["jsonrpc"], "2.0");
    assert!(sent.get("id").is_none(), "a notification has no id");
}

/// The fake server of task 0: it answers in milliseconds, indexes
/// nothing, and is what every test below runs against. A real server
/// belongs in a person's hands, not in `just check`.
fn fake_server() -> (String, Vec<String>) {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-lsp.py");
    (
        "python3".to_owned(),
        vec![script.to_string_lossy().into_owned()],
    )
}

/// Wait for something to arrive, on the clock rather than on a count of
/// tries: a fixed number of sleeps is a budget that holds on an idle
/// machine and fails under `just check`, which is how this project's
/// process tests have failed before.
fn within<T>(mut ask: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(found) = ask() {
            return Some(found);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    ask()
}

/// A server starts, is asked who it is, and answers by the id it was
/// asked with.
#[test]
fn a_server_starts_and_answers_by_id() {
    let (command, args) = fake_server();
    let mut client = lsp::client::Client::start(&command, &args, std::path::Path::new("/"))
        .expect("the fake server starts");

    let id = client.request("initialize", serde_json::json!({"capabilities": {}}));
    let answer = within(|| {
        client.poll();
        id.take_answer()
    })
    .expect("an answer arrives");

    assert_eq!(
        answer.expect("it is not a refusal")["serverInfo"]["name"],
        "fake"
    );
    assert!(client.is_alive(), "and it is still there");
}

/// A notification arrives with no id and reaches whoever is listening —
/// this is how every diagnostic in M8.5 gets to a tab.
#[test]
fn a_notification_arrives_without_being_asked_for() {
    let (command, args) = fake_server();
    let mut client = lsp::client::Client::start(&command, &args, std::path::Path::new("/"))
        .expect("the fake server starts");

    client.notify(
        "textDocument/didOpen",
        serde_json::json!({"textDocument": {
            "uri": "file:///a.rs", "languageId": "rust", "version": 1,
            "text": "fn ok() {}\nlet x = BAD;\n"
        }}),
    );

    let published = within(|| {
        client.poll().into_iter().find_map(|message| match message {
            lsp::Incoming::Notification { method, params }
                if method == "textDocument/publishDiagnostics" =>
            {
                Some(params)
            }
            _ => None,
        })
    })
    .expect("the server publishes");

    assert_eq!(published["uri"], "file:///a.rs");
    assert_eq!(published["diagnostics"][0]["range"]["start"]["line"], 1);
    assert_eq!(
        published["diagnostics"][0]["range"]["start"]["character"],
        8
    );
}

/// An answer nobody is waiting for is dropped rather than kept for ever:
/// a map that only grows is a leak with a slow fuse.
#[test]
fn an_answer_nobody_wants_is_not_kept() {
    let (command, args) = fake_server();
    let mut client = lsp::client::Client::start(&command, &args, std::path::Path::new("/"))
        .expect("the fake server starts");

    let id = client.request("initialize", serde_json::json!({"capabilities": {}}));
    assert!(
        within(|| {
            client.poll();
            id.take_answer()
        })
        .is_some()
    );
    assert!(
        id.take_answer().is_none(),
        "an answer is taken once, by the one waiting for it"
    );
}

/// A server that dies stops being alive, and a waiter finds out rather
/// than waiting for ever. This is the shape of a missing rust-analyzer:
/// task 0 found that the commonest failure is a shim that spawns and
/// exits, not a command that is absent.
#[test]
fn a_server_that_dies_says_so() {
    let (command, args) = fake_server();
    let mut client = lsp::client::Client::start(&command, &args, std::path::Path::new("/"))
        .expect("the fake server starts");

    let id = client.request("initialize", serde_json::json!({"capabilities": {}}));
    assert!(
        within(|| {
            client.poll();
            id.take_answer()
        })
        .is_some()
    );

    client.notify("exit", serde_json::json!({}));

    assert!(
        within(|| {
            client.poll();
            (!client.is_alive()).then_some(())
        })
        .is_some(),
        "the client notices the server has gone"
    );
}

/// A command that is not there is an error at **spawn**, with the name in
/// it — the shape task 0 met when `rust-analyzer` on the PATH turned out
/// to be a shim for a component that was not installed.
#[test]
fn a_server_that_is_not_there_fails_by_name() {
    let started = lsp::client::Client::start(
        "no-such-language-server-anywhere",
        &[],
        std::path::Path::new("/"),
    );
    let Err(error) = started else {
        panic!("a server that is not installed cannot start");
    };

    assert!(
        error
            .to_string()
            .contains("no-such-language-server-anywhere"),
        "the name is in the error: {error}"
    );
}

/// A path becomes a URI and comes back. **Not `format!("file://{path}")`**:
/// a project under `~/My Documents` would give every diagnostic a URI the
/// server cannot match to a file, and the squiggles would simply never
/// arrive — no error, nothing in the log.
#[test]
fn a_path_becomes_a_uri_and_comes_back() {
    let path = std::path::Path::new("/Users/somebody/My Documents/a file.rs");

    let uri = lsp::uri(path);

    assert_eq!(uri, "file:///Users/somebody/My%20Documents/a%20file.rs");
    assert_eq!(lsp::path_of(&uri).as_deref(), Some(path));
}

/// What a server answers with is not always a file on this disk —
/// `jar:` and `https:` are both legal — and a client that assumes
/// otherwise opens a tab called `jar:/rt.jar!/String.class`.
#[test]
fn a_uri_that_is_not_a_file_is_not_a_path() {
    assert!(lsp::path_of("https://docs.rs/std").is_none());
    assert!(lsp::path_of("jar:file:///rt.jar!/String.class").is_none());
    assert_eq!(
        lsp::path_of("file:///plain/path.rs").as_deref(),
        Some(std::path::Path::new("/plain/path.rs"))
    );
}

/// What a server is told about a file. **Full sync**: the whole text
/// every time, because the file is in memory anyway and incremental sync
/// is a second opinion about ranges that has to agree with the first.
#[test]
fn what_a_server_is_told_about_a_file() {
    let opened = lsp::document::opened("file:///a.rs", "rust", 1, "fn main() {}");
    assert_eq!(opened["textDocument"]["languageId"], "rust");
    assert_eq!(opened["textDocument"]["version"], 1);
    assert_eq!(opened["textDocument"]["text"], "fn main() {}");

    let changed = lsp::document::changed("file:///a.rs", 2, "fn main() { 1 }");
    assert_eq!(changed["textDocument"]["version"], 2);
    assert_eq!(
        changed["contentChanges"][0]["text"], "fn main() { 1 }",
        "the whole text, not a range"
    );
    assert!(
        changed["contentChanges"][0].get("range").is_none(),
        "a range here would be a second opinion about where the change is"
    );

    let closed = lsp::document::closed("file:///a.rs");
    assert_eq!(closed["textDocument"]["uri"], "file:///a.rs");
    assert!(closed["textDocument"].get("text").is_none());
}

/// Where a byte offset is, as the protocol counts it.
///
/// **UTF-16 units, not characters and not bytes.** It is the protocol's
/// default and we negotiate nothing else; a line with an emoji in it
/// would otherwise put every ask after that emoji out by one, which
/// reads as "hover works except in that file".
#[test]
fn a_position_is_counted_in_utf16_units() {
    let text = "fn main() {\n    let x = 1;\n}\n";

    assert_eq!(lsp::providers::position_in(text, 0), lsp_position(0, 0));
    assert_eq!(lsp::providers::position_in(text, 12), lsp_position(1, 0));
    assert_eq!(
        lsp::providers::position_in(text, 16),
        lsp_position(1, 4),
        "four spaces of indent"
    );

    // "// 🦀 here": the crab is two UTF-16 units and four bytes, and a
    // character count would say three where the protocol says four.
    let crab = "// 🦀 here";
    let after_crab = "// 🦀".len();
    assert_eq!(
        lsp::providers::position_in(crab, after_crab),
        lsp_position(0, 5),
        "three ASCII plus two units for the crab"
    );
}

fn lsp_position(line: u32, character: u32) -> lsp_types::Position {
    lsp_types::Position::new(line, character)
}

/// A definition comes back in one of three shapes and a server picks
/// whichever it likes: one location, an array of them, or an array of
/// links. A client that reads one shape works until it meets a server
/// that sends another.
#[test]
fn a_definition_is_read_in_all_three_shapes() {
    let range = serde_json::json!({"start": {"line": 4, "character": 0},
                                   "end": {"line": 4, "character": 5}});

    let one = serde_json::json!({"uri": "file:///a.rs", "range": range});
    assert_eq!(lsp::providers::links_in(one).len(), 1);

    let many = serde_json::json!([{"uri": "file:///a.rs", "range": range}]);
    assert_eq!(lsp::providers::links_in(many).len(), 1);

    let links = serde_json::json!([{
        "targetUri": "file:///a.rs",
        "targetRange": range,
        "targetSelectionRange": range
    }]);
    let read = lsp::providers::links_in(links);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].target_range.start.line, 4);

    assert!(
        lsp::providers::links_in(serde_json::Value::Null).is_empty(),
        "null is an answer: there is no definition there"
    );
}

/// **The component's own jump is never where a definition is reached.**
/// It moves the caret to a link's target *in the buffer it is in*,
/// whichever file the link names — so a `cmd`-click on a name defined in
/// `b.rs` put the caret on line 9 of `a.rs`. Every link reaches it held
/// at the place that was asked, where its jump moves nothing, and the
/// file tab sends Guild's own Go to Definition, which is `f12`'s path.
/// The URI and the underline's range are left as they came.
#[test]
fn every_definition_reaches_the_component_as_the_place_it_was_asked() {
    let range = |line: u32, from: u32, to: u32| {
        serde_json::json!({"start": {"line": line, "character": from},
                           "end": {"line": line, "character": to}})
    };
    let links: Vec<lsp_types::LocationLink> = serde_json::from_value(serde_json::json!([
        {
            "originSelectionRange": range(5, 4, 9),
            "targetUri": "file:///p/b.rs",
            "targetRange": range(9, 0, 30),
            "targetSelectionRange": range(9, 7, 12)
        },
        {
            "originSelectionRange": range(5, 4, 10),
            "targetUri": "file:///p/a.rs",
            "targetRange": range(2, 0, 20),
            "targetSelectionRange": range(2, 3, 8)
        }
    ]))
    .expect("two links");
    let asked = lsp_position(5, 7);

    let held = lsp::providers::held_at_asked(links.clone(), asked);

    assert_eq!(held.len(), 2, "every link, the same file's included");
    let here = lsp_types::Range::new(asked, asked);
    for (held, was) in held.iter().zip(&links) {
        assert_eq!(held.target_range, here, "{}", was.target_uri.as_str());
        assert_eq!(held.target_selection_range, here);
        assert_eq!(held.target_uri, was.target_uri, "its own URI");
        assert_eq!(
            held.origin_selection_range, was.origin_selection_range,
            "its own underline"
        );
    }
}

/// **A definition is asked in UTF-16 units and held in characters**, from
/// one offset. The protocol's position goes to the server; the hold is
/// read by the component's `position_to_offset`, which counts characters
/// — after an emoji the two differ, and a hold in the protocol's counting
/// would move the caret off the name before Guild's jump asked from it.
#[test]
fn a_definition_is_asked_in_utf16_units_and_held_in_characters() {
    let text = "fn one() {}\nlet 😀 = name;\n";
    let offset = text.find("name").expect("the name is in the text");

    let (asked, held) = lsp::providers::asked_and_held(text, offset);

    assert_eq!(
        asked,
        lsp_position(1, 9),
        "`let `, two units of emoji and ` = ` are nine UTF-16 units"
    );
    assert_eq!(
        held,
        lsp_position(1, 8),
        "and eight characters, which is what the component counts"
    );

    let plain = "let x = name;\n";
    let (asked, held) = lsp::providers::asked_and_held(plain, plain.find("name").unwrap());
    assert_eq!(asked, held, "without an astral character the two agree");
}

/// A completion is asked for on a letter and on the two ways a member is
/// reached, and not on a space: task 0 measured one answer at 588 ms and
/// 355 KB, and a rule that says yes to everything makes that of every
/// keystroke.
#[test]
fn a_completion_is_triggered_by_what_a_name_is_made_of() {
    assert!(lsp::providers::triggers("a"));
    assert!(lsp::providers::triggers("_"));
    assert!(lsp::providers::triggers("self."));
    assert!(lsp::providers::triggers("String::"));
    assert!(lsp::providers::triggers("->"));
    assert!(!lsp::providers::triggers(" "));
    assert!(!lsp::providers::triggers("\n"));
    assert!(!lsp::providers::triggers(""));
}

/// The protocol counts UTF-16 units; the editor component counts
/// characters. They differ exactly where an astral character is, and the
/// symptom would be a jump that lands a column short in the one file that
/// has an emoji in it.
#[test]
fn a_jump_lands_where_the_component_counts_it() {
    let text = "fn main() {\n    let x = 1;\n}\n";

    assert_eq!(
        lsp::providers::caret_for(text, lsp_position(1, 8)),
        (1, 8),
        "plain text counts the same either way"
    );

    let crabbed = "// 🦀 done\nlet x = 1;\n";
    assert_eq!(
        lsp::providers::caret_for(crabbed, lsp_position(0, 5)),
        (0, 4),
        "five UTF-16 units are four characters when one of them is a crab"
    );
}

#[test]
fn stopping_an_exited_server_also_stops_its_descendants() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let mut descendant = child_lifetime::Descendant::new(dir.path());
    let binary = dir.path().join("server");
    std::fs::write(&binary, format!("#!/bin/sh\n{}", descendant.shell())).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut client = lsp::client::Client::start(binary.to_str().unwrap(), &[], dir.path()).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while client.is_alive() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!client.is_alive(), "the parent closed its stream");
    client.stop();
    client.stop(); // Idempotence must not signal a PID after reaping it.
    descendant.assert_stopped();
}

#[test]
fn client_drop_and_idempotent_stop_own_only_their_group() {
    use crate::lsp_fixture::LspFixture;
    use guild_desktop::lsp::client::Client;
    let root = tempfile::tempdir().unwrap();
    let mut stopped = LspFixture::new();
    let (command, args) = stopped.command(true);
    let mut client = Client::start(&command, &args, root.path()).unwrap();
    stopped.ready();
    let mut unrelated = LspFixture::new();
    let (command, args) = unrelated.command(true);
    let other = Client::start(&command, &args, root.path()).unwrap();
    unrelated.ready();
    client.stop();
    client.stop();
    stopped.assert_stopped();
    assert!(other.is_alive());
    drop(other);
    unrelated.assert_stopped();
}
