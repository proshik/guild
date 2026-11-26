//! Turning scroll events into rows of history.
//!
//! A mouse wheel reports whole notches; a trackpad reports pixels, and on a
//! gentle swipe each event carries two or three of them. Dividing one event
//! by the cell height and rounding gives zero — so the terminal did not
//! scroll at all from a trackpad, while a wheel worked. That was the
//! complaint.
//!
//! The cure is to carry the remainder between events instead of throwing it
//! away: many small deltas add up to a row.
//!
//! Where the rows then go is a second question, and it has three answers.
//! A full-screen program — `claude`, `vim`, `less` — draws in the
//! alternate screen, which has no history to scroll: moving the viewport
//! there does nothing, and that looks exactly like a broken trackpad. Such
//! a program either reads the mouse itself, and then the wheel is
//! forwarded to it, or it does not, and then the wheel becomes cursor
//! keys. [`Action`] decides which.

use alacritty_terminal::term::TermMode;

/// The remainder carried between scroll events.
///
/// Its own type rather than a bare `f32` so that the direction of the sign
/// and the fact that it is *pixels, not rows* cannot be mixed up at the
/// call site.
#[derive(Debug, Default, Clone, Copy)]
pub struct Scroll {
    /// Pixels that have not yet added up to a whole row.
    carry: f32,
}

impl Scroll {
    /// How many rows this event scrolls, remembering what is left over.
    ///
    /// A positive number scrolls into the history, matching the sign of a
    /// scroll event pointing up.
    pub fn rows(&mut self, delta_px: f32, cell_height: f32) -> i32 {
        if cell_height <= 0.0 || !delta_px.is_finite() {
            return 0;
        }

        self.carry += delta_px;
        let rows = (self.carry / cell_height).trunc();
        self.carry -= rows * cell_height;

        rows as i32
    }

    /// Forget the remainder.
    ///
    /// Called when the direction changes or the view jumps to the bottom:
    /// carrying pixels from an old gesture into a new one makes the first
    /// event of the new one overshoot.
    pub fn clear(&mut self) {
        self.carry = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f32 = 16.0;

    /// A wheel notch is a whole row and always worked.
    #[test]
    fn a_whole_notch_scrolls_a_row() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(16.0, CELL), 1);
        assert_eq!(scroll.rows(-16.0, CELL), -1);
    }

    /// Where the bug was: a trackpad sends a few pixels per event, and one
    /// event on its own is not a row. Rounding each event separately gave
    /// zero every time and the terminal never moved.
    #[test]
    fn small_trackpad_deltas_add_up_to_a_row() {
        let mut scroll = Scroll::default();

        let mut rows = 0;
        for _ in 0..8 {
            rows += scroll.rows(3.0, CELL);
        }

        assert_eq!(rows, 1, "24 pixels of swipe is a row and a half");
        assert!(rows > 0, "a trackpad must scroll at all");
    }

    /// The remainder is kept rather than dropped: otherwise a slow swipe
    /// loses a row every time it crosses a boundary.
    #[test]
    fn the_remainder_is_carried_not_lost() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(12.0, CELL), 0, "not a row yet");
        assert_eq!(scroll.rows(12.0, CELL), 1, "24 pixels is one row");
        assert_eq!(scroll.rows(8.0, CELL), 1, "the 8 left over complete it");
    }

    #[test]
    fn direction_is_kept_across_events() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(-10.0, CELL), 0);
        assert_eq!(scroll.rows(-10.0, CELL), -1, "downwards stays downwards");
    }

    /// Clearing between gestures: a leftover from a swipe up must not make
    /// the first event of a swipe down jump.
    #[test]
    fn clearing_forgets_the_remainder() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(15.0, CELL), 0);
        scroll.clear();
        assert_eq!(scroll.rows(2.0, CELL), 0, "the 15 pixels are gone");
    }

    /// The cell size is unknown until the first paint. Dividing by it then
    /// would be a division by zero.
    #[test]
    fn an_unknown_cell_size_scrolls_nothing() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(100.0, 0.0), 0);
        assert_eq!(scroll.rows(100.0, -1.0), 0);
    }

    #[test]
    fn a_broken_delta_is_ignored() {
        let mut scroll = Scroll::default();

        assert_eq!(scroll.rows(f32::NAN, CELL), 0);
        assert_eq!(scroll.rows(f32::INFINITY, CELL), 0);
        assert_eq!(scroll.rows(16.0, CELL), 1, "and the carry is still sane");
    }
}

/// What a scroll should actually do.
///
/// The decision is a value rather than a branch inside the event handler
/// so that it can be checked without a window, a process or a pty — all
/// three of which the handler needs and none of which the rule does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move the viewport over the history.
    Grid(i32),
    /// Send cursor keys: the program keeps its own view.
    ///
    /// This is what makes a trackpad work in `less` and in `vim`, where
    /// there is no history to move over and no mouse being read.
    Keys { up: bool, count: usize },
    /// Forward the wheel to the program, which asked to receive it.
    ///
    /// `claude` asks: it turns on mouse motion and SGR reporting the
    /// moment it starts. Acting on the wheel ourselves there — or, worse,
    /// faking cursor keys — would make it act on presses nobody made.
    Report { up: bool, count: usize },
    /// Do nothing.
    Nothing,
}

/// Decide what a scroll of `rows` means in this terminal mode.
///
/// The rule is xterm's, and every terminal people come from follows it:
///
/// - the program reads the mouse itself → forward the wheel to it. It
///   decides what scrolling means in its own interface, and faking cursor
///   keys under it would make it act on presses nobody made;
/// - alternate screen with alternate scroll enabled → cursor keys;
/// - otherwise → the history.
pub fn action(mode: TermMode, rows: i32) -> Action {
    if rows == 0 {
        return Action::Nothing;
    }

    if mode.intersects(TermMode::MOUSE_MODE) {
        return Action::Report {
            up: rows > 0,
            count: rows.unsigned_abs() as usize,
        };
    }

    if mode.contains(TermMode::ALT_SCREEN) {
        if !mode.contains(TermMode::ALTERNATE_SCROLL) {
            return Action::Nothing;
        }
        return Action::Keys {
            up: rows > 0,
            count: rows.unsigned_abs() as usize,
        };
    }

    Action::Grid(rows)
}

/// Where a history key moves the grid — M5.12 task 5, decision 7.
///
/// `shift-pageup` and `shift-pagedown` a screen, `cmd-home` and `cmd-end` to
/// the top of the history and back to the newest output — what every
/// terminal a person comes from binds them to. **`None` in the alternate
/// screen**, which keeps no history: there the strokes are the program's and
/// go to it as the keys they are. `None` too for every other stroke,
/// including a history key with a modifier more than its own, so a program
/// that binds `cmd-shift-home` still gets it.
pub fn history_move(
    keystroke: &gpui::Keystroke,
    mode: TermMode,
) -> Option<alacritty_terminal::grid::Scroll> {
    use alacritty_terminal::grid::Scroll as Move;

    if mode.contains(TermMode::ALT_SCREEN) {
        return None;
    }
    let held = &keystroke.modifiers;
    let only_shift = held.shift && !held.platform && !held.alt && !held.control && !held.function;
    let only_cmd = held.platform && !held.shift && !held.alt && !held.control && !held.function;
    match keystroke.key.as_str() {
        "pageup" if only_shift => Some(Move::PageUp),
        "pagedown" if only_shift => Some(Move::PageDown),
        "home" if only_cmd => Some(Move::Top),
        "end" if only_cmd => Some(Move::Bottom),
        _ => None,
    }
}

/// How often a selection held past the terminal's edge scrolls — M12.1.
pub const EDGE_TICK: std::time::Duration = std::time::Duration::from_millis(80);

/// The most lines one tick moves, however far out the pointer is held.
pub const EDGE_CEILING: usize = 6;

/// How many lines a tick scrolls with the pointer `beyond` pixels past the
/// edge — M12.1 task 2.
///
/// **One just past the edge, one more for every two cells further**, up to
/// [`EDGE_CEILING`]: close to the edge a selection can still be ended on the
/// line meant, and far out a page goes by in a second. A point on the edge
/// or inside is not past it. A cell not measured yet counts as far enough
/// for one line and no more.
pub fn edge_rows(beyond: f32, cell_height: f32) -> usize {
    if beyond <= 0.0 {
        return 0;
    }
    if cell_height <= 0.0 {
        return 1;
    }
    (1 + (beyond / (2.0 * cell_height)) as usize).min(EDGE_CEILING)
}

#[cfg(test)]
mod action_tests {
    use super::*;

    /// Ordinary output: the wheel moves the viewport over the history.
    #[test]
    fn without_a_full_screen_program_the_history_moves() {
        assert_eq!(action(TermMode::default(), 3), Action::Grid(3));
        assert_eq!(action(TermMode::default(), -2), Action::Grid(-2));
    }

    /// The case the trackpad looked broken in. `claude` draws in the
    /// alternate screen, where there is no history at all: moving the
    /// viewport does nothing, and nothing is what was seen.
    #[test]
    fn a_full_screen_program_gets_cursor_keys() {
        let mode = TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL;

        assert_eq!(action(mode, 2), Action::Keys { up: true, count: 2 });
        assert_eq!(
            action(mode, -3),
            Action::Keys {
                up: false,
                count: 3
            }
        );
    }

    /// What `claude` actually does: it turns on mouse motion and SGR
    /// reporting at startup, so the wheel belongs to it. This is the case
    /// the trackpad was silent in — the mouse modes were checked and
    /// answered with "do nothing".
    #[test]
    fn a_program_reading_the_mouse_gets_the_wheel() {
        let claude = TermMode::ALT_SCREEN
            | TermMode::ALTERNATE_SCROLL
            | TermMode::MOUSE_MOTION
            | TermMode::SGR_MOUSE;

        assert_eq!(action(claude, 5), Action::Report { up: true, count: 5 });
        assert_eq!(
            action(claude, -2),
            Action::Report {
                up: false,
                count: 2
            }
        );
    }

    /// Reading the mouse wins over everything else: the program asked for
    /// the events, and it gets them whatever screen it is drawing on.
    #[test]
    fn reading_the_mouse_wins_over_the_history() {
        let mode = TermMode::MOUSE_REPORT_CLICK;

        assert_eq!(action(mode, 3), Action::Report { up: true, count: 3 });
    }

    /// A program can turn alternate scroll off, and then it means it.
    #[test]
    fn alternate_scroll_can_be_refused() {
        assert_eq!(action(TermMode::ALT_SCREEN, 4), Action::Nothing);
    }

    #[test]
    fn no_rows_is_no_action() {
        assert_eq!(action(TermMode::default(), 0), Action::Nothing);
    }
}

/// Where the scrollbar's thumb sits, in pixels down the painted area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// The thumb's top edge.
    pub top: f32,
    /// The thumb's height.
    pub height: f32,
}

/// The shortest a thumb may be drawn.
///
/// In a long history the proportional height goes to a couple of pixels
/// and stops reading as a thumb at all — it looks like a speck of dirt on
/// the edge of the window.
const MIN_THUMB: f32 = 24.0;

/// The scrollbar for a view of `screen` rows over `total`, scrolled back
/// by `offset`.
///
/// `None` when everything fits: a scrollbar over content that does not
/// scroll is furniture, and in the alternate screen — where there is no
/// history at all — it would be a lie.
pub fn bar(total: usize, screen: usize, offset: usize, height: f32) -> Option<Bar> {
    if screen == 0 || total <= screen || height <= 0.0 {
        return None;
    }

    let visible = (screen as f32 / total as f32).clamp(0.0, 1.0);
    let thumb = (height * visible).max(MIN_THUMB).min(height);

    // `offset` counts backwards from the newest output: zero is the
    // bottom. The thumb has to move the other way, or it would climb as
    // the person scrolls down.
    let furthest = (total - screen) as f32;
    let scrolled = (offset as f32 / furthest).clamp(0.0, 1.0);
    let travel = height - thumb;

    Some(Bar {
        top: travel * (1.0 - scrolled),
        height: thumb,
    })
}

#[cfg(test)]
mod bar_tests {
    use super::*;

    const HEIGHT: f32 = 400.0;

    /// A scrollbar over content that does not scroll is furniture — and in
    /// the alternate screen, where there is no history, it would be a lie
    /// about what the wheel can do.
    #[test]
    fn nothing_to_scroll_means_no_bar() {
        assert_eq!(bar(24, 24, 0, HEIGHT), None);
        assert_eq!(bar(10, 24, 0, HEIGHT), None);
    }

    /// At the bottom — where a terminal spends most of its life — the
    /// thumb is at the bottom.
    #[test]
    fn at_the_newest_output_the_thumb_is_at_the_bottom() {
        let bar = bar(100, 25, 0, HEIGHT).expect("there is history");

        assert!(
            (bar.top + bar.height - HEIGHT).abs() < 0.5,
            "the thumb must end at the bottom edge: {bar:?}"
        );
    }

    /// Scrolled all the way back, the thumb is at the top. Getting this
    /// backwards makes the thumb climb as the person scrolls down.
    #[test]
    fn at_the_oldest_output_the_thumb_is_at_the_top() {
        let bar = bar(100, 25, 75, HEIGHT).expect("there is history");

        assert!(
            bar.top.abs() < 0.5,
            "the thumb must start at the top: {bar:?}"
        );
    }

    /// The thumb's size says how much of the history is on screen.
    #[test]
    fn the_thumb_is_as_tall_as_the_share_on_screen() {
        let quarter = bar(100, 25, 0, HEIGHT).expect("there is history");
        let half = bar(50, 25, 0, HEIGHT).expect("there is history");

        assert!((quarter.height - HEIGHT / 4.0).abs() < 0.5, "{quarter:?}");
        assert!((half.height - HEIGHT / 2.0).abs() < 0.5, "{half:?}");
        assert!(half.height > quarter.height, "more on screen, taller thumb");
    }

    /// In a long history the proportional height goes to a couple of
    /// pixels and stops reading as a thumb at all.
    #[test]
    fn a_very_long_history_still_gets_a_visible_thumb() {
        let bar = bar(100_000, 25, 0, HEIGHT).expect("there is history");

        assert!(bar.height >= MIN_THUMB, "too small to see: {bar:?}");
        assert!(bar.top + bar.height <= HEIGHT + 0.5, "and still inside");
    }

    /// The thumb never leaves the track, at either end.
    #[test]
    fn the_thumb_stays_inside_the_track() {
        for offset in [0, 1, 37, 74, 75, 200] {
            let bar = bar(100, 25, offset, HEIGHT).expect("there is history");

            assert!(bar.top >= -0.5, "above the track at {offset}: {bar:?}");
            assert!(
                bar.top + bar.height <= HEIGHT + 0.5,
                "below the track at {offset}: {bar:?}"
            );
        }
    }

    /// A window collapsed to nothing has no track to draw in.
    #[test]
    fn no_height_means_no_bar() {
        assert_eq!(bar(100, 25, 0, 0.0), None);
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;
    use alacritty_terminal::grid::Scroll as Move;
    use gpui::{Keystroke, Modifiers};

    fn stroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    /// **The history keys move the grid on the primary screen, and only
    /// there** — M5.12 task 5, decision 7. `shift-pageup` and
    /// `shift-pagedown` go a screen, `cmd-home` and `cmd-end` to an end; in
    /// the alternate screen there is no history to move, so the same
    /// strokes are the program's, as keys — the trap *Scrolling a
    /// full-screen program does nothing, and that is correct*. A plain
    /// page key is the program's everywhere.
    #[test]
    fn the_history_keys_move_the_grid_only_on_the_primary_screen() {
        let primary = TermMode::default();
        assert!(matches!(
            history_move(&stroke("pageup", Modifiers::shift()), primary),
            Some(Move::PageUp)
        ));
        assert!(matches!(
            history_move(&stroke("pagedown", Modifiers::shift()), primary),
            Some(Move::PageDown)
        ));
        assert!(matches!(
            history_move(&stroke("home", Modifiers::command()), primary),
            Some(Move::Top)
        ));
        assert!(matches!(
            history_move(&stroke("end", Modifiers::command()), primary),
            Some(Move::Bottom)
        ));

        for (key, modifiers) in [
            ("pageup", Modifiers::shift()),
            ("pagedown", Modifiers::shift()),
            ("home", Modifiers::command()),
            ("end", Modifiers::command()),
        ] {
            assert!(
                history_move(&stroke(key, modifiers), TermMode::ALT_SCREEN).is_none(),
                "{key} goes to a full-screen program"
            );
        }

        for (key, modifiers) in [
            ("pageup", Modifiers::none()),
            ("pagedown", Modifiers::none()),
            ("home", Modifiers::none()),
            ("end", Modifiers::shift()),
            ("pageup", Modifiers::control()),
            ("home", Modifiers::command_shift()),
            ("up", Modifiers::shift()),
            ("a", Modifiers::command()),
        ] {
            assert!(
                history_move(&stroke(key, modifiers), primary).is_none(),
                "{key} with {modifiers:?} is not a history key"
            );
        }
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    /// **A selection held past the edge scrolls, and faster the further
    /// out it is held** — M12.1 task 2, the rate every terminal a person
    /// comes from has. Just past the edge is one line a tick, so a
    /// selection can still be ended on the line meant; far out is a
    /// ceiling, so the history is not gone before the eye follows.
    #[test]
    fn a_selection_past_the_edge_scrolls_faster_the_further_it_is() {
        let cell = 16.0;
        assert_eq!(edge_rows(0.0, cell), 0, "on the edge is inside");
        assert_eq!(edge_rows(-5.0, cell), 0, "inside is inside");
        assert_eq!(edge_rows(1.0, cell), 1, "just past");
        assert_eq!(edge_rows(2.0 * cell, cell), 2);
        assert!(edge_rows(4.0 * cell, cell) > edge_rows(2.0 * cell, cell));
        assert_eq!(edge_rows(1000.0, cell), EDGE_CEILING, "far out");
        assert_eq!(edge_rows(10.0, 0.0), 1, "a cell not measured yet");
    }
}
