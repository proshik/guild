//! The faces the window ships with.
//!
//! M5.9. Section 3 of the design system has said since it was written
//! that the families ship with the application; until this pass nothing
//! did, and `theme::font::UI` carried the name of what we fell back to.
//!
//! **Inter and JetBrains Mono**, and the reason is measured rather than
//! tasted: Inter is a common interface face for developer tools, and
//! Inter's x-height is 0.5459em against IBM Plex Sans's 0.5160em —
//! six per cent more ink at the same nominal size, which is why Plex
//! reads thin in a panel as narrow as a file tree.
//!
//! **Since M12.11 the defaults are one set, whatever the palette**: the
//! system face for the interface, Lilex for code and Lilex
//! Nerd Font for the terminal — a prompt is drawn with Nerd glyphs, and
//! without them a `powerlevel10k` prompt is a row of boxes, the trap M5.1
//! paid for. Inter and JetBrains Mono still ship, as rows of the list.

use std::borrow::Cow;

/// Inter, the interface face from M5.9 to M12.11.
pub const UI: &str = "Inter";
/// JetBrains Mono, the code face from M5.9 to M12.11.
pub const MONO: &str = "JetBrains Mono";
/// Lilex, the code face since M12.11.
pub const LILEX: &str = "Lilex";
/// Lilex's duospaced sibling: a wide letter takes a cell and a half.
/// Experimental upstream, labelled *Beta*, and offered for code only.
pub const LILEX_DUO: &str = "Lilex Duo";
/// Lilex patched with Nerd glyphs, the terminal's face — M12.6.
pub const LILEX_NERD: &str = "Lilex Nerd Font";

/// Every face that ships, as bytes.
///
/// **One table, and a test walks the directory against it** — the icons'
/// rule, for the same reason: a face nobody registers is a face the
/// window silently draws something else in.
pub const FACES: &[(&str, &[u8])] = &[
    (
        "Inter-Regular.ttf",
        include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    ),
    (
        "Inter-Medium.ttf",
        include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    ),
    (
        "Inter-SemiBold.ttf",
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    ),
    (
        "Inter-Italic.ttf",
        include_bytes!("../assets/fonts/Inter-Italic.ttf"),
    ),
    (
        "JetBrainsMono-Regular.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
    (
        "JetBrainsMono-Bold.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        "JetBrainsMono-Italic.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    ),
    (
        "JetBrainsMono-BoldItalic.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    ),
    (
        "Lilex-Regular.ttf",
        include_bytes!("../assets/fonts/Lilex-Regular.ttf"),
    ),
    (
        "Lilex-Bold.ttf",
        include_bytes!("../assets/fonts/Lilex-Bold.ttf"),
    ),
    (
        "Lilex-Italic.ttf",
        include_bytes!("../assets/fonts/Lilex-Italic.ttf"),
    ),
    (
        "Lilex-BoldItalic.ttf",
        include_bytes!("../assets/fonts/Lilex-BoldItalic.ttf"),
    ),
    (
        "LilexDuo-Regular.ttf",
        include_bytes!("../assets/fonts/LilexDuo-Regular.ttf"),
    ),
    (
        "LilexDuo-Bold.ttf",
        include_bytes!("../assets/fonts/LilexDuo-Bold.ttf"),
    ),
    (
        "LilexNerdFont-Regular.ttf",
        include_bytes!("../assets/fonts/LilexNerdFont-Regular.ttf"),
    ),
    (
        "LilexNerdFont-Bold.ttf",
        include_bytes!("../assets/fonts/LilexNerdFont-Bold.ttf"),
    ),
    (
        "LilexNerdFont-Italic.ttf",
        include_bytes!("../assets/fonts/LilexNerdFont-Italic.ttf"),
    ),
    (
        "LilexNerdFont-BoldItalic.ttf",
        include_bytes!("../assets/fonts/LilexNerdFont-BoldItalic.ttf"),
    ),
];

/// Hand the faces to the text system.
///
/// **Before the window opens.** A face registered after the first frame
/// is a first frame drawn in something else, and the difference is
/// visible: the window flashes from the system sans into ours.
pub fn register(cx: &gpui::App) {
    let faces: Vec<Cow<'static, [u8]>> = FACES
        .iter()
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .collect();
    match cx.text_system().add_fonts(faces) {
        Ok(()) => tracing::info!(faces = FACES.len(), "the bundled faces are registered"),
        // Not fatal: the window falls back to what the machine has, which
        // is what it did before this pass. Said out loud, because a
        // window in the wrong font with nothing in the log is a bug
        // report nobody can act on.
        Err(err) => tracing::warn!(%err, "the bundled faces were not registered"),
    }
}

/// The system's own interface face, by the name GPUI resolves on macOS.
pub const SYSTEM_UI: &str = ".SystemUIFont";

/// The families that ship, for a list to offer them.
pub const BUNDLED: &[&str] = &[UI, MONO, LILEX, LILEX_DUO, LILEX_NERD];

/// What the window draws in when a person has chosen nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Faces {
    /// The interface.
    pub ui: &'static str,
    /// Code: the editor, a diff, Annotate.
    pub code: &'static str,
    /// The terminal.
    pub terminal: &'static str,
}

/// The faces *no choice* means: the system face for the interface, at 13
/// to the pixel, Lilex for code and Lilex Nerd Font for the terminal.
///
/// **One set, not one per palette** — M12.11, decision 3. From M12.6 to
/// M12.11 these were Slate's and every other palette kept Inter and
/// JetBrains Mono; a palette is colours, and the person asked why it
/// should decide a font.
pub const DEFAULT: Faces = Faces {
    ui: SYSTEM_UI,
    code: LILEX,
    terminal: LILEX_NERD,
};

/// Which setting a list is choosing for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The interface: every family the machine has, because a face for
    /// reading is not a face for a grid and nothing can tell them apart
    /// by name.
    Ui,
    /// Code: the families that look like they can draw a grid, which is a
    /// guess and is stated as one where it is made.
    Code,
    /// The terminal, from the same families as code — M12.11.
    Terminal,
}

impl Which {
    /// The face *no choice* means for this setting.
    pub fn default_face(self) -> &'static str {
        match self {
            Which::Ui => DEFAULT.ui,
            Which::Code => DEFAULT.code,
            Which::Terminal => DEFAULT.terminal,
        }
    }
}

/// One of the buttons a setting offers before *Custom*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// The family the button chooses, and the name it is labelled with.
    pub family: &'static str,
    /// Whether it is labelled *Beta* — Lilex Duo, as upstream calls it.
    pub beta: bool,
}

/// The buttons a setting offers; the first is what *no choice* draws.
/// The interface offers none: its setting is its list alone.
pub fn presets(which: Which) -> &'static [Preset] {
    match which {
        Which::Ui => &[],
        Which::Code => &[
            Preset {
                family: LILEX,
                beta: false,
            },
            Preset {
                family: LILEX_DUO,
                beta: true,
            },
        ],
        Which::Terminal => &[Preset {
            family: LILEX_NERD,
            beta: false,
        }],
    }
}

/// Which button is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// One of [`presets`], by position.
    Preset(usize),
    /// Anything else the setting names.
    Custom,
}

/// The lit button, from the setting's name — M12.11, decision 6. No
/// choice is the first button, because that is what it draws; a name a
/// button carries is that button; any other name is *Custom*, installed or
/// not, because it is still the person's choice.
pub fn picked(which: Which, setting: Option<&str>) -> Pick {
    let presets = presets(which);
    match setting {
        None if !presets.is_empty() => Pick::Preset(0),
        None => Pick::Custom,
        Some(family) => presets
            .iter()
            .position(|preset| preset.family == family)
            .map_or(Pick::Custom, Pick::Preset),
    }
}

/// Where a row's face comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    /// No choice at all — what the window picks for itself.
    System,
    /// A face that ships with the window.
    Bundled,
    /// A face the machine has.
    Installed,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The family, or nothing for *from the system*.
    pub family: Option<String>,
    /// Where it comes from.
    pub from: From,
}

/// The list a person chooses from: the way back to no choice, then what
/// ships, then what is installed — narrowed by what has been typed.
///
/// **The bundled faces are not repeated among the installed ones.** A
/// machine that also has Inter installed would otherwise offer it twice,
/// and the two rows would do the same thing.
///
/// **Found by scattered letters, best first within each group** since
/// M8.9 — `jbm` is JetBrains Mono. The groups keep their order: what ships
/// is a handful of rows a person is meant to see first.
pub fn rows(which: Which, query: &str, installed: &[String]) -> Vec<Row> {
    let query = query.trim();
    let found = |families: &[String]| -> Vec<String> {
        crate::fuzzy::rank(query, families, String::as_str, crate::fuzzy::Shape::Name)
            .into_iter()
            .map(|(index, _)| families[index].clone())
            .collect()
    };

    let mut rows = Vec::new();
    // Code's and the terminal's way back is the first button beside
    // *Custom*: no choice is Lilex there, and a row saying *from the
    // system* would say something else — M12.11.
    if query.is_empty() && which == Which::Ui {
        rows.push(Row {
            family: None,
            from: From::System,
        });
    }
    // The interface offers both — somebody may want their code in Inter
    // or their interface in a mono — and so does code.
    // Not Lilex Duo for the terminal: no Nerd glyphs, and a wide letter a
    // cell and a half across, which a grid cannot draw — decision 1.
    let bundled: Vec<String> = BUNDLED
        .iter()
        .filter(|family| !(which == Which::Terminal && **family == LILEX_DUO))
        .map(|family| (*family).to_owned())
        .collect();
    rows.extend(found(&bundled).into_iter().map(|family| Row {
        family: Some(family),
        from: From::Bundled,
    }));
    let offered: Vec<String> = match which {
        Which::Ui => installed.to_vec(),
        Which::Code | Which::Terminal => crate::terminal::font::offerable(installed),
    }
    .into_iter()
    .filter(|family| !BUNDLED.contains(&family.as_str()))
    .collect();
    rows.extend(found(&offered).into_iter().map(|family| Row {
        family: Some(family),
        from: From::Installed,
    }));
    rows
}
