//! What closing a tab would interrupt, and the one sheet that asks about
//! it — M5.12, task 1. Pure: the window reads the facts, this decides.
//!
//! **Closing a tab ends its process, and the window asks first only when
//! there is a turn to lose** — decision 1 of the pass. A terminal tab's
//! agent is at stake while it is working or waiting on the person *and*
//! its process is alive. A shell closes
//! without a word. **An idle or finished Claude Code tab asks too since
//! 2026-09-16**, by the person's request and behind a setting of its own —
//! M5.12 had it close silently, on the reasoning that a sheet on every
//! close is one people learn to dismiss unread; the person found losing a
//! tab to a stray `cmd-w` worse.

use guild_session::status::Status;

use crate::i18n::Ui;

/// What a closing would interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stake {
    /// The agent is in the middle of a turn.
    Working,
    /// The agent is stopped until the person answers.
    Waiting,
    /// An agent is open between turns — idle, or finished. Asked about
    /// only while `confirm_closing_agent_tab` is on: Claude Code's since
    /// 2026-09-16, every harness's since 2026-09-28.
    Open,
}

/// A terminal tab's stake, from its agent's status and whether its
/// process is still alive. A process that has ended has no turn to lose,
/// whatever the board last heard.
pub fn terminal_stake(status: Status, alive: bool) -> Option<Stake> {
    if !alive {
        return None;
    }
    match status {
        Status::Working => Some(Stake::Working),
        Status::Waiting(_) => Some(Stake::Waiting),
        Status::Idle | Status::Done | Status::Failed => None,
    }
}

/// Whether a terminal tab holds an agent that has not ended: by the kind
/// of session it was started as, or by the program a shell runs in front
/// of it — any of the three harnesses. The rule named Claude Code alone
/// until 2026-09-28, four days after Codex and opencode could run in a tab.
pub fn agent_open(
    runs: Option<guild_session::Runs>,
    in_front: Option<guild_session::HarnessKind>,
    exited: bool,
) -> bool {
    !exited && (runs.is_some_and(guild_session::Runs::is_agent) || in_front.is_some())
}

/// What closing a tab asks about: the turn at stake when there is one, and
/// otherwise — with `always` on — an open agent's tab.
///
/// `live` is [`terminal_stake`]'s answer, already
/// `None` when the person turned that question off; `agent_open` is
/// [`agent_open`]'s answer.
pub fn tab_stake(live: Option<Stake>, agent_open: bool, always: bool) -> Option<Stake> {
    live.or((always && agent_open).then_some(Stake::Open))
}

/// The most a closing would interrupt among `stakes` — a divided tab's,
/// from its panes' (M12.8): a turn in progress, then an agent waiting on
/// the person, then an agent open between turns. Closing the tab ends
/// every pane, so the sheet names the most that is lost, not the first
/// pane's.
pub fn strongest(stakes: impl IntoIterator<Item = Stake>) -> Option<Stake> {
    at_stake(stakes.into_iter().map(|stake| ((), Some(stake)))).map(|((), stake)| stake)
}

/// Which of a divided tab's panes its stake is, and that stake: the
/// strongest, as [`strongest`] ranks them, and **the first listed among
/// equals** — the window lists the focused pane first. What the close
/// sheet names, M12.8's second window check: a tab asked about for an
/// agent's turn is named after the agent, not after the shell beside it
/// that has the keys.
pub fn at_stake<T>(panes: impl IntoIterator<Item = (T, Option<Stake>)>) -> Option<(T, Stake)> {
    let rank = |stake: &Stake| match stake {
        Stake::Working => 2,
        Stake::Waiting => 1,
        Stake::Open => 0,
    };
    let mut best: Option<(T, Stake)> = None;
    for (pane, stake) in panes {
        let Some(stake) = stake else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, held)| rank(&stake) > rank(held))
        {
            best = Some((pane, stake));
        }
    }
    best
}

/// The sheet for the tabs at stake — **its title, and nothing under it** —
/// or `None` when nothing is.
///
/// One tab is a question with its name and what its agent is doing in it,
/// *Close “fix-review” mid-turn?*; several carry their count. Until
/// 2026-09-28 a detail named every tab again with its state and said the
/// conversation was kept; the person found the sheet long, and asked for a
/// detail only where something is lost for good — a conversation is not:
/// closing a tab keeps its record whatever `forget_sessions_on_close` says.
pub fn sheet(ui: Ui, staked: &[(String, Stake)]) -> Option<String> {
    match staked {
        [] => None,
        [(name, stake)] => Some(ui.close_tab_title(name, *stake)),
        several => Some(ui.close_tabs_title(several.len())),
    }
}

/// The tab that comes forward when `closing`, the one in front, goes:
/// its left neighbour, else its right one — the common IDE rule, 2026-09-23.
///
/// It was the strip's last tab, so `cmd-w` on the second of eight jumped
/// to the eighth and went on closing from the far end.
pub fn neighbour<T: Copy + Eq>(strip: &[T], closing: T) -> Option<T> {
    let at = strip.iter().position(|tab| *tab == closing)?;
    at.checked_sub(1)
        .and_then(|left| strip.get(left))
        .or_else(|| strip.get(at + 1))
        .copied()
}
