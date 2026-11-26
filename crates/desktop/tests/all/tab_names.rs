//! What a tab is called.
//!
//! Out here rather than beside the code because every one of these goes
//! in through the public door — which makes them a check on that door as
//! well as on the behaviour behind it.

use guild_desktop::title::*;

/// What `claude` actually sets, captured from a live run. The topic is
/// the whole point: it is what tells one conversation from another.
#[test]
fn a_claude_topic_becomes_the_tab_name() {
    assert_eq!(
        of("✳ Rust uninstall workflow").as_deref(),
        Some("Rust uninstall workflow")
    );
    assert_eq!(of("✳ Claude Code").as_deref(), Some("Claude Code"));
}

/// What `zsh` actually sets. The `user@host` half is the same on every
/// tab; the directory is what differs.
#[test]
fn a_shell_title_is_cut_down_to_the_directory() {
    assert_eq!(
        of("person@host:~/work/project/guild").as_deref(),
        Some("guild")
    );
}

/// A bare path is treated the same way — some shells write only that.
#[test]
fn a_bare_path_is_cut_down_too() {
    assert_eq!(of("/Users/someone/work/guild").as_deref(), Some("guild"));
    assert_eq!(of("~/work/guild/").as_deref(), Some("guild"));
}

/// A running command is not a path and stays whole: `vim src/main.rs`
/// is exactly what a person wants to see on the tab.
#[test]
fn a_command_is_left_alone() {
    assert_eq!(of("vim src/main.rs").as_deref(), Some("vim src/main.rs"));
    assert_eq!(of("cargo test").as_deref(), Some("cargo test"));
}

/// A tab strip is only so wide.
#[test]
fn a_long_title_is_cut_and_says_so() {
    let long = of("Investigate why the trackpad does not scroll in the alternate screen")
        .expect("a title");

    assert!(long.chars().count() <= LIMIT, "too long: {long}");
    assert!(long.ends_with('…'), "the cut must be visible: {long}");
}

/// Cutting by bytes would split a multi-byte character in half and
/// panic. Topics are written in any alphabet.
#[test]
fn a_long_title_in_another_alphabet_does_not_panic() {
    let long =
        of("Разобраться, почему тачпад не прокручивает альтернативный экран").expect("a title");

    assert!(long.chars().count() <= LIMIT, "too long: {long}");
}

/// Before a program says anything there is nothing to show, and the
/// caller falls back to what it knows.
#[test]
fn nothing_worth_showing_is_nothing() {
    assert_eq!(of(""), None);
    assert_eq!(of("   "), None);
    assert_eq!(of("✳"), None, "a marker on its own is not a name");
}

/// **A name a person gave wins over everything the tab would call itself**
/// — M12.1 task 1. A tab named *login bug* must not turn into the topic
/// `claude` finds for the conversation a minute later; a blank name is no
/// name; and a long one is cut like any other, because the strip is the
/// same strip.
#[test]
fn a_name_a_person_gave_wins_over_the_topic_and_the_program() {
    assert_eq!(
        chosen(
            Some("login bug"),
            Some("Fix authentication flow".to_owned())
        )
        .as_deref(),
        Some("login bug")
    );
    assert_eq!(
        chosen(Some("  "), Some("Fix authentication flow".to_owned())).as_deref(),
        Some("Fix authentication flow")
    );
    assert_eq!(chosen(None, None), None);
    let long = "a name much longer than any tab in the strip has room for";
    assert_eq!(chosen(Some(long), None).unwrap().chars().count(), LIMIT);
}
