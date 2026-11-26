//! Find in Files — the popup, with the Search tool's own engine. M8.9,
//! decisions 12 and 14.
//!
//! What can be said about the popup without a window: the three toggles'
//! strokes, the history of queries and masks, and where a search looks.
//! The popup's state is `root::find_in_files::FindPopup` — not `Find`,
//! which is M5.12's action (decision 18) — and its card is
//! `view::find_in_files`.

use gpui::Keystroke;

use crate::search::Toggle;

/// Which of the popup's two fields the letters go to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Field {
    /// What is being looked for.
    #[default]
    Text,
    /// Which files to look in.
    Mask,
}

/// The toggle a stroke turns: `alt-c` *Match Case*, `alt-w` *Words*, `alt-x`
/// *Regex* — the common strokes, whose buttons underline those letters.
///
/// **By the key on the cap, with `alt` and nothing else.** `alt-c` types
/// `ç` on a Latin layout and something else on a Russian one, so the
/// character is no answer; and a stroke with another modifier is somebody
/// else's shortcut.
pub fn toggle(stroke: &Keystroke) -> Option<Toggle> {
    let held = &stroke.modifiers;
    if !held.alt || held.platform || held.control || held.shift || held.function {
        return None;
    }
    match stroke.key.as_str() {
        "c" => Some(Toggle::MatchCase),
        "w" => Some(Toggle::WholeWord),
        "x" => Some(Toggle::Regex),
        _ => None,
    }
}

/// How many queries a project remembers.
pub const QUERIES: usize = 20;

/// How many masks a project remembers.
pub const MASKS: usize = 10;

/// Put an entry at the head of a history: moved there when it is already
/// in it, never doubled, and the oldest dropped past `cap`.
///
/// **An empty entry is not remembered** — nothing searched is not a
/// search, and an empty mask is every file.
pub fn remember(list: &mut Vec<String>, entry: &str, cap: usize) {
    if entry.is_empty() {
        return;
    }
    list.retain(|kept| kept != entry);
    list.insert(0, entry.to_owned());
    list.truncate(cap);
}

/// The query a walk is asked: the one in the fields, less its mask while
/// the mask is turned off. **The mask stays in the field** — it is only not
/// applied.
pub fn query_for(query: &crate::search::Query, mask_off: bool) -> crate::search::Query {
    let mut asked = query.clone();
    if mask_off {
        asked.mask.clear();
    }
    asked
}

/// What the Search tool writes for the mask, given what the project
/// remembers (`mask`, `off`) and what the tool's field holds.
///
/// The tool has no checkbox: it is handed the mask **as applied**, so a
/// mask the popup turned off reaches it empty. **An empty field over a mask
/// that is off keeps that mask, still off** — writing the empty field would
/// empty the popup's the next time it opens. A mask in the tool's field is
/// a mask the tool applies, so it is on.
pub fn tool_mask(mask: &str, off: bool, typed: &str) -> (String, bool) {
    if typed.is_empty() && off {
        (mask.to_owned(), true)
    } else {
        (typed.to_owned(), false)
    }
}

/// A walk back through a history from a field — `alt-up` older, `alt-down`
/// newer — that ends on what was being typed when it began.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recall {
    /// The entry on show, newest first; `None` before the first `alt-up`
    /// and after coming back past the newest.
    at: Option<usize>,
    /// The line as it was before the walk began.
    typing: Option<String>,
}

impl Recall {
    /// One entry older: the newest, the first time, keeping `typing` to
    /// come back to. `None` at the oldest, or with no history.
    pub fn older(&mut self, list: &[String], typing: &str) -> Option<String> {
        let next = self.at.map_or(0, |at| at + 1);
        let entry = list.get(next)?;
        if self.at.is_none() {
            self.typing = Some(typing.to_owned());
        }
        self.at = Some(next);
        Some(entry.clone())
    }

    /// One entry newer, and past the newest what was being typed. `None`
    /// when the walk has not begun.
    pub fn newer(&mut self, list: &[String]) -> Option<String> {
        match self.at? {
            0 => {
                self.at = None;
                Some(self.typing.take().unwrap_or_default())
            }
            at => {
                let at = (at - 1).min(list.len().saturating_sub(1));
                self.at = Some(at);
                list.get(at).cloned()
            }
        }
    }
}

/// Where a search looks — decision 16: the project, or one directory of it.
/// **Not remembered**, for M8.7's reason: a scope is a gesture aimed at a
/// row, and a window that came back searching inside a directory nobody
/// had pointed at would answer a question nobody asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Scope {
    /// The whole project.
    #[default]
    Project,
    /// One directory, relative to the project — never the empty path.
    Directory(std::path::PathBuf),
}

impl Scope {
    /// The directory, as `search::Query::scope` carries it: `None` for the
    /// project.
    pub fn relative(&self) -> Option<&std::path::Path> {
        match self {
            Self::Project => None,
            Self::Directory(path) => Some(path),
        }
    }
}

/// The scope an Explorer row gives: a directory is itself, a file is its
/// directory — and **the empty path is the project**, which is the project's
/// own row and the directory of a file at its top.
pub fn scope_of_row(path: &std::path::Path, is_dir: bool) -> Scope {
    let directory = if is_dir {
        path
    } else {
        path.parent().unwrap_or(std::path::Path::new(""))
    };
    if directory.as_os_str().is_empty() {
        Scope::Project
    } else {
        Scope::Directory(directory.to_path_buf())
    }
}

/// The scope a folder chosen in the platform's chooser gives, or `None`
/// when it is not inside the project.
///
/// **Both made canonical first**: the chooser answers the real path, a
/// project may have been opened through a link, and every temporary
/// directory here is `/var` by its link and `/private/var` by its real
/// name. A folder that is not there is not inside anything.
pub fn scope_of_chosen(root: &std::path::Path, chosen: &std::path::Path) -> Option<Scope> {
    let root = root.canonicalize().ok()?;
    let chosen = chosen.canonicalize().ok()?;
    let relative = chosen.strip_prefix(&root).ok()?;
    Some(scope_of_row(relative, true))
}
