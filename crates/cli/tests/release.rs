//! `guild-release`, what `just release` asks — M5.16. Run as a program:
//! the recipe reads its exit code and its words and nothing else.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn tool(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_guild-release"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("guild-release starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn said(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn scratch(name: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("gr-{name}-"))
        .tempdir()
        .expect("a scratch folder")
}

const CHANGELOG: &str = "\
# Changelog

## [Unreleased]

### Fixed

- A fix.

## [nightly-2026-09-23] - 2026-09-23

### Added

- Old.
";

/// The tags come in on stdin, one a line, as `git tag -l` prints them,
/// and the changelog is read for the versions it has released.
#[test]
fn the_name_is_the_date_or_the_date_and_the_commit() {
    let dir = scratch("name");
    let file = dir.path().join("CHANGELOG.md");
    std::fs::write(&file, CHANGELOG).unwrap();
    let file = file.to_str().unwrap();

    let first = tool(
        &["name", "2026-09-24", "2bca70a", file],
        "nightly-2026-09-23\n",
    );
    assert!(first.status.success(), "{:?}", said(&first));
    assert_eq!(said(&first).0, "nightly-2026-09-24\n");

    let second = tool(
        &["name", "2026-09-24", "2bca70a", file],
        "nightly-2026-09-23\nnightly-2026-09-24\n",
    );
    assert!(second.status.success(), "{:?}", said(&second));
    assert_eq!(said(&second).0, "nightly-2026-09-24-2bca70a\n");

    let taken = tool(
        &["name", "2026-09-24", "2bca70a", file],
        "nightly-2026-09-24\nnightly-2026-09-24-2bca70a\n",
    );
    assert_eq!(taken.status.code(), Some(1));
    assert!(said(&taken).1.contains("nightly-2026-09-24-2bca70a"));
}

/// A repository whose history was started afresh has no tags for the
/// releases its changelog carries: a heading takes a name as a tag does.
#[test]
fn a_version_the_changelog_holds_is_taken_without_its_tag() {
    let dir = scratch("name-heading");
    let file = dir.path().join("CHANGELOG.md");
    std::fs::write(&file, CHANGELOG).unwrap();
    let file = file.to_str().unwrap();

    let out = tool(&["name", "2026-09-23", "2bca70a", file], "");
    assert!(out.status.success(), "{:?}", said(&out));
    assert_eq!(said(&out).0, "nightly-2026-09-23-2bca70a\n");

    let taken = tool(
        &["name", "2026-09-23", "2bca70a", file],
        "nightly-2026-09-23-2bca70a\n",
    );
    assert_eq!(taken.status.code(), Some(1));
    assert!(
        said(&taken).1.contains("nightly-2026-09-23-2bca70a"),
        "{:?}",
        said(&taken)
    );
}

/// Without the changelog the name could be one it already holds: the
/// old call is a usage error, not a guess from the tags alone.
#[test]
fn the_name_is_not_chosen_without_the_changelog() {
    let out = tool(&["name", "2026-09-24", "2bca70a"], "");

    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).1.contains("usage"), "{:?}", said(&out));
}

#[test]
fn check_says_whether_there_is_something_to_release() {
    let dir = scratch("check");
    let file = dir.path().join("CHANGELOG.md");

    std::fs::write(&file, CHANGELOG).unwrap();
    let ready = tool(&["check", file.to_str().unwrap()], "");
    assert!(ready.status.success(), "{:?}", said(&ready));

    std::fs::write(&file, CHANGELOG.replace("- A fix.\n", "")).unwrap();
    let empty = tool(&["check", file.to_str().unwrap()], "");
    assert_eq!(empty.status.code(), Some(1));
    assert!(said(&empty).1.contains("no entry"), "{:?}", said(&empty));
}

#[test]
fn cut_rewrites_the_file_in_place() {
    let dir = scratch("cut");
    let file = dir.path().join("CHANGELOG.md");
    std::fs::write(&file, CHANGELOG).unwrap();

    let out = tool(
        &[
            "cut",
            file.to_str().unwrap(),
            "nightly-2026-09-24",
            "2026-09-24",
        ],
        "",
    );

    assert!(out.status.success(), "{:?}", said(&out));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        CHANGELOG.replace(
            "## [Unreleased]\n",
            "## [Unreleased]\n\n## [nightly-2026-09-24] - 2026-09-24\n"
        )
    );

    let again = tool(
        &[
            "cut",
            file.to_str().unwrap(),
            "nightly-2026-09-25",
            "2026-09-25",
        ],
        "",
    );
    assert_eq!(again.status.code(), Some(1), "Unreleased is empty now");
}

#[test]
fn a_wrong_call_is_a_usage_error() {
    let out = tool(&["release", "everything"], "");

    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).1.contains("usage"), "{:?}", said(&out));
}

#[test]
fn exact_stable_build_metadata_is_written_by_the_release_binary() {
    let dir = scratch("build-info");
    let path = dir.path().join("build-info.json");
    let out = tool(
        &[
            "build-info",
            path.to_str().unwrap(),
            "1111111111111111111111111111111111111111",
            "v1.2.0",
            "50",
            "clean",
            "1790000000",
            "owner/guild",
            "123",
            "1",
        ],
        "",
    );
    assert!(out.status.success(), "{:?}", said(&out));
    let info: guild_session::update::BuildInfo =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(info.channel, Some(guild_session::update::Channel::Stable));
    assert_eq!(info.revision_order, Some(50));
    assert_eq!(info.commit, "1111111111111111111111111111111111111111");
    assert_eq!(info.ci.unwrap().run_id, 123);
    let bad = dir.path().join("bad.json");
    let out = tool(
        &[
            "build-info",
            bad.to_str().unwrap(),
            "1111111111111111111111111111111111111111",
            "v1.2.0",
            "50",
            "dirty",
            "1790000000",
            "owner/guild",
            "123",
            "1",
        ],
        "",
    );
    assert!(!out.status.success());
    assert!(!bad.exists(), "dirty CI metadata must not be published");
}

#[cfg(target_os = "macos")]
fn local_app(root: &std::path::Path, commit: &str) -> std::path::PathBuf {
    let app = root.join("Guild.app");
    let contents = app.join("Contents");
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    for name in ["guild", "guild-desktop"] {
        std::fs::copy(
            std::env::current_exe().unwrap(),
            contents.join("MacOS").join(name),
        )
        .unwrap();
    }
    std::fs::write(contents.join("Info.plist"), r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.guild.desktop</string><key>CFBundleExecutable</key><string>guild-desktop</string><key>CFBundlePackageType</key><string>APPL</string><key>LSMinimumSystemVersion</key><string>13.0</string></dict></plist>"#).unwrap();
    let out = tool(
        &[
            "build-info",
            resources.join("build-info.json").to_str().unwrap(),
            commit,
            "-",
            "-",
            "clean",
            "1790000000",
            "-",
            "-",
            "-",
        ],
        "",
    );
    assert!(out.status.success(), "{:?}", said(&out));
    std::fs::write(resources.join("commit"), format!("{commit}\n")).unwrap();
    std::fs::write(resources.join("built"), "1790000000\n").unwrap();
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    app
}

#[test]
#[cfg(target_os = "macos")]
fn completed_untagged_local_stage_preserves_previous_on_invalid_replacement() {
    use guild_session::update::bundle;
    let root = scratch("stage");
    let source = local_app(&root.path().join("source"), &"2".repeat(40));
    let destination = root.path().join("готовая сборка");
    let out = tool(
        &[
            "stage",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
        "",
    );
    assert!(out.status.success(), "{:?}", said(&out));
    let (_, before) = bundle::local(&destination).unwrap();
    assert_eq!(before.tag, None);
    let broken = local_app(&root.path().join("broken"), &"3".repeat(40));
    std::fs::write(broken.join("Contents/MacOS/guild"), "corrupt executable").unwrap();
    let out = tool(
        &[
            "stage",
            broken.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
        "",
    );
    assert!(!out.status.success());
    assert_eq!(bundle::local(&destination).unwrap().1, before);
    let protected = root.path().join("Applications/unused");
    let out = tool(
        &[
            "stage",
            source.to_str().unwrap(),
            protected.to_str().unwrap(),
        ],
        "",
    );
    assert!(!out.status.success());
    assert!(!protected.exists());
}

#[test]
fn stage_lock_holder_child() {
    let Some(directory) = std::env::var_os("GUILD_TEST_STAGE_LOCK") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let _lock = guild_session::update::lock::FileLock::acquire(
        &directory,
        ".guild-stage.lock",
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    std::fs::write(directory.join("holder-ready"), b"ready").unwrap();
    // The parent terminates this isolated test process, releasing the OS lock.
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[test]
#[cfg(target_os = "macos")]
fn canonical_alias_producers_serialize_and_continue_after_holder_termination() {
    use guild_session::{process::ProcessGroup, update::bundle};
    use std::{
        os::unix::fs::symlink,
        time::{Duration, Instant},
    };
    let root = scratch("stage-writers");
    let a = local_app(&root.path().join("a"), &"a".repeat(40));
    let b = local_app(&root.path().join("b"), &"b".repeat(40));
    let directory = root.path().join("completed");
    std::fs::create_dir(&directory).unwrap();
    let alias = root.path().join("alias");
    symlink(&directory, &alias).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "stage_lock_holder_child"])
        .env("GUILD_TEST_STAGE_LOCK", &directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut holder = ProcessGroup::spawn(&mut command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !directory.join("holder-ready").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let spawn = |source: &std::path::Path, dest: &std::path::Path| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_guild-release"));
        cmd.arg("stage")
            .arg(source)
            .arg(dest)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ProcessGroup::spawn(&mut cmd).unwrap()
    };
    let mut writer_a = spawn(&a, &directory);
    let mut writer_b = spawn(&b, &alias);
    std::thread::sleep(Duration::from_millis(100));
    assert!(writer_a.try_wait().unwrap().is_none());
    assert!(writer_b.try_wait().unwrap().is_none());
    assert!(!directory.join("guild-local-ready.json").exists());
    holder.stop().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    for writer in [&mut writer_a, &mut writer_b] {
        loop {
            if let Some(status) = writer.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let (_, build) = bundle::local(&directory).unwrap();
    assert!(["a".repeat(40), "b".repeat(40)].contains(&build.commit));
    bundle::validate(
        &directory.join("Guild.app"),
        Some(&build),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
}

#[test]
fn exact_release_tag_is_selected_without_nightly_preference() {
    let tags = "nightly-2026-10-18\nv1.2.0\ndogfood-2026-10-18\n";
    let stable = tool(&["select-tag", "v1.2.0"], tags);
    assert!(stable.status.success(), "{:?}", said(&stable));
    assert_eq!(said(&stable).0, "v1.2.0\n");
    let nightly = tool(&["select-tag", "nightly-2026-10-18"], tags);
    assert!(nightly.status.success());
    assert_eq!(said(&nightly).0, "nightly-2026-10-18\n");
    assert!(
        !tool(&["select-tag", "-"], tags).status.success(),
        "ambiguous release identity needs an explicit tag"
    );
    assert!(!tool(&["select-tag", "v9.9.9"], tags).status.success());
    assert_eq!(said(&tool(&["select-tag", "-"], "")).0, "-\n");
}

#[test]
fn bundle_script_preserves_selected_tag_and_proves_main_first_parent_in_ci() {
    let root = scratch("identity-git");
    let repo = root.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "fixture@example.invalid"]);
    git(&["config", "user.name", "Fixture"]);
    git(&["commit", "--allow-empty", "-m", "initial"]);
    git(&["tag", "v1.2.0"]);
    git(&["tag", "nightly-2026-10-18"]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/write-build-info.sh");
    let invoke = |name: &str, tag: &str, ci: bool| {
        let resources = root.path().join(name);
        std::fs::create_dir(&resources).unwrap();
        let out = Command::new("bash")
            .arg(&script)
            .arg(&resources)
            .arg(env!("CARGO_BIN_EXE_guild-release"))
            .current_dir(&repo)
            .env("RELEASE_TAG", tag)
            .env("GITHUB_ACTIONS", if ci { "true" } else { "false" })
            .env("GITHUB_REPOSITORY", "owner/guild")
            .env("GITHUB_RUN_ID", "123")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .output()
            .unwrap();
        (resources, out)
    };
    let (resources, out) = invoke("stable", "v1.2.0", true);
    assert!(out.status.success(), "{:?}", said(&out));
    let info: guild_session::update::BuildInfo =
        serde_json::from_slice(&std::fs::read(resources.join("build-info.json")).unwrap()).unwrap();
    assert_eq!(info.tag.as_deref(), Some("v1.2.0"));
    assert_eq!(info.revision_order, Some(1));
    assert_eq!(
        std::fs::read_to_string(resources.join("built"))
            .unwrap()
            .trim(),
        info.built.to_string()
    );
    std::fs::write(repo.join("untracked-source.rs"), "dirty").unwrap();
    let (_, out) = invoke("dirty-ci", "v1.2.0", true);
    assert!(!out.status.success());
    let (resources, out) = invoke("dirty-local", "v1.2.0", false);
    assert!(out.status.success(), "{:?}", said(&out));
    let info: guild_session::update::BuildInfo =
        serde_json::from_slice(&std::fs::read(resources.join("build-info.json")).unwrap()).unwrap();
    assert!(info.dirty);
    std::fs::remove_file(repo.join("untracked-source.rs")).unwrap();
    git(&["switch", "-c", "side"]);
    git(&["commit", "--allow-empty", "-m", "side"]);
    let side = git(&["rev-parse", "HEAD"]);
    git(&["tag", "v1.3.0"]);
    git(&["switch", "main"]);
    git(&["commit", "--allow-empty", "-m", "main"]);
    git(&["merge", "--no-ff", "side", "-m", "merge"]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(&["checkout", "--detach", &side]);
    let (_, out) = invoke("side-ci", "v1.3.0", true);
    assert!(
        !out.status.success(),
        "merge-reachable side commit is not main first-parent"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn owner_signing_preflights_exact_ci_payload_and_refuses_insecure_key_or_overwrite() {
    use guild_session::update::{self, Channel, CiIdentity, Provenance, bundle};
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("publisher");
    let app = local_app(&root.path().join("source"), &"4".repeat(40));
    let resources = app.join("Contents/Resources");
    let mut build = bundle::info(&app).unwrap();
    build.tag = Some("v1.2.0".into());
    build.channel = Some(Channel::Stable);
    build.revision_order = Some(50);
    build.provenance = Provenance::GithubActions;
    build.ci = Some(CiIdentity {
        repository: "owner/guild".into(),
        run_id: 123,
        attempt: 1,
    });
    std::fs::write(
        resources.join("build-info.json"),
        serde_json::to_vec(&build).unwrap(),
    )
    .unwrap();
    std::fs::write(resources.join("tag"), "v1.2.0\n").unwrap();
    std::fs::write(
        resources.join("CHANGELOG.md"),
        "# Changelog\n\n## [v1.2.0] - 2026-10-18\n\n### Added\n\n- Fixture release.\n",
    )
    .unwrap();
    let key = update::SigningKey::from_bytes(&[7; 32]);
    let public = serde_json::to_vec(&std::collections::BTreeMap::from([(
        "fixture",
        update::hex(key.verifying_key().as_bytes()),
    )]))
    .unwrap();
    std::fs::write(resources.join("update-keys.json"), &public).unwrap();
    let out = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(out.status.success());
    let zip = root.path().join("Guild-v1.2.0-macos-arm64.zip");
    let out = Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        .arg(&app)
        .arg(&zip)
        .output()
        .unwrap();
    assert!(out.status.success());
    let manifest = root.path().join("guild-update.json");
    let out = tool(
        &[
            "manifest",
            app.to_str().unwrap(),
            zip.to_str().unwrap(),
            "owner/guild",
            manifest.to_str().unwrap(),
        ],
        "",
    );
    assert!(out.status.success(), "{:?}", said(&out));
    let published: update::Manifest =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let sections =
        guild_session::changelog::parse(&format!("## [Unreleased]\n{}", published.notes));
    assert_eq!(
        guild_session::changelog::merged(&sections.iter().collect::<Vec<_>>()),
        vec![(
            guild_session::changelog::Group::Added,
            vec!["Fixture release.".into()]
        )],
        "publisher notes must reach the shared Update card parser"
    );
    let keyfile = root.path().join("fixture-private-key");
    std::fs::write(&keyfile, [7; 32]).unwrap();
    std::fs::set_permissions(&keyfile, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = root.path().join("signed");
    let args = [
        "sign",
        manifest.to_str().unwrap(),
        zip.to_str().unwrap(),
        keyfile.to_str().unwrap(),
        "fixture",
        output.to_str().unwrap(),
    ];
    let out = tool(&args, "");
    assert!(!out.status.success());
    assert!(!output.exists());
    assert!(!said(&out).1.contains(&update::hex(&[7; 32])));
    std::fs::set_permissions(&keyfile, std::fs::Permissions::from_mode(0o600)).unwrap();
    let public_output = tool(&["public-key", keyfile.to_str().unwrap(), "fixture"], "");
    assert!(public_output.status.success(), "{:?}", said(&public_output));
    assert_eq!(
        update::parse_keys(&public_output.stdout)
            .unwrap()
            .get("fixture"),
        Some(key.verifying_key().as_bytes())
    );
    assert!(!said(&public_output).1.contains(&update::hex(&[7; 32])));
    let out = tool(&args, "");
    assert!(out.status.success(), "{:?}", said(&out));
    assert!(
        !tool(&args, "").status.success(),
        "published signatures cannot be overwritten"
    );
    let keys = root.path().join("keys.json");
    std::fs::write(&keys, public).unwrap();
    let out = tool(
        &["verify", output.to_str().unwrap(), keys.to_str().unwrap()],
        "",
    );
    assert!(out.status.success(), "{:?}", said(&out));
    std::fs::write(output.join(zip.file_name().unwrap()), "corrupt").unwrap();
    assert!(
        !tool(
            &["verify", output.to_str().unwrap(), keys.to_str().unwrap()],
            ""
        )
        .status
        .success()
    );
}
