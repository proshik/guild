//! Find in Files' history: the chevron over a field — M12.7 task 7, a
//! menu of the engine in place of the hand-rolled card `view::find_in_files`
//! used to roll (the `popup::row` + `.hover()` pair that used to draw it).
//!
//! **A heading over the remembered texts, newest first.** The counter's
//! own pattern: the field's chevron gathers what the project remembers
//! into a [`super::Target::FindHistory`] at the click, and a row's deed
//! names its text by place in that list rather than by the text itself —
//! `Deed` stays `Copy`. No check, no mark, no ✕: a remembered text is not
//! a current value the way a filter's is, and it is not a row a person
//! removes from here.

use super::{Choice, Deed, Entry, Label, Mark};
use crate::find_in_files::Field;
use crate::i18n::Text;

/// Which heading a field's chevron wears.
pub(super) fn title(field: Field) -> Text {
    match field {
        Field::Text => Text::FindInFilesHistory,
        Field::Mask => Text::FindInFilesMaskHistory,
    }
}

/// The menu under a field's chevron: `title` over `items`, newest first —
/// the order they are already kept in.
pub fn history(field: Field, title: Text, items: &[String]) -> Vec<Entry> {
    let mut entries = vec![Entry::Heading(Label::Text(title))];
    entries.extend(items.iter().enumerate().map(|(index, text)| {
        Entry::Choice(Choice {
            label: Label::Named(text.clone()),
            mark: Mark::None,
            deed: Deed::UseHistory(field, index),
            check: None,
            enabled: true,
            detail: None,
            trailing: None,
        })
    }));
    entries
}

#[cfg(test)]
mod tests {
    use super::title;
    use crate::find_in_files::Field;
    use crate::i18n::Text;

    /// `title` is `pub(super)`, not reachable from `tests/all` — this is
    /// the direct test task 7 left for it. M12.7 final fix.
    #[test]
    fn each_field_wears_its_own_heading() {
        assert_eq!(title(Field::Text), Text::FindInFilesHistory);
        assert_eq!(title(Field::Mask), Text::FindInFilesMaskHistory);
    }
}
