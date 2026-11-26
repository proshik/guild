//! Inspect ZIP paths, sizes and links BEFORE extracting an authenticated archive.
//! Internal package paths are ASCII to avoid filesystem Unicode normalization aliases.
//! The outer download, Local staging and extraction directories may contain Unicode.
use super::{BuildInfo, Manifest, bundle};
mod limit;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::atomic::AtomicBool,
};
/// Inspect the entire central directory and the embedded identity under hard limits.
pub fn inspect(path: &Path, expected: &Manifest, cancel: &AtomicBool) -> Result<(), String> {
    inspect_inner(path, expected, cancel).map(|_| ())
}
fn inspect_inner(path: &Path, expected: &Manifest, cancel: &AtomicBool) -> Result<u64, String> {
    if std::fs::metadata(path)
        .map_err(|_| "archive unavailable")?
        .len()
        != expected.bytes
    {
        return Err("archive length does not match the manifest".into());
    }
    if expected.bytes == 0 || expected.bytes > super::MAX_ARCHIVE {
        return Err("archive exceeds compressed size limit".into());
    }
    let file = bundle::open_regular(path)?;
    let reader = limit::Reader::new(file, cancel);
    let checking = reader.checking.clone();
    let mut zip = zip::ZipArchive::with_config(
        zip::read::Config {
            archive_offset: zip::read::ArchiveOffset::Known(0),
        },
        reader,
    )
    .map_err(|_| "invalid or oversized ZIP update directory")?;
    checking.set(false);
    if zip.len() > 100000 {
        return Err("too many archive entries".into());
    }
    let mut names = BTreeSet::new();
    let mut nondirectories = BTreeSet::new();
    let mut links = BTreeMap::new();
    let mut total = 0u64;
    let mut embedded = None;
    for n in 0..zip.len() {
        bundle::check_cancel(cancel)?;
        let mut entry = zip.by_index(n).map_err(|_| "unreadable ZIP directory")?;
        let name = entry.name().to_owned();
        let cleaned = name.trim_end_matches('/');
        if !name.is_ascii()
            || cleaned.is_empty()
            || name.contains("//")
            || name.contains(['\\', '\0', ':'])
            || name.split('/').any(|p| p == "." || p == "..")
            || Path::new(&name).components().any(|c| {
                matches!(
                    c,
                    Component::RootDir | Component::ParentDir | Component::Prefix(_)
                )
            })
        {
            return Err("unsafe archive entry path".into());
        }
        if !names.insert(cleaned.to_lowercase()) {
            return Err("duplicate or case-colliding archive entry".into());
        }
        let app = cleaned == "Guild.app" || cleaned.starts_with("Guild.app/");
        let metadata = cleaned == "__MACOSX"
            || cleaned == "__MACOSX/._Guild.app"
            || cleaned == "__MACOSX/Guild.app"
            || cleaned.starts_with("__MACOSX/Guild.app/");
        if !app && !metadata {
            return Err("unexpected content outside Guild.app".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("expanded archive too large")?;
        if total > 8 * 1024 * 1024 * 1024 {
            return Err("expanded archive exceeds size limit".into());
        }
        let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
        if ![0, 0o100000, 0o040000, 0o120000].contains(&kind) {
            return Err("unsupported ZIP entry type".into());
        }
        if (kind == 0o040000) != entry.is_dir() && kind != 0 {
            return Err("inconsistent ZIP entry type".into());
        }
        if !entry.is_dir() {
            nondirectories.insert(PathBuf::from(cleaned.to_ascii_lowercase()));
        }
        let declared = entry.size();
        let mut data = Vec::new();
        if kind == 0o120000 || cleaned == "Guild.app/Contents/Resources/build-info.json" {
            let limit = if kind == 0o120000 {
                4096
            } else {
                super::MAX_MANIFEST
            };
            if declared > limit as u64 {
                return Err("oversized archive metadata".into());
            }
            (&mut entry)
                .take(limit as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|_| "corrupt archive metadata")?;
            if data.len() as u64 != declared {
                return Err("archive metadata length mismatch".into());
            }
        } else {
            let mut actual = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                bundle::check_cancel(cancel)?;
                let n = entry
                    .read(&mut buffer)
                    .map_err(|_| "corrupt archive entry")?;
                if n == 0 {
                    break;
                }
                actual += n as u64;
                if actual > declared {
                    return Err("archive entry exceeds declared size".into());
                }
            }
            if actual != declared {
                return Err("archive entry length mismatch".into());
            }
        }
        if kind == 0o120000 {
            if !app || entry.size() > 4096 {
                return Err("unsupported archive symlink".into());
            }
            let target = String::from_utf8(data).map_err(|_| "invalid symlink target")?;
            if !target.is_ascii()
                || target.is_empty()
                || target.len() > 4096
                || target.contains(['\\', '\0', ':'])
                || Path::new(&target).is_absolute()
            {
                return Err("unsafe archive symlink".into());
            }
            let mut resolved = PathBuf::from(cleaned).parent().unwrap().to_path_buf();
            for component in Path::new(&target).components() {
                match component {
                    Component::ParentDir => {
                        if !resolved.pop() {
                            return Err("escaping archive symlink".into());
                        }
                    }
                    Component::Normal(p) => resolved.push(p),
                    Component::CurDir => {}
                    _ => return Err("unsafe archive symlink".into()),
                }
            }
            if !resolved.starts_with("Guild.app") {
                return Err("archive symlink escapes Guild.app".into());
            }
            links.insert(
                PathBuf::from(cleaned.to_ascii_lowercase()),
                PathBuf::from(resolved.to_string_lossy().to_ascii_lowercase()),
            );
        } else if cleaned == "Guild.app/Contents/Resources/build-info.json" {
            if entry.size() > super::MAX_MANIFEST as u64 {
                return Err("embedded identity too large".into());
            }
            if data.len() > super::MAX_MANIFEST {
                return Err("embedded identity too large".into());
            }
            embedded = Some(
                serde_json::from_slice::<BuildInfo>(&data)
                    .map_err(|_| "invalid embedded build identity")?,
            );
        }
    }
    for (link, target) in &links {
        if links
            .keys()
            .any(|other| target.starts_with(other) || (link != other && link.starts_with(other)))
        {
            return Err("chained or nested archive symlink refused".into());
        }
    }
    for name in &names {
        let path = PathBuf::from(name);
        if path.ancestors().skip(1).any(|p| nondirectories.contains(p)) {
            return Err("archive entry descends through a non-directory".into());
        }
        if links
            .keys()
            .any(|link| path != *link && path.starts_with(link))
        {
            return Err("archive entry descends through a symlink".into());
        }
    }
    if embedded.as_ref() != Some(&expected.build) {
        return Err("archive build identity does not match the signed manifest".into());
    }
    Ok(total)
}
/// Sanitized extraction failures for consistent CLI and localized app reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Invalid archive layout, payload identity, seal or system compatibility.
    Invalid,
    /// Not enough available space on the extraction filesystem.
    Space,
    /// An owned filesystem or extraction-tool operation failed.
    Io,
    /// The operation was cancelled.
    Cancelled,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid or incompatible update bundle",
            Self::Space => "not enough free update space",
            Self::Io => "could not extract update bundle",
            Self::Cancelled => "update cancelled",
        })
    }
}
impl std::error::Error for Failure {}
/// Extract only after authentication and full path/size inspection into a fresh owned folder.
pub fn extract(
    path: &Path,
    destination: &Path,
    manifest: &Manifest,
    cancel: &AtomicBool,
) -> Result<PathBuf, Failure> {
    let classify = |failure| {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            Failure::Cancelled
        } else {
            failure
        }
    };
    let expanded = inspect_inner(path, manifest, cancel).map_err(|_| classify(Failure::Invalid))?;
    bundle::ensure_space(destination.parent().ok_or(Failure::Io)?, expanded)
        .map_err(|_| Failure::Space)?;
    if destination.exists() {
        return Err(Failure::Io);
    }
    std::fs::create_dir(destination).map_err(|_| Failure::Io)?;
    bundle::tool(
        "/usr/bin/ditto",
        &[
            "-x".as_ref(),
            "-k".as_ref(),
            path.as_os_str(),
            destination.as_os_str(),
        ],
        cancel,
    )
    .map_err(|_| classify(Failure::Io))?;
    let app = destination.join("Guild.app");
    bundle::validate(&app, Some(&manifest.build), cancel)
        .map_err(|_| classify(Failure::Invalid))?;
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, io::Write};
    fn archive(extra: &[(&str, &[u8], bool)]) -> (tempfile::TempDir, PathBuf, Manifest) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Guild.zip");
        let build = BuildInfo::local("1".repeat(40));
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("Guild.app/Contents/Resources/build-info.json", options)
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&build).unwrap())
            .unwrap();
        for (name, data, link) in extra {
            if *link {
                writer
                    .add_symlink(*name, std::str::from_utf8(data).unwrap(), options)
                    .unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
        }
        writer.finish().unwrap();
        let manifest = Manifest {
            schema: 1,
            app_id: super::super::APP_ID.into(),
            repository: "owner/guild".into(),
            build,
            archive: "Guild.zip".into(),
            bytes: std::fs::metadata(&path).unwrap().len(),
            sha256: "0".repeat(64),
            notes: String::new(),
        };
        (dir, path, manifest)
    }
    #[test]
    fn refuses_entries_through_case_aliased_symlinks() {
        let (_dir, path, manifest) = archive(&[
            ("Guild.app/Contents/Link", b"Resources", true),
            ("Guild.app/Contents/link/injected", b"bad", false),
        ]);
        assert!(inspect(&path, &manifest, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn refuses_unicode_filesystem_aliases_before_extraction() {
        let (_dir, path, manifest) = archive(&[
            ("Guild.app/Contents/caf\u{e9}", b"a", false),
            ("Guild.app/Contents/cafe\u{301}", b"b", false),
        ]);
        assert!(inspect(&path, &manifest, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn accepts_package_metadata_and_internal_link_but_rejects_unsafe_layouts() {
        let (_dir, path, manifest) = archive(&[
            ("__MACOSX/._Guild.app", b"resource fork", false),
            ("Guild.app/Contents/alias", b"Resources", true),
        ]);
        inspect(&path, &manifest, &AtomicBool::new(false)).unwrap();
        for entries in [
            vec![("Guild.app/../outside", b"bad".as_slice(), false)],
            vec![(
                "Guild.app/Contents/link",
                b"../../../outside".as_slice(),
                true,
            )],
            vec![
                ("Guild.app/Contents/a", b"file".as_slice(), false),
                ("Guild.app/Contents/A/child", b"bad".as_slice(), false),
            ],
            vec![
                ("Guild.app/Contents/a", b"b".as_slice(), true),
                ("Guild.app/Contents/b", b"a".as_slice(), true),
            ],
            vec![("outside", b"bad".as_slice(), false)],
        ] {
            let (_dir, path, manifest) = archive(&entries);
            assert!(
                inspect(&path, &manifest, &AtomicBool::new(false)).is_err(),
                "{entries:?}"
            );
        }
        let (_dir, path, mut manifest) = archive(&[]);
        manifest.build.commit = "2".repeat(40);
        assert!(inspect(&path, &manifest, &AtomicBool::new(false)).is_err());
        assert!(inspect(&path, &manifest, &AtomicBool::new(true)).is_err());
    }
}
