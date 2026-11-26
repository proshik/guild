//! The hunk writes against a real git — decision 55. The window computes
//! the index entry's new content; git swaps it in only over the entry it
//! was computed from, under its own `index.lock`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::scratch::Scratch;
use guild_desktop::git::{self, Hunk, Settled, Write};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository with `name` committed as `text`.
fn repository(name: &str, text: &str) -> Scratch {
    let dir = Scratch::new("guild-hunk-");
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "a@b"]);
    git(&dir, &["config", "user.name", "a"]);
    // Nothing converts bytes here unless a test says so, whatever the
    // person's own git config says — review r3 `docs+tests-7`. A
    // repository's config is read last, so it wins over the global and the
    // system ones for both keys.
    git(&dir, &["config", "core.autocrlf", "false"]);
    git(&dir, &["config", "core.attributesFile", "/dev/null"]);
    std::fs::write(dir.join(name), text).unwrap();
    git(&dir, &["add", "--", name]);
    git(&dir, &["commit", "-qm", "init"]);
    dir
}

fn index_text(dir: &Path, name: &str) -> String {
    git(dir, &["show", &format!(":{name}")])
}

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn stage(name: &str, pre: &str, post: &str, disk: Option<&str>) -> Write {
    Write::StageHunk(Hunk {
        path: PathBuf::from(name),
        preimage: bytes(pre),
        postimage: bytes(post),
        disk: disk.map(bytes),
        head: bytes(pre),
    })
}

/// Settle, then write: what the git thread does.
fn run(dir: &Path, write: Write) -> Result<(), String> {
    match git::settle(dir, write) {
        Settled::Ready(write) => git::write(dir, &write)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        Settled::Stale(_) => Err("stale".into()),
        Settled::Converted(_) => Err("converted".into()),
        Settled::WholeFile(_) => Err("whole".into()),
        other => Err(format!("{other:?}")),
    }
}

#[test]
fn the_argv_is_apply_cached_reading_stdin_and_nothing_else() {
    assert_eq!(
        stage("f", "a\n", "b\n", None).args(),
        ["apply", "--cached", "--no-3way", "--whitespace=nowarn", "-"]
    );
}

#[test]
fn a_stage_puts_the_postimage_in_the_index_and_leaves_the_disk_and_the_mode() {
    let dir = repository("f", "a\nb\nc\n");
    git(&dir, &["update-index", "--chmod=+x", "f"]);
    git(&dir, &["commit", "-qm", "x"]);
    std::fs::write(dir.join("f"), "A\nb\nC\n").unwrap();
    run(
        &dir,
        stage("f", "a\nb\nc\n", "A\nb\nc\n", Some("A\nb\nC\n")),
    )
    .unwrap();
    assert_eq!(index_text(&dir, "f"), "A\nb\nc\n");
    assert_eq!(std::fs::read_to_string(dir.join("f")).unwrap(), "A\nb\nC\n");
    assert!(git(&dir, &["ls-files", "-s", "f"]).starts_with("100755 "));
}

#[test]
fn a_path_with_a_space_and_a_non_ascii_name_is_written() {
    let name = "dir with space/файл \"q\".txt";
    let dir = Scratch::new("guild-hunk-");
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "a@b"]);
    git(&dir, &["config", "user.name", "a"]);
    std::fs::create_dir_all(dir.join("dir with space")).unwrap();
    std::fs::write(dir.join(name), "a\n").unwrap();
    git(&dir, &["add", "--", name]);
    git(&dir, &["commit", "-qm", "init"]);
    run(&dir, stage(name, "a\n", "b\n", None)).unwrap();
    assert_eq!(index_text(&dir, name), "b\n");
}

/// Review finding `lenses-1`: a `git add` from outside lands after the
/// window read the entry and before git takes its lock. git compares the
/// entry it reads under the lock with the preimage and refuses.
#[test]
fn an_entry_changed_after_settling_is_refused_by_git_and_reads_as_stale() {
    let dir = repository("f", "a\nb\nc\n");
    let write = stage("f", "a\nb\nc\n", "A\nb\nc\n", None);
    let Settled::Ready(write) = git::settle(&dir, write) else {
        panic!("settles");
    };
    std::fs::write(dir.join("f"), "a\nb\nX\n").unwrap();
    git(&dir, &["add", "f"]);
    assert!(git::write(&dir, &write).is_err());
    assert_eq!(index_text(&dir, "f"), "a\nb\nX\n");
    let Write::StageHunk(hunk) = &write else {
        unreachable!()
    };
    assert!(git::hunk::stale(&dir, hunk).unwrap());
}

#[test]
fn an_entry_changed_before_settling_is_stale() {
    let dir = repository("f", "a\nb\nc\n");
    std::fs::write(dir.join("f"), "a\nb\nX\n").unwrap();
    git(&dir, &["add", "f"]);
    assert_eq!(
        run(&dir, stage("f", "a\nb\nc\n", "A\nb\nc\n", None)),
        Err("stale".into())
    );
}

#[test]
fn a_disk_that_changed_since_the_diff_is_stale() {
    let dir = repository("f", "a\nb\nc\n");
    std::fs::write(dir.join("f"), "A\nb\nZ\n").unwrap();
    assert_eq!(
        run(
            &dir,
            stage("f", "a\nb\nc\n", "A\nb\nc\n", Some("A\nb\nC\n"))
        ),
        Err("stale".into())
    );
    assert_eq!(index_text(&dir, "f"), "a\nb\nc\n");
}

/// Review finding `lenses-2`: a commit from a terminal moves HEAD and
/// leaves the index and the disk as they were; the old Unstage would stage
/// the inverse of that commit.
#[test]
fn an_unstage_after_head_moved_is_stale() {
    let dir = repository("f", "one\ntwo\nthree\n");
    std::fs::write(dir.join("f"), "one\nTWO\nthree\n").unwrap();
    git(&dir, &["add", "f"]);
    let write = Write::UnstageHunk(Hunk {
        path: PathBuf::from("f"),
        preimage: bytes("one\nTWO\nthree\n"),
        postimage: bytes("one\ntwo\nthree\n"),
        disk: None,
        head: bytes("one\ntwo\nthree\n"),
    });
    git(&dir, &["commit", "-qm", "terminal"]);
    assert_eq!(run(&dir, write.clone()), Err("stale".into()));
    assert_eq!(index_text(&dir, "f"), "one\nTWO\nthree\n");
    // And an Unstage against the HEAD it was shown applies.
    git(&dir, &["reset", "-q", "--soft", "HEAD~1"]);
    run(&dir, write.clone()).unwrap();
    assert_eq!(index_text(&dir, "f"), "one\ntwo\nthree\n");
    let Write::UnstageHunk(hunk) = &write else {
        unreachable!()
    };
    assert!(!git::hunk::head_moved(&dir, hunk).unwrap());
}

/// Review finding `lenses-7`.
#[test]
fn ident_and_working_tree_encoding_are_converted_files() {
    let dir = repository("f", "a\n$Id$\nc\n");
    std::fs::write(dir.join(".gitattributes"), "f ident\n").unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\n$Id$\nc\n", "a\n$Id$\nC\n", None)),
        Err("converted".into())
    );
    std::fs::write(
        dir.join(".gitattributes"),
        "f working-tree-encoding=UTF-16LE\n",
    )
    .unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\n$Id$\nc\n", "a\n$Id$\nC\n", None)),
        Err("converted".into())
    );
}

#[test]
fn a_postimage_git_would_store_with_other_line_endings_is_converted() {
    let dir = repository("f", "a\nb\n");
    git(&dir, &["config", "core.autocrlf", "true"]);
    assert_eq!(
        run(&dir, stage("f", "a\nb\n", "a\r\nB\r\n", None)),
        Err("converted".into())
    );
    // The same file with LF lines is stored as it is.
    run(&dir, stage("f", "a\nb\n", "a\nB\n", None)).unwrap();
    assert_eq!(index_text(&dir, "f"), "a\nB\n");
}

#[test]
fn a_symlink_or_an_unmerged_path_is_not_written() {
    let dir = repository("f", "a\n");
    std::os::unix::fs::symlink("f", dir.join("l")).unwrap();
    git(&dir, &["add", "l"]);
    git(&dir, &["commit", "-qm", "link"]);
    assert_eq!(run(&dir, stage("l", "f", "g", None)), Err("stale".into()));
}

/// Review r1 `bugs+impl-1`: in a project that is a folder of its
/// repository, the write lands on the folder's file — not on the root's file
/// of the same name and content, and not on nothing with a success.
#[test]
fn a_folder_project_writes_its_own_file() {
    let dir = repository("f", "a\nb\n");
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/f"), "a\nb\n").unwrap();
    git(&dir, &["add", "sub/f"]);
    git(&dir, &["commit", "-qm", "sub"]);
    let sub = dir.join("sub");
    run(&sub, stage("f", "a\nb\n", "a\nB\n", None)).unwrap();
    assert_eq!(index_text(&dir, "sub/f"), "a\nB\n");
    assert_eq!(index_text(&dir, "f"), "a\nb\n");
}

/// Review r1 `bugs+impl-4`, r2 `docs+tests-9`: the entry becomes a symlink
/// with the same blob after the write built its patch; git itself refuses
/// the patch because it names the regular mode.
#[test]
fn a_patch_naming_the_regular_mode_is_refused_for_a_symlink_entry() {
    let dir = repository("f", "target");
    let old = git(&dir, &["rev-parse", ":f"]).trim().to_owned();
    let new = {
        let out = Command::new("git")
            .args(["hash-object", "--stdin", "--no-filters"])
            .current_dir(&dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write as _;
        out.stdin.as_ref().unwrap().write_all(b"other").unwrap();
        String::from_utf8(out.wait_with_output().unwrap().stdout).unwrap()
    };
    let patch = git::hunk::patch("f", &old, new.trim(), "100644", b"target", b"other");
    git(
        &dir,
        &["update-index", "--cacheinfo", &format!("120000,{old},f")],
    );
    let mut apply = Command::new("git")
        .args(["apply", "--cached", "--no-3way", "--whitespace=nowarn", "-"])
        .current_dir(&dir)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    {
        use std::io::Write as _;
        apply.stdin.take().unwrap().write_all(&patch).unwrap();
    }
    assert!(!apply.wait().unwrap().success(), "git refused the patch");
    let listed = git(&dir, &["ls-files", "-s", "f"]);
    assert!(listed.starts_with("120000 ") && listed.contains(&old));
}

/// Review r1 `bugs+impl-5`: a working file that is gone or a symlink, and a
/// HEAD entry that is a symlink, take no hunk.
#[test]
fn a_missing_or_symlinked_working_file_takes_no_hunk() {
    let dir = repository("f", "a\nb\n");
    std::fs::write(dir.join("f"), "a\nB\n").unwrap();
    git(&dir, &["add", "f"]);
    std::fs::remove_file(dir.join("f")).unwrap();
    let unstage = Write::UnstageHunk(Hunk {
        path: PathBuf::from("f"),
        preimage: bytes("a\nB\n"),
        postimage: bytes("a\nb\n"),
        disk: None,
        head: bytes("a\nb\n"),
    });
    assert_eq!(run(&dir, unstage), Err("stale".into()));
    std::fs::write(dir.join("elsewhere"), "a\nB\nc\n").unwrap();
    std::os::unix::fs::symlink("elsewhere", dir.join("f")).unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\nB\n", "a\nB\nc\n", Some("a\nB\nc\n"))),
        Err("stale".into())
    );
    assert_eq!(
        git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, usize::MAX).eligibility,
        git::hunk::Eligibility::WholeFile
    );
}

/// Review r1 `bugs+impl-6`, `arch+quality-7`: `filter=unset` reads like an
/// unset filter, and a driver of that name may be configured. Nothing the
/// checks run may start it.
#[test]
fn an_ambiguous_filter_is_refused_without_running_a_driver() {
    let dir = repository("f", "a\nb\n");
    let ran = dir.join("ran");
    git(
        &dir,
        &[
            "config",
            "filter.unset.clean",
            &format!("sh -c 'touch {}; cat'", ran.display()),
        ],
    );
    for attribute in ["f filter=unset\n", "f -filter\n"] {
        std::fs::write(dir.join(".gitattributes"), attribute).unwrap();
        assert_eq!(
            run(&dir, stage("f", "a\nb\n", "a\nB\n", None)),
            Err("converted".into())
        );
        assert_eq!(
            git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, usize::MAX).eligibility,
            git::hunk::Eligibility::Converted
        );
    }
    assert!(!ran.exists(), "a clean driver ran");
}

#[test]
fn crlf_bytes_are_staged_as_they_are_where_nothing_converts_them() {
    let dir = repository("f", "a\r\nb\r\n");
    run(&dir, stage("f", "a\r\nb\r\n", "a\r\nB\r\n", None)).unwrap();
    assert_eq!(index_text(&dir, "f"), "a\r\nB\r\n");
}

/// Review r1 `bugs+impl-7`, r2 `docs+tests-10`: an index entry past the
/// limit is not read though HEAD and the working file are small.
#[test]
fn an_index_entry_past_the_size_limit_is_not_read() {
    let dir = repository("f", "a\n");
    let big = "x\n".repeat(600_000);
    std::fs::write(dir.join("f"), &big).unwrap();
    git(&dir, &["add", "f"]);
    std::fs::write(dir.join("f"), "a\nb\n").unwrap();
    let facts = git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, usize::MAX);
    assert_eq!(facts.eligibility, git::hunk::Eligibility::WholeFile);
    assert!(facts.index.is_none(), "the oversized entry was not read");
}

/// The editor's line budget holds for the index text too — review r2
/// `adversarial-11`.
#[test]
fn an_index_entry_past_the_line_budget_is_not_offered() {
    let dir = repository("f", "a\n");
    std::fs::write(dir.join("f"), "\n".repeat(30_000)).unwrap();
    git(&dir, &["add", "f"]);
    std::fs::write(dir.join("f"), "a\nb\n").unwrap();
    let facts = git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, 20_000);
    assert_eq!(facts.eligibility, git::hunk::Eligibility::WholeFile);
}

/// A postimage past the limit is not written, from small eligible inputs.
#[test]
fn a_postimage_past_the_size_limit_is_not_written() {
    let dir = repository("f", "a\n");
    let big = "x\n".repeat((git::hunk::LIMIT / 2) + 1);
    assert_eq!(
        run(&dir, stage("f", "a\n", &big, None)),
        Err("whole".into())
    );
    assert_eq!(index_text(&dir, "f"), "a\n");
}

/// Review r1 `docs+tests-7`: HEAD moves after the checks and before the
/// write; the write lands and the post-check sees it.
#[test]
fn head_moving_during_an_unstage_is_seen_after_the_write() {
    let dir = repository("f", "one\ntwo\nthree\n");
    std::fs::write(dir.join("f"), "one\nTWO\nthree\n").unwrap();
    git(&dir, &["add", "f"]);
    let write = Write::UnstageHunk(Hunk {
        path: PathBuf::from("f"),
        preimage: bytes("one\nTWO\nthree\n"),
        postimage: bytes("one\ntwo\nthree\n"),
        disk: None,
        head: bytes("one\ntwo\nthree\n"),
    });
    let Settled::Ready(write) = git::settle(&dir, write) else {
        panic!("settles");
    };
    git(&dir, &["commit", "-qm", "terminal"]);
    git::write(&dir, &write).unwrap();
    let Write::UnstageHunk(hunk) = &write else {
        unreachable!()
    };
    assert!(git::hunk::head_moved(&dir, hunk).unwrap());
    assert_eq!(index_text(&dir, "f"), "one\ntwo\nthree\n");
}

/// Review r1 `bugs+impl-5`, the other way round: HEAD holds a symlink, the
/// index a regular file with the same bytes — no hunk.
#[test]
fn a_symlink_in_head_takes_no_hunk() {
    let dir = Scratch::new("guild-hunk-");
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "a@b"]);
    git(&dir, &["config", "user.name", "a"]);
    std::os::unix::fs::symlink("target", dir.join("f")).unwrap();
    git(&dir, &["add", "f"]);
    git(&dir, &["commit", "-qm", "link"]);
    std::fs::remove_file(dir.join("f")).unwrap();
    std::fs::write(dir.join("f"), "target").unwrap();
    git(&dir, &["add", "f"]);
    assert_eq!(
        git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, usize::MAX).eligibility,
        git::hunk::Eligibility::WholeFile
    );
    let unstage = Write::UnstageHunk(Hunk {
        path: PathBuf::from("f"),
        preimage: bytes("target"),
        postimage: bytes("other"),
        disk: None,
        head: bytes("target"),
    });
    assert_eq!(run(&dir, unstage), Err("stale".into()));
}

/// Review r2 `bugs+impl-7`: the legacy `crlf` attribute converts line
/// endings with `core.autocrlf` false.
#[test]
fn the_legacy_crlf_attribute_is_a_conversion() {
    let dir = repository("f", "a\nb\n");
    git(&dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join(".gitattributes"), "f crlf\n").unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\nb\n", "a\r\nB\r\n", None)),
        Err("converted".into())
    );
    assert_eq!(index_text(&dir, "f"), "a\nb\n");
}

/// Review r2 `bugs+impl-8`: `filter=unspecified` names a driver that
/// `check-attr` prints like no filter at all.
#[test]
fn a_filter_literally_named_unspecified_is_refused_without_running_it() {
    let dir = repository("f", "a\nb\n");
    let ran = dir.join("ran");
    git(
        &dir,
        &[
            "config",
            "filter.unspecified.clean",
            &format!("sh -c 'touch {}; cat'", ran.display()),
        ],
    );
    std::fs::write(dir.join(".gitattributes"), "f filter=unspecified\n").unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\nb\n", "a\nB\n", None)),
        Err("converted".into())
    );
    assert_eq!(index_text(&dir, "f"), "a\nb\n");
    assert!(!ran.exists(), "a clean driver ran");
}

/// Review r2 `adversarial-6`: a valueless `core.autocrlf` is true to git.
#[test]
fn a_valueless_autocrlf_is_a_conversion() {
    let dir = repository("f", "a\nb\n");
    let config = dir.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("[core]\n\tautocrlf\n");
    std::fs::write(&config, text).unwrap();
    assert_eq!(
        run(&dir, stage("f", "a\nb\n", "a\r\nB\r\n", None)),
        Err("converted".into())
    );
}

/// Review r2 `bugs+impl-11`: a working file that is binary takes no hunk,
/// in the Staged diff too.
#[test]
fn a_binary_working_file_takes_no_hunk() {
    let dir = repository("f", "a\nb\n");
    std::fs::write(dir.join("f"), "a\nB\n").unwrap();
    git(&dir, &["add", "f"]);
    std::fs::write(dir.join("f"), b"a\0B\n").unwrap();
    let unstage = Write::UnstageHunk(Hunk {
        path: PathBuf::from("f"),
        preimage: bytes("a\nB\n"),
        postimage: bytes("a\nb\n"),
        disk: None,
        head: bytes("a\nb\n"),
    });
    assert_ne!(run(&dir, unstage), Ok(()));
    assert_eq!(index_text(&dir, "f"), "a\nB\n");
    assert_eq!(
        git::hunk::facts(&dir, std::path::Path::new("f"), 1 << 20, usize::MAX).eligibility,
        git::hunk::Eligibility::WholeFile
    );
}

/// Review r2 `bugs+impl-6`: a parent directory that became a symlink is
/// not followed.
#[test]
fn a_symlinked_parent_directory_is_not_followed() {
    let dir = Scratch::new("guild-hunk-");
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "a@b"]);
    git(&dir, &["config", "user.name", "a"]);
    std::fs::create_dir(dir.join("d")).unwrap();
    std::fs::write(dir.join("d/f"), "a\nb\n").unwrap();
    git(&dir, &["add", "d/f"]);
    git(&dir, &["commit", "-qm", "x"]);
    std::fs::create_dir(dir.join("other")).unwrap();
    std::fs::write(dir.join("other/f"), "a\nB\n").unwrap();
    std::fs::remove_dir_all(dir.join("d")).unwrap();
    std::os::unix::fs::symlink("other", dir.join("d")).unwrap();
    assert_eq!(
        git::hunk::facts(&dir, std::path::Path::new("d/f"), 1 << 20, usize::MAX).eligibility,
        git::hunk::Eligibility::WholeFile
    );
    assert_eq!(
        run(&dir, stage("d/f", "a\nb\n", "a\nB\n", Some("a\nB\n"))),
        Err("stale".into())
    );
    assert_eq!(index_text(&dir, "d/f"), "a\nb\n");
}

/// Review r2 `bugs+impl-5`: a FIFO with no writer does not block the checks.
#[test]
fn a_fifo_does_not_block_the_checks() {
    let dir = repository("f", "a\nb\n");
    std::fs::remove_file(dir.join("f")).unwrap();
    let name = std::ffi::CString::new(dir.join("f").as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o644) }, 0);
    let (tx, rx) = std::sync::mpsc::channel();
    let path = dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(run(&path, stage("f", "a\nb\n", "a\nB\n", None)));
    });
    let answer = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the checks returned");
    assert_eq!(answer, Err("stale".into()));
}

/// Review r2 `bugs+impl-13`: a file whose name reads as pathspec magic is
/// looked up as its name.
#[test]
fn a_name_that_reads_as_pathspec_magic_is_literal() {
    let name = ":(exclude)notes";
    let dir = repository(name, "a\nb\n");
    std::fs::write(dir.join(name), "a\nB\n").unwrap();
    assert_eq!(
        git::hunk::facts(&dir, std::path::Path::new(name), 1 << 20, usize::MAX).eligibility,
        git::hunk::Eligibility::Yes
    );
}

/// Review r3 `bugs+impl-4`: `HEAD` moves to a commit where the path is a
/// symlink while the index and the disk keep their bytes. A Stage computed
/// before is stale, and the index stays as it was.
#[test]
fn a_stage_after_head_turned_the_path_into_a_symlink_is_stale() {
    let dir = repository("f", "a\nb\n");
    std::fs::write(dir.join("f"), "a\nB\n").unwrap();
    let original = git(&dir, &["rev-parse", "HEAD:f"]).trim().to_owned();
    std::fs::write(dir.join(".git/target"), "target").unwrap();
    let link = git(&dir, &["hash-object", "-w", ".git/target"])
        .trim()
        .to_owned();
    git(
        &dir,
        &["update-index", "--cacheinfo", &format!("120000,{link},f")],
    );
    let tree = git(&dir, &["write-tree"]).trim().to_owned();
    let commit = git(&dir, &["commit-tree", &tree, "-p", "HEAD", "-m", "link"])
        .trim()
        .to_owned();
    git(&dir, &["update-ref", "HEAD", &commit]);
    git(
        &dir,
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{original},f"),
        ],
    );
    assert_eq!(index_text(&dir, "f"), "a\nb\n");
    assert_eq!(
        run(&dir, stage("f", "a\nb\n", "a\nB\n", Some("a\nB\n"))),
        Err("stale".into())
    );
    assert_eq!(index_text(&dir, "f"), "a\nb\n");
}

/// Review r3 `adversarial-5`: a configured filesystem monitor is a program
/// of the person's. It runs for an ordinary `git status` here, and for none
/// of a hunk's reads or its write.
#[test]
fn no_hunk_read_or_write_runs_the_filesystem_monitor() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = repository("f", "a\nb\n");
    let marker = dir.join(".git/monitor-ran");
    let hook = dir.join(".git/monitor.sh");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\necho ran >> '{}'\nprintf '/'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&dir, &["config", "core.fsmonitor", hook.to_str().unwrap()]);
    std::fs::write(dir.join("f"), "a\nB\n").unwrap();
    git(&dir, &["status", "--porcelain"]);
    assert!(marker.exists(), "the monitor is live");
    std::fs::remove_file(&marker).unwrap();
    let facts = git::hunk::facts(&dir, Path::new("f"), 1 << 20, usize::MAX);
    assert_eq!(facts.eligibility, git::hunk::Eligibility::Yes);
    run(&dir, stage("f", "a\nb\n", "a\nB\n", Some("a\nB\n"))).unwrap();
    // The walk that settles the write, too — review r4 `bugs+impl-2`.
    git::hunk::status(&dir).unwrap();
    assert!(!marker.exists(), "no hunk call ran the monitor");
    assert_eq!(index_text(&dir, "f"), "a\nB\n");
}

/// Whether `oid` is missing here, asked without any lookup that could fetch
/// it — `rev-list --missing=print` reads only what is local, on every git
/// (review r5 `bugs+impl-7`: `cat-file -e` fetches on a git before 2.45).
fn missing_locally(dir: &Path, oid: &str) -> bool {
    git(dir, &["rev-list", "--objects", "--all", "--missing=print"])
        .lines()
        .any(|line| line == format!("?{oid}"))
}

/// A blob-filtered clone of a repository whose `f` was `one` then `two`:
/// the clone holds `two`, and `one` is a promisor object never fetched.
fn partial_clone() -> (Scratch, Scratch, String) {
    let origin = repository("f", "one\n");
    std::fs::write(origin.join("f"), "two\n").unwrap();
    git(&origin, &["commit", "-qam", "two"]);
    git(&origin, &["config", "uploadpack.allowFilter", "true"]);
    git(
        &origin,
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    );
    let clone = Scratch::new("guild-hunk-partial-");
    let url = format!("file://{}", origin.display());
    git(&clone, &["clone", "-q", "--filter=blob:none", &url, "."]);
    git(&clone, &["config", "core.autocrlf", "false"]);
    git(&clone, &["config", "core.attributesFile", "/dev/null"]);
    let old = git(&clone, &["rev-parse", "HEAD~1:f"]).trim().to_owned();
    assert!(
        missing_locally(&clone, &old),
        "the fixture's blob starts missing"
    );
    (origin, clone, old)
}

/// Review r4 `bugs+impl-5`: in a blob-filtered clone, a soft reset leaves the
/// working file and the index blob local while `HEAD`'s blob is a promisor
/// object never fetched. Reading the facts fetches nothing: the blob stays
/// missing and the file takes no hunks.
#[test]
fn a_missing_promisor_blob_is_never_fetched_for_a_hunk() {
    let (_origin, clone, old) = partial_clone();
    let missing = |dir: &Path| missing_locally(dir, &old);
    git(&clone, &["reset", "-q", "--soft", "HEAD~1"]);
    std::fs::write(clone.join("f"), "three\n").unwrap();
    let facts = git::hunk::facts(&clone, Path::new("f"), 1 << 20, usize::MAX);
    assert!(missing(&clone), "nothing was fetched");
    assert_eq!(facts.eligibility, git::hunk::Eligibility::WholeFile);
}

/// Review r5 `bugs+impl-2`: an unrelated staged rename whose source is a
/// promisor blob never fetched. The status walk that settles a hunk write
/// detects no renames, so it neither fetches that blob nor fails on it.
#[test]
fn the_settling_walk_needs_no_rename_source() {
    let (_origin, clone, old) = partial_clone();
    // HEAD's `f` is now `one`, missing; the index renames it to `g` with
    // other content, so only an inexact rename could pair them.
    git(&clone, &["reset", "-q", "--soft", "HEAD~1"]);
    git(&clone, &["mv", "f", "g"]);
    std::fs::write(clone.join("g"), "two, and more besides\n").unwrap();
    git(&clone, &["add", "g"]);
    let walked = git::hunk::status(&clone);
    assert!(walked.is_ok(), "{walked:?}");
    assert!(missing_locally(&clone, &old), "nothing was fetched");
    let listed = String::from_utf8_lossy(&walked.unwrap()).into_owned();
    assert!(
        listed.contains(" f\0") && listed.contains(" g\0"),
        "both paths: {listed:?}"
    );
}

/// Review r5 `docs+tests-3`: a remote with only `partialCloneFilter` is a
/// promisor to git — before 2.45 the guard must see a partial clone there.
#[test]
fn a_filter_only_remote_is_a_partial_clone() {
    let dir = repository("f", "a\n");
    assert!(!git::hunk::partial_clone(&dir).unwrap());
    git(
        &dir,
        &["remote", "add", "origin", "https://example.invalid/r"],
    );
    git(
        &dir,
        &["config", "remote.origin.partialCloneFilter", "blob:none"],
    );
    assert!(git::hunk::partial_clone(&dir).unwrap());
}
