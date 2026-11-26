//! The Language Server Protocol: the framing, and the shapes of message.
//!
//! M8.5. What is here is the wire — bytes in, bytes out — and it knows
//! nothing about servers, projects or files; [`client`] runs a process
//! with it, and `root::lsp` decides what to say.
//!
//! **The framing is the protocol's own**: a `Content-Length` header, a
//! blank line, and exactly that many bytes of JSON. Task 0 measured a
//! completion answer at 355 KB, so nothing here may assume a message is
//! small — a reader written for short bodies works until the first
//! completion and then fails in the one place a person is watching.

pub mod client;
pub mod progress;
pub mod providers;

use std::io::{BufRead, Read};

use serde_json::Value;

/// A message, framed as the protocol asks.
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body);
    framed
}

/// Read one message, or `None` when the stream has ended cleanly.
///
/// **The end of the stream and half a message are different things.** A
/// server that exits closes its pipe between messages and that is not an
/// error; a stream that stops inside a header or a body is one, because
/// there is nothing to carry on from.
pub const HEADER_LIMIT: usize = 8 * 1024;
pub const BODY_LIMIT: usize = 16 * 1024 * 1024;
pub fn read_message(stream: &mut impl BufRead) -> std::io::Result<Option<Vec<u8>>> {
    let Some(length) = read_header(stream, || {})? else {
        return Ok(None);
    };
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|e| {
        std::io::Error::new(e.kind(), "LSP stream ended or failed inside bounded body")
    })?;
    Ok(Some(body))
}
pub(crate) fn read_header(
    stream: &mut impl BufRead,
    mut started: impl FnMut(),
) -> std::io::Result<Option<usize>> {
    use std::io::{Error, ErrorKind};
    let invalid = |reason| Error::new(ErrorKind::InvalidData, reason);
    let mut header = Vec::new();
    loop {
        let buffer = stream.fill_buf()?;
        if buffer.is_empty() {
            return if header.is_empty() {
                Ok(None)
            } else {
                Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "LSP stream ended inside header",
                ))
            };
        }
        // Consume incrementally, never allocate a peer-sized unterminated line.
        let byte = buffer[0];
        if header.is_empty() {
            started();
        }
        stream.consume(1);
        if header.len() == HEADER_LIMIT {
            return Err(invalid("LSP header exceeds 8 KiB limit"));
        }
        header.push(byte);
        if header.ends_with(b"\r\n\r\n") || header.ends_with(b"\n\n") {
            break;
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| invalid("LSP header is not UTF-8"))?;
    let mut length = None;
    for line in header.lines().filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("Malformed LSP header"))?;
        if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("Invalid LSP Content-Length decimal"));
            }
            let parsed = value
                .bytes()
                .try_fold(0usize, |n, b| {
                    n.checked_mul(10)?.checked_add((b - b'0') as usize)
                })
                .ok_or_else(|| invalid("LSP Content-Length overflow"))?;
            if parsed > BODY_LIMIT {
                return Err(invalid("LSP body exceeds 16 MiB limit"));
            }
            if length.is_some_and(|old| old != parsed) {
                return Err(invalid("Conflicting LSP Content-Length headers"));
            }
            length = Some(parsed);
        }
    }
    let length = length.ok_or_else(|| invalid("LSP header missing Content-Length"))?;
    Ok(Some(length))
}

/// A request: it has an id, and an answer with that id will come back.
pub fn request(id: u64, method: &str, params: Value) -> Vec<u8> {
    frame(
        serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            .to_string()
            .as_bytes(),
    )
}

/// A notification: no id, and no answer.
pub fn notification(method: &str, params: Value) -> Vec<u8> {
    frame(
        serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params})
            .to_string()
            .as_bytes(),
    )
}

/// What arrives from a server.
///
/// Three shapes, told apart by what they carry rather than by a field
/// naming which they are: that is how the protocol is written.
#[derive(Debug)]
pub enum Incoming {
    /// An answer to something we asked. `result` is `Err` when the
    /// server refused, carrying its own words.
    Answer {
        /// The id we sent.
        id: u64,
        /// What it answered, or why it would not.
        result: Result<Value, String>,
    },
    /// Something the server says without being asked —
    /// `publishDiagnostics`, `$/progress`.
    Notification {
        /// Its name.
        method: String,
        /// Its body.
        params: Value,
    },
    /// Something the server asks **us** — `workspace/configuration`,
    /// `window/workDoneProgress/create`. It waits for an answer, and a
    /// server whose request is never answered sulks in ways that look
    /// like our bug.
    Request {
        /// The id to answer with.
        id: u64,
        /// What it wants.
        method: String,
        /// Its body.
        params: Value,
    },
}

/// Read a message's body, or nothing when it is not one.
///
/// A body that is not JSON, or that carries neither an id nor a method,
/// is dropped: the server is somebody else's program, and half a
/// protocol is not a reason to lose the window.
pub fn parse(body: &[u8]) -> Option<Incoming> {
    let message: Value = serde_json::from_slice(body)
        .inspect_err(|err| tracing::warn!(%err, "a server said something that is not JSON"))
        .ok()?;

    let id = message.get("id").and_then(Value::as_u64);
    let method = message.get("method").and_then(Value::as_str);
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    match (id, method) {
        (Some(id), Some(method)) => Some(Incoming::Request {
            id,
            method: method.to_owned(),
            params,
        }),
        (Some(id), None) => {
            let result = match message.get("error") {
                Some(error) => Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("the server refused")
                    .to_owned()),
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            Some(Incoming::Answer { id, result })
        }
        (None, Some(method)) => Some(Incoming::Notification {
            method: method.to_owned(),
            params,
        }),
        (None, None) => {
            tracing::warn!("a server said something with neither an id nor a method");
            None
        }
    }
}

/// Read one message from a stream that is not buffered.
///
/// The reader thread has a `ChildStdout`, which is not a `BufRead`; this
/// is where the buffer is put on it, once, rather than per message.
pub fn reader(stdout: impl Read) -> std::io::BufReader<impl Read> {
    std::io::BufReader::new(stdout)
}

/// A path as the protocol wants it: `file://` and the path, with the
/// characters a URI may not carry escaped.
///
/// **Not `format!("file://{path}")`.** A project under `~/My Documents`
/// gives every diagnostic a URI the server cannot match back to a file,
/// and the failure is silent — the squiggles simply never arrive. Only
/// the unreserved set of RFC 3986 and `/` are left alone.
pub fn uri(path: &std::path::Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri
}

/// The path a `file://` URI names, or nothing when it names something
/// else — a server may answer with `jar:` or `https:`, and neither is a
/// file on this disk.
pub fn path_of(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut path = Vec::with_capacity(rest.len());
    let mut bytes = rest.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next()?;
            let low = bytes.next()?;
            let hex = |b: u8| (b as char).to_digit(16);
            path.push((hex(high)? * 16 + hex(low)?) as u8);
        } else {
            path.push(byte);
        }
    }
    Some(std::path::PathBuf::from(String::from_utf8(path).ok()?))
}

/// The three things a server is told about a file, as bodies.
///
/// Pure, and out here rather than inside the window's half, because what
/// goes on the wire is the part that can be got wrong invisibly: a server
/// ignores a notification it does not understand and says nothing at all.
pub mod document {
    use serde_json::{Value, json};

    /// A file has been opened, and here is what is in it.
    pub fn opened(uri: &str, language: &str, version: i32, text: &str) -> Value {
        json!({"textDocument": {
            "uri": uri, "languageId": language, "version": version, "text": text
        }})
    }

    /// It has changed, and here is all of it — full sync, decision 6 of
    /// M8.5.
    pub fn changed(uri: &str, version: i32, text: &str) -> Value {
        json!({
            "textDocument": {"uri": uri, "version": version},
            "contentChanges": [{"text": text}]
        })
    }

    /// It is closed. The server keeps nothing about it, and its
    /// diagnostics stop.
    pub fn closed(uri: &str) -> Value {
        json!({"textDocument": {"uri": uri}})
    }
}

#[cfg(test)]
mod bounded_framing_tests {
    use super::*;
    #[test]
    fn oversized_headers_and_advertised_bodies_fail_before_body_allocation() {
        for bytes in [
            [
                b"X:".as_slice(),
                &vec![b'x'; 8192],
                b"\r\nContent-Length: 0\r\n\r\n",
            ]
            .concat(),
            b"Content-Length: 16777217\r\n\r\n".to_vec(),
            b"Content-Length: 1\r\nContent-Length: 2\r\n\r\nxx".to_vec(),
        ] {
            let result = read_message(&mut std::io::Cursor::new(bytes));
            assert!(
                matches!(result,Err(ref e) if e.kind()==std::io::ErrorKind::InvalidData),
                "resource/length violation must be rejected before consuming a body: {result:?}"
            );
        }
    }
}

#[cfg(test)]
mod framing_compatibility_tests {
    use super::*;
    #[test]
    fn ordinary_large_frames_and_exact_bounded_headers_survive_fragmentation() {
        for size in [355 * 1024, 5 * 1024 * 1024, BODY_LIMIT] {
            let body = vec![b'x'; size];
            let wire = frame(&body);
            let mut read = std::io::BufReader::with_capacity(7, std::io::Cursor::new(wire));
            assert_eq!(read_message(&mut read).unwrap(), Some(body));
            assert_eq!(read_message(&mut read).unwrap(), None);
        }
        let tail=b"\r\ncOnTeNt-LeNgTh: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n";
        let header = [
            b"X:".as_slice(),
            &vec![b'x'; HEADER_LIMIT - 2 - tail.len()],
            tail,
        ]
        .concat();
        assert_eq!(header.len(), HEADER_LIMIT);
        let mut read = std::io::Cursor::new([header, b"{}".to_vec()].concat());
        assert_eq!(read_message(&mut read).unwrap().unwrap(), b"{}");
    }
    #[test]
    fn malformed_or_truncated_framing_is_actionable_and_finite() {
        for wire in [
            b"Content-Length: -1\r\n\r\n".as_slice(),
            b"Content-Length: 184467440737095516160\r\n\r\n",
            b"Content-Length: nope\r\n\r\n",
            b"No-Value\r\n\r\n",
            b"Content-Type: x\r\n\r\n",
            b"Content-Length: 5\r\n",
            b"Content-Length: 5\r\n\r\nxx",
            b"X: unterminated",
        ] {
            let e = read_message(&mut std::io::Cursor::new(wire)).unwrap_err();
            assert!(!e.to_string().is_empty());
        }
    }
}
