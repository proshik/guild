//! Bundle validation and completed Local publication. No GPUI or AppKit.
use super::{APP_ID, BuildInfo, hex, lock::FileLock};
use crate::process::ProcessGroup;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
mod macho;

/// Identity binding the last completed Local publication to its bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    /// Completion protocol version.
    pub schema: u32,
    /// Unique bundle artifact identity.
    pub build_id: uuid::Uuid,
    /// Exact source commit.
    pub commit: String,
}
/// Stop work before publishing a cancelled operation.
pub fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("update cancelled".into())
    } else {
        Ok(())
    }
}
/// Open a regular nonsymlink file without blocking on special-file open semantics.
/// Byte limits remain the responsibility of the streaming reader.
pub fn open_regular(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "could not open regular update file")?;
    if !file
        .metadata()
        .map_err(|_| "could not inspect update file")?
        .is_file()
    {
        return Err("update input is not a regular file".into());
    }
    Ok(file)
}
/// Read a bounded, regular, nonsymlink metadata file.
pub fn read_small(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut f = open_regular(path)?;
    let mut bytes = Vec::new();
    (&mut f)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not read update metadata")?;
    if bytes.len() > limit {
        return Err("update metadata exceeds size limit".into());
    }
    Ok(bytes)
}
/// Write a new public artifact without overwriting an existing file.
pub fn write_new(path: &Path, bytes: &[u8], private: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .mode(if private { 0o600 } else { 0o644 })
        .open(path)
        .map_err(|_| "artifact exists or cannot be created")?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "artifact write failed")?;
    File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|_| "artifact directory sync failed".to_string())
}
/// Atomically publish an owned metadata record in a known directory.
pub fn publish_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("metadata has no parent")?;
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err("metadata destination is not a regular file".into());
    }
    let mut f =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "could not prepare update metadata")?;
    serde_json::to_writer(&mut f, value).map_err(|_| "could not encode update metadata")?;
    f.as_file()
        .sync_all()
        .map_err(|_| "could not sync update metadata")?;
    f.persist(path)
        .map_err(|_| "could not publish update metadata")?;
    File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|_| "could not sync update metadata directory".to_string())
}
/// Stream a bounded archive's SHA256 without retaining the payload in memory.
pub fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<(u64, String), String> {
    let mut file = open_regular(path)?;
    let mut hash = Sha256::new();
    let mut n = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buf).map_err(|_| "archive read failed")?;
        if count == 0 {
            break;
        }
        n += count as u64;
        if n > super::MAX_ARCHIVE {
            return Err("archive exceeds size limit".into());
        }
        hash.update(&buf[..count]);
    }
    if n == 0 {
        return Err("empty update archive".into());
    }
    Ok((n, hex(&hash.finalize())))
}
/// Read the embedded identity and require coherent legacy compatibility resources.
pub fn info(app: &Path) -> Result<BuildInfo, String> {
    let resources = app.join("Contents/Resources");
    let bytes = read_small(&resources.join("build-info.json"), super::MAX_MANIFEST)?;
    let info: BuildInfo =
        serde_json::from_slice(&bytes).map_err(|_| "invalid embedded build identity")?;
    info.validate()?;
    let commit = String::from_utf8(read_small(&resources.join("commit"), 1024)?)
        .map_err(|_| "invalid commit resource")?;
    let mut lines = commit.lines();
    let first = lines.next().unwrap_or_default();
    let dirty = lines.any(|l| l.trim() == "dirty");
    let built = String::from_utf8(read_small(&resources.join("built"), 64)?)
        .map_err(|_| "invalid build-time resource")?
        .trim()
        .parse::<u64>()
        .map_err(|_| "invalid build-time resource")?;
    let tag = match read_small(&resources.join("tag"), 1024) {
        Ok(v) => Some(
            String::from_utf8(v)
                .map_err(|_| "invalid tag resource")?
                .trim()
                .to_owned(),
        ),
        Err(_) if !resources.join("tag").exists() => None,
        Err(e) => return Err(e),
    };
    if first != info.commit || dirty != info.dirty || built != info.built || tag != info.tag {
        return Err("embedded and compatibility build identities disagree".into());
    }
    Ok(info)
}
/// A completed Local build; callers never infer completion from target output alone.
pub fn local(directory: &Path) -> Result<(PathBuf, BuildInfo), String> {
    let marker: Ready = serde_json::from_slice(&read_small(
        &directory.join("guild-local-ready.json"),
        4096,
    )?)
    .map_err(|_| "invalid Local completion marker")?;
    let app = directory.join("Guild.app");
    let build = info(&app)?;
    if marker.schema != 1 || marker.build_id != build.build_id || marker.commit != build.commit {
        return Err("Local build is incomplete or being replaced".into());
    }
    Ok((app, build))
}
/// A bounded owned child; raw diagnostics are never exposed as credential-bearing UI text.
pub fn tool(
    program: &str,
    args: &[&std::ffi::OsStr],
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    check_cancel(cancel)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child =
        ProcessGroup::spawn(&mut command).map_err(|_| format!("could not start {program}"))?;
    let (tx, rx) = mpsc::channel();
    let mut readers = Vec::new();
    for (kind, stream) in [
        (
            true,
            child
                .take_stdout()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
        (
            false,
            child
                .take_stderr()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
    ] {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut data = Vec::new();
            let ok = stream.is_some_and(|r| r.take(131073).read_to_end(&mut data).is_ok());
            let _ = tx.send((kind, ok, data));
        }));
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut output = Vec::new();
    let mut error = None;
    let result = loop {
        while let Ok((stdout, ok, data)) = rx.try_recv() {
            if !ok || data.len() > 131072 {
                error = Some("update validation tool exceeded output limit".to_string());
            } else if stdout {
                output = data;
            }
        }
        if let Err(e) = check_cancel(cancel) {
            error = Some(e);
        }
        if Instant::now() >= deadline {
            error = Some("update validation tool timed out".into());
        }
        if let Some(e) = error.take() {
            let _ = child.stop();
            break Err(e);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!("{program} refused this update bundle"))
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.stop();
                break Err("update validation tool failed".into());
            }
        }
    };
    for reader in readers {
        let _ = reader.join();
    }
    for (stdout, ok, data) in rx {
        if !ok || data.len() > 131072 {
            return Err("update validation tool exceeded output limit".into());
        }
        if stdout {
            output = data;
        }
    }
    result?;
    Ok(output)
}
/// Validate both executable slices, app identity, system compatibility and code seal.
pub fn validate(
    app: &Path,
    expected: Option<&BuildInfo>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    expanded_size(app, cancel)?;
    let root = app
        .canonicalize()
        .map_err(|_| "could not resolve update bundle")?;
    if let Some(expected) = expected
        && &info(app)? != expected
    {
        return Err("prepared copy has a different build identity".into());
    }
    let plist = app.join("Contents/Info.plist");
    let id = tool(
        "/usr/libexec/PlistBuddy",
        &[
            "-c".as_ref(),
            "Print :CFBundleIdentifier".as_ref(),
            plist.as_os_str(),
        ],
        cancel,
    )?;
    if String::from_utf8_lossy(&id).trim() != APP_ID {
        return Err("wrong application bundle identity".into());
    }
    let minimum = tool(
        "/usr/libexec/PlistBuddy",
        &[
            "-c".as_ref(),
            "Print :LSMinimumSystemVersion".as_ref(),
            plist.as_os_str(),
        ],
        cancel,
    )?;
    let minimum = String::from_utf8_lossy(&minimum);
    let minimum = minimum.trim();
    if expected.is_some_and(|e| e.min_macos != minimum) {
        return Err("minimum system version disagrees with build metadata".into());
    }
    let current = tool("/usr/bin/sw_vers", &["-productVersion".as_ref()], cancel)?;
    let parse = |text: &str| -> Option<Vec<u32>> {
        let p: Option<Vec<_>> = text
            .trim()
            .split('.')
            .map(|n| n.parse::<u32>().ok())
            .collect();
        let mut p = p?;
        if p.is_empty() || p.len() > 3 {
            return None;
        }
        p.resize(3, 0);
        Some(p)
    };
    if parse(minimum).ok_or("invalid minimum macOS")?
        > parse(&String::from_utf8_lossy(&current)).ok_or("unknown current macOS")?
    {
        return Err("update requires a newer macOS version".into());
    }
    for name in ["guild-desktop", "guild"] {
        let executable = app.join("Contents/MacOS").join(name);
        if !executable.is_file()
            || !executable
                .canonicalize()
                .map_err(|_| "missing app executable")?
                .starts_with(&root)
        {
            return Err("missing or escaping app executable".into());
        }
        check_cancel(cancel)?;
        macho::validate(&executable)?;
    }
    tool(
        "/usr/bin/codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            app.as_os_str(),
        ],
        cancel,
    )?;
    Ok(())
}
/// Bound the expanded tree and reject escaping links/special files without following them.
pub fn expanded_size(app: &Path, cancel: &AtomicBool) -> Result<u64, String> {
    check_cancel(cancel)?;
    if fs::symlink_metadata(app).is_err()
        || fs::symlink_metadata(app).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
    {
        return Err("update is not a regular app bundle".into());
    }
    let root = app
        .canonicalize()
        .map_err(|_| "could not resolve update bundle")?;
    let mut count = 0;
    let mut bytes = 0u64;
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).map_err(|_| "could not inspect update bundle")? {
            check_cancel(cancel)?;
            let path = entry.map_err(|_| "could not inspect update bundle")?.path();
            let meta = fs::symlink_metadata(&path).map_err(|_| "could not inspect update entry")?;
            count += 1;
            bytes = bytes.saturating_add(meta.len());
            if count > 100000 || bytes > 8 * 1024 * 1024 * 1024 {
                return Err("expanded update bundle exceeds size limit".into());
            }
            if meta.file_type().is_symlink() {
                if !path
                    .canonicalize()
                    .map_err(|_| "broken bundle symlink")?
                    .starts_with(&root)
                {
                    return Err("bundle symlink escapes the application".into());
                }
            } else if meta.is_dir() {
                pending.push(path)
            } else if !meta.is_file() {
                return Err("unsupported bundle entry type".into());
            }
        }
    }
    Ok(bytes)
}
/// Fingerprint the verified code seal and its resource envelope, not build time.
/// Final-copy validation must also verify codesign and exact embedded metadata.
pub fn seal(app: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut digest = Sha256::new();
    digest.update(b"Guild bundle seal v1\0");
    for name in [
        "Contents/MacOS/guild-desktop",
        "Contents/MacOS/guild",
        "Contents/_CodeSignature/CodeResources",
    ] {
        let (bytes, hash) = hash_file(&app.join(name), cancel)?;
        digest.update(bytes.to_le_bytes());
        digest.update(hash.as_bytes());
    }
    Ok(hex(&digest.finalize()))
}
/// Copy preserving macOS resources, with bounded cancellation and owned processes.
pub fn copy(source: &Path, target: &Path, cancel: &AtomicBool) -> Result<(), String> {
    tool(
        "/usr/bin/ditto",
        &[source.as_os_str(), target.as_os_str()],
        cancel,
    )
    .map(|_| ())
}
/// Publish a completed build under the one canonical-directory producer lock.
pub fn stage(source: &Path, destination: &Path, cancel: &AtomicBool) -> Result<BuildInfo, String> {
    let mut publication = Publication::prepare(source, destination, cancel)?;
    publication.publish_bundle()?;
    publication.finish()
}
// Keep the producer lock owned across both publication boundaries. The scratch
// directory is dropped first, so no other producer observes cleanup in progress.
struct Publication {
    scratch: tempfile::TempDir,
    app: PathBuf,
    marker: PathBuf,
    build: BuildInfo,
    published: bool,
    _lock: FileLock,
}
impl Publication {
    fn prepare(source: &Path, destination: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        let existing = destination
            .ancestors()
            .find(|p| p.exists())
            .and_then(|p| p.canonicalize().ok());
        if destination
            .components()
            .any(|p| p.as_os_str() == "Applications")
            || existing.is_some_and(|p| p.components().any(|c| c.as_os_str() == "Applications"))
        {
            return Err("choose a staging directory outside Applications".into());
        }
        let lock = FileLock::acquire(destination, ".guild-stage.lock", Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        let destination = destination
            .canonicalize()
            .map_err(|_| "could not resolve staging directory")?;
        if destination
            .components()
            .any(|p| p.as_os_str() == "Applications")
        {
            return Err("choose a staging directory outside Applications".into());
        }
        let source = source
            .canonicalize()
            .map_err(|_| "could not resolve completed build")?;
        if source == destination.join("Guild.app") {
            return Err("build output and completed staging directory must be separate".into());
        }
        let build = info(&source)?;
        validate(&source, Some(&build), cancel)?;
        let app = destination.join("Guild.app");
        let marker = destination.join("guild-local-ready.json");
        if app.exists() {
            let legacy_state = std::env::var_os("GUILD_STATE_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("guild"))
                })
                .or_else(|| {
                    std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/guild"))
                });
            let legacy = legacy_state
                .and_then(|s| s.join("update").canonicalize().ok())
                .is_some_and(|p| p == destination);
            if !legacy {
                let owned = read_small(&marker, 4096)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Ready>(&b).ok())
                    .is_some_and(|m| {
                        m.schema == 1
                            && !m.build_id.is_nil()
                            && m.commit.len() == 40
                            && m.commit.bytes().all(|c| c.is_ascii_hexdigit())
                    });
                // An interrupted first publication has no previous ready marker.
                // Its durable intent proves this tool already owns the exact bundle.
                let pending = read_small(&destination.join("guild-local-pending.json"), 4096)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Ready>(&bytes).ok());
                let pending_owned = pending.is_some_and(|pending| {
                    pending.schema == 1
                        && info(&app).is_ok_and(|current| {
                            current.build_id == pending.build_id && current.commit == pending.commit
                        })
                });
                if !owned && !pending_owned {
                    return Err(
                    "destination contains an unowned Guild.app; choose an empty staging directory"
                        .into(),
                );
                }
            }
            if fs::symlink_metadata(&app).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("staged app may not be a symlink".into());
            }
        }
        let scratch = tempfile::Builder::new()
            .prefix(".guild-stage-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&destination)
            .map_err(|_| "could not prepare staging directory")?;
        let next = scratch.path().join("Guild.app");
        copy(&source, &next, cancel)?;
        validate(&next, Some(&build), cancel)?;
        check_cancel(cancel)?;
        Ok(Self {
            scratch,
            app,
            marker,
            build,
            published: false,
            _lock: lock,
        })
    }
    fn publish_bundle(&mut self) -> Result<(), String> {
        if self.published {
            return Err("Local publication already completed".into());
        }
        let intent = Ready {
            schema: 1,
            build_id: self.build.build_id,
            commit: self.build.commit.clone(),
        };
        publish_json(
            &self.app.parent().unwrap().join("guild-local-pending.json"),
            &intent,
        )?;
        let previous = self.scratch.path().join("previous.app");
        let had = self.app.exists();
        if had {
            fs::rename(&self.app, &previous)
                .map_err(|_| "could not retain previous staged build")?;
        }
        if fs::rename(self.scratch.path().join("Guild.app"), &self.app).is_err() {
            if had {
                let _ = fs::rename(&previous, &self.app);
            }
            return Err("could not publish staged build".into());
        }
        self.published = true;
        Ok(())
    }
    fn finish(self) -> Result<BuildInfo, String> {
        if !self.published {
            return Err("Local bundle has not been published".into());
        }
        let ready = Ready {
            schema: 1,
            build_id: self.build.build_id,
            commit: self.build.commit.clone(),
        };
        // Marker last: a failed publication leaves a detectable identity mismatch.
        publish_json(&self.marker, &ready)?;
        Ok(self.build.clone())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, symlink};
    fn fixture(root: &Path, commit: &str) -> PathBuf {
        let app = root.join("Guild.app");
        let resources = app.join("Contents/Resources");
        let executables = app.join("Contents/MacOS");
        fs::create_dir_all(&resources).unwrap();
        fs::create_dir(&executables).unwrap();
        let build = BuildInfo::local(commit.into());
        fs::write(
            resources.join("build-info.json"),
            serde_json::to_vec(&build).unwrap(),
        )
        .unwrap();
        fs::write(resources.join("commit"), format!("{commit}\n")).unwrap();
        fs::write(resources.join("built"), "0\n").unwrap();
        for name in ["guild", "guild-desktop"] {
            fs::copy(std::env::current_exe().unwrap(), executables.join(name)).unwrap();
        }
        fs::write(app.join("Contents/Info.plist"),r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.guild.desktop</string><key>CFBundleExecutable</key><string>guild-desktop</string><key>CFBundlePackageType</key><string>APPL</string><key>LSMinimumSystemVersion</key><string>13.0</string></dict></plist>"#).unwrap();
        tool(
            "/usr/bin/codesign",
            &[
                "--force".as_ref(),
                "--deep".as_ref(),
                "--sign".as_ref(),
                "-".as_ref(),
                app.as_os_str(),
            ],
            &AtomicBool::new(false),
        )
        .unwrap();
        app
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn recipient_validation_does_not_need_developer_tools() {
        let root = tempfile::tempdir().unwrap();
        let app = fixture(root.path(), &"d".repeat(40));
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "update::bundle::tests::recipient_validation_child",
                "--ignored",
                "--nocapture",
            ])
            .env("GUILD_TEST_BUNDLE", app)
            .env("DEVELOPER_DIR", "/nonexistent-guild-fixture")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "isolated environment owned by recipient_validation_does_not_need_developer_tools"]
    fn recipient_validation_child() {
        let app = PathBuf::from(std::env::var_os("GUILD_TEST_BUNDLE").unwrap());
        validate(&app, Some(&info(&app).unwrap()), &AtomicBool::new(false)).unwrap();
    }
    #[test]
    fn producer_b_cannot_publish_between_a_bundle_and_marker_even_through_alias() {
        let root = tempfile::tempdir().unwrap();
        let a = fixture(&root.path().join("a"), &"a".repeat(40));
        let b = fixture(&root.path().join("b"), &"b".repeat(40));
        let directory = root.path().join("completed");
        let alias = root.path().join("alias");
        let cancel = AtomicBool::new(false);
        stage(&b, &directory, &cancel).unwrap();
        symlink(&directory, &alias).unwrap();
        let lock_path = directory.join(".guild-stage.lock");
        let inode = fs::metadata(&lock_path).unwrap().ino();
        let sentinel = root.path().join("sentinel");
        fs::write(&sentinel, b"untouched").unwrap();
        let mut publication = Publication::prepare(&a, &directory, &cancel).unwrap();
        publication.publish_bundle().unwrap();
        assert!(
            local(&directory).is_err(),
            "readers reject the new bundle with the old marker"
        );
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            scope.spawn(move || {
                started_tx.send(()).unwrap();
                let result = stage(&b, &alias, &cancel);
                done_tx.send(result).unwrap();
            });
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(matches!(
                done_rx.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            assert_eq!(
                info(&directory.join("Guild.app")).unwrap().commit,
                "a".repeat(40)
            );
            publication.finish().unwrap();
            done_rx
                .recv_timeout(Duration::from_secs(15))
                .unwrap()
                .unwrap();
        });
        assert_eq!(local(&directory).unwrap().1.commit, "b".repeat(40));
        assert_eq!(fs::metadata(lock_path).unwrap().ino(), inode);
        assert_eq!(fs::read(sentinel).unwrap(), b"untouched");
    }
    #[test]
    fn failed_first_marker_publication_is_rejected_by_readers_and_repairable() {
        let root = tempfile::tempdir().unwrap();
        let app = fixture(&root.path().join("source"), &"c".repeat(40));
        let directory = root.path().join("completed");
        fs::create_dir(&directory).unwrap();
        let marker = directory.join("guild-local-ready.json");
        fs::create_dir(&marker).unwrap();
        let cancel = AtomicBool::new(false);
        assert!(stage(&app, &directory, &cancel).is_err());
        assert!(local(&directory).is_err());
        assert!(directory.join("Guild.app").is_dir());
        fs::remove_dir(&marker).unwrap();
        stage(&app, &directory, &cancel).unwrap();
        assert_eq!(local(&directory).unwrap().1.commit, "c".repeat(40));
    }
}

/// Copy one exact bounded archive into a new owned regular file.
pub fn copy_archive(
    source: &Path,
    destination: &Path,
    bytes: u64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check_cancel(cancel)?;
    let mut input = open_regular(source)?;
    if bytes == 0
        || bytes > super::MAX_ARCHIVE
        || input
            .metadata()
            .map_err(|_| "could not inspect update archive")?
            .len()
            != bytes
    {
        return Err("archive length does not match the manifest".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .mode(0o600)
        .open(destination)
        .map_err(|_| "archive destination exists or cannot be created")?;
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check_cancel(cancel)?;
        let n = input.read(&mut buffer).map_err(|_| "archive read failed")?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > bytes {
            return Err("archive grew beyond its declared length".into());
        }
        output
            .write_all(&buffer[..n])
            .map_err(|_| "archive copy failed")?;
    }
    if count != bytes {
        return Err("archive became incomplete during copy".into());
    }
    output
        .sync_all()
        .map_err(|_| "could not sync copied archive".to_string())
}
/// Check the target filesystem before extracting/preparing, including metadata headroom.
pub fn ensure_space(directory: &Path, bytes: u64) -> Result<(), String> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let required = bytes
        .checked_add(64 * 1024 * 1024)
        .ok_or("update space requirement is too large")?;
    let directory =
        CString::new(directory.as_os_str().as_bytes()).map_err(|_| "invalid update directory")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: valid NUL-terminated path and writable storage for statvfs's output.
    if unsafe { libc::statvfs(directory.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err("could not inspect available update space".into());
    }
    // SAFETY: a successful statvfs initializes its entire output structure.
    let stats = unsafe { stats.assume_init() };
    let block = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };
    let available = (stats.f_bavail as u128) * (block as u128);
    if available < required as u128 {
        return Err("not enough free space for this update".into());
    }
    Ok(())
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn exact_archive_copy_refuses_mismatch_overwrite_and_insufficient_space() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.zip");
        let target = dir.path().join("копия.zip");
        fs::write(&source, b"archive bytes").unwrap();
        let cancel = AtomicBool::new(false);
        copy_archive(&source, &target, 13, &cancel).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"archive bytes");
        assert!(copy_archive(&source, &target, 13, &cancel).is_err());
        let mismatch = dir.path().join("wrong.zip");
        assert!(copy_archive(&source, &mismatch, 2, &cancel).is_err());
        assert!(!mismatch.exists());
        assert!(ensure_space(dir.path(), u64::MAX).is_err());
        assert!(ensure_space(dir.path(), 0).is_ok());
    }
    #[test]
    fn fifo_metadata_child() {
        let Some(path) = std::env::var_os("GUILD_TEST_FIFO_READ") else {
            return;
        };
        let path = Path::new(&path);
        let result = match std::env::var("GUILD_TEST_FIFO_KIND").as_deref() {
            Ok("preferences") => {
                super::super::PreferenceStore::new(path.parent().unwrap().to_path_buf())
                    .load()
                    .map(|_| ())
            }
            Ok("archive") => hash_file(path, &AtomicBool::new(false)).map(|_| ()),
            Ok("key") => super::super::publisher::sign(
                &path.parent().unwrap().join("guild-update.json"),
                &path.parent().unwrap().join("Guild.zip"),
                path,
                "fixture",
                &path.parent().unwrap().join("signed"),
            ),
            _ => read_small(path, 1024).map(|_| ()),
        };
        assert!(result.is_err());
    }
    #[test]
    fn special_metadata_is_rejected_without_waiting_for_a_writer() {
        for kind in ["metadata", "preferences", "archive", "key"] {
            use std::{ffi::CString, os::unix::ffi::OsStrExt};
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(if kind == "preferences" {
                "update-preferences.json"
            } else {
                "metadata"
            });
            let name = CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: a valid owned temporary pathname and ordinary private FIFO mode.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            let mut build = super::super::BuildInfo::local("1".repeat(40));
            build.tag = Some("v1.2.0".into());
            build.channel = Some(super::super::Channel::Stable);
            build.revision_order = Some(1);
            build.provenance = super::super::Provenance::GithubActions;
            build.ci = Some(super::super::CiIdentity {
                repository: "owner/guild".into(),
                run_id: 1,
                attempt: 1,
            });
            let manifest = super::super::Manifest {
                schema: 1,
                app_id: APP_ID.into(),
                repository: "owner/guild".into(),
                build,
                archive: "Guild.zip".into(),
                bytes: 13,
                sha256: "0".repeat(64),
                notes: String::new(),
            };
            fs::write(
                dir.path().join("guild-update.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "update::bundle::file_tests::fifo_metadata_child"])
                .env("GUILD_TEST_FIFO_READ", &path)
                .env("GUILD_TEST_FIFO_KIND", kind)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut process = ProcessGroup::spawn(&mut command).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut finished = None;
            while Instant::now() < deadline {
                if let Some(status) = process.try_wait().unwrap() {
                    finished = Some(status);
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            process.stop().unwrap();
            assert!(
                finished.is_some_and(|status| status.success()),
                "regular metadata validation must not block opening a FIFO"
            );
        }
    }
}
