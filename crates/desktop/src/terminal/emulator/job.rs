//! A terminal job keeps its session leader unreaped until descendant cleanup.
use super::{Child, ExitStatus, ended_unreaped};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

type Completion = Option<Result<ExitStatus, String>>;

pub(super) struct Job {
    pid: u32,
    cancel: mpsc::SyncSender<()>,
    result: Arc<Mutex<Completion>>,
}

/// Only this object can reap the child. Drop also covers thread creation failure.
struct Owner {
    child: Box<dyn Child + Send + Sync>,
    pid: libc::pid_t,
    reaped: bool,
    master_anchor: Option<std::fs::File>,
}
impl Owner {
    fn signal(&self, signal: i32) -> Result<(), String> {
        let mut errors = Vec::new();
        for group in session_groups(self.pid) {
            // SAFETY: the leader is our unreaped child. Additional groups are
            // discovered in that session immediately before signalling.
            if unsafe { libc::killpg(group, signal) } == -1 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH)
                    && !(error.raw_os_error() == Some(libc::EPERM)
                        && group == self.pid
                        && self.only_exited_leader())
                {
                    errors.push(format!("signal {signal} to group {group}: {error}"));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
    fn only_exited_leader(&self) -> bool {
        let until = Instant::now() + Duration::from_millis(100);
        while !ended_unreaped(self.pid) {
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut members = [0 as libc::pid_t; 2];
        // SAFETY: two slots distinguish our zombie from a group with any other
        // member. Darwin excludes zombies from killpg's signalable members.
        let bytes = unsafe {
            libc::proc_listpids(
                2,
                self.pid as u32,
                members.as_mut_ptr().cast(),
                size_of_val(&members) as i32,
            )
        };
        bytes == size_of::<libc::pid_t>() as i32 && members[0] == self.pid
    }
    fn finish(&mut self) -> Result<ExitStatus, String> {
        let cleanup = self.signal(libc::SIGKILL);
        self.master_anchor.take();
        // A denied signal must not turn shutdown into an unbounded wait. If
        // the leader ended, still reap it even when descendant cleanup failed.
        if let Err(error) = &cleanup
            && !ended_unreaped(self.pid)
        {
            return Err(error.clone());
        }
        let result = self.child.wait().map_err(|e| e.to_string());
        if result.is_ok() {
            self.reaped = true;
        }
        cleanup.and(result)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.reaped
            && let Err(error) = self.finish()
        {
            tracing::warn!(%error, "terminal job cleanup failed");
        }
    }
}

// A session's controlling terminal is detached when its leader exits. Querying
// the session IDs also finds background groups after that detach.
fn session_groups(leader: libc::pid_t) -> Vec<libc::pid_t> {
    let mut groups = vec![leader];
    // SAFETY: proc_listpids first measures and then writes only into our buffer.
    let bytes = unsafe { libc::proc_listpids(1, 0, std::ptr::null_mut(), 0) };
    if bytes <= 0 {
        return groups;
    }
    let mut pids = vec![0 as libc::pid_t; bytes as usize / size_of::<libc::pid_t>() + 128];
    let bytes = unsafe {
        libc::proc_listpids(
            1,
            0,
            pids.as_mut_ptr().cast(),
            size_of_val(pids.as_slice()) as i32,
        )
    };
    if bytes > 0 {
        for pid in pids
            .into_iter()
            .take(bytes as usize / size_of::<libc::pid_t>())
        {
            if pid > 0 && unsafe { libc::getsid(pid) } == leader {
                let group = unsafe { libc::getpgid(pid) };
                if group > 0 {
                    groups.push(group);
                }
            }
        }
    }
    groups.sort_unstable();
    groups.dedup();
    groups
}

impl Job {
    pub(super) fn start(
        child: Box<dyn Child + Send + Sync>,
        signal: Arc<tokio::sync::Notify>,
        master_anchor: std::fs::File,
    ) -> std::io::Result<(Self, std::thread::JoinHandle<()>)> {
        let pid = child
            .process_id()
            .ok_or_else(|| std::io::Error::other("terminal child has no PID"))?;
        let mut owner = Owner {
            child,
            pid: pid as libc::pid_t,
            reaped: false,
            master_anchor: Some(master_anchor),
        };
        let (cancel, requests) = mpsc::sync_channel(1);
        let result = Arc::new(Mutex::new(None));
        let completed = result.clone();
        let worker = std::thread::Builder::new()
            .name("terminal-job".into())
            .spawn(move || {
                let cancelled = loop {
                    if ended_unreaped(owner.pid) {
                        break false;
                    }
                    match requests.recv_timeout(Duration::from_millis(20)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break true,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                };
                let mut graceful = Ok(());
                if cancelled {
                    graceful = owner.signal(libc::SIGHUP);
                    let until = Instant::now() + Duration::from_secs(1);
                    while !ended_unreaped(owner.pid) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                let status = owner.finish().and_then(|status| graceful.map(|()| status));
                if let Ok(mut slot) = completed.lock() {
                    *slot = Some(status);
                }
                signal.notify_one();
            })?;
        Ok((
            Self {
                pid,
                cancel,
                result,
            },
            worker,
        ))
    }
    // Keep the real cleanup worker, but let tests order the UI publication
    // independently of its reader event. The worker retains its original slot.
    #[cfg(test)]
    pub(super) fn hold_publication(&mut self) -> Arc<Mutex<Completion>> {
        self.result = Arc::new(Mutex::new(None));
        self.result.clone()
    }
    pub(super) fn completion_handle(&self) -> Arc<Mutex<Completion>> {
        self.result.clone()
    }
    pub(super) fn completion(&self) -> Completion {
        self.result.lock().ok().and_then(|r| r.clone())
    }
    pub(super) fn pid(&self) -> Option<u32> {
        self.completion().is_none().then_some(self.pid)
    }
    pub(super) fn cancel(&self) {
        let _ = self.cancel.try_send(());
    }
    pub(super) fn shutdown(&mut self) {
        self.cancel();
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_job_signal_failure_is_reported_without_reaping_the_child() {
        let pty = portable_pty::native_pty_system()
            .openpty(portable_pty::PtySize::default())
            .unwrap();
        let mut command = portable_pty::CommandBuilder::new("/bin/sleep");
        command.arg("20");
        let child = pty.slave.spawn_command(command).unwrap();
        let mut owner = Owner {
            pid: child.process_id().unwrap() as libc::pid_t,
            child,
            reaped: false,
            master_anchor: None,
        };
        // A real kernel failure, without another user's process or privileges.
        let failed = owner.signal(i32::MAX);
        assert!(owner.child.try_wait().unwrap().is_none());
        owner.finish().unwrap();
        assert!(
            failed.is_err(),
            "a rejected signal was reported as successful"
        );
    }
}
