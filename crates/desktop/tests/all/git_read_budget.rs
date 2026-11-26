//! A workspace-wide polling burst must leave descriptors for state persistence.

use std::sync::{Arc, Barrier};

#[test]
fn concurrent_git_reads_leave_room_for_state_saves() {
    const CHILD: &str = "GUILD_TEST_GIT_READ_BUDGET_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "git_read_budget::concurrent_git_reads_leave_room_for_state_saves",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated Git descriptor regression failed"
        );
        return;
    }

    let folder = tempfile::tempdir().unwrap();
    let store =
        guild_session::Store::<guild_session::Settings>::new(folder.path().join("state.json"));
    store.save(&guild_session::Settings::default()).unwrap();
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: only this isolated child changes its own soft descriptor limit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    limit.rlim_cur = 64.min(limit.rlim_max);
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    let _held: Vec<_> = (0..20)
        .map(|_| std::fs::File::open("/dev/null").unwrap())
        .collect();

    for round in 0..4 {
        let start = Arc::new(Barrier::new(33));
        let results = std::thread::scope(|scope| {
            let mut reads = Vec::new();
            for _ in 0..32 {
                let start = start.clone();
                let root = folder.path();
                reads.push(scope.spawn(move || {
                    start.wait();
                    guild_desktop::git::run(root, &["--version"])
                }));
            }
            start.wait();
            let state = guild_session::Settings {
                ui_font_size: 14.0 + round as f32,
                ..Default::default()
            };
            let saved = store.save(&state);
            let reads: Vec<_> = reads.into_iter().map(|read| read.join().unwrap()).collect();
            (saved, reads)
        });
        let failures: Vec<_> = results
            .1
            .iter()
            .filter_map(|read| read.as_ref().err())
            .collect();
        assert!(
            failures.is_empty(),
            "Git polling exhausted descriptors: {failures:?}"
        );
        results.0.expect("state save during concurrent Git polling");
        for bytes in results.1.into_iter().map(Result::unwrap) {
            assert!(bytes.starts_with(b"git version "), "Git output changed");
        }
        assert_eq!(store.load().unwrap().ui_font_size, 14.0 + round as f32);
    }

    // Refusal and spawn failure must not retain any of the shared capacity.
    for _ in 0..8 {
        assert!(guild_desktop::git::run(folder.path(), &["guild-invalid-command"]).is_err());
        assert!(guild_desktop::git::run(&folder.path().join("missing"), &["--version"]).is_err());
    }
    assert!(guild_desktop::git::run(folder.path(), &["--version"]).is_ok());
}
