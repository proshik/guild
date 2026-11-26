//! Saving preserves filesystem identity as well as bytes.
use guild_desktop::{file_tab::save_atomically, highlight::LineEnding};
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn saving_a_relative_symlink_preserves_the_link_and_target_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    std::fs::write(&target, "old").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink("target", &link).unwrap();
    let record = save_atomically(&link, "new\n", LineEnding::Crlf).unwrap();
    assert!(link.is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap().to_str(), Some("target"));
    assert_eq!(std::fs::read(&target).unwrap(), b"new\r\n");
    assert_eq!(
        std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(record.matches(&link));
}

#[test]
fn saving_a_dangling_link_refuses_without_replacing_it() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("link");
    symlink("missing", &link).unwrap();
    assert!(save_atomically(&link, "new", LineEnding::Lf).is_err());
    assert!(link.is_symlink());
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn a_failed_save_preserves_existing_contents_and_cleans_its_draft() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("directory");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "original").unwrap();
    assert!(save_atomically(&target, "new", LineEnding::Lf).is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("keep")).unwrap(),
        "original"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
