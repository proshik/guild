//! Running-binary helper admission; no app move is allowed until pipe arming and parent exit.
use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Instant,
};
const PENDING: u8 = 0;
const ARMED: u8 = 1;
const ABORTED: u8 = 2;
/// Small one-shot UI permission. The worker retains the actual helper process.
pub struct ArmToken {
    stream: Option<UnixStream>,
    state: Arc<AtomicU8>,
    nonce: Uuid,
    id: Uuid,
}
impl std::fmt::Debug for ArmToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArmToken")
            .field("transaction", &self.id)
            .finish_non_exhaustive()
    }
}
impl ArmToken {
    fn new(
        stream: UnixStream,
        state: Arc<AtomicU8>,
        nonce: Uuid,
        id: Uuid,
    ) -> Result<Self, Failure> {
        stream.set_nonblocking(true).map_err(|_| Failure::Launch)?;
        Ok(Self {
            stream: Some(stream),
            state,
            nonce,
            id,
        })
    }
    /// Transaction whose exact candidate the host must recheck before freezing Settings and quitting.
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// Final commit point. The host must quit immediately after success, with Settings frozen.
    pub fn arm(mut self) -> Result<(), Failure> {
        self.state
            .compare_exchange(PENDING, ARMED, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Failure::Cancelled)?;
        let mut stream = self.stream.take().ok_or(Failure::Cancelled)?;
        if stream.write_all(self.nonce.to_string().as_bytes()).is_err() {
            self.state.store(ABORTED, Ordering::Release);
            return Err(Failure::Launch);
        }
        // EOF is part of the packet. No duplicate writer exists in the worker.
        drop(stream);
        Ok(())
    }
}
impl Drop for ArmToken {
    fn drop(&mut self) {
        let _ = self
            .state
            .compare_exchange(PENDING, ABORTED, Ordering::AcqRel, Ordering::Acquire);
    }
}
fn admission(
    stream: &mut UnixStream,
    nonce: Uuid,
    budget: Duration,
    mut parent_alive: impl FnMut() -> bool,
) -> Result<(), Failure> {
    let deadline = Instant::now() + budget;
    let expected = nonce.to_string();
    let mut packet = Vec::with_capacity(36);
    let mut buffer = [0u8; 37];
    loop {
        if Instant::now() >= deadline {
            return Err(Failure::Cancelled);
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                packet.extend_from_slice(&buffer[..count]);
                if packet.len() > expected.len() {
                    return Err(Failure::Cancelled);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err(Failure::Cancelled),
        }
    }
    if packet != expected.as_bytes() {
        return Err(Failure::Cancelled);
    }
    // This is the original direct-child relationship, not kill(pid, 0), so a
    // reused PID cannot impersonate the old parent or become a signal target.
    while parent_alive() {
        if Instant::now() >= deadline {
            return Err(Failure::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

/// Background-owned running-binary helper. Drop aborts/reaps an unarmed child;
/// an armed child deliberately survives the parent to finish its journal.
pub struct Handoff {
    child: std::process::Child,
    state: Arc<AtomicU8>,
}
impl Handoff {
    /// Whether admission passed the irreversible arm commit point.
    pub fn armed(&self) -> bool {
        self.state.load(Ordering::Acquire) == ARMED
    }
    /// Revoke and reap off-thread. Conflict means arming already won, so the
    /// caller must preserve the journal for its detached helper, never abort it.
    pub fn cancel(&mut self) -> Result<(), Failure> {
        match self
            .state
            .compare_exchange(PENDING, ABORTED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) | Err(ABORTED) => {
                let _ = self.child.kill();
                self.child.wait().map_err(|_| Failure::Launch)?;
                Ok(())
            }
            Err(_) => Err(Failure::Conflict),
        }
    }
}
impl Drop for Handoff {
    fn drop(&mut self) {
        let _ = self.cancel();
        // The armed helper is a detached transaction owner. Parent exit reparents
        // it for OS reaping; joining here would deadlock its parent-exit admission.
    }
}

impl Store {
    /// Start the exact running executable off-thread and wait for its bounded
    /// readiness handshake. No payload executable/script is started.
    pub fn spawn_helper(
        &self,
        id: Uuid,
        cancel: &AtomicBool,
    ) -> Result<(Handoff, ArmToken), Failure> {
        use std::{
            os::{fd::OwnedFd, unix::process::CommandExt},
            process::{Command, Stdio},
        };
        let receipt = self.load(id)?;
        if receipt.phase != Phase::Prepared
            || receipt.parent_pid != std::process::id()
            || self
                .pending(&receipt.destination)?
                .is_none_or(|active| active.id != id)
        {
            return Err(Failure::Conflict);
        }
        let executable = std::env::current_exe()
            .map_err(|_| Failure::Launch)?
            .canonicalize()
            .map_err(|_| Failure::Launch)?;
        if executable != receipt.destination.join("Contents/MacOS/guild-desktop")
            || Native.proof(&receipt.destination, cancel)? != receipt.previous
        {
            return Err(Failure::Changed);
        }
        let (mut parent, child) = UnixStream::pair().map_err(|_| Failure::Launch)?;
        parent
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|_| Failure::Launch)?;
        parent
            .set_write_timeout(Some(Duration::from_millis(100)))
            .map_err(|_| Failure::Launch)?;
        let mut command = Command::new(executable);
        command
            .arg("--guild-update-helper")
            .arg(&self.state)
            .arg(id.to_string())
            .stdin(Stdio::from(OwnedFd::from(child)))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: only async-signal-safe setsid is called between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let state = Arc::new(AtomicU8::new(PENDING));
        let handoff = Handoff {
            child: command.spawn().map_err(|_| Failure::Launch)?,
            state: state.clone(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ready = [0u8; 1];
        loop {
            if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err(Failure::Cancelled);
            }
            match parent.read(&mut ready) {
                Ok(1) if ready == *b"R" => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                _ => return Err(Failure::Launch),
            }
        }
        Ok((
            handoff,
            ArmToken::new(parent, state, receipt.arm_nonce, id)?,
        ))
    }
}
/// Private executable entry, before GPUI/logging initialization. Standard input
/// must be the parent-owned Unix socket; no command-line token is accepted.
pub fn run_helper(state: PathBuf, id: Uuid) -> Result<(), Failure> {
    use std::os::fd::AsFd;
    let store = Store::new(state);
    let receipt = store.load(id)?;
    // SAFETY: getppid has no preconditions; the direct parent is checked before readiness.
    let original_parent = unsafe { libc::getppid() } as u32;
    if original_parent != receipt.parent_pid
        || receipt.phase != Phase::Prepared
        || store
            .pending(&receipt.destination)?
            .is_none_or(|active| active.id != id)
    {
        return Err(Failure::Cancelled);
    }
    let executable = std::env::current_exe()
        .map_err(|_| Failure::Launch)?
        .canonicalize()
        .map_err(|_| Failure::Launch)?;
    if executable != receipt.destination.join("Contents/MacOS/guild-desktop") {
        return Err(Failure::Changed);
    }
    let fd = std::io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .map_err(|_| Failure::Launch)?;
    let mut stream = UnixStream::from(fd);
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|_| Failure::Launch)?;
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| Failure::Launch)?;
    stream.write_all(b"R").map_err(|_| Failure::Launch)?;
    admission(
        &mut stream,
        receipt.arm_nonce,
        Duration::from_secs(60),
        || {
            // SAFETY: querying our direct parent does not signal any process.
            (unsafe { libc::getppid() }) as u32 == original_parent
        },
    )?;
    store.execute(id, &AtomicBool::new(false)).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (ArmToken, UnixStream, Arc<AtomicU8>) {
        let (parent, child) = UnixStream::pair().unwrap();
        child
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        let state = Arc::new(AtomicU8::new(PENDING));
        (
            ArmToken {
                stream: Some(parent),
                state: state.clone(),
                nonce: Uuid::new_v4(),
                id: Uuid::new_v4(),
            },
            child,
            state,
        )
    }
    #[test]
    fn final_ui_arming_never_waits_for_a_full_helper_socket() {
        let (mut writer, receiver) = UnixStream::pair().unwrap();
        writer.set_nonblocking(true).unwrap();
        let bytes = [0u8; 65536];
        loop {
            match writer.write(&bytes) {
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        writer.set_nonblocking(false).unwrap();
        let token = ArmToken::new(
            writer,
            Arc::new(AtomicU8::new(PENDING)),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = send.send(token.arm());
        });
        let result = receive.recv_timeout(Duration::from_millis(500));
        // Release the peer even on failure, so a broken implementation cannot
        // strand the regression's own thread after its deadlock watchdog fires.
        drop(receiver);
        worker.join().unwrap();
        assert!(
            matches!(result, Ok(Err(Failure::Launch))),
            "UI arming blocked: {result:?}"
        );
    }
    #[test]
    fn nonce_and_eof_alone_do_not_authorize_replacement_before_parent_exit() {
        let (token, mut stream, _) = pair();
        let nonce = token.nonce;
        token.arm().unwrap();
        assert_eq!(
            admission(&mut stream, nonce, Duration::from_millis(15), || true),
            Err(Failure::Cancelled)
        );
        let (token, mut stream, _) = pair();
        let nonce = token.nonce;
        token.arm().unwrap();
        let mut observations = 0;
        assert_eq!(
            admission(&mut stream, nonce, Duration::from_millis(50), || {
                observations += 1;
                observations < 3
            }),
            Ok(())
        );
        assert!(observations >= 3);
    }
    #[test]
    fn unarmed_revoked_wrong_nonce_and_extra_bytes_never_commit() {
        let (token, mut stream, _) = pair();
        let nonce = token.nonce;
        drop(token);
        assert_eq!(
            admission(&mut stream, nonce, Duration::from_millis(20), || false),
            Err(Failure::Cancelled)
        );
        let (token, mut stream, state) = pair();
        state.store(ABORTED, Ordering::Release);
        assert_eq!(token.arm(), Err(Failure::Cancelled));
        assert_eq!(
            admission(&mut stream, nonce, Duration::from_millis(20), || false),
            Err(Failure::Cancelled)
        );
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        writer.write_all(nonce.to_string().as_bytes()).unwrap();
        assert_eq!(
            admission(&mut reader, nonce, Duration::from_millis(15), || false),
            Err(Failure::Cancelled)
        );
        drop(writer);
        for bytes in [Uuid::new_v4().to_string(), format!("{nonce}extra")] {
            let (mut parent, mut child) = UnixStream::pair().unwrap();
            child
                .set_read_timeout(Some(Duration::from_millis(5)))
                .unwrap();
            parent.write_all(bytes.as_bytes()).unwrap();
            drop(parent);
            assert_eq!(
                admission(&mut child, nonce, Duration::from_millis(20), || false),
                Err(Failure::Cancelled)
            );
        }
    }
    #[test]
    fn a_real_detached_child_waits_for_its_original_parent_to_exit() {
        use std::{
            os::fd::OwnedFd,
            process::{Command, Stdio},
        };
        let root = tempfile::tempdir().unwrap();
        let (mut stream, input) = UnixStream::pair().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let nonce = Uuid::new_v4();
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "update::transaction::handoff::tests::process_fixture",
            ])
            .env("GUILD_UPDATE_HANDOFF_FIXTURE_ROLE", "parent")
            .env("GUILD_UPDATE_HANDOFF_FIXTURE_DIR", root.path())
            .env("GUILD_UPDATE_HANDOFF_FIXTURE_NONCE", nonce.to_string())
            .stdin(Stdio::from(OwnedFd::from(input)))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut ready = [0u8; 1];
        stream.read_exact(&mut ready).unwrap();
        assert_eq!(&ready, b"R");
        stream.write_all(nonce.to_string().as_bytes()).unwrap();
        drop(stream);
        std::thread::sleep(Duration::from_millis(25));
        assert!(!root.path().join("admitted").exists());
        assert!(parent.try_wait().unwrap().is_none());
        std::fs::write(root.path().join("exit-parent"), b"exit").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = parent.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() >= deadline {
                let _ = parent.kill();
                let _ = parent.wait();
                panic!("fixture parent did not exit");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        while !root.path().join("admitted").exists() {
            assert!(
                Instant::now() < deadline,
                "detached helper never observed parent exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn retiring_an_unarmed_worker_reaps_its_child_and_revokes_a_late_ui_token() {
        use std::process::{Command, Stdio};
        let (token, _stream, state) = pair();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "update::transaction::handoff::tests::process_fixture",
            ])
            .env("GUILD_UPDATE_HANDOFF_FIXTURE_ROLE", "park")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        drop(Handoff { child, state });
        assert_eq!(token.arm(), Err(Failure::Cancelled));
        let mut status = 0;
        // SAFETY: waitpid only observes an original owned child; it never signals a PID.
        assert_eq!(
            unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    #[test]
    #[ignore = "subprocess entry used by the bounded parent-exit regression"]
    fn process_fixture() {
        use std::{
            os::{
                fd::{AsFd, OwnedFd},
                unix::process::CommandExt,
            },
            process::{Command, Stdio},
        };
        let role = std::env::var("GUILD_UPDATE_HANDOFF_FIXTURE_ROLE").unwrap();
        let directory =
            PathBuf::from(std::env::var_os("GUILD_UPDATE_HANDOFF_FIXTURE_DIR").unwrap());
        if role == "parent" {
            let input = std::io::stdin().as_fd().try_clone_to_owned().unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--ignored",
                    "--exact",
                    "update::transaction::handoff::tests::process_fixture",
                ])
                .env("GUILD_UPDATE_HANDOFF_FIXTURE_ROLE", "child")
                .stdin(Stdio::from(input))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // SAFETY: setsid is async-signal-safe, as in the production helper.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            // Joining would deadlock: this fixture deliberately models a helper
            // that must outlive its direct parent, then be reaped by the OS.
            #[expect(
                clippy::zombie_processes,
                reason = "the fixture must exit before its detached child can finish"
            )]
            let _detached = command.spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !directory.join("exit-parent").exists() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        } else {
            let nonce: Uuid = std::env::var("GUILD_UPDATE_HANDOFF_FIXTURE_NONCE")
                .unwrap()
                .parse()
                .unwrap();
            let fd: OwnedFd = std::io::stdin().as_fd().try_clone_to_owned().unwrap();
            let mut stream = UnixStream::from(fd);
            stream
                .set_read_timeout(Some(Duration::from_millis(5)))
                .unwrap();
            // SAFETY: querying the original direct parent, never signalling a PID.
            let parent = unsafe { libc::getppid() };
            stream.write_all(b"R").unwrap();
            admission(&mut stream, nonce, Duration::from_secs(2), || {
                // SAFETY: getppid has no preconditions.
                unsafe { libc::getppid() == parent }
            })
            .unwrap();
            std::fs::write(directory.join("admitted"), b"parent exited").unwrap();
        }
    }
}
