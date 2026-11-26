//! The Guild you work in, updating itself — M5.13 and M5.15: what a
//! bundle says it was built from and when, what staged build is offered,
//! and the copy the click makes of it.

use std::path::Path;

use crate::scratch::Scratch;
use guild_desktop::update;
use guild_session::changelog::Group;

const COMMIT: &str = "75315fa0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6";

fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("guild-update-{name}-"))
}

/// A bundle's `Resources` with its `commit` written.
fn resources(name: &str, commit: &str) -> Scratch {
    let dir = scratch(name);
    std::fs::write(dir.join("commit"), commit).unwrap();
    dir
}

/// What the build says of itself, for About and the Update button: the
/// commit short, the dogfood tag when it has one, and whether the tree
/// had changes.
#[test]
fn a_build_names_its_commit_its_tag_and_its_dirt() {
    let tagged = resources("tagged", &format!("{COMMIT}\n"));
    std::fs::write(tagged.join("tag"), "dogfood-2026-09-23\n").unwrap();
    let build = update::Build::read(&tagged).expect("a commit is enough");
    assert_eq!(build.short(), "75315fa");
    assert_eq!(build.label(), "75315fa · dogfood-2026-09-23");

    let dirty = scratch("build-dirty");
    std::fs::write(dirty.join("commit"), format!("{COMMIT}\ndirty\n")).unwrap();
    assert_eq!(
        update::Build::read(&dirty).unwrap().label(),
        "75315fa · dirty"
    );

    assert_eq!(update::Build::read(&scratch("build-none")), None);
}

/// M5.15: `bundle` writes when it built, in UTC seconds. A bundle
/// without the file is still a build — About and the status bar read it
/// — it just cannot say when.
#[test]
fn a_build_says_when_it_was_built() {
    let dir = resources("built", &format!("{COMMIT}\n"));
    std::fs::write(dir.join("built"), "1790000000\n").unwrap();
    assert_eq!(update::Build::read(&dir).unwrap().built, Some(1790000000));

    for nonsense in ["", "yesterday\n", "-5\n"] {
        std::fs::write(dir.join("built"), nonsense).unwrap();
        let build = update::Build::read(&dir).expect("still a build");
        assert_eq!(build.built, None, "{nonsense:?}");
    }

    std::fs::remove_file(dir.join("built")).unwrap();
    assert_eq!(update::Build::read(&dir).unwrap().built, None);
}

#[test]
fn the_staged_bundle_lives_in_the_state_folder() {
    assert_eq!(
        update::staged_bundle(Path::new("/s")),
        Path::new("/s/update/Guild.app")
    );
}

/// A fixture bundle: the executable, a commit, and a build time when
/// one is given.
fn bundle(name: &str, built: Option<u64>) -> Scratch {
    let app = scratch(name).within("Guild.app");
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/MacOS/guild-desktop"), "").unwrap();
    std::fs::write(resources.join("commit"), format!("{COMMIT}\n")).unwrap();
    // A release, since M5.16 only a nightly is offered — and a copy is
    // judged by the same rule.
    std::fs::write(resources.join("tag"), "nightly-2026-09-24\n").unwrap();
    if let Some(built) = built {
        std::fs::write(resources.join("built"), format!("{built}\n")).unwrap();
    }
    app
}

/// A staging folder half-written is not a build: without the executable
/// there is nothing to run.
#[test]
fn a_folder_is_a_bundle_only_with_its_executable() {
    let app = bundle("of-bundle", Some(7));
    assert_eq!(update::Build::of_bundle(&app).unwrap().built, Some(7));

    std::fs::remove_file(app.join("Contents/MacOS/guild-desktop")).unwrap();
    assert_eq!(update::Build::of_bundle(&app), None);
    assert_eq!(update::Build::of_bundle(&scratch("of-nothing")), None);
}

/// Decision 4 of the spec, row by row: built later is offered; the same
/// build or an older one is not; a debug build is offered anything that
/// says when it was built; a staged bundle that does not say is older
/// than the button, and never offered.
#[test]
fn what_is_offered_is_what_was_built_later() {
    let build = |built: Option<u64>| update::Build {
        commit: COMMIT.into(),
        dirty: false,
        tag: Some("nightly-2026-09-24".into()),
        built,
    };
    let running = build(Some(100));

    assert!(update::offered(Some(&running), &build(Some(200))));
    assert!(!update::offered(Some(&running), &build(Some(100))));
    assert!(!update::offered(Some(&running), &build(Some(50))));

    assert!(update::offered(None, &build(Some(1))));

    assert!(!update::offered(Some(&running), &build(None)));
    assert!(!update::offered(Some(&build(None)), &build(None)));
    assert!(!update::offered(None, &build(None)));
}

/// The bundle an update replaces is the one this process runs from,
/// wherever and whatever it is called — and a debug build is none.
#[test]
fn the_running_bundle_is_found_from_its_executable() {
    assert_eq!(
        update::running_bundle(Path::new(
            "/Applications/Guild.app/Contents/MacOS/guild-desktop"
        )),
        Some("/Applications/Guild.app".into())
    );
    assert_eq!(
        update::running_bundle(Path::new(
            "/Users/a/Guild 2.app/Contents/MacOS/guild-desktop"
        )),
        Some("/Users/a/Guild 2.app".into())
    );
    assert_eq!(
        update::running_bundle(Path::new("/w/target/debug/guild-desktop")),
        None
    );
    assert_eq!(
        update::running_bundle(Path::new("/x/Contents/MacOS/guild-desktop")),
        None
    );
}

/// A bundle `codesign` can seal: the fixture above with an `Info.plist`
/// and an executable that is a file of its own, signed as `bundle` signs.
fn sealed(name: &str, built: u64) -> Scratch {
    let app = bundle(name, Some(built));
    std::fs::write(
        app.join("Contents/Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
         <key>CFBundleIdentifier</key><string>dev.guild.fixture</string>\
         <key>CFBundleExecutable</key><string>guild-desktop</string></dict></plist>\n",
    )
    .unwrap();
    std::fs::write(app.join("Contents/MacOS/guild-desktop"), "#!/bin/sh\n").unwrap();
    let signed = std::process::Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(app.as_os_str())
        .output()
        .unwrap();
    assert!(signed.status.success(), "{signed:?}");
    app
}

/// The running bundle: what it is, and the helper it carries.
fn running(name: &str, built: u64) -> (Scratch, update::Build) {
    let app = bundle(name, Some(built));
    std::fs::write(
        app.join("Contents/Resources/swap-guild.sh"),
        "#!/bin/sh\n# the running bundle's own\n",
    )
    .unwrap();
    let build = update::Build::of_bundle(&app).unwrap();
    (app, build)
}

/// Step 4 of decision 7: a copy of the staged bundle, read back, and the
/// running bundle's helper beside it — the copy, because the helper
/// deletes the bundle it came from.
#[test]
fn a_prepared_update_is_a_copy_and_the_helper_beside_it() {
    let staged = sealed("prepare-staged", 200);
    let (app, build) = running("prepare-running", 100);
    let dir = scratch("prepare-dir").within("guild-update");

    let prepared = update::swap::prepare(&staged, &app, Some(&build), &dir).expect("offered");

    assert_eq!(prepared.bundle, dir.join("Guild.app"));
    assert_eq!(prepared.build.built, Some(200));
    assert_eq!(
        std::fs::read_to_string(&prepared.helper).unwrap(),
        "#!/bin/sh\n# the running bundle's own\n"
    );
    assert!(staged.exists(), "the staged bundle stays where it is");
}

/// What the copy caught is what is judged: the same build is no update,
/// and a no leaves nothing behind.
#[test]
fn a_copy_that_is_not_newer_is_refused() {
    let staged = sealed("stale-staged", 100);
    let (app, build) = running("stale-running", 100);
    let dir = scratch("stale-dir").within("guild-update");

    let refused = update::swap::prepare(&staged, &app, Some(&build), &dir);

    assert_eq!(refused.err(), Some(update::swap::Failure::NotNewer));
    assert!(!dir.exists());
}

#[test]
fn no_helper_in_the_running_bundle_is_refused() {
    let staged = sealed("helperless-staged", 200);
    let app = bundle("helperless-running", Some(100));
    let build = update::Build::of_bundle(&app).unwrap();
    let dir = scratch("helperless-dir").within("guild-update");

    let refused = update::swap::prepare(&staged, &app, Some(&build), &dir);

    assert_eq!(refused.err(), Some(update::swap::Failure::NoHelper));
    assert!(!dir.exists());
}

/// Task 0 of the plan: `ditto` walks by name, so a copy that `_stage`
/// renamed a new bundle under could mix two builds and exit 0. The seal
/// is what catches it — here, a file changed after signing.
#[test]
fn a_copy_whose_seal_is_broken_is_refused() {
    let staged = sealed("torn-staged", 200);
    std::fs::write(
        staged.join("Contents/Resources/commit"),
        format!("{COMMIT}\ndirty\n"),
    )
    .unwrap();
    let (app, build) = running("torn-running", 100);
    let dir = scratch("torn-dir").within("guild-update");

    let refused = update::swap::prepare(&staged, &app, Some(&build), &dir);

    assert!(
        matches!(refused, Err(update::swap::Failure::Seal(_))),
        "{refused:?}"
    );
    assert!(!dir.exists());
}

/// Step 5: the helper is told the copy, the bundle it replaces, the pid
/// to wait for — and the folder the click made, which it removes when
/// the swap is over. Told, not guessed from where it runs: the same
/// script runs from `scripts/` and from a bundle's `Resources/`.
#[test]
fn the_helper_is_told_the_folder_to_remove() {
    let staged = sealed("told-staged", 200);
    let (app, build) = running("told-running", 100);
    let dir = scratch("told-dir").within("guild-update");
    let prepared = update::swap::prepare(&staged, &app, Some(&build), &dir).expect("offered");

    let command = update::swap::helper_command(&prepared, &app, 4242);

    assert_eq!(command.get_program(), "sh");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            dir.join("swap-guild.sh").as_os_str(),
            dir.join("Guild.app").as_os_str(),
            app.as_os_str(),
            "4242".as_ref(),
            dir.as_os_str(),
        ]
    );
}

fn tagged(tag: Option<&str>, built: Option<u64>) -> update::Build {
    update::Build {
        commit: COMMIT.into(),
        dirty: false,
        tag: tag.map(str::to_owned),
        built,
    }
}

/// M5.16's decision 1: a dev build is never offered, however late it was
/// built — only a release, and only one built after what runs. A dev
/// build running is offered the next nightly, the way back to `main`.
#[test]
fn only_a_nightly_built_later_is_offered() {
    let running = tagged(Some("nightly-2026-09-24"), Some(100));

    assert!(!update::offered(
        Some(&running),
        &tagged(Some("dogfood-2026-09-25"), Some(200))
    ));
    assert!(!update::offered(Some(&running), &tagged(None, Some(200))));
    assert!(update::offered(
        Some(&running),
        &tagged(Some("nightly-2026-09-25"), Some(200))
    ));
    assert!(!update::offered(
        Some(&running),
        &tagged(Some("nightly-2026-09-23"), Some(50))
    ));

    let dev = tagged(Some("dogfood-2026-09-24"), Some(100));
    assert!(update::offered(
        Some(&dev),
        &tagged(Some("nightly-2026-09-25"), Some(200))
    ));
    assert!(update::offered(
        None,
        &tagged(Some("nightly-2026-09-25"), Some(1))
    ));
    assert!(!update::offered(
        None,
        &tagged(Some("dogfood-2026-09-25"), Some(1))
    ));
}

/// A build's version is its release name, and a dev build has none.
#[test]
fn a_version_is_a_nightly_tag() {
    assert_eq!(
        tagged(Some("nightly-2026-09-24"), None).version(),
        Some("nightly-2026-09-24")
    );
    assert_eq!(tagged(Some("dogfood-2026-09-24"), None).version(), None);
    assert_eq!(tagged(None, None).version(), None);
}

/// `just bundle` puts `CHANGELOG.md` in `Resources/`; a bundle without
/// one has nothing to say, which is no error.
#[test]
fn a_bundle_carries_its_changelog() {
    let app = bundle("changelog", Some(1));
    assert!(update::changelog_of(&app).is_empty());

    std::fs::write(
        app.join("Contents/Resources/CHANGELOG.md"),
        "## [nightly-2026-09-24] - 2026-09-24\n\n### Fixed\n\n- A fix.\n",
    )
    .unwrap();
    let releases = update::changelog_of(&app);
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].version, "nightly-2026-09-24");
}

const RELEASED: &str = "\
## [Unreleased]

## [nightly-2026-09-26] - 2026-09-26

### Fixed

- C fixed.

## [nightly-2026-09-25] - 2026-09-25

### Added

- B added.

## [nightly-2026-09-24] - 2026-09-24

### Security

- A secured.
";

/// A running bundle of `tag`, carrying the changelog above.
fn released(name: &str, tag: &str) -> (Scratch, update::Build) {
    let app = bundle(name, Some(100));
    std::fs::write(app.join("Contents/Resources/tag"), format!("{tag}\n")).unwrap();
    std::fs::write(app.join("Contents/Resources/CHANGELOG.md"), RELEASED).unwrap();
    let build = update::Build::of_bundle(&app).unwrap();
    (app, build)
}

fn seen(state: &Path) -> Option<String> {
    std::fs::read_to_string(update::seen_file(state)).ok()
}

/// The first nightly started with no record: its own section, and the
/// record written the moment it is shown.
#[test]
fn a_first_nightly_greets_with_its_own_section() {
    let state = scratch("greet-first");
    let (app, build) = released("greet-first-app", "nightly-2026-09-26");

    let greeting = update::greet(&state, Some(&build), Some(&app)).expect("a greeting");

    assert_eq!(greeting.version, "nightly-2026-09-26");
    assert_eq!(greeting.since, None);
    assert_eq!(
        greeting.releases,
        [update::Section {
            version: "nightly-2026-09-26".into(),
            date: Some("2026-09-26".into()),
            notes: vec![(Group::Fixed, vec!["C fixed.".to_owned()])],
        }]
    );
    assert_eq!(seen(&state).as_deref(), Some("nightly-2026-09-26\n"));
    assert_eq!(
        update::greet(&state, Some(&build), Some(&app)),
        None,
        "once"
    );
}

/// Everything since the one run before — two releases skipped are two
/// blocks, newest first, each with its own groups: the person chose a
/// release's block over one merged list on 2026-09-25.
#[test]
fn a_greeting_covers_every_release_since_the_one_seen() {
    let state = scratch("greet-since");
    std::fs::write(update::seen_file(&state), "nightly-2026-09-24\n").unwrap();
    let (app, build) = released("greet-since-app", "nightly-2026-09-26");

    let greeting = update::greet(&state, Some(&build), Some(&app)).expect("a greeting");

    assert_eq!(greeting.since.as_deref(), Some("nightly-2026-09-24"));
    let blocks: Vec<(&str, &guild_session::changelog::Notes)> = greeting
        .releases
        .iter()
        .map(|section| (section.version.as_str(), &section.notes))
        .collect();
    assert_eq!(
        blocks,
        [
            (
                "nightly-2026-09-26",
                &vec![(Group::Fixed, vec!["C fixed.".to_owned()])]
            ),
            (
                "nightly-2026-09-25",
                &vec![(Group::Added, vec!["B added.".to_owned()])]
            ),
        ]
    );
}

/// A release in the range that says nothing is no block of its own.
#[test]
fn a_release_with_no_entries_is_no_block() {
    let state = scratch("greet-quiet");
    std::fs::write(update::seen_file(&state), "nightly-2026-09-24\n").unwrap();
    let app = bundle("greet-quiet-app", Some(100));
    std::fs::write(app.join("Contents/Resources/tag"), "nightly-2026-09-26\n").unwrap();
    std::fs::write(
        app.join("Contents/Resources/CHANGELOG.md"),
        RELEASED.replace("### Added\n\n- B added.\n", ""),
    )
    .unwrap();
    let build = update::Build::of_bundle(&app).unwrap();

    let greeting = update::greet(&state, Some(&build), Some(&app)).expect("a greeting");

    let versions: Vec<&str> = greeting
        .releases
        .iter()
        .map(|s| s.version.as_str())
        .collect();
    assert_eq!(versions, ["nightly-2026-09-26"]);
}

/// A dev build, a debug build: nothing shown, and nothing written, so
/// the next nightly still knows which one was seen last.
#[test]
fn a_dev_build_greets_with_nothing_and_writes_nothing() {
    let state = scratch("greet-dev");
    let (app, build) = released("greet-dev-app", "dogfood-2026-09-26");

    assert_eq!(update::greet(&state, Some(&build), Some(&app)), None);
    assert_eq!(update::greet(&state, None, None), None);
    assert_eq!(seen(&state), None);
}

/// A release that says nothing is not a card, and is still seen.
#[test]
fn an_empty_release_greets_with_nothing_and_is_seen() {
    let state = scratch("greet-empty");
    let (app, build) = released("greet-empty-app", "nightly-2026-09-27");

    assert_eq!(update::greet(&state, Some(&build), Some(&app)), None);
    assert_eq!(seen(&state).as_deref(), Some("nightly-2026-09-27\n"));
}

/// What the hover card shows: the staged changelog since the running
/// version.
#[test]
fn the_notes_of_an_update_are_those_since_the_running_version() {
    let releases = guild_session::changelog::parse(RELEASED);

    assert_eq!(
        update::notes(&releases, Some("nightly-2026-09-25"), "nightly-2026-09-26"),
        [(Group::Fixed, vec!["C fixed.".to_owned()])]
    );
    assert_eq!(
        update::notes(&releases, None, "nightly-2026-09-26"),
        [(Group::Fixed, vec!["C fixed.".to_owned()])]
    );
    assert_eq!(
        update::notes(&releases, Some("nightly-2026-09-24"), "nightly-2026-09-26").len(),
        2
    );
}

#[test]
fn stable_versions_greet_without_expanding_legacy_nightly_install_policy() {
    let state = scratch("greet-stable");
    let (app, build) = released("greet-stable-app", "v1.2.0");
    std::fs::write(
        app.join("Contents/Resources/CHANGELOG.md"),
        "## [v1.2.0] - 2026-10-18\n### Added\n- Stable updates.\n",
    )
    .unwrap();
    assert_eq!(build.version(), Some("v1.2.0"));
    let greeting = update::greet(&state, Some(&build), Some(&app)).unwrap();
    assert_eq!(greeting.version, "v1.2.0");
    assert_eq!(greeting.releases[0].notes[0].1, ["Stable updates."]);
    assert!(update::greet(&state, Some(&build), Some(&app)).is_none());
    assert!(
        !update::offered(None, &build),
        "legacy bootstrap still requires Nightly"
    );
    for invalid in ["nightly-", "v01.2.0", "v1.2", "dogfood-2026-10-18"] {
        assert!(tagged(Some(invalid), Some(500)).version().is_none());
    }
}
