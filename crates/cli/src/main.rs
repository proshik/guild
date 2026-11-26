//! `guild` — the command line the open window answers. M13.
//!
//! **It does nothing itself.** Every command here is a thing the window
//! already does — makes a worktree, opens a session, says what its agents
//! are doing — and this binary's whole work is to say which one, over a
//! Unix socket, and to print what comes back. Decision 4 of M13's plan:
//! the CLI is a door, not a second way to do anything.
//!
//! It links the session crate and serde and nothing else. Task 0 measured
//! a bare binary at 1.99 ms to start and 2.12 ms to ask a whole question;
//! the desktop's own binary is 132 MB and opens a window first, which is
//! why these are two programs.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;

use guild_session::command::{Answer, Command, Waited};

mod terminal;
mod workflow;

/// Where the window says it is listening.
///
/// Beside `GUILD_HOOK_SOCKET`, which names the same socket for the hook
/// script: two readers with two futures, and a name that says which is
/// which. A shell without it has no window to talk to — that is also the
/// guard against a `guild` in somebody's login shell reaching a window
/// they did not open.
const SOCKET: &str = "GUILD_SOCKET";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A flag with a value takes the word after it, so the value never
    // becomes a word of the command: `worktree create x --from ../repo`
    // is three words and a flag, not four words.
    let mut json = false;
    let mut front = false;
    let mut delete_ignored = false;
    let mut select = false;
    let mut from: Option<&str> = None;
    let mut worktree: Option<&str> = None;
    let mut prompt: Option<&str> = None;
    let mut from_file: Option<&str> = None;
    let mut request_id: Option<&str> = None;
    let mut last: Option<&str> = None;
    let mut timeout: Option<&str> = None;
    let mut workspace: Option<&str> = None;
    let mut named_icon: Option<&str> = None;
    let mut harness: Option<&str> = None;
    let mut base: Option<&str> = None;
    let mut title: Option<&str> = None;
    let mut group: Option<&str> = None;
    let mut words: Vec<&str> = Vec::new();
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        let valued = match arg {
            // Everything after `--` is a word, never a flag: a group can be
            // called `--front`.
            "--" => {
                words.extend(rest.by_ref());
                break;
            }
            "--json" => {
                json = true;
                continue;
            }
            "--front" => {
                front = true;
                continue;
            }
            "--delete-ignored" => {
                delete_ignored = true;
                continue;
            }
            "--select" => {
                select = true;
                continue;
            }
            "--from" => &mut from,
            "--worktree" => &mut worktree,
            "--prompt" => &mut prompt,
            "--harness" => &mut harness,
            "--base" => &mut base,
            "--title" => &mut title,
            "--from-file" => &mut from_file,
            "--request-id" => &mut request_id,
            "--last" => &mut last,
            "--timeout" => &mut timeout,
            "--workspace" => &mut workspace,
            "--named-icon" => &mut named_icon,
            "--group" => &mut group,
            word => {
                words.push(word);
                continue;
            }
        };
        match rest.next() {
            Some(value) => *valued = Some(value),
            None => {
                eprintln!("guild: {arg} needs a value\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    // What an agent reads to use this well — needs no window.
    match words.as_slice() {
        ["instructions"] => {
            print!("{INSTRUCTIONS}");
            return ExitCode::SUCCESS;
        }
        ["instructions", topic] => {
            return match *topic {
                "worktrees" => {
                    print!("{WORKTREES}");
                    ExitCode::SUCCESS
                }
                "parallel" => {
                    print!("{PARALLEL}");
                    ExitCode::SUCCESS
                }
                "rules" => {
                    print!("{RULES}");
                    ExitCode::SUCCESS
                }
                "workspaces" => {
                    print!("{WORKSPACES}");
                    ExitCode::SUCCESS
                }
                other => {
                    eprintln!("guild: no instructions on {other}\n\n{INSTRUCTIONS}");
                    ExitCode::from(2)
                }
            };
        }
        _ => {}
    }

    if words.first() == Some(&"workflow") {
        if front
            || delete_ignored
            || select
            || from.is_some()
            || worktree.is_some()
            || prompt.is_some()
            || request_id.is_some()
            || last.is_some()
            || timeout.is_some()
            || workspace.is_some()
            || named_icon.is_some()
            || harness.is_some()
            || base.is_some()
            || title.is_some()
        {
            return usage("workflow input: unsupported flag; use --from-file for mutation context");
        }
        return workflow::run(&words[1..], from_file, json);
    }

    // `--group` names where `session new` runs — GLD-055. It is that
    // command's alone, and it says the folder, so a worktree beside it is a
    // contradiction refused here, sending nothing.
    if group.is_some() && words.as_slice() != ["session", "new"] {
        return usage("--group is only valid for session new");
    }
    if group.is_some() && worktree.is_some() {
        return usage(
            "give --group or --worktree, not both: a group session runs in the group's folder",
        );
    }

    if request_id.is_some()
        && !matches!(
            words.as_slice(),
            ["session", "resume" | "continuation-status", _]
        )
    {
        return usage("--request-id is only valid for session resume or continuation-status");
    }

    if matches!(
        words.as_slice(),
        ["session", "resume" | "continuation-status", _]
    ) && (from.is_some()
        || worktree.is_some()
        || last.is_some()
        || timeout.is_some()
        || workspace.is_some()
        || named_icon.is_some()
        || harness.is_some()
        || base.is_some()
        || title.is_some()
        || delete_ignored
        || select)
    {
        return usage("session resume/continuation-status received an unsupported flag");
    }

    // Resolve identity through the window, then read storage in this process.
    // Large transcripts never block the window's rendering thread.
    if let ["agent", "read", session] = words.as_slice() {
        let count = match last.map(str::parse::<usize>).unwrap_or(Ok(1)) {
            Ok(count) => count.max(1),
            Err(_) => return refuse_locally("--last needs a number"),
        };
        return read_agent(session, count, json);
    }

    if words.first() == Some(&"terminal") {
        return terminal::run(&words[1..], from_file, timeout, json);
    }

    let command = match words.as_slice() {
        [] | ["help"] => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        ["overlay", "list"] => Command::Overlay {
            operation: "list".into(),
            id: String::new(),
            title: String::new(),
            text: String::new(),
            caller: std::env::var(guild_session::SESSION_VAR).ok(),
        },
        ["overlay", operation, id] if ["open", "show", "hide", "close"].contains(operation) => {
            let content = match text(prompt, from_file) {
                Ok(value) => value.unwrap_or_default(),
                Err(code) => return code,
            };
            Command::Overlay {
                operation: (*operation).into(),
                id: (*id).into(),
                title: title.unwrap_or(id).into(),
                text: content,
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            }
        }
        ["agent", "list"] => Command::AgentList,
        ["agent", "stop", session] => Command::AgentStop {
            session: (*session).to_owned(),
        },
        // Who is asking travels as it does for `session new`: a task may not
        // steer another session.
        ["agent", "send", session] => match text(prompt, from_file) {
            Ok(Some(text)) => Command::AgentSend {
                session: (*session).to_owned(),
                text,
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            },
            Ok(None) => {
                return usage("agent send needs its text: --prompt or --from-file");
            }
            Err(code) => return code,
        },
        // Waiting is many questions, not one held line: a turn runs for
        // minutes and a connection is answered within ten seconds.
        ["agent", "wait", session] => {
            let limit = match timeout.map(str::parse::<u64>).unwrap_or(Ok(600)) {
                Ok(seconds) => std::time::Duration::from_secs(seconds),
                Err(_) => return refuse_locally("--timeout needs a number of seconds"),
            };
            let Some(socket) = std::env::var_os(SOCKET).map(PathBuf::from) else {
                return no_window();
            };
            return wait_agent(&socket, session, limit, json);
        }
        ["status"] => Command::Status,
        ["workspace", "list"] => Command::WorkspaceList,
        ["workspace", "create", name] => Command::WorkspaceCreate {
            name: (*name).to_owned(),
            icon: named_icon.map(str::to_owned),
            select,
        },
        ["workspace", "select", id] => Command::WorkspaceSelect {
            workspace: (*id).to_owned(),
        },
        ["worktree", "list"] => Command::WorktreeList,
        // No path is the caller's own folder — the natural rule for an
        // agent: *open the worktree I am in*.
        ["worktree", "open"] => match here(".") {
            Ok(path) => Command::WorktreeOpen {
                path,
                workspace: workspace.map(str::to_owned),
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            },
            Err(err) => return refuse_locally(&err),
        },
        ["worktree", "open", path] => match here(path) {
            Ok(path) => Command::WorktreeOpen {
                path,
                workspace: workspace.map(str::to_owned),
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            },
            Err(err) => return refuse_locally(&err),
        },
        // Who is asking travels as it does for `session new`: bringing a
        // task back is starting one, and a task may not start a task.
        ["session", "resume", session] => {
            if from_file.is_some() || request_id.is_some() || prompt.is_some() {
                if prompt.is_some() || from_file.is_none() || request_id.is_none() {
                    return usage(
                        "continuation resume requires --from-file PATH|- and --request-id ID; --prompt is not supported",
                    );
                }
                let value = match text(None, from_file) {
                    Ok(Some(value)) => value,
                    Ok(None) => return usage("continuation resume needs a task file"),
                    Err(code) => return code,
                };
                let request_id = request_id.unwrap();
                if let Err(error) = guild_session::resume_handoff::validate(request_id, &value) {
                    return usage(&error);
                }
                Command::SessionResumeTask {
                    session: (*session).into(),
                    request_id: request_id.into(),
                    prompt: value,
                    caller: std::env::var(guild_session::SESSION_VAR).ok(),
                    front,
                }
            } else {
                Command::SessionResume {
                    session: (*session).into(),
                    caller: std::env::var(guild_session::SESSION_VAR).ok(),
                    front,
                }
            }
        }
        ["session", "continuation-status", session] => {
            if from_file.is_some() || prompt.is_some() || front {
                return usage(
                    "continuation-status accepts only SESSION, --request-id ID and --json",
                );
            }
            if let Some(key) = request_id
                && let Err(error) = guild_session::resume_handoff::validate(key, "status")
            {
                return usage(&error);
            }
            Command::SessionContinuationStatus {
                session: (*session).into(),
                request_id: request_id.map(str::to_owned),
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            }
        }
        ["session", "resume"] => {
            return usage("session resume needs the session: `guild agent list` prints it");
        }
        // In the worktree the caller is standing in unless told otherwise.
        // Who is asking comes from the environment, not from a flag: a
        // session does not have to say who it is, and the window checks it
        // against what it started — decision 5 of M13's plan.
        ["session", "new"] if group.is_some() => {
            let prompt = match text(prompt, from_file) {
                Ok(prompt) => prompt,
                Err(code) => return code,
            };
            let harness = match harness_of(harness) {
                Ok(harness) => harness,
                Err(code) => return code,
            };
            Command::SessionNewInGroup {
                group: group.unwrap_or_default().to_owned(),
                prompt,
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
                front,
                workspace: workspace.map(str::to_owned),
                harness: Some(harness),
            }
        }
        // How the workspace's project groups stand against their folders —
        // the read the column's match line does — GLD-055.
        ["group", "check"] => Command::GroupCheck {
            group: None,
            workspace: workspace.map(str::to_owned),
            caller: std::env::var(guild_session::SESSION_VAR).ok(),
        },
        ["group", "check", name] => Command::GroupCheck {
            group: Some((*name).to_owned()),
            workspace: workspace.map(str::to_owned),
            caller: std::env::var(guild_session::SESSION_VAR).ok(),
        },
        ["session", "new"] => {
            let prompt = match text(prompt, from_file) {
                Ok(prompt) => prompt,
                Err(code) => return code,
            };
            match here(worktree.unwrap_or(".")) {
                Ok(worktree) => {
                    let harness = match harness_of(harness) {
                        Ok(harness) => harness,
                        Err(code) => return code,
                    };
                    Command::SessionNew {
                        worktree,
                        prompt,
                        caller: std::env::var(guild_session::SESSION_VAR).ok(),
                        front,
                        workspace: workspace.map(str::to_owned),
                        harness: Some(harness),
                    }
                }
                Err(err) => return refuse_locally(&err),
            }
        }
        // Made from the root the caller is standing in unless told
        // otherwise; the new branch starts at `--base`, or at the
        // repository's main branch, which the window finds — M13.1.
        //
        // **With a task it is a worktree and the session doing it**, one
        // command — M13.1: the flags that shape the session mean what they
        // mean on `session new`, and without a task there is no session
        // for them to shape.
        ["worktree", "create", name] => {
            let prompt = match text(prompt, from_file) {
                Ok(prompt) => prompt,
                Err(code) => return code,
            };
            if prompt.is_none() && (front || workspace.is_some() || harness.is_some()) {
                return usage(
                    "--front, --workspace and --harness shape the session a task starts — \
                     give the task with --prompt or --from-file",
                );
            }
            let harness = match harness_of(harness) {
                Ok(harness) => prompt.is_some().then_some(harness),
                Err(code) => return code,
            };
            match here(from.unwrap_or(".")) {
                Ok(from) => Command::WorktreeCreate {
                    from,
                    name: (*name).to_owned(),
                    base: base.map(str::to_owned),
                    prompt,
                    caller: std::env::var(guild_session::SESSION_VAR).ok(),
                    front,
                    workspace: workspace.map(str::to_owned),
                    harness,
                },
                Err(err) => return refuse_locally(&err),
            }
        }
        // **A removal names its folder**: `open`'s default is the folder the
        // caller stands in, and a removal with that default deletes the
        // ground the asking session stands on.
        ["worktree", "remove"] => {
            eprintln!("guild: worktree remove needs the worktree's folder\n\n{USAGE}");
            return ExitCode::from(2);
        }
        // Who is asking travels as it does for `session new`: a session
        // started with a task may not remove a worktree.
        ["worktree", "remove", path] => match here(path) {
            Ok(path) => Command::WorktreeRemove {
                path,
                delete_ignored,
                caller: std::env::var(guild_session::SESSION_VAR).ok(),
            },
            Err(err) => return refuse_locally(&err),
        },
        other => {
            eprintln!("guild: no such command: {}\n\n{USAGE}", other.join(" "));
            return ExitCode::from(2);
        }
    };

    let Some(socket) = std::env::var_os(SOCKET).map(PathBuf::from) else {
        return no_window();
    };

    match ask(&socket, &command) {
        Ok(answer) => say(&answer, json),
        Err(err) => {
            eprintln!("guild: {err}");
            ExitCode::from(4)
        }
    }
}

/// A path as the window must hear it: absolute, and with every link
/// resolved.
///
/// **Resolved here, not in the window**, because a relative path means
/// something only in the shell that typed it — the window's own working
/// directory is wherever it was launched from. And canonical, because the
/// window's watcher is fed real paths and git prints them: `/tmp` is
/// `/private/tmp` to both, and a root given the link matches nothing.
fn here(path: &str) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|err| format!("there is no folder at {path}: {err}"))
}

/// The text of a prompt or a message: `--prompt`, or `--from-file`, where
/// `-` is standard input — M13 task 9.
///
/// **A file, because a shell is a bad place to keep a long task.** Quotes, a
/// `$`, backticks and a dozen lines all mean something to the shell that
/// typed `--prompt`; read from a file — or from a heredoc on standard input
/// — the text arrives as it was written. Both at once, or an empty text, is
/// a mistake in the command.
fn text(prompt: Option<&str>, from_file: Option<&str>) -> Result<Option<String>, ExitCode> {
    let text = match (prompt, from_file) {
        (Some(_), Some(_)) => {
            return Err(usage(
                "give the text with --prompt or --from-file, not both",
            ));
        }
        (None, None) => return Ok(None),
        (Some(prompt), None) => prompt.to_owned(),
        (None, Some("-")) => {
            let mut read = String::new();
            if let Err(err) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut read) {
                return Err(usage(&format!("could not read standard input: {err}")));
            }
            read
        }
        (None, Some(path)) => match std::fs::read_to_string(path) {
            Ok(read) => read,
            Err(err) => return Err(usage(&format!("could not read {path}: {err}"))),
        },
    };
    if text.trim().is_empty() {
        return Err(usage("the text is empty"));
    }
    Ok(Some(text))
}

/// The harness a session runs, by `--harness`: Claude Code when none is
/// named.
fn harness_of(named: Option<&str>) -> Result<guild_session::HarnessKind, ExitCode> {
    match named {
        None | Some("claude") => Ok(guild_session::HarnessKind::Claude),
        Some("opencode") => Ok(guild_session::HarnessKind::OpenCode),
        Some("codex") => Ok(guild_session::HarnessKind::Codex),
        Some(other) => {
            eprintln!("guild: unknown harness `{other}`\n\n{USAGE}");
            Err(ExitCode::from(2))
        }
    }
}

/// A mistake in the command itself.
fn usage(said: &str) -> ExitCode {
    eprintln!("guild: {said}\n\n{USAGE}");
    ExitCode::from(2)
}

/// There is no window to ask.
fn no_window() -> ExitCode {
    eprintln!(
        "guild: no window to talk to — {SOCKET} is not set.\n\
         It is set for every session Guild starts; run this from one."
    );
    ExitCode::from(3)
}

/// Print what an agent last said, from its own transcript.
fn read_agent(session: &str, count: usize, json: bool) -> ExitCode {
    let Some(socket) = std::env::var_os(SOCKET) else {
        return no_window();
    };
    let agents = match ask(std::path::Path::new(&socket), &Command::AgentList) {
        Ok(Answer::Agents { agents }) => agents,
        Ok(Answer::Refused { because }) => return refuse_locally(&because),
        Ok(_) => return refuse_locally("the window answered something other than its agents"),
        Err(err) => return refuse_locally(&err),
    };
    let Some(agent) = agents.into_iter().find(|agent| agent.session == session) else {
        return refuse_locally(&format!("this window has no session {session}"));
    };
    let Some(harness) = agent.harness else {
        return refuse_locally(
            "the window has no harness identity for this session — update Guild if it is an older window",
        );
    };
    let Some(conversation) = agent.conversation.as_deref().filter(|id| !id.is_empty()) else {
        return refuse_locally(
            "the window has not observed this harness's conversation identity yet — inspect its tab; no conversation will be guessed",
        );
    };
    let replies = match harness {
        guild_session::HarnessKind::Claude => {
            let Some(path) = guild_session::transcript::path(
                &guild_session::transcript::projects_dir(),
                conversation,
            ) else {
                return refuse_locally(&format!(
                    "no transcript for conversation {conversation} yet"
                ));
            };
            guild_session::transcript::replies(&path, count)
        }
        guild_session::HarnessKind::Codex => {
            let Some(path) = guild_session::codex_transcript::path(
                &guild_session::harness::CodexHarness::home(),
                conversation,
            ) else {
                return refuse_locally(&format!(
                    "no unique Codex rollout for conversation {conversation}"
                ));
            };
            guild_session::codex_transcript::replies(&path, count)
        }
        guild_session::HarnessKind::OpenCode => {
            let path = guild_session::login_path::path_or_inherited();
            match guild_session::history::opencode::replies(
                std::ffi::OsStr::new(&path),
                &agent.worktree,
                conversation,
                count,
            ) {
                Ok(replies) => replies,
                Err(err) => return refuse_locally(&err),
            }
        }
    };
    if json {
        println!(
            "{}",
            serde_json::json!({ "session": session, "replies": replies })
        );
        return ExitCode::SUCCESS;
    }
    if replies.is_empty() {
        println!("(nothing said yet)");
    }
    for (at, reply) in replies.iter().enumerate() {
        if at > 0 {
            println!("\n---\n");
        }
        println!("{reply}");
    }
    ExitCode::SUCCESS
}

/// Ask the window about one session until it has finished, or until the
/// caller's patience runs out.
///
/// **Finished is `command::settled`**, not *not working*: a session just
/// started reads `idle` before its first turn's hook arrives, and a wait
/// that returned then would return before the agent had done anything.
fn wait_agent(
    socket: &std::path::Path,
    session: &str,
    limit: std::time::Duration,
    json: bool,
) -> ExitCode {
    let started = std::time::Instant::now();
    loop {
        let agents = match ask(socket, &Command::AgentList) {
            Ok(Answer::Agents { agents }) => agents,
            Ok(Answer::Refused { because }) => return refuse_locally(&because),
            Ok(_) => return refuse_locally("the window answered something other than its agents"),
            Err(err) => return refuse_locally(&err),
        };
        let Some(agent) = agents.into_iter().find(|agent| agent.session == session) else {
            return refuse_locally(&format!("this window has no session {session}"));
        };
        // **Gone is not finished** — M5.12, decision 16. A closed tab or an
        // ended process has its own word and its own code: 5 is *gave up*,
        // and this is *the thing you waited for is not there any more*.
        let gone = match guild_session::command::waited(&agent) {
            Waited::Going => None,
            Waited::Finished => {
                if json {
                    print_agent(&agent);
                } else {
                    println!("{} {}", agent.session, agent.status);
                }
                return ExitCode::SUCCESS;
            }
            Waited::Closed => Some(("closed", "its tab was closed")),
            Waited::Exited => Some(("exited", "its process ended")),
        };
        if let Some((word, why)) = gone {
            if json {
                print_agent(&agent);
            } else {
                println!("{} {word}", agent.session);
            }
            eprintln!(
                "guild: session {session} is gone — {why}; `guild agent read {session}` shows what it said"
            );
            return ExitCode::from(6);
        }
        if started.elapsed() >= limit {
            // A program that never reported is not one still doing
            // something: say which it is.
            if agent.status == "unknown" {
                eprintln!(
                    "guild: gave up after {}s — no status report received (OSC 7501) from session {session}",
                    limit.as_secs()
                );
            } else {
                eprintln!(
                    "guild: gave up after {}s — session {session} is still {}",
                    limit.as_secs(),
                    agent.status
                );
            }
            return ExitCode::from(5);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// An agent as `--json` prints it.
fn print_agent(agent: &guild_session::command::Agent) {
    println!(
        "{}",
        serde_json::to_string_pretty(agent).unwrap_or_else(|_| "{}".to_owned())
    );
}

/// Say why a command was not sent, and fail.
fn refuse_locally(because: &str) -> ExitCode {
    eprintln!("guild: {because}");
    ExitCode::FAILURE
}

/// Ask the window, and read what it says.
fn ask(socket: &std::path::Path, command: &Command) -> Result<Answer, String> {
    ask_with_timeout(socket, command, None)
}

fn ask_with_timeout(
    socket: &std::path::Path,
    command: &Command,
    timeout: Option<std::time::Duration>,
) -> Result<Answer, String> {
    let body = serde_json::to_vec(command).map_err(|err| err.to_string())?;
    let mut stream = UnixStream::connect(socket)
        .map_err(|err| format!("could not reach the window at {}: {err}", socket.display()))?;
    stream
        .set_read_timeout(timeout)
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(timeout)
        .map_err(|e| e.to_string())?;
    let head = format!(
        "POST /command HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&body))
        .map_err(|err| format!("the window closed the line: {err}"))?;

    let mut said = Vec::new();
    stream
        .read_to_end(&mut said)
        .map_err(|err| format!("the window said nothing: {err}"))?;
    let said = String::from_utf8_lossy(&said);
    let Some((_, body)) = said.split_once("\r\n\r\n") else {
        return Err(format!("the window answered something else: {said}"));
    };
    serde_json::from_str(body).map_err(|err| format!("the answer did not parse: {err}: {body}"))
}

/// Print the answer the way the caller asked for it.
///
/// **Text for a person, JSON for whoever parses**, and the same answer
/// behind both — decision 6 of M13's plan. A refusal is words rather than
/// a number, because every refusal in this window already has a sentence
/// and turning it into an exit code throws away the only part an agent
/// can act on. The code says *that* it failed; the sentence says why.
fn say(answer: &Answer, json: bool) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(answer).unwrap_or_else(|_| "{}".to_owned())
        );
        return match answer {
            Answer::Refused { .. }
            | Answer::Workflow {
                reply: guild_session::workflow::WorkflowReply::Gap { .. },
            } => ExitCode::FAILURE,
            _ => ExitCode::SUCCESS,
        };
    }

    match answer {
        Answer::Workflow { reply } => {
            println!(
                "{}",
                serde_json::to_string_pretty(reply).unwrap_or_default()
            );
            if matches!(reply, guild_session::workflow::WorkflowReply::Gap { .. }) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Answer::SessionContinuation { receipt } => {
            println!(
                "{}",
                serde_json::to_string_pretty(receipt).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Answer::SessionContinuationStatus { observation } => {
            println!(
                "{}",
                serde_json::to_string_pretty(observation).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Answer::Terminal { reply } => {
            println!(
                "{}",
                serde_json::to_string_pretty(reply).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Answer::Overlays { overlays } => {
            for overlay in overlays {
                println!(
                    "{}\t{}\t{}",
                    overlay.id,
                    if overlay.visible { "shown" } else { "hidden" },
                    overlay.title
                );
            }
            ExitCode::SUCCESS
        }
        Answer::Done => ExitCode::SUCCESS,
        Answer::Refused { because } => {
            eprintln!("guild: {because}");
            ExitCode::FAILURE
        }
        Answer::Status { projects } => {
            if projects.is_empty() {
                println!("no projects open");
            }
            for project in projects {
                let mark = if project.active { "*" } else { " " };
                let sessions = match project.sessions {
                    0 => String::new(),
                    1 => "  1 session".to_owned(),
                    many => format!("  {many} sessions"),
                };
                println!(
                    "{mark} {}  {}{sessions}",
                    project.name,
                    project.path.display()
                );
            }
            ExitCode::SUCCESS
        }
        Answer::Worktrees { worktrees } => {
            if worktrees.is_empty() {
                println!("no worktrees: git has listed none for the open roots yet");
            }
            let wide = worktrees
                .iter()
                .map(|tree| tree.path.display().to_string().chars().count())
                .max()
                .unwrap_or(0);
            let branch_wide = worktrees
                .iter()
                .map(|tree| {
                    tree.branch
                        .as_deref()
                        .unwrap_or("(detached)")
                        .chars()
                        .count()
                })
                .max()
                .unwrap_or(0);
            for tree in worktrees {
                let mut marks = Vec::new();
                if tree.primary {
                    marks.push("primary".to_owned());
                }
                if tree.opened {
                    marks.push("open".to_owned());
                }
                if let Some(reason) = &tree.locked {
                    marks.push(if reason.trim().is_empty() {
                        "locked".to_owned()
                    } else {
                        format!("locked: {}", reason.trim())
                    });
                }
                println!(
                    "{:wide$}  {:branch_wide$}  {}",
                    tree.path.display().to_string(),
                    tree.branch.as_deref().unwrap_or("(detached)"),
                    marks.join(", ")
                );
            }
            ExitCode::SUCCESS
        }
        Answer::Opened { path } => {
            println!("opened {}", path.display());
            ExitCode::SUCCESS
        }
        Answer::Created {
            path,
            branch,
            base,
            session,
        } => {
            match base {
                Some(base) => println!("created {} on branch {branch} from {base}", path.display()),
                None => println!("created {} on branch {branch}", path.display()),
            }
            if let Some(session) = session {
                println!("started {session} in {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Answer::Sent { session } => {
            println!("sent to {session}; its turn has begun");
            ExitCode::SUCCESS
        }
        Answer::Removed { path } => {
            println!("removed {}", path.display());
            ExitCode::SUCCESS
        }
        Answer::SessionStarted { session, worktree } => {
            println!("started {session} in {}", worktree.display());
            ExitCode::SUCCESS
        }
        Answer::Groups { groups } => {
            if groups.is_empty() {
                println!("no groups");
            }
            for group in groups {
                match (&group.folder, group.matches) {
                    (Some(folder), Some(true)) => {
                        println!("{}\t{}\tmatches", group.name, folder.display())
                    }
                    (Some(folder), _) => println!(
                        "{}\t{}\t{} of {} inside",
                        group.name,
                        folder.display(),
                        group
                            .members
                            .iter()
                            .filter(|m| m.inside == Some(true))
                            .count(),
                        group.members.len()
                    ),
                    (None, _) => println!("{}\tno folder", group.name),
                }
                for member in &group.members {
                    let place = match member.inside {
                        Some(true) => "inside",
                        Some(false) => "outside",
                        None => "-",
                    };
                    println!("  {}\t{place}", member.checkout.display());
                }
            }
            ExitCode::SUCCESS
        }
        Answer::Resumed { session, worktree } => {
            println!("resumed {session} in {}", worktree.display());
            ExitCode::SUCCESS
        }
        Answer::Workspaces { workspaces } => {
            for workspace in workspaces {
                println!(
                    "{}\t{}\tselected={}",
                    workspace.id, workspace.name, workspace.selected
                );
            }
            ExitCode::SUCCESS
        }
        Answer::WorkspaceCreated { id, .. } => {
            println!("{id}");
            ExitCode::SUCCESS
        }
        Answer::WorkspaceSelected { id } => {
            println!("selected {id}");
            ExitCode::SUCCESS
        }
        Answer::Agents { agents } => {
            if agents.is_empty() {
                println!("no sessions");
            }
            for agent in agents {
                let mut marks = Vec::new();
                if agent.task {
                    marks.push("task");
                }
                if !agent.open {
                    marks.push("closed");
                } else if !agent.tab {
                    // Open, but in another window or a workspace this one
                    // does not have: not running here.
                    marks.push("no tab in this window");
                }
                println!(
                    "{}  {:8}  {}  {}",
                    agent.session,
                    agent.status,
                    agent.worktree.display(),
                    marks.join(", ")
                );
            }
            ExitCode::SUCCESS
        }
    }
}

/// What `guild instructions` prints — M13 task 6.
///
/// **Written from what the commands do, not from what the plan hoped.** An
/// agent plans around what it is told it has — the lesson `ide::tools()`
/// already carries — so a command named here that does not exist is an
/// attempt spent on nothing. The test beside the binary runs every command
/// these texts name.
const INSTRUCTIONS: &str = "\
guild — the command line of the Guild window you are running in

What you make through `guild`, the window makes and shows: a worktree appears
in its column, a session opens in a tab the person can see. The socket identifies
the owning window; caller restrictions are enforced by that application.

Topics:
  guild instructions worktrees   make, open and list worktrees
  guild instructions parallel    start a task in another worktree and collect it
  guild instructions rules       caller restrictions enforced by the window
  guild instructions workspaces  list, create and select named workspaces

Terminal programs: probe `guild terminal capabilities --json`, then pass a literal
version-1 JSON descriptor to `guild terminal start --from-file PATH|- --json`.
When operations includes archive_viewer, optional archive_viewer metadata contains
absolute, NUL-free directory and program paths. Open run invokes the registered
program with literal view --archive DIRECTORY arguments after completion or
interruption, including after restart, without original review environment overrides.
Emit only archive_viewer; never send both archive metadata spellings. Retry a lost
start response with the same caller/request_id and unchanged descriptor. After a
Guild restart, inspect the existing job by ID; do not start a replacement review.
";

const WORKSPACES: &str = "\
Workspaces are named sets of roots, tabs and sessions in one Guild window.

  guild workspace list                         list them and show which is selected
  guild workspace create NAME                  add one without selecting it
  guild workspace create NAME --named-icon ICON --select
                                                add one with an icon and select it
  guild workspace select WORKSPACE_ID           bring one to the front

`worktree open`, `session new` and `worktree create` with a task put the checkout
where its repository is: in the one workspace that holds the repository, whoever
asks; among several, in yours if it is one of them — otherwise pass
--workspace WORKSPACE_ID; for a repository no workspace holds, in yours, or in
the selected one.

Create does not select a workspace unless --select is supplied.
";

const WORKTREES: &str = "\
Guild's worktree API registers checkouts and prompted sessions in the window.

  guild worktree list            every worktree git lists, which are open, which are locked
  guild worktree create NAME     a new worktree on a new branch NAME, beside the repository
                                 you are in (--from PATH for another), started from the
                                 repository's main branch (--base REF for another)
  guild worktree create NAME --from-file PATH
                                 the same, and a session in it doing the task in PATH
                                 (--prompt TEXT for a short one; --from-file - reads stdin)
  guild worktree open [PATH]     make a checkout a root; no PATH is the one you are in
  guild worktree remove PATH     remove a worktree; its branch stays

`create` answers once the worktree exists and is listed, and prints its folder
and what it started from. With a task it also prints the session identity — see
`guild instructions parallel`. Git failures are returned to the caller.

`remove` refuses a task caller, active agents, open user sessions or unsaved
files, locks and tracked changes. Ignored files are listed and require the
explicit --delete-ignored argument. The branch is retained. Raw Git operations
are not a substitute for registering a checkout/session with Guild's API.

";

const PARALLEL: &str = "\
Starting a task in another worktree, and getting the result back:

  guild worktree create fix-login --from-file PATH
      created /…/guild-fix-login on branch fix-login from main
      started <SESSION> in /…/guild-fix-login
  guild agent wait <SESSION>              until its turn ends, or it stops to ask
  guild agent read <SESSION> [--last N]   what its exact harness conversation said
  guild agent send <SESSION> --prompt \"<the answer or the next step>\"
                                          continue it; then wait and read again
  guild agent stop <SESSION>              end it, once its work has been taken
  guild agent list                        every session, and what each is doing
  guild session resume <SESSION>          bring back a task whose tab was closed

`worktree create` with a task creates both the checkout and a managed session.
For an existing checkout, `guild session new --worktree PATH --from-file PATH`
starts a managed session there. The returned session identity names the worker
for subsequent read/wait operations.

The session opens in a tab behind the person's work; --front brings it forward.
It does not see this conversation: write the prompt as a complete task, with
what to do, where, and what to report back. For a long task write it to a file
and pass --from-file PATH, or --from-file - with a heredoc: the shell's quoting
does not reach it.

`agent send` types into a session's prompt, so it is refused while the session
works (wait first), while it waits on a permission or a question (the person
answers those in its tab), and for a session a person opened.

`agent wait` printing `waiting` means the session stopped to ask the person.
That is not a result — tell the person which session is waiting.

`agent wait` printing `closed` means its tab was closed, and `exited` that its
process ended — both exit with code 6, and neither says the work was done:
`guild agent read <SESSION>` shows how far it got; tell the person.
`unknown` means telemetry has not observed this process; it does not mean idle
or stalled. Do not stop or restart a session merely because telemetry is missing.
Reading requires this window's exact harness conversation mapping.

Managed task continuation for Claude, Codex and OpenCode:
  guild session continuation-status SESSION --json
  guild session resume SESSION --from-file TASK|- --request-id STABLE_ID --json
  guild session continuation-status SESSION --request-id STABLE_ID --json
Without a request ID, status reconciles runtime/readiness without restoring a tab
or sending input. With an ID, it reads the original durable delivery receipt.
A proven stopped task resumes its exact saved conversation with literal startup
text. An exited retained body needs positive owned-session quiescence; Guild
never stops live or unknown work to obtain readiness. Codex live continuation
requires fresh true rollout completion and unchanged input. Claude Stop hooks
and OpenCode idle do not prove safe live input: those adapters return a precise
readiness limitation. OpenCode startup refuses unknown external holders, even
when they appear unrelated; missing saved history never intentionally creates one.
File/stdin text must be nonempty UTF-8 without NUL, at most 32 KiB. A supplied
caller must resolve to an open non-task record; an absent caller retains the
external-operator namespace. Never invent, remove or substitute caller identity.
Reuse the same request ID and exact text/front/context after losing a response.
Receipts distinguish reserved/launched/submitted/failed_to_start/delivery_unknown;
launched/submitted is not model delivery. turn_started is a separate exact-runtime
observation. Uncertain outcomes survive restart and block new keys/controllers;
inspect the existing receipt instead of replaying. Registered workflow tasks
retain their workflow delivery authority. Trust, permissions and genuine questions
remain the person's to answer. Older hosts support only their original subset or
refuse; update CLI and app together only under the applicable authorization.

A task whose tab was closed comes back with `guild session resume <SESSION>`
(behind the person's work; --front brings it forward). Reopened means a tab,
not prompt readiness. Use the checked startup continuation above for a stopped task;
ordinary send still requires observed readiness. `agent list` shows it as `task, closed`. It is refused while its
conversation is open somewhere else, and the refusal says where.
";

const RULES: &str = "\
The owning window verifies GUILD_SESSION against its session records and resume
bindings. A supplied environment identity alone does not prove ownership.

A session started with a task may not start another prompted task, steer another
session, resume a closed task or remove a worktree — the window refuses it.

`guild agent stop` and `guild agent send` reach only a session the command line
started. A task resumed in another tab remains a task. Send is refused while a
session works or waits for a permission/question, and for user-created sessions.

Worktree removal refuses live agents, open user sessions, unsaved files, locks
and tracked changes. Ignored files require the explicit --delete-ignored option.
These restrictions are enforced by the owning window.
";

const USAGE: &str = "\
guild — the window's own command line

Usage:
  guild overlay open ID --title TITLE --from-file PATH [--json]
                                      show literal text over this session; update by ID
  guild overlay show|hide|close ID [--json]
                                      control presentation only, never cancel work
  guild overlay list [--json]          this session's open and minimized surfaces
  guild terminal capabilities [--json]
  guild workflow instructions
  guild workflow capabilities [--json]
  guild workflow start --from-file PATH|- [--json]
  guild workflow status|show RUN [--json]
  guild workflow checkpoint|recover|reconcile|cancel RUN --from-file PATH|- [--json]
    Mutations require version:1, request_id and expected_revision in JSON.
    Start acceptance is distinct from effect launch/delivery; inspect receipts.

  guild terminal start --from-file PATH|- [--json]
  guild terminal status|show|hide|close|cancel JOB [--json]
  guild terminal wait JOB [--timeout 1..60] [--json]
                                     timeout returns pending status; never cancels
  guild status [--json]               what the window is holding
  guild workspace list [--json]       named workspaces and their projects
  guild workspace create NAME [--named-icon NAME] [--select] [--json]
                                      add a workspace; do not select by default
  guild workspace select WORKSPACE_ID [--json]
                                      bring a workspace to the front
  guild worktree list [--json]        every worktree git lists for the open roots
  guild worktree open [PATH] [--workspace WORKSPACE_ID] [--json]
                                      make a checkout a root and bring it to the
                                      front; no PATH is the folder you are in
  guild worktree create NAME [--from PATH] [--base REF] [--prompt TEXT | --from-file PATH] [--front] [--workspace WORKSPACE_ID] [--harness claude|opencode|codex] [--json]
                                      a new worktree on a new branch NAME, beside
                                      the repository, from its main branch or REF;
                                      answered once git has made it, or refused
                                      in git's words; with a task, a session in
                                      it asked TEXT first, as `session new` starts
  guild worktree remove PATH [--delete-ignored] [--json]
                                      remove a worktree and keep its branch; what
                                      git ignores in it is named, and deleted only
                                      with --delete-ignored
  guild session new [--worktree PATH] [--workspace WORKSPACE_ID] [--prompt TEXT | --from-file PATH] [--front] [--harness claude|opencode|codex] [--json]
                                      an agent session in a tab, in that worktree
                                      (the one you are in by default), asked TEXT
                                      first; behind your work unless --front;
                                      harness is claude by default
  guild session new --group NAME [--workspace WORKSPACE_ID] [--prompt TEXT | --from-file PATH] [--front] [--harness claude|opencode|codex] [--json]
                                      the same, in the folder a project group is
                                      bound to; refused when it has none
  guild group check [--workspace WORKSPACE_ID] [--json] [--] [NAME]
                                      whether each group's projects sit directly
                                      in its folder, and which do not
  guild session resume SESSION [--front] [--json]
  guild session resume SESSION --from-file PATH|- --request-id ID [--front] [--json]
                                      checked startup for stopped tasks; qualified Codex live input
  guild session continuation-status SESSION [--request-id ID] [--json]
                                      read-only readiness, or original request delivery receipt
                                      bring a closed session back into its tab,
                                      behind your work unless --front; a task
                                      comes back a task
  guild agent list [--json]           every session, and what its agent is doing
  guild agent read SESSION [--last N] [--json]
                                      what the agent last said, from its transcript
  guild agent wait SESSION [--timeout SECONDS] [--json]
                                      until its turn has ended or it asks you;
                                      `closed` or `exited` with exit code 6 when
                                      its tab was closed or its process ended
  guild agent send SESSION (--prompt TEXT | --from-file PATH) [--json]
                                      type into a session the command line started
                                      and send it; answered once its turn begins
  guild agent stop SESSION [--json]   end a session the command line started

  guild instructions [worktrees|parallel|rules|workspaces]
                                      how to use this, for an agent

--from-file - reads the text from standard input.

Every command speaks to the window named by GUILD_SOCKET, which is set
for every session Guild starts.
";
