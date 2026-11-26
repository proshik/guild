//! Process completion keeps its original status and consumes ownership once.
use guild_session::process::ProcessGroup;
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn an_exited_leader_without_descendants_is_reaped_with_its_original_status() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 7"]);
    let mut child = ProcessGroup::spawn(&mut command).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(7));
    assert_eq!(child.stop().unwrap(), status);
    assert_eq!(child.try_wait().unwrap(), Some(status));
}

#[test]
fn version_probe_finishes_when_a_descendant_inherits_output() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("codex");
    // The pid file by its absolute path: the test process keeps its own
    // directory.
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n[ \"$1\" = warm ] && exit 0\n/bin/sleep 30 &\necho $! > '{}'\necho 'codex 1.2.3'\n",
            dir.path().join("child.pid").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    // macOS checks a new executable at its first exec; while the suite
    // starts dozens at once that took over 2 s — the probe's whole deadline.
    // Run it once before the probe, so the deadline measures the probe.
    assert!(
        Command::new(&script)
            .arg("warm")
            .status()
            .unwrap()
            .success()
    );
    let path = dir.path().display().to_string();
    let (send, receive) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = guild_session::harness::HarnessKind::Codex.probe_version(&path);
        let _ = send.send(result);
    });
    let result = receive.recv_timeout(Duration::from_secs(3));
    if result.is_err()
        && let Ok(pid) = std::fs::read_to_string(dir.path().join("child.pid"))
    {
        unsafe {
            libc::kill(pid.trim().parse().unwrap(), libc::SIGKILL);
        }
    }
    thread.join().unwrap();
    assert_eq!(
        result.expect("the leader exited; inherited stdout must not hang"),
        Some((1, 2))
    );
}

/// `terminate` lets a group clean up on `SIGTERM` — git removes its lock
/// files there — and still ends it when it ignores the signal.
#[test]
fn terminate_asks_first_and_then_stops() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("cleaned");
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        &format!(
            "trap 'touch {}; exit 3' TERM; while :; do sleep 0.01; done",
            marker.display()
        ),
    ]);
    let mut polite = ProcessGroup::spawn(&mut command).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let status = polite.terminate(Duration::from_secs(5)).unwrap();
    assert!(marker.exists(), "the trap ran");
    assert_eq!(status.code(), Some(3));

    let mut command = Command::new("/bin/sh");
    command.args(["-c", "trap '' TERM; while :; do sleep 0.01; done"]);
    let mut stubborn = ProcessGroup::spawn(&mut command).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    let status = stubborn.terminate(Duration::from_millis(300)).unwrap();
    assert!(status.code().is_none(), "killed");
    assert!(started.elapsed() < Duration::from_secs(5));
}
