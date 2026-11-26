//! The hook Guild gives every `claude` it starts — M5.10.
//!
//! The script is run for real here, with `sh` and `/usr/bin/curl`,
//! against a socket the test holds: what it is promised to do is what a
//! session's hooks will do, and a script is a program nobody else checks.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::scratch::Scratch;
use guild_desktop::hooks;

/// A directory of its own per test. Short on purpose: a socket's path
/// has to fit in 104 bytes, and `$TMPDIR` on macOS is already half that.
fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("gh-{name}-"))
}

/// Run the script with `input` on its stdin, the way a hook is run.
fn run_script(dir: &Path, socket: Option<&Path>, input: &[u8]) -> (ExitStatus, Duration) {
    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let mut command = Command::new("sh");
    command
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .env_remove("GUILD_RUNTIME")
        .env_remove(guild_session::HOOK_SOCKET_VAR)
        // A process Guild did not start: no `GUILD_SESSION` either, even
        // when the tests run in a tab of Guild's — M13.3, the script adds
        // it to what it posts.
        .env_remove(guild_session::SESSION_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(socket) = socket {
        command.env(guild_session::HOOK_SOCKET_VAR, socket);
    }
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let status = child.wait().unwrap();
    (status, started.elapsed())
}

/// Accept one connection, read one request, answer 204; hand the request
/// back as text.
fn serve_once(listener: UnixListener) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&request);
            if let Some(end) = text.find("\r\n\r\n") {
                let length: usize = text[..end]
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().ok())?
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
            .unwrap();
        String::from_utf8(request).unwrap()
    })
}

/// Status comes from OSC 7501; a Claude session's hooks carry only what
/// it cannot say: which conversation it is (`SessionStart`, also on an
/// in-TUI `/resume`), and that the person submitted a prompt
/// (`UserPromptSubmit` — the project sort's authorship and a
/// continuation's turn-start acknowledgement).
#[test]
fn the_settings_name_the_two_hooks_and_the_script() {
    let script = Path::new("/Users/a person/it's here/guild-hook.sh");
    let settings = hooks::settings(script);
    assert_eq!(
        settings["theme"], "auto",
        "Guild-launched Claude follows the terminal background without writing personal settings"
    );
    let table = settings["hooks"].as_object().expect("a table of hooks");

    let events: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    assert_eq!(
        events,
        BTreeSet::from([
            // `/resume` fires it with the resumed id at once — M13.3.
            "SessionStart",
            "UserPromptSubmit",
        ])
    );
    for (event, groups) in table {
        let hook = &groups[0]["hooks"][0];
        assert_eq!(hook["type"], "command", "{event}");
        assert_eq!(hook["command"], hooks::shell_quote(script), "{event}");
        assert!(hook.get("async").is_none(), "{event}");
        assert!(groups[0].get("matcher").is_none(), "{event}");
    }
}

#[test]
fn a_quoted_path_is_the_path_again_to_a_shell() {
    let path = Path::new("/Users/a person/it's here/guild-hook.sh");
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("printf '%s' {}", hooks::shell_quote(path)))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        path.to_str().unwrap()
    );
}

#[test]
fn installing_writes_both_files_and_leaves_them_alone_the_second_time() {
    let dir = scratch("install");
    let installed = hooks::install(&dir).unwrap();

    assert_eq!(
        std::fs::read_to_string(&installed.script).unwrap(),
        hooks::SCRIPT
    );
    let mode = std::fs::metadata(&installed.script)
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0, "the script is executable");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&installed.settings).unwrap()).unwrap();
    assert_eq!(written, hooks::settings(&installed.script));

    let modified = |path: &Path| std::fs::metadata(path).unwrap().modified().unwrap();
    let (script, settings) = (modified(&installed.script), modified(&installed.settings));
    std::thread::sleep(Duration::from_millis(20));
    hooks::install(&dir).unwrap();
    assert_eq!(modified(&installed.script), script);
    assert_eq!(modified(&installed.settings), settings);
}

#[test]
fn installing_again_puts_back_a_script_that_lost_its_mode() {
    let dir = scratch("mode");
    let installed = hooks::install(&dir).unwrap();
    std::fs::set_permissions(&installed.script, std::fs::Permissions::from_mode(0o644)).unwrap();
    hooks::install(&dir).unwrap();
    let mode = std::fs::metadata(&installed.script)
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0);
}

#[test]
fn the_script_posts_its_stdin_to_the_socket() {
    let dir = scratch("post");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);

    // Bigger than a kilobyte, which is where curl would start asking
    // `Expect: 100-continue` and waiting a second for the answer.
    let payload = format!(
        r#"{{"hook_event_name":"PostToolUse","tool_response":"{}"}}"#,
        "x".repeat(200_000)
    );
    let (status, took) = run_script(&dir, Some(&socket), payload.as_bytes());

    assert!(status.success());
    let request = request.join().unwrap();
    assert!(
        request.starts_with("POST /hook HTTP/1.1\r\n"),
        "{}",
        &request[..80]
    );
    assert!(
        !request
            .to_ascii_lowercase()
            .contains("expect: 100-continue")
    );
    assert!(request.ends_with(&payload), "the body is the hook's stdin");
    assert!(took < Duration::from_secs(1), "took {took:?}");
}

/// A codex-style call names the event on the command line and leaves the
/// session's own id in GUILD_SESSION; the script is what turns those into
/// the envelope [`from_hook`] reads — the same shape Claude posts whole.
#[test]
fn a_codex_style_call_builds_an_envelope() {
    let dir = scratch("codexpost");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);

    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let session = "11111111-2222-3333-4444-555555555555";
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .arg("PreToolUse")
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(guild_session::SESSION_VAR, session)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());

    let request = request.join().unwrap();
    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    let envelope: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(envelope["hook_event_name"], "PreToolUse");
    assert_eq!(envelope["session_id"], session);
}

/// Codex generates its own session id and sends it on the hook's stdin —
/// measured live against 0.155.1, whose payload is the compact
/// `{"session_id":"…","transcript_path":…,"cwd":…,"hook_event_name":…,…}`.
/// The script lifts it into `codex_session`, the same way opencode's plugin
/// posts `oc_session`, so a reopened Codex can be resumed by its real id.
#[test]
fn a_codex_style_call_lifts_the_harness_session_from_stdin() {
    let dir = scratch("codexsess");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);

    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let session = "11111111-2222-3333-4444-555555555555";
    let native = "01a0c0cb-f105-73d0-a0d3-bb9faf592187";
    let payload = format!(
        "{{\"session_id\":\"{native}\",\"transcript_path\":\"/x/rollout.jsonl\",\
        \"cwd\":\"/work\",\"hook_event_name\":\"SessionStart\",\
        \"model\":\"gpt-6-astra\",\"source\":\"startup\"}}"
    );
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .arg("UserPromptSubmit")
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(guild_session::SESSION_VAR, session)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    assert!(child.wait().unwrap().success());

    let request = request.join().unwrap();
    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    let envelope: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(envelope["hook_event_name"], "UserPromptSubmit");
    assert_eq!(envelope["session_id"], session);
    assert_eq!(
        envelope["codex_session"], native,
        "the harness's own id is lifted out of the codex payload"
    );

    // A payload with no session id (or none at all) leaves the field empty,
    // which the window reads as "nothing to record".
    for (n, blank) in [
        &br#"{ "cwd": "/work", "hook_event_name": "Stop" }"#[..],
        &b""[..],
    ]
    .into_iter()
    .enumerate()
    {
        let socket = dir.join(format!("s{}.sock", n + 2));
        let listener = UnixListener::bind(&socket).unwrap();
        let request = serve_once(listener);
        let mut child = Command::new("sh")
            .arg(&script)
            .env_remove("GUILD_IDENTITY_DIR")
            .arg("Stop")
            .env(guild_session::HOOK_SOCKET_VAR, &socket)
            .env(guild_session::SESSION_VAR, session)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(blank);
        assert!(child.wait().unwrap().success());
        let request = request.join().unwrap();
        let envelope: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap();
        assert_eq!(envelope["codex_session"], "", "no id, no field");
    }
}

/// A codex-style call reads the event's JSON off stdin to lift the
/// harness's own session id out of it, so a payload larger than a pipe
/// buffer must not hold it up: reading stdin through to the end is what
/// keeps the writer moving, and this proves the branch is quick with a
/// big payload beside it.
#[test]
fn a_codex_style_call_does_not_hold_a_large_payload_up() {
    let dir = scratch("codexstdin");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);

    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let payload = "y".repeat(200_000);
    let started = Instant::now();
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .arg("Stop")
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(guild_session::SESSION_VAR, "x")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Writing to the script's stdin must never block on a pipe the script
    // is not draining — sed reads it whole, so the writer is never held up,
    // well under curl's --max-time of a second. The write end drops when
    // `take()` is finished with it, which is what lets sed reach EOF.
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    assert!(child.wait().unwrap().success());
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "a payload that is only consumed, not acted on, must not hold the call up"
    );
    assert!(
        !request.join().unwrap().is_empty(),
        "the envelope is still posted"
    );
}

#[test]
fn without_a_socket_the_script_sends_nothing_and_succeeds() {
    let dir = scratch("none");
    let (status, took) = run_script(&dir, None, br#"{"hook_event_name":"Stop"}"#);
    assert!(status.success());
    assert!(took < Duration::from_secs(1));

    let (status, _) = run_script(&dir, Some(&dir.join("gone.sock")), b"{}");
    assert!(
        status.success(),
        "a socket that is not there is not a failure"
    );
}

#[test]
fn a_window_that_does_not_answer_does_not_hold_the_hook_up() {
    let dir = scratch("mute");
    let socket = dir.join("s.sock");
    // Bound and never accepted: the connection lands in the backlog and
    // nothing ever answers it.
    let _listener = UnixListener::bind(&socket).unwrap();
    let (status, took) = run_script(&dir, Some(&socket), br#"{"hook_event_name":"Stop"}"#);
    assert!(status.success());
    assert!(took < Duration::from_millis(1500), "took {took:?}");
}

#[test]
fn the_socket_path_fits_in_sun_path() {
    assert!(hooks::socket_path(u32::MAX).as_os_str().len() < 104);
}

/// opencode's status plugin lands in opencode's own plugins directory —
/// the only place opencode V2 looks for a local plugin — wherever the
/// person's config lives. Reading the environment is enough for the tail:
/// the name is the whole of the contract.
#[test]
fn the_opencode_plugin_dirs_end_in_opencode_plugins() {
    let dir = hooks::opencode_plugins_dir();
    assert!(
        dir.ends_with(Path::new("opencode").join("plugins")),
        "the plugin must land where opencode scans: {}",
        dir.display()
    );
}

/// Codex's hooks live in the person's `config.toml`, beside another tool's,
/// and Guild owns only the block between its own markers: the model line and
/// the other tool's block must come back byte for byte, and a second install
/// must not double the block.
#[test]
fn codex_hooks_land_beside_another_tool_s_and_keep_their_place() {
    let dir = scratch("codex");
    let theirs = "# >>> other-tool agent-status >>>\n\
                  [[hooks.PreToolUse]]\n\
                  [[hooks.PreToolUse.hooks]]\n\
                  type = \"command\"\n\
                  command = \"'/path/other-codex-status.sh' pre-tool-use\"\n\
                  # <<< other-tool agent-status >>>\n";
    let config = format!("model = \"gpt-5.6-sol\"\n{theirs}");
    std::fs::write(dir.join("config.toml"), &config).unwrap();

    hooks::install_codex_into(&dir).unwrap();
    let once = std::fs::read_to_string(dir.join("config.toml")).unwrap();
    assert!(
        once.starts_with("model = \"gpt-5.6-sol\"\n"),
        "the person's model line is first"
    );
    assert!(
        once.contains(theirs),
        "the other tool's own block survives untouched"
    );
    assert!(
        once.contains("[[hooks.SessionStart]]"),
        "capture identity before the first turn or a restart"
    );
    let ours = once.matches("[[hooks.PreToolUse]]").count();
    assert!(
        once.contains("guild-hook.sh"),
        "the block names Guild's script"
    );
    assert_eq!(once.matches("# >>> guild hooks >>>").count(), 1);
    assert_eq!(once.matches("# <<< guild hooks <<<").count(), 1);
    assert!(
        ours >= 2,
        "Guild's PostToolUse and the other tool's PreToolUse both stay"
    );

    // A second install is a no-op: same text, no duplicate block.
    hooks::install_codex_into(&dir).unwrap();
    let twice = std::fs::read_to_string(dir.join("config.toml")).unwrap();
    assert_eq!(once, twice, "installing again leaves the file alone");
}

#[test]
fn a_launch_carries_the_settings_flag() {
    let launch = guild_session::Hooks {
        settings: PathBuf::from("/tmp/claude-settings.json"),
        statusline_settings: PathBuf::from("/tmp/claude-settings-statusline.json"),
        socket: PathBuf::from("/tmp/guild-hooks-1.sock"),
        user_statusline: None,
    };
    assert_eq!(
        launch.args(),
        [
            std::ffi::OsString::from("--settings"),
            std::ffi::OsString::from("/tmp/claude-settings.json"),
        ]
    );
}

/// The window's end: a socket, a request parsed, a channel.
mod receiver {
    use super::*;
    use std::os::unix::net::UnixStream;

    use guild_desktop::hooks::server::{Parsed, Receiver, parse};

    /// A payload recorded from a live `claude` by M5.10's task 0.
    const RECORDED: &str =
        include_str!("../../../session/tests/fixtures/hooks/permission-request.json");

    fn request(body: &str) -> Vec<u8> {
        format!(
            "POST /hook HTTP/1.1\r\nHost: guild\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    /// Everything the receiver has heard within `patience`.
    fn heard(receiver: &Receiver, patience: Duration) -> Vec<serde_json::Value> {
        let end = Instant::now() + patience;
        loop {
            let got = receiver.poll();
            if !got.is_empty() || Instant::now() > end {
                return got;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// What the receiver answered on a raw connection, up to the blank line.
    fn answer(stream: &mut UnixStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut got = Vec::new();
        let mut byte = [0u8; 1];
        while !got.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => got.push(byte[0]),
                _ => break,
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    #[test]
    fn a_whole_request_is_its_body() {
        let bytes = request(r#"{"a":1}"#);
        assert_eq!(
            parse(&bytes),
            Parsed::Request {
                path: "/hook".to_owned(),
                body: br#"{"a":1}"#.to_vec(),
                used: bytes.len(),
            }
        );
    }

    /// **A request says which door it came to** — M13 task 1. The window
    /// listens on one socket and will answer two kinds of caller: a
    /// session's hooks, which post their JSON and want nothing back, and
    /// the `guild` command, which asks a question and waits for the
    /// answer. Telling them apart by the shape of the body would be
    /// guessing; the path is what a request already carries, and until
    /// now the parser read the method and the length and threw it away.
    #[test]
    fn a_request_says_which_door_it_came_to() {
        let to = |path: &str, body: &str| {
            format!(
                "POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .into_bytes()
        };

        let hook = to("/hook", r#"{"a":1}"#);
        assert_eq!(
            parse(&hook),
            Parsed::Request {
                path: "/hook".to_owned(),
                body: br#"{"a":1}"#.to_vec(),
                used: hook.len(),
            }
        );

        let command = to("/command", r#"{"b":2}"#);
        assert_eq!(
            parse(&command),
            Parsed::Request {
                path: "/command".to_owned(),
                body: br#"{"b":2}"#.to_vec(),
                used: command.len(),
            }
        );
    }

    #[test]
    fn a_request_cut_anywhere_is_incomplete() {
        let bytes = request(RECORDED);
        for cut in 0..bytes.len() {
            assert_eq!(parse(&bytes[..cut]), Parsed::Incomplete, "cut at {cut}");
        }
    }

    #[test]
    fn what_is_not_a_hook_is_refused_by_its_status() {
        assert_eq!(
            parse(b"POST /hook HTTP/1.1\r\nHost: guild\r\n\r\n"),
            Parsed::Bad(411)
        );
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nHost: guild\r\n\r\n"),
            Parsed::Bad(405)
        );
        let huge = format!(
            "POST /hook HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            17 * 1024 * 1024
        );
        assert_eq!(parse(huge.as_bytes()), Parsed::Bad(413));
        assert_eq!(
            parse(b"POST /hook HTTP/1.1\r\nContent-Length: many\r\n\r\n"),
            Parsed::Bad(400)
        );
    }

    #[test]
    fn the_script_reaches_the_window() {
        let dir = scratch("trip");
        let socket = dir.join("s.sock");
        let receiver = Receiver::start(&socket).unwrap();

        let (status, _) = run_script(&dir, Some(&socket), RECORDED.as_bytes());

        assert!(status.success());
        let expected: serde_json::Value = serde_json::from_str(RECORDED).unwrap();
        assert_eq!(heard(&receiver, Duration::from_secs(2)), [expected]);
    }

    #[test]
    fn what_is_not_a_hook_is_refused_and_the_next_one_is_still_heard() {
        let dir = scratch("junk");
        let socket = dir.join("s.sock");
        let receiver = Receiver::start(&socket).unwrap();

        let mut stream = UnixStream::connect(&socket).unwrap();
        stream.write_all(b"nonsense\r\n\r\n").unwrap();
        assert!(answer(&mut stream).starts_with("HTTP/1.1 405"));

        let mut stream = UnixStream::connect(&socket).unwrap();
        stream.write_all(&request("not json")).unwrap();
        assert!(answer(&mut stream).starts_with("HTTP/1.1 400"));
        assert!(
            receiver.poll().is_empty(),
            "what is not JSON is not passed on"
        );

        let (status, _) = run_script(&dir, Some(&socket), RECORDED.as_bytes());
        assert!(status.success());
        assert_eq!(heard(&receiver, Duration::from_secs(2)).len(), 1);
    }

    #[test]
    fn a_client_that_asks_to_continue_is_told_to() {
        // The script clears `Expect:`; a curl that ignored it must still
        // be answered, or it waits a second for nothing.
        let dir = scratch("cont");
        let socket = dir.join("s.sock");
        let receiver = Receiver::start(&socket).unwrap();
        let body = RECORDED.trim();

        let mut stream = UnixStream::connect(&socket).unwrap();
        write!(
            stream,
            "POST /hook HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        assert!(answer(&mut stream).starts_with("HTTP/1.1 100"));
        stream.write_all(body.as_bytes()).unwrap();
        assert!(answer(&mut stream).starts_with("HTTP/1.1 204"));
        assert_eq!(heard(&receiver, Duration::from_secs(2)).len(), 1);
    }

    #[test]
    fn a_receiver_dropped_leaves_no_socket_behind() {
        let dir = scratch("drop");
        let socket = dir.join("s.sock");
        let receiver = Receiver::start(&socket).unwrap();
        assert!(socket.exists());
        drop(receiver);
        assert!(!socket.exists());
    }

    #[test]
    fn a_socket_file_left_behind_does_not_stop_a_start() {
        let dir = scratch("stale");
        let socket = dir.join("s.sock");
        // A listener dropped without unlinking leaves its file, as a
        // window that crashed does.
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        let receiver = Receiver::start(&socket).unwrap();
        let (status, _) = run_script(&dir, Some(&socket), RECORDED.as_bytes());
        assert!(status.success());
        assert_eq!(heard(&receiver, Duration::from_secs(2)).len(), 1);
    }

    #[test]
    fn a_sweep_takes_what_dead_windows_left_and_keeps_the_rest() {
        let dir = scratch("sweep");
        // macOS pids stop at 99998: this one can name no window.
        let dead = dir.join("guild-hooks-999999.sock");
        let living = dir.join(format!("guild-hooks-{}.sock", std::process::id()));
        let stranger = dir.join("someone-else.sock");
        for path in [&dead, &living, &stranger] {
            std::fs::write(path, b"").unwrap();
        }

        hooks::sweep(&dir);

        assert!(!dead.exists());
        assert!(living.exists(), "a window that answers is still running");
        assert!(stranger.exists(), "only ours are ours to remove");
    }
}

/// **A command is asked and answered over the same socket** — M13 task 1.
///
/// The window listens once and serves two kinds of caller: a session's
/// hooks, which post and want nothing back, and `guild`, which asks and
/// holds the line. This drives the second through a real socket, because
/// the part that can go wrong is the waiting.
#[test]
fn a_command_is_answered_over_the_same_socket() {
    use guild_desktop::hooks::server::Receiver;
    use guild_session::command::{Answer, Command};

    let dir = scratch("cmd");
    let socket = dir.join("s.sock");
    // The receiver stays here: it is the window, and the window is not a
    // thing that moves between threads.
    let receiver = Receiver::start(&socket).unwrap();

    let asking = std::thread::spawn({
        let socket = socket.clone();
        move || {
            let body = serde_json::to_vec(&Command::Status).unwrap();
            let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            stream
                .write_all(
                    format!(
                        "POST /command HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.write_all(&body).unwrap();
            let mut said = String::new();
            stream.read_to_string(&mut said).unwrap();
            said
        }
    });

    // The window's own half, on its own clock.
    let until = Instant::now() + Duration::from_secs(5);
    let mut answered = false;
    while !answered && Instant::now() < until {
        for asked in receiver.commands() {
            assert_eq!(asked.command, Command::Status);
            asked
                .answer
                .send(Answer::Status {
                    projects: Vec::new(),
                })
                .unwrap();
            answered = true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(answered, "the command never reached the window");

    let said = asking.join().unwrap();
    assert!(said.starts_with("HTTP/1.1 200"), "{said}");
    let json = said.split("\r\n\r\n").nth(1).unwrap_or_default();
    assert_eq!(
        serde_json::from_str::<Answer>(json).unwrap(),
        Answer::Status {
            projects: Vec::new()
        }
    );
}

/// **A Claude envelope carries who its process was started as** — M13.3.
/// After `/resume` Claude names the conversation it resumed, and the
/// window needs the tab too; the script adds `GUILD_SESSION` as
/// `guild_session`, and leaves the rest of the envelope as Claude wrote it.
#[test]
fn a_claude_envelope_carries_the_session_it_was_started_as() {
    let dir = scratch("guildsess");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);
    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let payload = r#"{"session_id":"aaaaaaaa-0000-0000-0000-000000000000","hook_event_name":"SessionStart","source":"resume"}"#;
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(
            guild_session::SESSION_VAR,
            "11111111-2222-3333-4444-555555555555",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    assert!(child.wait().unwrap().success());

    let request = request.join().unwrap();
    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
    let envelope: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(
        envelope["guild_session"],
        "11111111-2222-3333-4444-555555555555"
    );
    assert_eq!(
        envelope["session_id"],
        "aaaaaaaa-0000-0000-0000-000000000000"
    );
    assert_eq!(envelope["source"], "resume");
}

/// Codex's envelope carries it too — the same field for every harness.
#[test]
fn a_codex_envelope_carries_the_session_it_was_started_as() {
    let dir = scratch("codexguild");
    let socket = dir.join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let request = serve_once(listener);
    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let session = "11111111-2222-3333-4444-555555555555";
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .arg("Stop")
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(guild_session::SESSION_VAR, session)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());
    let request = request.join().unwrap();
    let envelope: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap();
    assert_eq!(envelope["guild_session"], session);
}

/// The actual plugin must translate the installed OpenCode's execution
/// lifecycle and carry both identities on the wire, including after resume.
#[test]
fn opencode_execution_events_report_the_turn_and_its_conversation() {
    let dir = scratch("oc-events");
    let plugin = dir.join("plugin.mjs");
    let driver = dir.join("driver.mjs");
    let socket = dir.join("events.sock");
    std::fs::write(&plugin, hooks::OPENCODE_PLUGIN).unwrap();
    std::fs::write(&driver, include_str!("../fixtures/opencode-plugin.mjs")).unwrap();
    let output = Command::new("node")
        .args([&driver, &plugin, &socket])
        .env(guild_session::HOOK_SOCKET_VAR, &socket)
        .env(guild_session::SESSION_VAR, "guild-record")
        .output()
        .expect("Node.js runs the OpenCode plugin's integration check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Codex 0.160 writes trust tables inside the markers on this machine.
#[test]
fn codex_hook_refresh_preserves_trust_inside_its_markers() {
    let dir = scratch("cxtrust");
    hooks::install_codex_into(&dir).unwrap();
    let path = dir.join("config.toml");
    let trust = "[hooks.state.\"/example/config.toml:stop:0:0\"]\ntrusted_hash = \"keep-me\"\n";
    let original = std::fs::read_to_string(&path).unwrap().replace(
        "# <<< guild hooks <<<",
        &format!("{trust}# <<< guild hooks <<<"),
    );
    std::fs::write(&path, &original).unwrap();
    hooks::install_codex_into(&dir).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains(trust));
}

/// A delivered event survives even when the window is already gone.
#[test]
fn codex_identity_survives_without_a_window() {
    let dir = scratch("identity");
    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let identities = dir.join("identities");
    let session = "11111111-2222-3333-4444-555555555555";
    let conversation = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let mut child = Command::new("sh")
        .arg(&script)
        .env_remove("GUILD_IDENTITY_DIR")
        .arg("SessionStart")
        .env(guild_session::SESSION_VAR, session)
        .env("GUILD_IDENTITY_DIR", &identities)
        .env(guild_session::HOOK_SOCKET_VAR, dir.join("gone.sock"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{{\"session_id\":\"{conversation}\"}}"
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    let saved = identities
        .join(session)
        .join(format!("{conversation}.json"));
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&saved).expect("identity must survive the missing socket"),
    )
    .unwrap();
    assert_eq!(value["guild_session"], session);
    assert_eq!(value["codex_session"], conversation);
    assert_eq!(
        std::fs::metadata(saved).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

fn report_identity(dir: &Path, session: &str, conversation: &str) {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(hooks::SCRIPT)
        .arg("guild-hook")
        .arg("SessionStart")
        .env(guild_session::SESSION_VAR, session)
        .env("GUILD_IDENTITY_DIR", dir)
        .env_remove(guild_session::HOOK_SOCKET_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let payload = serde_json::json!({"session_id": conversation, "tool_response": {"session_id": "cccccccc-0000-0000-0000-000000000003"}});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn codex_identity_restoration_is_exact_and_refuses_ambiguity() {
    use guild_session::{ProjectId, Runs, SessionMode, SessionRecord};
    let dir = scratch("recover");
    let project = ProjectId::new();
    let mut first = SessionRecord::new(
        "one".into(),
        project,
        "/same/folder".into(),
        Runs::Codex(SessionMode::Terminal),
    );
    let second = SessionRecord::new(
        "two".into(),
        project,
        "/same/folder".into(),
        Runs::Codex(SessionMode::Terminal),
    );
    let a = "aaaaaaaa-0000-0000-0000-000000000001";
    let b = "bbbbbbbb-0000-0000-0000-000000000002";
    report_identity(&dir, &first.id.to_string(), a);
    report_identity(&dir, &second.id.to_string(), b);
    assert_eq!(hooks::identity::recover(&dir, &first).as_deref(), Some(a));
    assert_eq!(hooks::identity::recover(&dir, &second).as_deref(), Some(b));
    // An invalid later event cannot damage the valid evidence.
    report_identity(&dir, &first.id.to_string(), "../bad");
    report_identity(&dir, &first.id.to_string(), &format!("{b}\nx"));
    assert_eq!(hooks::identity::recover(&dir, &first).as_deref(), Some(a));
    first.harness_session = Some(b.into());
    assert_eq!(
        hooks::identity::recover(&dir, &first),
        None,
        "saved identity wins"
    );
    first.harness_session = None;
    first.runs = Runs::Claude(SessionMode::Terminal);
    assert_eq!(hooks::identity::recover(&dir, &first), None);
    first.runs = Runs::Codex(SessionMode::Terminal);
    report_identity(&dir, &first.id.to_string(), b);
    assert_eq!(
        hooks::identity::recover(&dir, &first),
        None,
        "do not guess after an in-TUI resume"
    );
    assert_eq!(hooks::identity::recover(&dir, &second).as_deref(), Some(b));
}

#[test]
fn codex_identity_rejects_foreign_or_corrupt_evidence() {
    use guild_session::{ProjectId, Runs, SessionMode, SessionRecord};
    let dir = scratch("badidentity");
    let project = ProjectId::new();
    let record = SessionRecord::new(
        "one".into(),
        project,
        "/same/folder".into(),
        Runs::Codex(SessionMode::Terminal),
    );
    let id = "aaaaaaaa-0000-0000-0000-000000000001";
    report_identity(&dir, &record.id.to_string(), id);
    let path = dir.join(record.id.to_string()).join(format!("{id}.json"));
    std::fs::write(
        &path,
        serde_json::json!({"guild_session": project.to_string(), "codex_session": id}).to_string(),
    )
    .unwrap();
    assert_eq!(hooks::identity::recover(&dir, &record), None);
    std::fs::write(&path, "{truncated").unwrap();
    assert_eq!(hooks::identity::recover(&dir, &record), None);
    report_identity(&dir, "../escape", id);
    assert!(!dir.join("escape").exists());
}

#[test]
fn codex_permission_lifecycle_through_real_script_adapter_and_board() {
    use guild_session::harness::CodexHarness;
    use guild_session::status::{Latch, Status, Why};
    let dir = scratch("codexwait");
    let script = dir.join("guild-hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let session = guild_session::SessionId::new();
    let mut board = Latch::default();
    for (event, identity, expected) in [
        ("UserPromptSubmit", None, Status::Working),
        ("PreToolUse", Some("approval-1"), Status::Working),
        ("PermissionRequest", None, Status::Waiting(Why::Permission)),
        (
            "PostToolUse",
            Some("unrelated"),
            Status::Waiting(Why::Permission),
        ),
        ("PostToolUse", Some("approval-1"), Status::Working),
        ("Stop", None, Status::Done),
        ("UserPromptSubmit", None, Status::Working),
        ("PermissionRequest", None, Status::Waiting(Why::Permission)),
        ("PostToolUse", None, Status::Waiting(Why::Permission)),
        // Codex Stop is the authoritative turn completion even without tool IDs.
        ("Stop", None, Status::Done),
    ] {
        let socket = dir.join("s.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let request = serve_once(listener);
        let mut child = Command::new("sh")
            .arg(&script)
            .arg(event)
            .env_remove("GUILD_IDENTITY_DIR")
            .env(guild_session::HOOK_SOCKET_VAR, &socket)
            .env(guild_session::SESSION_VAR, session.to_string())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = serde_json::json!({"session_id":"11111111-2222-3333-4444-555555555555", "turn_id":"turn-current", "tool_name":"Bash", "tool_input":{"command":"cargo check"}});
        if let Some(id) = identity {
            input["tool_use_id"] = id.into();
        }
        if event == "PermissionRequest" {
            input["tool_input"]["description"] = "Build the project".into();
        }
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        let request = request.join().unwrap();
        let envelope = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        let (actual, signal) = CodexHarness.status_from_payload(&envelope).unwrap();
        assert_eq!(actual, session);
        board.apply_payload(actual, signal, &envelope);
        assert_eq!(board.status(session), expected, "{event} {identity:?}");
        std::fs::remove_file(socket).unwrap();
    }
}

/// Codex native subagents inherit Guild's environment. Their agent_id or
/// object-valued rollout source distinguishes them from the interactive TUI;
/// identity persistence must refuse them
/// even when the socket is absent, before the parent can be rebound on restart.
#[test]
fn nested_codex_hooks_cannot_persist_parent_identity() {
    for source in ["agent", "guardian"] {
        let dir = scratch("codex-nested");
        let script = dir.join("guild-hook.sh");
        std::fs::write(&script, hooks::SCRIPT).unwrap();
        let guild = guild_session::SessionId::new();
        let native = "11111111-2222-3333-4444-555555555555";
        let transcript = dir.join("nested.jsonl");
        let meta_source = if source == "guardian" {
            serde_json::json!({"subagent":{"other":"guardian"}})
        } else {
            serde_json::json!("exec")
        };
        std::fs::write(&transcript, format!("{}\n", serde_json::json!({"type":"session_meta","payload":{"id":native,"source":meta_source}}))).unwrap();
        let mut payload = serde_json::json!({"session_id":native,"transcript_path":transcript,"turn_id":"turn-child","hook_event_name":"PermissionRequest"});
        if source == "agent" {
            payload["agent_id"] = "child-agent".into();
            payload["agent_type"] = "reviewer".into();
        }
        let socket = dir.join("nested.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut child = Command::new("sh")
            .arg(&script)
            .arg("PermissionRequest")
            .env(guild_session::HOOK_SOCKET_VAR, &socket)
            .env("GUILD_IDENTITY_DIR", dir.join("identities"))
            .env(guild_session::SESSION_VAR, guild.to_string())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "{source} cannot post parent status"
        );
        assert!(
            !dir.join("identities").join(guild.to_string()).exists(),
            "{source} cannot identify the parent"
        );
    }
}

#[test]
fn hook_envelope_keeps_the_exact_launch_runtime_for_start_receipts() {
    let dir = scratch("runtime-envelope");
    let socket = dir.join("s.sock");
    let server = serve_once(UnixListener::bind(&socket).unwrap());
    let script = dir.join("hook.sh");
    std::fs::write(&script, hooks::SCRIPT).unwrap();
    let mut child = Command::new("/bin/sh")
        .arg(&script)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .env("GUILD_RUNTIME", "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        .env("GUILD_HOOK_SOCKET", &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"session_id":"11111111-2222-3333-4444-555555555555","hook_event_name":"UserPromptSubmit"}"#).unwrap();
    assert!(child.wait().unwrap().success());
    let request = server.join().unwrap();
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body["guild_runtime"], "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "an old process must not acknowledge a replacement runtime"
    );
}
