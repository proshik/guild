//! Anchored directory identity and atomic no-overwrite moves for app replacement.
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CString, OsStr},
    fs::{File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectoryId {
    pub device: u64,
    pub inode: u64,
}
pub(super) struct Directory {
    pub path: PathBuf,
    file: File,
    pub id: DirectoryId,
}
impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        let path = path.canonicalize()?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        let metadata = file.metadata()?;
        let id = DirectoryId {
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        Ok(Self { path, file, id })
    }
    pub fn revalidate(&self) -> io::Result<()> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.dev() != self.id.device
            || metadata.ino() != self.id.inode
            || self.path.canonicalize()? != self.path
        {
            return Err(io::Error::other("update destination directory changed"));
        }
        Ok(())
    }
    pub fn sync(&self) -> io::Result<()> {
        self.revalidate()?;
        self.file.sync_all()
    }
    pub fn rename(&self, source: &OsStr, destination: &OsStr) -> io::Result<()> {
        self.revalidate()?;
        rename_at(
            self.file.as_raw_fd(),
            &name(source)?,
            self.file.as_raw_fd(),
            &name(destination)?,
        )
    }
    pub fn adopt(&self, source: &Path, destination: &OsStr) -> io::Result<()> {
        self.revalidate()?;
        let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| invalid_name())?;
        rename_at(
            libc::AT_FDCWD,
            &source,
            self.file.as_raw_fd(),
            &name(destination)?,
        )
    }
}
fn invalid_name() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid update entry name")
}
fn name(value: &OsStr) -> io::Result<CString> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(invalid_name());
    }
    CString::new(bytes).map_err(|_| invalid_name())
}
fn rename_at(from_fd: i32, source: &CString, to_fd: i32, destination: &CString) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    // SAFETY: valid directory descriptors/C strings; RENAME_EXCL atomically refuses an existing destination.
    let result = unsafe {
        libc::renameatx_np(
            from_fd,
            source.as_ptr(),
            to_fd,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    // SAFETY: valid directory descriptors/C strings; RENAME_NOREPLACE has no overwrite fallback.
    let result = unsafe {
        libc::renameat2(
            from_fd,
            source.as_ptr(),
            to_fd,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace update moves are unavailable",
    ));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(test)]
pub(super) fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    let source_parent = Directory::open(source.parent().ok_or_else(invalid_name)?)?;
    let destination_parent = Directory::open(destination.parent().ok_or_else(invalid_name)?)?;
    source_parent.revalidate()?;
    destination_parent.revalidate()?;
    rename_at(
        source_parent.file.as_raw_fd(),
        &name(source.file_name().ok_or_else(invalid_name)?)?,
        destination_parent.file.as_raw_fd(),
        &name(destination.file_name().ok_or_else(invalid_name)?)?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transaction_moves_never_replace_an_existing_file_or_empty_directory() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("новая сборка.app");
        let destination = root.path().join("предыдущая.app");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("payload"), b"new").unwrap();
        std::fs::create_dir(&destination).unwrap();
        let inode = std::fs::metadata(&destination).unwrap().ino();
        assert!(
            rename_new(&source, &destination).is_err(),
            "POSIX rename would overwrite an unexpected empty backup"
        );
        assert_eq!(std::fs::metadata(&destination).unwrap().ino(), inode);
        assert_eq!(std::fs::read(source.join("payload")).unwrap(), b"new");
        let a = root.path().join("a");
        let b = root.path().join("b");
        std::fs::write(&a, b"new").unwrap();
        std::fs::write(&b, b"unrelated").unwrap();
        assert!(rename_new(&a, &b).is_err());
        assert_eq!(std::fs::read(&b).unwrap(), b"unrelated");
        let empty = root.path().join("ready.app");
        rename_new(&source, &empty).unwrap();
        assert_eq!(std::fs::read(empty.join("payload")).unwrap(), b"new");
    }
    #[test]
    fn parent_directory_substitution_cannot_redirect_a_journal_operation() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("apps");
        std::fs::create_dir(&parent).unwrap();
        let directory = Directory::open(&parent).unwrap();
        directory.revalidate().unwrap();
        std::fs::rename(&parent, root.path().join("original")).unwrap();
        std::fs::create_dir(&parent).unwrap();
        assert!(
            directory.revalidate().is_err(),
            "a canonical pathname alone cannot pin the destination volume/directory"
        );
        assert_eq!(
            directory.id.inode,
            std::fs::metadata(root.path().join("original"))
                .unwrap()
                .ino()
        );
    }
    #[test]
    fn an_unexpected_destination_symlink_keeps_the_external_tree_intact() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"keep").unwrap();
        let source = root.path().join("new.app");
        std::fs::create_dir(&source).unwrap();
        let destination = root.path().join("Guild.app");
        std::os::unix::fs::symlink(&outside, &destination).unwrap();
        assert!(rename_new(&source, &destination).is_err());
        assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
        assert!(
            std::fs::symlink_metadata(destination)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
