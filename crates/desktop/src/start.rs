//! The start screen as a model: what the rows are, where the one
//! highlight is, what a key does, and what resuming a row comes to —
//! M14.5. No GPUI here; `root::start` carries a `Deed` out (task 6) and
//! wires this to the window's focus and keys (task 7), `view::start`
//! (task 8) draws it.
//!
//! **One highlight, never two** (the design's "What a press does" and
//! `docs/traps.md`'s "A list with rows keeps one of them lit" /
//! "A hover tint in the highlight's colour is a second highlight"): `Lit`
//! is one value, walked by `Card, Row(0), Row(1), …` and nothing else
//! moves it.

use std::time::{Duration, SystemTime};

use guild_session::history::{LIMIT, Past};
use guild_session::{DefaultTab, HarnessKind, SessionId};

use crate::pick;

/// What of the start screen a place of this size shows — M12.8,
/// decision 20, amended by its second window check: the default card at
/// every size; the tiles and the heading while they fit, the heading
/// going first; *Recent* and the foot from their heights, and only under
/// the whole of the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fits {
    /// Whether the folder's name and the line under it are drawn.
    pub heading: bool,
    /// Whether *All tools* and its tiles are drawn.
    pub tiles: bool,
    /// Whether the tiles stand in two rows of two — the place is too
    /// narrow for four side by side. The height above is counted for
    /// these rows, and the view draws exactly these (window check 3).
    pub two_rows: bool,
    /// Whether *Recent* is drawn.
    pub recent: bool,
    /// Whether the foot is drawn.
    pub foot: bool,
}

impl Fits {
    /// Every part — a place not measured yet, which the first frame
    /// corrects.
    pub const ALL: Fits = Fits {
        heading: true,
        tiles: true,
        two_rows: false,
        recent: true,
        foot: true,
    };
}

/// GPUI's line height when a text sets none, which the heading's two
/// lines are set in: the golden ratio.
const LINE: f32 = 1.618;

/// [`Fits`] for a place `width` by `height` points — points of the design
/// system's own scale, so the window divides its pixels by its rem size
/// first: a larger interface needs a larger place for the same screen.
/// An empty area meets the same thresholds as a pane, which a full-height
/// area passes.
///
/// **A part goes rather than being cut** — the second window check's O2:
/// the heading, the card and the tiles are about 320 high, a place may be
/// 160, and the column is clipped at the place's edge. So the heights are
/// the screen's own parts added up from the design system's numbers: the
/// padding above and below, the card, the heading with its gap, the tiles
/// with their rubric — in **two rows** where the place is too narrow for
/// four tiles side by side, a tile being drawn about square — and the gap
/// between two parts.
pub fn fits(width: f32, height: f32) -> Fits {
    use crate::theme::{size, space};
    let px = |rems: gpui::Rems| rems.0 * crate::scale::BASE;
    let gap = px(space::START_GAP);
    let tile = px(space::START_TILE);
    let between = px(space::CARD_GAP);
    // Four tiles in one row need the column four squares wide.
    let one_row = 2.0 * gap + 4.0 * tile + 3.0 * between;
    let rows = if width >= one_row { 1.0 } else { 2.0 };
    let card = 2.0 * gap + px(space::START_CARD);
    let heading = (px(size::SECTION) + px(size::META)) * LINE + px(space::PANEL_PADDING);
    let tiles = gap + px(space::HEAD_TAB) + between + rows * tile + (rows - 1.0) * between;
    let (heading, tiles) = if height >= card + heading + tiles {
        (true, true)
    } else if height >= card + tiles {
        (false, true)
    } else {
        (height >= card + heading, false)
    };
    let whole = heading && tiles;
    Fits {
        heading,
        tiles,
        two_rows: rows > 1.0,
        recent: whole && height >= px(space::START_RECENT_FROM),
        foot: whole && height >= px(space::START_FOOT_FROM),
    }
}

/// Which rows `shown` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    /// Every harness's rows.
    #[default]
    All,
    /// One harness's rows.
    Only(HarnessKind),
}

/// The one thing lit on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lit {
    /// The default card, at the top.
    #[default]
    Card,
    /// A row of `shown()`, by its index there.
    Row(usize),
}

/// What a press means, aimed at the window's own deeds — `root::start`
/// carries these out with `open_harness_tab` and resuming, the same
/// functions the tab strip already calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deed {
    /// The default card opens the configured harness in a terminal.
    Default,
    /// A tile, or its key.
    Tile { tab: DefaultTab },
    /// A row, resumed in a new tab.
    Resume { past: Past },
}

/// The start screen's state: pure, so `key` and a click can be tested
/// without a window.
#[derive(Debug, Clone, Default)]
pub struct Start {
    /// Newest first, as `History::recent` answers.
    pub rows: Vec<Past>,
    /// Which of `rows` `shown` answers.
    pub filter: Filter,
    /// The one highlight.
    pub lit: Lit,
    /// Whether a read of the history has come back — while it has not,
    /// the rows' room stays empty rather than showing a stale answer.
    pub read: bool,
}

impl Start {
    /// A fresh read of the history: `rows` and `read`, plus what it
    /// changes downstream — a chip whose harness lost its last row
    /// cannot stay chosen, and the highlight follows its session.
    ///
    /// **The highlight is a session, not a place.** The rows are read
    /// again every [`AGAIN`] and sorted newest first, so a session written
    /// elsewhere moves up and shifts the ones below it: an index kept
    /// would light another session under the person's keys, and `enter`
    /// would resume that one. It stays on the same harness and id while
    /// that row is shown; only a row gone keeps the index, clamped.
    pub fn set_rows(&mut self, rows: Vec<Past>) {
        let lit = match self.lit {
            Lit::Row(index) => self
                .shown()
                .get(index)
                .map(|past| (past.harness, past.id.clone())),
            Lit::Card => None,
        };
        self.rows = rows;
        self.read = true;
        if let Filter::Only(kind) = self.filter
            && !self.rows.iter().any(|past| past.harness == kind)
        {
            self.filter = Filter::All;
        }
        let shown = self.shown();
        let len = shown.len();
        let found = lit.and_then(|(harness, id)| {
            shown
                .iter()
                .position(|past| past.harness == harness && past.id == id)
        });
        self.lit = match (self.lit, found) {
            (Lit::Card, _) => Lit::Card,
            (Lit::Row(_), Some(index)) => Lit::Row(index),
            (Lit::Row(_), None) if len == 0 => Lit::Card,
            (Lit::Row(index), None) => Lit::Row(index.min(len - 1)),
        };
    }

    /// A read's answer taken in, answering whether the picture changed —
    /// `docs/traps.md`: a clock's notify redraws the whole window, so it
    /// asks first. The first answer always does, even an empty one: it
    /// ends the reading state. The same rows again change nothing, the
    /// highlight and the chip included, which is why they are not handed
    /// to `set_rows` at all.
    pub fn answered(&mut self, rows: Vec<Past>) -> bool {
        if self.read && self.rows == rows {
            return false;
        }
        self.set_rows(rows);
        true
    }

    /// The chips: *All* and one per harness with at least one row, in
    /// `HarnessKind::ALL`'s order — `vec![]` when fewer than two
    /// harnesses have any, because a single chip filters nothing.
    pub fn chips(&self) -> Vec<(Filter, usize)> {
        let per_harness: Vec<(HarnessKind, usize)> = HarnessKind::ALL
            .into_iter()
            .map(|kind| {
                let count = self.rows.iter().filter(|past| past.harness == kind).count();
                (kind, count)
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        if per_harness.len() < 2 {
            return vec![];
        }
        std::iter::once((Filter::All, self.rows.len()))
            .chain(
                per_harness
                    .into_iter()
                    .map(|(kind, count)| (Filter::Only(kind), count)),
            )
            .collect()
    }

    /// `rows`, filtered by `filter`, at most `LIMIT`.
    pub fn shown(&self) -> Vec<&Past> {
        self.rows
            .iter()
            .filter(|past| match self.filter {
                Filter::All => true,
                Filter::Only(kind) => past.harness == kind,
            })
            .take(LIMIT)
            .collect()
    }

    /// A chip taken, by a click or by `left`/`right`: the filter changes
    /// and the highlight goes to the first row, or the card when the
    /// filtered list is empty.
    pub fn choose(&mut self, filter: Filter) {
        self.filter = filter;
        self.lit = if self.shown().is_empty() {
            Lit::Card
        } else {
            Lit::Row(0)
        };
    }

    /// What a key does. `enter` takes the lit thing; the arrows move the
    /// highlight or step the chosen chip; a tile's own key starts it.
    /// Everything else, and a tile whose harness the `PATH` does not
    /// have, answers `None`.
    pub fn key(&mut self, key: &str, available: &[HarnessKind]) -> Option<Deed> {
        match key {
            "enter" => self.take(self.lit),
            "up" => {
                self.walk(false);
                None
            }
            "down" => {
                self.walk(true);
                None
            }
            "left" => {
                self.step_chip(false);
                None
            }
            "right" => {
                self.step_chip(true);
                None
            }
            "1" | "2" | "3" | "t" => {
                let tab = DefaultTab::ALL
                    .into_iter()
                    .find(|tab| tile_key(*tab) == key)?;
                if harness_of(tab).is_some_and(|kind| !available.contains(&kind)) {
                    return None;
                }
                Some(Deed::Tile { tab })
            }
            _ => None,
        }
    }

    /// A click on the lit thing named by `lit` (the card, or a row of
    /// `shown()`) — `None` past the end of a list that has moved from
    /// under a stale click.
    pub fn take(&self, lit: Lit) -> Option<Deed> {
        match lit {
            Lit::Card => Some(Deed::Default),
            Lit::Row(index) => self.shown().get(index).map(|past| Deed::Resume {
                past: (*past).clone(),
            }),
        }
    }

    /// The age words of `shown()`, in its order, as `ago` (`Ui::ago`)
    /// says them at `now` — what the rows draw, and what `poll_start`
    /// compares with the last drawn to repaint an idle screen whose
    /// words moved on (R6: an answer of the same rows notifies nothing,
    /// so without this *just now* would stand for as long as the screen
    /// does). A row touched after `now` — another machine's clock — is
    /// zero seconds old.
    pub fn ages(&self, now: SystemTime, ago: impl Fn(i64) -> String) -> Vec<String> {
        self.shown()
            .iter()
            .map(|past| {
                let seconds = now
                    .duration_since(past.touched)
                    .map_or(0, |age| age.as_secs());
                ago(i64::try_from(seconds).unwrap_or(i64::MAX))
            })
            .collect()
    }

    /// `up`/`down`: one step over `Card, Row(0), …, Row(shown().len() -
    /// 1)`, clamped at both ends. `pick::step` already is this walk once
    /// the card is counted as the list's own first slot — `Card` is its
    /// index 0 and `Row(i)` is its index `i + 1`.
    fn walk(&mut self, down: bool) {
        let len = self.shown().len();
        let current = match self.lit {
            Lit::Card => 0,
            Lit::Row(index) => index + 1,
        };
        // `len + 1` is never 0, so `pick::step` always answers.
        if let Some(next) = pick::step(len + 1, Some(current), down) {
            self.lit = if next == 0 {
                Lit::Card
            } else {
                Lit::Row(next - 1)
            };
        }
    }

    /// `left`/`right`: one step over `chips()`, clamped at both ends —
    /// `choose`, so the highlight follows as a click would.
    fn step_chip(&mut self, right: bool) {
        let chips = self.chips();
        if chips.is_empty() {
            return;
        }
        let last = chips.len() - 1;
        let current = chips
            .iter()
            .position(|(filter, _)| *filter == self.filter)
            .unwrap_or(0);
        let next = if right {
            (current + 1).min(last)
        } else {
            current.saturating_sub(1)
        };
        self.choose(chips[next].0);
    }
}

/// The tile's key, by `DefaultTab::ALL`'s order: `1` `2` `3` `t`.
pub fn tile_key(tab: DefaultTab) -> &'static str {
    match tab {
        DefaultTab::Claude => "1",
        DefaultTab::OpenCode => "2",
        DefaultTab::Codex => "3",
        DefaultTab::Terminal => "t",
    }
}

/// The harness a tile starts, `None` for the terminal — the one mapping
/// `view/menu/strip.rs::harness_of` also calls.
pub fn harness_of(tab: DefaultTab) -> Option<HarnessKind> {
    match tab {
        DefaultTab::Claude => Some(HarnessKind::Claude),
        DefaultTab::OpenCode => Some(HarnessKind::OpenCode),
        DefaultTab::Codex => Some(HarnessKind::Codex),
        DefaultTab::Terminal => None,
    }
}

/// What resuming a row comes to, given the window's records — the
/// design's "Resuming a row".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resuming {
    /// A tab already holds it, in any workspace or seat: bring that one
    /// forward rather than start a second process on one transcript.
    Front(SessionId),
    /// A closed record is it: reopen that record. `conversation` is the
    /// harness's own id to resume, `None` for Claude — the record's id
    /// *is* the conversation, and `continue_or_forget` decides.
    Reopen {
        record: SessionId,
        conversation: Option<String>,
    },
    /// A new record: Claude's own id as the record's, or a fresh one with
    /// `harness_session`. Both `None` is a Claude row whose id is not a
    /// UUID — no record can carry it, so there is nothing to make.
    New {
        id: Option<SessionId>,
        harness_session: Option<String>,
    },
}

/// Whether a record is running as far as resuming it goes — the `open`
/// that [`resuming`] is given. A tab holding it is; so is a record left
/// open in a workspace not restored yet, which has no tab only because
/// that workspace's restore has not run — it will start one, and a
/// reopen now would make it two. Once its workspace is restored, a record
/// that says open and has no tab is one the restore could not bring back:
/// nothing runs it.
pub fn running(in_a_tab: bool, record_open: bool, workspace_restored: bool) -> bool {
    in_a_tab || (record_open && !workspace_restored)
}

/// What resuming `past` comes to, given every record as `(id,
/// harness_session, open)` — `open` being [`running`]'s answer.
///
/// Claude matches a record by its id, which is the conversation id since
/// M5 (`docs/traps.md`: "A `claude` session id is assigned, not guessed");
/// OpenCode and Codex by `harness_session`, the id they reported.
pub fn resuming(past: &Past, records: &[(SessionId, Option<String>, bool)]) -> Resuming {
    let claude = past.harness == HarnessKind::Claude;
    let own: Option<SessionId> = if claude { past.id.parse().ok() } else { None };
    let found = records.iter().find(|(id, theirs, _)| {
        if claude {
            own == Some(*id)
        } else {
            theirs.as_deref() == Some(past.id.as_str())
        }
    });
    match found {
        Some((id, _, true)) => Resuming::Front(*id),
        Some((id, _, false)) => Resuming::Reopen {
            record: *id,
            conversation: (!claude).then(|| past.id.clone()),
        },
        None if claude => Resuming::New {
            id: own,
            harness_session: None,
        },
        None => Resuming::New {
            id: None,
            harness_session: Some(past.id.clone()),
        },
    }
}

/// How often the history is read again while the screen stays up for one
/// folder.
///
/// **Priced by the round, not by one call** (`docs/traps.md`: a cadence
/// justified by measuring one call). One round is a `stat` per Claude
/// transcript of the folder and per rollout of the ≤ 400 Codex ones looked
/// at — each file opened again only when its mtime moved — and one
/// `opencode session list` process, measured at 0.09–0.47 s by the spec.
/// It runs for one folder at a time, only while the start screen is drawn
/// and, after its first read, only while the window is in front: a
/// window with a tab in front, or behind another application, reads
/// nothing.
pub const AGAIN: Duration = Duration::from_secs(10);

/// Whether a read of the history is due: at once when the folder has not
/// been read, then every [`AGAIN`] while the window is in front — never
/// while one is under way. `since_asked` is how long ago the last one
/// began.
pub fn read_due(since_asked: Option<Duration>, asking: bool, in_front: bool) -> bool {
    if asking {
        return false;
    }
    match since_asked {
        None => true,
        Some(since) => in_front && since >= AGAIN,
    }
}

/// Whether the screen takes a stroke rather than the window: its bare
/// keys — the arrows, `enter` and the tiles' `1` `2` `3` `t` — and `cmd`
/// with `enter` alone. **`cmd` with a digit or `t` is the window's**
/// (`cmd-1` switches projects, `cmd-t` opens a session), and so is
/// anything with `ctrl`, `alt` or `shift` (`others`).
pub fn hears(key: &str, cmd: bool, others: bool) -> bool {
    if others {
        return false;
    }
    if cmd {
        return key == "enter";
    }
    matches!(
        key,
        "up" | "down" | "left" | "right" | "enter" | "1" | "2" | "3" | "t"
    )
}
