//! Design tokens: colour, size, spacing, radius.
//!
//! The source of truth is
//! `docs/design-system.md`. Here is its
//! copy as numbers GPUI understands. A colour literal in widget code is a
//! reason to open the document rather than eyeball a shade: that is
//! exactly how an interface drifts away from its reference.
//!
//! The rule the document was written for in the first place: **a border
//! marks state, not structure.** Only an active element, or one that needs
//! attention, has an outline; everything else is separated by a step in
//! background. A border around every block turns the interface into a
//! spreadsheet.

use gpui::{Hsla, Rgba};

mod card_outline;
mod custom;
pub use custom::{FOLLOW_CAP, HIGHLIGHT_CAP, PRESETS, SURFACE_CAP, TEXT_CAP, TEXT_SHARE};
pub(crate) mod diff_center;
mod palettes;
pub mod tint;
pub use card_outline::card_outline;
pub mod push;
pub mod settings;

/// Surfaces — from the furthest to the nearest.
/// Text. The contrast is spread wide on purpose: two similar mid-greys
/// are the most common cause of a muddy look.
/// Accents. Colour sparingly but vividly: muting everything equally means
/// highlighting nothing.
pub use guild_session::ThemeKind as Kind;
pub use guild_session::ThemeMode as Mode;

/// Whether macOS is drawing its dark appearance — what a System mode follows.
pub fn system_dark(appearance: gpui::WindowAppearance) -> bool {
    matches!(
        appearance,
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    )
}

/// The palette the window draws with for these settings while the system
/// is dark or not: the mode's palette with the person's tints over it,
/// computed from the built-in palette each time.
pub fn in_effect(settings: &guild_session::Settings, system_dark: bool) -> Theme {
    Theme::resolve(settings.theme_kind(system_dark), &settings.theme_custom)
}

/// What `NSApp.appearance` is for the Theme mode a person chose: System
/// leaves AppKit to the system, Light and Dark force aqua and darkAqua —
/// so what AppKit draws for Guild (title bars and traffic lights, panels,
/// sheets, menus) is in the chosen mode. It follows the saved mode only:
/// while the customizer previews the other palette, AppKit stays on the
/// mode until one is chosen.
pub fn forced_appearance(mode: Mode) -> Option<gpui::WindowAppearance> {
    match mode {
        Mode::System => None,
        Mode::Light => Some(gpui::WindowAppearance::Light),
        Mode::Dark => Some(gpui::WindowAppearance::Dark),
    }
}

/// Hand `mode` to AppKit through `set`, and for System answer whether
/// the system is dark, through `read`.
///
/// **The override is cleared before the system is read.** While an
/// appearance is forced, AppKit reports the forced one; read first and a
/// return to System would keep the mode it is leaving. An explicit mode
/// answers `None`: the palette does not ask the system then.
pub fn follow_mode(
    mode: Mode,
    set: impl FnOnce(Option<gpui::WindowAppearance>),
    read: impl FnOnce() -> gpui::WindowAppearance,
) -> Option<bool> {
    let forced = forced_appearance(mode);
    set(forced);
    forced.is_none().then(|| system_dark(read()))
}

/// Every colour the interface draws with.
///
/// A value rather than a set of `const`s, because there are two palettes
/// and a person picks one. The values live in section 2 of the design
/// system and the tests below compare these against it — the document is
/// the source and this is its copy, which is the only way that comparison
/// means anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Which one this is.
    pub kind: Kind,
    /// Backgrounds, from the one behind the window to the one on a badge.
    pub surface: Surface,
    /// The ranks of text, loudest first.
    pub text: Text,
    /// The colours that mean something: a status, an action, an error.
    pub accent: Accent,
    /// The seven colours code is set in. Section 2.4, and section 2.6 for
    /// the other four palettes.
    pub syntax: Syntax,
    /// The two backgrounds of a diff. Section 2.7.
    pub diff: Diff,
}

/// The two backgrounds of a diff. Section 2.7: behind an added line and
/// behind a removed one; the markers are accents the theme already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diff {
    /// `diff.added`
    pub added: u32,
    /// `diff.removed`
    pub removed: u32,
}

/// What git says about a file, shown as a subtle row tint. Section 2.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// `?` — git does not know the file.
    Untracked,
    /// `A` — added since the last commit.
    Added,
    /// `D` — gone since the last commit.
    Deleted,
    /// `M` — changed since the last commit.
    Modified,
}

impl Mark {
    /// The letter.
    pub fn letter(self) -> &'static str {
        match self {
            Mark::Untracked => "?",
            Mark::Added => "A",
            Mark::Deleted => "D",
            Mark::Modified => "M",
        }
    }
}

/// Whether a file-list row is selected, and whether its list has the keys —
/// two selection colours. Decision 54.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    No,
    /// Selected in the list that has the keys.
    Focused,
    /// Selected in a list whose keys are elsewhere.
    Unfocused,
}

impl Selected {
    pub fn of(selected: bool, focused: bool) -> Self {
        match (selected, focused) {
            (false, _) => Selected::No,
            (true, true) => Selected::Focused,
            (true, false) => Selected::Unfocused,
        }
    }
}

impl Theme {
    /// Muted identifier backgrounds, independent of component defaults.
    pub fn identifier_occurrence(&self) -> gpui::Rgba {
        gpui::rgb(self.text.primary).opacity(alpha::IDENTIFIER_OCCURRENCE)
    }

    /// Full-width file-list ground — decision 54. A selection is one solid
    /// fill and replaces the Git tint: `accent.patina_bg` while the list
    /// has the keys, [`alpha::SELECTION_INACTIVE`] of primary ink over the
    /// ground while they are elsewhere.
    pub fn file_row(&self, mark: Option<Mark>, selected: Selected, ground: u32) -> gpui::Rgba {
        match selected {
            Selected::Focused => gpui::rgb(self.accent.patina_bg),
            Selected::Unfocused => gpui::rgb(ground)
                .blend(gpui::rgb(self.text.primary).opacity(alpha::SELECTION_INACTIVE)),
            Selected::No => mark.map_or(gpui::rgb(ground), |mark| {
                gpui::rgb(ground).blend(gpui::rgb(self.mark(mark)).opacity(alpha::GIT_ROW))
            }),
        }
    }

    /// An unselected file-list row under the pointer: [`alpha::ROW_HOVER`]
    /// of primary ink over what the row shows at rest, Git tint included.
    /// A selected row keeps its selection under the pointer.
    pub fn file_row_hover(&self, mark: Option<Mark>, ground: u32) -> gpui::Rgba {
        self.file_row(mark, Selected::No, ground)
            .blend(gpui::rgb(self.text.primary).opacity(alpha::ROW_HOVER))
    }

    /// The colour a mark is drawn in — section 2.7 names the accents.
    pub fn mark(&self, mark: Mark) -> u32 {
        match mark {
            Mark::Untracked => self.text.faint,
            Mark::Added => self.accent.green,
            Mark::Deleted => self.accent.rust,
            Mark::Modified => self.accent.brass,
        }
    }
}

/// Backgrounds. Section 2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    /// `surface.backdrop`
    pub backdrop: u32,
    /// `surface.app`
    pub app: u32,
    /// `surface.panel`
    pub panel: u32,
    /// `surface.sunken`
    pub sunken: u32,
    /// `surface.inset`
    pub inset: u32,
    /// `surface.card`
    pub card: u32,
    /// `surface.card_muted`
    pub card_muted: u32,
    /// `surface.raised`
    pub raised: u32,
    /// `surface.badge`
    pub badge: u32,
    /// `surface.divider`
    pub divider: u32,
}

/// Text ranks. Section 2.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    /// `text.primary`
    pub primary: u32,
    /// `text.secondary`
    pub secondary: u32,
    /// `text.tertiary`
    pub tertiary: u32,
    /// `text.muted`
    pub muted: u32,
    /// `text.faint`
    pub faint: u32,
    /// `text.gutter`
    pub gutter: u32,
}

/// Accents. Section 2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    /// `accent.brass`
    pub brass: u32,
    /// `accent.brass_bg`
    pub brass_bg: u32,
    /// `accent.patina`
    pub patina: u32,
    /// `accent.patina_bg`
    pub patina_bg: u32,
    /// `accent.green`
    pub green: u32,
    /// `accent.rust`
    pub rust: u32,
    /// `accent.rust_bg`
    pub rust_bg: u32,
    /// `accent.violet`
    pub violet: u32,
    /// `accent.highlight` — the ground of selected text and of a search
    /// match: the editor's selection, a find's matches in a file and in a
    /// terminal, speed search's letters. Equal to `brass_bg` in both
    /// built-in palettes, and apart from it because `brass_bg` is also a
    /// warning's ground, a drop target and a lit toggle, which Highlight
    /// must not recolour (`two-themes-customize`, D5).
    pub highlight: u32,
}

impl Theme {
    /// The colour and the face a paint asks for: the colour, whether it
    /// is italic, whether it is bold.
    ///
    /// **One answer, two askers.** The component's highlight theme is
    /// built from it and our own spans are drawn with it; a second
    /// mapping is how the tab and the diff would start to disagree about
    /// what a comment looks like.
    pub fn of_paint(&self, paint: crate::highlight::Paint) -> (u32, bool, bool) {
        use crate::highlight::{Paint, Role};
        // **Italic is the palette's to say, since M8.10.** It was one
        // hardcoded rule — *the comment is italic* — which is section
        // 2.4's decision for Guild's own palette and is wrong for the Dark
        // one, where a line comment is upright and only a doc comment leans.
        let ink = |ink: Ink| (ink.colour, ink.italic, false);
        match paint {
            Paint::Role(Role::Comment) => ink(self.syntax.comment),
            Paint::Role(Role::Doc) => ink(self.syntax.doc),
            Paint::Role(Role::Keyword) => ink(self.syntax.keyword),
            Paint::Role(Role::Type) => ink(self.syntax.ty),
            Paint::Role(Role::String) => ink(self.syntax.string),
            Paint::Role(Role::Number) => ink(self.syntax.number),
            Paint::Role(Role::Constant) => ink(self.syntax.constant),
            Paint::Role(Role::Macro) => ink(self.syntax.r#macro),
            Paint::Role(Role::Function) => ink(self.syntax.function),
            Paint::Role(Role::Tag) => ink(self.syntax.tag),
            Paint::Role(Role::TypeRef) => ink(self.syntax.type_ref),
            Paint::Role(Role::Namespace) => ink(self.syntax.namespace),
            Paint::Role(Role::Builtin) => ink(self.syntax.builtin),
            Paint::Plain => (self.text.primary, false, false),
            Paint::Quiet => (self.text.secondary, false, false),
            Paint::Emphasis => (self.text.primary, true, false),
            Paint::Strong => (self.text.primary, false, true),
        }
    }
}

/// Syntax highlighting. Section 2.4 for Guild's own palette; section 2.6
/// for the other four, two of them transcribed from a token file and
/// two derived from a terminal palette by the rule written there.
///
/// Seven roles and no more: every capture a grammar can name falls onto
/// one of these or onto a text rank, by the table in section 2.4. A theme
/// that named forty would be forty numbers nobody checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax {
    /// `keyword`, `type.builtin`, an escape in a string.
    pub keyword: Ink,
    /// `type`, `constructor`, `enum`, `variant`. Not `type`: that is a keyword in Rust.
    pub ty: Ink,
    /// `string`.
    pub string: Ink,
    /// `number`, `boolean`.
    pub number: Ink,
    /// `comment`.
    pub comment: Ink,
    /// `comment_doc` — a documentation comment, which the Dark palette
    /// draws in a colour of its own. M8.10.
    pub doc: Ink,
    /// `attribute`, `preproc`, and a macro invocation.
    pub r#macro: Ink,
    /// `function`, and a Markdown title.
    pub function: Ink,
    /// `constant` — a role of its own, italic in the Dark palette. M8.10.
    pub constant: Ink,
    /// `tag` — a markup element's name. M8.10.
    pub tag: Ink,
    /// A **reference** to a type, where a grammar's own row asks for one
    /// instead of `ty` — Go's. M8.10's layer.
    pub type_ref: Ink,
    /// A package or module's name, where a grammar asks for one.
    pub namespace: Ink,
    /// A built-in of the language: Go's `len`, `nil`, `int64`.
    pub builtin: Ink,
}

/// A colour and whether the role is set in italic.
///
/// **Both belong to the palette**: the Dark palette's line comment is upright
/// and its doc comment leans, and Guild's own palette leans its comments —
/// one rule in code could not say both. M8.10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ink {
    /// `0xrrggbb`.
    pub colour: u32,
    /// Whether it is drawn in italic.
    pub italic: bool,
}

impl Ink {
    /// Upright.
    pub const fn plain(colour: u32) -> Self {
        Self {
            colour,
            italic: false,
        }
    }

    /// Leaning.
    pub const fn italic(colour: u32) -> Self {
        Self {
            colour,
            italic: true,
        }
    }
}

/// A length from the design system, in the unit the interface uses.
///
/// The document's numbers are pixels at the base interface size, and this
/// is the single place they are turned into rems — so every token below
/// still reads as the number the document gives, and a person checking one
/// against the document compares like with like.
pub(crate) const fn from_px(pixels: f32) -> gpui::Rems {
    gpui::Rems(pixels / crate::scale::BASE)
}

/// Font sizes. The scale is wide on purpose: a range of three or four
/// points creates no hierarchy.
/// The colours that belong to somebody else, and so to no palette.
///
/// Section 2.8 of the design system. A theme's accent says what a thing
/// is doing; a brand's mark says whose it is, and is drawn in its own
/// colour whatever the person picked.
pub mod brand {
    /// Claude's orange, the colour of its mark — the mark on a tab that
    /// holds Claude Code.
    pub const CLAUDE: u32 = 0xce825f;
    /// OpenCode's mark, drawn on a tab that holds OpenCode. A brand blue,
    /// chosen to sit apart from Claude's orange in the same strip.
    pub const OPENCODE: u32 = 0x4f9cf9;
    /// Codex's mark, drawn on a tab that holds Codex — OpenAI's green.
    pub const CODEX: u32 = 0x10a37f;
}

pub mod size {
    use gpui::Rems;

    use super::from_px;

    /// A section heading.
    pub const SECTION: Rems = from_px(20.0);
    /// A task heading in a panel.
    pub const TITLE: Rems = from_px(16.0);
    /// A card heading.
    pub const CARD_TITLE: Rems = from_px(13.5);
    /// Body text.
    pub const BODY: Rems = from_px(13.0);
    /// Interface text, code, the tree.
    pub const UI: Rems = from_px(12.5);
    /// Metadata.
    pub const META: Rems = from_px(11.5);
    /// An eyebrow in capitals.
    pub const EYEBROW: Rems = from_px(11.0);
}

/// Fixed activity geometry, section 6.29. The ring/glyph reuse the existing
/// heading/file icon sizes at the base scale; the leading slot stays fixed
/// when text grows so branch alignment and workspace button geometry stay put.
pub mod activity {
    /// One clockwise linear revolution in 1.2 seconds.
    pub const ROTATION_SECONDS: f32 = 1.2;
    /// Decorative motion is capped at 20 Hz, independently of display refresh.
    pub const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
    use gpui::{Pixels, px};

    pub const RING: Pixels = px(14.0);
    pub const GLYPH: Pixels = px(super::space::ICON.0 * crate::scale::BASE);
    pub const DOT: Pixels = px(8.0);
    pub const SLOT_WIDTH: Pixels = RING;
    pub const SLOT_HEIGHT: Pixels = px(18.0);
    /// Between a state's mark and its number in the status bar's counters.
    pub const COUNT_GAP: Pixels = px(4.0);
    /// Between one counter and the next.
    pub const COUNTS_GAP: Pixels = px(10.0);
    /// Between a state's mark and its word in the sessions popup.
    pub const STATE_GAP: Pixels = px(6.0);
}

/// Fixed sidebar grid from section 6.29; action centers stay aligned even when
/// a control has a larger hit target or the interface text scale changes.
pub mod column {
    use gpui::{Pixels, px};

    pub const LIST_INSET: Pixels = px(6.0);
    pub const HEADER_PADDING: Pixels = px(10.0);
    pub const ACTION_TARGET: Pixels = px(18.0);
    /// How far right of the *Projects* heading the list's top-level items
    /// stand — groups and ungrouped projects — so the heading reads as the
    /// root of a tree.
    pub const TOP_INDENT: Pixels = px(8.0);
    /// How far right of its header a group's projects stand.
    pub const GROUP_INDENT: Pixels = px(12.0);
}

/// Line height.
pub mod leading {
    /// For text.
    pub const TEXT: f32 = 1.45;
    /// For code. Verified: 1.78 does not fit 24 editor lines.
    pub const CODE: f32 = 1.70;
    /// For the terminal: a 12-point face on 16-point lines, chosen on
    /// 2026-09-24 because it fits more lines than the looser spacing.
    pub const TERMINAL: f32 = 16.0 / 12.0;
}

/// Spacing and block sizes.
pub mod space {
    use gpui::Rems;

    use super::from_px;

    /// The window title bar.
    pub const TITLEBAR: Rems = from_px(34.0);
    /// What stands above the title bar's contents, putting their centre a
    /// point below the bar's — the person's optical centring, 2026-09-25:
    /// measured exactly centred, the buttons read as high between the
    /// window's light top edge and the card under the bar.
    pub const TITLEBAR_OFFSET: Rems = from_px(2.0);
    /// The traffic lights' frame's top, in pixels — `TitlebarOptions`
    /// takes pixels. A 14-high frame at 11 is centred where the bar's
    /// contents are: a point below the bar's centre line.
    pub const TRAFFIC_LIGHTS_TOP: f32 = 11.0;
    /// How much of the title bar the macOS traffic lights take.
    ///
    /// Three 12px buttons with 8px between them, from an inset of 12 —
    /// and then a gap, so our own content does not sit against them. Not
    /// in the design system: the mockups draw the lights for presentation
    /// and say the system provides them, which stops being true the
    /// moment the title bar is ours to draw in.
    pub const TRAFFIC_LIGHTS: Rems = from_px(82.0);
    /// Native AppKit buttons keep their point geometry when Guild scales down.
    pub const TRAFFIC_LIGHTS_MIN: f32 = TRAFFIC_LIGHTS.0 * crate::scale::BASE;
    /// A drop zone at a side of the window while a tool is dragged —
    /// the width M8.3's rail had. Section 6.15.
    pub const DROP_ZONE: Rems = from_px(52.0);
    /// The active sessions panel.
    pub const SESSIONS_PANEL: Rems = from_px(330.0);
    /// The column of projects and their worktrees, down the left of the
    /// window — section 6.29, M12.
    pub const COLUMN: Rems = from_px(232.0);
    /// Logical-point target centered on the column boundary, independent of UI scale.
    pub const COLUMN_RESIZE_REACH: f32 = 9.0;
    /// The terminal's scrollbar: the thumb's width and its inset from
    /// the right edge.
    ///
    /// Not in the design system — it has no scrollbar — so the numbers are
    /// the thinnest that still reads as a control next to 12.5px text.
    /// Section 5 is the place to add it if one is ever specified.
    pub const SCROLLBAR: Rems = from_px(5.0);

    /// The width of a notice in the corner.
    ///
    /// Wide enough for a shortened path with a reason after it, narrow
    /// enough not to cover the tab it is reporting on.
    pub const NOTICE: Rems = from_px(320.0);

    /// How far a notice sits from the window's edges.
    pub const NOTICE_INSET: Rems = from_px(16.0);

    /// A panel's heading strip: the tab strip's height, so that a panel's
    /// title and the tabs beside it are one line across the window and the
    /// tree begins where the editor begins. Section 5, since the first use
    /// of M8.1 — it was 26, half a section header, when the strip spanned
    /// the window and the panels started under it; 42 until M12.6, 36 since,
    /// with the strip.
    pub const PANEL_HEAD: Rems = from_px(36.0);

    /// The strip along the bottom of the window.
    ///
    /// Not in the design system either. It sits under the title strip's 34
    /// so the window reads as a frame with the work inside it, and it is the
    /// same height as a panel's heading because both are a line of labels.
    pub const STATUS_BAR: Rems = from_px(28.0);
    /// A list down the left edge of a panel — the settings' sections.
    ///
    /// The same width as the file tree in section 5 of the design system:
    /// both are a narrow column of names beside a wide pane, and giving
    /// them different widths would break the rhythm for no reason.
    pub const SIDE_LIST: Rems = from_px(230.0);
    /// A section header.
    pub const SECTION_HEADER: Rems = from_px(64.0);
    /// The tab strip — 36 since M12.6, section 4.1.
    pub const TABS: Rems = from_px(36.0);
    /// A tab's widest — its name is cut short past it. Section 6.5, M12.6.
    pub const TAB_WIDTH: Rems = from_px(270.0);
    /// A tab's inset at its sides — section 6.5, M12.6.
    pub const TAB_PADDING: Rems = from_px(12.0);
    /// From a tab's mark to its name — section 6.5, M12.6.
    pub const TAB_GAP: Rems = from_px(9.0);
    /// The underline of the tab in front — section 6.5, M12.6.
    pub const TAB_UNDERLINE: Rems = from_px(2.0);
    /// The rule between the last tab and the add button — section 6.5, M12.6.
    pub const STRIP_RULE: Rems = from_px(20.0);
    /// How far the work's card stands in from the window's right, and the
    /// bottom dock's from both sides — section 4.1, M12.6.
    pub const WORK_INSET: Rems = from_px(6.0);
    /// The window's colour under a card — section 4.1, M12.6.
    pub const WORK_GAP: Rems = from_px(6.0);
    /// The gap between the work's card and the bottom dock's, where the
    /// grip lies — section 4.1. **The same as every other gap**, by the
    /// person's word on 2026-09-18: the grey between the black panels one
    /// width across and down, and the thinner one. It was 10, by the same
    /// person's word in M12.6, when 6 read as one card broken in half.
    pub const WORK_BETWEEN: Rems = from_px(WORK_BETWEEN_PX);
    /// [`WORK_BETWEEN`] in points, for the grip that is centred in it.
    pub const WORK_BETWEEN_PX: f32 = 6.0;
    /// The least an area or a pane is made — decision 17 of M12.8, section
    /// 5: a terminal of about forty columns under a strip across, and of
    /// about seven lines down. A split that would give a side less is
    /// refused rather than made narrow, and a dragged border stops here.
    /// Existing configured-process phase width, independent of retired native review.
    pub const PROCESS_PHASE_MIN: Rems = from_px(168.0);
    pub const AREA_MIN_WIDTH: Rems = from_px(320.0);
    pub const AREA_MIN_HEIGHT: Rems = from_px(160.0);
    /// Panel padding.
    /// Generic terminal overlay geometry. Values retain the established layout.
    pub const TERMINAL_OVERLAY_OBSERVER_WIDTH: Rems = from_px(1000.0);
    pub const TERMINAL_OVERLAY_OBSERVER_HEIGHT: Rems = from_px(640.0);
    pub const TERMINAL_OVERLAY_COMPACT_WIDTH: Rems = from_px(650.0);
    pub const TERMINAL_OVERLAY_COMPACT_HEIGHT: Rems = from_px(400.0);
    pub const TERMINAL_OVERLAY_MIN_WIDTH: Rems = from_px(440.0);
    pub const TERMINAL_OVERLAY_COMPACT_MIN_HEIGHT: Rems = from_px(300.0);
    pub const TERMINAL_OVERLAY_EXPANDED_MIN_HEIGHT: Rems = from_px(460.0);
    pub const PANEL_PADDING: Rems = from_px(18.0);
    /// The gap between cards.
    pub const CARD_GAP: Rems = from_px(8.0);
    /// A card's inner padding.
    pub const CARD_PADDING: Rems = from_px(13.0);
    /// The gaps inside a card.
    pub const CARD_INNER_GAP: Rems = from_px(10.0);
    /// A file tree row's height. Section 6.4.
    pub const TREE_ROW: Rems = from_px(22.0);
    /// A file tree row's horizontal padding. Section 6.4.
    pub const TREE_PADDING: Rems = from_px(10.0);
    /// The indent step in the tree. Section 5.
    pub const TREE_INDENT: Rems = from_px(18.0);
    /// The disclosure chevron. Section 6.4.
    pub const CHEVRON: Rems = from_px(11.0);
    /// A file icon. Section 6.4.
    pub const ICON: Rems = from_px(16.0);
    /// Main source editor Git stripe lane, separate from number/run cells.
    pub const GIT_MARKER_LANE: Rems = from_px(6.0);
    /// Width of an Added/Modified stripe within that lane.
    pub const GIT_MARKER_STRIPE: Rems = from_px(3.0);
    /// Height of a full-lane Deleted boundary mark.
    pub const GIT_MARKER_DELETED: Rems = from_px(3.0);
    /// The gap between the chevron, the icon and the name in a row.
    pub const TREE_GAP: Rems = from_px(6.0);
    /// Legacy tree inset retained inside full-width rows and around controls.
    pub const TREE_INSET: Rems = from_px(4.0);
    /// The unsaved-changes dot on a tab. Section 6.5.
    pub const UNSAVED_DOT: Rems = from_px(5.0);
    /// A control in a panel's heading — the crosshair. Section 6.4.
    pub const HEAD_ICON: Rems = from_px(16.0);
    /// A tool's tab in a dock head that holds more than one. Section 6.16.
    pub const HEAD_TAB: Rems = from_px(24.0);
    /// The branch popup, section 6.18: the card's width, a tab, the
    /// field, a row, and how tall the list may grow before it scrolls.
    pub const BRANCH_POPUP: Rems = from_px(460.0);
    pub const BRANCH_TAB: Rems = from_px(28.0);
    pub const BRANCH_FIELD: Rems = from_px(34.0);
    pub const BRANCH_ROW: Rems = from_px(44.0);
    pub const BRANCH_LIST: Rems = from_px(320.0);
    /// Update branch's source popup, hanging under the title bar's ↓:
    /// narrower than the branch popup, whose rows carry a second line —
    /// a source row is one name and a faint tag, and 300 holds
    /// `origin/` and a long branch name at `ui` beside it. Its list stops
    /// at seven `MENU_ROW`s before it scrolls.
    pub const UPDATE_SOURCES: Rems = from_px(300.0);
    pub const UPDATE_SOURCES_LIST: Rems = from_px(224.0);
    /// The diff's toolbar, the bar of marks at its right, and a mark.
    /// Section 6.10.
    pub const DIFF_TOOLBAR: Rems = from_px(42.0);
    pub const MARK_BAR: Rems = from_px(10.0);
    pub const MARK: Rems = from_px(2.0);
    /// The annotate tab's column: an author and a date. Section 6.14.
    pub const ANNOTATE_COLUMN: Rems = from_px(200.0);
    /// The width from which the Log has three columns. Section 6.13.
    ///
    /// The two outer columns' own widths are not here: since M8.3.2 a
    /// person drags them, so they are state rather than a token, and
    /// their design-system defaults sit with the panel widths in
    /// `guild_session::Columns` — one place, as `Panels::default` has
    /// held the panels' since M5.7.
    pub const LOG_WIDE: Rems = from_px(700.0);
    /// A log row — two lines — and how tall the opened commit below the
    /// list may grow. Section 6.13.
    pub const LOG_ROW: Rems = from_px(26.0);
    /// The author's column in the log, wide enough for a full name and
    /// no wider: past this the summary is what a person is reading.
    pub const LOG_AUTHOR: Rems = from_px(160.0);
    /// The dot that says *a commit is here*, and the ring that says the
    /// commit here is a merge.
    pub const LOG_DOT: Rems = from_px(7.0);
    pub const LOG_GRAPH_LANE: Rems = from_px(14.0);
    pub const LOG_GRAPH_STROKE: Rems = from_px(1.5);
    /// Preserve summary space in narrow history columns; graph lanes compress together.
    pub const LOG_GRAPH_FRACTION: f32 = 0.25;
    pub const LOG_AUTHOR_FRACTION: f32 = 0.22;
    pub const LOG_REFS_FRACTION: f32 = 0.25;
    /// The least a log row's summary keeps — four characters and an
    /// ellipsis at `ui`. It gives way first and down to this; only then
    /// do the chips end in an ellipsis, while the author and the graph keep
    /// their places so the column stays one line.
    pub const LOG_SUMMARY_MIN: Rems = from_px(40.0);
    /// The log's filter strip. Shorter than a panel head: it is content,
    /// not chrome, and it sits under one already.
    pub const LOG_TOOLBAR: Rems = from_px(32.0);
    /// The field in it, wide enough for a sentence of a commit message.
    pub const LOG_FIELD: Rems = from_px(220.0);
    /// The Search toolbar's file mask, narrower than the query beside
    /// it: `*.rs` is what goes in it, not a sentence. M8.7.
    /// Maximum width of either aligned local Find/Replace field.
    pub const LOCAL_SEARCH_FIELD: Rems = from_px(440.0);
    pub const SEARCH_MASK_FIELD: Rems = from_px(120.0);
    /// Search panel result rows, including directory and file groups.
    pub const SEARCH_ROW: Rems = from_px(26.0);
    /// The line number at the left of a search hit: wide enough for four
    /// digits, so the lines under one file start at the same column.
    pub const SEARCH_LINE_NUMBER: Rems = from_px(36.0);
    /// The Output toolbar's command button at its widest. Section 6.20.
    pub const RUN_COMMAND: Rems = from_px(420.0);
    /// The popup a project's `+` opens: room for a branch name. M12.2.
    pub const NEW_WORKTREE: Rems = from_px(260.0);
    /// A menu group heading, scaled with its eyebrow text.
    pub const MENU_HEADING: Rems = from_px(24.0);
    /// A row of a menu — 32 since M12.2; it was 28, and before that a
    /// tree's.
    pub const MENU_ROW: Rems = from_px(32.0);
    /// The narrowest a menu is; it grows to what it holds. M12.2.
    pub const MENU_MIN: Rems = from_px(180.0);
    /// Above the first row of a menu and below the last. M12.2.
    pub const MENU_PADDING: Rems = from_px(5.0);
    /// From a menu's edge to the start of an item's text. M12.2.
    pub const MENU_INSET: Rems = from_px(14.0);
    /// An item's icon, when the menu has icons. M12.2.
    pub const MENU_ICON: Rems = from_px(16.0);
    /// Between an item's icon and its text. M12.2.
    pub const MENU_ICON_GAP: Rems = from_px(6.0);
    /// Above and below a menu's separator. M12.2.
    pub const MENU_RULE: Rems = from_px(6.0);
    /// A one-line picker row: Recent Projects, Recent Files, the run
    /// list, the fonts, Search Everywhere and Find in Files' hits all
    /// draw at this height. M12.7 task 9 — the pickers take the menu's
    /// card and two row heights, and this is the shorter of the two; the
    /// branch popup keeps [`BRANCH_ROW`], and Recent Locations, whose
    /// rows wrap, keeps its own.
    pub const PICKER_ROW: Rems = from_px(28.0);
    /// The refs on a commit's row, at most. They are the least of what a
    /// row says; the summary is the most, and it had been losing.
    pub const LOG_REFS: Rems = from_px(180.0);
    pub const LOG_DETAIL: Rems = from_px(320.0);
    /// A button's vertical padding. Section 6.7.
    pub const BUTTON_PADDING: Rems = from_px(9.0);
    /// The commit box's button: a minimum size of 72×28, and 10
    /// either side of its label. Section 6.7, 2026-09-18.
    pub const BUTTON: Rems = from_px(28.0);
    pub const BUTTON_MIN: Rems = from_px(72.0);
    pub const BUTTON_SIDE: Rems = from_px(10.0);
    /// A checkbox, and the tick inside it. Section 6.12.
    pub const CHECKBOX: Rems = from_px(14.0);
    pub const CHECK: Rems = from_px(10.0);
    /// The hit area around it.
    pub const HEAD_BUTTON: Rems = from_px(24.0);
    /// One of a diff row's two line-number columns: four characters of
    /// `mono` 12.5 and a gap. Section 6.10.
    pub const DIFF_NUMBERS: Rems = from_px(40.0);
    /// Curved old/new block transition lane, section 6.10.
    pub const DIFF_CONNECTOR: Rems = from_px(24.0);
    /// Compact worktree hover target from the same-scale recording (40 physical pixels at 2x).
    pub const WORKTREE_ACTION: Rems = from_px(20.0);
    /// Worktree action glyph canvas, measured separately from its hit target.
    pub const WORKTREE_GLYPH: Rems = from_px(12.0);
    /// Worktree hint line, independent of the sidebar's inherited line height.
    pub const WORKTREE_HINT_LINE: Rems = from_px(16.0);
    /// Keep the anchored worktree hint offset independent of compact footer gaps.
    pub const WORKTREE_HINT_GAP: Rems = from_px(4.0);
    /// A tool's button in the status bar, the icon in it, the gap between
    /// buttons, and the gap between the docks' groups. Section 6.11.
    pub const STATUS_BUTTON: Rems = from_px(24.0);
    pub const STATUS_ICON: Rems = from_px(16.0);
    pub const STATUS_BUTTON_GAP: Rems = from_px(2.0);
    /// Keeps 24-point targets inside the unchanged 28-point footer.
    pub const STATUS_BOTTOM: Rems = from_px(4.0);
    pub const STATUS_GROUP_GAP: Rems = from_px(12.0);
    /// Trade label air for larger targets without changing the footer's width budget.
    pub const FOOTER_LABEL_GAP: Rems = from_px(5.0);
    pub const FOOTER_SOURCE_PADDING: Rems = from_px(3.0);
    /// What a language server is doing, in the status bar: the widest its
    /// detail may run before it is cut, and the bar beside it. Section
    /// 6.21, M8.5.1.
    pub const SERVER_DETAIL: Rems = from_px(240.0);
    pub const PROGRESS_WIDTH: Rems = from_px(48.0);
    pub const PROGRESS_HEIGHT: Rems = from_px(3.0);
    /// The list of a project's servers: its width. Section 6.21.
    /// Compact recovery list, bounded to the window by its host.
    pub const TERMINAL_JOBS_WIDTH: Rems = from_px(400.0);
    pub const TERMINAL_JOBS_HEIGHT: Rems = from_px(420.0);
    pub const SERVER_LIST: Rems = from_px(360.0);
    /// The Recent Files popup: its width, how far below the title bar it
    /// hangs, and how tall it may grow. Section 6.25.
    ///
    /// Narrower than the branch popup and much narrower than Recent
    /// Projects: a row here is a file name and the directory above it,
    /// not a path from the root, and a popup wider than its longest row
    /// is a hole in the middle of the window.
    pub const RECENT_FILES: Rems = from_px(520.0);
    pub const RECENT_FILES_TOP: Rems = from_px(120.0);
    pub const RECENT_FILES_LIST: Rems = from_px(420.0);
    /// The Go to Line field. A line number and a colon and a column: it
    /// holds `12000:120` with room to spare and nothing else ever goes
    /// in it. Section 6.25.
    pub const GOTO_LINE: Rems = from_px(240.0);
    /// Search Everywhere: how wide the card opens, and how tall its list
    /// may grow. Sections 5 and 6.33, handed out by M8.9's seams commit.
    ///
    /// Wider than Recent Files: a row names a symbol, its container and
    /// the file and line it is at, and the head holds five tabs and a
    /// checkbox on one line.
    pub const EVERYWHERE: Rems = from_px(680.0);
    pub const EVERYWHERE_LIST: Rems = from_px(440.0);
    /// Find in Files: the card's width and its list's height. Wider still
    /// and shorter, because a preview stands under the hits and a hit is a
    /// line of code. Sections 5 and 6.34.
    pub const FIND_IN_FILES: Rems = from_px(760.0);
    pub const FIND_IN_FILES_LIST: Rems = from_px(280.0);
    /// The code around a lit row, under either popup's list. Section 5.
    pub const PREVIEW: Rems = from_px(220.0);
    /// The title bar's quiet controls: the branch button and the Update
    /// pill — one height, so the two words sit alike in the row. Section
    /// 6.9, since the pill was toned on 2026-09-24.
    pub const TITLE_CHIP: Rems = from_px(18.0);
    /// The Update pill's inset either side of its word — section 6.9.
    pub const PILL_PADDING: Rems = from_px(7.0);
    /// The usage chip's inset either side — the branch button's 6.
    /// Section 6.9, M12.10.
    pub const CHIP_PADDING: Rems = from_px(6.0);
    /// A status disc: the Problems button's, the sessions counter's and
    /// the usage chip's, 7 across. Section 6.11.
    pub const DISC: Rems = from_px(7.0);
    /// Physical viewport margin for the usage card, independent of text size.
    pub const USAGE_WINDOW_MARGIN_PX: f32 = 8.0;
    /// Usage bars follow the measured five-point rounded reference.
    pub const USAGE_BAR_HEIGHT: Rems = from_px(5.0);
    /// The usage card's width — the language servers' list's. Section 5.
    pub const USAGE_CARD: Rems = from_px(360.0);
    /// The column *All providers* says a reset in — `3d 11h` at `meta`
    /// fits. Section 5, M12.12.
    pub const USAGE_RESET_COLUMN: Rems = from_px(56.0);
    /// How wide the card over the Update pill is — a width, not a limit,
    /// since a line wraps only against a width it has. Section 5, M5.16.
    pub const NOTES_CARD: Rems = from_px(360.0);
    /// *What's new*: the card's width and its list's height. Section 5.
    pub const WHATS_NEW: Rems = from_px(460.0);
    pub const WHATS_NEW_LIST: Rems = from_px(440.0);
    /// The start screen's column, centred in the work's card and clipped
    /// by it rather than wrapped. Sections 5 and 6.35, M14.5.
    pub const START_COLUMN: Rems = from_px(560.0);
    /// The start screen's default card: its height, and the disc its
    /// harness's mark stands on. Sections 5 and 6.35.
    pub const START_CARD: Rems = from_px(60.0);
    pub const START_DISC: Rems = from_px(32.0);
    /// A tile of the start screen's *All tools*. Sections 5 and 6.35.
    pub const START_TILE: Rems = from_px(84.0);
    /// The start screen's parts stand this far apart: the card, *All
    /// tools*, *Recent* and the foot. Section 6.35.
    pub const START_GAP: Rems = from_px(24.0);
    /// The default harness's mark on its disc. Section 6.35.
    pub const START_MARK: Rems = from_px(20.0);
    /// A tile's mark — a size above the card's, the tile being the
    /// larger square. Section 6.35.
    pub const START_TILE_MARK: Rems = from_px(22.0);
    /// The cog beside the foot's *Settings*. Section 6.35.
    pub const START_COG: Rems = from_px(13.0);
    /// How far in from a pane's edges the focused pane's outline is drawn
    /// — M12.8, decision 19; section 5's *Pane outline*.
    pub const PANE_INSET: Rems = from_px(2.0);
    /// The height from which a place's start screen draws *Recent* —
    /// M12.8, decision 20; section 5's *Start screen, compact*. Under it
    /// the screen is the heading, the default card and the tiles.
    pub const START_RECENT_FROM: Rems = from_px(440.0);
    /// The height from which it draws its foot as well. Section 5.
    pub const START_FOOT_FROM: Rems = from_px(520.0);
}

/// Shared opacity tokens.
pub mod alpha {
    /// Muted whole-identifier occurrences, under Find and selection.
    pub const IDENTIFIER_OCCURRENCE: f32 = 0.12;
    /// Git status ink mixed over a file tree row, section 2.7.
    pub const GIT_ROW: f32 = 0.12;
    /// Primary ink over a file-list row under the pointer, its Git tint
    /// included — decision 54. Below [`SELECTION_INACTIVE`], so on an
    /// untinted row the two greys stay apart; a tinted row keeps its hue.
    pub const ROW_HOVER: f32 = 0.09;
    /// Primary ink over the ground: a file list's selection while the keys
    /// are elsewhere — decision 54.
    pub const SELECTION_INACTIVE: f32 = 0.14;
    const _: () = assert!(ROW_HOVER < SELECTION_INACTIVE, "the hover stays softer");
    /// Source diff replacement emphasis, section 6.10.
    pub const DIFF_INTRALINE: f32 = 0.24;
    /// Source diff selection is painted below glyphs, section 6.10.
    pub const DIFF_SELECTION: f32 = 0.35;
    /// Connections between old/new changed ranges, section 6.10.
    pub const DIFF_CONNECTOR: f32 = 0.30;
    /// Worktree neutral action hover: primary ink over the row ground.
    pub const WORKTREE_HOVER: f32 = 0.16;
    /// The column's selection — the selected worktree's card and the tab
    /// in front under it, one fill for both:
    /// `accent.patina` over the panel, barely there. The card's outline
    /// carries its selection; the hover, louder, stays apart from it.
    pub const COLUMN_SELECTED: f32 = 0.05;
    /// Worktree Delete hover: rust over the row, balanced with neutral controls.
    pub const WORKTREE_DESTRUCTIVE_HOVER: f32 = 0.36;
    /// Markdown paints selection over glyphs; keep their contrast (design system).
    pub const MARKDOWN_SELECTION: f32 = 0.25;
    /// The start screen's lit default card: its one-point outline in
    /// `accent.brass` at this alpha — section 6.35. And the focused
    /// pane's, in `accent.patina`, while its tab has more than one —
    /// M12.8, decision 19.
    pub const LIT_OUTLINE: f32 = 0.38;
}

/// Neutral interaction fill, using the established worktree-hover strength.
/// Keep palette surfaces unchanged: callers supply the control's actual ground.
pub fn control_hover(theme: Theme, ground: u32) -> Rgba {
    control_hover_over(theme, gpui::rgb(ground))
}

/// [`control_hover`] over a ground that may itself be a fill — a control
/// inside a row the pointer is already lighting. The same blend over the
/// panel would be the row's own colour, and the control would not light.
pub fn control_hover_over(theme: Theme, ground: Rgba) -> Rgba {
    ground.blend(gpui::rgb(theme.text.primary).opacity(alpha::WORKTREE_HOVER))
}

/// A control over a dock row the pointer is lighting — a terminal's or a
/// run's tab: [`control_hover_over`] the panel's own [`control_hover`].
/// Penpot's `color.state.control_hover_lit`.
pub fn control_hover_lit(theme: Theme) -> Rgba {
    control_hover_over(theme, control_hover(theme, theme.surface.panel))
}

/// The hover behind a tab's × in the Terminal tool and in Run: over the
/// raised tab in front, the control hover over `raised`; over a tab at
/// rest, which the pointer lights too, [`control_hover_lit`].
pub fn close_hover(theme: Theme, selected: bool) -> Rgba {
    if selected {
        control_hover(theme, theme.surface.raised)
    } else {
        control_hover_lit(theme)
    }
}

/// The column's selection fill: the selected worktree's card and the tab
/// in front under it — [`alpha::COLUMN_SELECTED`] of `accent.patina` over
/// the panel.
pub fn column_selected(theme: Theme) -> Rgba {
    gpui::rgb(theme.surface.panel)
        .blend(gpui::rgb(theme.accent.patina).opacity(alpha::COLUMN_SELECTED))
}

/// WCAG's floor for text under 18 points: what a word must reach against
/// what it is drawn on.
pub const LEGIBLE: f32 = 4.5;

/// WCAG's contrast ratio of two colours, from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f32 {
    let luminance = |colour: u32| {
        let channel = |shift: u32| {
            let c = ((colour >> shift) & 0xff) as f32 / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Text on the shared Update titlebar control, legible on both surfaces.
pub fn update_ink(theme: &Theme) -> u32 {
    if [
        theme.surface.card,
        theme.surface.raised,
        u32::from(control_hover(*theme, theme.surface.card)) >> 8,
    ]
    .into_iter()
    .all(|fill| contrast(theme.accent.brass, fill) >= LEGIBLE)
    {
        theme.accent.brass
    } else {
        theme.text.primary
    }
}

/// The text on a primary button, section 6.7: one ink on brass in every
/// palette, which is why it is not a palette's field.
pub const ON_BRASS: u32 = 0x141414;

/// Corner radii.
pub mod radius {
    /// A tool's button in the status bar. Section 6.11.
    pub const STATUS_BUTTON: Rems = from_px(6.0);
    use gpui::Rems;

    use super::from_px;

    /// Compact worktree action hover corners.
    pub const WORKTREE_ACTION: Rems = from_px(4.0);
    /// Worktree row hover corners from the same-scale reference.
    pub const WORKTREE_ROW: Rems = from_px(4.0);
    /// Tree and list rows.
    pub const ROW: Rems = from_px(8.0);
    /// A checkbox. Section 6.12.
    pub const CHECKBOX: Rems = from_px(3.0);
    /// Cards, buttons, badges.
    pub const CARD: Rems = from_px(14.0);
    /// The application window.
    pub const WINDOW: Rems = from_px(11.0);
    /// The work's card — section 4.1, M12.6.
    pub const WORK: Rems = from_px(14.0);
    /// The focused pane's outline — M12.8, decision 19; section 5's
    /// *Pane outline*.
    pub const PANE: Rems = from_px(6.0);
}

/// The lengths the terminal measures in pixels rather than rems.
///
/// Two reasons, and either alone would be enough. The terminal's chrome is
/// measured against the terminal's own font, not the interface's, so it must
/// not move when the interface's size changes — that is what having two font
/// settings exists to prevent. And these numbers reach arithmetic against a
/// mouse position, where a rem means nothing without a window to resolve it
/// against, and `cell_at` has no window.
pub mod terminal {
    /// Horizontal breathing room, independent of the interface font.
    pub const PADDING_X: f32 = 13.0;
    /// Compact vertical inset: at 16-point leading this returns one row
    /// compared with the former 13-point inset at both ends.
    pub const PADDING_Y: f32 = 5.0;

    /// The scrollbar's width and its inset from the right edge.
    pub const SCROLLBAR: f32 = 5.0;
}

/// Font families.
pub mod font {
    /// The interface when nothing is chosen — the system's face since
    /// M12.11, and the name is here so that it does not spread across
    /// widgets.
    pub const UI: &str = crate::fonts::DEFAULT.ui;
    /// Code and paths when nothing is chosen — Lilex since M12.11, which
    /// `desktop::fonts` registers before the window opens.
    pub const MONO: &str = crate::fonts::DEFAULT.code;
}

/// Dim a colour by mixing it with the background.
///
/// The terminal needs it: `DIM` (SGR 2) is not a separate colour but a
/// request to make the current one dimmer. Without it `claude`'s hints
/// come out at full brightness, indistinguishable from typed text — this
/// is where the milestone started.
///
/// The share is how much of the original colour remains: 0.0 is entirely
/// the background, 1.0 is the colour untouched.
pub fn blend(color: Hsla, background: Hsla, keep: f32) -> Hsla {
    let keep = keep.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| b + (a - b) * keep;

    // Mixing in RGB rather than HSL: in HSL hues mix around the circle
    // and produce an unrelated colour — a dim red drifts towards green by
    // way of yellow. In RGB the dim version stays the same colour, only
    // closer to the background.
    let a = Rgba::from(color);
    let b = Rgba::from(background);

    Rgba {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: a.a,
    }
    .into()
}

/// How dim text under the `DIM` flag should be.
///
/// The value comes from terminal practice: a half is already
/// indistinguishable, three quarters is imperceptible. Fifty-five
/// hundredths reads as "a hint" while staying legible.
pub const DIM_KEEP: f32 = 0.55;

/// One-shot feedback for a completed tab drag; design system §6.5.
pub mod motion {
    use std::time::Duration;
    /// Cubic ease-out from the drag preview to the final slot.
    pub const TAB_SETTLE: Duration = Duration::from_millis(180);
    /// The landing tint fades after the tab has settled.
    pub const TAB_FEEDBACK: Duration = Duration::from_millis(360);
    /// The initial patina tint, distinct from the active tab underline.
    pub const TAB_TINT: f32 = 0.16;
}

/// Compact one-line control hints; rich information cards retain their own layout.
pub mod tooltip {
    use gpui::{Rems, rems};
    const fn from_px(px: f32) -> Rems {
        rems(px / crate::scale::BASE)
    }
    pub const HEIGHT: Rems = from_px(28.0);
    pub const INSET: Rems = from_px(8.0);
    pub const GAP: Rems = from_px(12.0);
    /// Approximate initial delay, measured from a reference recording.
    pub const SHOW_DELAY: std::time::Duration = std::time::Duration::from_millis(250);
}

/// The supplied worktree deletion reference, measured at 2× capture scale.
pub mod worktree_confirmation {
    use super::from_px;
    use gpui::Rems;
    pub const WIDTH: Rems = from_px(400.);
    pub const WINDOW_INSET: Rems = from_px(8.);
    pub const PADDING: Rems = from_px(20.);
    pub const CONTEXT: Rems = from_px(40.);
    /// Description offset retains the existing confirmation card geometry.
    pub const TITLE_GAP: Rems = from_px(4.);
    pub const LINE: Rems = from_px(18.);
    pub const BUTTON: Rems = from_px(34.);
    pub const GAP: Rems = from_px(16.);
    pub const DIM: f32 = 0.5;
    pub const HOVER: f32 = 0.9;
}
