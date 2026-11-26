//! Which faces the terminal and code are drawn with.
//!
//! Out here rather than beside the code because every one of these goes
//! in through the public door — which makes them a check on that door as
//! well as on the behaviour behind it.

use guild_desktop::fonts::{LILEX, LILEX_DUO, LILEX_NERD};
use guild_desktop::terminal::font::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

/// **No choice is Lilex Nerd Font, whatever is installed** — M12.11,
/// decision 4. Until then the terminal chased an installed Nerd Font by
/// name; the one that ships is always there and carries the glyphs a
/// prompt is drawn with.
#[test]
fn no_choice_is_the_nerd_font_that_ships() {
    let installed = names(&["Menlo", "SF Mono", "MesloLGS Nerd Font Mono", "Arial"]);

    assert_eq!(choose(&installed, None), LILEX_NERD);
    assert_eq!(
        choose(&[], None),
        LILEX_NERD,
        "an empty list is no reason for none"
    );
}

#[test]
fn fallbacks_cover_emoji_and_symbols() {
    let list = fallbacks();

    assert!(list.iter().any(|name| name.contains("Emoji")));
    assert!(!list.is_empty());
}

/// What a person picked wins over what we would have preferred.
#[test]
fn a_chosen_font_beats_the_preferences() {
    let installed = names(&["MesloLGS Nerd Font Mono", "Courier New"]);

    assert_eq!(
        choose(&installed, Some("Courier New")),
        "Courier New",
        "a Nerd Font is our preference, not theirs"
    );
}

/// A settings file travels between machines and names a font the next
/// one may not have. Falling through to what ships beats falling through
/// to a blank terminal.
#[test]
fn a_chosen_font_that_is_not_installed_falls_back() {
    let installed = names(&["Menlo", "MesloLGS Nerd Font Mono"]);

    assert_eq!(choose(&installed, Some("A Font Nobody Has")), LILEX_NERD);
}

/// **A face that ships is chosen by name even before the text system has
/// listed it.** The list is read from the machine; what `fonts::register`
/// handed over is in it too, but a caller asking before the first frame
/// has an empty one, and the answer must not depend on when it asked.
#[test]
fn a_bundled_face_is_taken_without_being_listed() {
    assert_eq!(choose(&[], Some(LILEX)), LILEX);
    assert_eq!(choose_code(&[], Some(LILEX_DUO)), LILEX_DUO);
}

/// **Every face that ships is in the table, and every name in the table
/// is a file.** The icons' guard, for the same reason: a face nobody
/// registers is one the window silently draws something else in — no
/// error, no log line, just a different shape of letter.
#[test]
fn every_face_that_ships_is_registered() {
    use std::collections::HashSet;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    let on_disk: HashSet<String> = std::fs::read_dir(&dir)
        .expect("the fonts are beside the crate")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ttf"))
        .collect();
    let in_table: HashSet<String> = guild_desktop::fonts::FACES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();

    assert_eq!(on_disk, in_table, "a face on disk that nothing registers");
    assert!(
        guild_desktop::fonts::FACES
            .iter()
            .all(|(_, bytes)| bytes.len() > 10_000),
        "an empty or truncated face loads as nothing"
    );
}

/// OFL 1.1 asks that the licence travel with the files, and a licence
/// that is not in the repository is not travelling anywhere.
#[test]
fn every_licence_is_beside_the_faces() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");

    for licence in [
        "LICENSE-inter",
        "LICENSE-jetbrains-mono",
        "LICENSE-lilex",
        "LICENSE-lilex-nerd-font",
    ] {
        let text = std::fs::read_to_string(dir.join(licence)).expect("the licence is there");
        assert!(
            text.contains("SIL OPEN FONT LICENSE"),
            "{licence} is not the licence it claims to be"
        );
    }
}

/// The design system's names for the faces are the defaults, and the one
/// place they are spelled is `desktop::fonts`.
#[test]
fn the_theme_names_the_default_faces() {
    assert_eq!(guild_desktop::theme::font::UI, ".SystemUIFont");
    assert_eq!(guild_desktop::theme::font::MONO, "Lilex");
    assert_eq!(
        guild_desktop::fonts::BUNDLED,
        &[
            "Inter",
            "JetBrains Mono",
            "Lilex",
            "Lilex Duo",
            "Lilex Nerd Font"
        ]
    );
}

/// **A file is not a terminal.** A file has no Nerd glyphs, so the editor
/// takes what the person asked for and Lilex otherwise — never a Nerd Font
/// nobody chose, whose metrics are somebody else's patch of somebody
/// else's face.
#[test]
fn the_editor_takes_lilex_rather_than_a_nerd_font() {
    let installed = names(&["MesloLGS Nerd Font Mono", "Menlo", "JetBrains Mono"]);

    assert_eq!(
        choose_code(&installed, None),
        LILEX,
        "nobody asked for the Nerd Font"
    );
    assert_eq!(
        choose_code(&installed, Some("Menlo")),
        "Menlo",
        "and what a person asked for wins"
    );
    assert_eq!(
        choose_code(&installed, Some("A Font Nobody Has")),
        LILEX,
        "a settings file from another machine falls back to what ships"
    );
}

/// **A palette decides colours and nothing else** — M12.11, decision 3.
/// M12.6 put the current faces behind Slate so that no other palette
/// changed; the person asked why a palette decides a font at all. There
/// is one set of defaults now.
#[test]
fn the_defaults_are_super_s_and_no_palette_s() {
    use guild_desktop::fonts::{DEFAULT, Faces};
    assert_eq!(
        DEFAULT,
        Faces {
            ui: ".SystemUIFont",
            code: "Lilex",
            terminal: "Lilex Nerd Font",
        }
    );
}

/// **A button is lit by the setting's name** — decision 6. Nothing is the
/// first button; a name a button carries is that button; any other name
/// is Custom, and so is a name the machine lacks, because it is still the
/// person's choice.
#[test]
fn the_lit_button_follows_the_setting() {
    use guild_desktop::fonts::{Pick, Which, picked, presets};

    let code: Vec<&str> = presets(Which::Code)
        .iter()
        .map(|preset| preset.family)
        .collect();
    assert_eq!(code, [LILEX, LILEX_DUO], "the two Lilex faces, in order");
    let terminal: Vec<&str> = presets(Which::Terminal)
        .iter()
        .map(|preset| preset.family)
        .collect();
    assert_eq!(terminal, [LILEX_NERD], "Lilex Duo has no Nerd glyphs");
    assert!(presets(Which::Code)[1].beta, "upstream calls Duo a beta");

    assert_eq!(picked(Which::Code, None), Pick::Preset(0));
    assert_eq!(picked(Which::Code, Some("Lilex")), Pick::Preset(0));
    assert_eq!(picked(Which::Code, Some("Lilex Duo")), Pick::Preset(1));
    assert_eq!(picked(Which::Code, Some("Menlo")), Pick::Custom);
    assert_eq!(picked(Which::Terminal, None), Pick::Preset(0));
    assert_eq!(picked(Which::Terminal, Some("Lilex Duo")), Pick::Custom);
}
