//! Search Everywhere — one popup with five tabs, and Go to File, Go to
//! Symbol and Find Action are that popup opened on a tab. M8.9, decision 6.
//!
//! What is pure about it is here: the tabs and their order, what a tab's
//! stroke means while the popup is already up, and how an action is found
//! by what it is called. The popup's state is
//! `root::everywhere::Everywhere`, and its card is `view::everywhere`.
//!
//! **Four popups would be four heads, four lists and four places to keep a
//! preview in step**; there is one.

use crate::actions::keymap::Named;
use crate::fuzzy::{self, Shape};

/// What the popup searches among — its five tabs, in the familiar order.
///
/// Named `Among` rather than `Tab` because `crate::Tab` is a tab of the
/// strip, and a file that reads both would read one of them wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Among {
    /// Files, symbols and actions together — a double Shift.
    #[default]
    All,
    /// A project's files by name — the Go to File menu/custom binding.
    Files,
    /// Functions, types and the rest — `cmd-alt-o`.
    Symbols,
    /// What the window can do — `cmd-shift-a`.
    Actions,
    /// The text inside the files — no stroke of its own; `cmd-shift-f` is
    /// the Find in Files popup.
    Text,
}

impl Among {
    /// Every tab, in the order Tab walks them.
    pub const ALL: [Among; 5] = [
        Among::All,
        Among::Files,
        Among::Symbols,
        Among::Actions,
        Among::Text,
    ];

    /// The tab Tab goes to — wrapping, as the arrows in a popup do.
    pub fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|among| *among == self)
            .unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// The tab Shift-Tab goes to.
    pub fn previous(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|among| *among == self)
            .unwrap_or(0);
        Self::ALL[(at + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// The tab an action opens the popup on, if it opens it at all.
    pub fn of(action: Named) -> Option<Among> {
        match action {
            Named::SearchEverywhere => Some(Among::All),
            Named::GoToFile => Some(Among::Files),
            Named::GoToSymbol => Some(Among::Symbols),
            Named::FindAction => Some(Among::Actions),
            _ => None,
        }
    }
}

/// What a tab's stroke asks for while the popup is already up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// Show that tab.
    Open(Among),
    /// *Include non-project items* — the dependencies too.
    Libraries,
    /// A double Shift over another tab: back to *All*.
    ToAll,
}

/// What pressing the stroke of `pressed` means with `showing` up.
///
/// **The tab's own stroke again reaches past the project** — the familiar
/// gesture, and the only keyboard route to the checkbox; any other tab's
/// stroke switches to it, and a double Shift over a tab that is not *All*
/// goes back to *All*.
pub fn again(showing: Among, pressed: Among) -> Asked {
    match (showing == pressed, pressed) {
        (true, _) => Asked::Libraries,
        (false, Among::All) => Asked::ToAll,
        (false, pressed) => Asked::Open(pressed),
    }
}

/// The actions `query` finds, best first, each with the characters of its
/// label that matched.
///
/// **By the label and by the keymap name**, the better of the two:
/// Settings → Keymap lists the second, so `ToggleSidebar` is a word a
/// person has read, and `togsid` should find it. The marks are the
/// label's — the label is what a row draws — and a row found by its name
/// alone is drawn with none. Nothing typed keeps the list's own order.
pub fn rank_actions(query: &str, named: &[(Named, String)]) -> Vec<(Named, Vec<u32>)> {
    let by_label = fuzzy::rank(query, named, |(_, label)| label.as_str(), Shape::Name);
    let by_name = fuzzy::rank(query, named, |(action, _)| action.name(), Shape::Name);

    let mut best: Vec<Option<(u32, Vec<u32>)>> = vec![None; named.len()];
    for (index, found) in by_label {
        best[index] = Some((found.score, found.marks));
    }
    for (index, found) in by_name {
        match &mut best[index] {
            Some((score, _)) if *score >= found.score => {}
            Some((score, _)) => *score = found.score,
            slot @ None => *slot = Some((found.score, Vec::new())),
        }
    }

    let mut found: Vec<(usize, u32, Vec<u32>)> = best
        .into_iter()
        .enumerate()
        .filter_map(|(index, found)| found.map(|(score, marks)| (index, score, marks)))
        .collect();
    // Stable, so a tie keeps the list's order; and nothing typed scores
    // every row zero, which is the list's order too.
    found.sort_by_key(|(_, score, _)| std::cmp::Reverse(*score));
    found
        .into_iter()
        .map(|(index, _, marks)| (named[index].0, marks))
        .collect()
}

/// How many rows a section of *All* shows before its *More* row.
///
/// A handful: *All* is for a query that could be any of the three,
/// and a section that fills the card buries the two below it.
pub const SECTION: usize = 5;

/// A row of *All*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// A section's title — never lit.
    Heading(Among),
    /// The row at this index of that tab's own list.
    Row(Among, usize),
    /// *N more*: Enter on it shows that tab.
    More(Among),
}

/// *All*, from how many rows each of its three lists holds: *Files*, then
/// *Symbols*, then *Actions*, each under its heading and at most
/// [`SECTION`] rows, with a *More* row under a section that holds more. A
/// section with no rows has no heading.
pub fn all(files: usize, symbols: usize, actions: usize) -> Vec<Slot> {
    let mut slots = Vec::new();
    for (among, count) in [
        (Among::Files, files),
        (Among::Symbols, symbols),
        (Among::Actions, actions),
    ] {
        if count == 0 {
            continue;
        }
        slots.push(Slot::Heading(among));
        slots.extend((0..count.min(SECTION)).map(|row| Slot::Row(among, row)));
        if count > SECTION {
            slots.push(Slot::More(among));
        }
    }
    slots
}

/// What a tab asks for, for what has been typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wanted {
    /// The file index's ranking — the recent files, for nothing typed.
    pub files: bool,
    /// The symbol index's.
    pub symbols: bool,
    /// The actions, ranked here.
    pub actions: bool,
    /// A walk of the text.
    pub text: bool,
}

/// What `among` asks for with `query` typed.
///
/// **Symbols and Text ask nothing until something is typed**: every symbol
/// of a project, or every line of it, is not what an empty field means. An
/// empty *Files* is the recent files (decision 10), and so is an empty
/// *All* — and nothing else, or the recent files would sit over every
/// action there is. *Actions* lists every action for nothing typed, as a
/// menu would.
pub fn wanted(among: Among, query: &str) -> Wanted {
    let typed = !query.trim().is_empty();
    let nothing = Wanted::default();
    match among {
        Among::All => Wanted {
            files: true,
            symbols: typed,
            actions: typed,
            text: false,
        },
        Among::Files => Wanted {
            files: true,
            ..nothing
        },
        Among::Symbols => Wanted {
            symbols: typed,
            ..nothing
        },
        Among::Actions => Wanted {
            actions: true,
            ..nothing
        },
        Among::Text => Wanted {
            text: typed,
            ..nothing
        },
    }
}

/// A file's name and the place typed after it: `history.rs:120:4` is
/// `history.rs` at line 120, column 4.
///
/// **Split at the first `:` after which `parse_target` reads the rest** —
/// Go to Line's own rule for what a place is — so a colon that belongs to
/// the name stays in it, and a query with no numbers is all name.
pub fn split_line(query: &str) -> (&str, Option<crate::root::history::Target>) {
    for (at, _) in query.match_indices(':') {
        if let Some(target) = crate::root::history::parse_target(&query[at + 1..]) {
            return (&query[..at], Some(target));
        }
    }
    (query, None)
}

/// Which emptiness an empty list is — section 6.33's *Nothing to show*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Empty {
    /// An index the tab reads has not landed its first walk.
    Indexing,
    /// The tab asks nothing until something is typed.
    TypeToSearch,
    /// The *Text* tab's walk is under way and has found nothing yet.
    Searching,
    /// Everything asked has answered, and nothing matched.
    NothingMatches,
}

/// What an empty list of `among` says: `typed` is whether anything but
/// spaces is in the field, `files_ready` and `symbols_ready` whether each
/// index has landed its first walk, and `walking` whether the *Text* tab's
/// walk is still running.
pub fn empty(
    among: Among,
    typed: bool,
    files_ready: bool,
    symbols_ready: bool,
    walking: bool,
) -> Empty {
    match among {
        Among::Files if !files_ready => Empty::Indexing,
        // Something typed in *All* asks both indexes, so either unready
        // is *Indexing…*.
        Among::All if typed && !(files_ready && symbols_ready) => Empty::Indexing,
        Among::Symbols if !typed => Empty::TypeToSearch,
        Among::Symbols if !symbols_ready => Empty::Indexing,
        Among::All | Among::Text if !typed => Empty::TypeToSearch,
        Among::Text if walking => Empty::Searching,
        _ => Empty::NothingMatches,
    }
}

/// The head above the list — the tabs and the field — for the height the
/// card first opens at. **In rems**, as the list's height is: the two are
/// added before the rem size multiplies them, and a head in pixels opened
/// the card 1720 high.
const HEAD: gpui::Rems = crate::theme::from_px(80.0);

/// The card's first size, in pixels at a rem of `rem` pixels: section
/// 6.33's width, and its list's height with the head above it.
pub fn first_size(rem: f32) -> (f32, f32) {
    (
        crate::theme::space::EVERYWHERE.0 * rem,
        (crate::theme::space::EVERYWHERE_LIST.0 + HEAD.0) * rem,
    )
}
