//! Keys into bytes.
//!
//! A terminal does not receive a "keyboard event" — it receives bytes, and
//! whether the program understands the key press depends on which ones.
//! Arrows in application mode are encoded differently from normal; `Ctrl`
//! with a letter gives a control code; `Esc` and `Alt` differ by a single
//! prefix.
//!
//! All of this is covered by tests, because it is easy to get wrong and
//! hard to notice: a wrong code breaks nothing, the key simply "does not
//! work".

use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

/// What a key press turns into.
///
/// `None` means the press is not addressed to the terminal: a modifier on
/// its own, an unknown function key.
pub fn bytes_for(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let modifiers = &keystroke.modifiers;
    let key = keystroke.key.as_str();

    // Arrows and Home/End in application mode go through SS3 rather than
    // CSI. Programs like `claude` turn that mode on, and there a plain CSI
    // is not recognised: the cursor simply does not move.
    // **A held modifier changes the sequence, not just the key.** xterm
    // encodes it as `CSI 1 ; <code> <letter>`, where the code is 1 plus a
    // bit each for shift, alt and control — and with a modifier the form
    // is always CSI, even in application mode, which is what programs
    // parse. Without this `option-left` reached the shell as a plain left
    // arrow: the caret moved one character and *by word* looked
    // unimplemented. Seen in the window, M12 task 17.
    let cursor = |letter: char| {
        let code = 1
            + u8::from(modifiers.shift)
            + 2 * u8::from(modifiers.alt)
            + 4 * u8::from(modifiers.control);
        if code > 1 {
            return Some(format!("\x1b[1;{code}{letter}").into_bytes());
        }
        let prefix = if mode.contains(TermMode::APP_CURSOR) {
            "\x1bO"
        } else {
            "\x1b["
        };
        Some(format!("{prefix}{letter}").into_bytes())
    };

    match key {
        "up" => return cursor('A'),
        "down" => return cursor('B'),
        // `cmd` on an arrow or on backspace is a line edit rather than a
        // cursor key — M5.12, decision 6: `ctrl-a`, `ctrl-e` and `ctrl-u`,
        // what common macOS terminals send for natural text editing and
        // what a shell and `claude`'s prompt both read. Asked before the cursor
        // sequence is built, because the platform check further down comes
        // too late for keys matched here: until M5.12 `cmd-left` went out
        // as a bare left arrow and `cmd-backspace` as one DEL.
        "right" if modifiers.platform => return Some(vec![0x05]),
        "left" if modifiers.platform => return Some(vec![0x01]),
        "right" => return cursor('C'),
        "left" => return cursor('D'),
        "home" => return cursor('H'),
        "end" => return cursor('F'),
        "pageup" => return Some(b"\x1b[5~".to_vec()),
        "pagedown" => return Some(b"\x1b[6~".to_vec()),
        "insert" => return Some(b"\x1b[2~".to_vec()),
        "delete" => return Some(b"\x1b[3~".to_vec()),
        "escape" => return Some(b"\x1b".to_vec()),
        "tab" => {
            return Some(if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            });
        }
        // A carriage return rather than a line feed: in line mode a line
        // feed does not end the input, and Enter "does nothing".
        // **Shift and Option make it a new line instead** — M5.12, decision
        // 5, measured by its task 0 against `claude` 2.1.272: `\n` is the
        // key `claude` names `enter` and binds to a new line, `ESC CR` is a
        // new line there and in `zsh`'s editor, and `\r` submits. The kitty
        // keyboard protocol stays off, so these are the only spellings.
        "enter" => {
            return Some(if modifiers.alt {
                b"\x1b\r".to_vec()
            } else if modifiers.shift {
                b"\n".to_vec()
            } else {
                b"\r".to_vec()
            });
        }
        "backspace" => {
            return Some(if modifiers.platform {
                vec![0x15]
            } else if modifiers.alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            });
        }
        _ => {}
    }

    if let Some(function) = key.strip_prefix('f')
        && let Ok(number) = function.parse::<u8>()
    {
        return function_key(number);
    }

    // Ctrl with a letter is a control code: Ctrl+C is 0x03, Ctrl+D is
    // 0x04. Without it there is nothing to interrupt work in the terminal
    // with.
    if modifiers.control
        && !modifiers.platform
        && let Some(code) = control_code(key)
    {
        return Some(vec![code]);
    }

    // Cmd is left to the system: those are its shortcuts, not the
    // terminal's.
    if modifiers.platform {
        return None;
    }

    // Ordinary input. `key_char` accounts for the layout and Shift, while
    // `key` is what is printed on the key cap. With a non-Latin layout the
    // difference is fundamental.
    let text = keystroke.key_char.as_deref().filter(|t| !t.is_empty())?;

    Some(if modifiers.alt {
        // Alt in a terminal is an Esc prefix, not a separate modifier —
        // **and the key, not what option composes** (M5.14): GPUI's
        // `key_char` for option-b is `∫`, and `ESC ∫` moved no word in zsh
        // or `claude`. `key` is the Latin cap, which a Russian layout
        // reports too; shift makes it a capital, as meta-B is.
        let mut bytes = vec![0x1b];
        match meta_key(&keystroke.key, modifiers.shift) {
            Some(key) => bytes.extend_from_slice(key.as_bytes()),
            None => bytes.extend_from_slice(text.as_bytes()),
        }
        bytes
    } else {
        text.as_bytes().to_vec()
    })
}

/// What meta sends after its escape: the key's one character, a capital
/// with shift — `None` for a named key, which keeps what GPUI typed.
fn meta_key(key: &str, shift: bool) -> Option<String> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(if shift {
        first.to_uppercase().collect()
    } else {
        first.to_string()
    })
}

/// The bytes for an up or down cursor key.
///
/// Shared with scrolling: in a full-screen program the wheel is translated
/// into these, and encoding them a second time by hand there is how the
/// application-cursor case gets forgotten in one of the two places.
pub fn cursor_key(up: bool, mode: TermMode) -> &'static [u8] {
    match (up, mode.contains(TermMode::APP_CURSOR)) {
        (true, false) => b"\x1b[A",
        (true, true) => b"\x1bOA",
        (false, false) => b"\x1b[B",
        (false, true) => b"\x1bOB",
    }
}

/// The bytes a paste of `text` is.
///
/// **Wrapped in markers when the program asked for them**, so it knows the
/// text arrived whole rather than typed: without that, a multi-line paste
/// into a shell runs line by line. **And with no `ESC` of its own** — a text
/// holding the end marker would close the paste early, and whatever came
/// after would reach the program as keystrokes. A person's clipboard and a
/// command's message are the same risk, so both come through here.
pub fn pasted(text: &str, mode: TermMode) -> Vec<u8> {
    if !mode.contains(TermMode::BRACKETED_PASTE) {
        return text.as_bytes().to_vec();
    }
    let mut bytes = Vec::with_capacity(text.len() + 12);
    bytes.extend_from_slice(b"\x1b[200~");
    bytes.extend(text.bytes().filter(|&byte| byte != 0x1b));
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}

/// Compute the complete encoded size before allocating a paste buffer. In
/// bracketed mode ESC bytes are filtered exactly as in pasted().
pub(crate) fn pasted_bounded(text: &str, mode: TermMode, budget: usize) -> Option<Vec<u8>> {
    bounded_paste(text, mode, budget, false)
}
pub(crate) fn submitted_bounded(text: &str, budget: usize) -> Option<Vec<u8>> {
    bounded_paste(text, TermMode::BRACKETED_PASTE, budget, true)
}
fn bounded_paste(text: &str, mode: TermMode, budget: usize, enter: bool) -> Option<Vec<u8>> {
    let bracketed = mode.contains(TermMode::BRACKETED_PASTE);
    let overhead = if bracketed { 12 } else { 0 } + usize::from(enter);
    let available = budget.checked_sub(overhead)?;
    let count = if bracketed {
        text.bytes()
            .filter(|b| *b != 0x1b)
            .take(available.saturating_add(1))
            .count()
    } else {
        text.len()
    };
    if count > available {
        return None;
    }
    let mut bytes = Vec::with_capacity(count + overhead);
    if bracketed {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend(text.bytes().filter(|b| !bracketed || *b != 0x1b));
    if bracketed {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    if enter {
        bytes.push(b'\r');
    }
    Some(bytes)
}

/// The text files dropped on a terminal are typed as — the form common
/// macOS terminals share.
///
/// **A backslash, not quotes.** The shell reads either, but `claude`
/// splits a drop into paths at a space followed by `/` and then strips
/// backslashes — measured in 2.1.281 — so `'/a b.png' '/c.png'` is one
/// path to it and `/a\ b.png /c.png` is two. That is what makes a
/// screenshot dragged from the corner of the screen arrive as an image.
/// Only ASCII is escaped: a letter outside it splits nothing, and macOS
/// puts a narrow no-break space into a screenshot's name. The trailing
/// space is the common form too, so the next word typed is a word.
pub fn dropped(paths: &[std::path::PathBuf]) -> String {
    let mut text = String::new();
    for path in paths {
        for c in path.to_string_lossy().chars() {
            if c.is_ascii() && !c.is_ascii_alphanumeric() && !"+-./:@_,%".contains(c) {
                text.push('\\');
            }
            text.push(c);
        }
        text.push(' ');
    }
    text
}

/// A message typed into a program's prompt and sent: the paste and one
/// Return, in one write — M13 task 9.
///
/// Measured against `claude` 2.1.269 rather than assumed: no pause is needed
/// between the paste and the Return, a newline inside the paste stays in the
/// message, and eighty lines arrive whole. Only for a program in bracketed
/// mode, where a newline cannot send the message early.
pub fn submitted(text: &str) -> Vec<u8> {
    let mut bytes = pasted(text, TermMode::BRACKETED_PASTE);
    bytes.push(b'\r');
    bytes
}

fn function_key(number: u8) -> Option<Vec<u8>> {
    let sequence = match number {
        1 => "\x1bOP",
        2 => "\x1bOQ",
        3 => "\x1bOR",
        4 => "\x1bOS",
        5 => "\x1b[15~",
        6 => "\x1b[17~",
        7 => "\x1b[18~",
        8 => "\x1b[19~",
        9 => "\x1b[20~",
        10 => "\x1b[21~",
        11 => "\x1b[23~",
        12 => "\x1b[24~",
        _ => return None,
    };
    Some(sequence.as_bytes().to_vec())
}

fn control_code(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    match first {
        // Ctrl+A..Ctrl+Z give 0x01..0x1a.
        'a'..='z' => Some(first as u8 - b'a' + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        ' ' => Some(0x00),
        _ => None,
    }
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

    fn plain(name: &str, ch: &str) -> Keystroke {
        key(name, Some(ch), Modifiers::default())
    }

    /// There is nothing to interrupt work in the terminal with if Ctrl+C
    /// does not give 0x03.
    #[test]
    fn ctrl_c_is_the_interrupt_byte() {
        let stroke = key("c", Some("c"), Modifiers::control());

        assert_eq!(
            bytes_for(&stroke, TermMode::default()),
            Some(vec![0x03]),
            "Ctrl+C must give a control code, not a letter"
        );
    }

    #[test]
    fn ctrl_letters_cover_the_whole_alphabet() {
        for (letter, code) in [('a', 0x01u8), ('d', 0x04), ('z', 0x1a)] {
            let stroke = key(
                &letter.to_string(),
                Some(&letter.to_string()),
                Modifiers::control(),
            );
            assert_eq!(bytes_for(&stroke, TermMode::default()), Some(vec![code]));
        }
    }

    /// Enter is a carriage return. A line feed does not end the input,
    /// and it looks like "Enter does not work".
    #[test]
    fn enter_sends_carriage_return_not_line_feed() {
        let stroke = key("enter", None, Modifiers::default());

        assert_eq!(
            bytes_for(&stroke, TermMode::default()),
            Some(b"\r".to_vec())
        );
    }

    /// **Shift-Enter is a new line, Option-Enter the other one** — M5.12
    /// task 4, decision 5. Measured by task 0 against `claude` 2.1.272 in a
    /// pty: `\n` is the key it names `enter` and binds to a new line in its
    /// prompt, `\r` submits, and `ESC CR` is a new line too — in `zsh`'s
    /// line editor as well, where `\n` runs the line as Return does. The
    /// cursor mode changes none of it.
    #[test]
    fn shift_enter_is_a_line_feed_and_option_enter_escape_return() {
        for mode in [TermMode::default(), TermMode::APP_CURSOR] {
            assert_eq!(
                bytes_for(&key("enter", None, Modifiers::shift()), mode).as_deref(),
                Some(b"\n".as_slice()),
                "shift-enter in {mode:?}"
            );
            assert_eq!(
                bytes_for(&key("enter", None, Modifiers::alt()), mode).as_deref(),
                Some(b"\x1b\r".as_slice()),
                "option-enter in {mode:?}"
            );
            assert_eq!(
                bytes_for(&key("enter", None, Modifiers::default()), mode).as_deref(),
                Some(b"\r".as_slice()),
                "a plain enter stays a carriage return in {mode:?}"
            );
        }
    }

    /// **`cmd`-arrows and `cmd-backspace` edit the line** — M5.12 task 4,
    /// decision 6: `ctrl-a`, `ctrl-e`, `ctrl-u`, what common macOS terminals
    /// send for natural text editing, and what both a shell and `claude`'s
    /// prompt read (task 0's table). Before this the arrows were matched
    /// before the platform was looked at, so `cmd-left` went out as a bare
    /// left arrow and `cmd-backspace` as one DEL.
    #[test]
    fn cmd_arrows_and_cmd_backspace_are_the_line_edits() {
        for mode in [TermMode::default(), TermMode::APP_CURSOR] {
            assert_eq!(
                bytes_for(&key("left", None, Modifiers::command()), mode),
                Some(vec![0x01]),
                "cmd-left is the start of the line in {mode:?}"
            );
            assert_eq!(
                bytes_for(&key("right", None, Modifiers::command()), mode),
                Some(vec![0x05]),
                "cmd-right is the end of the line in {mode:?}"
            );
            assert_eq!(
                bytes_for(&key("backspace", None, Modifiers::command()), mode),
                Some(vec![0x15]),
                "cmd-backspace takes the line in {mode:?}"
            );
        }
    }

    /// Arrows in application mode go through SS3. `claude` turns that
    /// mode on, and there a plain CSI is not recognised.
    #[test]
    fn arrows_follow_the_application_cursor_mode() {
        let up = key("up", None, Modifiers::default());

        assert_eq!(
            bytes_for(&up, TermMode::default()),
            Some(b"\x1b[A".to_vec()),
            "normal mode is CSI"
        );
        assert_eq!(
            bytes_for(&up, TermMode::APP_CURSOR),
            Some(b"\x1bOA".to_vec()),
            "application mode is SS3"
        );
    }

    /// The layout comes from `key_char`: `key` is what is printed on the
    /// key cap, and on a non-Latin layout that is a Latin letter.
    ///
    /// The Cyrillic letter here is test data, not prose: a different
    /// alphabet is exactly what this test is about.
    #[test]
    fn the_typed_character_comes_from_the_layout_not_the_keycap() {
        let stroke = key("d", Some("в"), Modifiers::default());

        assert_eq!(
            bytes_for(&stroke, TermMode::default()),
            Some("в".as_bytes().to_vec()),
            "the letter of the layout must reach the terminal"
        );
    }

    /// **A held modifier travels with the arrow** — M12 task 17. Seen in
    /// the window: `option-left` moved the caret one character, because
    /// the arrows were encoded before the modifiers were looked at and
    /// the shell received a plain left arrow. xterm's form is
    /// `CSI 1 ; <code> <letter>`, the code being 1 plus a bit each for
    /// shift, alt and control — and it stays CSI in application mode,
    /// where a bare arrow would have gone out as SS3.
    #[test]
    fn a_modifier_travels_with_the_arrow() {
        let word_left = bytes_for(&key("left", None, Modifiers::alt()), TermMode::empty());
        assert_eq!(word_left.as_deref(), Some(b"\x1b[1;3D".as_slice()));

        // The same in application mode, which `claude` turns on: a
        // modified arrow is CSI there too.
        let in_app = bytes_for(&key("right", None, Modifiers::alt()), TermMode::APP_CURSOR);
        assert_eq!(in_app.as_deref(), Some(b"\x1b[1;3C".as_slice()));

        // Control and shift have their own bits, and they add up.
        let ctrl_left = bytes_for(&key("left", None, Modifiers::control()), TermMode::empty());
        assert_eq!(ctrl_left.as_deref(), Some(b"\x1b[1;5D".as_slice()));
        let shift_up = bytes_for(&key("up", None, Modifiers::shift()), TermMode::empty());
        assert_eq!(shift_up.as_deref(), Some(b"\x1b[1;2A".as_slice()));

        // And a bare arrow is what it always was, in both modes.
        let plain = bytes_for(&key("left", None, Modifiers::none()), TermMode::empty());
        assert_eq!(plain.as_deref(), Some(b"\x1b[D".as_slice()));
        let plain_app = bytes_for(&key("left", None, Modifiers::none()), TermMode::APP_CURSOR);
        assert_eq!(plain_app.as_deref(), Some(b"\x1bOD".as_slice()));
    }

    /// **Option is meta, from the key and not from what option composes**
    /// — M5.14. GPUI hands `∫` for option-b, and `ESC ∫` moved no word in
    /// zsh or `claude`; the old fixture said `key_char "b"`, which a Mac
    /// never produces. On a Russian layout the key is the Latin one too.
    #[test]
    fn option_sends_escape_and_the_key() {
        let stroke = key("b", Some("∫"), Modifiers::alt());
        assert_eq!(
            bytes_for(&stroke, TermMode::default()),
            Some(vec![0x1b, b'b'])
        );
        let shifted = key(
            "b",
            Some("ı"),
            Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::none()
            },
        );
        assert_eq!(
            bytes_for(&shifted, TermMode::default()),
            Some(vec![0x1b, b'B'])
        );
    }

    /// Cmd stays with the system: those are its shortcuts.
    #[test]
    fn platform_shortcuts_are_not_forwarded() {
        let stroke = key("c", Some("c"), Modifiers::command());

        assert_eq!(bytes_for(&stroke, TermMode::default()), None);
    }

    #[test]
    fn backspace_sends_delete_and_alt_backspace_deletes_a_word() {
        assert_eq!(
            bytes_for(
                &key("backspace", None, Modifiers::default()),
                TermMode::default()
            ),
            Some(vec![0x7f])
        );
        assert_eq!(
            bytes_for(
                &key("backspace", None, Modifiers::alt()),
                TermMode::default()
            ),
            Some(vec![0x1b, 0x7f])
        );
    }

    #[test]
    fn shift_tab_moves_backwards() {
        assert_eq!(
            bytes_for(&key("tab", None, Modifiers::shift()), TermMode::default()),
            Some(b"\x1b[Z".to_vec())
        );
    }

    #[test]
    fn function_keys_are_encoded() {
        assert_eq!(
            bytes_for(&key("f1", None, Modifiers::default()), TermMode::default()),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            bytes_for(&key("f12", None, Modifiers::default()), TermMode::default()),
            Some(b"\x1b[24~".to_vec())
        );
    }

    /// A press with no character and no meaning for the terminal is not
    /// sent at all: otherwise rubbish ends up in the stream.
    #[test]
    fn a_bare_modifier_sends_nothing() {
        let stroke = key("shift", None, Modifiers::shift());

        assert_eq!(bytes_for(&stroke, TermMode::default()), None);
    }

    /// Scrolling in a full-screen program sends these, and a key press
    /// sends them too. If the two ever disagree, one of them is wrong —
    /// and the one that is wrong will be the one nobody presses.
    #[test]
    fn the_scroll_keys_are_the_same_as_the_pressed_ones() {
        for mode in [TermMode::default(), TermMode::APP_CURSOR] {
            let pressed_up = bytes_for(&key("up", None, Modifiers::default()), mode);
            let pressed_down = bytes_for(&key("down", None, Modifiers::default()), mode);

            assert_eq!(
                pressed_up.as_deref(),
                Some(cursor_key(true, mode)),
                "scrolling up must send what the up key sends"
            );
            assert_eq!(
                pressed_down.as_deref(),
                Some(cursor_key(false, mode)),
                "scrolling down must send what the down key sends"
            );
        }
    }

    /// **A paste is wrapped when the program asked for it, and carries no
    /// escape of its own** — M13 task 9, and a person's `cmd-v`. A text
    /// holding the end marker would close the paste early, and whatever
    /// followed would reach the program as keystrokes: `ESC` is dropped
    /// rather than trusted, the rest of the text kept as it was.
    #[test]
    fn a_paste_is_wrapped_when_asked_and_cannot_end_itself() {
        let bracketed = TermMode::BRACKETED_PASTE;
        assert_eq!(
            pasted("two\nlines", bracketed),
            b"\x1b[200~two\nlines\x1b[201~".to_vec()
        );
        assert_eq!(pasted("plain", TermMode::default()), b"plain".to_vec());
        assert_eq!(
            pasted("a\x1b[201~\x03b", bracketed),
            b"\x1b[200~a[201~\x03b\x1b[201~".to_vec(),
            "the end marker inside the text is defused"
        );
    }

    /// **A dropped file is typed the way macOS terminals type it**: a
    /// backslash before whatever a shell would read as more than a
    /// character, one space between the paths, one after the last. The
    /// form is `claude`'s as much as the shell's — 2.1.281 splits a drop
    /// at a space followed by `/` and strips the backslashes, so quotes
    /// would glue two screenshots into one path that does not exist.
    #[test]
    fn a_dropped_path_is_escaped_as_terminal_app_escapes_it() {
        use std::path::PathBuf;
        let shot = PathBuf::from(
            "/var/folders/x/T/TemporaryItems/NSIRD_screencaptureui_a/Screenshot 2026-09-23 at 10.15.32.png",
        );
        assert_eq!(
            dropped(&[shot]),
            "/var/folders/x/T/TemporaryItems/NSIRD_screencaptureui_a/Screenshot\\ 2026-09-23\\ at\\ 10.15.32.png ",
        );
        assert_eq!(
            dropped(&[PathBuf::from("/a/b.png"), PathBuf::from("/c/d e.png")]),
            "/a/b.png /c/d\\ e.png ",
            "the paths are words of one line, as the shell and claude both split them"
        );
        assert_eq!(
            dropped(&[PathBuf::from("/x/it's (1) $HOME&*.txt")]),
            "/x/it\\'s\\ \\(1\\)\\ \\$HOME\\&\\*.txt ",
        );
        assert_eq!(
            dropped(&[PathBuf::from("/x/отчёт\u{202f}PM.png")]),
            "/x/отчёт\u{202f}PM.png ",
            "a letter outside ASCII is a letter: no shell splits on it"
        );
    }

    /// **A message is the paste and a Return, in one write** — measured
    /// against `claude` 2.1.269: no pause is needed between them, and a
    /// newline inside the paste stays a newline.
    #[test]
    fn a_message_is_a_paste_and_one_return() {
        assert_eq!(submitted("say ok"), b"\x1b[200~say ok\x1b[201~\r".to_vec());
    }

    #[test]
    fn plain_letters_pass_through() {
        assert_eq!(
            bytes_for(&plain("g", "g"), TermMode::default()),
            Some(b"g".to_vec())
        );
    }
}

#[cfg(test)]
mod bounded_input_tests {
    use super::*;
    #[test]
    fn wrappers_unicode_and_enter_are_counted_before_whole_operation_allocation() {
        let text = "🦀\x1bЖ";
        let expected = submitted(text);
        assert_eq!(
            submitted_bounded(text, expected.len()),
            Some(expected.clone())
        );
        assert_eq!(submitted_bounded(text, expected.len() - 1), None);
        assert_eq!(
            pasted_bounded("Ж", TermMode::empty(), 2),
            Some("Ж".as_bytes().to_vec())
        );
        assert_eq!(pasted_bounded("Ж", TermMode::empty(), 1), None);
        assert!(submitted_bounded(&"x".repeat(1024 * 1024), 64).is_none());
        let filtered =
            pasted_bounded(&"\x1b".repeat(1024 * 1024), TermMode::BRACKETED_PASTE, 12).unwrap();
        assert_eq!(filtered, b"\x1b[200~\x1b[201~");
        assert_eq!(filtered.capacity(), 12);
    }
}
