//! What the window knows about a terminal tab without reading its entity.
//!
//! Reading a view while the window draws ties the window to that view's
//! notifies — a terminal printing in a tab nobody sees redrew the whole
//! window at up to sixty frames a second. The facts the window needs are
//! published beside the view, and the window is told only when one of
//! them changes what it would draw.

use guild_desktop::terminal::facts::Facts;
use guild_session::HarnessKind;

#[test]
fn the_first_title_is_news_and_the_same_name_again_is_not() {
    let facts = Facts::new();

    assert!(facts.set_title("✳ Fix the login bug".to_owned()));
    // `claude` animates the marker in front of its title while it works;
    // the name a tab shows is the words after it, and they did not move.
    assert!(!facts.set_title("⠂ Fix the login bug".to_owned()));
    assert_eq!(facts.title().as_deref(), Some("⠂ Fix the login bug"));
}

#[test]
fn a_title_that_names_something_else_is_news() {
    let facts = Facts::new();
    facts.set_title("user@host:~/work/guild".to_owned());

    assert!(facts.set_title("user@host:~/work/meadow".to_owned()));
}

#[test]
fn the_agent_in_front_is_news_only_when_it_changes() {
    let facts = Facts::new();
    assert_eq!(facts.harness_in_front(), None);
    assert!(!facts.claude_in_front());

    assert!(facts.set_harness(Some(HarnessKind::Claude)));
    assert!(!facts.set_harness(Some(HarnessKind::Claude)));
    assert!(facts.claude_in_front());

    assert!(facts.set_harness(Some(HarnessKind::Codex)));
    assert!(!facts.claude_in_front());
    assert!(facts.set_harness(None));
}

#[test]
fn an_exit_is_news_once() {
    let facts = Facts::new();
    assert!(!facts.exited());

    assert!(facts.set_exited());
    assert!(!facts.set_exited());
    assert!(facts.exited());
}

#[test]
fn nobody_hears_until_someone_listens() {
    let facts = Facts::new();
    assert_eq!(facts.listener(), None);

    facts.listen(gpui::EntityId::from(7u64));

    assert_eq!(facts.listener(), Some(gpui::EntityId::from(7u64)));
}
