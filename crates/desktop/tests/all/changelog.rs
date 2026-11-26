//! `CHANGELOG.md`, as Guild reads it and `just release` writes it — M5.16.
//! The module is `guild_session::changelog`; it is checked here, beside
//! the window that shows what it reads.

use guild_session::changelog::{self, Group, Release};

const FILE: &str = "\
# Changelog

Everything a person would notice. Keep a Changelog 1.1.0.

## [Unreleased]

### Added

- Something not yet released.

## [nightly-2026-09-25] - 2026-09-25

### Security

- Sixth.

### Fixed

- Fifth.

### Removed

- Fourth.

### Deprecated

- Third.

### Changed

- Second.

### Added

- First.
- First, again.

## [nightly-2026-09-24] - 2026-09-24

### Fixed

- The oldest fix.
";

fn release<'a>(releases: &'a [Release], version: &str) -> &'a Release {
    releases
        .iter()
        .find(|release| release.version == version)
        .unwrap_or_else(|| panic!("{version} in {releases:?}"))
}

#[test]
fn a_changelog_is_its_releases_newest_first() {
    let releases = changelog::parse(FILE);

    let versions: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(
        versions,
        ["Unreleased", "nightly-2026-09-25", "nightly-2026-09-24"]
    );
    assert_eq!(releases[0].date, None);
    assert_eq!(releases[1].date.as_deref(), Some("2026-09-25"));
    assert_eq!(
        releases[0].entries,
        [(Group::Added, "Something not yet released.".to_owned())]
    );
}

/// The six names of keepachangelog, each read as its own group, in the
/// order the file has them.
#[test]
fn all_six_groups_are_read() {
    let releases = changelog::parse(FILE);

    let groups: Vec<(Group, &str)> = release(&releases, "nightly-2026-09-25")
        .entries
        .iter()
        .map(|(group, line)| (*group, line.as_str()))
        .collect();
    assert_eq!(
        groups,
        [
            (Group::Security, "Sixth."),
            (Group::Fixed, "Fifth."),
            (Group::Removed, "Fourth."),
            (Group::Deprecated, "Third."),
            (Group::Changed, "Second."),
            (Group::Added, "First."),
            (Group::Added, "First, again."),
        ]
    );
    let names: Vec<&str> = Group::ALL.iter().map(|group| group.name()).collect();
    assert_eq!(
        names,
        [
            "Added",
            "Changed",
            "Deprecated",
            "Removed",
            "Fixed",
            "Security"
        ]
    );
}

/// A line the form does not have costs that line, not the file: the
/// window reads a changelog it did not write.
#[test]
fn what_is_not_the_form_is_ignored_and_the_rest_is_kept() {
    let text = "\
# Changelog

- An entry before any release.

## [Unreleased]

- An entry before any group.

### Added

* A star bullet.
- Kept.
  a continuation of the line above
-Not an entry either.

### Notes

- Under a group keepachangelog does not have.

### Fixed

- Kept too.

## [nightly-2026-09-23]

### Added

- Under a heading with no date.

## [nightly-2026-09-22] - 22 September

### Added

- Under a heading whose date is not a date.

## [nightly-2026-09-21] - 2026-09-21

### Added

- The last release, whole.
";
    let releases = changelog::parse(text);

    let versions: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(versions, ["Unreleased", "nightly-2026-09-21"]);
    assert_eq!(
        releases[0].entries,
        [
            (Group::Added, "Kept.".to_owned()),
            (Group::Fixed, "Kept too.".to_owned()),
        ]
    );
    assert_eq!(
        releases[1].entries,
        [(Group::Added, "The last release, whole.".to_owned())]
    );
}

/// Inline markdown is shown as the characters it is — the person's
/// decision 4 — and a CRLF file reads as an LF one.
#[test]
fn an_entry_is_its_text_as_written() {
    let text = "## [Unreleased]\r\n\r\n### Fixed\r\n\r\n- `just install` sees **the** Guild.\r\n";

    assert_eq!(
        changelog::parse(text)[0].entries,
        [(
            Group::Fixed,
            "`just install` sees **the** Guild.".to_owned()
        )]
    );
}

fn versions(releases: &[&Release]) -> Vec<String> {
    releases.iter().map(|r| r.version.clone()).collect()
}

const HISTORY: &str = "\
## [Unreleased]

### Added

- Not yet.

## [nightly-2026-09-26] - 2026-09-26

### Fixed

- C fixed.

### Added

- C added.

## [nightly-2026-09-25] - 2026-09-25

### Added

- B added.
- B added again.

## [nightly-2026-09-24] - 2026-09-24

### Security

- A secured.
";

/// Decision 9's range: the upper version's section and the ones below
/// it, down to the lower one, exclusive — `Unreleased` never.
#[test]
fn between_two_versions_is_what_lies_above_the_older() {
    let releases = changelog::parse(HISTORY);

    assert_eq!(
        versions(&changelog::between(
            &releases,
            Some("nightly-2026-09-24"),
            "nightly-2026-09-26"
        )),
        ["nightly-2026-09-26", "nightly-2026-09-25"]
    );
    assert_eq!(
        versions(&changelog::between(
            &releases,
            Some("nightly-2026-09-25"),
            "nightly-2026-09-26"
        )),
        ["nightly-2026-09-26"]
    );
}

/// A running build the changelog does not know — a dev build, a debug
/// build, a version from nowhere — and a rollback, where the lower
/// version is above the upper: what the release itself says, never the
/// whole history.
#[test]
fn an_unknown_or_newer_lower_version_gives_the_upper_section_alone() {
    let releases = changelog::parse(HISTORY);
    let alone = ["nightly-2026-09-25"];

    assert_eq!(
        versions(&changelog::between(&releases, None, "nightly-2026-09-25")),
        alone
    );
    assert_eq!(
        versions(&changelog::between(
            &releases,
            Some("dogfood-2026-09-20"),
            "nightly-2026-09-25"
        )),
        alone
    );
    assert_eq!(
        versions(&changelog::between(
            &releases,
            Some("nightly-2026-09-26"),
            "nightly-2026-09-25"
        )),
        alone
    );
    assert_eq!(
        versions(&changelog::between(
            &releases,
            Some("nightly-2026-09-25"),
            "nightly-2026-09-25"
        )),
        alone,
        "the same version is the caller's to skip; the range still names it"
    );
}

#[test]
fn a_missing_upper_version_or_unreleased_is_nothing() {
    let releases = changelog::parse(HISTORY);

    assert!(changelog::between(&releases, None, "nightly-2030-01-01").is_empty());
    assert!(changelog::between(&releases, None, "Unreleased").is_empty());
    assert!(changelog::between(&[], None, "nightly-2026-09-25").is_empty());
}

/// The entries of several releases, one list per group in
/// keepachangelog's order, the newest release's first.
#[test]
fn releases_are_merged_by_group() {
    let releases = changelog::parse(HISTORY);
    let range = changelog::between(&releases, None, "nightly-2026-09-26");
    let mut all = range.clone();
    all.extend(changelog::between(
        &releases,
        Some("nightly-2026-09-26"),
        "nightly-2026-09-25",
    ));
    all.push(&releases[3]);

    let notes = changelog::merged(&all);

    assert_eq!(
        notes,
        [
            (
                Group::Added,
                vec![
                    "C added.".to_owned(),
                    "B added.".to_owned(),
                    "B added again.".to_owned()
                ]
            ),
            (Group::Fixed, vec!["C fixed.".to_owned()]),
            (Group::Security, vec!["A secured.".to_owned()]),
        ]
    );
}

/// The hover card's limit: the first entries in the order shown, a
/// group the cut empties gone, the rest counted.
#[test]
fn notes_are_cut_to_a_count_and_the_rest_counted() {
    let notes = vec![
        (Group::Added, vec!["a1".to_owned(), "a2".to_owned()]),
        (Group::Fixed, vec!["f1".to_owned()]),
        (Group::Security, vec!["s1".to_owned()]),
    ];

    assert_eq!(
        changelog::cut(&notes, 3),
        (
            vec![
                (Group::Added, vec!["a1".to_owned(), "a2".to_owned()]),
                (Group::Fixed, vec!["f1".to_owned()]),
            ],
            1
        )
    );
    assert_eq!(
        changelog::cut(&notes, 1),
        (vec![(Group::Added, vec!["a1".to_owned()])], 3)
    );
    assert_eq!(changelog::cut(&notes, 8), (notes.clone(), 0));
}

/// The person's decision 3: a release is named by its date, and a second
/// one the same day by the short hash of the commit it releases.
#[test]
fn a_version_is_named_by_its_date_then_its_commit() {
    assert_eq!(
        changelog::version_name("2026-09-24", "2bca70a", &["nightly-2026-09-23"], ""),
        Ok("nightly-2026-09-24".to_owned())
    );
    assert_eq!(
        changelog::version_name("2026-09-24", "2bca70a", &["nightly-2026-09-24"], ""),
        Ok("nightly-2026-09-24-2bca70a".to_owned())
    );
}

/// Only a tag made by hand can take both names, and a release does not
/// guess a third.
#[test]
fn a_name_taken_twice_is_refused() {
    let taken = changelog::version_name(
        "2026-09-24",
        "2bca70a",
        &["nightly-2026-09-24", "nightly-2026-09-24-2bca70a"],
        "",
    );

    assert_eq!(
        taken,
        Err(changelog::Taken("nightly-2026-09-24-2bca70a".to_owned()))
    );
}

/// A version the changelog already holds is taken as a tag is: the
/// public repository's fresh history has no tags for the releases its
/// changelog carries.
#[test]
fn a_version_in_the_changelog_is_taken_without_a_tag() {
    let held = "## [Unreleased]\n\n## [nightly-2026-09-24] - 2026-09-24\n";

    assert_eq!(
        changelog::version_name("2026-09-24", "2bca70a", &[], held),
        Ok("nightly-2026-09-24-2bca70a".to_owned())
    );
    assert_eq!(
        changelog::version_name(
            "2026-09-24",
            "2bca70a",
            &["nightly-2026-09-24-2bca70a"],
            held
        ),
        Err(changelog::Taken("nightly-2026-09-24-2bca70a".to_owned()))
    );
    assert_eq!(
        changelog::version_name("2026-09-25", "2bca70a", &[], held),
        Ok("nightly-2026-09-25".to_owned())
    );
}

const TO_RELEASE: &str = "\
# Changelog

Free text.

## [Unreleased]

### Added

- The new thing.

### Fixed

- The fixed thing.

## [nightly-2026-09-23] - 2026-09-23

### Added

- The old thing.
";

/// `Unreleased` becomes the version, a fresh empty one goes above it, and
/// not another byte of the file moves.
#[test]
fn a_release_renames_unreleased_and_opens_a_new_one() {
    let released = changelog::release(TO_RELEASE, "nightly-2026-09-24", "2026-09-24").unwrap();

    assert_eq!(
        released,
        TO_RELEASE.replace(
            "## [Unreleased]\n",
            "## [Unreleased]\n\n## [nightly-2026-09-24] - 2026-09-24\n"
        )
    );
}

/// What the release writes is what the window reads: an empty
/// `Unreleased`, and the version holding what `Unreleased` held.
#[test]
fn what_a_release_writes_parses_back() {
    let released = changelog::release(TO_RELEASE, "nightly-2026-09-24", "2026-09-24").unwrap();
    let before = changelog::parse(TO_RELEASE);
    let after = changelog::parse(&released);

    assert_eq!(after.len(), 3);
    assert!(after[0].is_unreleased());
    assert!(after[0].entries.is_empty());
    assert_eq!(after[1].version, "nightly-2026-09-24");
    assert_eq!(after[1].date.as_deref(), Some("2026-09-24"));
    assert_eq!(after[1].entries, before[0].entries);
    assert_eq!(after[2], before[1]);
}

#[test]
fn a_release_needs_something_to_release() {
    use changelog::Refusal;

    let none = "# Changelog\n\n## [nightly-2026-09-23] - 2026-09-23\n\n### Added\n\n- Old.\n";
    assert_eq!(changelog::check(none), Err(Refusal::NoUnreleased));
    assert_eq!(
        changelog::release(none, "nightly-2026-09-24", "2026-09-24"),
        Err(Refusal::NoUnreleased)
    );

    let empty = "## [Unreleased]\n\n### Added\n\n## [nightly-2026-09-23] - 2026-09-23\n\n### Added\n\n- Old.\n";
    assert_eq!(changelog::check(empty), Err(Refusal::EmptyUnreleased));
    assert_eq!(
        changelog::release(empty, "nightly-2026-09-24", "2026-09-24"),
        Err(Refusal::EmptyUnreleased)
    );

    assert_eq!(changelog::check(TO_RELEASE), Ok(()));
    assert_eq!(
        changelog::release(TO_RELEASE, "nightly-2026-09-23", "2026-09-23"),
        Err(Refusal::Exists("nightly-2026-09-23".to_owned()))
    );
}
