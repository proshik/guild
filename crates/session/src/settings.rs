//! Application settings.
//!
//! Only what a person chooses themselves and what must survive a restart
//! lives here. Session state is in [`crate::desk`]: different things
//! with different lifetimes, not worth confusing even for the sake of one
//! file.

use serde::{Deserialize, Serialize};

use crate::store::Normalise;

/// What the tab strip's **+** opens by default, and in what order the
/// choice offers it — M14.
///
/// A harness _and_ a terminal share this one setting, because the button is
/// one place and what it opens is the immediate question. `Terminal` is
/// deliberately a variant: a hands-on person's **+** may want a shell as
/// simply as a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultTab {
    /// Claude Code.
    Claude,
    /// OpenCode.
    OpenCode,
    /// Codex.
    Codex,
    /// A plain terminal.
    Terminal,
}

impl DefaultTab {
    /// Every tab the **+** can open, in the order the choice lists them.
    pub const ALL: [Self; 4] = [Self::Claude, Self::OpenCode, Self::Codex, Self::Terminal];

    /// The default nobody chose — GLD-081: the first of Claude Code,
    /// Codex and OpenCode in `available`, else a terminal, which needs
    /// nothing on the `PATH`. Its own order, not [`Self::ALL`]'s: the
    /// tiles' order is a layout, this one is a preference.
    pub fn first_installed(available: &[crate::HarnessKind]) -> Self {
        use crate::HarnessKind;
        [
            (HarnessKind::Claude, Self::Claude),
            (HarnessKind::Codex, Self::Codex),
            (HarnessKind::OpenCode, Self::OpenCode),
        ]
        .into_iter()
        .find(|(kind, _)| available.contains(kind))
        .map_or(Self::Terminal, |(_, tab)| tab)
    }

    /// The tab after this one, wrapping around — what a setting button that
    /// cycles through the choices lands on.
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Independent per-harness launch strings: trusted prefixes or literal argument text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchPrefixes {
    /// Claude Code's prefix.
    pub claude: String,
    /// Codex's prefix.
    pub codex: String,
    /// OpenCode's prefix.
    pub opencode: String,
}
impl LaunchPrefixes {
    /// The independently configured value for this harness.
    pub fn get(&self, harness: crate::HarnessKind) -> &str {
        match harness {
            crate::HarnessKind::Claude => &self.claude,
            crate::HarnessKind::Codex => &self.codex,
            crate::HarnessKind::OpenCode => &self.opencode,
        }
    }
    /// Edit one harness without changing the others.
    pub fn set(&mut self, harness: crate::HarnessKind, value: String) {
        *match harness {
            crate::HarnessKind::Claude => &mut self.claude,
            crate::HarnessKind::Codex => &mut self.codex,
            crate::HarnessKind::OpenCode => &mut self.opencode,
        } = value;
    }
}

/// What happened during a branch update, independently of the UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchUpdateKind {
    /// No commit changed.
    Unchanged,
    /// HEAD advanced without a merge commit.
    FastForward,
    /// A new two-parent commit was created.
    MergeCommit,
    /// Git retained conflicts for manual recovery.
    Conflict,
    /// Fetch failed before any merge.
    FetchRefused,
    /// A guard refused the update.
    Refused,
}

/// A bounded, language-neutral record of an update in one worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchUpdateReceipt {
    /// Canonical originating worktree.
    pub root: std::path::PathBuf,
    /// Commit before the operation.
    pub before: String,
    /// Commit after the operation, when readable.
    pub after: Option<String>,
    /// Outcome and therefore the recovery route.
    pub kind: BranchUpdateKind,
    /// Short diagnostic excerpt; full output stays in Console.
    pub diagnostic: String,
}
impl BranchUpdateReceipt {
    /// Whether the update requires attention.
    pub fn failed(&self) -> bool {
        matches!(
            self.kind,
            BranchUpdateKind::Conflict | BranchUpdateKind::Refused | BranchUpdateKind::FetchRefused
        )
    }
}

/// Read old localized strings without breaking existing settings files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BranchUpdateRecord {
    /// Current records are formatted in the active language at render time.
    Current(BranchUpdateReceipt),
    /// Legacy guidance is retained until the next meaningful update.
    Legacy(String),
}

/// The titlebar's selected external Open In destination.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "bundle_id", rename_all = "snake_case")]
pub enum OpenInTarget {
    /// The system file browser.
    #[default]
    Finder,
    /// Legacy trial choice, migrated to an available supported external terminal.
    Terminal,
    /// A configured external application: bundle identifier or stable custom row key.
    App(String),
}

/// Persisted preferences shared by Guild windows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Configured external application order, visibility and custom connections.
    pub open_in_apps: crate::open_in::Config,
    /// Selected Open In destination, shared across projects.
    pub open_in_target: OpenInTarget,
    /// Global proxy preferences, defaulting to current environment behavior.
    #[serde(deserialize_with = "crate::network::deserialize")]
    pub network: crate::network::Network,
    /// Saved Action Bar identifiers; unknown controls are retained for newer builds.
    pub action_bar_order: Vec<String>,
    /// Explicitly excluded Action Bar IDs; unknown IDs survive ordinary edits.
    pub action_bar_excluded: Vec<String>,
    /// Global supported-service defaults; absent ids are enabled.
    pub lsp_defaults: std::collections::BTreeMap<String, bool>,
    /// Shared preferences by canonical local repository identity.
    pub repository_settings: std::collections::BTreeMap<
        std::path::PathBuf,
        crate::repository_settings::RepositorySettings,
    >,
    /// Last update identity and manual recovery guidance, per worktree.
    pub branch_update_receipts: std::collections::BTreeMap<std::path::PathBuf, BranchUpdateRecord>,
    /// Branch-update source, keyed by the canonical worktree path.
    pub branch_update_sources: std::collections::BTreeMap<std::path::PathBuf, String>,
    /// Which tab the strip's **+** opens by default, when the person has
    /// chosen one — M14, GLD-081.
    ///
    /// The harness's own mark stands on the button, and a click opens that
    /// tab directly; the arrow beside it still offers the whole choice. A
    /// terminal is a row of the choice, so it can be the default too.
    ///
    /// **`None` is "not chosen", and it is not written down**: a fresh file
    /// then keeps following what is installed
    /// ([`Settings::default_tab_among`]), while a file that names a default
    /// keeps it even when its agent goes missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tab: Option<DefaultTab>,
    /// New-tab menu order; missing entries retain their historical order.
    #[serde(default)]
    pub tab_launcher_order: Vec<DefaultTab>,
    /// Wrappers used only by future interactive harness sessions.
    #[serde(default)]
    pub launch_prefixes: LaunchPrefixes,
    /// Quoted argument words inserted after each harness program, before Guild arguments.
    #[serde(default)]
    pub launch_arguments: LaunchPrefixes,
    /// Limit newly launched Codex processes to the workspace-write sandbox.
    pub codex_sandbox: bool,
    /// Let newly launched Codex processes request command approval.
    pub codex_command_approvals: bool,

    /// What a session may do without asking.
    ///
    /// The CLI shows approval dialogues itself; this decides which actions
    /// it asks about when starting a new session.
    pub permission_mode: PermissionMode,

    /// Copy the selection immediately, with no separate action.
    ///
    /// A habit from other terminals. A setting rather than a fixed
    /// behaviour: those it annoys, it annoys a lot — every selection
    /// overwrites the clipboard, including one made just to read.
    pub copy_on_select: bool,

    /// Opt in to new native development workflows; existing runs remain usable.
    pub development_workflows: bool,

    /// Experimental high-refresh terminal output; Current remains the default.
    pub adaptive_terminal_refresh: bool,

    /// Interface language.
    pub language: Language,

    /// The column of projects and their worktrees is folded to its strip
    /// — M12.
    ///
    /// A window's preference rather than a project's: it is about how
    /// much room the work gets, which does not change with what is being
    /// worked on. `default` so that a settings file written before M12
    /// still reads, and so that the column opens unfolded.
    #[serde(default)]
    pub column_folded: bool,

    /// How wide the column is, in points at the default scale.
    ///
    /// A window's choice like the folding beside it, and for the same
    /// reason: how much room the work gets does not change with what is
    /// being worked on. `default` so that a file written before it existed
    /// still reads, and reads as the design system's own 232.
    #[serde(default = "column_width")]
    pub column_width: f32,

    /// What the column lists: the workspace's projects or its agent
    /// sessions by urgency. The window's preference, like the folding
    /// beside it; a file written before reads as Projects.
    #[serde(default)]
    pub column_mode: ColumnMode,

    /// After how many days without activity an open agent tab, and within
    /// how many days a closed session, goes under the Sessions mode's
    /// Settled section. A file written before reads as three.
    #[serde(default = "settle_after_days")]
    pub settle_after_days: u32,

    /// Which of the Sessions mode's status sections are collapsed into the
    /// stack at the column's foot, and whether Settled is open. The
    /// window's, like the mode: one choice for every workspace, kept across
    /// a restart. A file written before reads as everything expanded and
    /// Settled closed.
    #[serde(default)]
    pub session_folds: SessionFolds,

    /// Which docks are open, how big, and which tool each shows — M12.2.
    ///
    /// **The window's, not a project's.** Until M12.2 they were kept in each
    /// project's `state.json`, so switching projects rebuilt the window
    /// around a person who had only asked to look at other code; every IDE
    /// keeps them for the window. `None` in a file written before, and until
    /// [`Settings::adopt_panels`] has taken a layout from the project in
    /// front — so a layout somebody chose is kept rather than reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panels: Option<crate::Panels>,

    /// Where the window was and how big it was.
    ///
    /// Part of the settings rather than of session state: the window is a
    /// property of the person's workspace, and it must survive even a full
    /// clearing of the session list.
    pub window: Option<WindowPlace>,

    /// The size, in cells, the window's tab terminals were last fitted to.
    ///
    /// Beside `window` for its reason: a property of how the window was
    /// left. A tab is spawned before it is drawn — every tab restored behind
    /// another is — and `claude` prints its first screen at whatever size
    /// the pty says, so a tab started at a made-up 80 × 24 drew its header
    /// narrow and stretched only when it was first shown. `None` until a
    /// tab has been drawn once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_grid: Option<TabGrid>,

    /// Where the Push Commits window was left — M8.3.6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_window: Option<WindowPlace>,

    /// Where the diff window was left — M8.3.6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_window: Option<WindowPlace>,

    /// Push Commits' two splits, as the share their first side takes:
    /// the commits beside the files, and the files over the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_splits: Option<(f32, f32)>,

    /// The Find tool's split, as the share its list takes beside the
    /// preview. M8.15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find_split: Option<f32>,

    /// Where the Recent Projects popup was left, and how big.
    ///
    /// Beside [`Self::window`] and for the same reason: a size a person
    /// chose once and would have to choose again on every launch is a size
    /// not worth choosing.
    pub picker: crate::picker::PickerPlace,

    /// Where the Recent Files popup was dragged to — M12.3. Only its place
    /// is a person's: its size is the design system's, so `width` and
    /// `height` are set from it before a drag and mean nothing on disk.
    #[serde(default)]
    pub recent_files: crate::picker::PickerPlace,

    /// Where the Search Everywhere popup was left, and how big — M8.9. A
    /// person resizes it, so its size is theirs, as Recent Projects' is.
    #[serde(default)]
    pub everywhere: crate::picker::PickerPlace,

    /// Where the Find in Files popup was left, and how big — M8.9.
    #[serde(default)]
    pub find_in_files: crate::picker::PickerPlace,

    /// Whether Search Everywhere shows the code around the lit row under
    /// its list — M8.9. Off until a person turns it on: the popup is for a
    /// name, and a preview is a second thing to read.
    #[serde(default)]
    pub everywhere_preview: bool,

    /// Terminal font size.
    ///
    /// A setting because it depends on eyes and screens: what is
    /// comfortable on a laptop is unreadable on an external monitor, and
    /// the other way round.
    pub terminal_font_size: f32,

    /// How large the interface is drawn.
    ///
    /// Everything except the terminal: the tab strip, the project row, the
    /// title bar, the menus and the settings themselves. The terminal keeps
    /// a size of its own — choosing a comfortable interface is not choosing
    /// a comfortable code font, and on a large display the two answers
    /// differ.
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: f32,

    /// The terminal's font, when a person has chosen one.
    ///
    /// `None` means the face that ships for it, Lilex Nerd Font since
    /// M12.11 — a Nerd Font, without which a modern shell prompt is a row
    /// of boxes. Until M12.11 it also named the face of code; that is
    /// [`Self::code_font`] now.
    ///
    /// A *name*, not a position in any list we offer. A settings file
    /// travels between machines, and a name that is not installed falls
    /// back to what ships rather than to a blank terminal.
    #[serde(default)]
    pub terminal_font: Option<String>,

    /// The face of code — the editor, a diff, Annotate — when a person has
    /// chosen one; `None` is Lilex. M12.11, a *Monospace font* of its own:
    /// a face for a file is not a face for a prompt, and one setting
    /// for both made Lilex Duo, which has no Nerd glyphs, impossible to
    /// offer.
    #[serde(default)]
    pub code_font: Option<String>,

    /// Whether [`Self::code_font`] is its own setting in this file — false
    /// on a file written before M12.11, where [`Self::terminal_font`] named
    /// both faces and is carried into code once by `normalise`.
    ///
    /// **A flag rather than an absent key**: `None` is also a choice, the
    /// default face, and a file that says so must not have the terminal's
    /// face copied over it at the next start.
    #[serde(default)]
    pub code_font_own: bool,

    /// The interface's font, on the same terms.
    #[serde(default)]
    pub ui_font: Option<String>,

    /// The `theme` value a file written before `two-themes-customize`
    /// held — `guild`, `cloud`, `slate` and the others — **kept exactly as
    /// read and never written by this build**. It is the migration's input
    /// when [`Self::theme_mode`] is absent, and it is what an older build
    /// reading the same file goes on drawing: writing `"cloud"` there would
    /// make the file unreadable to a build older than Cloud, and an
    /// unreadable settings file is moved aside whole.
    #[serde(
        rename = "theme",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_string"
    )]
    pub legacy_theme: Option<String>,

    /// System, Light or Dark. `None` only in a file from before the mode
    /// existed, which [`Self::theme_mode`] answers from
    /// [`Self::legacy_theme`] and `normalise` writes down. A field-level
    /// default, so an absent key is `None` while a fresh install's
    /// [`Settings::default`] is `Some(System)`; and lenient, so a value a
    /// newer build wrote reads as System instead of failing the file.
    #[serde(default, deserialize_with = "lenient_mode")]
    pub theme_mode: Option<ThemeMode>,

    /// The person's tints of the two palettes — choices only; the colours
    /// are always computed from the built-in palette. Lenient: a malformed
    /// entry is dropped alone.
    #[serde(deserialize_with = "lenient_custom")]
    pub theme_custom: ThemeCustom,

    /// Open the project you were last in when Guild starts.
    ///
    /// On by default: coming back to an empty window and hunting for the
    /// project you were in is not the same as coming back to work.
    #[serde(default = "yes")]
    pub reopen_last_project: bool,

    /// Ask before closing a tab with an agent in it — Claude Code, Codex or
    /// opencode — whatever it is doing. Asked for on 2026-09-16 for Claude
    /// Code, and for every harness on 2026-09-28; on by default.
    ///
    /// Beside `confirm_closing_live_project` rather than folded into it:
    /// that one is about a turn that would be lost, in a tab, a project or
    /// its removal; this one is about the tab alone, and an agent between
    /// turns. **Read under its old key too**, `confirm_closing_claude_tab`:
    /// a person who turned it off is not asked again for a rename.
    #[serde(default = "yes", alias = "confirm_closing_claude_tab")]
    pub confirm_closing_agent_tab: bool,

    /// Ask before closing a project that has a session still running.
    ///
    /// "Running" means a session that is working or waiting, not merely
    /// that a record exists. A prompt that appears every time teaches
    /// people to dismiss it without reading, and then it is not there when
    /// it matters.
    #[serde(default = "yes")]
    pub confirm_closing_live_project: bool,

    /// Throw a project's sessions away when it is closed.
    ///
    /// Off by default, which is the behaviour a person expects and did not
    /// have until M5.5: a project reopened comes back with its
    /// conversations. On, closing a project also clears its `.guild/`.
    #[serde(default)]
    pub forget_sessions_on_close: bool,

    /// Keep the default branch — the one new worktrees start from — up to
    /// date: fetch its upstream when a repository opens and every five
    /// minutes, and fast-forward it when it is clean and behind. On by
    /// default; off, no background
    /// fetch runs at all. Decision 21.
    #[serde(default = "yes")]
    pub auto_fast_forward_default_branch: bool,

    /// Show each worktree's GitLab merge request in the column, read with
    /// the person's glab login — GLD-063. On by default; off, Guild starts
    /// no glab at all and the rows and the status bar are as without it.
    #[serde(default = "yes")]
    pub gitlab_merge_requests: bool,

    /// Write a file as it is edited: 300 ms after the last key, when the
    /// editor loses the focus, before a commit, on closing. On by
    /// default; off, a file waits for `cmd-s`
    /// and a dirty tab asks before it closes. M8.3.
    #[serde(default = "yes")]
    pub autosave: bool,

    /// Initial presentation for newly opened Markdown panes in every project.
    /// Explicit saved pane modes take precedence during restoration.
    pub markdown_default_view: crate::markdown::Mode,
    /// How a diff is drawn: two columns by default, one on request.
    /// Decision 11 of M8.3.
    #[serde(default)]
    pub diff_view: DiffView,
}

/// The default for a switch that should start on.
fn yes() -> bool {
    true
}

/// The interface size the design system was drawn at.
///
/// Changing this number redraws the whole interface for everyone who never
/// touched the setting, which is the point of it being one number.
fn default_ui_font_size() -> f32 {
    13.0
}

/// A terminal's size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGrid {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
}

/// Where the window was.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowPlace {
    /// Offset from the left.
    pub x: f32,
    /// Offset from the top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl WindowPlace {
    /// Is the remembered place usable?
    ///
    /// A monitor may have been unplugged, a resolution changed. A window
    /// restored off-screen looks like an application that failed to start:
    /// there is an icon in the Dock but no window.
    pub fn is_usable(&self) -> bool {
        const MIN: f32 = 400.0;

        self.width >= MIN
            && self.height >= MIN
            && self.width.is_finite()
            && self.height.is_finite()
            && self.x.is_finite()
            && self.y.is_finite()
    }
}

/// Font size limits.
///
/// Below seven the text is unreadable; above forty nothing meaningful fits
/// in the window. The limit is needed because the size is changed with a
/// keyboard shortcut: a held `Cmd+-` would otherwise drive it to zero, and
/// there would be nothing left to bring it back with — the interface is no
/// longer visible.
pub const MIN_FONT_SIZE: f32 = 7.0;
/// Upper font size limit.
pub const MAX_FONT_SIZE: f32 = 40.0;

/// Lower interface size limit.
///
/// Narrower than the terminal's range at both ends, and for a different
/// reason: the terminal shows one kind of thing and can be squinted at,
/// while the interface has to lay out a tab strip and a settings form. Too
/// small and the labels stop being hit targets; too large and the strip
/// becomes one tab.
pub const MIN_UI_FONT_SIZE: f32 = 10.0;
/// Upper interface size limit.
pub const MAX_UI_FONT_SIZE: f32 = 20.0;

/// Which palette the interface draws with.
///
/// Here rather than in the desktop for `Language`'s reason: it is a setting
/// value. The colours themselves belong to the desktop — they are drawing,
/// not configuration — but *which* of them is a person's answer, and
/// answers live in the settings file.
///
/// **Two since `two-themes-customize`**: Light is what Cloud was and Dark
/// is what Midnight (`islands`) was, value for value; the other six were
/// removed and a file naming one opens Dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeKind {
    /// The light palette — Cloud's values.
    Light,
    /// The dark palette — Midnight's values.
    #[default]
    Dark,
}

impl ThemeKind {
    /// Both palettes, light first.
    pub const ALL: &'static [ThemeKind] = &[ThemeKind::Light, ThemeKind::Dark];

    /// How the palette names itself — an identity, not interface text:
    /// the settings page says it in the person's language.
    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Light => "Light",
            ThemeKind::Dark => "Dark",
        }
    }

    /// Whether UI assets and editors should use their light variant.
    pub fn is_light(self) -> bool {
        matches!(self, Self::Light)
    }

    /// The other one — the customizer's *Copy from*.
    pub fn other(self) -> ThemeKind {
        match self {
            ThemeKind::Light => ThemeKind::Dark,
            ThemeKind::Dark => ThemeKind::Light,
        }
    }
}

/// Which palette the window takes: one of the two, or the system's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Light while macOS is light, Dark while it is dark — a fresh install's.
    #[default]
    System,
    /// Light whatever macOS says.
    Light,
    /// Dark whatever macOS says.
    Dark,
}

impl ThemeMode {
    /// Every mode, in the order the settings offer them.
    pub const ALL: &'static [ThemeMode] = &[ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    /// The palette this mode means while the system is dark or not.
    pub fn resolve(self, system_dark: bool) -> ThemeKind {
        match self {
            ThemeMode::Light => ThemeKind::Light,
            ThemeMode::Dark => ThemeKind::Dark,
            ThemeMode::System if system_dark => ThemeKind::Dark,
            ThemeMode::System => ThemeKind::Light,
        }
    }

    /// The mode a file from before the mode existed meant: the two light
    /// palettes open Light, everything else — the default Guild included,
    /// and a value nobody knows — opens Dark.
    pub fn migrated(legacy: Option<&str>) -> ThemeMode {
        match legacy {
            Some("cloud" | "light") => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }
}

/// One tint: the chosen colour and how far towards it, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    /// `0xrrggbb`; on disk `"#rrggbb"`.
    pub color: u32,
    /// 0 is the built-in palette exactly, 100 as far as the rule goes.
    pub intensity: u8,
}

/// The upper end of [`Tint::intensity`].
pub const MAX_INTENSITY: u8 = 100;

impl Tint {
    /// Read `#rgb`, `#rrggbb`, with or without the `#`, in any case.
    pub fn parse_color(text: &str) -> Option<u32> {
        let hex = text.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            6 => u32::from_str_radix(hex, 16).ok(),
            3 => {
                let short = u32::from_str_radix(hex, 16).ok()?;
                let (r, g, b) = ((short >> 8) & 0xf, (short >> 4) & 0xf, short & 0xf);
                Some(((r * 0x11) << 16) | ((g * 0x11) << 8) | (b * 0x11))
            }
            _ => None,
        }
    }

    /// `#rrggbb`, lower case.
    pub fn hex(color: u32) -> String {
        format!("#{:06x}", color & 0xff_ffff)
    }

    /// A tint read off a JSON value; `None` when its colour is not one.
    /// An intensity out of range is clamped rather than dropped — the
    /// colour is still the person's.
    fn from_value(value: &serde_json::Value) -> Option<Tint> {
        let color = Tint::parse_color(value.get("color")?.as_str()?)?;
        let intensity = value
            .get("intensity")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, f64::from(MAX_INTENSITY))
            .round() as u8;
        Some(Tint { color, intensity })
    }
}

impl Serialize for Tint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;
        let mut tint = serializer.serialize_struct("Tint", 2)?;
        tint.serialize_field("color", &Tint::hex(self.color))?;
        tint.serialize_field("intensity", &self.intensity)?;
        tint.end()
    }
}

/// A palette's three tints; `None` is the built-in look for that tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Customization {
    /// Every surface towards a hue, and text a little.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Tint>,
    /// `accent.patina` and what is drawn from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Tint>,
    /// Selection and match grounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Tint>,
}

impl Customization {
    /// Nothing chosen: the built-in palette.
    pub fn is_empty(&self) -> bool {
        *self == Customization::default()
    }

    fn from_value(value: &serde_json::Value) -> Customization {
        let tint = |key| value.get(key).and_then(Tint::from_value);
        Customization {
            background: tint("background"),
            accent: tint("accent"),
            highlight: tint("highlight"),
        }
    }
}

/// Both palettes' customizations — one pair for the whole application.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ThemeCustom {
    /// Light's.
    pub light: Customization,
    /// Dark's.
    pub dark: Customization,
}

impl ThemeCustom {
    /// The customization of one palette.
    pub fn of(&self, kind: ThemeKind) -> &Customization {
        match kind {
            ThemeKind::Light => &self.light,
            ThemeKind::Dark => &self.dark,
        }
    }

    /// The same, to change.
    pub fn of_mut(&mut self, kind: ThemeKind) -> &mut Customization {
        match kind {
            ThemeKind::Light => &mut self.light,
            ThemeKind::Dark => &mut self.dark,
        }
    }
}

/// Theme data never fails the settings file: an unknown or misshapen value
/// is read as nothing, because a file that fails to parse is moved aside
/// whole and every other setting with it (decision 21 of M5.11).
fn lenient_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_str().map(str::to_owned))
}

/// See [`lenient_string`]: a mode this build does not know is System.
fn lenient_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<ThemeMode>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::Null => None,
        other => Some(serde_json::from_value(other).unwrap_or_default()),
    })
}

/// See [`lenient_string`]: a misshapen tint is dropped alone.
fn lenient_custom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ThemeCustom, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let of = |key| {
        value
            .get(key)
            .map(Customization::from_value)
            .unwrap_or_default()
    };
    Ok(ThemeCustom {
        light: of("light"),
        dark: of("dark"),
    })
}
/// Interface language.
///
/// Here rather than in the desktop because it is a setting value: it is
/// read and written together with the rest. The translations themselves
/// live in the interface — this is only the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// English — the default.
    #[default]
    English,
    /// Russian.
    Russian,
}

impl Language {
    /// Every language — for cycling through in the settings.
    pub const ALL: &'static [Language] = &[Language::English, Language::Russian];

    /// How the language names itself.
    ///
    /// The name in the language itself, not translated into the current
    /// one: the list is read by someone looking for their own language,
    /// and they recognise it in its native spelling.
    pub fn endonym(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Russian => "Русский",
        }
    }

    /// The next language, cycling round.
    ///
    /// Cycling rather than stopping at the end: otherwise the last
    /// language becomes a trap — you pick it and cannot switch back,
    /// because the interface is already in a language you cannot read.
    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// Permission policy.
///
/// The values are the vocabulary of the `--permission-mode` flag from
/// `claude --help` version 2.1.251. They go to the command line as they
/// are, so renaming them "for tidiness" is not an option: it would drift
/// silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Ask through the harness CLI.
    Manual,
    /// File edits without asking, everything else as usual.
    AcceptEdits,
    /// `claude` decides for itself.
    Auto,
    /// Plan without doing.
    Plan,
    /// Do not ask.
    DontAsk,
    /// Allow everything.
    BypassPermissions,
}

impl PermissionMode {
    /// The value for the `--permission-mode` flag.
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::Plan => "plan",
            Self::DontAsk => "dontAsk",
            Self::BypassPermissions => "bypassPermissions",
        }
    }
}

/// How a diff is drawn: two columns by default, or one. Decision 11 of
/// M8.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffView {
    /// The old text on the left, the new on the right, changed lines
    /// facing each other.
    #[default]
    SideBySide,
    /// One column, removed lines above added ones, as `git diff` prints.
    Unified,
}

impl DiffView {
    /// The other one.
    pub fn toggled(self) -> Self {
        match self {
            DiffView::SideBySide => DiffView::Unified,
            DiffView::Unified => DiffView::SideBySide,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open_in_apps: Default::default(),
            open_in_target: OpenInTarget::default(),
            network: Default::default(),
            action_bar_order: Vec::new(),
            action_bar_excluded: Vec::new(),
            lsp_defaults: Default::default(),
            repository_settings: Default::default(),
            branch_update_sources: Default::default(),
            branch_update_receipts: Default::default(),
            // Nobody has chosen: a new tab opens the first agent installed.
            default_tab: None,
            tab_launcher_order: Vec::new(),
            launch_prefixes: LaunchPrefixes::default(),
            launch_arguments: LaunchPrefixes::default(),
            codex_sandbox: true,
            codex_command_approvals: true,
            diff_view: DiffView::default(),
            // A strict default: asking and being refused is safer than
            // allowing everything silently. Loosening it is a deliberate
            // act.
            // **Auto, not manual** — asked for from the running window.
            // Manual was the cautious default from M5, when a session was
            // a thing a person watched; what they actually do is start a
            // session and let it work, and a window whose every new tab
            // opens saying *manual mode on* is one where the first act is
            // always the same keystroke. `claude` still asks about
            // anything its own settings say to ask about.
            permission_mode: PermissionMode::Auto,
            copy_on_select: true,
            adaptive_terminal_refresh: false,
            development_workflows: false,
            terminal_font_size: 12.5,
            ui_font_size: default_ui_font_size(),
            terminal_font: None,
            code_font: None,
            code_font_own: true,
            ui_font: None,
            legacy_theme: None,
            theme_mode: Some(ThemeMode::System),
            theme_custom: ThemeCustom::default(),
            reopen_last_project: true,
            auto_fast_forward_default_branch: true,
            gitlab_merge_requests: true,
            confirm_closing_live_project: true,
            confirm_closing_agent_tab: true,
            forget_sessions_on_close: false,
            autosave: true,
            markdown_default_view: crate::markdown::Mode::default(),
            language: Language::default(),
            // The column opens: a person who has never folded it should
            // see what it holds. M12.
            column_folded: false,
            column_width: column_width(),
            column_mode: ColumnMode::default(),
            settle_after_days: settle_after_days(),
            session_folds: SessionFolds::default(),
            panels: None,
            window: None,
            tab_grid: None,
            push_window: None,
            diff_window: None,
            push_splits: None,
            find_split: None,
            picker: crate::picker::PickerPlace::default(),
            recent_files: crate::picker::PickerPlace::default(),
            everywhere: crate::picker::PickerPlace::default(),
            find_in_files: crate::picker::PickerPlace::default(),
            everywhere_preview: false,
        }
    }
}

impl Settings {
    /// System, Light or Dark — a file from before the mode, migrated.
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
            .unwrap_or_else(|| ThemeMode::migrated(self.legacy_theme.as_deref()))
    }

    /// The palette in effect while the system is dark or not.
    pub fn theme_kind(&self, system_dark: bool) -> ThemeKind {
        self.theme_mode().resolve(system_dark)
    }

    /// The tab a new tab opens: the person's choice, or — until there is
    /// one — the first of `available` in [`DefaultTab::first_installed`]'s
    /// order. The one place every surface asks, so the start screen, the
    /// **+** and `cmd-t` cannot disagree.
    pub fn default_tab_among(&self, available: &[crate::HarnessKind]) -> DefaultTab {
        self.default_tab
            .unwrap_or_else(|| DefaultTab::first_installed(available))
    }

    /// Every supported launcher item exactly once, preserving the saved preference.
    pub fn tab_launcher_order(&self) -> Vec<DefaultTab> {
        let mut order = Vec::with_capacity(DefaultTab::ALL.len());
        for tab in self.tab_launcher_order.iter().chain(DefaultTab::ALL.iter()) {
            if !order.contains(tab) {
                order.push(*tab);
            }
        }
        order
    }
}

/// Fields are added with `serde(default)`, and an old file is read as it
/// is — with one migration, the code face's.
impl Normalise for Settings {
    fn normalise(&mut self) {
        // M12.11 split one face into two, and a face chosen before the
        // split was chosen for code as well.
        if !self.code_font_own {
            self.code_font = self.terminal_font.clone();
            self.code_font_own = true;
        }
        // The font size comes from a file edited by hand: zero or a
        // negative number would make the interface invisible, with nothing
        // left to bring it back.
        self.terminal_font_size = self.terminal_font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        // A file from before the mode existed: its palette says which of
        // the two it opens on, and the next save writes the answer down.
        self.theme_mode = Some(self.theme_mode());
        // The window's docks follow the rule a project's do — M8.3.6
        // retired Push, and the person's own settings showed it.
        if let Some(panels) = self.panels.as_mut() {
            panels.tidy();
        }
    }
}

/// Section 6.29's width, and what a dragged border comes back to.
impl Settings {
    /// Take the window's panels from a project's layout, the first time —
    /// and answer whether anything was taken, so the caller knows to save.
    ///
    /// Once the window has panels of its own no project's layout replaces
    /// them. With no project to take one from, the defaults are written down,
    /// so the question is asked once.
    pub fn adopt_panels(&mut self, from_project: Option<crate::Panels>) -> bool {
        if self.panels.is_some() {
            return false;
        }
        self.panels = Some(from_project.unwrap_or_default());
        true
    }
}

/// What the column lists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnMode {
    /// The workspace's projects and their worktrees.
    #[default]
    Projects,
    /// The workspace's agent sessions, by who needs the person first.
    Sessions,
}

impl ColumnMode {
    /// The other one — what the title bar's button and `Toggle Session
    /// List` switch to.
    pub fn toggled(self) -> Self {
        match self {
            Self::Projects => Self::Sessions,
            Self::Sessions => Self::Projects,
        }
    }
}

/// A status section of the Sessions mode, as the setting names it — the
/// desktop's urgency, without the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSection {
    /// Waiting for the person, or failed.
    Waiting,
    /// In the middle of a turn.
    Working,
    /// A finished turn nobody has read.
    Done,
    /// Nothing going on.
    Idle,
}

/// What the Sessions mode has folded: the status sections collapsed into
/// the stack above Settled, and whether Settled itself is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionFolds {
    /// The collapsed status sections. A name this build does not know —
    /// a newer build's — is dropped alone.
    #[serde(deserialize_with = "lenient_sections")]
    pub collapsed: std::collections::BTreeSet<SessionSection>,
    /// Whether Settled shows its rows.
    pub settled_open: bool,
}

impl SessionFolds {
    /// Collapse `section`, or expand it again.
    pub fn toggle(&mut self, section: SessionSection) {
        if !self.collapsed.remove(&section) {
            self.collapsed.insert(section);
        }
    }
}

/// See [`lenient_string`]: an unknown section name is dropped alone.
fn lenient_sections<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<std::collections::BTreeSet<SessionSection>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::Array(names) => names
            .into_iter()
            .filter_map(|name| serde_json::from_value(name).ok())
            .collect(),
        _ => Default::default(),
    })
}

/// The Settled threshold a person has not chosen: three days.
pub const SETTLE_AFTER_DAYS: u32 = 3;

fn settle_after_days() -> u32 {
    SETTLE_AFTER_DAYS
}

fn column_width() -> f32 {
    232.0
}

impl Settings {
    /// Keep the latest recoverable update; a later refusal/no-op must not
    /// erase the hashes needed to undo it. An initial refusal is still useful.
    pub fn remember_branch_update(&mut self, receipt: BranchUpdateReceipt) {
        let meaningful = |kind| {
            matches!(
                kind,
                BranchUpdateKind::FastForward
                    | BranchUpdateKind::MergeCommit
                    | BranchUpdateKind::Conflict
            )
        };
        let keep_previous = match self.branch_update_receipts.get(&receipt.root) {
            Some(BranchUpdateRecord::Current(previous)) => meaningful(previous.kind),
            // An old localized string can contain the only saved recovery hashes.
            Some(BranchUpdateRecord::Legacy(_)) => true,
            None => false,
        };
        if meaningful(receipt.kind) || !keep_previous {
            self.branch_update_receipts
                .insert(receipt.root.clone(), BranchUpdateRecord::Current(receipt));
        }
    }
}

#[cfg(test)]
mod adaptive_preference_tests {
    use super::*;
    #[test]
    fn codex_permission_defaults_ignore_obsolete_full_access() {
        for json in [
            "{}",
            r#"{"codex_full_access":true}"#,
            r#"{"codex_full_access":false}"#,
            r#"{"codex_sandbox":false}"#,
            r#"{"codex_command_approvals":false}"#,
        ] {
            let settings: Settings = serde_json::from_str(json).unwrap();
            assert_eq!(settings.codex_sandbox, !json.contains("sandbox\":false"));
            assert_eq!(
                settings.codex_command_approvals,
                !json.contains("approvals\":false")
            );
            let value = serde_json::to_value(&settings).unwrap();
            assert!(value.get("codex_full_access").is_none());
        }
        assert!(Settings::default().codex_sandbox);
        assert!(Settings::default().codex_command_approvals);
    }

    #[test]
    fn adaptive_terminal_preference_defaults_off_and_round_trips_without_environment() {
        assert!(!Settings::default().adaptive_terminal_refresh);
        let legacy: Settings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.adaptive_terminal_refresh);
        let saved: Settings =
            serde_json::from_str(r#"{"adaptive_terminal_refresh":true}"#).unwrap();
        assert!(saved.adaptive_terminal_refresh);
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert!(restored.adaptive_terminal_refresh);
    }
}

#[cfg(test)]
mod launch_prefix_tests {
    use super::*;
    #[test]
    fn launch_prefixes_are_independent_and_backward_compatible() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        let value = serde_json::to_value(&old).unwrap();
        assert_eq!(value["launch_prefixes"]["codex"], "");
        assert_eq!(value["launch_arguments"]["codex"], "");
        let configured: Settings =
            serde_json::from_str(r#"{"launch_prefixes":{"codex":"dp"}}"#).unwrap();
        let mut configured = configured;
        configured.launch_arguments.codex = "--model glm".into();
        configured.launch_arguments.claude = "--model 'another model'".into();
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&configured).unwrap()).unwrap();
        let value = serde_json::to_value(restored).unwrap();
        assert_eq!(value["launch_prefixes"]["codex"], "dp");
        assert_eq!(value["launch_prefixes"]["claude"], "");
        assert_eq!(value["launch_prefixes"]["opencode"], "");
        assert_eq!(value["launch_arguments"]["codex"], "--model glm");
        assert_eq!(
            value["launch_arguments"]["claude"],
            "--model 'another model'"
        );
        assert_eq!(value["launch_arguments"]["opencode"], "");
    }
}
