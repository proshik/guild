//! `scripts/swap-guild.sh`, the helper that puts a built Guild in place
//! of the one that ran — M5.15. Run here with no pid, so it waits for
//! nothing and opens nothing: the half with a pid is checked in a window.

use crate::scratch::Scratch;
use std::path::Path;
use std::process::Command;

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/swap-guild.sh");

fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("guild-swap-{name}-"))
}

/// A folder shaped enough like a bundle, carrying a word to tell it by.
fn bundle(at: &Path, marker: &str) {
    std::fs::create_dir_all(at.join("Contents")).unwrap();
    std::fs::write(at.join("Contents/marker"), marker).unwrap();
}

fn marker(at: &Path) -> String {
    std::fs::read_to_string(at.join("Contents/marker")).unwrap()
}

fn swap(new: &Path, old: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(SCRIPT)
        .arg(new)
        .arg(old)
        .output()
        .unwrap()
}

/// The bundle given is the one replaced, whatever it is called — the
/// helper of M5.13 could only replace `<folder>/Guild.app`.
#[test]
fn the_helper_replaces_the_bundle_it_is_given() {
    let dir = scratch("replaces");
    let new = dir.join("new/Guild.app");
    let old = dir.join("apps/Guild 2.app");
    bundle(&new, "new");
    bundle(&old, "old");

    let out = swap(&new, &old);

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(marker(&old), "new");
    assert!(!dir.join("apps/Guild 2.app.old").exists());
    assert_eq!(
        std::fs::read_dir(dir.join("apps")).unwrap().count(),
        1,
        "nothing beside the bundle"
    );
}

/// A copy that fails leaves the Guild that was there, whole.
#[test]
fn a_failed_copy_puts_the_old_bundle_back() {
    let dir = scratch("fails");
    let old = dir.join("apps/Guild.app");
    bundle(&old, "old");

    let out = swap(&dir.join("nowhere/Guild.app"), &old);

    assert!(!out.status.success());
    assert_eq!(marker(&old), "old");
    assert!(!dir.join("apps/Guild.app.old").exists());
}

/// The window's run, without the pid: the helper copied into a folder of
/// its own beside the copy it swaps in, and told that folder is to go.
/// The pid is `""` — waited for by nobody, reopened by nobody.
fn swap_from(folder: &Path, new: &Path, old: &Path, remove: &Path) -> std::process::Output {
    std::fs::create_dir_all(folder).unwrap();
    let helper = folder.join("swap-guild.sh");
    std::fs::copy(SCRIPT, &helper).unwrap();
    Command::new("sh")
        .arg(&helper)
        .arg(new)
        .arg(old)
        .arg("")
        .arg(remove)
        .output()
        .unwrap()
}

/// The window's folder, `$TMPDIR/guild-update-<pid>`, goes once the swap
/// is over — the helper it holds included, which is the file `sh` is
/// reading: unlinked, it stays readable through the open descriptor, and
/// the exit is still 0.
#[test]
fn the_window_s_folder_goes_after_a_swap() {
    let dir = scratch("folder-swap");
    let folder = dir.join("guild-update-1");
    let new = folder.join("Guild.app");
    let old = dir.join("apps/Guild.app");
    bundle(&new, "new");
    bundle(&old, "old");

    let out = swap_from(&folder, &new, &old, &folder);

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(marker(&old), "new");
    assert!(!folder.exists(), "the window's folder is gone");
}

/// A failed copy puts the old Guild back — and the folder is still the
/// window's garbage, so it goes as well.
#[test]
fn the_window_s_folder_goes_after_a_failed_copy() {
    let dir = scratch("folder-fails");
    let folder = dir.join("guild-update-1");
    let old = dir.join("apps/Guild.app");
    bundle(&old, "old");

    let out = swap_from(&folder, &folder.join("Guild.app"), &old, &folder);

    assert!(!out.status.success());
    assert_eq!(marker(&old), "old");
    assert!(!folder.exists(), "the window's folder is gone");
}

/// A folder is removed only when it is the one holding the new bundle —
/// the window's own. Given `scripts/` or a bundle's `Resources/` by
/// mistake, the helper swaps and removes nothing of it.
#[test]
fn a_folder_that_does_not_hold_the_new_bundle_is_kept() {
    let dir = scratch("folder-kept");
    let new = dir.join("target/release/Guild.app");
    let old = dir.join("apps/Guild.app");
    let checkout = dir.join("scripts");
    let resources = dir.join("Other.app/Contents/Resources");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("neighbour"), "kept").unwrap();
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(resources.join("built"), "kept").unwrap();

    for folder in [&checkout, &resources] {
        bundle(&new, "new");
        bundle(&old, "old");

        let out = swap_from(folder, &new, &old, folder);

        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(marker(&old), "new");
        assert!(folder.join("swap-guild.sh").is_file(), "{folder:?}");
    }
    assert!(checkout.join("neighbour").is_file());
    assert!(resources.join("built").is_file());
}

/// Without a folder the helper removes nothing but the new bundle it
/// swapped in — run from a bundle's `Resources/`, that folder stays.
#[test]
fn without_a_folder_the_helper_s_own_stays() {
    let dir = scratch("no-folder");
    let resources = dir.join("Other.app/Contents/Resources");
    let new = resources.join("Guild.app");
    let old = dir.join("apps/Guild.app");
    bundle(&new, "new");
    bundle(&old, "old");
    std::fs::copy(SCRIPT, resources.join("swap-guild.sh")).unwrap();

    let out = Command::new("sh")
        .arg(resources.join("swap-guild.sh"))
        .arg(&new)
        .arg(&old)
        .output()
        .unwrap();

    assert!(out.status.success());
    assert_eq!(marker(&old), "new");
    assert!(resources.join("swap-guild.sh").is_file());
}

/// What the removal leans on, measured rather than assumed: `sh` reads a
/// script through the descriptor it opened, so a script whose folder is
/// removed under it still runs the lines after the removal. The helper's
/// trap runs last anyway; this is why it would not matter if it did not.
#[test]
fn a_script_whose_folder_is_removed_reads_on() {
    let dir = scratch("unlinked");
    let folder = dir.join("guild-update-1");
    std::fs::create_dir_all(&folder).unwrap();
    let script = folder.join("probe.sh");
    std::fs::write(
        &script,
        "rm -rf \"$1\"\n[ ! -e \"$0\" ] && echo read on after the removal\n",
    )
    .unwrap();

    let out = Command::new("sh")
        .arg(&script)
        .arg(&folder)
        .output()
        .unwrap();

    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "read on after the removal\n"
    );
    assert!(!folder.exists());
}

#[test]
fn a_swap_refreshes_the_bundle_timestamp_for_launch_services() {
    let dir = scratch("refresh-icon");
    let new = dir.join("new/Guild.app");
    let old = dir.join("apps/Guild.app");
    bundle(&new, "new");
    assert!(
        Command::new("touch")
            .args(["-t", "200001010000"])
            .arg(&new)
            .status()
            .unwrap()
            .success()
    );
    let before = std::time::SystemTime::now();
    assert!(swap(&new, &old).status.success());
    assert!(
        std::fs::metadata(&old).unwrap().modified().unwrap() >= before,
        "ditto preserves the staged timestamp; the installed bundle must announce replacement"
    );
}
