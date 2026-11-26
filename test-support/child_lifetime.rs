//! Observe fixture termination through EOF, not OS-specific zombie reaping.
use std::{
    ffi::CString,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

pub struct Descendant {
    fifo: PathBuf,
    ready: PathBuf,
    pid: PathBuf,
    closed: mpsc::Receiver<()>,
    stopped: bool,
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\"'\"'"))
}
impl Descendant {
    pub fn new(root: &Path) -> Self {
        let fifo = root.join("lifetime.fifo");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (send, closed) = mpsc::channel();
        let reader = fifo.clone();
        std::thread::spawn(move || {
            let mut pipe = std::fs::File::open(reader).unwrap();
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"ready");
            let _ = send.send(());
        });
        Self {
            fifo,
            ready: root.join("ready"),
            pid: root.join("child.pid"),
            closed,
            stopped: false,
        }
    }
    /// The child opens its lifetime pipe before the leader can report success.
    pub fn shell(&self) -> String {
        format!(
            "(printf ready >&3; : > {}; exec /bin/sleep 60) 3>{} </dev/null >/dev/null 2>&1 &\necho $! > {}\nwhile [ ! -f {} ]; do /bin/sleep 0.01; done\n",
            quote(&self.ready),
            quote(&self.fifo),
            quote(&self.pid),
            quote(&self.ready)
        )
    }
    pub fn assert_stopped(&mut self) {
        // A watchdog for a broken lifecycle, not a performance threshold.
        self.closed
            .recv_timeout(Duration::from_secs(10))
            .expect("the descendant must close its lifetime pipe after owner cleanup");
        self.stopped = true;
    }
}
impl Drop for Descendant {
    fn drop(&mut self) {
        if self.stopped || self.closed.try_recv().is_ok() {
            return;
        }
        if let Ok(pid) = std::fs::read_to_string(&self.pid) {
            // Failure cleanup only: this fixture has not closed its lifetime pipe.
            unsafe {
                libc::kill(pid.trim().parse().unwrap(), libc::SIGKILL);
            }
        }
    }
}
