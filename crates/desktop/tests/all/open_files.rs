//! The window's open-file limit, raised at its start.
//!
//! macOS hands an application started from the Dock a soft limit of 256,
//! and each agent session holds about seven descriptors: thirty-odd
//! sessions ran the installed Guild out of them on 2026-11-07, and the state
//! store could no longer open its own directory.

#[test]
fn a_dock_sized_limit_is_raised_toward_the_hard_one() {
    const CHILD: &str = "GUILD_TEST_OPEN_FILES_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "open_files::a_dock_sized_limit_is_raised_toward_the_hard_one",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated open-file limit check failed");
        return;
    }

    // Only the isolated child changes its own soft limit: what the Dock
    // gives an application, under whatever hard limit the shell left.
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: plain out-parameter calls on this process's own limit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    let hard = limit.rlim_max;
    assert!(hard > 256, "the fixture needs room above 256: hard {hard}");
    limit.rlim_cur = 256;
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);

    guild_desktop::system::open_files::raise();

    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    assert_eq!(limit.rlim_max, hard, "the hard limit is never touched");
    assert!(
        limit.rlim_cur > 256,
        "the soft limit stayed at what the Dock gave: {}",
        limit.rlim_cur
    );
    assert!(
        limit.rlim_cur <= 10_240,
        "above what macOS accepts for a process: {}",
        limit.rlim_cur
    );

    // And what the number promises is true: well past 256 can be open.
    let held: Vec<_> = (0..400)
        .map(|_| std::fs::File::open("/dev/null"))
        .collect::<Result<_, _>>()
        .expect("400 descriptors open under the raised limit");
    drop(held);
}

#[test]
fn a_limit_already_higher_is_never_lowered() {
    const CHILD: &str = "GUILD_TEST_OPEN_FILES_HIGH_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "open_files::a_limit_already_higher_is_never_lowered",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated open-file limit check failed");
        return;
    }

    let mut before = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: plain out-parameter calls on this process's own limit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut before) },
        0
    );
    guild_desktop::system::open_files::raise();
    let mut after = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut after) },
        0
    );
    assert!(
        after.rlim_cur >= before.rlim_cur,
        "lowered from {} to {}",
        before.rlim_cur,
        after.rlim_cur
    );
    assert_eq!(after.rlim_max, before.rlim_max);
}
