//! Is there a conversation to come back to?
//!
//! We assign the identifier ourselves with `--session-id`, so there is no
//! need to search for it. What is needed is different: knowing whether a
//! conversation actually happened. A mistake here costs a dead tab —
//! `--resume` on a conversation that does not exist exits immediately.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_session::transcript;

fn temp_dir(name: &str) -> Scratch {
    Scratch::new(&format!("guild-transcript-{name}-"))
}

fn write(projects: &Path, slug: &str, session_id: &str, records: &[&str]) {
    let dir = projects.join(slug);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{session_id}.jsonl")), records.join("\n")).unwrap();
}

/// A real conversation: there are messages.
#[test]
fn a_real_conversation_is_recognised() {
    let dir = temp_dir("real");
    write(
        &dir,
        "any-directory-name",
        "ours",
        &[
            r#"{"type":"ai-title","aiTitle":"something"}"#,
            r#"{"type":"user","message":{"role":"user"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant"}}"#,
        ],
    );

    assert!(transcript::has_conversation(&dir, "ours"));
}

/// **A stub is not a conversation.** `claude` creates the file as soon as
/// a session starts and puts service records into it. Treating that as a
/// conversation would mean offering to "resume" where `--resume` answers
/// `No conversation found` and exits. That is exactly what happened: the
/// tab was restored dead.
///
/// The set of records is taken from a real file of such a session.
#[test]
fn a_stub_transcript_is_not_a_conversation() {
    let dir = temp_dir("stub");
    write(
        &dir,
        "a",
        "empty",
        &[
            r#"{"type":"ai-title","aiTitle":"Rust uninstall workflow"}"#,
            r#"{"type":"agent-name","name":"claude"}"#,
        ],
    );

    assert!(
        !transcript::has_conversation(&dir, "empty"),
        "service records are not a conversation"
    );
}

#[test]
fn a_missing_transcript_is_not_a_conversation() {
    let dir = temp_dir("no-file");
    write(&dir, "a", "someone-elses", &[r#"{"type":"user"}"#]);

    assert!(!transcript::has_conversation(&dir, "ours"));
}

/// A missing directory is no reason to fail: a person may have no sessions
/// at all.
#[test]
fn a_missing_projects_directory_is_not_an_error() {
    let missing = Scratch::absent("guild-transcript-missing-");

    assert!(!transcript::has_conversation(&missing, "ours"));
}

/// A broken line must not hide the real messages.
#[test]
fn a_broken_line_does_not_hide_the_conversation() {
    let dir = temp_dir("broken-line");
    write(
        &dir,
        "a",
        "ours",
        &["{this is not json", r#"{"type":"assistant","message":{}}"#],
    );

    assert!(transcript::has_conversation(&dir, "ours"));
}

/// **The key is `aiTitle`.** Every `ai-title` record on the person's
/// machine carries `aiTitle` — 22 378 of them on 2026-09-28, none with
/// `title` — and the fixtures above were written to the code. The shape
/// below is a real record, trimmed.
#[test]
fn claude_s_own_title_is_the_name() {
    let dir = temp_dir("ai-title");
    write(
        &dir,
        "a",
        "s",
        &[
            r#"{"type":"user","message":{"role":"user","content":"что за проект"}}"#,
            r#"{"type":"ai-title","aiTitle":"Default project screen design","sessionId":"s"}"#,
        ],
    );
    let path = transcript::path(&dir, "s").unwrap();
    assert_eq!(
        transcript::name(&path).as_deref(),
        Some("Default project screen design")
    );
}

/// `claude` rewrites its title as the conversation goes: the last one is
/// what it thinks now.
#[test]
fn the_last_ai_title_wins() {
    let dir = temp_dir("ai-title-last");
    write(
        &dir,
        "a",
        "s",
        &[
            r#"{"type":"ai-title","aiTitle":"First guess"}"#,
            r#"{"type":"user","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"ai-title","aiTitle":"What it became"}"#,
        ],
    );
    let path = transcript::path(&dir, "s").unwrap();
    assert_eq!(transcript::name(&path).as_deref(), Some("What it became"));
}

/// A `/rename` is the person's word and outranks `claude`'s.
#[test]
fn a_rename_outranks_claude_s_title() {
    let dir = temp_dir("custom-title");
    write(
        &dir,
        "a",
        "s",
        &[
            r#"{"type":"ai-title","aiTitle":"Claude's idea"}"#,
            r#"{"type":"custom-title","customTitle":"login bug","sessionId":"s"}"#,
            r#"{"type":"ai-title","aiTitle":"Claude's later idea"}"#,
        ],
    );
    let path = transcript::path(&dir, "s").unwrap();
    assert_eq!(transcript::name(&path).as_deref(), Some("login bug"));
}

/// A check against the real `~/.claude/projects`: the tests above work on
/// made-up directories and check the logic, but not that it fits the
/// actual layout. The layout is the whole point of the code.
#[test]
#[ignore = "needs real claude sessions on the machine"]
fn works_on_the_real_projects_directory() {
    let home = std::env::var("HOME").expect("HOME is set");
    let projects = PathBuf::from(home).join(".claude").join("projects");

    // A large file is certainly a conversation rather than a stub.
    let big = std::fs::read_dir(&projects)
        .expect("the sessions directory is there")
        .flatten()
        .filter_map(|project| std::fs::read_dir(project.path()).ok())
        .flatten()
        .flatten()
        .filter(|file| file.metadata().is_ok_and(|meta| meta.len() > 20_000))
        .find_map(|file| {
            let path = file.path();
            (path.extension()? == "jsonl").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .expect("there is at least one substantial transcript");

    assert!(
        transcript::has_conversation(&projects, &big),
        "conversation {big} was not recognised — the on-disk layout is not what we assume"
    );
}

#[test]
fn configured_claude_history_is_shared_by_read_and_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "transcript::configured_claude_history_fixture",
            "--ignored",
        ])
        .env("CLAUDE_CONFIG_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
#[ignore = "private config environment owned by configured_claude_history_is_shared_by_read_and_recovery"]
fn configured_claude_history_fixture() {
    let configured = std::path::PathBuf::from(std::env::var_os("CLAUDE_CONFIG_DIR").unwrap());
    assert_eq!(
        guild_session::transcript::projects_dir(),
        configured.join("projects")
    );
}
