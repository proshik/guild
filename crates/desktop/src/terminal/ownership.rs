//! Ownership retained only while an already identified PTY is being cleaned up.
use std::sync::{Arc, Mutex, Weak};

static CLOSING: Mutex<Vec<Weak<Cleanup>>> = Mutex::new(Vec::new());

pub(super) struct Cleanup {
    leader: Option<libc::pid_t>,
    groups: Vec<libc::pid_t>,
    active: std::sync::atomic::AtomicBool,
}

impl Cleanup {
    pub(super) fn finish(&self) {
        // Complete in-flight identity queries before releasing the reserved PID.
        let _closing = CLOSING.lock().unwrap();
        self.active
            .store(false, std::sync::atomic::Ordering::Release);
    }

    pub(super) fn retain(leader: Option<libc::pid_t>, groups: &[libc::pid_t]) -> Arc<Self> {
        let owner = Arc::new(Self {
            leader,
            groups: groups.to_vec(),
            active: std::sync::atomic::AtomicBool::new(true),
        });
        let mut closing = CLOSING.lock().unwrap();
        closing.retain(|entry| entry.strong_count() != 0);
        closing.push(Arc::downgrade(&owner));
        owner
    }
}

/// One holder decision, relative to the cleanup evidence retained before ps/lsof.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    Closing,
    Foreign,
    Changed,
    Uncertain,
}

/// Keep a witness across the process snapshot, without extending cleanup or its
/// PID reservation. An inactive witness only requests a fresh observation.
pub(crate) struct Snapshot(Vec<Arc<Cleanup>>);

impl Snapshot {
    pub(crate) fn capture() -> Self {
        let closing = CLOSING.lock().unwrap();
        Self(
            closing
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|owner| owner.active.load(std::sync::atomic::Ordering::Acquire))
                .collect(),
        )
    }

    pub(crate) fn holder(&self, pid: u32) -> Holder {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return Holder::Uncertain;
        };
        if pid <= 0 {
            return Holder::Uncertain;
        }
        let _closing = CLOSING.lock().unwrap();
        // SAFETY: identity queries only. finish() takes the same lock before
        // releasing the leader's reservation. ESRCH invalidates a stale ps row;
        // other errors never establish that the holder is free.
        let sid = unsafe { libc::getsid(pid) };
        if sid < 0 {
            return query_failure();
        }
        let group = unsafe { libc::getpgid(pid) };
        if group < 0 {
            return query_failure();
        }
        for owner in &self.0 {
            if owner.leader == Some(sid) && owner.groups.contains(&group) {
                return if owner.active.load(std::sync::atomic::Ordering::Acquire) {
                    Holder::Closing
                } else {
                    Holder::Changed
                };
            }
        }
        Holder::Foreign
    }
}

fn query_failure() -> Holder {
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Holder::Changed
    } else {
        Holder::Uncertain
    }
}

#[cfg(test)]
fn closing_owns(pid: u32) -> bool {
    Snapshot::capture().holder(pid) == Holder::Closing
}

/// Keep every holder in the final decision. Only a retained closing PTY earns
/// bounded time to release it; an open terminal's background job is foreign.
pub(crate) fn await_holders(conversation: &str) -> Result<Vec<(u32, u32, String)>, String> {
    await_holders_observed(conversation, || {})
}

fn await_holders_observed(
    conversation: &str,
    observed: impl Fn(),
) -> Result<Vec<(u32, u32, String)>, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let ownership = Snapshot::capture();
        let output = std::process::Command::new("/bin/ps")
            .args(["-axww", "-o", "pid=,ppid=,args="])
            .output()
            .map_err(|_| "Process inspection could not start")?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err("Process inspection failed".into());
        }
        let ps = std::str::from_utf8(&output.stdout)
            .map_err(|_| "Process inspection output is invalid")?;
        if ps.trim().is_empty() {
            return Err("Process inspection output is empty".into());
        }
        let holders = guild_session::harness::holders_with_parents(ps, conversation);
        if !holders.is_empty() {
            observed();
        }
        if holders.is_empty()
            || holders.iter().any(|(pid, _, _)| {
                matches!(ownership.holder(*pid), Holder::Foreign | Holder::Uncertain)
            })
            || std::time::Instant::now() >= deadline
        {
            return Ok(holders);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use crate::terminal::emulator::{Emulator, GridSize};
    use portable_pty::CommandBuilder;
    use std::time::{Duration, Instant};

    #[test]
    fn open_plain_terminal_background_holder_is_not_closing_ownership() {
        let fixture = tempfile::tempdir().unwrap();
        let pidfile = fixture.path().join("pid");
        use std::os::unix::fs::PermissionsExt;
        let fake = fixture.path().join("claude");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\necho $$ > {}\n/bin/sleep 30\n",
                pidfile.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.args([
            "-fic",
            &format!(
                "{} --resume foreign-background-fixture --print fixture & wait",
                fake.display()
            ),
        ]);
        let mut terminal = Emulator::spawn(command, fixture.path(), GridSize::new(80, 24)).unwrap();
        let pid = ready_pid(&pidfile);
        let owned = closing_owns(pid);
        let start = Instant::now();
        let holders = await_holders("foreign-background-fixture").unwrap();
        terminal.hang_up_for_quit();
        assert!(
            !owned,
            "a descendant of an open plain terminal must block resume"
        );
        assert!(
            holders.iter().any(|(held, _, _)| *held == pid),
            "the other terminal holder must remain in the refusal decision: {holders:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "foreign holders must not earn cleanup grace"
        );
    }

    #[test]
    fn orphaned_holder_retains_only_active_cleanup_ownership() {
        let fixture = tempfile::tempdir().unwrap();
        let pidfile = fixture.path().join("pid");
        use std::os::unix::fs::PermissionsExt;
        let fake = fixture.path().join("codex");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\ntrap '' HUP\necho $$ > {}\nwhile :; do /bin/sleep 1; done\n",
                pidfile.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.args([
            "-fic",
            &format!("{} resume orphaned-owned-fixture; true", fake.display()),
        ]);
        let mut terminal = Emulator::spawn(command, fixture.path(), GridSize::new(80, 24)).unwrap();
        let pid = ready_pid(&pidfile);
        let over = terminal.let_go().unwrap();
        // The shell has already received HUP; wait for its nested child to be reparented.
        let until = Instant::now() + Duration::from_millis(500);
        loop {
            let out = std::process::Command::new("/bin/ps")
                .args(["-o", "ppid=", "-p", &pid.to_string()])
                .output()
                .unwrap();
            if String::from_utf8_lossy(&out.stdout).trim() == "1" {
                break;
            }
            assert!(Instant::now() < until, "fixture did not orphan the holder");
            std::thread::sleep(Duration::from_millis(10));
        }
        let owned = closing_owns(pid);
        let (sent, result) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            sent.send(await_holders("orphaned-owned-fixture").unwrap())
                .unwrap();
        });
        let premature = result.recv_timeout(Duration::from_millis(100));
        over.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(
            matches!(premature, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
            "resume must wait for the orphaned live harness: {premature:?}"
        );
        assert!(
            result
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .is_empty()
        );
        waiter.join().unwrap();
        assert!(owned, "cleanup must retain ownership after wrapper exit");
        assert!(
            !closing_owns(pid),
            "cleanup completion must revoke ownership"
        );
    }

    #[test]
    fn holder_exiting_after_ps_is_reobserved_before_resume() {
        let fixture = tempfile::tempdir().unwrap();
        let fake = fixture.path().join("codex");
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&fake, "#!/bin/sh\nwhile :; do /bin/sleep 1; done\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = std::process::Command::new(&fake);
        command.args(["resume", "snapshot-exit-fixture"]);
        let child = std::sync::Mutex::new(
            guild_session::process::ProcessGroup::spawn(&mut command).unwrap(),
        );
        let observed = std::sync::atomic::AtomicBool::new(false);
        let holders = await_holders_observed("snapshot-exit-fixture", || {
            child.lock().unwrap().stop().unwrap();
            observed.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .unwrap();
        assert!(
            observed.load(std::sync::atomic::Ordering::Relaxed),
            "fixture holder was never observed"
        );
        assert!(
            holders.is_empty(),
            "an exited holder must trigger a fresh snapshot, not a stale refusal: {holders:?}"
        );
    }

    #[test]
    fn completed_cleanup_invalidates_only_its_retained_snapshot() {
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        let mut child = guild_session::process::ProcessGroup::spawn(&mut command).unwrap();
        let pid = child.id();
        // A local witness, not registered globally and never used to signal.
        let owner = Arc::new(Cleanup {
            leader: Some(unsafe { libc::getsid(pid as i32) }),
            groups: vec![unsafe { libc::getpgid(pid as i32) }],
            active: std::sync::atomic::AtomicBool::new(true),
        });
        let snapshot = Snapshot(vec![owner.clone()]);
        assert_eq!(snapshot.holder(pid), Holder::Closing);
        owner.finish();
        assert_eq!(
            snapshot.holder(pid),
            Holder::Changed,
            "cleanup finishing after ps requires a fresh snapshot"
        );
        assert_eq!(
            Snapshot(vec![]).holder(pid),
            Holder::Foreign,
            "a fresh live foreign holder still blocks"
        );
        child.stop().unwrap();
        assert_eq!(snapshot.holder(pid), Holder::Changed);
        assert_eq!(snapshot.holder(0), Holder::Uncertain);
    }

    fn ready_pid(path: &std::path::Path) -> u32 {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(pid) = text.trim().parse()
            {
                return pid;
            }
            assert!(Instant::now() < until, "fixture did not become ready");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A read-only proof for a retained, unreaped PTY leader. Never signals a process.
pub(crate) fn quiescent_session(leader: u32) -> Result<(), String> {
    quiescent_session_observed(leader, || {})
}
pub(crate) fn quiescent_session_observed(
    leader: u32,
    mut observed: impl FnMut(),
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut previous = None;
    loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                leader as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
            || info.si_pid == 0
        {
            return Err("ownership_unqualified: leader exit is not retained".into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("ownership_unqualified: session snapshot did not stabilize".into());
        }
        let leader_birth = monotonic_birth(leader as i32);
        let Some(members) = session_snapshot(leader, leader_birth, deadline, &mut observed)? else {
            previous = None;
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        };
        if leader_birth.is_some() && monotonic_birth(leader as i32) != leader_birth {
            return Err(
                "ownership_unqualified: retained leader generation changed or unavailable".into(),
            );
        }
        // A process can fork after the PID list then exit before getsid. Never
        // skip disappeared PIDs as proof of emptiness. Two complete stable
        // generations, with every possible member non-live and older generations
        // proven foreign, close the fork/reap/reuse window while the owned leader
        // reserves the session ID. Include birth identities, not PID alone.
        if previous.as_ref() == Some(&members) {
            return Ok(());
        }
        previous = Some(members);
    }
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SessionGeneration {
    Exited(u32, u64, u64),
    OlderForeign(u32, u64),
}
type SessionMembers = Vec<SessionGeneration>;
fn session_snapshot(
    leader: u32,
    leader_birth: Option<u64>,
    deadline: std::time::Instant,
    observed: &mut impl FnMut(),
) -> Result<Option<SessionMembers>, String> {
    let mut pids = vec![0 as libc::pid_t; 65536];
    let capacity = std::mem::size_of_val(pids.as_slice());
    // PROC_ALL_PIDS. A full/partial buffer is uncertainty, never an empty session.
    let bytes = unsafe { libc::proc_listpids(1, 0, pids.as_mut_ptr().cast(), capacity as i32) };
    if bytes <= 0
        || bytes as usize >= capacity
        || !(bytes as usize).is_multiple_of(std::mem::size_of::<libc::pid_t>())
    {
        return Err("ownership_unqualified: incomplete session inspection".into());
    }
    observed();
    let mut members = Vec::new();
    for pid in pids
        .into_iter()
        .take(bytes as usize / std::mem::size_of::<libc::pid_t>())
        .filter(|p| *p > 0 && *p != leader as libc::pid_t)
    {
        if std::time::Instant::now() >= deadline {
            return Err("ownership_unqualified: session inspection deadline".into());
        }
        let sid = unsafe { libc::getsid(pid) };
        if sid < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err("ownership_unqualified: session identity unavailable".into());
        }
        if sid >= 0 && sid != leader as libc::pid_t {
            continue;
        }
        // Darwin may refuse getsid for a still-listed zombie. Retain its birth
        // identity in both passes: it cannot fork now, but could have forked
        // between enumeration and exit. A disappeared/changed PID retries.
        let mut member: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let n = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut member as *mut libc::proc_bsdinfo).cast(),
                std::mem::size_of_val(&member) as i32,
            )
        };
        if n == 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            match zombie_generation(pid)? {
                Some((seconds, microseconds)) => {
                    members.push(SessionGeneration::Exited(pid as u32, seconds, microseconds));
                    continue;
                }
                None => {
                    if let Some(birth) = older_foreign_birth(pid, leader_birth, monotonic_birth) {
                        members.push(SessionGeneration::OlderForeign(pid as u32, birth));
                        continue;
                    }
                    return Ok(None);
                }
            }
        }
        if n as usize != std::mem::size_of_val(&member) {
            return Err("ownership_unqualified: session member state unavailable".into());
        }
        if sid < 0 && member.pbi_status != libc::SZOMB {
            if let Some(birth) = older_foreign_birth(pid, leader_birth, monotonic_birth) {
                members.push(SessionGeneration::OlderForeign(pid as u32, birth));
                continue;
            }
            return Ok(None);
        }
        if member.pbi_status != libc::SZOMB {
            return Err(
                "cleanup_pending: retained PTY session still has live work; no signals sent".into(),
            );
        }
        members.push(SessionGeneration::Exited(
            pid as u32,
            member.pbi_start_tvsec,
            member.pbi_start_tvusec,
        ));
    }
    members.sort_unstable();
    Ok(Some(members))
}

// Public Darwin ABI already provided by libc. Exit ticks are deliberately not
// consumed: they precede task termination and do not attest creation quiescence.
pub(crate) fn monotonic_birth(pid: i32) -> Option<u64> {
    let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V0,
            (&mut info as *mut libc::rusage_info_v0).cast(),
        )
    };
    (result == 0 && info.ri_proc_start_abstime != 0).then_some(info.ri_proc_start_abstime)
}

fn older_foreign_birth(
    pid: i32,
    leader_birth: Option<u64>,
    mut read: impl FnMut(i32) -> Option<u64>,
) -> Option<u64> {
    let boundary = leader_birth.filter(|birth| *birth != 0)?;
    let birth = read(pid).filter(|birth| *birth != 0 && *birth < boundary)?;
    // Every member inherits this newly created SID from the retained leader.
    // setpgid cannot cross sessions; setsid creates only the caller's own SID.
    // Exec preserves monotonic birth, so a strictly older generation cannot be
    // a member, irrespective of whether it or an in-flight spawn can still run.
    // Re-read to refuse PID reuse/disappearance, then retain it in both snapshots.
    (read(pid) == Some(birth)).then_some(birth)
}

unsafe extern "C" {
    fn guild_process_generation(
        pid: i32,
        state: *mut i32,
        seconds: *mut i64,
        microseconds: *mut i64,
    ) -> i32;
}
fn zombie_generation(pid: i32) -> Result<Option<(u64, u64)>, String> {
    let (mut state, mut seconds, mut microseconds) = (0, 0, 0);
    // The tiny native shim compiles against the host SDK's public kinfo_proc.
    // It only observes status/birth, never attaches, signals or reaps a process.
    let error =
        unsafe { guild_process_generation(pid, &mut state, &mut seconds, &mut microseconds) };
    if error == libc::ESRCH {
        return Ok(None);
    }
    if error != 0 {
        return Err("ownership_unqualified: zombie generation unavailable".into());
    }
    if state != libc::SZOMB as i32 {
        return Ok(None);
    }
    if seconds < 0 || microseconds < 0 {
        return Err("ownership_unqualified: invalid process generation".into());
    }
    Ok(Some((seconds as u64, microseconds as u64)))
}

#[cfg(test)]
mod generation_tests {
    #[test]
    fn older_foreign_proof_requires_strict_stable_nonzero_monotonic_generations() {
        for (boundary, observations, expected) in [
            (Some(20), [Some(10), Some(10)], Some(10)),
            (Some(20), [Some(20), Some(20)], None),
            (Some(20), [Some(30), Some(30)], None),
            (Some(20), [Some(0), Some(0)], None),
            (Some(20), [None, None], None),
            (Some(20), [Some(10), None], None),
            (Some(20), [Some(10), Some(11)], None),
            (None, [Some(10), Some(10)], None),
            (Some(0), [Some(10), Some(10)], None),
        ] {
            let mut observations = observations.into_iter();
            assert_eq!(
                super::older_foreign_birth(1, boundary, |_| observations.next().unwrap()),
                expected
            );
        }
    }
}
