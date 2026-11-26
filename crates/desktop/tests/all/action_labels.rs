//! Find Action's names for every action, in both languages — M8.9 group
//! A's task 6, decision 15.
//!
//! A row of Find Action is a label, and the list is flat: nothing around a
//! row says which menu it came from. So every action needs a name that
//! reads alone, in each language, and no two may share one.

use std::collections::HashMap;

use guild_desktop::actions::keymap::Named;
use guild_desktop::actions::labels::label;
use guild_desktop::i18n::{Language, Ui};

/// **Every action has a label in both languages, and the two differ.** No
/// word of seams item 5's table is the same in English and in Russian, so
/// a label that reads the same in both has not been translated — which is
/// what the seams stub answered for every one: the keymap name, twice.
#[test]
fn every_action_has_a_label_in_both_languages() {
    for action in Named::ALL {
        let english = label(*action, Ui(Language::English));
        let russian = label(*action, Ui(Language::Russian));
        assert!(
            !english.trim().is_empty(),
            "{} has no English label",
            action.name()
        );
        assert!(
            !russian.trim().is_empty(),
            "{} has no Russian label",
            action.name()
        );
        assert_ne!(
            english,
            russian,
            "{} reads the same in both languages",
            action.name()
        );
    }
}

/// **A flat list of *Left* and *Left* is two rows nobody can tell apart.**
/// The twelve moves share three menu words, the six tool windows share
/// their tools' names with other items, and in a menu the submenu around
/// them says which is which; in Find Action nothing does.
#[test]
fn no_two_actions_share_a_label() {
    for language in [Language::English, Language::Russian] {
        let mut seen: HashMap<String, Named> = HashMap::new();
        for action in Named::ALL {
            let name = label(*action, Ui(language));
            if let Some(before) = seen.insert(name.clone(), *action) {
                panic!(
                    "{} and {} are both {name:?} in {language:?}",
                    before.name(),
                    action.name()
                );
            }
        }
    }
}
