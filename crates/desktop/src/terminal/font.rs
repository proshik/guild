//! Which font to draw the terminal, and code, with.
//!
//! Modern shell prompts — `powerlevel10k`, `starship` and the rest — are
//! drawn with Nerd Font glyphs: separator arrows, the branch glyph,
//! language icons. An ordinary monospace font does not have those
//! characters, and they come out as boxes with a question mark. That is
//! exactly what was visible in the window.
//!
//! Until M12.11 the terminal's face was therefore found among what is
//! installed, a Nerd Font by name first. It is Lilex Nerd Font now, which
//! ships with the window and so is always there.

/// The font to draw the terminal with.
///
/// `chosen` is what a person asked for, and it wins whenever the machine
/// has it or the window ships it. A name that is neither falls through to
/// the face that ships rather than to nothing: a settings file carried to
/// another machine names a font that machine may not have, and a blank
/// terminal is a worse answer than a different font.
pub fn choose(available: &[String], chosen: Option<&str>) -> String {
    found(available, chosen).unwrap_or_else(|| crate::fonts::DEFAULT.terminal.to_owned())
}

/// What a person chose, when there is a face by that name.
///
/// **A bundled face counts without being listed**: the text system lists
/// what `fonts::register` handed it, but a caller asking before the first
/// frame has no list yet, and the answer must not depend on when it asked.
fn found(available: &[String], chosen: Option<&str>) -> Option<String> {
    let wanted = chosen?;
    (crate::fonts::BUNDLED.contains(&wanted) || available.iter().any(|name| name == wanted))
        .then(|| wanted.to_owned())
}

/// Fallback fonts for characters the main one lacks.
///
/// Emoji and rare symbols are not even in a Nerd Font; without fallbacks
/// they stay as boxes.
pub fn fallbacks() -> Vec<String> {
    vec![
        "Apple Color Emoji".to_owned(),
        "Apple Symbols".to_owned(),
        "Menlo".to_owned(),
    ]
}

/// The families worth offering for a terminal.
///
/// **A guess, and stated as one.** A machine has hundreds of families and
/// almost none of them can draw a grid, but GPUI hands out names without
/// metrics — so "is this monospace" cannot be asked, only inferred from the
/// name. Every heuristic of that kind misses something.
///
/// That is why the setting stores a *name* rather than a position in this
/// list: a person whose font is missed writes it into the settings file and
/// [`choose`] honours it, list or no list.
pub fn offerable(available: &[String]) -> Vec<String> {
    let mut families: Vec<String> = available
        .iter()
        .filter(|name| {
            MONOSPACE_HINTS.iter().any(|hint| name.contains(hint))
                || MONOSPACE_NAMES.contains(&name.as_str())
        })
        .cloned()
        .collect();

    families.sort();
    families.dedup();
    families
}

/// What a monospace family tends to be called.
const MONOSPACE_HINTS: &[&str] = &["Mono", "Code", "Nerd", "Consol", "Courier"];

/// Monospace families whose names say nothing of it.
const MONOSPACE_NAMES: &[&str] = &["Menlo"];

/// The face **code** is drawn in — the editor, a diff, Annotate.
///
/// Not [`choose`], though the rule is the same shape: what a person asked
/// for wins, and Lilex is everything else. **Never a Nerd Font nobody
/// chose** — a file has none of those glyphs, and M5.9's decision 2 is
/// why the two faces are resolved apart.
pub fn choose_code(available: &[String], chosen: Option<&str>) -> String {
    found(available, chosen).unwrap_or_else(|| crate::fonts::DEFAULT.code.to_owned())
}
