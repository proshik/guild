//! The state file: what must survive a restart, and what must survive a
//! corrupted file.
//!
//! Projects and sessions are covered in `desk.rs`. Here it is only
//! the store itself, whose law is that a corrupted file must not stop the
//! application from opening.

use std::path::PathBuf;

use crate::scratch::Scratch;
use guild_session::{Desk, ProjectState, Store};

// These tests observe one process-wide notification queue.
static FAILURE_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn temp_dir(name: &str) -> Scratch {
    Scratch::absent(&format!("guild-store-{name}-"))
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ProcessSnapshot {
    writer: usize,
    revision: usize,
    payload: String,
}
impl guild_session::store::Normalise for ProcessSnapshot {}

#[test]
fn a_save_leaves_another_writers_scratch_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = dir.path().join("workspace.json.tmp");
    std::fs::write(&foreign, "another writer's unpublished bytes").unwrap();
    Store::new(dir.path().join("workspace.json"))
        .save(&Desk::default())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(foreign).unwrap(),
        "another writer's unpublished bytes"
    );
}

#[test]
fn a_failed_publish_reports_the_operation_and_removes_its_scratch() {
    let _serial = FAILURE_TESTS.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let backup = dir.path().join("workspace.json.bak");
    // Both names are occupied by nonempty directories: backup rotation and
    // publication must fail on a real filesystem, even when running as root.
    for entry in [&path, &backup] {
        std::fs::create_dir(entry).unwrap();
        std::fs::write(entry.join("keep"), "untouched").unwrap();
    }
    let error = Store::new(path.clone())
        .save(&Desk::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("rename state scratch"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
    let (failed_path, reason) =
        guild_session::take_failure().expect("publication failure is reported");
    assert_eq!(failed_path, path);
    assert!(reason.contains("rename state scratch"), "{reason}");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        2,
        "failed save removes its owned scratch"
    );
    for entry in [&path, &backup] {
        assert_eq!(
            std::fs::read_to_string(entry.join("keep")).unwrap(),
            "untouched"
        );
    }
}

/// Each process has its own store queue. They must not consume or truncate
/// another process's unpublished snapshot, even when saving the same path.
#[test]
fn competing_processes_save_complete_snapshots() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    let mut children = Vec::new();
    for writer in 0..4 {
        let log = std::fs::File::create(dir.path().join(format!("log-{writer}"))).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "store::competing_process_writer",
                "--ignored",
                "--nocapture",
            ])
            .env("GUILD_STORE_RACE_DIR", dir.path())
            .env("GUILD_STORE_RACE_WRITER", writer.to_string())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        children.push(guild_session::process::ProcessGroup::spawn(&mut command).unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while !(0..4).all(|writer| dir.path().join(format!("ready-{writer}")).exists()) {
        assert!(
            Instant::now() < deadline,
            "writers did not reach the barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(dir.path().join("go"), "").unwrap();
    let mut outcomes = vec![None; children.len()];
    while outcomes.iter().any(Option::is_none) {
        for (child, outcome) in children.iter_mut().zip(&mut outcomes) {
            if outcome.is_none() {
                *outcome = child.try_wait().unwrap();
            }
        }
        assert!(Instant::now() < deadline, "writers did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    let failures: Vec<_> = outcomes
        .iter()
        .enumerate()
        .filter(|(_, status)| !status.unwrap().success())
        .map(|(writer, _)| {
            std::fs::read_to_string(dir.path().join(format!("log-{writer}"))).unwrap()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for file in ["workspace.json", "workspace.json.bak"] {
        let snapshot: ProcessSnapshot =
            serde_json::from_slice(&std::fs::read(dir.path().join(file)).unwrap()).unwrap();
        assert!(snapshot.writer < 4 && snapshot.revision < 40);
        assert_eq!(snapshot.payload, snapshot.writer.to_string().repeat(65536));
    }
    let remaining: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        remaining.len(),
        11,
        "only state, backup, barrier and child logs/markers remain: {remaining:?}"
    );
}

#[test]
#[ignore = "owned subprocess of competing_processes_save_complete_snapshots"]
fn competing_process_writer() {
    use std::time::{Duration, Instant};
    let dir =
        PathBuf::from(std::env::var_os("GUILD_STORE_RACE_DIR").expect("run via parent fixture"));
    let writer: usize = std::env::var("GUILD_STORE_RACE_WRITER")
        .unwrap()
        .parse()
        .unwrap();
    let store = Store::new(dir.join("workspace.json"));
    std::fs::write(dir.join(format!("ready-{writer}")), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !dir.join("go").exists() {
        assert!(Instant::now() < deadline, "parent did not release barrier");
        std::thread::sleep(Duration::from_millis(5));
    }
    for revision in 0..40 {
        store
            .save(&ProcessSnapshot {
                writer,
                revision,
                payload: writer.to_string().repeat(65536),
            })
            .expect("a competing process must not take this save's scratch file");
    }
}

/// A broken state file must not prevent startup.
///
/// An application that will not open because of a corrupted settings file
/// is the worst kind of breakage: there is nothing to fix it with, because
/// there is no interface. So we read what we can, and when we cannot, we
/// start clean.
#[test]
fn broken_state_file_does_not_prevent_startup() {
    let dir = temp_dir("broken");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    std::fs::write(&path, "{this is not json").unwrap();

    let workspace = Store::<Desk>::new(path)
        .load()
        .expect("startup must not fail");

    assert_eq!(workspace.projects().count(), 0, "we start clean");
}

#[test]
fn missing_state_file_is_not_an_error() {
    let dir = temp_dir("no-file");

    let workspace = Store::<Desk>::new(dir.join("workspace.json"))
        .load()
        .expect("there has been no first run yet — that is normal");

    assert_eq!(workspace.projects().count(), 0);
}

#[test]
fn state_directory_is_created_when_absent() {
    let dir = temp_dir("create-directory");
    let path = dir.join("nested").join("workspace.json");
    let store = Store::new(path.clone());

    store
        .save(&Desk::default())
        .expect("the directory is created");

    assert!(path.exists(), "the state file is there");
}

/// An unknown field must not break reading: Guild versions drift apart,
/// and a file from a newer version has to be readable by an older one.
#[test]
fn unknown_fields_do_not_discard_the_file() {
    let dir = temp_dir("unknown-field");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    std::fs::write(
        &path,
        r#"{"projects":[],"sessions":[],"theme_colour":"dark"}"#,
    )
    .unwrap();

    let workspace = Store::<Desk>::new(path).load().unwrap();

    assert_eq!(workspace.projects().count(), 0);
}

/// The state file is written beside and renamed into place, so a crash
/// mid-write cannot leave half a file where the state used to be. The
/// previous copy is what proves a rename happened rather than an
/// overwrite.
#[test]
fn a_save_keeps_the_previous_copy() {
    let dir = temp_dir("previous-copy");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());

    store.save(&Desk::default()).expect("the first save");
    let first = std::fs::read_to_string(&path).unwrap();

    let mut second = Desk::default();
    second.add_project(&dir);
    store.save(&second).expect("the second save");

    let backup =
        std::fs::read_to_string(dir.join("workspace.json.bak")).expect("the previous copy is kept");
    assert_eq!(backup, first, "the backup holds what was there before");
    assert_ne!(
        std::fs::read_to_string(&path).unwrap(),
        backup,
        "and the main file holds the new state"
    );
}

/// The crash this exists for: the main file is half-written. Without a
/// fallback every project would be gone, with only a log line to say so.
#[test]
fn a_truncated_file_is_recovered_from_the_previous_copy() {
    let dir = temp_dir("truncated");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());

    let mut state = Desk::default();
    state.add_project(&dir);
    store.save(&state).expect("the first save");
    // A second save, so the backup holds the good state rather than the
    // empty one the first save displaced.
    store.save(&state).expect("the second save");

    // What a crash mid-write leaves behind.
    std::fs::write(&path, r#"{"projects": [{"name": "surv"#).unwrap();

    let loaded = store.load().expect("startup must not fail");

    assert_eq!(
        loaded.projects().count(),
        1,
        "the project comes back from the previous copy rather than being lost"
    );
}

/// A save must not leave its scratch file behind: the next reader would
/// find two state files and no rule about which one is real.
#[test]
fn a_save_leaves_no_scratch_file() {
    let dir = temp_dir("no-scratch");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");

    Store::<Desk>::new(path)
        .save(&Desk::default())
        .expect("the save");

    assert!(
        !dir.join("workspace.json.tmp").exists(),
        "the scratch file is renamed, not left"
    );
}

/// Both copies unreadable is still a clean start. The law that a corrupted
/// file must not stop the application from opening is unchanged; it is
/// only no longer the first answer.
#[test]
fn both_copies_broken_still_start_clean() {
    let dir = temp_dir("both-broken");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    std::fs::write(&path, "{not json").unwrap();
    std::fs::write(dir.join("workspace.json.bak"), "also not json").unwrap();

    let loaded = Store::<Desk>::new(path)
        .load()
        .expect("startup must not fail");

    assert_eq!(loaded.projects().count(), 0);
}

/// A queued save is seen by the next load before the disk has it, and the
/// disk has it after a flush. Two queued saves of one path: the latest
/// wins. This is what lets the window save on every change without
/// waiting twenty milliseconds for a synced write each time.
#[test]
fn a_queued_save_is_seen_at_once_and_lands_on_flush() {
    let dir = temp_dir("queued");
    let path = dir.join("workspace.json");
    let store = Store::<Desk>::new(path.clone());

    let mut first = Desk::default();
    first.add_project(&dir.join("one"));
    store.save_soon(&first).expect("queued");
    assert_eq!(
        store.load().unwrap().projects().count(),
        1,
        "the next load answers from the queue"
    );

    let mut second = Desk::default();
    second.add_project(&dir.join("one"));
    second.add_project(&dir.join("two"));
    store.save_soon(&second).expect("queued");
    guild_session::flush();

    let on_disk: Desk = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        on_disk.projects().count(),
        2,
        "the latest snapshot is what landed"
    );
    assert!(
        !dir.join("workspace.json.tmp").exists(),
        "and nothing beside it"
    );
}

/// **A save and a queued save of one file never race** — 2026-09-16.
/// The window saved its settings both ways: a panel dragged queued a
/// save, and each frame's terminal fit wrote one at once. Two writers of
/// one scratch file rename it from under each other, and the loser says
/// *No such file or directory* — the notice a person saw while dragging a
/// dock. Every write of a path goes through one thread.
#[test]
fn a_save_and_a_queued_save_of_one_file_never_race() {
    let dir = temp_dir("race");
    let path = dir.join("workspace.json");
    let store = Store::<Desk>::new(path.clone());
    let mut state = Desk::default();
    state.add_project(&dir.join("one"));

    for _ in 0..300 {
        store.save_soon(&state).expect("queued");
        store
            .save(&state)
            .expect("a save beside a queued one lands");
    }
    guild_session::flush();
}

/// A queued save that fails is reported once, to whoever asks.
#[test]
fn a_failed_queued_save_is_reported_once() {
    let _serial = FAILURE_TESTS.lock().unwrap();
    let dir = temp_dir("queued-failure");
    std::fs::create_dir_all(&dir).unwrap();
    let blocker = dir.join("not-a-directory");
    std::fs::write(&blocker, "").unwrap();
    let store = Store::<Desk>::new(blocker.join("workspace.json"));

    store.save_soon(&Desk::default()).expect("queued");
    guild_session::flush();

    let (path, reason) = guild_session::take_failure().expect("the failure is reported");
    assert!(path.starts_with(&blocker), "{path:?}");
    assert!(!reason.is_empty());
    assert!(guild_session::take_failure().is_none(), "once");
}

/// The head of a real project's `.guild/state.json`, cut where a crash or
/// a newer build's variant would leave an older build unable to read it —
/// the shape taken from the disk, not from memory, with the directory and
/// the ids replaced.
const TRUNCATED_STATE: &str = r#"{
  "sessions": [
    {
      "id": "20a701ff-6388-49b4-b85b-2909c76d42ca",
      "project": "31066fd6-b018-4ac3-a037-12e087e3d2e1",
      "title": "guild",
      "cwd": "/Users/someone/work/guild",
      "runs": {
        "claude": "terminal"
      },
      "cost_usd": 0.0,
      "topic": null,
      "name": null,
      "usage": {
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_read_tokens": 0,
        "cache_creation_tokens": 0,
        "cost_usd": null
      },
      "transcript_offset": 0,
      "had_turn": false,
      "open": true,
      "task": fal"#;

/// Every file beside `state.json` whose name says it was kept unreadable.
fn kept_beside(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("state.json.unreadable-"))
                })
                .collect()
        })
        .unwrap_or_default();
    kept.sort();
    kept
}

/// **An unreadable file is moved aside before anything can write over
/// it.** `load` answered a default and the next two saves turned the file
/// into `.bak` and then into nothing; now it is renamed beside itself,
/// byte for byte, and the report names where. Decision 21 of M5.11.
#[test]
fn an_unreadable_file_is_moved_aside_before_anything_writes_over_it() {
    let dir = temp_dir("unreadable-aside");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");
    std::fs::write(&path, TRUNCATED_STATE).unwrap();
    let store = Store::<ProjectState>::new(path.clone());

    let (state, report) = store.load_reporting().expect("a file that reads");
    assert_eq!(
        serde_json::to_string(&state).unwrap(),
        serde_json::to_string(&ProjectState::default()).unwrap(),
        "no backup: a clean start"
    );
    let report = report.expect("an unreadable file is reported");
    assert_eq!(report.path, path);
    assert!(!report.from_backup);
    assert!(!path.exists(), "moved, not copied");
    assert_eq!(
        std::fs::read_to_string(&report.kept).unwrap(),
        TRUNCATED_STATE,
        "kept byte for byte"
    );

    store.save(&ProjectState::default()).unwrap();
    store.save(&ProjectState::default()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&report.kept).unwrap(),
        TRUNCATED_STATE,
        "two saves later it is still there and unchanged"
    );
    assert_eq!(kept_beside(&dir), std::slice::from_ref(&report.kept));
}

/// With a good previous copy the state comes back from it, and the
/// unreadable file is still kept rather than left for the next save to
/// push into `.bak` over that very copy.
#[test]
fn an_unreadable_file_with_a_good_backup_is_kept_and_the_backup_used() {
    let dir = temp_dir("unreadable-backup");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());
    let mut good = Desk::default();
    good.add_project(&dir);
    store.save(&good).unwrap();
    store.save(&good).unwrap();
    std::fs::write(&path, "{\"projects\": [{\"name\": \"surv").unwrap();

    let (state, report) = store.load_reporting().expect("a file that reads");
    assert_eq!(state.projects().count(), 1, "the backup's state");
    let report = report.expect("reported");
    assert!(report.from_backup);
    assert_eq!(
        std::fs::read_to_string(&report.kept).unwrap(),
        "{\"projects\": [{\"name\": \"surv"
    );
    assert!(dir.join("workspace.json.bak").exists(), "the backup stays");
}

/// **A file that cannot be moved aside is still named.** A directory the
/// rename is refused in is the case: the backup reads, the state comes
/// back from it, and a load that said nothing would leave an unreadable
/// file where it is with nobody told — the next save that can rename
/// anything pushes it over that good backup. The report names the file
/// where it still is.
#[test]
fn an_unreadable_file_that_cannot_be_moved_is_still_named() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("unreadable-unmoved");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());
    let mut good = Desk::default();
    good.add_project(&dir);
    store.save(&good).unwrap();
    store.save(&good).unwrap();
    std::fs::write(&path, "{\"projects\": [{\"name\": \"surv").unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let (state, report) = store.load_reporting().expect("a file that reads");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(state.projects().count(), 1, "the backup's state");
    let report = report.expect("an unreadable file is reported even where it stays");
    assert!(report.from_backup);
    assert_eq!(report.kept, path, "kept where it is");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\"projects\": [{\"name\": \"surv"
    );
    assert!(kept_beside(&dir).is_empty());
}

/// A file that is not there is the ordinary first run, not a failure.
#[test]
fn a_missing_file_is_not_unreadable() {
    let dir = temp_dir("unreadable-missing");
    let (state, report) = Store::<ProjectState>::new(dir.join("state.json"))
        .load_reporting()
        .expect("a file that reads");
    assert_eq!(
        serde_json::to_string(&state).unwrap(),
        serde_json::to_string(&ProjectState::default()).unwrap()
    );
    assert!(report.is_none());
}

/// **Kept once.** The realistic cause — a newer build beside an older one
/// — writes the same bytes back again and again, and a load meets them on
/// every gesture; a copy per load would fill the directory. The same bytes
/// as the newest copy kept are removed instead; different bytes are a new
/// copy under a name of their own.
#[test]
fn the_same_unreadable_bytes_are_kept_once() {
    let dir = temp_dir("unreadable-once");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");
    let store = Store::<ProjectState>::new(path.clone());

    std::fs::write(&path, TRUNCATED_STATE).unwrap();
    let (_, first) = store.load_reporting().expect("a file that reads");
    let first = first.expect("reported");

    std::fs::write(&path, TRUNCATED_STATE).unwrap();
    let (_, again) = store.load_reporting().expect("a file that reads");
    assert_eq!(
        kept_beside(&dir),
        std::slice::from_ref(&first.kept),
        "one copy"
    );
    assert!(!path.exists(), "and the file is gone again");
    assert_eq!(again.map(|report| report.kept), Some(first.kept.clone()));

    std::fs::write(&path, "{\"sessions\": [").unwrap();
    let (_, other) = store.load_reporting().expect("a file that reads");
    let other = other.expect("reported");
    let kept = kept_beside(&dir);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_ne!(other.kept, first.kept, "a name of its own");
    assert_eq!(
        std::fs::read_to_string(&other.kept).unwrap(),
        "{\"sessions\": ["
    );
}

/// The file's identity on the disk: a rename into place gives it a new
/// one, so an unchanged identity is a write that did not happen.
fn identity(path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).expect("the file is there").ino()
}

/// **The same state saved again is not written** — 2026-09-24. The
/// window saves every open project whenever anything changes, and while
/// one agent worked eighteen projects had their `state.json` renamed,
/// synced and backed up every few seconds with the bytes they already
/// held. A save whose bytes are the file's leaves the file alone, queued
/// or waited for.
#[test]
fn the_same_state_saved_again_is_not_written() {
    let dir = temp_dir("unchanged");
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());
    let mut state = Desk::default();
    state.add_project(&dir);
    store.save(&state).expect("the first save");
    store.save(&state).expect("the second save");
    let before = identity(&path);

    store.save(&state).expect("a save of the same state");
    store.save_soon(&state).expect("queued");
    guild_session::flush();

    assert_eq!(identity(&path), before, "the file was not replaced");
    assert!(!dir.join("workspace.json.tmp").exists());
}

/// **The backup is the previous different copy.** A save that is skipped
/// does not push the file into `.bak`, so the copy kept beside is the
/// state before the last change rather than the same bytes twice.
#[test]
fn the_backup_is_the_previous_different_copy() {
    let dir = temp_dir("backup-different");
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());
    store.save(&Desk::default()).expect("the first save");
    let first = std::fs::read_to_string(&path).unwrap();
    let mut second = Desk::default();
    second.add_project(&dir);

    store.save(&second).expect("the second save");
    store.save(&second).expect("the same again");

    assert_eq!(
        std::fs::read_to_string(dir.join("workspace.json.bak")).unwrap(),
        first,
        "the backup still holds what was there before the change"
    );
}

/// **The disk is what is compared, not what was last written.** A file
/// changed by someone else — another build, a person — is written over by
/// the next save as it always was.
#[test]
fn a_file_changed_on_the_disk_is_written_again() {
    let dir = temp_dir("changed-outside");
    let path = dir.join("workspace.json");
    let store: Store<Desk> = Store::new(path.clone());
    let mut state = Desk::default();
    state.add_project(&dir);
    store.save(&state).expect("the first save");
    store.save(&state).expect("the second save");
    let ours = std::fs::read_to_string(&path).unwrap();

    std::fs::write(&path, r#"{"projects":[],"sessions":[]}"#).unwrap();
    store.save(&state).expect("the same state again");

    assert_eq!(std::fs::read_to_string(&path).unwrap(), ours);
}

#[test]
fn a_successful_save_does_not_return_an_earlier_background_failure() {
    let _serial = FAILURE_TESTS.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("parent");
    std::fs::write(&parent, "blocker").unwrap();
    let path = parent.join("state.json");
    let store = Store::<Desk>::new(path.clone());
    store.save_soon(&Desk::default()).unwrap();
    guild_session::flush();
    std::fs::remove_file(&parent).unwrap();
    store.save(&Desk::default()).expect("this write succeeded");
    assert!(path.is_file());
}
/// Every file in `dir` with its bytes, by name: what a load and a save must
/// leave exactly as it was when they could not read.
fn listing(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

/// Every file in `dir` made readable again, wherever a load moved it.
fn readable_again(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o644)).unwrap();
        }
    }
}

/// A state file and its backup holding two different good states.
fn two_good_copies(dir: &std::path::Path) -> Store<Desk> {
    std::fs::create_dir_all(dir.join("second")).unwrap();
    let store: Store<Desk> = Store::new(dir.join("state.json"));
    let mut older = Desk::default();
    older.add_project(dir);
    store.save(&older).unwrap();
    let mut newer = older.clone();
    newer.add_project(&dir.join("second"));
    store.save(&newer).unwrap();
    store
}

/// **A file that could not be opened is not a broken file** — 2026-11-07.
/// The installed Guild ran out of descriptors (`EMFILE`), and every load
/// that met it moved a perfectly good `state.json` aside, then its
/// `.bak`, and answered a default the next save wrote over the person's
/// state: 44 files were kept as unreadable that day and all 44 were valid
/// JSON. The real condition, in a child with a small descriptor limit:
/// nothing is renamed, removed or written, the load says it failed, and no
/// save reaches the file for the rest of the run — not even once it reads
/// again, because whoever loaded the default may still be holding it.
#[test]
fn a_file_that_cannot_be_opened_is_left_where_it_is_and_never_saved_over() {
    const CHILD: &str = "GUILD_TEST_STORE_EMFILE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "store::a_file_that_cannot_be_opened_is_left_where_it_is_and_never_saved_over",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated descriptor regression failed");
        return;
    }

    let dir = temp_dir("emfile");
    std::fs::create_dir_all(&dir).unwrap();
    let store = two_good_copies(&dir);
    let before = listing(&dir);

    // Only this isolated child lowers its own soft limit, then takes every
    // descriptor left — the window's state on 2026-11-07.
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: out-parameter calls on this process's own limit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    limit.rlim_cur = 64.min(limit.rlim_max);
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    let mut held = Vec::new();
    let exhausted = loop {
        match std::fs::File::open("/dev/null") {
            Ok(file) => held.push(file),
            Err(err) => break err,
        }
    };
    assert_eq!(exhausted.raw_os_error(), Some(libc::EMFILE));

    let loaded = store.load();
    let saved = store.save(&Desk::default());
    let queued = store.save_soon(&Desk::default());
    drop(held);
    guild_session::flush();

    assert_eq!(listing(&dir), before, "nothing renamed, removed or written");
    assert!(kept_beside(&dir).is_empty());
    assert!(loaded.is_err(), "a default is not the person's state");
    assert!(saved.is_err(), "a save over an unread file is refused");
    assert!(queued.is_err(), "and so is a queued one");
    let failures = guild_session::take_read_failures();
    assert_eq!(failures.len(), 1, "said once: {failures:?}");
    assert_eq!(failures[0].path, dir.join("state.json"));
    assert!(
        failures[0].reason.contains("Too many open files"),
        "the system's own words: {}",
        failures[0].reason
    );

    let reread = store
        .load()
        .expect("the file reads once descriptors are free");
    assert_eq!(reread.projects().count(), 2, "the newer copy, intact");
    assert!(
        store.save(&Desk::default()).is_err(),
        "still refused this run: a default loaded earlier may be held"
    );
    guild_session::flush();
    assert_eq!(listing(&dir), before);
}

/// **A broken file whose backup cannot be opened stays with it.** The
/// main file's bytes were read and do not parse, which alone would keep
/// it aside and use the backup — but the backup could not be opened, so
/// whether it is good is unknown, and a default saved now would rotate
/// over it. Both stay exactly as they are and the path is not saved to.
#[test]
fn a_broken_file_whose_backup_cannot_be_opened_is_left_with_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("backup-unopened");
    std::fs::create_dir_all(&dir).unwrap();
    let store = two_good_copies(&dir);
    let path = dir.join("state.json");
    let backup = dir.join("state.json.bak");
    std::fs::write(&path, "{\"projects\": [{\"name\": \"surv").unwrap();
    let before = listing(&dir);
    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o000)).unwrap();

    let loaded = store.load();
    let saved = store.save(&Desk::default());
    guild_session::flush();
    readable_again(&dir);

    assert_eq!(listing(&dir), before, "nothing renamed, removed or written");
    assert!(kept_beside(&dir).is_empty());
    assert!(loaded.is_err());
    assert!(saved.is_err());
}

/// **The caller that did the damage: a desk loaded and saved.**
/// `persist::load` reads each project's own file and `persist::save` reads
/// it again and writes it back; a project file that could not be opened
/// came back as a default and went out over the real one. It stays as it
/// is now, and the desk is still saved.
#[test]
fn a_project_file_that_cannot_be_opened_survives_the_desk_saved_over_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("project-unopened");
    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let global = dir.join("workspace.json");
    let mut desk = Desk::default();
    desk.add_project(&project);
    Store::<Desk>::new(global.clone()).save(&desk).unwrap();
    let state = ProjectState {
        recent_files: vec![project.join("README.md")],
        ..ProjectState::default()
    };
    guild_session::project_state::save(&project, &state).unwrap();
    guild_session::flush();
    let file = guild_session::project_state::dir(&project).join("state.json");
    let before = std::fs::read(&file).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

    let mut loaded = guild_session::persist::load(&global);
    let saved = guild_session::persist::save(&mut loaded, &global);
    guild_session::flush();
    readable_again(&guild_session::project_state::dir(&project));

    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "the project's state, untouched"
    );
    assert!(kept_beside(&guild_session::project_state::dir(&project)).is_empty());
    assert!(
        saved.is_ok(),
        "the workspace file itself is still written: {saved:?}"
    );
    assert_eq!(loaded.projects().count(), 1);
}
