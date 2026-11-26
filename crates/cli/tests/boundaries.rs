//! Run the actual dependency guard with a controlled Cargo answer.
use std::{os::unix::fs::PermissionsExt, process::Command};

fn check(body: &str) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let cargo = dir.path().join("cargo");
    std::fs::write(&cargo, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(dir.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    Command::new("just")
        .arg("boundaries")
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env("PATH", path)
        .output()
        .unwrap()
}
#[test]
fn dependency_discovery_failure_is_not_a_clean_boundary_check() {
    let output = check("echo 'fixture dependency discovery failed' >&2; exit 42");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("fixture dependency discovery failed")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("boundaries hold"));
}
#[test]
fn a_forbidden_dependency_at_the_start_of_a_large_tree_is_not_hidden_by_sigpipe() {
    let output = check(
        "echo 'gpui v0.2.2'; i=0; while [ $i -lt 20000 ]; do echo 'other-dependency v1.0'; i=$((i+1)); done",
    );
    assert!(!output.status.success());
}
#[test]
fn review_is_headless_and_clean_dependency_trees_pass() {
    let output =
        check(r#"case "$*" in *guild-session*) echo 'gpui v0.2.2';; *) echo 'serde v1';; esac"#);
    assert!(!output.status.success());
    assert!(check("echo 'serde v1'").status.success());
}
