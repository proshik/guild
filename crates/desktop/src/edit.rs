//! Pure editing vocabulary and readline deletion rules.
//!
//! Live single-line fields use `text_field::TextField`: InputState owns their
//! caret, selection and history. The adapter reuses these word deletion rules;
//! older pure models retain the edit vocabulary for their public API.
//!
//! **The rules are readline's**, because that is what the terminal in the
//! next tab is running and what every shell teaches. `option-backspace`
//! and `ctrl-w` take a word; `cmd-backspace` and `ctrl-u` take the line.
//! The terminal itself is not a caller here: it forwards the keystroke and
//! the program at the other end decides, which is why
//! `terminal::keys::bytes_for` sends `ESC DEL` and stops there.
//!
//! **A word ends where readline says it ends** — at a run of anything that
//! is not a letter or a digit. That is what makes `~/work/guild` come back
//! as `~/work/` and then `~/` rather than vanishing whole, which is what
//! deleting to whitespace would do to a path.
//!
//! There is no caret to move: both fields type at the end. When one of
//! them grows a caret, the movement rules belong here too.

use gpui::Keystroke;

/// What a keystroke asks of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Put these characters at the end.
    Insert(String),
    /// One character back.
    Rub,
    /// Back to the start of the word before the end.
    RubWord,
    /// The whole line.
    RubLine,
    /// Whatever is on the clipboard, at the end.
    ///
    /// **The text is not here**, because `edit` reads a keystroke and the
    /// clipboard needs an `App`. The caller turns this into an `Insert`
    /// with [`pasted`] and applies that.
    Paste,
}

/// What this keystroke does to a line, if it does anything.
///
/// `None` means the key was not typing — a shortcut, a function key, an
/// arrow — and the caller decides what it is instead.
pub fn edit(stroke: &Keystroke) -> Option<Edit> {
    let held = &stroke.modifiers;

    match stroke.key.as_str() {
        // `platform` is what GPUI calls the command key. Command wins over
        // alt when both are down: it is the larger of the two deletions,
        // and a person holding both meant the bigger one.
        "backspace" if held.platform || held.control => Some(Edit::RubLine),
        "backspace" if held.alt => Some(Edit::RubWord),
        "backspace" => Some(Edit::Rub),

        // readline's own two, for the hands that already know them.
        "w" if held.control => Some(Edit::RubWord),
        "u" if held.control => Some(Edit::RubLine),

        // **Paste, in the one spelling this platform uses.** Every field
        // of Guild's own is typed into character by character, and until
        // M8.7 none of them could take a name off the clipboard — which
        // is exactly what a person does with a symbol they mean to search
        // for. `ctrl-y` is readline's yank and is deliberately not bound:
        // it yanks readline's own kill ring, which these fields do not
        // have, and binding it to the system clipboard would be a word
        // that means something else in the terminal one tab away.
        "v" if held.platform => Some(Edit::Paste),

        // Anything with command, control or a function modifier is a
        // shortcut on its way somewhere else. Without this, `cmd-q` types
        // a `q` on its way out of the application.
        _ if held.platform || held.control || held.function => None,

        // `key_char` rather than `key`: it accounts for the layout and the
        // modifiers, while `key` is what is printed on the cap. With a
        // non-Latin layout the difference is fundamental.
        _ => stroke
            .key_char
            .clone()
            .filter(|text| !text.is_empty())
            .map(Edit::Insert),
    }
}

/// Resolve a [`Edit::Paste`] against the clipboard; everything else is
/// itself.
///
/// **A paste of nothing is not an edit.** An empty clipboard, or one
/// holding an image, answers `None` here rather than clearing the field
/// or inserting an empty string — and a paste of many lines is flattened
/// to one, because these fields are one line and a newline in them would
/// be a character nothing can draw.
pub fn pasted(edit: Edit, cx: &mut gpui::App) -> Option<Edit> {
    if edit != Edit::Paste {
        return Some(edit);
    }
    let text = cx.read_from_clipboard()?.text()?;
    let one_line: String = text.lines().collect::<Vec<_>>().join(" ").trim().to_owned();
    (!one_line.is_empty()).then_some(Edit::Insert(one_line))
}

/// Carry it out.
pub fn apply(line: &mut String, edit: Edit) {
    match edit {
        Edit::Insert(text) => line.push_str(&text),
        // Never reached: the caller resolves it with `pasted` first, and
        // an unresolved paste is a caller that forgot the clipboard.
        Edit::Paste => {}
        Edit::Rub => {
            line.pop();
        }
        Edit::RubWord => rub_word(line),
        Edit::RubLine => line.clear(),
    }
}

/// Carry it out over a line that is selected whole.
///
/// A field of Guild's own has no caret, so a selection is a state of the
/// field and there is only one: all of it. **What is typed replaces it,
/// and every rub empties it** — the rule of every text field on this
/// platform, which a person pressing a letter over a lit query expects.
/// M5.11, decision 5, for `cmd-shift-f`'s query; it lives here so the next
/// field that needs it finds it where the rules are.
pub fn apply_selected(line: &mut String, edit: Edit) {
    match edit {
        // Unresolved, as in `apply`: nothing to put in its place, so the
        // selection is not thrown away for it.
        Edit::Paste => {}
        Edit::Insert(_) => {
            line.clear();
            apply(line, edit);
        }
        Edit::Rub | Edit::RubWord | Edit::RubLine => line.clear(),
    }
}

/// Take back the word before the end of the line.
///
/// Two steps, and both are needed: whatever separates words goes first as
/// one step rather than one keypress per punctuation mark, and then the
/// word itself. Without the first step, deleting a word from `~/work/`
/// would spend a keypress on the slash and leave `~/work` — which looks
/// like the key doing nothing at all.
fn rub_word(line: &mut String) {
    let part_of_a_word = |c: char| c.is_alphanumeric();

    let end = line.trim_end_matches(|c| !part_of_a_word(c)).len();
    let start = line[..end]
        .char_indices()
        .rev()
        .find(|(_, c)| !part_of_a_word(*c))
        .map_or(0, |(index, c)| index + c.len_utf8());

    line.truncate(start);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: key_char.map(str::to_owned),
        }
    }

    fn after(line: &str, stroke: &Keystroke) -> String {
        let mut line = line.to_owned();
        if let Some(edit) = edit(stroke) {
            apply(&mut line, edit);
        }
        line
    }

    /// **`cmd-v` is an edit, not a shortcut on its way elsewhere.**
    /// Everything else with the command key is refused by the arm under
    /// it, which is what stops `cmd-q` typing a `q` on its way out.
    #[test]
    fn command_v_is_a_paste_and_the_rest_of_command_is_not() {
        let command = Modifiers {
            platform: true,
            ..Modifiers::default()
        };
        assert_eq!(edit(&key("v", Some("v"), command)), Some(Edit::Paste));
        assert_eq!(edit(&key("q", Some("q"), command)), None);
        assert_eq!(edit(&key("s", Some("s"), command)), None);
    }

    /// A path comes back one component at a time. Deleting to whitespace
    /// would take the whole path in one keypress, which is the opposite of
    /// what the key is for.
    #[test]
    fn a_word_is_one_component_of_a_path() {
        let mut line = String::from("~/work/guild");

        rub_word(&mut line);
        assert_eq!(line, "~/work/");
        rub_word(&mut line);
        assert_eq!(line, "~/");
        rub_word(&mut line);
        assert_eq!(line, "");
    }

    /// A dash separates words, as it does in readline. `guild-spike` is
    /// two things a person might have meant.
    #[test]
    fn punctuation_separates_words() {
        let mut line = String::from("guild-spike");

        rub_word(&mut line);
        assert_eq!(line, "guild-");
        rub_word(&mut line);
        assert_eq!(line, "");
    }

    /// The separators go as one step. One keypress per slash is a key that
    /// looks like it did nothing.
    #[test]
    fn a_run_of_separators_goes_in_one_step() {
        let mut line = String::from("work///");

        rub_word(&mut line);
        assert_eq!(line, "");
    }

    /// A word is not an English word. Guild's own interface is in two
    /// languages and its projects are named in more.
    #[test]
    fn a_word_can_be_written_in_any_alphabet() {
        let mut line = String::from("проект тест");

        rub_word(&mut line);
        assert_eq!(line, "проект ");
        rub_word(&mut line);
        assert_eq!(line, "");
    }

    /// An empty line has nothing to take, and must not panic reaching for
    /// it.
    #[test]
    fn an_empty_line_survives_every_deletion() {
        let mut line = String::new();

        rub_word(&mut line);
        apply(&mut line, Edit::Rub);
        apply(&mut line, Edit::RubLine);

        assert_eq!(line, "");
    }

    /// The four strokes a person brings from the shell in the next tab.
    #[test]
    fn the_shells_own_strokes_do_what_they_do_there() {
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let cmd = Modifiers {
            platform: true,
            ..Modifiers::default()
        };
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };

        let line = "~/work/guild";
        assert_eq!(after(line, &key("backspace", None, alt)), "~/work/");
        assert_eq!(after(line, &key("backspace", None, ctrl)), "");
        assert_eq!(after(line, &key("backspace", None, cmd)), "");
        assert_eq!(after(line, &key("w", Some("w"), ctrl)), "~/work/");
        assert_eq!(after(line, &key("u", Some("u"), ctrl)), "");
        assert_eq!(
            after(line, &key("backspace", None, Modifiers::default())),
            "~/work/guil"
        );
    }

    /// **The trap.** A shortcut on its way somewhere else must not leave a
    /// letter behind in the field it passed through.
    #[test]
    fn a_shortcut_does_not_type_itself() {
        let cmd = Modifiers {
            platform: true,
            ..Modifiers::default()
        };

        assert_eq!(edit(&key("q", Some("q"), cmd)), None);
        assert_eq!(edit(&key("o", Some("o"), cmd)), None);
    }

    /// **A line selected whole is replaced by what is typed, and taken by
    /// any rub.** M5.11, decision 5: `cmd-shift-f` lights the old query,
    /// and the first letter starts a new one instead of being added to
    /// the end of it.
    #[test]
    fn an_edit_over_a_selected_line_replaces_it() {
        let over = |edit: Edit| {
            let mut line = String::from("old");
            apply_selected(&mut line, edit);
            line
        };

        assert_eq!(over(Edit::Insert("zzz".to_owned())), "zzz");
        assert_eq!(over(Edit::Rub), "");
        assert_eq!(over(Edit::RubWord), "");
        assert_eq!(over(Edit::RubLine), "");
    }

    /// Typing is still typing.
    #[test]
    fn an_ordinary_key_is_inserted() {
        assert_eq!(
            after("gu", &key("i", Some("i"), Modifiers::default())),
            "gui"
        );
    }
}
