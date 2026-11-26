use guild_desktop::terminal::emulator::{Emulator, GridSize};
use portable_pty::CommandBuilder;
use std::time::{Duration, Instant};

fn completed(emulator: &mut Emulator) -> portable_pty::ExitStatus {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = emulator.exit_status() {
            return status;
        }
        assert!(Instant::now() < until, "terminal job did not complete");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn terminal_jobs_drain_output_without_a_view_and_keep_exit_status() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("finished");
    let mut command = CommandBuilder::new("/bin/sh");
    command.args([
        "-c",
        "head -c 1048576 /dev/zero | tr '\\000' x; printf done > \"$1\"; exit 7",
        "fixture",
    ]);
    command.arg(&marker);
    let mut emulator = Emulator::spawn_job(command, temp.path(), GridSize::new(80, 24)).unwrap();
    assert_eq!(completed(&mut emulator).exit_code(), 7);
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "done");
    assert_eq!(emulator.exit_status().unwrap().exit_code(), 7);
    emulator.kill();
    assert_eq!(emulator.exit_status().unwrap().exit_code(), 7);
}

#[test]
fn terminal_jobs_stop_descendants_before_reaping_an_exited_leader() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("heartbeat");
    let mut control = std::process::Command::new("sleep")
        .arg("20")
        .spawn()
        .unwrap();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "(trap '' HUP TERM; i=0; while [ \"$i\" -lt 100 ]; do printf . >> \"$1\"; i=$((i+1)); sleep 0.02; done) & while [ ! -s \"$1\" ]; do sleep 0.01; done; exit 7", "fixture"]);
    command.arg(&marker);
    let mut emulator = Emulator::spawn_job(command, temp.path(), GridSize::new(80, 24)).unwrap();
    let status = completed(&mut emulator);
    let before = std::fs::read(&marker).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let after = std::fs::read(&marker).unwrap();
    let control_alive = control.try_wait().unwrap().is_none();
    let _ = control.kill();
    let _ = control.wait();
    assert_eq!(status.exit_code(), 7);
    assert_eq!(before, after, "background writer survived job completion");
    assert!(control_alive, "unrelated process was terminated");
}

#[test]
fn terminal_jobs_cancel_is_repeatable_and_keeps_the_final_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "trap '' HUP TERM; sleep 20"]);
    let mut emulator = Emulator::spawn_job(command, temp.path(), GridSize::new(80, 24)).unwrap();
    emulator.kill();
    emulator.kill();
    let status = completed(&mut emulator);
    assert!(!status.success());
    let again = emulator.exit_status().unwrap();
    assert_eq!(again.exit_code(), status.exit_code());
    assert_eq!(again.signal(), status.signal());
}

use guild_desktop::{terminal::view::Look, terminal_job::LiveJobs};
use guild_session::{
    ProjectId, SessionId,
    terminal_job::{JobId, JobState, LaunchSpec},
};

fn spec(owner: SessionId, cwd: &std::path::Path) -> LaunchSpec {
    serde_json::from_value(serde_json::json!({
        "version": 1, "request_id": "one", "program": "/bin/sh",
        "args": ["-c", "printf started >> started; exit 7"], "cwd": cwd,
        "target": {"kind": "session", "id": owner}, "env": {"PRIVATE_FIXTURE": "never-store-me"}
    }))
    .unwrap()
}
fn look() -> Look {
    Look::of(&Default::default(), Default::default())
}
fn poll_job(jobs: &mut LiveJobs, id: &JobId, cx: &mut gpui::App) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        jobs.poll(cx);
        if jobs.receipt(id).unwrap().state.is_terminal() {
            return;
        }
        assert!(Instant::now() < until, "job did not complete");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui::test]
fn terminal_jobs_retain_receipts_and_release_completed_entities(cx: &mut gpui::TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("receipts.json");
    let owner = SessionId::new();
    let project = ProjectId::new();
    let spec = spec(owner, temp.path());
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(path.clone());
        let first = jobs.start(owner, project, &spec, look(), cx).unwrap();
        let again = jobs.start(owner, project, &spec, look(), cx).unwrap();
        assert_eq!(first.id, again.id);
        poll_job(&mut jobs, &first.id, cx);
        assert_eq!(jobs.receipt(&first.id).unwrap().exit_code, Some(7));
        assert!(jobs.terminal(&first.id).is_some());
        jobs.release(&first.id).unwrap();
        assert!(jobs.terminal(&first.id).is_none());
        assert_eq!(jobs.receipt(&first.id).unwrap().exit_code, Some(7));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("started")).unwrap(),
            "started"
        );
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("never-store-me")
        );
        let restored = LiveJobs::load(path);
        assert_eq!(restored.receipt(&first.id).unwrap().exit_code, Some(7));
        assert!(restored.terminal(&first.id).is_none());
        assert_eq!(
            serde_json::to_value(restored.receipt(&first.id).unwrap()).unwrap()["output_deleted"],
            true
        );
    });
}

#[gpui::test]
fn terminal_jobs_failed_receipt_write_prevents_spawn(cx: &mut gpui::TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("not-a-directory");
    let owner = SessionId::new();
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(parent.join("jobs.json"));
        // Break storage after loading: this must reach the reservation write.
        std::fs::write(&parent, "occupied").unwrap();
        let error = jobs
            .start(
                owner,
                ProjectId::new(),
                &spec(owner, temp.path()),
                look(),
                cx,
            )
            .unwrap_err();
        assert!(error.contains("no process was started"), "{error}");
        assert_eq!(jobs.receipts()[0].state, JobState::FailedToStart);
        assert!(!temp.path().join("started").exists());
    });
}

#[gpui::test]
fn terminal_jobs_unavailable_program_has_a_stable_failed_receipt(cx: &mut gpui::TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("receipts.json");
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut spec = spec(owner, temp.path());
    spec.program = temp.path().join("missing-program").display().to_string();
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(path.clone());
        let failed = jobs.start(owner, project, &spec, look(), cx).unwrap();
        assert_eq!(failed.state, JobState::FailedToStart);
        assert!(failed.error.is_some());
        assert!(jobs.terminal(&failed.id).is_none());
        assert_eq!(
            jobs.start(owner, project, &spec, look(), cx).unwrap(),
            failed
        );
        assert_eq!(
            LiveJobs::load(path).receipt(&failed.id).unwrap().state,
            failed.state
        );
        assert!(!temp.path().join("started").exists());
    });
}

#[gpui::test]
fn terminal_jobs_late_storage_failure_keeps_the_child_supervised(cx: &mut gpui::TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("receipts.json");
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut spec = spec(owner, temp.path());
    spec.args = vec![
        "-c".into(),
        "printf started >> started; while [ ! -f proceed ]; do sleep 0.01; done; exit 7".into(),
    ];
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(path.clone());
        let first = jobs.start(owner, project, &spec, look(), cx).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            jobs.present(
                &first.id,
                None,
                guild_session::terminal_job::Presentation::Hidden
            )
            .is_err()
        );
        let retry = jobs.start(owner, project, &spec, look(), cx).unwrap();
        assert_eq!(retry.id, first.id);
        assert_eq!(retry.state, JobState::Running);
        assert!(retry.error.is_some());
        assert!(jobs.terminal(&first.id).is_some());
        std::fs::write(temp.path().join("proceed"), "").unwrap();
        poll_job(&mut jobs, &first.id, cx);
        let completed = jobs.receipt(&first.id).unwrap();
        assert_eq!(completed.exit_code, Some(7));
        assert!(completed.error.is_some());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("started")).unwrap(),
            "started"
        );
        std::fs::remove_dir(&path).unwrap();
        jobs.release(&first.id).unwrap();
        assert!(jobs.error().is_none());
        assert_eq!(
            LiveJobs::load(path).receipt(&first.id).unwrap().exit_code,
            Some(7)
        );
    });
}

#[gpui::test]
fn terminal_jobs_restart_never_relaunches_unresolved_receipts(cx: &mut gpui::TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("receipts.json");
    let owner = SessionId::new();
    let project = ProjectId::new();
    let mut spec = spec(owner, temp.path());
    spec.args = vec!["-c".into(), "sleep 20".into()];
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(path.clone());
        let receipt = jobs.start(owner, project, &spec, look(), cx).unwrap();
        let mut recovered = LiveJobs::load(path);
        assert_eq!(
            recovered.receipt(&receipt.id).unwrap().state,
            JobState::Interrupted
        );
        assert!(recovered.start(owner, project, &spec, look(), cx).is_err());
        assert!(jobs.release(&receipt.id).is_err());
        jobs.shutdown(cx);
        // Shutdown revokes immediately; final completion arrives through the
        // same public poll path, rather than a blocking UI-thread join.
        poll_job(&mut jobs, &receipt.id, cx);
        assert!(jobs.receipt(&receipt.id).unwrap().state.is_terminal());
    });
}

#[test]
fn terminal_jobs_authorize_live_shell_and_builder_but_never_foreign_targets() {
    use guild_desktop::terminal_job::authorize;
    use guild_session::{Desk, PaneId, Runs, SessionRecord, overlay::Target};
    let mut desk = Desk::default();
    let project = desk.add_project(std::path::Path::new("/fixture"));
    let owner = desk.add_session(SessionRecord::new(
        "shell".into(),
        project,
        "/fixture".into(),
        Runs::Shell,
    ));
    let pane = PaneId::new();
    for builder in [false, true] {
        if builder {
            desk.mark_task(owner);
        }
        assert_eq!(
            authorize(
                &desk,
                Some(&owner.to_string()),
                Some(&Target::Session(owner)),
                Some(pane)
            )
            .unwrap(),
            (owner, project)
        );
        assert!(
            authorize(
                &desk,
                Some(&owner.to_string()),
                Some(&Target::Pane(pane)),
                Some(pane)
            )
            .is_ok()
        );
        assert!(
            authorize(
                &desk,
                Some(&owner.to_string()),
                Some(&Target::Pane(PaneId::new())),
                Some(pane)
            )
            .is_err()
        );
        assert!(
            authorize(
                &desk,
                Some(&owner.to_string()),
                Some(&Target::Session(SessionId::new())),
                Some(pane)
            )
            .is_err()
        );
        assert!(
            authorize(
                &desk,
                Some(&owner.to_string()),
                Some(&Target::Session(owner)),
                None
            )
            .is_err()
        );
    }
    assert!(authorize(&desk, None, None, None).is_err());
    assert!(authorize(&desk, Some("forged"), None, None).is_err());
    assert!(authorize(&desk, Some(&SessionId::new().to_string()), None, None).is_err());
    desk.close_session(owner);
    assert!(authorize(&desk, Some(&owner.to_string()), None, Some(pane)).is_err());
}

#[gpui::test]
fn terminal_jobs_native_reattachment_keeps_original_owner_and_retained_process(
    cx: &mut gpui::TestAppContext,
) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    use guild_session::{PaneId, overlay::Target, terminal_job::Presentation};
    let temp = tempfile::tempdir().unwrap();
    let owner = SessionId::new();
    let mut spec = spec(owner, temp.path());
    spec.args = vec!["-c".into(), "sleep 20".into()];
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(temp.path().join("receipts.json"));
        let receipt = jobs
            .start(owner, ProjectId::new(), &spec, look(), cx)
            .unwrap();
        let retained = jobs.terminal(&receipt.id).unwrap();
        jobs.present(&receipt.id, None, Presentation::Closed)
            .unwrap();
        assert_eq!(jobs.receipt(&receipt.id).unwrap().state, JobState::Running);
        let target = Target::Pane(PaneId::new());
        jobs.present(&receipt.id, Some(target.clone()), Presentation::Shown)
            .unwrap();
        let shown = jobs.receipt(&receipt.id).unwrap();
        assert_eq!(shown.owner, owner);
        assert_eq!(shown.target, Some(target));
        assert_eq!(jobs.terminal(&receipt.id).unwrap(), retained);
        jobs.shutdown(cx);
        poll_job(&mut jobs, &receipt.id, cx);
    });
}

#[gpui::test]
fn terminal_jobs_focused_terminal_delivers_escape_to_the_child(cx: &mut gpui::TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    use gpui::{Focusable, px};
    let temp = tempfile::tempdir().unwrap();
    let ready = temp.path().join("ready");
    let got = temp.path().join("key");
    let mut command = CommandBuilder::new("/bin/sh");
    command.args([
        "-c",
        "stty raw -echo; touch ready; dd bs=1 count=1 of=key 2>/dev/null",
    ]);
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let terminal =
            guild_desktop::terminal::view::open_job(command, temp.path(), look(), cx).unwrap();
        terminal.focus_handle(cx).focus(window, cx);
        gpui_component::Root::new(terminal, window, cx)
    });
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    let until = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("escape");
    while std::fs::read(&got).unwrap_or_default().is_empty() {
        assert!(Instant::now() < until, "Escape never reached the child");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read(got).unwrap(), b"\x1b");
}

#[test]
fn terminal_jobs_ansi_teardown_reproduction() {
    for (script, expected) in [
        ("printf diagnostic", "diagnostic"),
        ("true", ""),
        (
            "printf '\\033[?1049h\\033[Hfullscreen-result\\033[2J\\033[?1049l'",
            "",
        ),
        (
            "printf '\\033[?1049h\\033[Hfullscreen-result\\033[?1049l'; printf trailing-diagnostic",
            "trailing-diagnostic",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        let mut emulator =
            Emulator::spawn_job(command, temp.path(), GridSize::new(80, 24)).unwrap();
        assert_eq!(completed(&mut emulator).exit_code(), 0);
        // Wait for the reader independently of the supervisor's process completion.
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if emulator.drain_events().iter().any(|event| {
                matches!(
                    event,
                    guild_desktop::terminal::emulator::TerminalEvent::Exited
                )
            }) {
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        for size in [
            GridSize::new(80, 24),
            GridSize::new(100, 30),
            GridSize::new(45, 12),
        ] {
            emulator.resize(size);
            let text: String = emulator
                .term()
                .lock()
                .unwrap()
                .grid()
                .display_iter()
                .map(|cell| cell.c)
                .collect();
            assert_eq!(text.trim(), expected, "script={script}, size={size:?}");
        }
    }
}

#[gpui::test]
fn terminal_jobs_explain_empty_deleted_and_unavailable_output(cx: &mut gpui::TestAppContext) {
    use guild_desktop::terminal_job::OutputState;
    for (script, expected) in [
        ("printf diagnostic", OutputState::Retained),
        ("true", OutputState::Empty),
        (
            "printf '\\033[?1049h\\033[Hresult\\033[2J\\033[?1049l'",
            OutputState::Empty,
        ),
        (
            "printf '\\033[?1049hresult\\033[?1049l'; printf trailing",
            OutputState::Retained,
        ),
        (
            "printf history; i=0; while [ $i -lt 40 ]; do printf '\\r\\n'; i=$((i+1)); done",
            OutputState::Retained,
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("jobs.json");
        let owner = SessionId::new();
        let mut spec = spec(owner, temp.path());
        spec.args = vec!["-c".into(), script.into()];
        cx.update(|cx| {
            let mut jobs = LiveJobs::load(path.clone());
            let receipt = jobs
                .start(owner, ProjectId::new(), &spec, look(), cx)
                .unwrap();
            poll_job(&mut jobs, &receipt.id, cx);
            // Process completion and PTY drain are independently scheduled.
            let until = Instant::now() + Duration::from_secs(5);
            while jobs.output_state(&receipt.id, cx) != Some(expected) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                jobs.output_state(&receipt.id, cx),
                Some(expected),
                "{script}"
            );
            let restarted = LiveJobs::load(path.clone());
            assert_eq!(
                restarted.output_state(&receipt.id, cx),
                Some(OutputState::Unavailable)
            );
            jobs.release(&receipt.id).unwrap();
            assert_eq!(
                jobs.output_state(&receipt.id, cx),
                Some(OutputState::Deleted)
            );
            assert_eq!(
                LiveJobs::load(path).output_state(&receipt.id, cx),
                Some(OutputState::Deleted)
            );
            assert_eq!(jobs.receipt(&receipt.id).unwrap().owner, owner);
        });
    }
}

#[test]
fn terminal_jobs_labels_keep_calendar_time_and_unknown_legacy_time() {
    use guild_desktop::i18n::{Language, Text, Ui};
    for language in [Language::English, Language::Russian] {
        let ui = Ui(language);
        assert_eq!(
            ui.terminal_started(None, 0, 10800),
            ui.text(Text::TerminalUnknownTime)
        );
        assert_eq!(
            ui.terminal_started(Some(0), 86400, 10800),
            "01.01.1970 03:00:00"
        );
        assert_ne!(
            ui.text(Text::TerminalOutputDeleted),
            ui.text(Text::TerminalOutputUnavailable)
        );
    }
}

#[test]
fn terminal_jobs_today_uses_local_day_and_older_jobs_keep_the_date() {
    use guild_desktop::i18n::{Language, Ui};
    for language in [Language::English, Language::Russian] {
        let ui = Ui(language);
        assert_eq!(ui.terminal_started(Some(0), 60, 10800), "03:00:00");
        // UTC is the same day, but local midnight lies between these launches.
        assert_eq!(
            ui.terminal_started(Some(20 * 3600 * 1000), 22 * 3600, 10800),
            "01.01.1970 23:00:00"
        );
        // Different UTC days may still be one local day west of UTC.
        assert_eq!(
            ui.terminal_started(Some(23 * 3600 * 1000), 25 * 3600, -10800),
            "20:00:00"
        );
    }
}

#[gpui::test]
fn archive_view_uses_saved_metadata_after_restart_and_reuses_live_viewer(
    cx: &mut gpui::TestAppContext,
) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    use guild_session::{overlay::Target, terminal_job::RevmuxArchive};
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let program = temp.path().join("custom-review-viewer");
    std::fs::write(
        &program,
        r#"#!/bin/sh
printf '%s\n' "$@" > viewer-args
while IFS= read -r line; do [ "$line" = q ] && break; done
printf '{"saved":true}\n'
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = temp.path().join("receipts.json");
    let owner = SessionId::new();
    let conversation = SessionId::new();
    let original_cwd = temp.path().join("program-cwd");
    std::fs::create_dir(&original_cwd).unwrap();
    let mut original = spec(owner, &original_cwd);
    let context = guild_desktop::terminal_job::launch_context(
        &guild_session::Desk::default(),
        conversation,
        guild_session::ProjectId::new(),
        &original_cwd,
        Some("observed-at-launch"),
    );
    original.revmux_archive = Some(Box::new(RevmuxArchive {
        directory: temp.path().to_path_buf(),
        program,
    }));
    cx.update(|cx| {
        let mut jobs = LiveJobs::load(path.clone());
        let first = jobs
            .start_in(
                owner,
                ProjectId::new(),
                &original,
                Some(context),
                look(),
                cx,
            )
            .unwrap();
        poll_job(&mut jobs, &first.id, cx);
        jobs.release(&first.id).unwrap();
        let mut restored = LiveJobs::load(path.clone());
        let before = restored.receipt(&first.id).unwrap();
        let a = restored
            .open_archive(&first.id, Target::Session(owner), look(), cx)
            .unwrap();
        let b = restored
            .open_archive(&first.id, Target::Session(owner), look(), cx)
            .unwrap();
        assert_eq!(a.id, b.id);
        let linked = restored
            .open_archive(&a.id, Target::Session(owner), look(), cx)
            .unwrap();
        assert_eq!(
            linked.id, a.id,
            "linked rows must reuse the same live viewer"
        );
        assert_eq!(a.title, "custom-review-viewer (archive)");

        let viewer_context = a.launch_context.as_ref().unwrap();
        assert_eq!(viewer_context.conversation, Some(conversation));
        assert_eq!(viewer_context.program_cwd, temp.path());
        assert_eq!(
            before.launch_context.as_ref().unwrap().program_cwd,
            original_cwd
        );
        let mut expected_context = before.launch_context.clone().unwrap();
        expected_context.program_cwd = temp.path().to_path_buf();
        assert_eq!(a.launch_context.as_ref(), Some(&expected_context));
        let history = guild_session::terminal_job::HistoryContext {
            conversation: Some(conversation),
            ..Default::default()
        };
        assert!(history.contains(guild_session::terminal_job::Scope::Session, &a));
        assert_eq!(a.owner, owner);

        // Viewing a saved report must not look like another paid review run.
        let mut viewing = a.clone();
        viewing.state = JobState::Running;
        for (language, expected) in [
            (guild_desktop::i18n::Language::English, "Viewing archive"),
            (guild_desktop::i18n::Language::Russian, "Просмотр архива"),
        ] {
            let ui = guild_desktop::i18n::Ui(language);
            assert_eq!(ui.terminal_status(&viewing), expected);
            let mut ordinary = before.clone();
            ordinary.state = JobState::Running;
            ordinary.title = viewing.title.clone();
            assert_eq!(
                ui.terminal_status(&ordinary),
                ui.text(guild_desktop::i18n::Text::TerminalRunning),
                "an ordinary job's title cannot turn it into an archive viewer"
            );
        }

        assert_eq!(restored.receipt(&first.id).unwrap(), before);
        assert_ne!(a.id, first.id);
        let until = Instant::now() + Duration::from_secs(8);
        let args = temp.path().join("viewer-args");
        let expected_args = format!("view\n--archive\n{}\n", temp.path().display());
        while std::fs::read_to_string(&args).unwrap_or_default() != expected_args {
            assert!(Instant::now() < until, "viewer did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            std::fs::read_to_string(args).unwrap(),
            format!("view\n--archive\n{}\n", temp.path().display())
        );
        restored
            .present(
                &a.id,
                None,
                guild_session::terminal_job::Presentation::Hidden,
            )
            .unwrap();
        assert_eq!(
            restored
                .open_archive(&first.id, Target::Session(owner), look(), cx)
                .unwrap()
                .id,
            a.id
        );
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("q\n".into()));
        restored
            .terminal(&a.id)
            .unwrap()
            .update(cx, |tab, cx| tab.paste(cx));
        poll_job(&mut restored, &a.id, cx);
        assert_eq!(restored.receipt(&a.id).unwrap().exit_code, Some(0));
        let c = restored
            .open_archive(&a.id, Target::Session(owner), look(), cx)
            .unwrap();
        assert_ne!(c.id, a.id);
        assert_eq!(
            restored
                .open_archive(&first.id, Target::Session(owner), look(), cx)
                .unwrap()
                .id,
            c.id
        );
        restored.cancel(&c.id, "test complete", cx).unwrap();
        poll_job(&mut restored, &c.id, cx);
        assert_eq!(restored.receipt(&first.id).unwrap(), before);
        assert!(restored.can_open_archive(&a.id));
        let mut reloaded = LiveJobs::load(path);
        assert!(reloaded.can_open_archive(&a.id));
        assert!(reloaded.terminal(&a.id).is_none());
        let d = reloaded
            .open_archive(&a.id, Target::Session(owner), look(), cx)
            .unwrap();
        reloaded.cancel(&d.id, "test complete", cx).unwrap();
        poll_job(&mut reloaded, &d.id, cx);
        assert_eq!(reloaded.receipt(&first.id).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(original_cwd.join("started")).unwrap(),
            "started"
        );
    });
}

#[test]
fn launch_context_uses_repository_names_and_preserves_user_renames() {
    let mut desk = guild_session::Desk::default();
    desk.normalise();
    let project = desk.open_in(
        desk.active_workspace(),
        std::path::Path::new("/work/feature"),
    );
    for (key, expected) in [("/work/normal/.git", "normal"), ("/work/bare.git", "bare")] {
        let key = std::path::PathBuf::from(key);
        desk.set_repository(project, Some(key.clone()));
        let context = guild_desktop::terminal_job::launch_context(
            &desk,
            SessionId::new(),
            project,
            std::path::Path::new("/program"),
            None,
        );
        assert_eq!(context.repository_label.as_deref(), Some(expected));
        desk.rename_repository(&key, "My repository");
        let renamed = guild_desktop::terminal_job::launch_context(
            &desk,
            SessionId::new(),
            project,
            std::path::Path::new("/program"),
            None,
        );
        assert_eq!(renamed.repository_label.as_deref(), Some("My repository"));
        assert_eq!(context.repository_label.as_deref(), Some(expected));
    }
}

#[test]
fn retained_completed_jobs_do_not_exhaust_descriptors() {
    const CHILD: &str = "GUILD_TEST_RETAINED_PTY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "terminal_jobs::retained_completed_jobs_do_not_exhaust_descriptors",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated descriptor regression failed");
        return;
    }
    // Only the isolated test child changes its own soft limit.
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    limit.rlim_cur = 64.min(limit.rlim_max);
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    let count = || {
        (0..limit.rlim_cur as i32)
            .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
            .count()
    };
    let temp = tempfile::tempdir().unwrap();
    let probe = temp.path().join("receipts.json");
    let baseline = count();
    let mut retained = Vec::new();
    for _ in 0..32 {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf retained-output"]);
        let mut emulator =
            Emulator::spawn_job(command, std::path::Path::new("/"), GridSize::new(80, 24)).unwrap();
        assert!(completed(&mut emulator).success());
        retained.push(emulator);
        let until = Instant::now() + Duration::from_secs(3);
        while count() > baseline + 1 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            count() <= baseline + 1,
            "completed screens retained PTY descriptors: baseline={baseline}, now={}",
            count()
        );
        assert!(retained.last().unwrap().has_text());
        std::fs::write(&probe, b"saved").unwrap();
    }
    std::fs::remove_file(&probe).unwrap();
}

#[gpui::test]
fn receipt_presentation_retry_recovers_after_failed_write(cx: &mut gpui::TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    use guild_session::terminal_job::Presentation;
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("state");
    let path = parent.join("jobs.json");
    cx.update(|cx| {
        let owner = SessionId::new();
        let mut jobs = LiveJobs::load(path.clone());
        let first = jobs
            .start(
                owner,
                ProjectId::new(),
                &spec(owner, temp.path()),
                look(),
                cx,
            )
            .unwrap();
        poll_job(&mut jobs, &first.id, cx);
        let before = jobs.receipt(&first.id).unwrap();
        std::fs::rename(&parent, temp.path().join("kept-state")).unwrap();
        std::fs::write(&parent, "not a directory").unwrap();
        let target = Some(guild_session::overlay::Target::Session(owner));
        assert!(
            jobs.present(&first.id, target.clone(), Presentation::Shown)
                .is_err()
        );
        assert!(jobs.error().is_some());
        assert!(jobs.terminal(&first.id).is_some());
        std::fs::remove_file(&parent).unwrap();
        std::fs::create_dir(&parent).unwrap();
        jobs.present(&first.id, target.clone(), Presentation::Shown)
            .unwrap();
        assert!(
            jobs.error().is_none(),
            "identical Show output must retry a failed save"
        );
        let restored = LiveJobs::load(path);
        let saved = restored.receipt(&first.id).unwrap();
        assert_eq!(saved.exit_code, before.exit_code);
        assert_eq!(saved.owner, before.owner);
        assert_eq!(saved.launch_context, before.launch_context);
        assert!(restored.terminal(&first.id).is_none());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("started")).unwrap(),
            "started"
        );
    });
}

// The helper runs in a private test process: no neighboring test's PTYs can
// confuse device identities, and child-only environment never changes the host.
fn terminal_devices() -> std::collections::BTreeSet<u64> {
    std::fs::read_dir("/dev/fd")
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter_map(|fd| {
            let mut info: libc::stat = unsafe { std::mem::zeroed() };
            if fd > 2
                && unsafe { libc::fstat(fd, &mut info) } == 0
                && info.st_mode & libc::S_IFMT == libc::S_IFCHR
            {
                Some(info.st_rdev as u64)
            } else {
                None
            }
        })
        .collect()
}

#[test]
#[ignore = "descriptor probe invoked only by live_terminal_descriptors_are_not_inherited"]
fn terminal_descriptor_probe() {
    println!(
        "TERMINAL_DEVICES={}",
        serde_json::to_string(&terminal_devices()).unwrap()
    );
}

#[test]
fn live_terminal_descriptors_are_not_inherited() {
    const CHILD: &str = "GUILD_TEST_PTY_INHERITANCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "terminal_jobs::live_terminal_descriptors_are_not_inherited",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated PTY inheritance regression failed"
        );
        return;
    }
    for supervised in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let before = terminal_devices();
        let ready = temp.path().join("ready");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            ": > \"$1\"; read line; printf 'retained:%s' \"$line\"",
            "fixture",
        ]);
        command.arg(&ready);
        let mut emulator = if supervised {
            Emulator::spawn_job(command, temp.path(), GridSize::new(80, 24))
        } else {
            Emulator::spawn(command, temp.path(), GridSize::new(80, 24))
        }
        .unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(Instant::now() < until, "terminal never became ready");
            std::thread::sleep(Duration::from_millis(5));
        }
        let own: std::collections::BTreeSet<_> =
            terminal_devices().difference(&before).copied().collect();
        assert_eq!(own.len(), 1, "one live PTY, regardless of its FD copies");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "terminal_jobs::terminal_descriptor_probe",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        let inherited: std::collections::BTreeSet<u64> = serde_json::from_str(
            text.split_once("TERMINAL_DEVICES=")
                .unwrap()
                .1
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap();

        let mut fake = crate::lsp_fixture::LspFixture::new();
        let (program, args) = fake.command(false);
        let mut client =
            guild_desktop::lsp::client::Client::start(&program, &args, temp.path()).unwrap();
        fake.ready();
        let id = client.request("initialize", serde_json::json!({}));
        let until = Instant::now() + Duration::from_secs(10);
        let answer = loop {
            if let Some(answer) = id.take_answer() {
                break answer.unwrap();
            }
            assert!(Instant::now() < until, "fake LSP never answered");
            std::thread::sleep(Duration::from_millis(5));
        };
        let lsp_inherited: std::collections::BTreeSet<u64> =
            serde_json::from_value(answer["inheritedDevices"].clone()).unwrap();
        client.stop();
        fake.assert_stopped();

        emulator.send(b"marker\n");
        assert!(completed(&mut emulator).success());
        let until = Instant::now() + Duration::from_secs(10);
        while !emulator
            .term()
            .lock()
            .unwrap()
            .grid()
            .display_iter()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains("retained:marker")
        {
            assert!(Instant::now() < until, "retained output never arrived");
            std::thread::sleep(Duration::from_millis(5));
        }
        emulator.resize(GridSize::new(100, 30));
        assert!(
            emulator
                .term()
                .lock()
                .unwrap()
                .grid()
                .display_iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .contains("retained:marker"),
            "diagnostic output must remain after resize"
        );
        assert!(
            own.is_disjoint(&inherited),
            "ordinary child inherited terminal master/input devices: {own:?}, {inherited:?}"
        );
        assert!(
            own.is_disjoint(&lsp_inherited),
            "LSP child inherited terminal master/input devices: {own:?}, {lsp_inherited:?}"
        );
    }
}
