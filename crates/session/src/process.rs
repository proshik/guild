//! Ownership of a synchronous child and its process group.
//!
//! The leader stays unreaped until its group is stopped. Observing exit with
//! waitid/WNOWAIT preserves that identity; a reused PID is never signalled.
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus},
    sync::{Arc, Mutex},
};

/// A group started and reaped only by this owner. Dropping it stops the group.
pub struct ProcessGroup {
    child: Child,
    status: Option<ExitStatus>,
    /// Whether the leader was reaped, under the lock a [`Killer`] signals
    /// under: once it is, the leader's ID may name another group.
    reaped: Arc<Mutex<bool>>,
}

/// The right to `SIGKILL` one group from another thread while its owner
/// still has it — for a quit that cannot wait for the owner (GLD-063, code
/// review r2, bugs+impl-1). It signals only while the leader is unreaped,
/// so a reused ID is never signalled.
#[derive(Clone)]
pub struct Killer {
    leader: libc::pid_t,
    reaped: Arc<Mutex<bool>>,
}

impl Killer {
    /// [`Killer::kill`] unless the owner holds the lock: whether this group
    /// is done with — signalled now, or already reaped.
    pub fn try_kill(&self) -> bool {
        let reaped = match self.reaped.try_lock() {
            Ok(reaped) => reaped,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return false,
        };
        if !*reaped {
            // SAFETY: as in `kill`: unreaped while the lock says so.
            unsafe { libc::killpg(self.leader, libc::SIGKILL) };
        }
        true
    }

    /// `SIGKILL` the whole group, unless its owner has reaped it.
    pub fn kill(&self) {
        let reaped = self.reaped.lock().unwrap_or_else(|p| p.into_inner());
        if !*reaped {
            // SAFETY: the leader is unreaped while the lock says so, and
            // its ID still names our group.
            unsafe { libc::killpg(self.leader, libc::SIGKILL) };
        }
    }
}

/// `SIGKILL` every group, for a quit that cannot wait. A group whose
/// owner is reaping it right now — its lock held — is passed over and
/// visited again, so one slow reap never delays another group's signal
/// (code review r3, lenses-2); the owner holding the lock has already sent
/// its own `SIGKILL`. Gives up on a group still held after `within`.
pub fn kill_all(killers: &[Killer], within: std::time::Duration) {
    let until = std::time::Instant::now() + within;
    let mut pending: Vec<&Killer> = killers.iter().collect();
    loop {
        pending.retain(|killer| !killer.try_kill());
        if pending.is_empty() || std::time::Instant::now() >= until {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

impl ProcessGroup {
    /// Start a command in a new group, independent of Guild's process group.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        command.process_group(0);
        Ok(Self {
            child: command.spawn()?,
            status: None,
            reaped: Arc::new(Mutex::new(false)),
        })
    }
    /// A handle that can `SIGKILL` this group until it is reaped.
    pub fn killer(&self) -> Killer {
        Killer {
            leader: self.child.id() as libc::pid_t,
            reaped: self.reaped.clone(),
        }
    }
    /// The original leader's identity; not authority to signal it externally.
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    /// Transfer ownership of the input pipe, when one was requested.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }
    /// Transfer ownership of the output pipe, when one was requested.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }
    /// Transfer ownership of the diagnostic pipe, when one was requested.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// Observe exit without releasing the group ID, then stop descendants and reap.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        if self.has_exited()? {
            self.stop().map(Some)
        } else {
            Ok(None)
        }
    }

    fn has_exited(&self) -> io::Result<bool> {
        // SAFETY: a live owned child ID, valid zeroed output storage, and WNOWAIT
        // deliberately leaves the leader unreaped until stop signals its group.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        loop {
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(unsafe { info.si_pid() } != 0)
    }

    // Darwin's killpg filters zombies and returns EPERM if no signalable member
    // remains. Do not mistake a real permissions failure for that empty group:
    // accept it only after observing exit and finding exactly our own leader.
    #[cfg(target_os = "macos")]
    fn only_exited_leader(&self) -> io::Result<bool> {
        // XNU stops accepting signals before waitid exposes the final status.
        // Wait briefly for that transition; a still-running or foreign member
        // never turns a real permissions error into successful cleanup.
        let until = std::time::Instant::now() + std::time::Duration::from_millis(100);
        while !self.has_exited()? {
            if std::time::Instant::now() >= until {
                return Ok(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut members = [0 as libc::pid_t; 2];
        // PROC_PGRP_ONLY = 2 in libproc.h. Two slots distinguish one member from
        // two or more; an unreaped leader cannot acquire new children meanwhile.
        let bytes = unsafe {
            libc::proc_listpids(
                2,
                self.child.id(),
                members.as_mut_ptr().cast(),
                std::mem::size_of_val(&members) as i32,
            )
        };
        Ok(bytes == std::mem::size_of::<libc::pid_t>() as i32
            && members[0] == self.child.id() as libc::pid_t)
    }
    #[cfg(not(target_os = "macos"))]
    fn only_exited_leader(&self) -> io::Result<bool> {
        Ok(false)
    }

    /// Ask the group to end with `SIGTERM` — git removes its lock files on
    /// it — and stop it whole once the leader has exited or `grace` has
    /// passed.
    pub fn terminate(&mut self, grace: std::time::Duration) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        // SAFETY: as in `stop`: the unreaped leader's PID still names our group.
        unsafe { libc::killpg(self.child.id() as libc::pid_t, libc::SIGTERM) };
        let until = std::time::Instant::now() + grace;
        while std::time::Instant::now() < until && !self.has_exited()? {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        self.stop()
    }

    /// Stop the whole owned group and reap once. Subsequent calls return its status.
    pub fn stop(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        // Held through the reap, so a `Killer` never signals a reused ID.
        let reaped = self.reaped.clone();
        let mut reaped = reaped.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: nobody outside this owner can reap child; its PID still names
        // our group even if the leader exited before its descendants.
        if unsafe { libc::killpg(self.child.id() as libc::pid_t, libc::SIGKILL) } == -1 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH)
                && !(error.raw_os_error() == Some(libc::EPERM) && self.only_exited_leader()?)
            {
                return Err(error);
            }
        }
        let status = self.child.wait()?;
        *reaped = true;
        self.status = Some(status);
        Ok(status)
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            tracing::warn!(%error, "process group could not be stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Code review r3, lenses-2: one group's owner holding its reap lock —
    /// a slow `wait` — does not keep `SIGKILL` from another group.
    #[test]
    fn a_held_reap_does_not_delay_another_group_s_kill() {
        let mut a = ProcessGroup::spawn(Command::new("sleep").arg("30")).unwrap();
        let mut b = ProcessGroup::spawn(Command::new("sleep").arg("30")).unwrap();
        let killers = [a.killer(), b.killer()];
        let held = a.reaped.clone();
        let guard = held.lock().unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            kill_all(&killers, Duration::from_millis(100));
            let _ = done.send(());
        });
        let started = Instant::now();
        let mut b_dead = false;
        while started.elapsed() < Duration::from_secs(2) {
            if b.has_exited().unwrap() {
                b_dead = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let returned = finished.recv_timeout(Duration::from_secs(2)).is_ok();
        drop(guard);
        let _ = a.stop();
        let _ = b.stop();
        assert!(
            b_dead,
            "the other group was killed while the first reap was held"
        );
        assert!(returned, "the kill pass returned within its bound");
    }
}
