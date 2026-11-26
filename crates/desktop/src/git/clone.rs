//! A clone owns a process group and private same-filesystem staging directory.
//! No operation writes through the requested destination before publication.
use guild_session::process::ProcessGroup;
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const DIAGNOSTIC_LIMIT: usize = 64 * 1024;
const INACTIVITY: Duration = Duration::from_secs(120);

/// A submitted request retains the parent identity across worker scheduling.
pub struct Request {
    url: String,
    destination: PathBuf,
    parent: File,
    parent_path: PathBuf,
    name: CString,
}
impl Request {
    /// Validate and anchor the existing parent; never create missing ancestors.
    pub fn new(url: &str, destination: &Path) -> Result<Self, String> {
        crate::clone_project::repository_name(url)?;
        crate::clone_project::validate_destination(destination)?;
        let parent_path = destination.parent().ok_or("Missing parent")?.to_path_buf();
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(&parent_path)
            .map_err(|e| e.to_string())?;
        let name = CString::new(
            destination
                .file_name()
                .ok_or("Missing directory name")?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        let request = Self {
            url: url.trim().into(),
            destination: destination.into(),
            parent,
            parent_path,
            name,
        };
        request.check_parent().map_err(|e| e.to_string())?;
        Ok(request)
    }
    fn check_parent(&self) -> io::Result<()> {
        let actual = std::fs::metadata(&self.parent_path)?;
        let held = self.parent.metadata()?;
        if actual.dev() != held.dev() || actual.ino() != held.ino() {
            return Err(io::Error::other(
                "The parent directory changed; choose the destination again.",
            ));
        }
        Ok(())
    }
}

/// Terminal outcomes are produced only after the group and readers have stopped.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Published without replacing an existing entry.
    Succeeded(PathBuf),
    /// Failed; a retained private directory contains an available clone or partial data.
    Failed {
        message: String,
        retained: Option<PathBuf>,
    },
    /// Explicit cancellation, with any area whose cleanup could not be proved safe.
    Cancelled { retained: Option<PathBuf> },
}
#[derive(Default)]
struct Progress {
    proxy: crate::network::GitPolicy,
    text: String,
    last_output: Option<Instant>,
}
impl Progress {
    fn line(&mut self, bytes: &[u8]) {
        let line = redact(&self.proxy.redact(&String::from_utf8_lossy(bytes)));
        if line.trim().is_empty() {
            return;
        }
        self.text.push_str(&line);
        self.text.push('\n');
        if self.text.len() > DIAGNOSTIC_LIMIT {
            let mut start = self.text.len() - DIAGNOSTIC_LIMIT;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text.drain(..start);
        }
    }
}
/// Redact credential-bearing URL tokens before they enter retained diagnostics.
fn redact(line: &str) -> String {
    let mut hidden_value = false;
    line.split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let secret = hidden_value
                || (word.contains("://") && (word.contains('@') || word.contains('?')))
                || ["password=", "token=", "authorization:", "bearer"]
                    .iter()
                    .any(|part| lower.contains(part));
            hidden_value |= matches!(
                lower.as_str(),
                "password:" | "token:" | "authorization:" | "bearer"
            );
            if secret { "[redacted]" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Window-owned worker. Drop joins cleanup, including normal application shutdown.
pub struct Worker {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    thread: Option<JoinHandle<Outcome>>,
}
impl Worker {
    /// Start one validated clone on a background thread.
    pub fn start(request: Request) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress::default()));
        let stop = cancel.clone();
        let output = progress.clone();
        let thread = thread::spawn(move || {
            let mut command = super::git_command();
            command.args(
                super::Write::Clone {
                    url: request.url.clone(),
                    destination: PathBuf::from("repo"),
                }
                .args(),
            );
            execute(request, command, INACTIVITY, stop, output)
        });
        Self {
            cancel,
            progress,
            thread: Some(thread),
        }
    }
    /// Request cancellation; completion is separately observed after cleanup.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    /// A bounded sanitized diagnostic snapshot, polled by the window.
    pub fn progress(&self) -> String {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .text
            .clone()
    }
    /// Consume a terminal outcome exactly once, without blocking the window.
    pub fn finish(&mut self) -> Option<Outcome> {
        if !self.thread.as_ref()?.is_finished() {
            return None;
        }
        Some(
            self.thread
                .take()?
                .join()
                .unwrap_or_else(|_| Outcome::Failed {
                    message: "The clone worker stopped unexpectedly.".into(),
                    retained: None,
                }),
        )
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Staging {
    directory: File,
    name: CString,
    path: PathBuf,
}
impl Staging {
    fn new(request: &Request) -> io::Result<Self> {
        request.check_parent()?;
        // Recheck relative to the retained parent, including dangling links.
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let found = unsafe {
            libc::fstatat(
                request.parent.as_raw_fd(),
                request.name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if found == 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "The destination already exists.",
            ));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error);
        }
        let name = CString::new(format!(".guild-clone-{}", uuid::Uuid::new_v4()))
            .expect("UUID has no NUL");
        if unsafe { libc::mkdirat(request.parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let directory = open_directory(&request.parent, &name)?;
        let path = request
            .parent_path
            .join(std::ffi::OsStr::from_bytes(name.as_bytes()));
        Ok(Self {
            directory,
            name,
            path,
        })
    }
    fn publish(&self, request: &Request) -> io::Result<File> {
        request.check_parent()?;
        // Open without following a substituted repository symlink.
        let repository = open_directory(&self.directory, c"repo")?;
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                self.directory.as_raw_fd(),
                c"repo".as_ptr(),
                request.parent.as_raw_fd(),
                request.name.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                self.directory.as_raw_fd(),
                c"repo".as_ptr(),
                request.parent.as_raw_fd(),
                request.name.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Atomic no-replace publication is unavailable.",
        ));
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(repository)
    }
    fn current_path(&self) -> PathBuf {
        descriptor_path(&self.directory).unwrap_or_else(|| self.path.clone())
    }
    fn clean(&self, request: &Request) -> io::Result<()> {
        // Delete through descriptors, never by re-following a destination path.
        let named = open_directory(&request.parent, &self.name)?;
        let a = named.metadata()?;
        let b = self.directory.metadata()?;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(io::Error::other(
                "Staging directory changed; retained for inspection.",
            ));
        }
        clean_contents(&self.directory)?;
        if unsafe {
            libc::unlinkat(
                request.parent.as_raw_fd(),
                self.name.as_ptr(),
                libc::AT_REMOVEDIR,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
fn descriptor_path(directory: &File) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut path = [0 as libc::c_char; libc::PATH_MAX as usize];
        if unsafe { libc::fcntl(directory.as_raw_fd(), libc::F_GETPATH, path.as_mut_ptr()) } != 0 {
            return None;
        }
        let path = unsafe { CStr::from_ptr(path.as_ptr()) };
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())))
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_link(format!("/proc/self/fd/{}", directory.as_raw_fd())).ok()
    }
}
fn open_directory(parent: &File, name: &CStr) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn clean_contents(directory: &File) -> io::Result<()> {
    // fdopendir owns its fd, so duplicate it rather than relinquishing our anchor.
    let fd = unsafe { libc::dup(directory.as_raw_fd()) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        unsafe {
            libc::close(fd);
        }
        return Err(io::Error::last_os_error());
    }
    struct Entries(*mut libc::DIR);
    impl Drop for Entries {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    let entries = Entries(stream);
    loop {
        let entry = unsafe { libc::readdir(entries.0) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let flags = match open_directory(directory, name) {
            Ok(child) => {
                clean_contents(&child)?;
                libc::AT_REMOVEDIR
            }
            Err(error) if matches!(error.raw_os_error(), Some(libc::ENOTDIR | libc::ELOOP)) => 0,
            Err(error) => return Err(error),
        };
        if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
fn drain(mut pipe: impl Read, progress: Arc<Mutex<Progress>>) {
    let mut bytes = [0; 4096];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let n = match pipe.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let mut progress = progress.lock().unwrap_or_else(|e| e.into_inner());
        progress.last_output = Some(Instant::now());
        for &byte in &bytes[..n] {
            if matches!(byte, b'\r' | b'\n') {
                if oversized {
                    progress.line(b"[long diagnostic line omitted]");
                } else {
                    progress.line(&line);
                }
                line.clear();
                oversized = false;
            } else if line.len() < 8192 {
                line.push(byte);
            } else {
                oversized = true;
            }
        }
    }
    let mut progress = progress.lock().unwrap_or_else(|e| e.into_inner());
    if !oversized {
        progress.line(&line);
    }
}
// An explicit SSH wrapper remains the user's command. Prompt controls and the
// watchdog still apply; only default OpenSSH receives its specific switches.
pub(super) fn configure_ssh(
    command: &mut Command,
    fd: i32,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<(), String> {
    let value = |key: &str| {
        command
            .get_envs()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
            .unwrap_or_else(|| std::env::var_os(key))
    };
    if value("GIT_SSH_COMMAND").is_some() || value("GIT_SSH").is_some() {
        return Ok(());
    }
    let mut query = super::git_command();
    query
        .args(["config", "--get", "core.sshCommand"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => {
                query.env(key, value);
            }
            None => {
                query.env_remove(key);
            }
        }
    }
    unsafe {
        query.pre_exec(move || {
            if libc::fchdir(fd) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let mut group = ProcessGroup::spawn(&mut query).map_err(|e| e.to_string())?;
    let stdout = group.take_stdout().expect("piped config output");
    let started = Instant::now();
    let bytes = thread::scope(|scope| -> Result<Vec<u8>, String> {
        let reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(16 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        loop {
            if cancel.load(Ordering::Acquire) {
                group.stop().map_err(|e| e.to_string())?;
                break;
            }
            if group.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if started.elapsed() >= timeout {
                group.stop().map_err(|e| e.to_string())?;
                return Err("No output while reading Git SSH configuration; retry.".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        group.stop().map_err(|e| e.to_string())?;
        reader
            .join()
            .map_err(|_| "SSH configuration reader stopped".to_owned())?
            .map_err(|e| e.to_string())
    })?;
    if bytes.len() > 16 * 1024 {
        return Err("Git SSH configuration is too large.".into());
    }
    let configured = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let configured = configured.trim();
    command.env(
        "GIT_SSH_COMMAND",
        if configured.is_empty() {
            "ssh -oBatchMode=yes -oStrictHostKeyChecking=yes"
        } else {
            configured
        },
    );
    Ok(())
}
pub(super) fn noninteractive(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/usr/bin/false")
        .env("SSH_ASKPASS", "/usr/bin/false")
        .env("SSH_ASKPASS_REQUIRE", "force");
}
fn execute(
    request: Request,
    mut command: Command,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
) -> Outcome {
    let applied = match crate::network::git(
        &mut command,
        &request.parent_path,
        &request.url,
        false,
        false,
    ) {
        Ok(applied) => applied,
        Err(error) => {
            return Outcome::Failed {
                message: error.to_string(),
                retained: None,
            };
        }
    };
    progress.lock().unwrap_or_else(|e| e.into_inner()).proxy = applied;
    let staging = match Staging::new(&request) {
        Ok(staging) => staging,
        Err(e) => {
            return Outcome::Failed {
                message: e.to_string(),
                retained: None,
            };
        }
    };
    noninteractive(&mut command);
    let fd = staging.directory.as_raw_fd();
    // The held fd remains alive through spawn. Only async-signal-safe fchdir
    // runs after fork; the child's cwd remains anchored if its path is moved.
    unsafe {
        command.pre_exec(move || {
            if libc::fchdir(fd) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let result = (|| -> Result<bool, String> {
        if cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        request.check_parent().map_err(|e| e.to_string())?;
        configure_ssh(&mut command, fd, &cancel, timeout)?;
        if cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        let mut group = ProcessGroup::spawn(&mut command).map_err(|e| e.to_string())?;
        let out = group.take_stdout().expect("piped stdout");
        let err = group.take_stderr().expect("piped stderr");
        let started = Instant::now();
        thread::scope(|scope| {
            scope.spawn(|| drain(out, progress.clone()));
            scope.spawn(|| drain(err, progress.clone()));
            let result = loop {
                if cancel.load(Ordering::Acquire) {
                    break Ok(false);
                }
                match group.try_wait() {
                    Ok(Some(status)) => {
                        break if status.success() {
                            Ok(true)
                        } else {
                            Err(format!("Git exited with {status}."))
                        };
                    }
                    Ok(None) => {}
                    Err(e) => break Err(e.to_string()),
                }
                let last = progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .last_output
                    .unwrap_or(started);
                if last.elapsed() >= timeout {
                    break Err("No output from Git for 120 seconds. Check authentication or the network, then retry.".into());
                }
                thread::sleep(Duration::from_millis(20));
            };
            // Stop before scope joins readers: descendants can hold both pipes.
            group.stop().map_err(|e| e.to_string())?;
            result
        })
    })();
    match result {
        Ok(true) if !cancel.load(Ordering::Acquire) => match staging.publish(&request) {
            Ok(repository) => {
                // A parent can move even during the rename syscall. Publication
                // stays anchored, but do not open a different path in the window.
                let visible = request.check_parent().and_then(|()| {
                    let named = std::fs::symlink_metadata(&request.destination)?;
                    let held = repository.metadata()?;
                    if named.dev() == held.dev() && named.ino() == held.ino() {
                        Ok(())
                    } else {
                        Err(io::Error::other("The published destination changed."))
                    }
                });
                let _ = staging.clean(&request);
                match visible {
                    Ok(()) => Outcome::Succeeded(request.destination),
                    Err(error) => Outcome::Failed {
                        message: format!("The repository completed, but its path changed: {error}"),
                        retained: descriptor_path(&repository).or(Some(request.destination)),
                    },
                }
            }
            Err(error) => Outcome::Failed {
                message: format!(
                    "Clone completed but could not be opened at the destination: {error}. The private clone is retained."
                ),
                retained: Some(staging.current_path()),
            },
        },
        result => {
            let retained = staging
                .clean(&request)
                .err()
                .map(|_| staging.current_path());
            match result {
                Ok(_) => Outcome::Cancelled { retained },
                Err(message) => Outcome::Failed { message, retained },
            }
        }
    }
}

#[cfg(test)]
#[path = "clone_tests.rs"]
mod tests;
