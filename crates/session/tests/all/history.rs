//! What each harness remembers of a folder, read from its own store.
//!
//! Every fixture is a real record trimmed; no test reads the person's
//! own `~/.claude`, `~/.codex` or runs their `opencode`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::scratch::Scratch;
use guild_session::HarnessKind;
use guild_session::history::{self, History, Stores};

fn stores(dir: &Scratch) -> Stores {
    Stores {
        claude: dir.join("claude-projects"),
        codex: dir.join("codex"),
        path: dir.join("bin").into_os_string(),
    }
}

fn claude_session(stores: &Stores, folder: &Path, id: &str, records: &[String]) -> PathBuf {
    let dir = stores.claude.join(history::claude::folder_name(folder));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{id}.jsonl"));
    std::fs::write(&file, records.join("\n")).unwrap();
    file
}

fn user(cwd: &Path, words: &str) -> String {
    serde_json::json!({"type":"user","cwd":cwd,"message":{"role":"user","content":words}})
        .to_string()
}

#[test]
fn a_folder_s_name_is_every_other_character_a_dash() {
    assert_eq!(
        history::claude::folder_name(Path::new("/Users/p/work/guild/.claude/worktrees/x_y")),
        "-Users-p-work-guild--claude-worktrees-x-y"
    );
    assert_eq!(
        history::claude::folder_name(Path::new("/tmp/проба")),
        "-tmp------"
    );
}

#[test]
fn claude_s_sessions_of_the_folder_are_listed_newest_first_with_their_titles() {
    let dir = Scratch::new("history-claude-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    let old = claude_session(&stores, &folder, "old", &[user(&folder, "first words")]);
    claude_session(
        &stores,
        &folder,
        "new",
        &[
            user(&folder, "что за проект"),
            r#"{"type":"ai-title","aiTitle":"Start screen"}"#.to_owned(),
        ],
    );
    let hour_ago = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();

    let rows = History::default().recent(&stores, &folder, history::LIMIT);
    let seen: Vec<_> = rows
        .iter()
        .map(|p| (p.harness, p.id.as_str(), p.title.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (HarnessKind::Claude, "new", "Start screen"),
            (HarnessKind::Claude, "old", "first words"),
        ]
    );
}

/// Two folders can share a name — `a.b` and `a-b` both become `a-b` — so
/// a file is the folder's only when its own `cwd` says so.
#[test]
fn a_claude_file_whose_cwd_is_another_folder_is_not_listed() {
    let dir = Scratch::new("history-claude-cwd-");
    let stores = stores(&dir);
    let folder = dir.join("a.b");
    claude_session(
        &stores,
        &folder,
        "theirs",
        &[user(&dir.join("a-b"), "hello")],
    );
    assert!(History::default().recent(&stores, &folder, 50).is_empty());
}

/// A session's subagents live under `<id>/subagents/`; a stub has no
/// `user` record and nothing to resume.
#[test]
fn subagents_and_stubs_are_not_sessions() {
    let dir = Scratch::new("history-claude-sub-");
    let stores = stores(&dir);
    let folder = dir.join("g");
    claude_session(&stores, &folder, "real", &[user(&folder, "hi")]);
    claude_session(
        &stores,
        &folder,
        "stub",
        &[r#"{"type":"ai-title","aiTitle":"x"}"#.to_owned()],
    );
    let sub = stores
        .claude
        .join(history::claude::folder_name(&folder))
        .join("real/subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("agent-1.jsonl"), user(&folder, "sub")).unwrap();

    let ids: Vec<_> = History::default()
        .recent(&stores, &folder, 50)
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, ["real"]);
}

/// **Review focus 1.** A transcript of a hundred megabytes is read at its
/// two ends: the head to the first `user` record and the last 256 KiB.
#[test]
fn a_huge_transcript_is_read_at_its_ends() {
    struct Counting<R> {
        inner: R,
        read: u64,
    }
    impl<R: std::io::Read> std::io::Read for Counting<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.read += n as u64;
            Ok(n)
        }
    }
    impl<R: std::io::Seek> std::io::Seek for Counting<R> {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    let folder = Path::new("/w/guild");
    let mut text = user(folder, "hello");
    text.push('\n');
    let filler = format!(
        "{}\n",
        serde_json::json!({"type":"assistant","message":{"content":"x".repeat(1000)}})
    );
    while text.len() < 100 * 1024 * 1024 {
        text.push_str(&filler);
    }
    text.push_str(r#"{"type":"ai-title","aiTitle":"The end"}"#);
    let len = text.len() as u64;
    let mut file = Counting {
        inner: std::io::Cursor::new(text.into_bytes()),
        read: 0,
    };

    let read = history::claude::read(&mut file, len);
    assert_eq!(
        read.as_ref()
            .map(|(cwd, title)| (cwd.as_path(), title.as_str())),
        Some((folder, "The end"))
    );
    assert!(
        file.read <= (1024 + 256 + 64) * 1024,
        "read {} bytes",
        file.read
    );
}

/// **Final review.** The tail starts at `len - 256 KiB`, any byte at all
/// — and `claude` writes Cyrillic, em dashes and `⎿` raw, so that byte is
/// often the second half of a character. The cut first line is skipped
/// as bytes; read as text it was an `InvalidData` that dropped the whole
/// session, cached until the file changes, which a finished one never
/// does.
#[test]
fn a_tail_cut_inside_a_character_still_gives_the_title() {
    const TAIL: usize = 256 * 1024;
    let folder = Path::new("/w/guild");
    let mut text = user(folder, "привет");
    text.push('\n');
    let filler = format!(
        "{}\n",
        serde_json::json!({"type":"assistant","message":{"content":"я".repeat(10_000)}})
    );
    while text.len() < 2 * TAIL {
        text.push_str(&filler);
    }
    text.push_str(r#"{"type":"ai-title","aiTitle":"Конец"}"#);
    // One byte more at the end moves the cut one byte on; two-byte
    // letters alternate lead and continuation, so it lands inside one.
    while text.as_bytes()[text.len() - TAIL] & 0b1100_0000 != 0b1000_0000 {
        text.push(' ');
    }
    let len = text.len() as u64;

    let read = history::claude::read(&mut std::io::Cursor::new(text.into_bytes()), len);
    assert_eq!(
        read.as_ref()
            .map(|(cwd, title)| (cwd.as_path(), title.as_str())),
        Some((folder, "Конец"))
    );
}

/// A line that is not UTF-8 at all — torn by a crash, say — is one
/// malformed line, skipped like one that is not JSON, at either end:
/// it does not hide the lines after it, nor the session.
#[test]
fn a_line_that_is_not_utf8_hides_nothing_after_it() {
    let folder = Path::new("/w/guild");
    let mut bytes = b"{\"type\":\"summary\",\"x\":\"\xd1\"}\n".to_vec();
    bytes.extend_from_slice(user(folder, "hello").as_bytes());
    bytes.extend_from_slice(b"\n\xff\xfe not text\n");
    bytes.extend_from_slice(br#"{"type":"ai-title","aiTitle":"After"}"#);
    let len = bytes.len() as u64;

    let read = history::claude::read(&mut std::io::Cursor::new(bytes), len);
    assert_eq!(
        read.as_ref()
            .map(|(cwd, title)| (cwd.as_path(), title.as_str())),
        Some((folder, "After"))
    );
}

/// Two folders can share Claude's lossy directory (`a.b`/`a-b` both
/// become `a-b`); a cache is per file, not per folder, so the same
/// `History` must not answer one folder's question with the other's
/// cached session.
#[test]
fn claude_cache_does_not_leak_a_session_between_folders_sharing_a_directory() {
    let dir = Scratch::new("history-claude-leak-");
    let stores = stores(&dir);
    let folder_a = dir.join("a.b");
    let folder_b = dir.join("a-b");
    claude_session(&stores, &folder_a, "theirs", &[user(&folder_a, "hello")]);

    let mut history = History::default();
    assert_eq!(history.recent(&stores, &folder_a, 50).len(), 1);
    assert!(
        history.recent(&stores, &folder_b, 50).is_empty(),
        "folder b must not see folder a's cached session"
    );
}

/// **Final review.** One `History` serves a worktree switch, and going
/// A → B → A must not read A's transcripts again: asking about B keeps
/// A's answers. Observed through a file changed under its old mtime —
/// only a kept answer still says what the file said before.
#[test]
fn claude_cache_keeps_another_folder_s_answers() {
    let dir = Scratch::new("history-claude-keep-");
    let stores = stores(&dir);
    let folder_a = dir.join("a");
    let folder_b = dir.join("b");
    let file = claude_session(&stores, &folder_a, "a1", &[user(&folder_a, "before")]);
    claude_session(&stores, &folder_b, "b1", &[user(&folder_b, "other")]);
    let touched = std::fs::metadata(&file).unwrap().modified().unwrap();

    let mut history = History::default();
    let titles = |rows: Vec<history::Past>| rows.into_iter().map(|p| p.title).collect::<Vec<_>>();
    assert_eq!(titles(history.recent(&stores, &folder_a, 50)), ["before"]);
    assert_eq!(titles(history.recent(&stores, &folder_b, 50)), ["other"]);

    std::fs::write(&file, user(&folder_a, "after")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(touched)
        .unwrap();
    assert_eq!(
        titles(history.recent(&stores, &folder_a, 50)),
        ["before"],
        "A's transcript was read again after B was asked about"
    );
}

fn rollout(
    stores: &Stores,
    day: &str,
    stamp: &str,
    id: &str,
    meta: serde_json::Value,
    said: Option<&str>,
) {
    let dir = stores.codex.join("sessions").join(day);
    std::fs::create_dir_all(&dir).unwrap();
    let mut lines = vec![serde_json::json!({"type":"session_meta","payload":meta}).to_string()];
    if let Some(words) = said {
        lines.push(serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":words}}).to_string());
    }
    std::fs::write(
        dir.join(format!("rollout-{stamp}-{id}.jsonl")),
        lines.join("\n"),
    )
    .unwrap();
}

#[test]
fn codex_s_own_sessions_of_the_folder_are_listed_and_its_subagents_are_not() {
    let dir = Scratch::new("history-codex-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    let f = folder.to_string_lossy().into_owned();
    rollout(
        &stores,
        "2026/09/27",
        "2026-09-27T10-00-00",
        "u1",
        serde_json::json!({"id":"u1","cwd":f,"source":"cli","thread_source":"user"}),
        Some("Port the probe"),
    );
    rollout(
        &stores,
        "2026/09/27",
        "2026-09-27T11-00-00",
        "s1",
        serde_json::json!({"id":"s1","cwd":f,"source":{"subagent":{"thread_spawn":{}}},"thread_source":"subagent"}),
        Some("sub"),
    );
    rollout(
        &stores,
        "2026/09/27",
        "2026-09-27T12-00-00",
        "g1",
        serde_json::json!({"id":"g1","cwd":f,"source":{"subagent":{"other":"guardian"}},"thread_source":"guardian_review"}),
        None,
    );
    rollout(
        &stores,
        "2026/09/26",
        "2026-09-26T09-00-00",
        "o1",
        serde_json::json!({"id":"o1","cwd":"/elsewhere","source":"cli","thread_source":"user"}),
        Some("other"),
    );

    let rows = History::default().recent(&stores, &folder, 50);
    let seen: Vec<_> = rows
        .iter()
        .map(|p| (p.harness, p.id.as_str(), p.title.as_str()))
        .collect();
    assert_eq!(seen, [(HarnessKind::Codex, "u1", "Port the probe")]);
}

/// A thread a person named is called by that name.
#[test]
fn a_named_codex_thread_is_called_by_its_name() {
    let dir = Scratch::new("history-codex-name-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    rollout(
        &stores,
        "2026/09/27",
        "2026-09-27T10-00-00",
        "u1",
        serde_json::json!({"id":"u1","cwd":folder,"source":"cli","thread_source":"user"}),
        Some("first words"),
    );
    std::fs::write(
        stores.codex.join("session_index.jsonl"),
        r#"{"id":"u1","thread_name":"Покажи план доработок","updated_at":"2026-09-27T10:05:00Z"}"#,
    )
    .unwrap();

    let rows = History::default().recent(&stores, &folder, 50);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Покажи план доработок");
}

/// A rollout's raw lines, `session_meta` first — for a shape `rollout`
/// above cannot write, since `event_msg`/`user_message` is only one
/// CLI's; `lines` holds the rest verbatim.
fn rollout_lines(
    stores: &Stores,
    day: &str,
    stamp: &str,
    id: &str,
    meta: serde_json::Value,
    lines: &[serde_json::Value],
) {
    let dir = stores.codex.join("sessions").join(day);
    std::fs::create_dir_all(&dir).unwrap();
    let mut all = vec![serde_json::json!({"type":"session_meta","payload":meta}).to_string()];
    all.extend(lines.iter().map(serde_json::Value::to_string));
    std::fs::write(
        dir.join(format!("rollout-{stamp}-{id}.jsonl")),
        all.join("\n"),
    )
    .unwrap();
}

/// CLI 0.153.4 and 0.158.0 (measured on this machine, 2026-09-28) carry
/// the person's words as a `response_item` message of role `user`, not
/// an `event_msg`/`user_message` — and Codex injects its own role-`user`
/// context first: `<environment_context>` here, and AGENTS.md
/// instructions starting with `#`, both to be skipped over.
#[test]
fn a_codex_response_item_past_its_injected_context_is_the_title() {
    let dir = Scratch::new("history-codex-response-item-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    let f = folder.to_string_lossy().into_owned();
    let context = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"}]}});
    let agents = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions\nsomething"}]}});
    let said = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Check the probe depth"}]}});
    rollout_lines(
        &stores,
        "2026/09/27",
        "2026-09-27T13-00-00",
        "r1",
        serde_json::json!({"id":"r1","cwd":f,"source":"cli","thread_source":"user"}),
        &[context, agents, said],
    );

    let rows = History::default().recent(&stores, &folder, 50);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Check the probe depth");
}

/// **Final review, the same shape as Claude's tail.** A rollout is read
/// to a byte bound, so its lines are bytes until `serde_json` reads them:
/// one that is not UTF-8 is a malformed line, skipped, and the person's
/// words after it are still the title.
#[test]
fn a_codex_line_that_is_not_utf8_hides_nothing_after_it() {
    let dir = Scratch::new("history-codex-bytes-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    let meta = serde_json::json!({"type":"session_meta","payload":{"id":"b1","cwd":folder,"source":"cli","thread_source":"user"}});
    let said = serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"Слова после"}});
    let mut bytes = meta.to_string().into_bytes();
    bytes.extend_from_slice(b"\n\xff\xfe torn\n");
    bytes.extend_from_slice(said.to_string().as_bytes());
    let day = stores.codex.join("sessions/2026/09/27");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(day.join("rollout-2026-09-27T14-00-00-b1.jsonl"), bytes).unwrap();

    let rows = History::default().recent(&stores, &folder, 50);
    let titles: Vec<_> = rows.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, ["Слова после"]);
}

/// **Final review.** Codex's directory is one rule for everything Guild
/// does there — the hooks it installs and the rollouts it reads — so a
/// `$CODEX_HOME` does not put the two in different places.
#[test]
fn codex_s_home_is_codex_home_else_dot_codex() {
    use guild_session::harness::CodexHarness;
    assert_eq!(
        CodexHarness::home_in(Some("/c/home".into()), Some("/Users/p".into())),
        Path::new("/c/home")
    );
    assert_eq!(
        CodexHarness::home_in(None, Some("/Users/p".into())),
        Path::new("/Users/p/.codex")
    );
    assert_eq!(CodexHarness::home_in(None, None), Path::new(""));
}

/// A cache is per file, not per folder; the same `History` must not
/// leak one folder's cached session into another's answer (Task 7 keeps
/// one `History` across a worktree switch).
#[test]
fn codex_cache_does_not_leak_a_session_between_folders() {
    let dir = Scratch::new("history-codex-leak-");
    let stores = stores(&dir);
    let folder_a = dir.join("a");
    let folder_b = dir.join("b");
    let fa = folder_a.to_string_lossy().into_owned();
    rollout(
        &stores,
        "2026/09/27",
        "2026-09-27T10-00-00",
        "u1",
        serde_json::json!({"id":"u1","cwd":fa,"source":"cli","thread_source":"user"}),
        Some("Port the probe"),
    );

    let mut history = History::default();
    assert_eq!(history.recent(&stores, &folder_a, 50).len(), 1);
    assert!(
        history.recent(&stores, &folder_b, 50).is_empty(),
        "folder b must not see folder a's cached session"
    );
}

/// A stand-in `opencode`: checks it was asked for the list as JSON, then
/// runs `body` in the folder it was started in. The test's `PATH` is only
/// `<scratch>/bin`, so `body` names any tool it needs — `cat`, `sleep` —
/// by its absolute path; `echo`, `exit` and `wait` are `sh` builtins.
fn opencode(dir: &Scratch, body: &str) {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("opencode");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n[ \"$1 $2 $3 $4\" = \"session list --format json\" ] || exit 3\n{body}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

#[test]
fn opencode_s_sessions_of_the_folder_are_its_own_answer() {
    let dir = Scratch::new("history-opencode-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    std::fs::create_dir_all(&folder).unwrap();
    let answer = serde_json::json!([
        {"id":"ses_a","title":"Проверка присутствия","updated":1790355564437_u64,"created":1,"directory":folder,"projectId":"p"},
        {"id":"ses_b","title":"New session - 2026-09-27T10:00:00.000Z","updated":1790199515280_u64,"created":1,"directory":folder,"projectId":"p"},
        {"id":"ses_c","title":"elsewhere","updated":1790399515280_u64,"created":1,"directory":"/other","projectId":"p"}
    ]);
    std::fs::write(dir.join("answer.json"), answer.to_string()).unwrap();
    opencode(
        &dir,
        &format!("/bin/cat '{}'", dir.join("answer.json").display()),
    );

    let rows = History::default().recent(&stores, &folder, 50);
    let seen: Vec<_> = rows
        .iter()
        .map(|p| (p.harness, p.id.as_str(), p.title.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (HarnessKind::OpenCode, "ses_a", "Проверка присутствия"),
            (HarnessKind::OpenCode, "ses_b", "New session"),
        ]
    );
}

/// **Review focus 5.** An `opencode` that hangs is killed at its deadline
/// — its whole process group, so a child it spawned does not outlive the
/// window's patience — and one that says something else is nothing; the
/// others still answer.
#[test]
fn an_opencode_that_hangs_or_babbles_gives_nothing() {
    for body in ["sleep 30", "echo 'not json'", "exit 1"] {
        let dir = Scratch::new("history-opencode-bad-");
        let stores = stores(&dir);
        let folder = dir.join("guild");
        std::fs::create_dir_all(&folder).unwrap();
        claude_session(&stores, &folder, "c", &[user(&folder, "still here")]);
        let pidfile = dir.join("sleep.pid");
        let script = if body == "sleep 30" {
            // Backgrounded, so its pid is its own — distinct from the
            // `sh` running it — and `wait` keeps `sh` blocked on it, so
            // the stand-in really hangs rather than exiting at once.
            format!("/bin/sleep 30 &\necho $! > '{}'\nwait", pidfile.display())
        } else {
            body.to_owned()
        };
        opencode(&dir, &script);

        let started = std::time::Instant::now();
        let ids: Vec<_> = History::default()
            .recent(&stores, &folder, 50)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["c"], "{body}");
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "{body}: {:?}",
            started.elapsed()
        );

        if body == "sleep 30" {
            // The group kill, not just the pid `ask` itself spawned: the
            // backgrounded `sleep` must not outlive `recent`'s return.
            let pid = std::fs::read_to_string(&pidfile).unwrap().trim().to_owned();
            let gone = (0..40).any(|_| {
                std::thread::sleep(Duration::from_millis(50));
                !std::process::Command::new("/bin/kill")
                    .args(["-0", &pid])
                    .status()
                    .unwrap()
                    .success()
            });
            assert!(gone, "sleep {pid} outlived recent()'s return");
        }
    }
}

/// **Fix rounds 1 and 2.** `opencode` can exit normally while something
/// it started keeps running, holding our end of the pipe open: once the
/// leader is reaped and the pipe is still held `GRACE` later, the group
/// is killed — not only at the deadline — so `recent` returns promptly
/// rather than waiting on that descendant's own exit. What was already
/// written before the kill is still the answer, since the reader thread
/// had already taken it from the pipe and was only waiting on the EOF
/// the kill supplies.
#[test]
fn opencode_s_own_exit_does_not_wait_for_what_it_left_running() {
    let dir = Scratch::new("history-opencode-orphan-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    std::fs::create_dir_all(&folder).unwrap();
    let answer = serde_json::json!([
        {"id":"ses_a","title":"Осталось после выхода","updated":1790355564437_u64,"created":1,"directory":folder,"projectId":"p"},
    ]);
    std::fs::write(dir.join("answer.json"), answer.to_string()).unwrap();
    // `sleep` is backgrounded without redirecting its stdout, so it
    // inherits the script's — our piped fd — and keeps its write end
    // open long after `cat` has written the answer and the script itself
    // has exited.
    opencode(
        &dir,
        &format!(
            "/bin/sleep 30 &\n/bin/cat '{}'",
            dir.join("answer.json").display()
        ),
    );

    let started = std::time::Instant::now();
    let rows = History::default().recent(&stores, &folder, 50);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    let seen: Vec<_> = rows
        .iter()
        .map(|p| (p.harness, p.id.as_str(), p.title.as_str()))
        .collect();
    assert_eq!(
        seen,
        [(HarnessKind::OpenCode, "ses_a", "Осталось после выхода")]
    );
}

#[test]
fn no_opencode_on_the_path_is_no_sessions() {
    let dir = Scratch::new("history-opencode-none-");
    let folder = dir.join("guild");
    std::fs::create_dir_all(&folder).unwrap();
    assert!(
        History::default()
            .recent(&stores(&dir), &folder, 50)
            .is_empty()
    );
}

/// OpenCode 2.0's own export is a typed message list, not Claude JSONL.
#[test]
fn opencode_export_reads_exact_assistant_text_without_reasoning() {
    let json = br#"{"info":{"id":"ses_exact"},"messages":[
        {"type":"user","text":"private input"},
        {"type":"assistant","finish":"stop","time":{"completed":42},"content":[
            {"type":"reasoning","text":"private reasoning"},
            {"type":"text","text":"First"}]},
        {"type":"assistant","content":[{"type":"text","text":"Last"}]}] }"#;
    assert_eq!(
        guild_session::history::opencode::replies_in(json, "ses_exact", 10).unwrap(),
        ["First", "Last"]
    );
    assert_eq!(
        guild_session::history::opencode::replies_in(json, "ses_exact", 1).unwrap(),
        ["Last"]
    );
    assert!(guild_session::history::opencode::replies_in(json, "ses_other", 1).is_err());
}

/// Actual v2.0.21 export metadata uses info.location.directory. The local
/// executable only returns saved JSON; it cannot start a session or a provider.
#[test]
fn opencode_recovery_export_qualifies_location_without_ambiguous_identity() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("history-opencode-qualification-");
    let stores = stores(&dir);
    let folder = dir.join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exporter = bin.join("opencode");
    std::fs::write(&exporter, format!(
        "#!/bin/sh\n[ \"$1 $2 $3 $4\" = \"session export ses_exact --standalone\" ] || exit 3\nexec /bin/cat '{}'\n",
        dir.join("export.json").display()
    )).unwrap();
    std::fs::set_permissions(&exporter, std::fs::Permissions::from_mode(0o755)).unwrap();
    let current = serde_json::json!({"info":{"id":"ses_exact","location":{"directory":folder}},"messages":[]});
    let mut cases = vec![("v2 location", current.clone(), true)];
    let mut both = current.clone();
    both["info"]["directory"] = serde_json::json!(folder);
    cases.push(("matching legacy and current", both.clone(), true));
    // Preserve the previously supported shape exercised by native fixtures.
    let legacy = serde_json::json!({"info":{"id":"ses_exact","directory":folder},"messages":[]});
    cases.push(("legacy only", legacy.clone(), true));
    let mut wrong_id = current.clone();
    wrong_id["info"]["id"] = serde_json::json!("ses_other");
    cases.push(("wrong id", wrong_id, false));
    let mut conflict = both.clone();
    conflict["info"]["location"]["directory"] = serde_json::json!("/another-project");
    cases.push((
        "correct legacy cannot override wrong current cwd",
        conflict,
        false,
    ));
    let mut conflict = both.clone();
    conflict["info"]["directory"] = serde_json::json!("/another-project");
    cases.push(("conflicting legacy cwd", conflict, false));
    for location in [
        serde_json::Value::Null,
        serde_json::json!({}),
        serde_json::json!([]),
        serde_json::json!({"directory":42}),
    ] {
        let mut invalid = legacy.clone();
        invalid["info"]["location"] = location;
        cases.push((
            "malformed current location with correct legacy",
            invalid,
            false,
        ));
    }
    cases.push((
        "missing cwd",
        serde_json::json!({"info":{"id":"ses_exact"},"messages":[]}),
        false,
    ));
    let mut invalid = both;
    invalid["info"]["directory"] = serde_json::Value::Null;
    cases.push(("malformed legacy with correct current", invalid, false));
    for messages in [serde_json::Value::Null, serde_json::json!({})] {
        let mut invalid = current.clone();
        invalid["messages"] = messages;
        cases.push(("missing or malformed message list", invalid, false));
    }
    for (case, export, expected) in cases {
        std::fs::write(dir.join("export.json"), export.to_string()).unwrap();
        let result = history::opencode::qualify_conversation(&stores.path, &folder, "ses_exact");
        assert_eq!(result.is_ok(), expected, "{case}: {result:?}");
    }
}

/// Settled asks for everything a folder's harnesses touched since a
/// moment, however many newer conversations there are: the start screen's
/// cap of fifty is applied after the age, never before it.
#[test]
fn since_keeps_everything_touched_after_the_moment_and_nothing_older() {
    let dir = Scratch::new("guild-history-since-");
    let stores = stores(&dir);
    let folder = dir.join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let now = SystemTime::now();
    let mut fresh = Vec::new();
    for i in 0..60 {
        let file = claude_session(
            &stores,
            &folder,
            &format!("00000000-0000-4000-8000-{i:012}"),
            &[user(&folder, &format!("task {i}"))],
        );
        let at = now - Duration::from_secs(60 * (i as u64 + 1));
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(at)
            .unwrap();
        fresh.push(file);
    }
    let old = claude_session(
        &stores,
        &folder,
        "00000000-0000-4000-8000-999999999999",
        &[user(&folder, "old task")],
    );
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(now - Duration::from_secs(5 * 86_400))
        .unwrap();
    let since = now - Duration::from_secs(3 * 86_400);
    let rows = History::default().since(&stores, &folder, since);
    assert_eq!(
        rows.len(),
        60,
        "all sixty within the period, past the cap of fifty"
    );
    assert!(rows.iter().all(|past| past.touched >= since));
    assert!(rows.iter().all(|past| past.title != "old task"));
}

/// A Codex thread begun long ago and written to today is today's for
/// Settled, however many rollouts were begun after it: `since` goes by
/// when a rollout was last written, not by the time in its name.
#[test]
fn codex_since_goes_by_the_last_write_not_the_name() {
    let dir = Scratch::new("history-codex-since-");
    let stores = stores(&dir);
    let folder = dir.join("guild");
    let f = folder.to_string_lossy().into_owned();
    let elsewhere = serde_json::json!({"cwd":"/elsewhere","source":"cli","thread_source":"user"});
    for i in 0..410 {
        let mut meta = elsewhere.clone();
        meta["id"] = serde_json::json!(format!("x{i}"));
        rollout(
            &stores,
            "2026/11/20",
            &format!("2026-11-20T10-{:02}-{:02}", i / 60, i % 60),
            &format!("x{i}"),
            meta,
            Some("elsewhere"),
        );
    }
    rollout(
        &stores,
        "2020/01/01",
        "2020-01-01T10-00-00",
        "old",
        serde_json::json!({"id":"old","cwd":f,"source":"cli","thread_source":"user"}),
        Some("Resumed today"),
    );
    let since = SystemTime::now() - Duration::from_secs(3 * 86_400);
    let rows = History::default().since(&stores, &folder, since);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "old");
    assert!(
        History::default()
            .recent(&stores, &folder, history::LIMIT)
            .is_empty(),
        "the start screen still looks only at the newest names"
    );
}

/// A file named `opencode` that cannot run, in an earlier folder of the
/// `PATH`, does not hide the one that can — and the program runs with the
/// `PATH`'s absolute folders only, so nothing it looks up is read against
/// the project it was started in.
#[test]
fn opencode_is_the_first_that_runs_and_sees_only_absolute_folders() {
    let dir = Scratch::new("history-opencode-path-");
    let mut stores = stores(&dir);
    let folder = dir.join("guild");
    std::fs::create_dir_all(&folder).unwrap();
    let early = dir.join("early");
    std::fs::create_dir_all(&early).unwrap();
    std::fs::write(early.join("opencode"), "not a program").unwrap();
    let answer = serde_json::json!([
        {"id":"ses_a","title":"Found","updated":1790355564437_u64,"created":1,"directory":folder,"projectId":"p"}
    ]);
    std::fs::write(dir.join("answer.json"), answer.to_string()).unwrap();
    opencode(
        &dir,
        &format!(
            "case \":$PATH:\" in *:relative:*|*::*) exit 4;; esac\n/bin/cat '{}'",
            dir.join("answer.json").display()
        ),
    );
    stores.path = std::env::join_paths([
        early.as_path(),
        Path::new("relative"),
        dir.join("bin").as_path(),
    ])
    .unwrap();

    let rows = History::default().recent(&stores, &folder, 50);
    assert_eq!(
        rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["ses_a"]
    );
}
