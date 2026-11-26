//! The Git tool's filter menus — *Branch*, *User* and *Date* — M12.7
//! task 6: menus of the engine instead of the two cards
//! `view::log_filters` used to roll by hand.
//!
//! **What the list shows is what was true at the click.** The button
//! gathers the branches, the authors and the chosen value into a
//! [`FilterList`] and the target carries it, the counter's own pattern
//! for a menu whose rows are the window's data. A deed names a row by its
//! place in that list rather than by the name — `Deed` is `Copy`, and a
//! branch is a `String` — so the window resolves it against the same
//! list the rows were built from, and the two cannot disagree.
//!
//! **One rule for every menu**: the row that takes the filter off first,
//! a rule under it, a check on the chosen value, and a footnote at
//! the foot — *Press ⌫ to clear the filter*, which `context_menu_key`
//! keeps as a promise for these three menus alone.

use super::{Choice, Deed, Entry, Label, Mark, Target};
use crate::Root;
use crate::git::Scope;
use crate::i18n::Text;

/// Which of the toolbar's dropdowns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMenu {
    /// Which branch is listed — *All*, *HEAD*, then the branches.
    Branch,
    /// Whose commits, from the authors of what is loaded.
    User,
    /// How far back.
    Date,
}

/// How far back the Date menu reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Since {
    Day,
    Week,
    Month,
}

impl Since {
    /// Every one, in the menu's order.
    pub const ALL: [Since; 3] = [Since::Day, Since::Week, Since::Month];

    /// What git is given: our spelling of *Last week*.
    pub fn arg(self) -> &'static str {
        match self {
            Since::Day => "1.day",
            Since::Week => "1.week",
            Since::Month => "1.month",
        }
    }

    /// The one git was given, read back.
    pub fn from_arg(arg: &str) -> Option<Since> {
        Since::ALL.into_iter().find(|since| since.arg() == arg)
    }

    /// Its words.
    pub fn text(self) -> Text {
        match self {
            Since::Day => Text::LogLastDay,
            Since::Week => Text::LogLastWeek,
            Since::Month => Text::LogLastMonth,
        }
    }
}

/// What one of the menus lists, gathered at the click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterList {
    Branch {
        /// What the log is listing now.
        chosen: Scope,
        /// The branch `HEAD` is on, which wears the star; `None` detached.
        head: Option<String>,
        /// The branch everybody comes back to, when it is `main` or
        /// `master`.
        default: Option<String>,
        /// The branches whose log was read lately, the default left out.
        recent: Vec<String>,
        /// Every local branch — behind *Local ▸*.
        local: Vec<String>,
    },
    User {
        /// The author narrowed by, if any.
        chosen: Option<String>,
        /// The authors of what is loaded.
        authors: Vec<String>,
    },
    Date {
        /// How far back, if narrowed.
        chosen: Option<Since>,
    },
}

impl FilterList {
    /// Which dropdown it is.
    pub fn kind(&self) -> FilterMenu {
        match self {
            FilterList::Branch { .. } => FilterMenu::Branch,
            FilterList::User { .. } => FilterMenu::User,
            FilterList::Date { .. } => FilterMenu::Date,
        }
    }

    /// Every branch row's name, in the order `FilterDeed::Branch` counts:
    /// the default, the recent ones, then the local ones. A name may come
    /// twice — `main` is the default and a local branch — and each row
    /// has a place of its own.
    fn branches(&self) -> Vec<&str> {
        match self {
            FilterList::Branch {
                default,
                recent,
                local,
                ..
            } => default
                .iter()
                .chain(recent)
                .chain(local)
                .map(String::as_str)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The branch a `FilterDeed::Branch` names.
    pub fn branch(&self, at: usize) -> Option<&str> {
        self.branches().get(at).copied()
    }

    /// The author a `FilterDeed::Author` names.
    pub fn author(&self, at: usize) -> Option<&str> {
        match self {
            FilterList::User { authors, .. } => authors.get(at).map(String::as_str),
            _ => None,
        }
    }
}

/// What a row of a filter menu does — what its click did on the old card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDeed {
    /// Take the filter off: every branch, anyone, any time. The first
    /// row, and Backspace.
    Clear,
    /// List the head of the branch checked out.
    Head,
    /// List one branch, by its place in [`FilterList::branch`].
    Branch(usize),
    /// Narrow to one author, by its place in [`FilterList::author`].
    Author(usize),
    /// Narrow to a date.
    Since(Since),
}

/// A row of a filter menu. Every one carries a check's column, because
/// every one of these menus has a current value.
fn row(label: Label, mark: Mark, deed: FilterDeed, chosen: bool) -> Entry {
    Entry::Choice(Choice {
        label,
        mark,
        deed: Deed::LogFilter(deed),
        check: Some(chosen),
        enabled: true,
        detail: None,
        trailing: None,
    })
}

/// The menu for `list`.
pub fn log_filter(list: &FilterList) -> Vec<Entry> {
    let mut entries = Vec::new();
    match list {
        FilterList::Branch {
            chosen,
            head,
            default,
            recent,
            local,
        } => {
            // The star the branch column uses for the branch `HEAD` is
            // on: one mark, one meaning, wherever a branch is named.
            let star = |name: &str| {
                if head.as_deref() == Some(name) {
                    Mark::Star
                } else {
                    Mark::None
                }
            };
            let picked = |name: &str| chosen.branch() == Some(name);
            // One counter across the default, the recent and the local
            // rows: the order `FilterList::branches` reads them back in.
            let mut at = 0;
            let mut named = |name: &String| {
                let entry = row(
                    Label::Named(name.clone()),
                    star(name),
                    FilterDeed::Branch(at),
                    picked(name),
                );
                at += 1;
                entry
            };

            entries.push(row(
                Label::Text(Text::LogAllBranches),
                Mark::None,
                FilterDeed::Clear,
                *chosen == Scope::All,
            ));
            entries.push(Entry::Separator);
            // `HEAD` is where the star is, whichever branch that is.
            entries.push(row(
                Label::Text(Text::LogHead),
                Mark::Star,
                FilterDeed::Head,
                *chosen == Scope::Head,
            ));
            entries.extend(default.iter().map(&mut named));
            if !recent.is_empty() {
                entries.push(Entry::Separator);
                entries.push(Entry::Heading(Label::Text(Text::LogRecentBranches)));
                entries.extend(recent.iter().map(&mut named));
            }
            entries.push(Entry::Separator);
            // **The branches go into a submenu.** A repository with forty
            // of them turns the menu into the list it was meant to open,
            // so they go behind *Local ▸*.
            let local: Vec<Entry> = local.iter().map(&mut named).collect();
            entries.push(Entry::Submenu(Text::LogLocalBranches, local));
        }
        FilterList::User { chosen, authors } => {
            entries.push(row(
                Label::Text(Text::LogAnyone),
                Mark::None,
                FilterDeed::Clear,
                chosen.is_none(),
            ));
            entries.push(Entry::Separator);
            entries.extend(authors.iter().enumerate().map(|(at, author)| {
                row(
                    Label::Named(author.clone()),
                    Mark::None,
                    FilterDeed::Author(at),
                    chosen.as_ref() == Some(author),
                )
            }));
        }
        FilterList::Date { chosen } => {
            entries.push(row(
                Label::Text(Text::LogAnyTime),
                Mark::None,
                FilterDeed::Clear,
                chosen.is_none(),
            ));
            entries.push(Entry::Separator);
            entries.extend(Since::ALL.into_iter().map(|since| {
                row(
                    Label::Text(since.text()),
                    Mark::None,
                    FilterDeed::Since(since),
                    *chosen == Some(since),
                )
            }));
        }
    }
    // The footnote, **always there**: a footer that
    // comes and goes is a menu that changes height as it is used.
    entries.push(Entry::Note(Label::Text(Text::LogClearHint)));
    entries
}

impl Root {
    /// What the button of `which` offers now, gathered for its menu.
    pub(crate) fn filter_list(&self, which: FilterMenu) -> FilterList {
        let filter = self.log_filter();
        match which {
            FilterMenu::Branch => {
                let branches = self.branches_here();
                // `branch_list::ordered` has already put the default first,
                // so it is simply the first name there.
                let default = branches
                    .first()
                    .map(|branch| branch.name.clone())
                    .filter(|name| ["main", "master"].contains(&name.as_str()));
                FilterList::Branch {
                    chosen: self.log_scope(),
                    head: branches
                        .iter()
                        .find(|branch| branch.current)
                        .map(|branch| branch.name.clone()),
                    recent: self
                        .recent_branches()
                        .into_iter()
                        .filter(|name| !["main", "master"].contains(&name.as_str()))
                        .collect(),
                    local: branches.iter().map(|branch| branch.name.clone()).collect(),
                    default,
                }
            }
            FilterMenu::User => FilterList::User {
                chosen: filter.author,
                authors: self.log_authors(),
            },
            FilterMenu::Date => FilterList::Date {
                chosen: filter.since.as_deref().and_then(Since::from_arg),
            },
        }
    }

    /// Take `which` filter off — the first row's deed, and Backspace's.
    pub(crate) fn clear_log_filter(&mut self, which: FilterMenu, cx: &mut gpui::Context<Self>) {
        match which {
            FilterMenu::Branch => self.show_branch_log(Scope::All, cx),
            FilterMenu::User => self.filter_log_author(None, cx),
            FilterMenu::Date => self.filter_log_since(None, cx),
        }
    }

    /// Carry out a row of a filter menu: what its click did on the card
    /// it replaced — a scope, an author or a date, and git asked again.
    pub(crate) fn run_filter_deed(
        &mut self,
        deed: FilterDeed,
        target: &Target,
        cx: &mut gpui::Context<Self>,
    ) {
        let Target::LogFilter(list) = target else {
            return;
        };
        match deed {
            FilterDeed::Clear => self.clear_log_filter(list.kind(), cx),
            FilterDeed::Head => self.show_branch_log(Scope::Head, cx),
            FilterDeed::Branch(at) => {
                if let Some(name) = list.branch(at) {
                    self.show_branch_log(Scope::Named(name.to_owned()), cx);
                }
            }
            FilterDeed::Author(at) => {
                if let Some(author) = list.author(at) {
                    self.filter_log_author(Some(author.to_owned()), cx);
                }
            }
            FilterDeed::Since(since) => self.filter_log_since(Some(since.arg().to_owned()), cx),
        }
    }
}
