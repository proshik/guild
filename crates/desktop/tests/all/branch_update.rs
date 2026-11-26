use guild_desktop::git::{
    self,
    branch_update::{Request, Snapshot},
};
use std::path::Path;

fn git_at(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git_at(dir.path(), &["init", "-b", "main"]);
    git_at(dir.path(), &["config", "user.name", "Test"]);
    git_at(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    std::fs::write(dir.path().join("file"), "base\n").unwrap();
    git_at(dir.path(), &["add", "."]);
    git_at(dir.path(), &["commit", "-m", "base"]);
    dir
}
fn request(root: &Path, source: &str) -> Request {
    Snapshot::read(root).unwrap().request(source).unwrap()
}
fn run(root: &Path, req: &Request) -> Result<(), git::GitError> {
    git::branch_update::execute(root, req, |_, _| {})
}
#[test]
fn selecting_source_is_read_only_and_local_merge_preserves_current_branch() {
    let dir = repo();
    let root = dir.path();
    git_at(root, &["switch", "-c", "topic"]);
    std::fs::write(root.join("new"), "topic").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "topic"]);
    git_at(root, &["switch", "main"]);
    let before = git_at(root, &["rev-parse", "HEAD"]);
    let req = request(root, "refs/heads/topic");
    assert_eq!(before, git_at(root, &["rev-parse", "HEAD"]));
    run(root, &req).unwrap();
    assert_eq!(git_at(root, &["branch", "--show-current"]), "main");
    assert_eq!(
        git_at(root, &["rev-parse", "HEAD"]),
        git_at(root, &["rev-parse", "topic"])
    );
}
#[test]
fn stale_head_dirty_untracked_and_busy_states_refuse_without_losing_work() {
    let dir = repo();
    let root = dir.path();
    git_at(root, &["branch", "source"]);
    let req = request(root, "refs/heads/source");
    std::fs::write(root.join("untracked"), "keep").unwrap();
    assert!(run(root, &req).is_err());
    std::fs::remove_file(root.join("untracked")).unwrap();
    std::fs::write(root.join("file"), "changed").unwrap();
    assert!(run(root, &req).is_err());
    git_at(root, &["add", "file"]);
    assert!(run(root, &req).is_err());
    git_at(root, &["commit", "-m", "changed"]);
    assert!(run(root, &req).is_err());
    let req = request(root, "refs/heads/source");
    std::fs::create_dir(root.join(".git/rebase-merge")).unwrap();
    assert!(run(root, &req).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("file")).unwrap(),
        "changed"
    );
}
#[test]
fn same_branch_without_upstream_and_detached_or_unborn_head_are_unavailable() {
    let dir = repo();
    let root = dir.path();
    assert!(
        Snapshot::read(root)
            .unwrap()
            .request("refs/heads/main")
            .is_err()
    );
    git_at(root, &["checkout", "--detach"]);
    assert!(
        Snapshot::read(root)
            .unwrap()
            .request("refs/heads/main")
            .is_err()
    );
    let empty = tempfile::tempdir().unwrap();
    git_at(empty.path(), &["init"]);
    assert!(
        Snapshot::read(empty.path())
            .unwrap()
            .request("refs/heads/main")
            .is_err()
    );
}
#[test]
fn remote_source_fetches_new_tip_and_failed_fetch_never_merges_cached_tip() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(
        root,
        &["remote", "add", "upstream", remote.path().to_str().unwrap()],
    );
    git_at(root, &["fetch", "upstream"]);
    git_at(root, &["branch", "--set-upstream-to=upstream/main", "main"]);
    let req = request(root, "refs/heads/main");
    std::fs::write(remote.path().join("later"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "later"]);
    run(root, &req).unwrap();
    assert!(root.join("later").exists());
    assert_eq!(
        git_at(root, &["rev-parse", "refs/remotes/upstream/main"]),
        git_at(remote.path(), &["rev-parse", "HEAD"])
    );
    let req = request(root, "refs/remotes/upstream/main");
    git_at(
        root,
        &[
            "remote",
            "set-url",
            "upstream",
            "/no-such-guild-update-fixture",
        ],
    );
    let before = git_at(root, &["rev-parse", "HEAD"]);
    assert!(run(root, &req).is_err());
    assert_eq!(before, git_at(root, &["rev-parse", "HEAD"]));
}
#[test]
fn merge_conflicts_keep_git_state_and_report_both_output_channels() {
    let dir = repo();
    let root = dir.path();
    git_at(root, &["switch", "-c", "source"]);
    std::fs::write(root.join("file"), "source\n").unwrap();
    git_at(root, &["commit", "-am", "source"]);
    git_at(root, &["switch", "main"]);
    std::fs::write(root.join("file"), "current\n").unwrap();
    git_at(root, &["commit", "-am", "current"]);
    let receipt = git::branch_update::execute_recorded(
        root,
        &request(root, "refs/heads/source"),
        || true,
        |_, _| {},
    );
    assert_eq!(receipt.kind, git::branch_update::ReceiptKind::Conflict);
    let guidance =
        guild_desktop::i18n::Ui(guild_session::Language::English).branch_update_receipt(&receipt);
    assert!(
        guidance.contains("CONFLICT")
            && guidance.contains("git merge --abort")
            && guidance.contains("git add <resolved paths>")
    );
    assert!(guidance.contains(root.canonicalize().unwrap().to_str().unwrap()));
    assert!(root.join(".git/MERGE_HEAD").exists());
    assert!(
        std::fs::read_to_string(root.join("file"))
            .unwrap()
            .contains("<<<<<<<")
    );
}

#[test]
fn changing_branch_source_or_origin_invalidates_a_queued_request() {
    let dir = repo();
    let root = dir.path();
    git_at(root, &["branch", "source"]);
    let req = request(root, "refs/heads/source");
    git_at(root, &["switch", "-c", "other"]);
    assert!(run(root, &req).is_err());
    git_at(root, &["switch", "main"]);
    let other = repo();
    assert!(run(other.path(), &req).is_err());
    git_at(root, &["branch", "-d", "source"]);
    assert!(run(root, &req).is_err());
}

#[test]
fn a_new_unsaved_buffer_between_fetch_and_merge_refuses_the_merge() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    let req = request(root, "refs/heads/main");
    let before = git_at(root, &["rev-parse", "HEAD"]);
    std::fs::write(remote.path().join("later"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "later"]);
    let calls = std::cell::Cell::new(0);
    let result = git::branch_update::execute_guarded(
        root,
        &req,
        || {
            let n = calls.get();
            calls.set(n + 1);
            n == 0
        },
        |_, _| {},
    );
    assert!(result.is_err());
    assert_eq!(before, git_at(root, &["rev-parse", "HEAD"]));
}

#[test]
fn merging_a_source_checked_out_elsewhere_never_moves_its_worktree() {
    let dir = repo();
    let root = dir.path();
    let other = tempfile::tempdir().unwrap();
    let work = other.path().join("source");
    git_at(
        root,
        &["worktree", "add", "-b", "source", work.to_str().unwrap()],
    );
    std::fs::write(work.join("new"), "source").unwrap();
    git_at(&work, &["add", "."]);
    git_at(&work, &["commit", "-m", "source"]);
    let source_head = git_at(&work, &["rev-parse", "HEAD"]);
    run(root, &request(root, "refs/heads/source")).unwrap();
    assert_eq!(source_head, git_at(&work, &["rev-parse", "HEAD"]));
    assert_eq!(git_at(&work, &["status", "--porcelain"]), "");
    assert!(root.join("new").exists());
}

#[test]
fn source_preferences_round_trip_per_worktree_and_missing_choice_is_not_replaced() {
    let dir = repo();
    let root = dir.path();
    let mut settings = guild_session::Settings::default();
    settings
        .branch_update_sources
        .insert(root.into(), "refs/heads/deleted".into());
    settings
        .branch_update_sources
        .insert(root.join("other"), "refs/remotes/origin/main".into());
    let saved = serde_json::to_vec(&settings).unwrap();
    let read: guild_session::Settings = serde_json::from_slice(&saved).unwrap();
    assert_eq!(read.branch_update_sources, settings.branch_update_sources);
    assert!(
        Snapshot::read(root)
            .unwrap()
            .request(&read.branch_update_sources[root])
            .is_err()
    );
    let old: guild_session::Settings = serde_json::from_str("{}").unwrap();
    assert!(old.branch_update_sources.is_empty());
}

#[test]
fn an_orphaned_remote_reference_is_not_treated_as_a_local_source() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(root, &["config", "--remove-section", "remote.origin"]);
    assert!(
        Snapshot::read(root)
            .unwrap()
            .request("refs/remotes/origin/main")
            .is_err()
    );
}

#[test]
fn remote_source_uses_its_configured_fetch_mapping() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(
        root,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/default",
        ],
    );
    git_at(root, &["fetch", "origin"]);
    let req = request(root, "refs/remotes/origin/default");
    assert_eq!(
        req.source.remote,
        Some(("origin".into(), "refs/heads/main".into()))
    );
    std::fs::write(remote.path().join("mapped"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "mapped"]);
    run(root, &req).unwrap();
    assert!(root.join("mapped").exists());
}

#[test]
fn corrections_local_main_uses_new_upstream_without_moving_local_main() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(root, &["switch", "-c", "feature"]);
    let old_main = git_at(root, &["rev-parse", "main"]);
    let req = request(root, "refs/heads/main");
    assert_eq!(req.source.reference, "refs/remotes/origin/main");
    std::fs::write(remote.path().join("new-upstream"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "upstream"]);
    run(root, &req).unwrap();
    assert!(root.join("new-upstream").exists());
    assert_eq!(git_at(root, &["rev-parse", "main"]), old_main);
    assert_eq!(git_at(root, &["branch", "--show-current"]), "feature");
}

#[test]
fn corrections_fetch_timeout_stops_children_and_never_merges_stale_tip() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    let helper = tempfile::tempdir().unwrap();
    let script = helper.path().join("ssh");
    let env = helper.path().join("env");
    let late = helper.path().join("late");
    std::fs::write(&script, format!("#!/bin/sh\nprintf '%s %s' \"$GIT_TERMINAL_PROMPT\" \"$SSH_ASKPASS_REQUIRE\" > '{}'\necho fixture-diagnostic >&2\n(sleep 3; touch '{}') &\nwait\n", env.display(), late.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    git_at(
        root,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    git_at(
        root,
        &["remote", "set-url", "origin", "ssh://example.invalid/repo"],
    );
    let req = request(root, "refs/heads/main");
    let old = git_at(root, &["rev-parse", "HEAD"]);
    let start = Instant::now();
    let mut attempted = Vec::new();
    let result = git::branch_update::execute_with_timeout(
        root,
        &req,
        || true,
        |write, _| attempted.push(write.clone()),
        Duration::from_secs(1),
    );
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("timed out") && error.contains("fixture-diagnostic"),
        "{error}"
    );
    assert!(
        guild_desktop::i18n::Ui(guild_session::Language::Russian)
            .branch_update_diagnostic(&error)
            .contains("Время ожидания fetch")
    );
    assert!(env.exists(), "{error}");
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(std::fs::read_to_string(env).unwrap(), "0 force");
    assert!(
        attempted
            .iter()
            .all(|w| matches!(w, git::Write::FetchSource { .. }))
    );
    assert_eq!(old, git_at(root, &["rev-parse", "HEAD"]));
    std::thread::sleep(Duration::from_millis(3100));
    assert!(
        !late.exists(),
        "the fetch descendant must have been stopped"
    );
}

#[test]
fn corrections_recovery_records_heads_and_distinguishes_merge_from_fast_forward() {
    use git::branch_update::{ReceiptKind, execute_recorded};
    let dir = repo();
    let root = dir.path();
    git_at(root, &["switch", "-c", "source"]);
    std::fs::write(root.join("source-file"), "source").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "source"]);
    git_at(root, &["switch", "main"]);
    let before = git_at(root, &["rev-parse", "HEAD"]);
    let receipt = execute_recorded(
        root,
        &request(root, "refs/heads/source"),
        || true,
        |_, _| {},
    );
    assert_eq!(receipt.kind, ReceiptKind::FastForward);
    assert_eq!(receipt.before, before);
    assert_eq!(
        receipt.after.as_deref(),
        Some(git_at(root, &["rev-parse", "HEAD"]).as_str())
    );
    let text =
        guild_desktop::i18n::Ui(guild_session::Language::English).branch_update_receipt(&receipt);
    assert!(text.contains(&before) && text.contains("git revert <commit>"));
    git_at(root, &["switch", "source"]);
    std::fs::write(root.join("source-two"), "source").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "source two"]);
    git_at(root, &["switch", "main"]);
    std::fs::write(root.join("current"), "current").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "current"]);
    let receipt = execute_recorded(
        root,
        &request(root, "refs/heads/source"),
        || true,
        |_, _| {},
    );
    assert_eq!(receipt.kind, ReceiptKind::MergeCommit);
    assert_eq!(
        git_at(root, &["log", "-1", "--format=%s"]),
        "Merge refs/heads/source into refs/heads/main"
    );
    let text =
        guild_desktop::i18n::Ui(guild_session::Language::English).branch_update_receipt(&receipt);
    assert!(text.contains(&format!("git revert -m 1 {}", receipt.after.unwrap())));
}

#[test]
fn large_update_keeps_console_output_but_bounds_recovery_diagnostic() {
    let dir = repo();
    let root = dir.path();
    git_at(root, &["switch", "-c", "topic"]);
    for i in 0..80 {
        std::fs::write(root.join(format!("added-{i:03}")), "text").unwrap();
    }
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "many files"]);
    git_at(root, &["switch", "main"]);
    let mut console = String::new();
    let receipt = git::branch_update::execute_recorded(
        root,
        &request(root, "refs/heads/topic"),
        || true,
        |_, said| {
            if let Ok(said) = said {
                console.push_str(&said.text());
            }
        },
    );
    assert!(console.contains("added-079"));
    assert!(
        receipt.diagnostic.lines().count() <= 6,
        "notice diagnostic is bounded"
    );
}

#[test]
fn rewritten_upstream_refusal_has_manual_fetch_guidance_and_never_merges() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    let before = git_at(root, &["rev-parse", "HEAD"]);
    let req = request(root, "refs/heads/main");
    git_at(remote.path(), &["checkout", "--orphan", "rewritten"]);
    git_at(remote.path(), &["commit", "-m", "rewritten"]);
    let rewritten = git_at(remote.path(), &["rev-parse", "HEAD"]);
    git_at(
        remote.path(),
        &["update-ref", "refs/heads/main", &rewritten],
    );
    let mut merged = false;
    let receipt = git::branch_update::execute_recorded(
        root,
        &req,
        || true,
        |write, _| {
            merged |= matches!(write, git::Write::MergeSource { .. });
        },
    );
    assert_eq!(receipt.kind, git::branch_update::ReceiptKind::FetchRefused);
    assert!(!receipt.diagnostic.contains("git failed: git failed:"));
    assert!(!merged);
    assert_eq!(git_at(root, &["rev-parse", "HEAD"]), before);
    let en =
        guild_desktop::i18n::Ui(guild_session::Language::English).branch_update_receipt(&receipt);
    let ru =
        guild_desktop::i18n::Ui(guild_session::Language::Russian).branch_update_receipt(&receipt);
    assert!(en.contains("fetch it manually"));
    assert!(ru.contains("fetch вручную"));
}

#[test]
fn recovery_record_survives_refusal_and_language_switch_and_reads_legacy_settings() {
    use guild_session::settings::{BranchUpdateKind as Kind, BranchUpdateReceipt as Receipt};
    let mut settings: guild_session::Settings =
        serde_json::from_str(r#"{"branch_update_receipts":{"/old":"legacy guidance"}}"#).unwrap();
    let receipt = Receipt {
        root: "/new".into(),
        before: "before".into(),
        after: Some("after".into()),
        kind: Kind::MergeCommit,
        diagnostic: String::new(),
    };
    settings.remember_branch_update(receipt.clone());
    for kind in [Kind::Refused, Kind::Unchanged, Kind::FetchRefused] {
        settings.remember_branch_update(Receipt {
            kind,
            ..receipt.clone()
        });
        settings.remember_branch_update(Receipt {
            root: "/old".into(),
            kind,
            ..receipt.clone()
        });
    }
    let saved = serde_json::to_string(&settings).unwrap();
    let read: guild_session::Settings = serde_json::from_str(&saved).unwrap();
    let record = &read.branch_update_receipts[Path::new("/new")];
    let en = guild_desktop::i18n::Ui(guild_session::Language::English);
    let ru = guild_desktop::i18n::Ui(guild_session::Language::Russian);
    assert!(
        en.branch_update_record(record)
            .contains("Merge commit created")
    );
    assert!(
        ru.branch_update_record(record)
            .contains("Создан merge-коммит")
    );
    assert!(
        ru.branch_update_record(record)
            .contains("git revert -m 1 after")
    );
    assert_eq!(
        en.branch_update_record(&read.branch_update_receipts[Path::new("/old")]),
        "legacy guidance"
    );
    assert!(
        ru.branch_update_diagnostic("Save unsaved files before updating the branch.")
            .contains("Сохраните файлы")
    );
}

#[test]
fn latest_nonrecoverable_outcome_replaces_an_earlier_refusal() {
    use guild_session::settings::{
        BranchUpdateKind as Kind, BranchUpdateReceipt as Receipt, BranchUpdateRecord,
    };
    let mut settings = guild_session::Settings::default();
    for kind in [Kind::Refused, Kind::FetchRefused, Kind::Unchanged] {
        let receipt = Receipt {
            root: "/repo".into(),
            before: "before".into(),
            after: Some("after".into()),
            kind,
            diagnostic: format!("{kind:?}"),
        };
        settings.remember_branch_update(receipt.clone());
        assert_eq!(
            settings.branch_update_receipts[Path::new("/repo")],
            BranchUpdateRecord::Current(receipt)
        );
    }
}

/// **The manual fetch writes its source's tracking ref and nothing else.**
/// A configured refspec applies as a refmap beside an explicit one, so
/// without `--refmap=` this update also force-moved `feature` to the
/// remote's `main` — measured on git 2.50.1.
#[test]
fn an_extra_configured_refspec_moves_no_other_ref_during_an_update() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(root, &["config", "user.name", "Test"]);
    git_at(root, &["config", "user.email", "test@example.invalid"]);
    git_at(root, &["switch", "-c", "feature"]);
    std::fs::write(root.join("feature"), "work").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-m", "feature"]);
    let feature = git_at(root, &["rev-parse", "feature"]);
    git_at(root, &["switch", "main"]);
    git_at(
        root,
        &[
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/heads/main:refs/heads/feature",
        ],
    );
    std::fs::write(remote.path().join("upstream"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "upstream"]);
    let upstream = git_at(remote.path(), &["rev-parse", "HEAD"]);
    let req = request(root, "refs/heads/main");
    run(root, &req).unwrap();
    assert_eq!(git_at(root, &["rev-parse", "origin/main"]), upstream);
    assert_eq!(git_at(root, &["rev-parse", "HEAD"]), upstream);
    assert_eq!(
        git_at(root, &["rev-parse", "feature"]),
        feature,
        "feature keeps its own commit"
    );
}

/// The same symbolic tracking ref offered to the manual update: the
/// snapshot never lists a symbolic ref as a source, so the update is
/// refused before its fetch and `feature` keeps its commit.
#[test]
fn a_symbolic_tracking_ref_is_never_an_update_source() {
    let remote = repo();
    let local = tempfile::tempdir().unwrap();
    let root = local.path();
    git_at(root, &["clone", remote.path().to_str().unwrap(), "."]);
    git_at(root, &["branch", "feature"]);
    let feature = git_at(root, &["rev-parse", "feature"]);
    git_at(
        root,
        &[
            "symbolic-ref",
            "refs/remotes/origin/main",
            "refs/heads/feature",
        ],
    );
    std::fs::write(remote.path().join("upstream"), "new").unwrap();
    git_at(remote.path(), &["add", "."]);
    git_at(remote.path(), &["commit", "-m", "upstream"]);
    let snapshot = Snapshot::read(root).unwrap();
    assert!(
        snapshot
            .sources
            .iter()
            .all(|source| source.reference != "refs/remotes/origin/main"),
        "a symbolic ref is not a source"
    );
    assert!(snapshot.request("refs/heads/main").is_err());
    assert!(snapshot.request("refs/remotes/origin/main").is_err());
    assert_eq!(git_at(root, &["rev-parse", "feature"]), feature);
}

/// The popup's footer says what the last manual update did in one short
/// line — no hashes, no path, no recovery paragraph — in either language,
/// and nothing for a legacy record.
#[test]
fn the_last_update_is_one_short_line() {
    use guild_session::settings::{
        BranchUpdateKind as Kind, BranchUpdateReceipt as Receipt, BranchUpdateRecord as Record,
    };
    let en = guild_desktop::i18n::Ui(guild_session::Language::English);
    let ru = guild_desktop::i18n::Ui(guild_session::Language::Russian);
    let receipt = |kind| {
        Record::Current(Receipt {
            root: "/repo/worktree".into(),
            before: "a".repeat(40),
            after: Some("b".repeat(40)),
            kind,
            diagnostic: "CONFLICT (content): Merge conflict in src/lib.rs".into(),
        })
    };
    assert_eq!(
        en.branch_update_last(&receipt(Kind::Unchanged)).as_deref(),
        Some("Last: already up to date")
    );
    assert_eq!(
        ru.branch_update_last(&receipt(Kind::Unchanged)).as_deref(),
        Some("Последний раз: ветка уже актуальна")
    );
    for kind in [
        Kind::Unchanged,
        Kind::FastForward,
        Kind::MergeCommit,
        Kind::Conflict,
        Kind::FetchRefused,
        Kind::Refused,
    ] {
        for ui in [en, ru] {
            let line = ui.branch_update_last(&receipt(kind)).unwrap();
            assert!(!line.contains('\n'), "{line}");
            assert!(!line.contains("aaaa") && !line.contains("bbbb"), "{line}");
            assert!(!line.contains("/repo"), "{line}");
            assert!(!line.contains("CONFLICT"), "{line}");
            assert!(line.chars().count() <= 48, "{line}");
        }
    }
    assert_eq!(
        en.branch_update_last(&Record::Legacy("Recovery: a wall of text".into())),
        None
    );
}

/// The sheet that confirms a chosen source names the branch and the
/// source, and says whether a fetch comes before the merge.
#[test]
fn the_confirmation_says_what_the_update_does() {
    let en = guild_desktop::i18n::Ui(guild_session::Language::English);
    let ru = guild_desktop::i18n::Ui(guild_session::Language::Russian);
    assert_eq!(
        en.update_branch_title("feature", "origin/main"),
        "Update feature from origin/main?"
    );
    assert_eq!(
        en.update_branch_detail("feature", "origin/main", true),
        "Fetches origin/main and merges it into feature."
    );
    assert_eq!(
        en.update_branch_detail("feature", "main", false),
        "Merges main into feature."
    );
    assert_eq!(
        ru.update_branch_title("feature", "origin/main"),
        "Обновить feature из origin/main?"
    );
    assert_eq!(
        ru.update_branch_detail("feature", "origin/main", true),
        "Загрузит origin/main и сольёт его в feature."
    );
    assert_eq!(
        ru.update_branch_detail("feature", "main", false),
        "Сольёт main в feature."
    );
    assert_eq!(
        en.text(guild_desktop::i18n::Text::UpdateBranchConfirm),
        "Update"
    );
    assert_eq!(
        ru.text(guild_desktop::i18n::Text::UpdateBranchConfirm),
        "Обновить"
    );
}

/// A source's trailing tag is faint on a resting row and one step up on
/// the lit one: `control_hover` eats faint's contrast, and a tag the
/// highlight hides is a tag read only while it does not matter. Still
/// below the name, so it stays the second thing on the row.
#[test]
fn the_lit_source_s_tag_stays_readable_and_secondary() {
    use guild_desktop::theme::{Theme, contrast, control_hover};
    use guild_desktop::view::popup::trailing_ink;
    for theme in Theme::all() {
        assert_eq!(trailing_ink(theme, false), theme.text.faint);
        assert_eq!(trailing_ink(theme, true), theme.text.tertiary);
        let lit = u32::from(control_hover(theme, theme.surface.card)) >> 8;
        let tag = contrast(trailing_ink(theme, true), lit);
        assert!(
            tag > contrast(theme.text.faint, lit),
            "{:?}: the lit tag gains on faint",
            theme.kind
        );
        assert!(
            tag < contrast(theme.text.primary, lit),
            "{:?}: the lit tag stays under the name",
            theme.kind
        );
    }
}
