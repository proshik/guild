//! The status line Guild gives a `claude` it starts, when the person has
//! one of their own — the plan's limits tapped on their way to it, and
//! theirs run with the same input. The script is run for real here, with
//! `sh` and `/usr/bin/curl`, against the window's own receiver.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::scratch::Scratch;
use guild_desktop::hooks::{self, server::Receiver};

/// Short on purpose: a socket's path has to fit in 104 bytes.
fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("gs-{name}-"))
}

/// A payload as Claude Code streams it to a status line, cut short.
const PAYLOAD: &str = r#"{"session_id":"35e368c2","model":{"display_name":"Opus"},"rate_limits":{"five_hour":{"used_percentage":14,"resets_at":1791045000},"seven_day":{"used_percentage":4,"resets_at":1791608400}}}"#;

/// What a run of the status line came to.
struct Ran {
    status: ExitStatus,
    stdout: String,
}

/// Run the installed script as Claude runs a status line: the payload on
/// stdin, what it prints read back.
fn run(script: &Path, socket: Option<&Path>, user: Option<&str>, input: &str) -> Ran {
    let mut command = Command::new(script);
    command
        .env_remove(guild_session::HOOK_SOCKET_VAR)
        .env_remove(guild_session::USER_STATUSLINE_VAR)
        .env(
            guild_session::SESSION_VAR,
            "11111111-2222-3333-4444-555555555555",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(socket) = socket {
        command.env(guild_session::HOOK_SOCKET_VAR, socket);
    }
    if let Some(user) = user {
        command.env(guild_session::USER_STATUSLINE_VAR, user);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Ran {
        status: out.status,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    }
}

/// What the receiver has heard at its usage door within `patience`.
fn heard(receiver: &Receiver, patience: Duration) -> Vec<serde_json::Value> {
    let end = Instant::now() + patience;
    loop {
        let got = receiver.usage();
        if !got.is_empty() || Instant::now() > end {
            return got;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_tap_has_settings_of_its_own_beside_the_plain_ones() {
    let script = Path::new("/Users/a person/it's here/guild-hook.sh");
    let tap = Path::new("/Users/a person/it's here/guild-statusline.sh");
    let settings = hooks::settings_with_statusline(script, tap);

    assert_eq!(settings["statusLine"]["type"], "command");
    assert_eq!(settings["statusLine"]["command"], hooks::shell_quote(tap));
    assert_eq!(
        settings["hooks"],
        hooks::settings(script)["hooks"],
        "the same hooks"
    );
    assert!(hooks::settings(script).get("statusLine").is_none());
}

#[test]
fn installing_writes_the_tap_and_its_settings() {
    let dir = scratch("install");
    let installed = hooks::install(&dir).unwrap();

    assert_eq!(
        std::fs::read_to_string(&installed.statusline).unwrap(),
        hooks::STATUSLINE_SCRIPT
    );
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&installed.statusline_settings).unwrap())
            .unwrap();
    assert_eq!(
        written,
        hooks::settings_with_statusline(&installed.script, &installed.statusline)
    );
}

/// Write `settings.json`-shaped text at `path`, its folders with it.
fn settings_at(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The person's own status line is found where Claude finds it, in
/// Claude's order: the project's local settings, the project's, then the
/// person's — the first with a command wins.
#[test]
fn the_persons_status_line_is_found_in_claudes_order() {
    let root = scratch("order");
    let cwd = root.join("project");
    let config = root.join("config");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    assert_eq!(hooks::user_statusline(&cwd, &config), None, "none at all");

    settings_at(
        &config.join("settings.json"),
        r#"{"statusLine":{"type":"command","command":"~/.claude/statusline.sh"}}"#,
    );
    assert_eq!(
        hooks::user_statusline(&cwd, &config).as_deref(),
        Some("~/.claude/statusline.sh")
    );

    settings_at(
        &cwd.join(".claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"project-line"}}"#,
    );
    assert_eq!(
        hooks::user_statusline(&cwd, &config).as_deref(),
        Some("project-line")
    );

    settings_at(
        &cwd.join(".claude/settings.local.json"),
        r#"{"statusLine":{"type":"command","command":"local-line"}}"#,
    );
    assert_eq!(
        hooks::user_statusline(&cwd, &config).as_deref(),
        Some("local-line")
    );
}

/// A file that names no command — broken, empty, or a status line with
/// nothing to run — is passed over for the next.
#[test]
fn a_settings_file_without_a_command_is_passed_over() {
    let root = scratch("skip");
    let cwd = root.join("project");
    let config = root.join("config");
    settings_at(&cwd.join(".claude/settings.local.json"), "not json");
    settings_at(
        &cwd.join(".claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":""}}"#,
    );
    settings_at(&config.join("settings.json"), r#"{"model":"opus"}"#);
    assert_eq!(hooks::user_statusline(&cwd, &config), None);
}

/// A launch with a status line of the person's to chain to takes the
/// tap's settings; one without keeps the plain ones.
#[test]
fn a_launch_takes_the_taps_settings_only_with_a_status_line_to_chain() {
    let plain = guild_session::Hooks {
        settings: PathBuf::from("/s/claude-settings.json"),
        statusline_settings: PathBuf::from("/s/claude-settings-statusline.json"),
        socket: PathBuf::from("/tmp/guild-hooks-1.sock"),
        user_statusline: None,
        doors: None,
    };
    assert_eq!(plain.args()[1], "/s/claude-settings.json");

    let tapped = guild_session::Hooks {
        user_statusline: Some("~/.claude/statusline.sh".into()),
        ..plain
    };
    assert_eq!(tapped.args()[1], "/s/claude-settings-statusline.json");
}

/// The tap's whole job: the payload reaches the window under `/usage`,
/// and the person's own status line prints what it prints, from the same
/// input.
#[test]
fn the_tap_posts_the_payload_and_runs_the_persons_status_line() {
    let dir = scratch("tap");
    let installed = hooks::install(&dir.join("hooks")).unwrap();
    let socket = dir.join("s.sock");
    let receiver = Receiver::start(&socket).unwrap();
    let seen = dir.join("seen.json");
    let user = format!("cat > '{}'; printf 'mine'", seen.display());

    let ran = run(&installed.statusline, Some(&socket), Some(&user), PAYLOAD);

    assert!(ran.status.success());
    assert_eq!(ran.stdout, "mine", "the person's status line is what shows");
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap().trim_end(),
        PAYLOAD,
        "with the same input"
    );
    let posted = heard(&receiver, Duration::from_secs(3));
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0]["provider"], "claude");
    assert_eq!(
        posted[0]["payload"],
        serde_json::from_str::<serde_json::Value>(PAYLOAD).unwrap()
    );
    assert!(receiver.poll().is_empty(), "not a hook");
}

/// Claude runs a status line several times a second while it streams:
/// one post per session per fifteen seconds, and a tick without limits
/// does not spend the turn.
#[test]
fn the_tap_posts_once_per_fifteen_seconds_and_only_what_has_limits() {
    let dir = scratch("throttle");
    let installed = hooks::install(&dir.join("hooks")).unwrap();
    let socket = dir.join("s.sock");
    let receiver = Receiver::start(&socket).unwrap();

    run(
        &installed.statusline,
        Some(&socket),
        None,
        r#"{"model":{"display_name":"Opus"}}"#,
    );
    assert!(heard(&receiver, Duration::from_millis(500)).is_empty());

    run(&installed.statusline, Some(&socket), None, PAYLOAD);
    assert_eq!(heard(&receiver, Duration::from_secs(3)).len(), 1);
    run(&installed.statusline, Some(&socket), None, PAYLOAD);
    assert!(
        heard(&receiver, Duration::from_millis(500)).is_empty(),
        "the second tick is inside the fifteen seconds"
    );
}

/// The post is in the background: a window that never answers does not
/// hold the person's status line up, and no window at all still shows it.
#[test]
fn the_tap_never_holds_the_status_line_up() {
    let dir = scratch("hold");
    let installed = hooks::install(&dir.join("hooks")).unwrap();
    // A socket that is bound and never accepted from.
    let socket = dir.join("s.sock");
    let _deaf = std::os::unix::net::UnixListener::bind(&socket).unwrap();

    // Replace only the installed HTTP adapter. The production backgrounding,
    // redirects and user-command pipeline are exercised unchanged. File gates
    // prove ordering independently of curl deadlines or machine speed.
    let ready = dir.join("post-ready");
    let release = dir.join("post-release");
    let done = dir.join("post-done");
    let adapter = installed.statusline.with_file_name("guild-usage-post.sh");
    std::fs::write(&adapter, format!(
        "#!/bin/sh\nwhile IFS= read -r line || [ -n \"$line\" ]; do :; done\n: > {}\nwhile [ ! -f {} ]; do /bin/sleep 0.01; done\n: > {}\n",
        hooks::shell_quote(&ready), hooks::shell_quote(&release), hooks::shell_quote(&done))).unwrap();
    let script = installed.statusline.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = send.send(run(&script, Some(&socket), Some("printf mine"), PAYLOAD));
    });
    let wait_for = |path: &Path| {
        let until = Instant::now() + Duration::from_secs(10);
        while !path.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        path.exists()
    };
    let started = wait_for(&ready);
    let result = receive.recv_timeout(Duration::from_secs(10));
    // Release the fixture even on a failed assertion so no process is stranded.
    std::fs::write(&release, "release").unwrap();
    let finished = wait_for(&done);
    worker.join().unwrap();
    assert!(
        started && finished,
        "the controlled post must run to completion"
    );
    let ran = result.expect("user output must finish while the post is still blocked");
    assert!(ran.status.success());
    assert_eq!(ran.stdout, "mine");

    let ran = run(&installed.statusline, None, Some("printf mine"), PAYLOAD);
    assert!(ran.status.success());
    assert_eq!(ran.stdout, "mine");
}
