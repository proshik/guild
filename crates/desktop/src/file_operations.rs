//! Project-local transfers shared by Explorer keyboard, clipboard and dragging.
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Move,
    Copy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collision {
    Cancel,
    Skip,
    KeepBoth,
}
#[derive(Default, Debug)]
pub struct Outcome {
    pub completed: Vec<(PathBuf, PathBuf)>,
    pub remaining: Vec<PathBuf>,
    pub errors: Vec<(PathBuf, String)>,
    pub collisions: Vec<PathBuf>,
}

pub fn normalize(sources: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = sources.to_vec();
    paths.sort();
    paths.dedup();
    let all = paths.clone();
    paths.retain(|path| {
        !all.iter()
            .any(|parent| parent != path && path.starts_with(parent))
    });
    paths
}

/// Component-wise mapping; similarly prefixed siblings never match.
pub fn remap(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|suffix| {
        if suffix.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(suffix)
        }
    })
}

pub fn execute(
    root: &Path,
    sources: &[PathBuf],
    destination: &Path,
    operation: Operation,
    collision: Collision,
    cancel: &AtomicBool,
) -> Outcome {
    execute_with_identities(
        root,
        sources,
        destination,
        operation,
        collision,
        cancel,
        None,
    )
}

/// A retained operation must still address the objects that the user selected.
pub(crate) fn execute_with_identities(
    root: &Path,
    sources: &[PathBuf],
    destination: &Path,
    operation: Operation,
    collision: Collision,
    cancel: &AtomicBool,
    identities: Option<&std::collections::BTreeMap<PathBuf, (u64, u64)>>,
) -> Outcome {
    use std::sync::atomic::Ordering;
    let sources = normalize(sources);
    let mut out = Outcome::default();
    if cancel.load(Ordering::Relaxed) {
        out.remaining = sources;
        return out;
    }
    let prepare = || -> std::io::Result<_> {
        valid_relative(destination, true)?;
        let outside: Vec<_> = sources
            .iter()
            .filter(|p| p.is_absolute())
            .cloned()
            .collect();
        if !outside.is_empty() {
            if operation != Operation::Copy {
                return Err(invalid("Files from outside the project are only copied"));
            }
            admits(&present(&outside)?, &destination_of(root, destination)?)?;
        }
        for path in sources.iter().filter(|p| !p.is_absolute()) {
            valid_relative(path, false)?;
            if destination.starts_with(path) {
                return Err(invalid("A destination cannot be inside its source"));
            }
        }
        let root = std::fs::File::open(root.canonicalize()?)?;
        let dest = directory(&root, destination)?;
        Ok((root, dest))
    };
    let (root, dest) = match prepare() {
        Ok(ready) => ready,
        Err(error) => {
            out.errors.push((destination.into(), error.to_string()));
            out.remaining = sources;
            return out;
        }
    };
    if collision == Collision::Cancel {
        for source in &sources {
            let target = destination.join(source.file_name().unwrap());
            if target != *source && stat(&dest, source.file_name().unwrap()).is_ok() {
                out.collisions.push(target);
            }
        }
        if !out.collisions.is_empty() {
            out.remaining = sources;
            return out;
        }
    }
    for (index, source) in sources.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            out.remaining.extend_from_slice(&sources[index..]);
            break;
        }
        let name = source.file_name().unwrap();
        let mut target = destination.join(name);
        if *source == target {
            // Moving in place is a no-op. Copy in place deliberately makes a sibling.
            if operation == Operation::Move {
                out.completed.push((source.clone(), target));
                continue;
            }
        }
        let result = (|| -> std::io::Result<Option<PathBuf>> {
            let parent = if source.is_absolute() {
                // Another application named it: its folder is opened as given.
                File::open(source.parent().unwrap_or(Path::new("/")))?
            } else {
                directory(&root, source.parent().unwrap_or(Path::new("")))?
            };
            let source_stat = stat(&parent, name)?;
            let mut serial = 1;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                if let Some(identities) = identities {
                    let current = stat(&parent, name)?;
                    if identities.get(source)
                        != Some(&(current.st_dev as u64, current.st_ino as u64))
                    {
                        return Err(invalid(
                            "The source changed; select Copy, Cut or Move again",
                        ));
                    }
                }
                let to = target.file_name().unwrap();
                let result = if operation == Operation::Move {
                    rename(&parent, name, &dest, to)
                } else {
                    copy_staged(&parent, name, &dest, to, cancel)
                };
                match result {
                    Ok(()) => return Ok(Some(target)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        match collision {
                            Collision::KeepBoth => {
                                serial += 1;
                                target = destination.join(unique_name(
                                    name,
                                    source_stat.st_mode & libc::S_IFMT == libc::S_IFDIR,
                                    serial,
                                ));
                            }
                            _ => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        })();
        match result {
            Ok(Some(target)) => out.completed.push((source.clone(), target)),
            Ok(None) => out.remaining.push(source.clone()),
            Err(error) => {
                out.remaining.push(source.clone());
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    out.collisions.push(destination.join(name));
                    if collision == Collision::Cancel {
                        out.remaining.extend_from_slice(&sources[index + 1..]);
                        break;
                    }
                } else {
                    out.errors.push((source.clone(), if error.raw_os_error() == Some(libc::EXDEV) {
                        "Cross-device move refused; copy the item before moving its source to Trash".into()
                    } else { error.to_string() }));
                }
            }
        }
    }
    out
}

/// Rename a basename or directory under the project without following parent links.
pub(crate) fn rename_beneath(root: &Path, from: &Path, to: &Path) -> std::io::Result<()> {
    valid_relative(from, false)?;
    valid_relative(to, false)?;
    if from == to {
        return validate_source(root, from);
    }
    if to.starts_with(from) {
        return Err(invalid("A destination cannot be inside its source"));
    }
    let root = File::open(root.canonicalize()?)?;
    let source = directory(&root, from.parent().unwrap_or(Path::new("")))?;
    let dest = directory(&root, to.parent().unwrap_or(Path::new("")))?;
    rename(
        &source,
        from.file_name().unwrap(),
        &dest,
        to.file_name().unwrap(),
    )
}

/// Drop feedback uses the same topology boundary as execution.
pub(crate) fn validate_destination(
    root: &Path,
    sources: &[PathBuf],
    destination: &Path,
) -> std::io::Result<()> {
    valid_relative(destination, true)?;
    if sources.is_empty() {
        return Err(invalid("No files selected"));
    }
    for source in sources {
        valid_relative(source, false)?;
        if destination.starts_with(source) {
            return Err(invalid("A destination cannot be inside its source"));
        }
    }
    let root = File::open(root.canonicalize()?)?;
    directory(&root, destination).map(|_| ())
}

/// Validate an existing project entry without following any parent symlink.
pub(crate) fn validate_source(root: &Path, path: &Path) -> std::io::Result<()> {
    valid_relative(path, false)?;
    let root = File::open(root.canonicalize()?)?;
    let parent = directory(&root, path.parent().unwrap_or(Path::new("")))?;
    stat(&parent, path.file_name().unwrap()).map(|_| ())
}

/// What a drag that leaves the window hands to the platform: each source that
/// is still a row inside the project, made absolute, with whether it is a
/// directory. A link is offered as the link, never as what it points to.
pub(crate) fn outbound(root: &Path, sources: &[PathBuf]) -> Vec<(PathBuf, bool)> {
    sources
        .iter()
        .filter(|path| validate_source(root, path).is_ok())
        .filter_map(|path| {
            let path = root.join(path);
            let is_dir = std::fs::symlink_metadata(&path).ok()?.is_dir();
            Some((path, is_dir))
        })
        .collect()
}

/// The prefix of a copy's private staging folder, beside its destination.
const STAGING: &str = ".guild-transfer-";

/// The staging folders of the copies running in this process, by name.
static LIVE: std::sync::Mutex<std::collections::BTreeSet<OsString>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// A running copy's claim on its staging folder's name, from before the
/// folder exists until after it is removed. A folder whose copy is gone —
/// a crash, a failed cleanup — is nobody's and shows like any other.
pub(crate) struct LiveStage(OsString);
impl LiveStage {
    pub(crate) fn hold(name: OsString) -> Self {
        LIVE.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.clone());
        Self(name)
    }
}
impl Drop for LiveStage {
    fn drop(&mut self) {
        LIVE.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Whether a directory entry is the staging folder of a copy running right
/// now in this process, which nobody is meant to see.
pub fn is_staging(name: &str) -> bool {
    name.strip_prefix(STAGING)
        .is_some_and(|rest| uuid::Uuid::try_parse(rest).is_ok())
        && LIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(OsStr::new(name))
}

/// Where a drop on a row lands: a folder row is that folder, a file row the
/// folder it is in, and the empty path — the project's own row and the tree's
/// empty space — the project root.
pub(crate) fn drop_target(row: &Path, is_dir: bool) -> PathBuf {
    if is_dir {
        row.into()
    } else {
        row.parent().unwrap_or(Path::new("")).into()
    }
}

/// How many times a set of dropped paths was read: a window test counts them per
/// gesture. The window tests run alone, in a process of their own.
#[cfg(test)]
pub(crate) static EXAMINED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A file's identity: its device and inode, whatever the spelling of its path.
pub(crate) type Identity = (u64, u64);

/// One item another application dropped, by identity: itself, never followed
/// if it is a link, and the folder it is in.
#[derive(Clone, Debug)]
pub(crate) struct Dropped {
    pub path: PathBuf,
    pub id: Identity,
    parent: Identity,
}

/// A destination folder by identity, with every folder it lies in up to the
/// root of the filesystem — itself first.
#[derive(Clone, Debug)]
pub(crate) struct Destination {
    within: Vec<Identity>,
}

/// What was dropped, read from the disk once: absolute paths that are there.
/// Their folders may be on any volume, so this is a worker's job.
pub(crate) fn examine(sources: &[PathBuf]) -> std::io::Result<Vec<Dropped>> {
    #[cfg(test)]
    EXAMINED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if sources.is_empty() {
        return Err(invalid("No files dropped"));
    }
    sources.iter().map(|source| dropped(source)).collect()
}

/// What was dropped and is still there. An item gone since is left out, to
/// fail alone in the copier's own loop under its own name; its folder must
/// still be there.
fn present(sources: &[PathBuf]) -> std::io::Result<Vec<Dropped>> {
    #[cfg(test)]
    EXAMINED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut found = Vec::new();
    for source in sources {
        match dropped(source) {
            Ok(item) => found.push(item),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && source.parent().is_some_and(Path::is_dir) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(found)
}

fn dropped(source: &Path) -> std::io::Result<Dropped> {
    use std::os::unix::fs::MetadataExt;
    let (Some(parent), Some(_)) = (source.parent(), source.file_name()) else {
        return Err(invalid("Only files from another application are copied in"));
    };
    if !source.is_absolute() {
        return Err(invalid("Only files from another application are copied in"));
    }
    let own = std::fs::symlink_metadata(source)?;
    let folder = std::fs::metadata(parent)?;
    Ok(Dropped {
        path: source.into(),
        id: (own.dev(), own.ino()),
        parent: (folder.dev(), folder.ino()),
    })
}

/// A project folder and the folders above it, by identity: opened from the
/// project's root without following a link, then up through `..`.
pub(crate) fn destination_of(root: &Path, destination: &Path) -> std::io::Result<Destination> {
    valid_relative(destination, true)?;
    let root = File::open(root.canonicalize()?)?;
    let mut current = directory(&root, destination)?;
    let mut within = Vec::new();
    loop {
        let id = fstat(&current)?;
        // `..` of the filesystem's root is that root again.
        if within.contains(&id) || within.len() >= 4096 {
            break;
        }
        within.push(id);
        // Search-only: a folder above may be traversable and not readable.
        current = open_at(&current, OsStr::new(".."), libc::O_SEARCH, 0)?;
    }
    Ok(Destination { within })
}

/// An in-window drop's refusals, for a copy from outside: nothing dropped
/// may be the destination or a folder it lies in, and nothing may already
/// be in the destination itself.
pub(crate) fn admits(dropped: &[Dropped], destination: &Destination) -> std::io::Result<()> {
    for item in dropped {
        if destination.within.contains(&item.id) {
            return Err(invalid("A destination cannot be inside its source"));
        }
        if destination.within.first() == Some(&item.parent) {
            return Err(invalid("Already in this folder"));
        }
    }
    Ok(())
}

/// Whether what another application drops may be copied into `destination`,
/// asked of the disk in one go — the worker's final word before it copies.
pub(crate) fn inbound(
    root: &Path,
    sources: &[PathBuf],
    destination: &Path,
) -> std::io::Result<Vec<Dropped>> {
    if sources.is_empty() {
        return Err(invalid("No files dropped"));
    }
    let dropped = present(sources)?;
    if dropped.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Nothing dropped is there any more",
        ));
    }
    admits(&dropped, &destination_of(root, destination)?)?;
    Ok(dropped)
}

fn fstat(file: &File) -> std::io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let meta = file.metadata()?;
    Ok((meta.dev(), meta.ino()))
}

fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}
fn valid_relative(path: &Path, root_allowed: bool) -> std::io::Result<()> {
    if (!root_allowed && path.as_os_str().is_empty())
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(invalid("Only paths inside the project are writable"));
    }
    Ok(())
}
fn unique_name(name: &std::ffi::OsStr, directory: bool, serial: usize) -> std::ffi::OsString {
    let path = Path::new(name);
    let mut next = if directory {
        name.to_os_string()
    } else {
        path.file_stem().unwrap_or(name).to_os_string()
    };
    next.push(format!(" ({serial})"));
    if !directory && let Some(extension) = path.extension() {
        next.push(".");
        next.push(extension);
    }
    next
}

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

/// A file's bytes as `std::fs::read` reads them — links followed — but
/// never waiting: opened with `O_NONBLOCK`, so a FIFO with no writer
/// answers at once, and anything but a regular file is `None`, as a file
/// that is not there. For what the window's thread reads from a working
/// tree — review r3 `docs+tests-4`.
pub(crate) fn read_regular_file(path: &Path) -> Option<Vec<u8>> {
    read_regular_file_up_to(path, u64::MAX - 1)
}

/// [`read_regular_file`], taking at most `limit` bytes and one more — enough
/// for a reader to say *too large* without the rest coming into memory.
/// Review r4 `adversarial-6`.
pub(crate) fn read_regular_file_up_to(path: &Path, limit: u64) -> Option<Vec<u8>> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len().min(limit + 1) as usize);
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn c(name: &OsStr) -> std::io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| invalid("NUL in a path"))
}
fn checked(value: libc::c_int) -> std::io::Result<()> {
    if value == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn open_at(
    parent: &File,
    name: &OsStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> std::io::Result<File> {
    let name = c(name)?;
    // SAFETY: owned directory descriptor, valid C string; returned descriptor is adopted once.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    };
    checked(fd)?;
    Ok(unsafe { File::from_raw_fd(fd) })
}
/// A file under `root`, opened without following a link at any component —
/// a parent directory that became a symlink is refused like the file itself
/// — and without blocking on a FIFO. The caller checks what it opened.
/// GLD-026, review r2.
pub(crate) fn open_beneath(root: &Path, relative: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    valid_relative(relative, false)?;
    let top = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(root)?;
    let name = relative
        .file_name()
        .ok_or_else(|| invalid("no file name"))?;
    let parent = relative.parent().unwrap_or(Path::new(""));
    let dir = directory(&top, parent)?;
    open_at(&dir, name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
}
fn directory(root: &File, path: &Path) -> std::io::Result<File> {
    valid_relative(path, true)?;
    let mut dir = root.try_clone()?;
    for component in path.components() {
        dir = open_at(
            &dir,
            component.as_os_str(),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
    }
    Ok(dir)
}
fn stat(parent: &File, name: &OsStr) -> std::io::Result<libc::stat> {
    let name = c(name)?;
    let mut result = std::mem::MaybeUninit::uninit();
    // SAFETY: output is initialized by a successful fstatat, without following the final link.
    checked(unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    })?;
    Ok(unsafe { result.assume_init() })
}
fn rename(from: &File, name: &OsStr, dest: &File, target: &OsStr) -> std::io::Result<()> {
    let name = c(name)?;
    let target = c(target)?;
    // SAFETY: the two pinned parents and strings outlive the no-replace kernel operation.
    checked(unsafe {
        libc::renameatx_np(
            from.as_raw_fd(),
            name.as_ptr(),
            dest.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    })
}
fn mkdir(parent: &File, name: &OsStr) -> std::io::Result<()> {
    let name = c(name)?;
    // SAFETY: parent and name are valid for this call. Exclusive mkdir never merges.
    checked(unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) })
}
fn names(dir: &File) -> std::io::Result<Vec<OsString>> {
    // SAFETY: fdopendir takes ownership of the duplicate, never the caller's descriptor.
    let fd = unsafe { libc::dup(dir.as_raw_fd()) };
    checked(fd)?;
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(error);
    }
    struct Stream(*mut libc::DIR);
    impl Drop for Stream {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    let stream = Stream(stream);
    let mut names = Vec::new();
    loop {
        // SAFETY: this stream is exclusively owned; each entry is copied before the next read.
        unsafe {
            *libc::__error() = 0;
        }
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(0) {
                return Err(error);
            }
            break;
        }
        let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if bytes != b"." && bytes != b".." {
            names.push(OsString::from_vec(bytes.to_vec()));
        }
    }
    names.sort();
    Ok(names)
}
fn copy_entry(
    from: &File,
    name: &OsStr,
    dest: &File,
    target: &OsStr,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Copy cancelled",
        ));
    }
    let metadata = stat(from, name)?;
    match metadata.st_mode & libc::S_IFMT {
        libc::S_IFDIR => {
            let input = open_at(from, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            mkdir(dest, target)?;
            let output = open_at(dest, target, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            for child in names(&input)? {
                copy_entry(&input, &child, &output, &child, cancel)?;
            }
            checked(unsafe { libc::fchmod(output.as_raw_fd(), metadata.st_mode & 0o777) })?;
        }
        libc::S_IFREG => {
            let mut input = open_at(from, name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
            if !input.metadata()?.is_file() {
                return Err(invalid("Source type changed during copy"));
            }
            let mut output = open_at(
                dest,
                target,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )?;
            std::io::copy(&mut input, &mut output)?;
            output.set_permissions(input.metadata()?.permissions())?;
            output.sync_all()?;
        }
        libc::S_IFLNK => {
            let name = c(name)?;
            let target = c(target)?;
            let mut buffer = vec![0u8; 4096];
            let size = unsafe {
                libc::readlinkat(
                    from.as_raw_fd(),
                    name.as_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                )
            };
            if size < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if size as usize == buffer.len() {
                return Err(invalid("Symbolic link is too long"));
            }
            buffer.truncate(size as usize);
            let value = CString::new(buffer).map_err(|_| invalid("Invalid symbolic link"))?;
            checked(unsafe { libc::symlinkat(value.as_ptr(), dest.as_raw_fd(), target.as_ptr()) })?;
        }
        _ => {
            return Err(invalid(
                "Only regular files, directories and symbolic links can be copied",
            ));
        }
    }
    Ok(())
}
fn remove_owned(parent: &File, name: &OsStr) -> std::io::Result<()> {
    let is_dir = stat(parent, name)?.st_mode & libc::S_IFMT == libc::S_IFDIR;
    if is_dir {
        let child = open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        checked(unsafe { libc::fchmod(child.as_raw_fd(), 0o700) })?;
        for name in names(&child)? {
            remove_owned(&child, &name)?;
        }
    }
    let name = c(name)?;
    checked(unsafe {
        libc::unlinkat(
            parent.as_raw_fd(),
            name.as_ptr(),
            if is_dir { libc::AT_REMOVEDIR } else { 0 },
        )
    })
}
fn copy_staged(
    from: &File,
    name: &OsStr,
    dest: &File,
    target: &OsStr,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    let stage = OsString::from(format!("{STAGING}{}", uuid::Uuid::new_v4()));
    let _live = LiveStage::hold(stage.clone());
    mkdir(dest, &stage)?;
    let result = (|| {
        let staging = open_at(dest, &stage, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        copy_entry(from, name, &staging, OsStr::new("item"), cancel)?;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Copy cancelled",
            ));
        }
        rename(&staging, OsStr::new("item"), dest, target)
    })();
    let cleanup = remove_owned(dest, &stage);
    // A successful publication must remain a success even if staging cleanup fails.
    if let Err(error) = cleanup {
        tracing::warn!(?stage,%error,"transfer staging cleanup failed");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    fn run(
        root: &Path,
        sources: &[&str],
        dest: &str,
        op: Operation,
        collision: Collision,
    ) -> Outcome {
        execute(
            root,
            &sources.iter().map(PathBuf::from).collect::<Vec<_>>(),
            Path::new(dest),
            op,
            collision,
            &AtomicBool::new(false),
        )
    }
    #[test]
    fn a_drag_out_hands_over_only_project_paths_absolute_with_their_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("dir/nested")).unwrap();
        fs::write(r.join("file.txt"), "file").unwrap();
        fs::write(r.join("dir/nested/child"), "child").unwrap();
        symlink(r.join("dir"), r.join("link")).unwrap();
        let sources = [
            "file.txt",
            "dir",
            "link",
            "gone.txt",
            "",
            "../escape",
            "link/nested",
        ]
        .map(PathBuf::from);
        assert_eq!(
            outbound(r, &sources),
            vec![
                (r.join("file.txt"), false),
                (r.join("dir"), true),
                (r.join("link"), false),
            ],
            "missing rows, the project itself, escapes and paths through a link are not offered"
        );
    }
    #[test]
    fn a_drop_on_a_row_lands_in_its_folder() {
        assert_eq!(drop_target(Path::new("src"), true), Path::new("src"));
        assert_eq!(
            drop_target(Path::new("src/lib.rs"), false),
            Path::new("src")
        );
        assert_eq!(drop_target(Path::new("top.rs"), false), Path::new(""));
        assert_eq!(
            drop_target(Path::new(""), true),
            Path::new(""),
            "the project's own row and the tree's empty space are the root"
        );
    }
    #[test]
    fn an_inbound_drop_refuses_what_an_in_window_drop_refuses() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path().join("project");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(r.join("dest/sub")).unwrap();
        fs::create_dir_all(outside.join("folder")).unwrap();
        fs::write(outside.join("file.txt"), "file").unwrap();
        fs::write(r.join("dest/inside.txt"), "inside").unwrap();
        let ok = [outside.join("file.txt"), outside.join("folder")];
        assert!(inbound(&r, &ok, Path::new("dest")).is_ok());
        assert!(inbound(&r, &ok, Path::new("")).is_ok());
        for (sources, destination, why) in [
            (vec![], "dest", "nothing dropped"),
            (
                vec![PathBuf::from("dest/inside.txt")],
                "",
                "a relative path",
            ),
            (
                vec![r.join("dest/inside.txt")],
                "dest",
                "already in the folder",
            ),
            (vec![r.join("dest")], "dest/sub", "into a folder of its own"),
            (vec![r.clone()], "dest", "the project into itself"),
            (
                vec![tmp.path().into()],
                "",
                "a parent of the project into it",
            ),
            (ok.to_vec(), "missing", "a folder that is not there"),
            (
                ok.to_vec(),
                "../outside",
                "a destination outside the project",
            ),
            (
                vec![outside.join("gone.txt")],
                "dest",
                "a source that is gone",
            ),
        ] {
            assert!(
                inbound(&r, &sources, Path::new(destination)).is_err(),
                "{why}"
            );
        }
    }
    /// Containment is a question of identity, not of spelling: on
    /// case-insensitive APFS `PROJECT` is `project`, and `/System/Volumes/Data`
    /// is a firmlinked spelling of the same folders. Either spelling of a
    /// folder the destination lies in must be refused, or the copy recurses
    /// into its own growing staging folder.
    #[test]
    fn an_inbound_drop_knows_its_own_folder_by_any_spelling() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let r = base.join("project");
        fs::create_dir_all(r.join("dest/sub")).unwrap();
        fs::write(r.join("dest/inside.txt"), "inside").unwrap();
        let mut spellings = Vec::new();
        if base.join("PROJECT").exists() {
            spellings.push(base.join("PROJECT"));
        }
        let firmlink = Path::new("/System/Volumes/Data").join(base.strip_prefix("/").unwrap());
        if firmlink.join("project").exists() {
            spellings.push(firmlink.join("project"));
        }
        assert!(!spellings.is_empty(), "no second spelling on this volume");
        let link = base.join("link-to-project");
        symlink(&r, &link).unwrap();
        assert!(
            inbound(&r, std::slice::from_ref(&link), Path::new("dest")).is_ok(),
            "a link is copied as the link, never followed"
        );
        for project in &spellings {
            let out = execute(
                &r,
                std::slice::from_ref(project),
                Path::new("dest/sub"),
                Operation::Copy,
                Collision::KeepBoth,
                &AtomicBool::new(false),
            );
            assert!(
                out.completed.is_empty() && !out.errors.is_empty(),
                "{out:?}"
            );
            assert_eq!(fs::read_dir(r.join("dest/sub")).unwrap().count(), 0);
        }
        for project in spellings {
            for (source, destination, why) in [
                (
                    project.join("dest"),
                    "dest/sub",
                    "a folder into its own subfolder",
                ),
                (project.clone(), "dest", "the project into itself"),
                (
                    project.join("dest/inside.txt"),
                    "dest",
                    "a file into its own folder",
                ),
            ] {
                assert!(
                    inbound(&r, std::slice::from_ref(&source), Path::new(destination)).is_err(),
                    "{why}: {}",
                    source.display()
                );
            }
        }
    }
    /// A folder above the project may be traversable and not readable — a
    /// shared parent of mode `--x`. Walking up from the destination must not
    /// need to read it, or every drop into the project is refused in silence.
    #[test]
    fn an_inbound_drop_walks_up_through_a_folder_it_cannot_read() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let locked = tmp.path().join("locked");
        let r = locked.join("project");
        fs::create_dir_all(r.join("dest")).unwrap();
        let outside = tmp.path().join("file.txt");
        fs::write(&outside, "file").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o100)).unwrap();
        let walked = destination_of(&r, Path::new("dest")).map(|d| d.within.len());
        let dropped = inbound(&r, std::slice::from_ref(&outside), Path::new("dest")).is_ok();
        let into_itself = inbound(&r, std::slice::from_ref(&locked), Path::new("dest")).is_err();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(walked.as_ref().is_ok_and(|n| *n > 3), "{walked:?}");
        assert!(dropped, "a file from outside lands");
        assert!(
            into_itself,
            "and the unreadable folder still knows it is above"
        );
    }
    /// One dropped item gone before the copy fails alone: the others are
    /// copied and the error names the item, on every round of the collision
    /// loop too.
    #[test]
    fn an_inbound_item_that_vanished_fails_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path().join("project");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(r.join("dest")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("kept.txt"), "kept").unwrap();
        let gone = outside.join("gone.txt");
        let sources = [gone.clone(), outside.join("kept.txt")];
        assert!(inbound(&r, &sources, Path::new("dest")).is_ok());
        let out = execute(
            &r,
            &sources,
            Path::new("dest"),
            Operation::Copy,
            Collision::Cancel,
            &AtomicBool::new(false),
        );
        assert!(r.join("dest/kept.txt").is_file(), "{out:?}");
        assert_eq!(out.remaining, vec![gone.clone()]);
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].0, gone);
        assert!(inbound(&r, std::slice::from_ref(&gone), Path::new("dest")).is_err());
    }
    #[test]
    fn an_inbound_copy_uses_the_one_copier_and_leaves_the_originals() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path().join("project");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(r.join("dest")).unwrap();
        fs::create_dir_all(outside.join("folder/nested")).unwrap();
        fs::write(outside.join("folder/nested/child"), "child").unwrap();
        fs::write(outside.join("file.txt"), "new").unwrap();
        fs::write(r.join("dest/file.txt"), "existing").unwrap();
        let sources = [outside.join("file.txt"), outside.join("folder")];
        let none = AtomicBool::new(false);
        let out = execute(
            &r,
            &sources,
            Path::new("dest"),
            Operation::Copy,
            Collision::Cancel,
            &none,
        );
        assert_eq!(out.collisions, vec![PathBuf::from("dest/file.txt")]);
        assert!(out.completed.is_empty(), "a collision stops before writing");
        let out = execute(
            &r,
            &sources,
            Path::new("dest"),
            Operation::Copy,
            Collision::KeepBoth,
            &none,
        );
        assert!(out.errors.is_empty(), "{out:?}");
        assert_eq!(
            fs::read_to_string(r.join("dest/file.txt")).unwrap(),
            "existing"
        );
        assert_eq!(
            fs::read_to_string(r.join("dest/file (2).txt")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(r.join("dest/folder/nested/child")).unwrap(),
            "child"
        );
        assert!(outside.join("file.txt").is_file());
        assert!(outside.join("folder/nested/child").is_file());
        let out = execute(
            &r,
            &[outside.join("file.txt")],
            Path::new(""),
            Operation::Move,
            Collision::Cancel,
            &none,
        );
        assert!(!out.errors.is_empty(), "files from outside are never moved");
        assert!(outside.join("file.txt").is_file());
        assert!(!r.join("file.txt").exists());
    }
    #[test]
    fn recursive_copy_preserves_links_and_normalizes_selected_descendants() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("src/nested")).unwrap();
        fs::create_dir(r.join("dest")).unwrap();
        fs::write(r.join("src/nested/file"), "disk content").unwrap();
        symlink("/outside/missing", r.join("src/link")).unwrap();
        let out = run(
            r,
            &["src/nested/file", "src", "src"],
            "dest",
            Operation::Copy,
            Collision::Cancel,
        );
        assert_eq!(
            out.completed,
            vec![("src".into(), "dest/src".into())],
            "{out:?}"
        );
        assert_eq!(
            fs::read_to_string(r.join("dest/src/nested/file")).unwrap(),
            "disk content"
        );
        assert_eq!(
            fs::read_link(r.join("dest/src/link")).unwrap(),
            Path::new("/outside/missing")
        );
        assert!(r.join("src/nested/file").is_file());
    }
    #[test]
    fn collisions_never_overwrite_and_partial_moves_report_survivors() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir(r.join("dest")).unwrap();
        for (name, text) in [
            ("a.txt", "source"),
            ("b.txt", "second"),
            ("dest/a.txt", "original"),
        ] {
            fs::write(r.join(name), text).unwrap();
        }
        let cancelled = run(
            r,
            &["b.txt", "a.txt"],
            "dest",
            Operation::Move,
            Collision::Cancel,
        );
        assert!(cancelled.completed.is_empty());
        assert!(r.join("b.txt").exists());
        let skipped = run(
            r,
            &["b.txt", "a.txt"],
            "dest",
            Operation::Move,
            Collision::Skip,
        );
        assert_eq!(
            skipped.completed,
            vec![("b.txt".into(), "dest/b.txt".into())]
        );
        assert_eq!(skipped.remaining, vec![PathBuf::from("a.txt")]);
        let both = run(r, &["a.txt"], "dest", Operation::Move, Collision::KeepBoth);
        assert_eq!(
            both.completed,
            vec![("a.txt".into(), "dest/a (2).txt".into())]
        );
        assert_eq!(
            fs::read_to_string(r.join("dest/a.txt")).unwrap(),
            "original"
        );
        assert!(!r.join("a.txt").exists());
    }
    #[test]
    fn invalid_topology_and_external_symlink_destinations_do_not_write() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("src/child")).unwrap();
        symlink(outside.path(), r.join("link")).unwrap();
        fs::write(r.join("file"), "keep").unwrap();
        for (sources, dest) in [
            (vec![""], "src"),
            (vec!["src"], "src/child"),
            (vec!["file"], "link"),
            (vec!["../outside"], "src"),
            (vec!["link/file"], "src"),
        ] {
            let out = run(r, &sources, dest, Operation::Move, Collision::KeepBoth);
            assert!(!out.errors.is_empty(), "{sources:?} -> {dest}: {out:?}");
            assert!(out.completed.is_empty());
        }
        assert_eq!(fs::read_to_string(r.join("file")).unwrap(), "keep");
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    }
    #[test]
    fn a_late_source_failure_preserves_successes_and_cancellation_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir(r.join("dest")).unwrap();
        fs::write(r.join("good"), "data").unwrap();
        let out = run(
            r,
            &["good", "missing"],
            "dest",
            Operation::Move,
            Collision::Skip,
        );
        assert_eq!(out.completed, vec![("good".into(), "dest/good".into())]);
        assert_eq!(out.remaining, vec![PathBuf::from("missing")]);
        assert_eq!(out.errors.len(), 1);
        let cancelled = execute(
            r,
            &["dest/good".into()],
            Path::new(""),
            Operation::Move,
            Collision::Cancel,
            &AtomicBool::new(true),
        );
        assert!(cancelled.completed.is_empty());
        assert!(r.join("dest/good").is_file());
    }
}

#[cfg(test)]
mod race_tests {
    use super::*;
    #[test]
    fn concurrent_publications_never_replace_and_leave_no_staging() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for dir in ["one", "two", "dest"] {
            std::fs::create_dir(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("one/file"), "one").unwrap();
        std::fs::write(root.join("two/file"), "two").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = ["one/file", "two/file"]
                .into_iter()
                .map(|source| {
                    scope.spawn(|| {
                        barrier.wait();
                        execute(
                            root,
                            &[source.into()],
                            Path::new("dest"),
                            Operation::Copy,
                            Collision::Cancel,
                            &AtomicBool::new(false),
                        )
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().map(|r| r.completed.len()).sum::<usize>(), 1);
        assert_eq!(results.iter().map(|r| r.collisions.len()).sum::<usize>(), 1);
        assert_eq!(std::fs::read_dir(root.join("dest")).unwrap().count(), 1);
        assert_eq!(
            std::fs::read_to_string(root.join("one/file")).unwrap(),
            "one"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("two/file")).unwrap(),
            "two"
        );
    }
    #[test]
    fn failed_recursive_copy_removes_its_partial_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::create_dir(root.join("dest")).unwrap();
        std::fs::write(root.join("src/first"), "data").unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(root.join("src/socket")).unwrap();
        let result = execute(
            root,
            &["src".into()],
            Path::new("dest"),
            Operation::Copy,
            Collision::Cancel,
            &AtomicBool::new(false),
        );
        assert!(result.completed.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(std::fs::read_dir(root.join("dest")).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_to_string(root.join("src/first")).unwrap(),
            "data"
        );
    }
}
