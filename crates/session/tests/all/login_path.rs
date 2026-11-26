//! The login shell's `PATH` — decision 22 of M5.11. The shell is stood in
//! for by a script in a temporary directory, so what is checked is how
//! Guild reads a shell's answer, gives up on one that gives none, and ends
//! what it started — not what this machine's `~/.zshrc` happens to say.

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::scratch::Scratch;
use guild_session::login_path::{MARK, merge, parse, resolve};

fn temp_dir(name: &str) -> Scratch {
    Scratch::new(&format!("guild-login-path-{name}-"))
}

/// A script standing in for `$SHELL`: it ignores `-l -i -c …` and says
/// what the test wrote in it.
fn shell(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("shell");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// An interactive shell prints what its rc files print: a greeting, a
/// title escape, a prompt's leftovers. The `PATH` is what lies between the
/// two marks and nothing else.
#[test]
fn the_path_is_what_lies_between_the_marks() {
    let output = format!(
        "Last login: Mon Sep 15 on ttys001\n\x1b]0;~\x07\x1b[?2004h\n{MARK}/a/bin:/b/bin{MARK}\nbye\n"
    );
    assert_eq!(
        parse(output.as_bytes()),
        Some(OsString::from("/a/bin:/b/bin"))
    );
    assert_eq!(
        parse(format!("{MARK}/only/one").as_bytes()),
        None,
        "one mark is no answer"
    );
    assert_eq!(parse(b"nothing at all"), None);
    assert_eq!(
        parse(format!("{MARK}{MARK}").as_bytes()),
        None,
        "an empty PATH is no answer"
    );
}

/// The login shell's entries first, so a tab, the Output and an agent mean
/// the same `git`; whatever the window inherited that they lack after, so
/// nothing the window could run before is lost; none twice.
#[test]
fn the_login_shells_entries_come_first_and_none_twice() {
    let merged = merge(
        OsStr::new("/home/me/.local/bin:/usr/bin:/opt/homebrew/bin:/usr/bin"),
        Some(OsStr::new("/usr/bin:/bin:/custom/bin:/home/me/.local/bin")),
    );
    assert_eq!(
        merged,
        OsString::from("/home/me/.local/bin:/usr/bin:/opt/homebrew/bin:/bin:/custom/bin")
    );
    assert_eq!(
        merge(OsStr::new("/a:/b"), None),
        OsString::from("/a:/b"),
        "nothing inherited"
    );
}

/// A shell that answers is read, through the noise before its answer.
#[test]
fn a_shell_that_answers_is_read() {
    let dir = temp_dir("answers");
    let shell = shell(
        &dir,
        &format!(
            "echo 'Welcome back'\nprintf '\\033]0;title\\007'\nprintf '\\n{MARK}/x/bin:/y/bin{MARK}\\n'"
        ),
    );
    let found = resolve(&shell, Duration::from_secs(3)).expect("the shell answered");
    assert_eq!(found, OsString::from("/x/bin:/y/bin"));
}

/// **A shell that says nothing is given up on, and ended.** A broken rc
/// file costs one launch its patience and no more, and leaves nothing
/// behind: neither the script's own process nor what it started is running
/// once `resolve` answers.
#[test]
fn a_shell_that_does_not_answer_is_given_up_on_and_ended() {
    let dir = temp_dir("silent");
    let pid_file = dir.join("pid");
    // What the rc file started, standing in for a hung `nvm` or `pyenv`: a
    // script of its own, so that its command line names this test's
    // directory as the shell's does. Ending the shell alone would leave it
    // running; only the group's end takes it.
    let sleeper = dir.join("sleeper");
    std::fs::write(
        &sleeper,
        "#!/bin/sh\n[ \"$1\" = warm ] && exit 0\nsleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let shell = shell(
        &dir,
        &format!(
            "[ \"$1\" = warm ] && exit 0\necho $$ > '{}'\n'{}'",
            pid_file.display(),
            sleeper.display()
        ),
    );
    // Each script run once first: a new script's first exec can outlast
    // 300 ms while macOS assesses it, and a sleeper never started would
    // make the check below pass on nothing. Made to fail with the group's
    // end replaced by `child.kill()`: *still running: …/sleeper*.
    for script in [&sleeper, &shell] {
        std::process::Command::new(script)
            .arg("warm")
            .status()
            .unwrap();
    }

    let asked = Instant::now();
    let answer = resolve(&shell, Duration::from_millis(300));
    assert!(answer.is_err(), "{answer:?}");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );

    // By its pid when the script got as far as writing it — a new script's
    // first run can be slow while the system assesses it, and 300 ms under
    // a full test run is sometimes not that far — and by its command line
    // always, which names this test's own directory — the shell's and the
    // sleeper's alike. An orphan the group's end took is reaped by launchd
    // rather than by us, so the list is asked again for a moment, by a
    // clock, before anything left in it is believed.
    if let Ok(pid) = std::fs::read_to_string(&pid_file) {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(
            !alive.success(),
            "the shell {} is still running",
            pid.trim()
        );
    }
    let named_after = |dir: &Path| {
        let listed = std::process::Command::new("/bin/ps")
            .args(["-Ao", "command="])
            .output()
            .unwrap();
        String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter(|line| line.contains(&*dir.to_string_lossy()))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let looked = Instant::now();
    let mut named = named_after(&dir);
    while !named.is_empty() && looked.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
        named = named_after(&dir);
    }
    assert!(named.is_empty(), "still running: {named:?}");
}

/// **Read at the mark, not at the end.** An rc file that starts something
/// in the background hands it the shell's stdout, and the pipe stays open
/// long after both marks — task 0.3 measured a read to the end waiting the
/// whole patience.
#[test]
fn a_shell_whose_child_keeps_the_pipe_is_read_at_the_mark() {
    let dir = temp_dir("background");
    let child_file = dir.join("child");
    let shell = shell(
        &dir,
        &format!(
            "[ \"$1\" = warm ] && exit 0\nsleep 30 &\necho $! > '{}'\nprintf '\\n{MARK}/z/bin{MARK}\\n'",
            child_file.display()
        ),
    );
    // Run once first, so that the second below is timed without a new
    // script's first exec, which under a full run has measured 1.8 s
    // while macOS assesses it — the read at the mark is what is timed.
    std::process::Command::new(&shell)
        .arg("warm")
        .status()
        .unwrap();
    let asked = Instant::now();
    let found = resolve(&shell, Duration::from_secs(3)).expect("the shell answered");
    assert_eq!(found, OsString::from("/z/bin"));
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );
    // The background child is the rc file's, not Guild's to end; the test
    // ends its own so that it does not outlive the run.
    if let Ok(child) = std::fs::read_to_string(&child_file) {
        let _ = std::process::Command::new("/bin/kill")
            .arg(child.trim())
            .status();
    }
}
