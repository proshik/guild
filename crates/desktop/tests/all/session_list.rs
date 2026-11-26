//! The Sessions mode's model: urgency sections, Settled, the popup's order,
//! the status bar's counts and the filter — one list, three surfaces.

use std::time::{Duration, SystemTime};

use guild_desktop::agents::Agent;
use guild_desktop::session_list::{
    Counts, Filter, Item, Key, Line, Page, SettledZone, Urgency, breakdown, counts, listing, page,
    popup_order,
};
use guild_session::SessionId;
use guild_session::settings::{SessionFolds, SessionSection};
use guild_session::status::{Status, Why};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000)
}

fn open(agent: Agent, status: Status, project: &str, ago: Option<Duration>) -> Item {
    Item {
        key: Key::Open(SessionId::new()),
        agent,
        status,
        project: project.into(),
        last: ago.map(|ago| now() - ago),
    }
}

fn closed(agent: Agent, project: &str, ago: Duration) -> Item {
    Item {
        key: Key::Closed {
            harness: agent.harness(),
            id: format!("past-{}", ago.as_secs()),
        },
        agent,
        status: Status::Idle,
        project: project.into(),
        last: Some(now() - ago),
    }
}

fn minutes(n: u64) -> Option<Duration> {
    Some(Duration::from_secs(n * 60))
}

#[test]
fn sections_go_by_urgency_and_newest_first_within_each() {
    let waiting = open(
        Agent::Codex,
        Status::Waiting(Why::Question),
        "atlas",
        minutes(9),
    );
    let failed = open(Agent::ClaudeCode, Status::Failed, "harbor", minutes(4));
    let old_work = open(Agent::OpenCode, Status::Working, "harbor", minutes(6));
    let new_work = open(Agent::ClaudeCode, Status::Working, "atlas", minutes(1));
    let done = open(Agent::ClaudeCode, Status::Done, "atlas", minutes(12));
    let idle = open(Agent::Codex, Status::Idle, "atlas", minutes(120));
    let unknown = open(Agent::Codex, Status::Idle, "atlas", None);
    let items = vec![
        idle.clone(),
        old_work.clone(),
        done.clone(),
        unknown.clone(),
        waiting.clone(),
        new_work.clone(),
        failed.clone(),
    ];

    let list = listing(items, Vec::new(), now(), 3 * DAY, &Filter::default());

    assert_eq!(
        list.sections,
        vec![
            (Urgency::Waiting, vec![failed.key, waiting.key]),
            (Urgency::Working, vec![new_work.key, old_work.key]),
            (Urgency::Done, vec![done.key]),
            // A session with no known time goes to the bottom, and never
            // settles: nothing says it is old.
            (Urgency::Idle, vec![idle.key, unknown.key]),
        ]
    );
    assert!(list.settled.is_empty());
}

#[test]
fn empty_sections_are_left_out() {
    let work = open(Agent::ClaudeCode, Status::Working, "atlas", minutes(1));
    let list = listing(
        vec![work.clone()],
        Vec::new(),
        now(),
        3 * DAY,
        &Filter::default(),
    );
    assert_eq!(list.sections, vec![(Urgency::Working, vec![work.key])]);
}

#[test]
fn an_old_idle_tab_and_a_recently_closed_session_are_settled() {
    let stale = open(Agent::Codex, Status::Idle, "atlas", Some(4 * DAY));
    let fresh = open(Agent::Codex, Status::Idle, "atlas", Some(2 * DAY));
    // Something that still asks for the person is never put away.
    let stale_waiting = open(
        Agent::ClaudeCode,
        Status::Waiting(Why::Permission),
        "atlas",
        Some(5 * DAY),
    );
    let yesterday = closed(Agent::ClaudeCode, "harbor", DAY);
    let last_week = closed(Agent::ClaudeCode, "harbor", 7 * DAY);

    let list = listing(
        vec![stale.clone(), fresh.clone(), stale_waiting.clone()],
        vec![last_week, yesterday.clone()],
        now(),
        3 * DAY,
        &Filter::default(),
    );

    assert_eq!(
        list.sections,
        vec![
            (Urgency::Waiting, vec![stale_waiting.key]),
            (Urgency::Idle, vec![fresh.key.clone()]),
        ]
    );
    assert_eq!(list.settled, vec![yesterday.key.clone(), stale.key.clone()]);

    // One day: the two-day-old tab settles too, and yesterday's closed
    // session is still within it.
    let list = listing(
        vec![stale.clone(), fresh.clone()],
        vec![yesterday.clone()],
        now() + Duration::from_secs(60),
        DAY,
        &Filter::default(),
    );
    assert!(list.sections.is_empty());
    assert_eq!(list.settled, vec![fresh.key, stale.key]);
}

#[test]
fn the_popup_lists_everything_by_urgency_without_settling() {
    let idle = open(Agent::Codex, Status::Idle, "atlas", Some(9 * DAY));
    let done = open(Agent::ClaudeCode, Status::Done, "atlas", minutes(12));
    let work = open(Agent::ClaudeCode, Status::Working, "atlas", minutes(1));
    let waiting = open(
        Agent::Codex,
        Status::Waiting(Why::Unstated),
        "atlas",
        minutes(30),
    );
    let failed = open(Agent::ClaudeCode, Status::Failed, "harbor", minutes(4));
    assert_eq!(
        popup_order(vec![
            idle.clone(),
            done.clone(),
            work.clone(),
            waiting.clone(),
            failed.clone()
        ]),
        vec![failed.key, waiting.key, work.key, done.key, idle.key]
    );
}

#[test]
fn counts_are_the_status_bar_s_and_the_breakdown_is_per_agent() {
    let statuses = [
        (Agent::ClaudeCode, Status::Working),
        (Agent::ClaudeCode, Status::Working),
        (Agent::ClaudeCode, Status::Waiting(Why::Question)),
        (Agent::Codex, Status::Working),
        (Agent::Codex, Status::Done),
        (Agent::Codex, Status::Idle),
    ];
    assert_eq!(
        counts(statuses.iter().map(|(_, status)| *status)),
        Counts {
            failed: 0,
            working: 3,
            waiting: 1,
            done: 1,
            idle: 1,
        }
    );
    assert_eq!(
        breakdown(statuses),
        vec![
            (
                Agent::ClaudeCode,
                Counts {
                    failed: 0,
                    working: 2,
                    waiting: 1,
                    done: 0,
                    idle: 0,
                }
            ),
            (
                Agent::Codex,
                Counts {
                    failed: 0,
                    working: 1,
                    waiting: 0,
                    done: 1,
                    idle: 1,
                }
            ),
        ]
    );
    assert!(Counts::default().is_empty());
    // Idle agents are counted too: only no agent at all is nothing.
    let all_idle = counts([Status::Idle, Status::Idle]);
    assert_eq!(
        all_idle,
        Counts {
            idle: 2,
            ..Counts::default()
        }
    );
    assert!(!all_idle.is_empty());
    assert_eq!(
        breakdown([(Agent::Codex, Status::Idle)]),
        vec![(
            Agent::Codex,
            Counts {
                idle: 1,
                ..Counts::default()
            }
        )]
    );
    assert_eq!(
        counts([Status::Failed]),
        Counts {
            failed: 1,
            ..Counts::default()
        }
    );
}

#[test]
fn the_filter_hides_agents_states_and_projects_until_cleared() {
    let claude = open(Agent::ClaudeCode, Status::Working, "atlas", minutes(1));
    let codex = open(Agent::Codex, Status::Working, "atlas", minutes(2));
    let harbor = open(Agent::ClaudeCode, Status::Done, "harbor", minutes(3));
    let past = closed(Agent::Codex, "atlas", DAY);
    let items = vec![claude.clone(), codex.clone(), harbor.clone()];

    let mut filter = Filter::default();
    assert!(filter.is_empty());
    filter.toggle_agent(Agent::Codex);
    let list = listing(items.clone(), vec![past.clone()], now(), 3 * DAY, &filter);
    assert_eq!(
        list.sections,
        vec![
            (Urgency::Working, vec![claude.key.clone()]),
            (Urgency::Done, vec![harbor.key.clone()]),
        ]
    );
    assert!(list.settled.is_empty(), "a closed Codex session is Codex's");

    filter.toggle_project("harbor");
    filter.toggle_state(Urgency::Working);
    let list = listing(items.clone(), vec![past.clone()], now(), 3 * DAY, &filter);
    assert!(list.sections.is_empty());
    assert!(!filter.admits(&claude));

    filter.toggle_agent(Agent::Codex);
    assert!(!filter.excludes_agent(Agent::Codex));
    filter.clear();
    assert!(filter.is_empty());
    let list = listing(items, vec![past.clone()], now(), 3 * DAY, &filter);
    assert_eq!(list.settled, vec![past.key]);
}

/// Four sections, two of them collapsed, and Settled: what the page draws
/// where. The open sections scroll; the collapsed ones are a stack of
/// headings pinned above Settled, in the same urgency order; Settled shows
/// its rows only while it is open.
fn four_sections_and_settled() -> (Vec<Item>, Vec<Item>) {
    let open_items = vec![
        open(
            Agent::Codex,
            Status::Waiting(Why::Question),
            "atlas",
            minutes(9),
        ),
        open(Agent::ClaudeCode, Status::Working, "atlas", minutes(1)),
        open(Agent::ClaudeCode, Status::Done, "atlas", minutes(12)),
        open(Agent::Codex, Status::Idle, "atlas", minutes(120)),
        open(Agent::Codex, Status::Idle, "atlas", Some(4 * DAY)),
    ];
    let closed_items = vec![closed(Agent::ClaudeCode, "harbor", DAY)];
    (open_items, closed_items)
}

#[test]
fn collapsed_sections_leave_the_list_for_the_stack_in_urgency_order() {
    let (open_items, closed_items) = four_sections_and_settled();
    let [waiting, _, done, _, _] = open_items.clone().map_keys();
    let list = listing(open_items, closed_items, now(), 3 * DAY, &Filter::default());
    // Collapsed out of order on purpose: the stack still goes by urgency.
    let folds = SessionFolds {
        collapsed: [SessionSection::Idle, SessionSection::Working]
            .into_iter()
            .collect(),
        settled_open: false,
    };

    let drawn = page(list, &folds);

    assert_eq!(
        drawn,
        Page {
            list: vec![
                Line::Section(Urgency::Waiting, 1),
                Line::Row(waiting.clone()),
                Line::Section(Urgency::Done, 1),
                Line::Row(done.clone()),
            ],
            collapsed: vec![(Urgency::Working, 1), (Urgency::Idle, 1)],
            settled: Some(SettledZone {
                count: 2,
                open: false,
                rows: Vec::new(),
            }),
        }
    );
    // The arrows walk only what is drawn: no collapsed section's rows, and
    // nothing of a closed Settled.
    assert_eq!(drawn.rows(), vec![waiting, done]);
}

#[test]
fn an_open_settled_lists_its_rows_after_the_open_sections() {
    let (open_items, closed_items) = four_sections_and_settled();
    let [waiting, working, _, idle, stale] = open_items.clone().map_keys();
    let past = closed_items[0].key.clone();
    let list = listing(open_items, closed_items, now(), 3 * DAY, &Filter::default());
    let folds = SessionFolds {
        collapsed: [SessionSection::Done].into_iter().collect(),
        settled_open: true,
    };

    let drawn = page(list, &folds);

    assert_eq!(
        drawn.settled,
        Some(SettledZone {
            count: 2,
            open: true,
            rows: vec![past.clone(), stale.clone()],
        })
    );
    assert_eq!(drawn.collapsed, vec![(Urgency::Done, 1)]);
    assert_eq!(drawn.rows(), vec![waiting, working, idle, past, stale]);
}

#[test]
fn nothing_collapsed_and_nothing_settled_is_the_list_alone() {
    let items = vec![open(Agent::Codex, Status::Working, "atlas", minutes(3))];
    let key = items[0].key.clone();
    let list = listing(items, Vec::new(), now(), 3 * DAY, &Filter::default());

    let drawn = page(list, &SessionFolds::default());

    assert_eq!(
        drawn,
        Page {
            list: vec![Line::Section(Urgency::Working, 1), Line::Row(key.clone())],
            collapsed: Vec::new(),
            settled: None,
        }
    );
    assert_eq!(drawn.rows(), vec![key]);
}

/// Every section maps to the setting's name for it and back, so a fold
/// saved by one is read as the same section by the other.
#[test]
fn every_section_has_its_setting() {
    for urgency in Urgency::ALL {
        assert_eq!(Urgency::from(SessionSection::from(urgency)), urgency);
    }
}

trait MapKeys {
    fn map_keys(self) -> [Key; 5];
}

impl MapKeys for Vec<Item> {
    fn map_keys(self) -> [Key; 5] {
        let keys: Vec<Key> = self.into_iter().map(|item| item.key).collect();
        keys.try_into().expect("five items")
    }
}
