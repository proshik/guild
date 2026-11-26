use guild_desktop::terminal_panel::TerminalTabs;

#[test]
fn terminals_keep_stable_identity_and_select_a_neighbor_on_close() {
    let mut tabs = TerminalTabs::default();
    let first = tabs.add("one");
    let second = tabs.add("two");
    let third = tabs.add("three");
    assert_eq!(tabs.active().unwrap().id, third);
    tabs.select(second);
    assert_eq!(tabs.remove(first).unwrap().value, "one");
    assert_eq!(tabs.active().unwrap().value, "two");
    tabs.remove(second);
    assert_eq!(tabs.active().unwrap().id, third);
    tabs.remove(third);
    assert!(tabs.active().is_none());
    assert!(tabs.add("four") > third);
}

#[test]
fn each_project_keeps_its_terminals_and_closed_ids_cannot_select_another() {
    let mut a = TerminalTabs::default();
    let mut b = TerminalTabs::default();
    let id = a.add("project-a");
    b.add("project-b");
    a.remove(id);
    let replacement = a.add("replacement");
    a.select(id);
    assert_eq!(a.active().unwrap().id, replacement);
    assert_eq!(b.active().unwrap().value, "project-b");
}

#[gpui::test]
fn closing_one_terminal_reaps_its_shell_while_another_keeps_its_cwd_and_process(
    cx: &mut gpui::TestAppContext,
) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    use guild_desktop::terminal::view::{Look, open_command};
    use std::time::{Duration, Instant};
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        let mut tabs = TerminalTabs::default();
        for dir in [first_dir.path(), second_dir.path()] {
            let mut command = portable_pty::CommandBuilder::new("/bin/sh");
            command.args(["-c", "pwd -P > cwd; trap 'exit 0' HUP TERM; while :; do echo tick >> heartbeat; sleep 0.05; done"]);
            let view = open_command(command, dir, Look::of(&Default::default(), Default::default()), None, cx).unwrap();
            tabs.add(view);
        }
        let until = Instant::now() + Duration::from_secs(5);
        while !first_dir.path().join("heartbeat").exists() || !second_dir.path().join("heartbeat").exists() {
            assert!(Instant::now() < until, "both terminals start");
            std::thread::sleep(Duration::from_millis(10));
        }
        for dir in [first_dir.path(), second_dir.path()] {
            assert_eq!(std::fs::read_to_string(dir.join("cwd")).unwrap().trim(), dir.canonicalize().unwrap().to_str().unwrap());
        }
        let first = tabs.entries()[0].id;
        let second = tabs.entries()[1].id;
        tabs.select(first);
        let closed = tabs.remove(first).unwrap();
        let done = closed.value.update(cx, |terminal, _| terminal.let_go()).unwrap();
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(tabs.active().unwrap().id, second);
        let stopped = std::fs::metadata(first_dir.path().join("heartbeat")).unwrap().len();
        let running = std::fs::metadata(second_dir.path().join("heartbeat")).unwrap().len();
        let until = Instant::now() + Duration::from_secs(5);
        while std::fs::metadata(second_dir.path().join("heartbeat")).unwrap().len() <= running {
            assert!(Instant::now() < until, "the neighboring shell must keep running");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::metadata(first_dir.path().join("heartbeat")).unwrap().len(), stopped);
        assert!(std::fs::metadata(second_dir.path().join("heartbeat")).unwrap().len() > running);
        let done = tabs.remove(second).unwrap().value.update(cx, |terminal, _| terminal.let_go()).unwrap();
        done.recv_timeout(Duration::from_secs(3)).unwrap();
    });
}

#[test]
fn panel_shell_constructor_removes_inherited_agent_identity() {
    const CHILD: &str = "GUILD_PANEL_IDENTITY_TEST_CHILD";
    let keys = [
        guild_session::SESSION_VAR,
        guild_session::SOCKET_VAR,
        guild_session::HOOK_SOCKET_VAR,
    ];
    if std::env::var_os(CHILD).is_none() {
        let home = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "terminal_panel::panel_shell_constructor_removes_inherited_agent_identity",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("SHELL", "/bin/sh")
            .env("HOME", home.path());
        for key in keys {
            child.env(key, "unrelated-builder");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    // The child alone receives this environment; parallel tests remain untouched.
    for key in keys {
        assert_eq!(std::env::var(key).unwrap(), "unrelated-builder");
    }
    let mut command = guild_desktop::terminal::shell::panel_command();
    command.args(["-c", "printf 'identity=<%s><%s><%s>\\n' \"${GUILD_SESSION-}\" \"${GUILD_SOCKET-}\" \"${GUILD_HOOK_SOCKET-}\""]);
    let pair = portable_pty::native_pty_system()
        .openpty(portable_pty::PtySize::default())
        .unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut shell = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut output = String::new();
    // Unix PTYs may signal EOF as EIO after the child exits.
    let _ = std::io::Read::read_to_string(&mut reader, &mut output);
    assert!(shell.wait().unwrap().success());
    assert!(output.contains("identity=<><><>"), "{output}");
    assert!(!output.contains("unrelated-builder"), "{output}");
}
