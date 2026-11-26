//! The Sessions mode's two menus: a row's, and the heading's filter.
//!
//! **A row's menu offers what the window already does to a tab** — bring
//! it forward, name it, close it — and Show in Projects; a closed
//! conversation offers to continue it. **The filter is three groups of
//! checks** over what the page lists, and Show All clears them.

use gpui::{Context, Window};

use super::{Choice, Deed, Entry, Label, Mark, Target};
use crate::agents::Agent;
use crate::i18n::Text;
use crate::session_list::{Filter, Key, Urgency};

/// What a session row's menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDeed {
    /// Bring its tab forward, or continue a closed one.
    Open,
    /// Name its tab.
    Rename,
    /// Show Projects with its worktree chosen.
    ShowInProjects,
    /// Close its tab.
    Close,
}

/// One check of the filter menu: an agent, a section, a project by its
/// place in the menu's own list, or all of them back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterPick {
    Agent(Agent),
    State(Urgency),
    Project(usize),
    All,
}

fn choice(label: Label, deed: Deed, check: Option<bool>) -> Entry {
    Entry::Choice(Choice {
        label,
        mark: Mark::None,
        deed,
        check,
        enabled: true,
        detail: None,
        trailing: None,
    })
}

/// A row's menu: an open session's four, a closed one's two.
pub fn row_entries(key: &Key) -> Vec<Entry> {
    match key {
        Key::Open(_) => vec![
            Entry::Item(Text::MenuOpen, Deed::Session(SessionDeed::Open)),
            Entry::Item(Text::MenuRenameTab, Deed::Session(SessionDeed::Rename)),
            Entry::Item(
                Text::SessionShowInProjects,
                Deed::Session(SessionDeed::ShowInProjects),
            ),
            Entry::Separator,
            Entry::Item(Text::MenuCloseTab, Deed::Session(SessionDeed::Close)),
        ],
        Key::Closed { .. } => vec![
            Entry::Item(Text::SessionContinue, Deed::Session(SessionDeed::Open)),
            Entry::Item(
                Text::SessionShowInProjects,
                Deed::Session(SessionDeed::ShowInProjects),
            ),
        ],
    }
}

/// The filter's checks: each agent, each section, each project the page
/// can list, ticked while shown — and Show All.
pub fn filter_entries(filter: &Filter, projects: &[(String, String)]) -> Vec<Entry> {
    let mut entries = vec![Entry::Heading(Label::Text(Text::SessionFilterAgent))];
    for agent in [Agent::ClaudeCode, Agent::Codex, Agent::OpenCode] {
        entries.push(choice(
            Label::Text(super::agents::kind_text(agent)),
            Deed::SessionFilter(FilterPick::Agent(agent)),
            Some(!filter.excludes_agent(agent)),
        ));
    }
    entries.push(Entry::Heading(Label::Text(Text::SessionFilterState)));
    for (state, text) in [
        (Urgency::Waiting, Text::SessionStateWaiting),
        (Urgency::Working, Text::SessionStateWorking),
        (Urgency::Done, Text::SessionStateDone),
        (Urgency::Idle, Text::SessionStateIdle),
    ] {
        entries.push(choice(
            Label::Text(text),
            Deed::SessionFilter(FilterPick::State(state)),
            Some(!filter.excludes_state(state)),
        ));
    }
    if !projects.is_empty() {
        entries.push(Entry::Heading(Label::Text(Text::SessionFilterProject)));
        for (index, (key, label)) in projects.iter().enumerate() {
            entries.push(choice(
                Label::Named(label.clone()),
                Deed::SessionFilter(FilterPick::Project(index)),
                Some(!filter.excludes_project(key)),
            ));
        }
    }
    entries.push(Entry::Separator);
    entries.push(Entry::Item(
        Text::MenuShowAll,
        Deed::SessionFilter(FilterPick::All),
    ));
    entries
}

impl crate::Root {
    /// The repositories the filter offers for `workspace`, each by its
    /// group key and its name — and its folder beside the name when two
    /// share it, so each can be told apart and unticked alone.
    pub(crate) fn session_filter_projects(
        &self,
        workspace: guild_session::WorkspaceId,
    ) -> Vec<(String, String)> {
        let groups = self.column_groups_for(workspace);
        groups
            .iter()
            .map(|group| {
                let shared = groups
                    .iter()
                    .filter(|other| other.name == group.name)
                    .count()
                    > 1;
                let label = if shared {
                    format!("{} · {}", group.name, group.key.display())
                } else {
                    group.name.clone()
                };
                (group.key.display().to_string(), label)
            })
            .collect()
    }

    pub(super) fn run_session_deed(
        &mut self,
        deed: SessionDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<crate::Root>,
    ) {
        let Target::SessionRow { key } = target else {
            return;
        };
        match deed {
            SessionDeed::Open => self.activate_session(key.clone(), window, cx),
            SessionDeed::ShowInProjects => self.show_session_in_projects(key.clone(), cx),
            SessionDeed::Rename => {
                if let Key::Open(id) = key
                    && let Some(tab) = self.tab_of_session(*id)
                {
                    self.go_to_session(*id, cx);
                    self.start_renaming_tab(tab, window, cx);
                }
            }
            SessionDeed::Close => {
                if let Key::Open(id) = key
                    && let Some(tab) = self.tab_of_session(*id)
                {
                    self.close_tab_asking(tab, window, cx);
                }
            }
        }
    }

    pub(super) fn run_filter_pick(
        &mut self,
        pick: FilterPick,
        target: &Target,
        cx: &mut Context<crate::Root>,
    ) {
        let Target::SessionFilter { projects, .. } = target else {
            return;
        };
        match pick {
            FilterPick::Agent(agent) => self.session_filter.toggle_agent(agent),
            FilterPick::State(state) => self.session_filter.toggle_state(state),
            FilterPick::Project(index) => {
                if let Some((key, _)) = projects.get(index) {
                    self.session_filter.toggle_project(key);
                }
            }
            FilterPick::All => self.session_filter.clear(),
        }
        self.session_cursor = None;
        cx.notify();
    }
}
