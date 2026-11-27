//! `guild` driven as a program — M13 task 2.
//!
//! The binary is run for real against a socket this test holds, playing
//! the window: what it prints, what it exits with, and what it refuses
//! are the whole of what an agent sees, and an agent reads those rather
//! than the source.

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;

use guild_session::command::{Agent, Answer, Project, WorkspaceRow, Worktree};
use tempfile::TempDir;

/// A directory of its own, removed when the test lets go of it. Short,
/// because a socket's path must fit in 104 bytes and `$TMPDIR` on macOS
/// is already half of that — task 0 hit this at once: `gc-<name>-` and
/// six random characters, then `/s.sock`, is under eighty.
fn scratch(name: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("gc-{name}-"))
        .tempdir()
        .expect("a scratch folder")
}

/// Read the entire HTTP request before replying. A socket read may contain only
/// headers; closing after that read races the client's separate body write.
fn request(stream: &mut impl Read) -> String {
    let mut got = Vec::new();
    let mut chunk = [0u8; 4096];
    // Enough to hold the head and the small body a command sends.
    loop {
        let read = stream.read(&mut chunk).unwrap();
        got.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&got);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
        if read == 0 {
            break;
        }
    }
    String::from_utf8(got).unwrap()
}

/// Play the window for one request: read it, answer with `answer`.
fn window(listener: UnixListener, answer: Answer) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let got = request(&mut stream);
        let body = serde_json::to_vec(&answer).unwrap();
        let head = format!("HTTP/1.1 200 \r\nContent-Length: {}\r\n\r\n", body.len());
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        got
    })
}

fn guild() -> Command {
    Command::new(env!("CARGO_BIN_EXE_guild"))
}

/// `status` asks the window and prints what it holds, the root in front
/// marked. The request that reaches the window is the tagged command, at
/// the door commands are asked at.
#[test]
fn status_prints_what_the_window_holds() {
    let dir = scratch("status");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Status {
            projects: vec![
                Project {
                    path: PathBuf::from("/w/guild"),
                    name: "guild".to_owned(),
                    repository: Some(PathBuf::from("/w/guild/.git")),
                    active: true,
                    sessions: 2,
                    workspaces: vec!["workspace-1".into()],
                },
                Project {
                    path: PathBuf::from("/w/notes"),
                    name: "notes".to_owned(),
                    repository: None,
                    active: false,
                    sessions: 0,
                    workspaces: Vec::new(),
                },
            ],
        },
    );

    let out = guild()
        .arg("status")
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.starts_with("POST /command "), "{request}");
    assert!(request.contains(r#""command":"status""#), "{request}");

    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        printed.contains("* guild  /w/guild  2 sessions"),
        "{printed}"
    );
    assert!(printed.contains("  notes  /w/notes"), "{printed}");
}

/// **A refusal is a sentence, and the exit code only says that it
/// failed.** Every refusal in the window already has words — git's own or
/// a guard's — and an agent can act on the words; a bare `1` throws them
/// away.
#[test]
fn a_refusal_is_said_in_words_and_fails() {
    let dir = scratch("refused");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Refused {
            because: "That worktree is locked: claude session updates".to_owned(),
        },
    );

    let out = guild()
        .arg("status")
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let _ = asked.join();

    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("That worktree is locked"), "{said}");
}

/// **No window, no guess.** A shell Guild did not start has no
/// `GUILD_SOCKET`, and `guild` says so rather than looking for a socket it
/// might find — which is also what keeps a stray `guild` from reaching a
/// window its caller did not open.
#[test]
fn without_a_window_it_says_so() {
    let out = guild()
        .arg("status")
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("GUILD_SOCKET is not set"));
}

#[test]
fn workspace_commands_use_the_window_and_keep_creation_unselected_by_default() {
    let dir = scratch("workspaces");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Workspaces {
            workspaces: vec![WorkspaceRow {
                id: "00000000-0000-0000-0000-000000000001".into(),
                name: "Default".into(),
                icon: None,
                selected: true,
                projects: vec![PathBuf::from("/w/guild")],
            }],
        },
    );
    let out = guild()
        .args(["workspace", "list"])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"workspace_list""#),
        "{request}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Default\tselected=true"));

    let socket = dir.path().join("c.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::WorkspaceCreated {
            id: "id-2".into(),
            name: "Side".into(),
        },
    );
    let out = guild()
        .args(["workspace", "create", "Side", "--named-icon", "coffee"])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(out.status.success());
    assert!(
        request.contains(r#""command":"workspace_create""#),
        "{request}"
    );
    assert!(request.contains(r#""icon":"coffee""#), "{request}");
    assert!(request.contains(r#""select":false"#), "{request}");

    let socket = dir.path().join("s2.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(listener, Answer::WorkspaceSelected { id: "id-2".into() });
    let out = guild()
        .args(["workspace", "select", "id-2"])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(out.status.success());
    assert!(
        request.contains(r#""command":"workspace_select""#),
        "{request}"
    );
}

/// `worktree list` prints each checkout with what matters about it, the
/// primary and a lock's reason included — the column's own source, so the
/// command line and the window cannot tell two stories.
#[test]
fn worktree_list_names_the_primary_what_is_open_and_why_one_is_locked() {
    let dir = scratch("wtlist");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Worktrees {
            worktrees: vec![
                Worktree {
                    path: PathBuf::from("/w/guild"),
                    repository: PathBuf::from("/w/guild/.git"),
                    branch: Some("main".to_owned()),
                    primary: true,
                    opened: true,
                    locked: None,
                },
                Worktree {
                    path: PathBuf::from("/w/guild-updates"),
                    repository: PathBuf::from("/w/guild/.git"),
                    branch: None,
                    primary: false,
                    opened: false,
                    locked: Some("claude session updates (pid 35060)".to_owned()),
                },
            ],
        },
    );

    let out = guild()
        .args(["worktree", "list"])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"worktree_list""#),
        "{request}"
    );
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        printed.contains("main") && printed.contains("primary, open"),
        "{printed}"
    );
    assert!(printed.contains("(detached)"), "{printed}");
    assert!(
        printed.contains("locked: claude session updates (pid 35060)"),
        "{printed}"
    );
}

/// **`worktree open` sends the path the window must hear: absolute and
/// with its links resolved.** A relative path means something only in the
/// shell that typed it, and the window's watcher is fed real paths — on
/// macOS the temporary directory itself sits under the `/var` link, which
/// is exactly the case this runs in.
#[test]
fn worktree_open_sends_the_real_path_of_the_folder() {
    let dir = scratch("wtopen");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let real = std::fs::canonicalize(&checkout).unwrap();

    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(listener, Answer::Opened { path: real.clone() });

    let out = guild()
        .args(["worktree", "open"])
        .arg(&checkout)
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"worktree_open""#),
        "{request}"
    );
    assert!(request.contains(&real.display().to_string()), "{request}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("opened"));
}

/// A folder that does not exist is refused before the window is asked —
/// there is nothing it could do with the path, and a round trip that ends
/// in the same sentence is a round trip for nothing.
#[test]
fn worktree_open_of_nothing_is_refused_before_the_window_is_asked() {
    let out = guild()
        .args(["worktree", "open", "/no/such/checkout/anywhere"])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("there is no folder"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `worktree create` sends the name and the root to make it from, and
/// **the value of `--from` is not a word of the command**: `worktree create
/// feat --from <folder>` is three words and a flag. The folder arrives
/// resolved, for the same reason `open`'s does.
#[test]
fn worktree_create_sends_the_name_and_where_to_make_it_from() {
    let dir = scratch("wtcreate");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let real = std::fs::canonicalize(&repo).unwrap();

    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Created {
            path: PathBuf::from("/w/repo-feat"),
            branch: "feat".to_owned(),
            base: Some("main".to_owned()),
            session: None,
        },
    );

    let out = guild()
        .args(["worktree", "create", "feat", "--from"])
        .arg(&repo)
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"worktree_create""#),
        "{request}"
    );
    assert!(request.contains(r#""name":"feat""#), "{request}");
    assert!(request.contains(&real.display().to_string()), "{request}");
    // No `--base`: the window picks the main branch, so none is sent.
    assert!(!request.contains(r#""base""#), "{request}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("created /w/repo-feat on branch feat from main"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `--base` is sent as given, and the answer names it — M13.1.
#[test]
fn worktree_create_sends_the_base_it_was_given() {
    let dir = scratch("wtbase");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Created {
            path: PathBuf::from("/w/repo-feat"),
            branch: "feat".to_owned(),
            base: Some("release/1.2".to_owned()),
            session: None,
        },
    );

    let out = guild()
        .args([
            "worktree",
            "create",
            "feat",
            "--base",
            "release/1.2",
            "--from",
        ])
        .arg(dir.path())
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.contains(r#""base":"release/1.2""#), "{request}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("on branch feat from release/1.2"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// **A worktree and its task in one command** — M13.1, decision 3: the
/// task from standard input, who is asking, and the session's flags go
/// with the name, and both results are printed.
#[test]
fn worktree_create_with_a_task_sends_it_and_says_both() {
    let dir = scratch("wttask");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Created {
            path: PathBuf::from("/w/repo-feat"),
            branch: "feat".to_owned(),
            base: Some("main".to_owned()),
            session: Some("0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned()),
        },
    );

    let mut child = guild()
        .args([
            "worktree",
            "create",
            "feat",
            "--from-file",
            "-",
            "--front",
            "--harness",
            "codex",
            "--from",
        ])
        .arg(dir.path())
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fix the \"build\"\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"worktree_create""#),
        "{request}"
    );
    assert!(
        request.contains(r#""prompt":"fix the \"build\"\n""#),
        "{request}"
    );
    assert!(
        request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
        "{request}"
    );
    assert!(request.contains(r#""front":true"#), "{request}");
    assert!(request.contains(r#""harness":"codex""#), "{request}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.contains("created /w/repo-feat on branch feat from main")
            && said.contains("started 0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d in /w/repo-feat"),
        "{said}"
    );
}

/// A worktree's task without `--harness` names none either: the window
/// decides, as for `session new` — GLD-081's review.
#[test]
fn worktree_create_s_task_without_a_harness_leaves_the_agent_to_the_window() {
    let dir = scratch("wtdefault");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Refused {
            because: "Codex is not installed: install it from https://github.com/openai/codex"
                .to_owned(),
        },
    );
    let out = guild()
        .args(["worktree", "create", "feat", "--prompt", "fix it", "--from"])
        .arg(dir.path())
        .env("GUILD_SOCKET", &socket)
        .env_remove("GUILD_SESSION")
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        request.contains(r#""command":"worktree_create""#),
        "{request}"
    );
    assert!(!request.contains("harness"), "{request}");
    // The window's refusal is the command's: no *started*, and its words.
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("https://github.com/openai/codex"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The session's flags without a task are a mistake in the command: there
/// is no session for them to shape.
#[test]
fn worktree_create_s_session_flags_need_a_task() {
    for flag in [
        &["--front"][..],
        &["--harness", "codex"],
        &["--workspace", "x"],
    ] {
        let out = guild()
            .args(["worktree", "create", "feat"])
            .args(flag)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{flag:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--prompt or --from-file"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// A `--from` with nothing after it is a mistake in the command, not a
/// question for the window.
#[test]
fn from_without_a_folder_is_a_usage_error() {
    let out = guild()
        .args(["worktree", "create", "feat", "--from"])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--from needs a value"));
}

/// `worktree remove` sends the folder as the window must hear it, whether
/// what git ignores may go, and who is asking — M13 task 8 — and says what
/// is gone.
#[test]
fn worktree_remove_sends_the_folder_the_consent_and_who_is_asking() {
    let dir = scratch("wtremove");
    let worktree = dir.path().join("repo-feat");
    std::fs::create_dir_all(&worktree).unwrap();
    let real = std::fs::canonicalize(&worktree).unwrap();

    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(listener, Answer::Removed { path: real.clone() });

    let out = guild()
        .args(["worktree", "remove", "--delete-ignored"])
        .arg(&worktree)
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"worktree_remove""#),
        "{request}"
    );
    assert!(request.contains(r#""delete_ignored":true"#), "{request}");
    assert!(
        request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
        "{request}"
    );
    assert!(request.contains(&real.display().to_string()), "{request}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(&format!("removed {}", real.display())),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// **A removal names its folder.** `open` takes the folder you are in when
/// given none; a removal that did would delete the ground the asking session
/// stands on, so it is a mistake in the command, said before any window is
/// asked.
#[test]
fn worktree_remove_without_a_folder_is_a_usage_error() {
    let out = guild()
        .args(["worktree", "remove"])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("needs the worktree's folder"), "{said}");
}

/// `session new` sends the worktree, the prompt, whether to come to the
/// front, and **who is asking — from `GUILD_SESSION`, not from a flag**. A
/// session does not have to say who it is; the window checks the id against
/// what it started, and that is the whole of decision 5's provenance.
#[test]
fn session_new_sends_the_task_and_who_is_asking() {
    let dir = scratch("sessnew");
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let real = std::fs::canonicalize(&worktree).unwrap();

    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::SessionStarted {
            session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
            worktree: real.clone(),
        },
    );

    let out = guild()
        .args(["session", "new", "--prompt", "fix the build", "--worktree"])
        .arg(&worktree)
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.contains(r#""command":"session_new""#), "{request}");
    assert!(request.contains(r#""prompt":"fix the build""#), "{request}");
    assert!(
        request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
        "{request}"
    );
    assert!(request.contains(r#""front":false"#), "{request}");
    assert!(request.contains(&real.display().to_string()), "{request}");
    // GLD-081's review: no `--harness` names none, and the window runs the
    // default its own `+` opens — not Claude Code whatever is installed.
    assert!(!request.contains("harness"), "{request}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("started 0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `session resume` names the session and who is asking, stays behind
/// unless `--front`, and says where it came back — M13.3.
#[test]
fn session_resume_sends_the_session_and_who_is_asking() {
    for front in [false, true] {
        let dir = scratch("resume");
        let socket = dir.path().join("s.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let asked = window(
            listener,
            Answer::Resumed {
                session: "493409f7-1111-2222-3333-444444444444".to_owned(),
                worktree: PathBuf::from("/w/guild-fix"),
            },
        );
        let mut args = vec!["session", "resume", "493409f7-1111-2222-3333-444444444444"];
        if front {
            args.push("--front");
        }
        let out = guild()
            .args(&args)
            .env("GUILD_SOCKET", &socket)
            .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
            .output()
            .unwrap();
        let request = asked.join().unwrap();

        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            request.contains(r#""command":"session_resume""#),
            "{request}"
        );
        assert!(
            request.contains(r#""session":"493409f7-1111-2222-3333-444444444444""#),
            "{request}"
        );
        assert!(
            request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
            "{request}"
        );
        assert!(
            request.contains(&format!(r#""front":{front}"#)),
            "{request}"
        );
        assert!(
            String::from_utf8_lossy(&out.stdout)
                .contains("resumed 493409f7-1111-2222-3333-444444444444 in /w/guild-fix"),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

/// A resume names its session: there is no *the one I am in* to default to.
#[test]
fn session_resume_without_a_session_is_a_usage_error() {
    let out = guild().args(["session", "resume"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("agent list"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `agent send` sends the text and who is asking, and says the turn has
/// begun — M13 task 9.
#[test]
fn agent_send_sends_the_text_and_who_is_asking() {
    let dir = scratch("send");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Sent {
            session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
        },
    );

    let out = guild()
        .args([
            "agent",
            "send",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--prompt",
            "yes, use the second option",
        ])
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .output()
        .unwrap();
    let request = asked.join().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.contains(r#""command":"agent_send""#), "{request}");
    assert!(
        request.contains(r#""text":"yes, use the second option""#),
        "{request}"
    );
    assert!(
        request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
        "{request}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("sent to 0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// **`--from-file` carries a text the shell would have mangled** — quotes,
/// a dollar sign, backticks and many lines, byte for byte — and `-` reads
/// it from standard input, which is what a heredoc gives. M13 task 9.
#[test]
fn a_text_comes_from_a_file_or_from_standard_input_as_written() {
    let dir = scratch("fromfile");
    let task = "Fix the \"login\" bug.\nDo not touch `$HOME` or 'quoted' paths.\n\nReport back.\n";
    let file = dir.path().join("task.md");
    std::fs::write(&file, task).unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();

    // From a file, for `session new`.
    let socket = dir.path().join("a.sock");
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        Answer::SessionStarted {
            session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
            worktree: worktree.clone(),
        },
    );
    let out = guild()
        .args(["session", "new", "--from-file"])
        .arg(&file)
        .arg("--worktree")
        .arg(&worktree)
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(sent["prompt"], task);

    // From standard input, for `agent send`.
    let socket = dir.path().join("b.sock");
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        Answer::Sent {
            session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
        },
    );
    let mut child = guild()
        .args([
            "agent",
            "send",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--from-file",
            "-",
        ])
        .env("GUILD_SOCKET", &socket)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(task.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(sent["text"], task);
}

/// **Two texts, no text, or an empty one is a mistake in the command**,
/// said before any window is asked.
#[test]
fn a_text_is_given_once_and_is_not_empty() {
    let dir = scratch("notext");
    let empty = dir.path().join("empty.md");
    std::fs::write(&empty, "  \n").unwrap();
    let run = |args: &[&str]| {
        guild()
            .args(args)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap()
    };

    let both = run(&[
        "agent",
        "send",
        "x",
        "--prompt",
        "a",
        "--from-file",
        empty.to_str().unwrap(),
    ]);
    assert_eq!(both.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&both.stderr).contains("not both"));

    let none = run(&["agent", "send", "x"]);
    assert_eq!(none.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&none.stderr).contains("--prompt or --from-file"));

    let blank = run(&["agent", "send", "x", "--from-file", empty.to_str().unwrap()]);
    assert_eq!(blank.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&blank.stderr).contains("empty"));

    let missing = run(&["session", "new", "--from-file", "/no/such/task.md"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("/no/such/task.md"));
}

/// Play the window for several requests in a row, one answer each.
fn windows(listener: UnixListener, answers: Vec<Answer>) -> std::thread::JoinHandle<usize> {
    std::thread::spawn(move || {
        let mut served = 0;
        for answer in answers {
            let (mut stream, _) = listener.accept().unwrap();
            let _request = request(&mut stream);
            let body = serde_json::to_vec(&answer).unwrap();
            let head = format!("HTTP/1.1 200 \r\nContent-Length: {}\r\n\r\n", body.len());
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            served += 1;
        }
        served
    })
}

fn agent(status: &str, turned: bool) -> Agent {
    Agent {
        session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
        harness: Some(guild_session::HarnessKind::Claude),
        conversation: None,
        worktree: PathBuf::from("/w/guild-feat"),
        status: status.to_owned(),
        kind: None,
        turned,
        task: true,
        open: true,
        exited: false,
        workspace: "workspace-1".into(),
        tab: true,
    }
}

/// **An open record no tab of this window holds is marked** — its tab is in
/// another window, or in a workspace this window does not have — so the
/// list does not read as if the agent ran here.
#[test]
fn agent_list_marks_an_open_record_without_a_tab_here() {
    let dir = scratch("notab");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let elsewhere = Agent {
        session: "7c1e2d3f-0000-4000-8000-000000000001".to_owned(),
        tab: false,
        ..agent("unknown", true)
    };
    let _served = windows(
        listener,
        vec![Answer::Agents {
            agents: vec![agent("working", true), elsewhere],
        }],
    );
    let out = guild()
        .args(["agent", "list"])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = |id: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(id))
            .unwrap()
            .to_owned()
    };
    assert!(
        line("7c1e2d3f").contains("no tab in this window"),
        "{stdout}"
    );
    assert!(
        !line("0b6b0a52").contains("no tab"),
        "a record held by a tab here is not marked: {stdout}"
    );
}

/// **`wait` does not return while the agent works, nor before its first
/// turn** — a session just started reads `idle` until the hook for its first
/// turn arrives. It asks until the answer is finished, and prints that.
#[test]
fn agent_wait_asks_until_the_turn_has_ended() {
    let dir = scratch("wait");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let served = windows(
        listener,
        vec![
            Answer::Agents {
                agents: vec![agent("idle", false)],
            },
            Answer::Agents {
                agents: vec![agent("working", false)],
            },
            Answer::Agents {
                agents: vec![agent("done", true)],
            },
        ],
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "30",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        served.join().unwrap(),
        3,
        "it asked until the third answer, and no further"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("done"));
}

/// A wait that runs out of patience says so, and says what the session was
/// still doing — with its own exit code, because *gave up* is not *failed*.
#[test]
fn agent_wait_gives_up_after_its_timeout() {
    let dir = scratch("waitto");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let _served = windows(
        listener,
        (0..10)
            .map(|_| Answer::Agents {
                agents: vec![agent("working", true)],
            })
            .collect(),
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "0",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("still working"));
}

/// **A wait on a session that never reports says so at its limit** —
/// OSC 7501: `unknown` is not a generic timeout, it is no status report.
#[test]
fn agent_wait_on_a_session_without_a_report_says_none_was_received() {
    let dir = scratch("waitnone");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let _served = windows(
        listener,
        (0..10)
            .map(|_| Answer::Agents {
                agents: vec![agent("unknown", false)],
            })
            .collect(),
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "0",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(5));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no status report received (OSC 7501)"),
        "{stderr}"
    );
}

/// **A wait right after `session new` outlasts the program's start** — a
/// freshly started session has no report until its program runs and answers
/// the probe; the wait keeps asking through `unknown`, then working, and
/// ends with the finished turn.
#[test]
fn agent_wait_keeps_asking_until_a_new_session_reports() {
    let dir = scratch("waitnew");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let served = windows(
        listener,
        vec![
            Answer::Agents {
                agents: vec![agent("unknown", false)],
            },
            Answer::Agents {
                agents: vec![agent("unknown", false)],
            },
            Answer::Agents {
                agents: vec![agent("working", false)],
            },
            Answer::Agents {
                agents: vec![agent("done", true)],
            },
        ],
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "30",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(served.join().unwrap(), 4);
    assert!(String::from_utf8_lossy(&out.stdout).contains("done"));
}

/// **A wait on a task whose tab was closed says so and fails** — M5.12,
/// decision 16. It used to exit 0: closing marks the turn as soon as the
/// transcript holds a record, so `idle` with `turned` read as finished, and
/// an agent went on to read a result that was never written.
#[test]
fn agent_wait_on_a_closed_task_says_so_and_fails() {
    let dir = scratch("waitclosed");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let closed = Agent {
        open: false,
        ..agent("idle", true)
    };
    let _served = windows(
        listener,
        (0..10)
            .map(|_| Answer::Agents {
                agents: vec![closed.clone()],
            })
            .collect(),
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "1",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d closed"),
        "{stdout}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("guild agent read"),
        "the agent is told where what it said is"
    );
}

/// **A wait on a task whose process ended says so and fails**, rather than
/// running out its clock on a board that will never move again.
#[test]
fn agent_wait_on_an_ended_process_says_so_and_fails() {
    let dir = scratch("waitexited");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let exited = Agent {
        exited: true,
        ..agent("working", true)
    };
    let _served = windows(
        listener,
        (0..10)
            .map(|_| Answer::Agents {
                agents: vec![exited.clone()],
            })
            .collect(),
    );

    let out = guild()
        .args([
            "agent",
            "wait",
            "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d",
            "--timeout",
            "1",
        ])
        .env("GUILD_SOCKET", &socket)
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d exited"),
        "{stdout}"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("guild agent read"));
}

/// `read` needs no window: the transcript is `claude`'s file, found by the
/// session's id under `~/.claude/projects`, and what comes back is the
/// agent's words.
#[test]
fn agent_read_prints_what_the_agent_last_said() {
    let home = scratch("readhome");
    let folder = home
        .path()
        .join(".claude")
        .join("projects")
        .join("-w-guild-feat");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d.jsonl"),
        [
            r#"{"type":"user","message":{"content":"fix the build"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Fixed it."}]}}"#,
        ]
        .join("\n"),
    )
    .unwrap();

    let socket = home.path().join("s.sock");
    let mut a = agent("done", true);
    a.conversation = Some(a.session.clone());
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        Answer::Agents { agents: vec![a] },
    );
    let out = guild()
        .args(["agent", "read", "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d"])
        .env("HOME", home.path())
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Fixed it.");
    asked.join().unwrap();
}

/// **Every command the instructions name is a command.** An agent plans
/// around what it is told it has, so a text naming a command that does not
/// exist is an attempt spent on nothing — the lesson `ide::tools()` carries
/// for the IDE protocol, met from the command line. Each example line of
/// each topic is run as written, placeholders filled, with no window: a real
/// command is refused for want of a window or of a folder, never with
/// *no such command*.
#[test]
fn every_command_the_instructions_name_exists() {
    let home = scratch("home");
    let topics = ["", "worktrees", "parallel", "rules"];
    let mut ran = 0;
    for topic in topics {
        let mut args = vec!["instructions"];
        if !topic.is_empty() {
            args.push(topic);
        }
        let text = String::from_utf8(guild().args(&args).output().unwrap().stdout).unwrap();
        assert!(!text.is_empty(), "no text for {topic:?}");

        for line in text.lines() {
            // An example is indented; a heading that happens to begin with
            // the word `guild` is not.
            if !line.starts_with(' ') {
                continue;
            }
            let trimmed = line.trim_start();
            if !trimmed.starts_with("guild ") {
                continue;
            }
            // A table row: the command, then two spaces or more, then what it does.
            let command = trimmed.split("  ").next().unwrap_or(trimmed);
            let words = example_words(command);
            // `guild instructions …` itself is not an example of a task.
            if words.first().map(String::as_str) == Some("instructions") {
                continue;
            }
            let out = guild()
                .args(&words)
                .env_remove("GUILD_SOCKET")
                .env("HOME", home.path())
                .output()
                .unwrap();
            let said = String::from_utf8_lossy(&out.stderr);
            assert!(
                !said.contains("no such command"),
                "the instructions name `{command}`, which is not a command: {said}"
            );
            ran += 1;
        }
    }
    assert!(
        ran >= 8,
        "only {ran} examples found — the instructions lost their commands"
    );
}

/// **What a session is told at its start names only commands that exist.**
/// The window appends it to every `claude` it launches, and an agent plans
/// around what it is told it has — a renamed command there is an agent
/// sent to a door that is not there, in every session, silently.
#[test]
fn what_a_session_is_told_names_only_commands_that_exist() {
    let home = scratch("home");
    let spans: Vec<&str> = guild_session::command::SESSION_PROMPT
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| span.starts_with("guild "))
        .collect();
    assert!(!spans.is_empty(), "the prompt names no `guild` command");
    for span in spans {
        let out = guild()
            .args(example_words(span))
            .env_remove("GUILD_SOCKET")
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("no such command"),
            "`{span}` is not a command"
        );
    }
}

/// The words of an example line, as a shell would split them, with its
/// placeholders filled and its optional parts left out.
fn example_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in command.trim_start_matches("guild ").chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    let mut filled = Vec::new();
    let mut skipping = false;
    for word in words {
        if word.starts_with('[') || word.starts_with('(') {
            skipping = !(word.ends_with(']') || word.ends_with(')'));
            continue;
        }
        if skipping {
            skipping = !(word.ends_with(']') || word.ends_with(')'));
            continue;
        }
        filled.push(match word.as_str() {
            "<SESSION>" => "00000000-0000-0000-0000-000000000000".to_owned(),
            "NAME" => "x".to_owned(),
            "PATH" => ".".to_owned(),
            other => other.to_owned(),
        });
    }
    filled
}

/// A Claude file with the Guild id must not hide a Codex conversation.
#[test]
fn agent_read_uses_the_windows_exact_codex_conversation() {
    let home = scratch("cxread");
    let session = "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d";
    let conversation = "11111111-2222-3333-4444-555555555555";
    let claude = home.path().join(".claude/projects/p");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join(format!("{session}.jsonl")),
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Wrong conversation"}]}}"#).unwrap();
    let rollouts = home.path().join(".codex/sessions/2026/10/02");
    std::fs::create_dir_all(&rollouts).unwrap();
    std::fs::write(rollouts.join(format!("rollout-2026-10-02T12-00-00-{conversation}.jsonl")), format!(
        "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{conversation}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"Exact Codex reply\"}}]}}}}\n"
    )).unwrap();
    let socket = home.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    // Deserialize a future wire field: the regression also compiles before
    // the window/CLI know it, when read incorrectly ignores the socket.
    let a: Agent = serde_json::from_value(serde_json::json!({
        "session": session, "worktree":"/w", "status":"unknown", "turned":false,
        "task":true, "open":true, "harness":"codex", "conversation":conversation
    }))
    .unwrap();
    let asked = window(listener, Answer::Agents { agents: vec![a] });
    let out = guild()
        .args(["agent", "read", session, "--json"])
        .env("HOME", home.path())
        .env("CODEX_HOME", home.path().join(".codex"))
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let reply: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(reply["replies"], serde_json::json!(["Exact Codex reply"]));
    assert!(asked.join().unwrap().contains("agent_list"));
}

#[test]
fn agent_read_opencode_exports_only_the_mapped_conversation() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("ocread");
    let program = dir.path().join("opencode");
    std::fs::write(&program, r#"#!/bin/sh
[ "$1 $2 $3" = 'session export ses_exact' ] || exit 9
printf '%s' '{"info":{"id":"ses_exact"},"messages":[{"type":"assistant","content":[{"type":"text","text":"OpenCode reply"}]}]}'
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let socket = dir.path().join("s.sock");
    let mut a = agent("done", true);
    a.harness = Some(guild_session::HarnessKind::OpenCode);
    a.conversation = Some("ses_exact".into());
    a.worktree = dir.path().into();
    let id = a.session.clone();
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        Answer::Agents { agents: vec![a] },
    );
    let out = guild()
        .args(["agent", "read", &id])
        .env("GUILD_SOCKET", socket)
        .env("PATH", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "OpenCode reply"
    );
    asked.join().unwrap();
}

#[test]
fn agent_read_refuses_missing_mapping_and_unknown_telemetry_does_not_finish_wait() {
    for harness in guild_session::HarnessKind::ALL {
        let dir = scratch("unbound");
        let socket = dir.path().join("s.sock");
        let mut a = agent("unknown", true);
        a.harness = Some(harness);
        let id = a.session.clone();
        for args in [
            vec!["agent", "read", &id],
            vec!["agent", "wait", &id, "--timeout", "0"],
        ] {
            let listener = UnixListener::bind(&socket).unwrap();
            let asked = window(
                listener,
                Answer::Agents {
                    agents: vec![a.clone()],
                },
            );
            let out = guild()
                .args(&args)
                .env("GUILD_SOCKET", &socket)
                .output()
                .unwrap();
            assert!(!out.status.success());
            if args[1] == "wait" {
                assert_eq!(out.status.code(), Some(5));
            } else {
                assert!(String::from_utf8_lossy(&out.stderr).contains("conversation identity"));
            }
            asked.join().unwrap();
            std::fs::remove_file(&socket).unwrap();
        }
    }
}

#[test]
fn overlay_open_keeps_literal_text_and_calling_session_on_the_wire() {
    let dir = scratch("overlay");
    let socket = dir.path().join("s.sock");
    let body = dir.path().join("log.txt");
    std::fs::write(&body, "first line\n$(not a command)\n").unwrap();
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        Answer::Overlays { overlays: vec![] },
    );
    let out = guild()
        .args([
            "overlay",
            "open",
            "build",
            "--title",
            "Build output",
            "--from-file",
        ])
        .arg(&body)
        .arg("--json")
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "own-session")
        .output()
        .unwrap();
    assert!(out.status.success());
    let request = asked.join().unwrap();
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    let command: guild_session::command::Command = serde_json::from_str(body).unwrap();
    assert_eq!(
        command,
        guild_session::command::Command::Overlay {
            operation: "open".into(),
            id: "build".into(),
            title: "Build output".into(),
            text: "first line\n$(not a command)\n".into(),
            caller: Some("own-session".into()),
        }
    );
}

fn terminal_answer(state: guild_session::terminal_job::JobState) -> Answer {
    use guild_session::{ProjectId, SessionId, terminal_job::*};
    Answer::Terminal {
        reply: TerminalReply::Job {
            receipt: Box::new(JobReceipt {
                version: 1,
                id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
                owner: SessionId::new(),
                project: ProjectId::new(),
                request_id: "literal-one".into(),
                title: "fixture".into(),
                started_at_ms: None,
                output_deleted: false,
                revmux_archive: None,
                launch_context: None,
                state,
                target: None,
                presentation: Presentation::Closed,
                exit_code: state.is_terminal().then_some(7),
                signal: None,
                cancellation: None,
                error: None,
            }),
        },
    }
}

#[test]
fn terminal_start_forwards_literal_descriptor_and_caller() {
    for field in ["archive_viewer", "revmux_archive"] {
        let dir = scratch("terminal");
        let socket = dir.path().join("s.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let owner = guild_session::SessionId::new();
        let mut descriptor = serde_json::json!({
            "version":1,"request_id":"literal-one","program":"/bin/echo",
            "args":["$HOME; `echo unsafe`", "with spaces", "строка\nдва"],
            "cwd":dir.path(),"target":{"kind":"session","id":owner}
        });
        let archive =
            serde_json::json!({"directory":"/tmp/saved round", "program":"/opt/custom/viewer"});
        descriptor[field] = archive.clone();
        let file = dir.path().join("request.json");
        std::fs::write(&file, descriptor.to_string()).unwrap();
        let asked = window(
            listener,
            terminal_answer(guild_session::terminal_job::JobState::Running),
        );
        let output = guild()
            .args(["terminal", "start", "--from-file"])
            .arg(file)
            .arg("--json")
            .env("GUILD_SOCKET", socket)
            .env("GUILD_SESSION", owner.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let request = asked.join().unwrap();
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["request"]["spec"]["archive_viewer"], archive);
        assert!(body["request"]["spec"].get("revmux_archive").is_none());
        assert_eq!(body["caller"], owner.to_string());
        assert_eq!(body["request"]["spec"]["args"], descriptor["args"]);
        assert_eq!(body["request"]["spec"]["cwd"], descriptor["cwd"]);
    }
}

#[test]
fn terminal_wait_returns_child_failure_without_calling_it_transport_failure() {
    let dir = scratch("termwait");
    let socket = dir.path().join("s.sock");
    let asked = window(
        UnixListener::bind(&socket).unwrap(),
        terminal_answer(guild_session::terminal_job::JobState::Exited),
    );
    let output = guild()
        .args([
            "terminal",
            "wait",
            "11111111-1111-4111-8111-111111111111",
            "--timeout",
            "1",
            "--json",
        ])
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(asked.join().unwrap().contains("status"));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["reply"]["receipt"]["state"], "exited");
    assert_eq!(value["reply"]["receipt"]["exit_code"], 7);
}

#[test]
fn terminal_status_transport_error_is_distinct() {
    let dir = scratch("termerr");
    let output = guild()
        .args([
            "terminal",
            "status",
            "11111111-1111-4111-8111-111111111111",
            "--json",
        ])
        .env("GUILD_SOCKET", dir.path().join("absent.sock"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not reach"));
}

#[test]
fn terminal_wait_timeout_does_not_cancel() {
    let dir = scratch("termpending");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let server = std::thread::spawn(move || {
        let mut operations = Vec::new();
        while !flag.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                operations.push(request(&mut stream));
                // The window's next reply crosses the wait deadline. The last
                // pending receipt must survive this ordinary timeout boundary.
                if operations.len() == 2 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                let body = serde_json::to_vec(&terminal_answer(
                    guild_session::terminal_job::JobState::Running,
                ))
                .unwrap();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 \r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        operations
    });
    let output = guild()
        .args([
            "terminal",
            "wait",
            "11111111-1111-4111-8111-111111111111",
            "--timeout",
            "1",
            "--json",
        ])
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let operations = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["reply"]["receipt"]["state"], "running");
    assert!(operations.len() >= 2);
    assert!(
        operations
            .iter()
            .all(|r| r.contains("status") && !r.contains("cancel"))
    );
}

#[test]
fn continuation_resume_passes_literal_file_or_stdin_with_a_distinct_wire_tag() {
    use std::io::Write;
    for stdin in [false, true] {
        let dir = scratch("handoff");
        let socket = dir.path().join("s.sock");
        let asked = window(UnixListener::bind(&socket).unwrap(), Answer::Done);
        let task = "--literal \nпродолжит $HOME; `no shell`";
        let file = dir.path().join("task");
        std::fs::write(&file, task).unwrap();
        let mut child = guild()
            .args([
                "session",
                "resume",
                "41a339e5-263b-47a8-a6c2-92afe1fef306",
                "--from-file",
            ])
            .arg(if stdin {
                std::path::Path::new("-")
            } else {
                &file
            })
            .args(["--request-id", "handoff-one", "--json"])
            .env("GUILD_SOCKET", &socket)
            .env_remove("GUILD_SESSION")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        if stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(task.as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = asked.join().unwrap();
        let wire: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(wire["command"], "session_resume_task");
        assert_eq!(wire["prompt"], task);
        assert_eq!(wire["request_id"], "handoff-one");
        assert!(wire["caller"].is_null());
        assert_eq!(wire["front"], false);
    }
}

#[test]
fn continuation_rejects_invalid_text_and_flags_before_contacting_a_window() {
    let dir = scratch("invalid-handoff");
    let file = dir.path().join("task");
    for bytes in [
        b"".as_slice(),
        b" ",
        b"bad\0prompt",
        &[0xff],
        &vec![b'x'; 32769],
    ] {
        std::fs::write(&file, bytes).unwrap();
        let output = guild()
            .args(["session", "resume", "task", "--from-file"])
            .arg(&file)
            .args(["--request-id", "one"])
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("GUILD_SOCKET is not set"));
    }
    std::fs::write(&file, "continue").unwrap();
    for args in [
        vec!["--from-file", file.to_str().unwrap()],
        vec!["--request-id", "one"],
        vec!["--prompt", "continue", "--request-id", "one"],
        vec![
            "--from-file",
            file.to_str().unwrap(),
            "--request-id",
            "one",
            "--harness",
            "codex",
        ],
    ] {
        let output = guild()
            .args(["session", "resume", "task"])
            .args(args)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("GUILD_SOCKET is not set"));
    }
}

#[test]
fn continuation_status_uses_the_same_external_operator_namespace() {
    let dir = scratch("handoff-status");
    let socket = dir.path().join("s.sock");
    let asked = window(UnixListener::bind(&socket).unwrap(), Answer::Done);
    let output = guild()
        .args([
            "session",
            "continuation-status",
            "task",
            "--request-id",
            "checkpoint",
            "--json",
        ])
        .env("GUILD_SOCKET", &socket)
        .env_remove("GUILD_SESSION")
        .output()
        .unwrap();
    assert!(output.status.success());
    let raw = asked.join().unwrap();
    let wire: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(wire["command"], "session_continuation_status");
    assert_eq!(wire["request_id"], "checkpoint");
    assert!(wire["caller"].is_null());
}

#[test]
fn public_discovery_commands_work_without_a_window() {
    for args in [vec!["help"], vec!["instructions"]] {
        let out = guild()
            .args(args)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(!out.stdout.is_empty());
    }
    let rules = guild().args(["instructions", "rules"]).output().unwrap();
    let text = String::from_utf8(rules.stdout).unwrap();
    assert!(text.contains("window refuses"));
    assert!(!text.contains("wait for the answer before anything is created"));
}

#[test]
fn workflow_capabilities_use_only_the_owning_socket() {
    let dir = scratch("workflow");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(listener, Answer::Done);
    let output = guild()
        .args(["workflow", "capabilities", "--json"])
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = asked.join().unwrap();
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["command"], "workflow");
    assert_eq!(
        body["request"],
        serde_json::json!({"operation":"capabilities","version":1})
    );
}

#[test]
fn workflow_rejects_unknown_protocol_and_missing_mutation_context_without_a_socket() {
    let dir = scratch("workflow");
    let file = dir.path().join("request.json");
    for payload in [
        serde_json::json!({"version":99,"request_id":"version-probe","expected_revision":0,
            "origin":guild_session::SessionId::new(),"target":dir.path(),"selection":null,
            "definition":null,"overrides":null,"task":"Controlled protocol version probe",
            "architecture_only":false,"checks":[]}),
        serde_json::json!({"version":1}),
    ] {
        std::fs::write(&file, payload.to_string()).unwrap();
        let output = guild()
            .args(["workflow", "start", "--from-file"])
            .arg(&file)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("workflow input"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if payload["version"] == 99 {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("workflow.version"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(!String::from_utf8_lossy(&output.stderr).contains("GUILD_SOCKET is not set"));
    }
}

#[test]
fn workflow_checkpoint_forwards_typed_context_and_preserves_structured_refusal() {
    use guild_session::{SessionId, workflow::*};
    let directory = scratch("workflow-proof");
    let socket = directory.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let run = RunId::new();
    let caller = SessionId::new();
    let source = SourceIdentity::unversioned(directory.path().into(), vec![]).unwrap();
    let proof = serde_json::json!({"version":1,"request_id":"proof-one","expected_revision":2,"step":"contract","outcome":"contract","evidence":[{
        "id":"11111111-1111-4111-8111-111111111111","producer":caller,"contract":digest(b"accepted"),"source":source,
        "scenario":"contract","step":"contract","tool":{"program":"fixture","version":"1","argv":["fixture","$HOME; `echo unsafe`"]},
        "observed_at_ms":1,"artifacts":[],"observation":{"kind":"claim","message":"done\n'quoted'"}
    }]});
    let file = directory.path().join("checkpoint.json");
    std::fs::write(&file, proof.to_string()).unwrap();
    let asked = window(
        listener,
        Answer::Workflow {
            reply: WorkflowReply::Gap {
                code: "evidence".into(),
                message: "authored done claim is not proof".into(),
                run: Some(run),
                revision: Some(2),
            },
        },
    );
    let output = guild()
        .args(["workflow", "checkpoint", &run.to_string(), "--from-file"])
        .arg(file)
        .arg("--json")
        .env("GUILD_SOCKET", socket)
        .env("GUILD_SESSION", caller.to_string())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let raw = asked.join().unwrap();
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["caller"], caller.to_string());
    assert_eq!(body["request"]["run"], run.to_string());
    assert_eq!(body["request"]["spec"], proof);
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["reply"]["code"], "evidence");
    assert_eq!(reply["reply"]["revision"], 2);
}

#[test]
fn workflow_start_forwards_project_dependent_overrides_to_the_owning_window() {
    let dir = scratch("wf-layer");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let owner = std::thread::spawn(move || {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let got = request(&mut stream);
                    let body = serde_json::to_vec(&Answer::Done).unwrap();
                    write!(
                        stream,
                        "HTTP/1.1 200 \r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    stream.write_all(&body).unwrap();
                    return Some(got);
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < until =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                Err(_) => return None,
            }
        }
    });
    let mut steps = serde_json::to_value(
        guild_session::workflow::Definition::shipped()
            .unwrap()
            .steps,
    )
    .unwrap();
    steps[0]["role"] = serde_json::json!("fixture-parent");
    let payload = serde_json::json!({"version":1,"request_id":"project-overlay","expected_revision":0,
        "origin":guild_session::SessionId::new(),"target":dir.path(),"selection":null,
        "definition":".guild/workflows/fixture.json","task":"Controlled socket transport only",
        "overrides":{"steps":steps}});
    let file = dir.path().join("start.json");
    std::fs::write(&file, payload.to_string()).unwrap();
    let output = guild()
        .args(["workflow", "start", "--from-file"])
        .arg(&file)
        .arg("--json")
        .env("GUILD_SOCKET", socket)
        .output()
        .unwrap();
    let received = owner.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = received.expect("actual owning socket received the request");
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["request"]["spec"]["overrides"], payload["overrides"]);
    for overlay in [
        serde_json::json!({"unknown":true}),
        serde_json::json!({"steps":[{"id":"bad","label":"Bad","action":"execute.shell","role":"fixture-parent","depends_on":[],"outcomes":[]}]}),
    ] {
        let mut invalid = payload.clone();
        invalid["overrides"] = overlay;
        std::fs::write(&file, invalid.to_string()).unwrap();
        let output = guild()
            .args(["workflow", "start", "--from-file"])
            .arg(&file)
            .env_remove("GUILD_SOCKET")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("workflow input"));
    }
}

#[test]
fn workflow_review_forwards_the_explicit_parent_request_without_local_launch() {
    for operation in ["review", "retry"] {
        let dir = scratch("wf-review");
        let socket = dir.path().join("s.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let owner = std::thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let got = request(&mut stream);
                        let body = serde_json::to_vec(&Answer::Done).unwrap();
                        write!(
                            stream,
                            "HTTP/1.1 200 \r\nContent-Length: {}\r\n\r\n",
                            body.len()
                        )
                        .unwrap();
                        stream.write_all(&body).unwrap();
                        return Some(got);
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < until =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(1))
                    }
                    Err(_) => return None,
                }
            }
        });
        let run = guild_session::workflow::RunId::new();
        let spec = serde_json::json!({"version":1,"request_id":"plan-review","expected_revision":4,"step":"plan-review","package":"/explicit/selected/package","config_dir":null,"profile":null,"scope":"Literal $HOME `scope`\nReview this accepted contract only.","allowance":2,"keep_open":true,"prior":[]});
        let spec = if operation == "retry" {
            serde_json::json!({"version":1,"request_id":"partial-launch","expected_revision":8,"attempt":guild_session::workflow::AttemptId::new()})
        } else {
            spec
        };
        let file = dir.path().join("request.json");
        std::fs::write(&file, spec.to_string()).unwrap();
        let output = guild()
            .args(["workflow", operation, &run.to_string(), "--from-file"])
            .arg(file)
            .arg("--json")
            .env("GUILD_SOCKET", socket)
            .output()
            .unwrap();
        let received = owner.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = received.unwrap();
        let got: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(got["request"]["operation"], operation);
        assert_eq!(got["request"]["spec"], spec);
    }
}

#[test]
fn workflow_recover_is_an_owning_socket_request_not_a_local_launcher() {
    use guild_session::{SessionId, workflow::*};
    let directory = scratch("workflow-recover");
    let socket = directory.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let run = RunId::new();
    let caller = SessionId::new();
    let spec =
        serde_json::json!({"version":1,"request_id":"same-origin-once","expected_revision":7});
    let file = directory.path().join("recover.json");
    std::fs::write(&file, spec.to_string()).unwrap();
    let asked = window(
        listener,
        Answer::Workflow {
            reply: WorkflowReply::Gap {
                code: "holder_unknown".into(),
                message: "No exact holder proof; no launch".into(),
                run: Some(run),
                revision: Some(7),
            },
        },
    );
    let output = guild()
        .args(["workflow", "recover", &run.to_string(), "--from-file"])
        .arg(file)
        .arg("--json")
        .env("GUILD_SOCKET", socket)
        .env("GUILD_SESSION", caller.to_string())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let raw = asked.join().unwrap();
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["caller"], caller.to_string());
    assert_eq!(body["request"]["operation"], "recover");
    assert_eq!(body["request"]["run"], run.to_string());
    assert_eq!(body["request"]["spec"], spec);
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer["reply"]["code"], "holder_unknown");
}

#[test]
fn workflow_reconcile_transports_assessment_and_explicit_ack_without_local_effects() {
    use guild_session::{SessionId, workflow::*};
    for assessment in [None, Some(digest(b"actual frozen assessment"))] {
        let directory = scratch("workflow-reconcile");
        let socket = directory.path().join("s.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let run = RunId::new();
        let caller = SessionId::new();
        let spec = serde_json::json!({"version":1,"request_id":"parent-reconciliation","expected_revision":22,"assessment":assessment});
        let file = directory.path().join("reconcile.json");
        std::fs::write(&file, spec.to_string()).unwrap();
        let asked = window(
            listener,
            Answer::Workflow {
                reply: WorkflowReply::Gap {
                    code: "reconciliation_blocked".into(),
                    message: "Unanswered permission; no launch".into(),
                    run: Some(run),
                    revision: Some(22),
                },
            },
        );
        let output = guild()
            .args(["workflow", "reconcile", &run.to_string(), "--from-file"])
            .arg(&file)
            .arg("--json")
            .env("GUILD_SOCKET", &socket)
            .env("GUILD_SESSION", caller.to_string())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let raw = asked.join().unwrap();
        let body: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["request"]["operation"], "reconcile");
        assert_eq!(body["caller"], caller.to_string());
        assert_eq!(body["request"]["spec"], spec);
        let mut invalid = spec;
        invalid["assessment"] = "not-a-sha256".into();
        std::fs::write(&file, invalid.to_string()).unwrap();
        let output = guild()
            .args(["workflow", "reconcile", &run.to_string(), "--from-file"])
            .arg(file)
            .env("GUILD_SOCKET", socket)
            .env("GUILD_SESSION", caller.to_string())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn public_workflow_instructions_supply_a_structurally_valid_start_without_private_source_hashes() {
    use guild_session::{SessionId, workflow::StartRequest};
    let output = guild()
        .args(["workflow", "instructions"])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let example = text
        .split_once("```json\n")
        .unwrap()
        .1
        .split_once("```")
        .unwrap()
        .0;
    let mut value: serde_json::Value = serde_json::from_str(example).unwrap();
    value["origin"] = serde_json::json!(SessionId::new());
    let request: StartRequest = serde_json::from_value(value).unwrap();
    request.validate().unwrap();
    assert!(!text.contains("SESSION_PROMPT"));
}

#[test]
fn removed_native_review_commands_fail_locally_without_contacting_the_window() {
    for args in [
        vec!["review", "ready"],
        vec!["review", "launch"],
        vec!["review", "wait"],
        vec!["instructions", "review"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_guild"))
            .args(&args)
            .env("GUILD_SOCKET", "/missing-native-review-test.sock")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("no such command") || error.contains("no instructions on"),
            "{args:?}: {error}"
        );
        assert!(
            !error.contains("could not reach"),
            "unsupported commands must fail locally"
        );
    }
}

#[test]
fn continuation_preflight_without_request_id_is_read_only_and_keeps_caller() {
    let dir = scratch("continuation-preflight");
    let socket = dir.path().join("s.sock");
    let asked = window(UnixListener::bind(&socket).unwrap(), Answer::Done);
    let output = guild()
        .args(["session", "continuation-status", "task", "--json"])
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "exact-controller")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = asked.join().unwrap();
    let wire: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(wire["command"], "session_continuation_status");
    assert!(wire.get("request_id").is_none());
    assert_eq!(wire["caller"], "exact-controller");
}

/// GLD-055: a group session is its own request, with who is asking.
#[test]
fn session_new_in_a_group_sends_the_group_variant_and_who_is_asking() {
    let dir = scratch("groupnew");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::SessionStarted {
            session: "0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d".to_owned(),
            worktree: "/w/acme".into(),
        },
    );
    let out = guild()
        .args([
            "session",
            "new",
            "--group",
            "acme",
            "--prompt",
            "where is AUTH_URL read?",
        ])
        .env("GUILD_SOCKET", &socket)
        .env("GUILD_SESSION", "11111111-2222-3333-4444-555555555555")
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        request.contains(r#""command":"session_new_in_group""#),
        "{request}"
    );
    assert!(request.contains(r#""group":"acme""#), "{request}");
    assert!(
        request.contains(r#""caller":"11111111-2222-3333-4444-555555555555""#),
        "{request}"
    );
    assert!(!request.contains("worktree"), "{request}");
    assert!(!request.contains("harness"), "{request}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("started 0b6b0a52-8f1e-4b41-9d6e-0f1c2a3b4c5d in /w/acme")
    );
}

/// `--group` and `--worktree` contradict each other: refused here, and the
/// window is never asked — there is no socket to ask.
#[test]
fn group_with_worktree_is_refused_locally() {
    let out = guild()
        .args(["session", "new", "--group", "acme", "--worktree", "."])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--group or --worktree"));
    let out = guild()
        .args(["worktree", "list", "--group", "acme"])
        .env_remove("GUILD_SOCKET")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn group_check_asks_the_window_and_prints_each_member() {
    use guild_session::command::{GroupCheck, GroupMember};
    let dir = scratch("groupcheck");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(
        listener,
        Answer::Groups {
            groups: vec![GroupCheck {
                name: "acme".to_owned(),
                workspace: "w".to_owned(),
                folder: Some("/w/acme".into()),
                matches: Some(false),
                members: vec![
                    GroupMember {
                        checkout: "/w/acme/backend".into(),
                        inside: Some(true),
                    },
                    GroupMember {
                        checkout: "/w/helm".into(),
                        inside: Some(false),
                    },
                ],
            }],
        },
    );
    let out = guild()
        .args(["group", "check", "acme"])
        .env("GUILD_SOCKET", &socket)
        .env_remove("GUILD_SESSION")
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.contains(r#""command":"group_check""#), "{request}");
    assert!(request.contains(r#""group":"acme""#), "{request}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("acme\t/w/acme\t1 of 2 inside"), "{said}");
    assert!(said.contains("  /w/helm\toutside"), "{said}");
}

/// A group named like a flag is reached after `--`.
#[test]
fn a_name_after_the_terminator_is_never_a_flag() {
    let dir = scratch("groupdash");
    let socket = dir.path().join("s.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let asked = window(listener, Answer::Groups { groups: Vec::new() });
    let out = guild()
        .args(["group", "check", "--", "--front"])
        .env("GUILD_SOCKET", &socket)
        .env_remove("GUILD_SESSION")
        .output()
        .unwrap();
    let request = asked.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(request.contains(r#""group":"--front""#), "{request}");
}
