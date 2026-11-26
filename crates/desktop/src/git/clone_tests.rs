use super::*;
use std::os::unix::fs::symlink;

fn request(root: &Path) -> Request {
    Request::new("https://example.test/repo.git", &root.join("final")).unwrap()
}
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", script])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}
fn run(request: Request, command: Command) -> Outcome {
    execute(
        request,
        command,
        Duration::from_millis(150),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(Progress::default())),
    )
}
#[test]
fn clone_worker_publishes_only_after_success() {
    let tmp = tempfile::tempdir().unwrap();
    let outcome = run(
        request(tmp.path()),
        shell("mkdir repo; echo keep > repo/file"),
    );
    assert!(matches!(outcome, Outcome::Succeeded(_)), "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("final/file")).unwrap(),
        "keep\n"
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}
#[test]
fn clone_worker_collision_retains_clone_without_replacing_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(tmp.path());
    let mut command = shell("mkdir repo; echo retained > repo/file; mkdir \"$FINAL\"");
    command.env("FINAL", tmp.path().join("final"));
    let outcome = run(req, command);
    let Outcome::Failed {
        retained: Some(path),
        ..
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert!(path.join("repo/file").exists());
    assert_eq!(
        std::fs::read_dir(tmp.path().join("final")).unwrap().count(),
        0
    );
}
#[test]
fn clone_worker_parent_substitution_before_start_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let other = tmp.path().join("other");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&other).unwrap();
    let req = request(&parent);
    std::fs::rename(&parent, tmp.path().join("moved")).unwrap();
    symlink(&other, &parent).unwrap();
    assert!(matches!(
        run(req, shell("mkdir repo")),
        Outcome::Failed { .. }
    ));
    assert_eq!(std::fs::read_dir(other).unwrap().count(), 0);
}
#[test]
fn clone_worker_parent_substitution_during_clone_cannot_redirect_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let moved = tmp.path().join("moved");
    let other = tmp.path().join("other");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&other).unwrap();
    let req = request(&parent);
    let mut command = shell(
        "mv \"$PARENT\" \"$MOVED\"; ln -s \"$OTHER\" \"$PARENT\"; mkdir repo; echo keep > repo/file",
    );
    command
        .env("PARENT", &parent)
        .env("MOVED", &moved)
        .env("OTHER", &other);
    let Outcome::Failed {
        retained: Some(retained),
        ..
    } = run(req, command)
    else {
        panic!("parent substitution must refuse publication");
    };
    assert!(
        retained.join("repo/file").exists(),
        "disclosed staging path must resolve after parent movement"
    );
    assert_eq!(std::fs::read_dir(other).unwrap().count(), 0);
    assert!(!moved.join("final").exists());
}
#[test]
fn clone_worker_target_symlink_is_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let other = tmp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("keep"), "original").unwrap();
    let mut command = shell("mkdir repo; ln -s \"$OTHER\" \"$FINAL\"");
    command
        .env("OTHER", &other)
        .env("FINAL", tmp.path().join("final"));
    assert!(matches!(
        run(request(tmp.path()), command),
        Outcome::Failed {
            retained: Some(_),
            ..
        }
    ));
    assert_eq!(
        std::fs::read_to_string(other.join("keep")).unwrap(),
        "original"
    );
    assert!(
        std::fs::symlink_metadata(tmp.path().join("final"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
#[test]
fn clone_worker_silent_descendants_timeout_and_missing_git_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let result = run(request(tmp.path()), shell("sleep 30 & wait"));
    assert!(
        matches!(result, Outcome::Failed { ref message, .. } if message.contains("output")),
        "{result:?}"
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    assert!(matches!(
        run(request(tmp.path()), Command::new("/no/such/git")),
        Outcome::Failed { .. }
    ));
}
#[test]
fn clone_worker_nonzero_exit_cleans_only_staging() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("keep"), "keep").unwrap();
    let result = run(
        request(tmp.path()),
        shell("mkdir repo; echo failure >&2; exit 1"),
    );
    assert!(matches!(result, Outcome::Failed { .. }));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}
#[test]
fn clone_worker_diagnostics_are_bounded_redacted_and_handle_cr() {
    let mut p = Progress::default();
    p.line(b"fatal https://user:secret@host/repo?token=other");
    assert!(!p.text.contains("secret"));
    assert!(!p.text.contains("other"));
    for _ in 0..2000 {
        p.line("progress \u{1f980} 12345678901234567890".as_bytes());
    }
    assert!(p.text.len() <= DIAGNOSTIC_LIMIT);
    assert!(!p.text.contains(char::REPLACEMENT_CHARACTER));
}
#[test]
fn clone_worker_child_auth_controls_override_inherited_askpass() {
    let mut command = shell(
        "test \"$GIT_TERMINAL_PROMPT\" = 0 && test \"$GIT_ASKPASS\" = /usr/bin/false && test \"$SSH_ASKPASS\" = /usr/bin/false && mkdir repo",
    );
    command
        .env("GIT_ASKPASS", "/bad/askpass")
        .env("SSH_ASKPASS", "/bad/askpass");
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        run(request(tmp.path()), command),
        Outcome::Succeeded(_)
    ));
}

#[test]
fn clone_worker_default_ssh_uses_batch_and_strict_host_checking() {
    let tmp = tempfile::tempdir().unwrap();
    let mut command = shell(
        "case \"$GIT_SSH_COMMAND\" in *BatchMode=yes*StrictHostKeyChecking=yes*) mkdir repo;; *) exit 9;; esac",
    );
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_SSH");
    assert!(matches!(
        run(request(tmp.path()), command),
        Outcome::Succeeded(_)
    ));
}
#[test]
fn clone_worker_preserves_custom_ssh_and_bounds_silent_override() {
    let tmp = tempfile::tempdir().unwrap();
    let mut command = shell("eval \"$GIT_SSH_COMMAND\"");
    command.env("GIT_SSH_COMMAND", "sleep 30");
    assert!(
        matches!(run(request(tmp.path()), command), Outcome::Failed { ref message, .. } if message.contains("output"))
    );
}
#[test]
fn clone_worker_successful_real_git_including_empty_repository() {
    for populated in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let git = |args: &[&str]| {
            Command::new("git")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(args)
                .output()
                .unwrap()
        };
        assert!(git(&["init", source.to_str().unwrap()]).status.success());
        if populated {
            std::fs::write(source.join("file"), "content").unwrap();
            assert!(
                git(&["-C", source.to_str().unwrap(), "add", "file"])
                    .status
                    .success()
            );
            assert!(
                git(&[
                    "-C",
                    source.to_str().unwrap(),
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.test",
                    "commit",
                    "-m",
                    "fixture"
                ])
                .status
                .success()
            );
        }
        let req = request(tmp.path());
        let mut command = Command::new("git");
        command
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                &format!(
                    "url.{}.insteadOf=https://example.test/repo.git",
                    source.display()
                ),
            ])
            .args(
                super::super::Write::Clone {
                    url: req.url.clone(),
                    destination: "repo".into(),
                }
                .args(),
            );
        let outcome = execute(
            req,
            command,
            Duration::from_secs(10),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(Progress::default())),
        );
        assert!(matches!(outcome, Outcome::Succeeded(_)), "{outcome:?}");
        assert!(tmp.path().join("final/.git").is_dir());
        assert_eq!(tmp.path().join("final/file").exists(), populated);
    }
}
#[test]
fn clone_worker_cancel_stops_readers_and_preserves_new_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new(Progress::default()));
    let req = request(tmp.path());
    let mut command = shell("mkdir repo; mkdir \"$FINAL\"; echo ready; sleep 30 & wait");
    command.env("FINAL", tmp.path().join("final"));
    let worker_stop = stop.clone();
    let worker_progress = progress.clone();
    let worker = thread::spawn(move || {
        execute(
            req,
            command,
            Duration::from_secs(10),
            worker_stop,
            worker_progress,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !progress.lock().unwrap().text.contains("ready") {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    stop.store(true, Ordering::Release);
    assert!(matches!(
        worker.join().unwrap(),
        Outcome::Cancelled { retained: None }
    ));
    assert!(tmp.path().join("final").is_dir());
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn clone_worker_output_resets_watchdog_and_large_stream_is_drained() {
    let tmp = tempfile::tempdir().unwrap();
    let progress = Arc::new(Mutex::new(Progress::default()));
    let result = execute(
        request(tmp.path()),
        shell(
            "mkdir repo; for x in 1 2 3 4; do echo progress; sleep 0.06; done; i=0; while [ $i -lt 10000 ]; do printf 'progress\\r'; i=$((i+1)); done",
        ),
        Duration::from_millis(150),
        Arc::new(AtomicBool::new(false)),
        progress.clone(),
    );
    assert!(matches!(result, Outcome::Succeeded(_)), "{result:?}");
    let output = &progress.lock().unwrap().text;
    assert!(output.len() <= DIAGNOSTIC_LIMIT);
    assert!(output.contains("progress"));
}
#[test]
fn clone_worker_core_ssh_command_is_preserved_without_global_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("gitconfig");
    std::fs::write(
        &config,
        "[core]\nsshCommand = custom-ssh --existing-option\n",
    )
    .unwrap();
    let mut command =
        shell("test \"$GIT_SSH_COMMAND\" = 'custom-ssh --existing-option' && mkdir repo");
    command
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_SSH");
    assert!(matches!(
        run(request(tmp.path()), command),
        Outcome::Succeeded(_)
    ));
}
#[test]
fn clone_worker_prestart_target_substitution_leaves_it_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(tmp.path());
    std::fs::write(tmp.path().join("final"), "keep").unwrap();
    assert!(matches!(
        run(req, shell("mkdir repo")),
        Outcome::Failed { retained: None, .. }
    ));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("final")).unwrap(),
        "keep"
    );
}
#[test]
fn clone_worker_cancel_before_start_creates_no_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let result = execute(
        request(tmp.path()),
        shell("mkdir repo"),
        Duration::from_secs(1),
        Arc::new(AtomicBool::new(true)),
        Arc::new(Mutex::new(Progress::default())),
    );
    assert!(matches!(result, Outcome::Cancelled { retained: None }));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn clone_worker_redacts_complete_authorization_header() {
    let mut progress = Progress::default();
    progress.line(b"Authorization: Basic example-secret-payload");
    assert!(!progress.text.contains("example-secret-payload"));
}

#[test]
fn clone_worker_unreadable_source_fails_without_registerable_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(tmp.path());
    let mut command = Command::new("git");
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            &format!(
                "url.{}.insteadOf=https://example.test/repo.git",
                tmp.path().join("missing").display()
            ),
        ])
        .args(
            super::super::Write::Clone {
                url: req.url.clone(),
                destination: "repo".into(),
            }
            .args(),
        );
    assert!(matches!(
        run(req, command),
        Outcome::Failed { retained: None, .. }
    ));
    assert!(!tmp.path().join("final").exists());
}
#[test]
fn clone_worker_failed_staging_symlink_cleanup_preserves_external_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let protected = tmp.path().join("protected");
    std::fs::create_dir(&protected).unwrap();
    std::fs::write(protected.join("keep"), "original").unwrap();
    let mut command = shell("mkdir repo; ln -s \"$PROTECTED\" repo/link; exit 1");
    command.env("PROTECTED", &protected);
    assert!(matches!(
        run(request(tmp.path()), command),
        Outcome::Failed { retained: None, .. }
    ));
    assert_eq!(
        std::fs::read_to_string(protected.join("keep")).unwrap(),
        "original"
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}
#[test]
fn clone_worker_drop_cancels_and_joins_its_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let req = request(tmp.path());
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new(Progress::default()));
    let stop = cancel.clone();
    let output = progress.clone();
    let join = thread::spawn(move || {
        execute(
            req,
            shell("mkdir repo; echo ready; sleep 30 & wait"),
            Duration::from_secs(10),
            stop,
            output,
        )
    });
    let worker = Worker {
        cancel,
        progress: progress.clone(),
        thread: Some(join),
    };
    let until = Instant::now() + Duration::from_secs(5);
    while !progress.lock().unwrap().text.contains("ready") {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(10));
    }
    drop(worker);
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn clone_worker_real_git_silent_ssh_override_is_terminated() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let ssh = tmp.path().join("ssh-helper");
    std::fs::write(&ssh, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).unwrap();
    let req = Request::new("git@example.invalid:repo.git", &tmp.path().join("final")).unwrap();
    let mut command = Command::new("git");
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_SSH_COMMAND", &ssh)
        .env("GIT_SSH_VARIANT", "ssh")
        .args(
            super::super::Write::Clone {
                url: req.url.clone(),
                destination: "repo".into(),
            }
            .args(),
        );
    let start = Instant::now();
    assert!(
        matches!(run(req, command), Outcome::Failed { ref message, retained: None } if message.contains("output"))
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}
#[test]
fn clone_worker_real_git_silent_credential_helper_is_terminated() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request);
                    stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    return true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("{error}"),
            }
            if Instant::now() >= until {
                return false;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let req = request(tmp.path());
    let mut command = Command::new("git");
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("NO_PROXY", "127.0.0.1")
        .env("no_proxy", "127.0.0.1")
        .args([
            "-c",
            &format!("url.http://{address}/repo.insteadOf=https://example.test/repo.git"),
            "-c",
            "credential.helper=!f() { sleep 30; }; f",
        ])
        .args(
            super::super::Write::Clone {
                url: req.url.clone(),
                destination: "repo".into(),
            }
            .args(),
        );
    let outcome = run(req, command);
    assert!(
        server.join().unwrap(),
        "Git must reach the isolated 401 fixture before the helper blocks"
    );
    assert!(
        matches!(outcome, Outcome::Failed { ref message, retained: None } if message.contains("output")),
        "{outcome:?}"
    );
}
#[test]
fn clone_worker_unwritable_parent_is_refused_without_creating_paths() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let outcome = run(request(tmp.path()), shell("mkdir repo"));
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(outcome, Outcome::Failed { retained: None, .. }));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}
#[test]
fn clone_worker_staging_substitution_is_retained_without_following_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let protected = tmp.path().join("protected");
    let moved = tmp.path().join("moved-staging");
    std::fs::create_dir(&protected).unwrap();
    std::fs::write(protected.join("keep"), "original").unwrap();
    let mut command = shell(
        "old=$(pwd); mv \"$old\" \"$MOVED\"; ln -s \"$PROTECTED\" \"$old\"; mkdir repo; exit 1",
    );
    command.env("MOVED", &moved).env("PROTECTED", &protected);
    let Outcome::Failed {
        retained: Some(retained),
        ..
    } = run(request(tmp.path()), command)
    else {
        panic!("unowned staging pathname must not be removed");
    };
    assert_eq!(retained, moved.canonicalize().unwrap());
    assert!(retained.join("repo").exists());
    assert_eq!(
        std::fs::read_to_string(protected.join("keep")).unwrap(),
        "original"
    );
}
