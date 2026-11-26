//! Persistent-inode advisory locks shared by staging and preference writers.
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};
/// Exclusive lock on one owned inode. Dropping releases it; it is never unlinked.
#[derive(Debug)]
pub struct FileLock(File);
impl FileLock {
    /// Lock a canonical directory's named inode with a bounded wait.
    pub fn acquire(directory: &Path, name: &str, timeout: Duration) -> io::Result<Self> {
        if name.contains('/') || name == "." || name == ".." {
            return Err(io::Error::other("invalid lock name"));
        }
        std::fs::create_dir_all(directory)?;
        let path = directory.canonicalize()?.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let meta = file.metadata()?;
        // SAFETY: geteuid has no preconditions and does not return a pointer.
        let uid = unsafe { libc::geteuid() };
        if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
            return Err(io::Error::other("lock is not a private owned regular file"));
        }
        let deadline = Instant::now() + timeout;
        loop {
            // SAFETY: file owns a valid fd; flock changes only this open file's lock.
            let result = unsafe {
                libc::flock(
                    std::os::fd::AsRawFd::as_raw_fd(&file),
                    libc::LOCK_EX | libc::LOCK_NB,
                )
            };
            if result == 0 {
                return Ok(Self(file));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EWOULDBLOCK)
                && error.kind() != io::ErrorKind::Interrupted
            {
                return Err(error);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "another update writer owns this directory",
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        // SAFETY: the fd remains owned until after this destructor; no path is deleted.
        unsafe {
            libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.0), libc::LOCK_UN);
        }
    }
}
