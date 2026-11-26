//! Manual recovery checks the owner's running executable paths; never signals a process.
use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};
/// The owner must keep all transaction apps closed for the full manual recovery.
/// This bounded same-owner process check supplements that explicit assertion.
pub(crate) fn in_use(paths: &[PathBuf]) -> Result<bool, ()> {
    let targets: Vec<_> = paths
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect();
    let own = std::process::id() as libc::pid_t;
    let mut pids = [0 as libc::pid_t; 4096];
    let bytes = std::mem::size_of_val(&pids) as libc::c_int;
    // SAFETY: geteuid has no preconditions; PROC_UID_ONLY (4) selects this owner.
    // The array is writable for exactly the byte count supplied to libproc.
    let count = unsafe { libc::proc_listpids(4, libc::geteuid(), pids.as_mut_ptr().cast(), bytes) };
    if count <= 0
        || count >= bytes
        || !(count as usize).is_multiple_of(std::mem::size_of::<libc::pid_t>())
    {
        return Err(());
    }
    for pid in &pids[..count as usize / std::mem::size_of::<libc::pid_t>()] {
        if *pid <= 0 || *pid == own {
            continue;
        }
        let mut buffer = [0u8; 4096];
        // SAFETY: buffer owns the full bounded writable region. No process args
        // or environment are requested or logged.
        let size =
            unsafe { libc::proc_pidpath(*pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if size <= 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::ENOENT)) {
                continue;
            }
            return Err(());
        }
        let path = CStr::from_bytes_until_nul(&buffer).map_err(|_| ())?;
        let path = PathBuf::from(OsStr::from_bytes(path.to_bytes()));
        let actual = path.canonicalize().unwrap_or(path);
        if targets.contains(&actual) {
            return Ok(true);
        }
    }
    Ok(false)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_executable_is_refused_even_after_its_bundle_directory_moves() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().canonicalize().unwrap().join("Guild.app");
        std::fs::create_dir(&app).unwrap();
        let executable = app.join("sleep");
        std::fs::copy("/bin/sleep", &executable).unwrap();
        let mut child = std::process::Command::new(&executable)
            // Keep the owned child alive throughout a loaded workspace test run.
            // It is killed and reaped below on both success and assertion failure.
            .arg("300")
            .spawn()
            .unwrap();
        let moved = root.path().canonicalize().unwrap().join("previous.app");
        let result = || {
            assert!(in_use(std::slice::from_ref(&executable)).unwrap());
            std::fs::rename(&app, &moved).unwrap();
            assert!(
                child.try_wait().unwrap().is_none(),
                "fixture child exited before the moved-path check"
            );
            assert!(in_use(&[executable.clone(), moved.join("sleep")]).unwrap());
        };
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(result));
        let _ = child.kill();
        child.wait().unwrap();
        checked.unwrap();
        assert!(!in_use(&[executable.clone(), moved.join("sleep")]).unwrap());
    }
}
