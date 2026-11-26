//! Classification of a precise, two-finger horizontal swipe.
//!
//! GPUI deliberately stays outside this state machine. The view translates a
//! `ScrollWheelEvent` into [`Detector::heard`], which makes recorded gestures
//! replayable without a window or a trackpad.

use std::collections::VecDeque;
use std::time::Duration;

use gpui::TouchPhase;

pub const DECIDE: f32 = 10.0;
pub const FLICK: f32 = 0.6;
pub const TAIL: Duration = Duration::from_millis(650);
pub const QUIET: Duration = Duration::from_millis(250);
pub const LANDING: Duration = Duration::from_millis(180);
const VELOCITY_WINDOW: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take {
    Pass,
    Swallow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    None,
    Follow(f32),
    Land { to: Option<Step>, from: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Undecided,
    Swipe,
    Vertical,
}

#[derive(Debug)]
struct Gesture {
    kind: Kind,
    x: f32,
    y: f32,
    shown: f32,
    last: Duration,
    history: VecDeque<(Duration, f32)>,
}

#[derive(Debug)]
struct Tail {
    last: Duration,
}

#[derive(Debug, Default)]
pub struct Detector {
    gesture: Option<Gesture>,
    tail: Option<Tail>,
}

impl Detector {
    #[allow(clippy::too_many_arguments)]
    pub fn heard(
        &mut self,
        at: Duration,
        phase: TouchPhase,
        precise: bool,
        dx: f32,
        dy: f32,
        natural: bool,
        width: f32,
        ends: (bool, bool),
    ) -> (Take, Motion) {
        if matches!(phase, TouchPhase::Started) {
            self.tail = None;
            self.gesture = precise.then(|| Gesture {
                kind: Kind::Undecided,
                x: 0.0,
                y: 0.0,
                shown: 0.0,
                last: at,
                history: VecDeque::from([(at, 0.0)]),
            });
        } else if let Some(tail) = &mut self.tail {
            if at.saturating_sub(tail.last) <= TAIL {
                tail.last = at;
                return (Take::Swallow, Motion::None);
            }
            self.tail = None;
        }

        let Some(gesture) = &mut self.gesture else {
            return (Take::Pass, Motion::None);
        };
        // A workspace carousel follows the content rather than the scroll
        // direction: fingers moving left reveal the workspace on the left.
        // AppKit reverses its device delta when Natural Scrolling is off, so
        // keep the gesture itself stable and then apply the carousel's
        // deliberately inverted direction.
        let dx = if natural { -dx } else { dx };
        gesture.x += dx;
        gesture.y += dy;
        gesture.last = at;
        gesture.history.push_back((at, gesture.x));
        while gesture.history.len() > 1 && at.saturating_sub(gesture.history[0].0) > VELOCITY_WINDOW
        {
            gesture.history.pop_front();
        }

        if gesture.kind == Kind::Undecided && gesture.x.hypot(gesture.y) >= DECIDE {
            gesture.kind = if gesture.x.abs() >= 2.0 * gesture.y.abs() {
                Kind::Swipe
            } else {
                Kind::Vertical
            };
        }

        let take = match gesture.kind {
            Kind::Swipe => Take::Swallow,
            Kind::Vertical => Take::Pass,
            Kind::Undecided if dx.abs() > dy.abs() => Take::Swallow,
            Kind::Undecided => Take::Pass,
        };

        if gesture.kind == Kind::Swipe {
            let missing = (gesture.x > 0.0 && !ends.0) || (gesture.x < 0.0 && !ends.1);
            gesture.shown = if missing { gesture.x / 3.0 } else { gesture.x };
        }

        if matches!(phase, TouchPhase::Ended) {
            let gesture = self.gesture.take().expect("gesture was just borrowed");
            if gesture.kind == Kind::Vertical {
                return (Take::Pass, Motion::None);
            }
            let motion = Self::land(&gesture, width, ends);
            if take == Take::Swallow || gesture.kind == Kind::Swipe {
                self.tail = Some(Tail { last: at });
            }
            return (take, motion);
        }

        let motion = if gesture.kind == Kind::Swipe {
            Motion::Follow(gesture.shown)
        } else {
            Motion::None
        };
        (take, motion)
    }

    pub fn tick(&mut self, at: Duration) -> Motion {
        if self
            .tail
            .as_ref()
            .is_some_and(|tail| at.saturating_sub(tail.last) > TAIL)
        {
            self.tail = None;
        }
        let Some(gesture) = self.gesture.as_ref() else {
            return Motion::None;
        };
        if at.saturating_sub(gesture.last) < QUIET {
            return Motion::None;
        }
        let gesture = self.gesture.take().expect("gesture was present");
        if gesture.kind == Kind::Swipe || gesture.shown != 0.0 {
            Motion::Land {
                to: None,
                from: gesture.shown,
            }
        } else {
            Motion::None
        }
    }

    fn land(gesture: &Gesture, width: f32, ends: (bool, bool)) -> Motion {
        let velocity = gesture
            .history
            .front()
            .zip(gesture.history.back())
            .and_then(|((start_at, start_x), (end_at, end_x))| {
                let millis = end_at.saturating_sub(*start_at).as_secs_f32() * 1_000.0;
                (millis > 0.0).then(|| (end_x - start_x) / millis)
            });
        let far = gesture.x.abs() >= width / 3.0;
        let flick = velocity
            .is_some_and(|speed| speed.abs() >= FLICK && speed.signum() == gesture.x.signum());
        let to = if far || flick {
            if gesture.x > 0.0 && ends.0 {
                Some(Step::Previous)
            } else if gesture.x < 0.0 && ends.1 {
                Some(Step::Next)
            } else {
                None
            }
        } else {
            None
        };
        Motion::Land {
            to,
            from: gesture.shown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }
    fn hear(
        detector: &mut Detector,
        t: u64,
        phase: TouchPhase,
        dx: f32,
        dy: f32,
    ) -> (Take, Motion) {
        detector.heard(ms(t), phase, true, dx, dy, true, 240.0, (true, true))
    }

    fn replay(text: &str) -> Vec<(Take, Motion)> {
        let mut detector = Detector::default();
        text.lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let mut fields = line.split_whitespace();
                let at = fields.next().unwrap().parse().unwrap();
                let phase = match fields.next().unwrap() {
                    "started" => TouchPhase::Started,
                    "moved" => TouchPhase::Moved,
                    "ended" => TouchPhase::Ended,
                    other => panic!("unknown phase {other}"),
                };
                let dx = fields.next().unwrap().parse().unwrap();
                let dy = fields.next().unwrap().parse().unwrap();
                hear(&mut detector, at, phase, dx, dy)
            })
            .collect()
    }

    #[test]
    fn recorded_left_swipe_lands_previous_and_tail_is_swallowed() {
        let events = replay(include_str!("../tests/fixtures/swipe-horizontal.txt"));
        assert!(
            events
                .iter()
                .any(|(_, motion)| matches!(motion, Motion::Follow(_)))
        );
        assert!(events.iter().any(|(_, motion)| matches!(
            motion,
            Motion::Land {
                to: Some(Step::Previous),
                ..
            }
        )));
        assert_eq!(events.last().unwrap().0, Take::Swallow);
    }

    #[test]
    fn recorded_diagonal_is_vertical() {
        let events = replay(include_str!("../tests/fixtures/swipe-diagonal.txt"));
        assert!(events.iter().all(|(_, motion)| *motion == Motion::None));
        assert_eq!(events.last().unwrap().0, Take::Pass);
    }

    #[test]
    fn recorded_vertical_and_its_momentum_pass() {
        let events = replay(include_str!("../tests/fixtures/swipe-vertical.txt"));
        assert!(
            events
                .iter()
                .all(|event| *event == (Take::Pass, Motion::None))
        );
    }

    #[test]
    fn line_wheel_is_never_a_candidate() {
        let mut detector = Detector::default();
        assert_eq!(
            detector.heard(
                ms(0),
                TouchPhase::Started,
                false,
                -100.0,
                0.0,
                true,
                240.0,
                (true, true)
            ),
            (Take::Pass, Motion::None)
        );
        assert_eq!(
            detector.heard(
                ms(10),
                TouchPhase::Ended,
                false,
                -100.0,
                0.0,
                true,
                240.0,
                (true, true)
            ),
            (Take::Pass, Motion::None)
        );
    }

    #[test]
    fn a_vertical_decision_never_changes_its_mind() {
        let mut detector = Detector::default();
        hear(&mut detector, 0, TouchPhase::Started, 0.0, 0.0);
        hear(&mut detector, 10, TouchPhase::Moved, 8.0, 11.0);
        assert_eq!(
            hear(&mut detector, 20, TouchPhase::Moved, 80.0, 0.0),
            (Take::Pass, Motion::None)
        );
    }

    #[test]
    fn missing_neighbour_rubber_bands_and_lands_back() {
        let mut detector = Detector::default();
        detector.heard(
            ms(0),
            TouchPhase::Started,
            true,
            0.0,
            0.0,
            true,
            240.0,
            (false, true),
        );
        let (_, motion) = detector.heard(
            ms(30),
            TouchPhase::Moved,
            true,
            -90.0,
            0.0,
            true,
            240.0,
            (false, true),
        );
        assert_eq!(motion, Motion::Follow(30.0));
        let (_, motion) = detector.heard(
            ms(60),
            TouchPhase::Ended,
            true,
            0.0,
            0.0,
            true,
            240.0,
            (false, true),
        );
        assert_eq!(
            motion,
            Motion::Land {
                to: None,
                from: 30.0
            }
        );
    }

    #[test]
    fn a_short_slow_swipe_lands_back() {
        let mut detector = Detector::default();
        hear(&mut detector, 0, TouchPhase::Started, 0.0, 0.0);
        hear(&mut detector, 100, TouchPhase::Moved, -30.0, 0.0);
        let (_, motion) = hear(&mut detector, 200, TouchPhase::Ended, -10.0, 0.0);
        assert_eq!(
            motion,
            Motion::Land {
                to: None,
                from: 40.0
            }
        );
    }

    #[test]
    fn a_fast_short_flick_lands() {
        let mut detector = Detector::default();
        hear(&mut detector, 0, TouchPhase::Started, 0.0, 0.0);
        hear(&mut detector, 70, TouchPhase::Moved, -5.0, 0.0);
        let (_, motion) = hear(&mut detector, 86, TouchPhase::Ended, -20.0, 0.0);
        assert!(matches!(
            motion,
            Motion::Land {
                to: Some(Step::Previous),
                ..
            }
        ));
    }

    #[test]
    fn an_unended_swipe_lands_back_after_quiet() {
        let mut detector = Detector::default();
        hear(&mut detector, 0, TouchPhase::Started, 0.0, 0.0);
        hear(&mut detector, 20, TouchPhase::Moved, -30.0, 0.0);
        assert_eq!(detector.tick(ms(269)), Motion::None);
        assert_eq!(
            detector.tick(ms(270)),
            Motion::Land {
                to: None,
                from: 30.0
            }
        );
    }

    #[test]
    fn a_new_start_ends_the_old_tail() {
        let mut detector = Detector::default();
        hear(&mut detector, 0, TouchPhase::Started, 0.0, 0.0);
        hear(&mut detector, 20, TouchPhase::Moved, -100.0, 0.0);
        hear(&mut detector, 30, TouchPhase::Ended, 0.0, 0.0);
        assert_eq!(
            hear(&mut detector, 40, TouchPhase::Moved, -5.0, 0.0).0,
            Take::Swallow
        );
        assert_eq!(
            hear(&mut detector, 50, TouchPhase::Started, 0.0, 0.0),
            (Take::Pass, Motion::None)
        );
    }

    #[test]
    fn natural_setting_normalizes_device_delta() {
        let mut natural = Detector::default();
        let mut traditional = Detector::default();
        natural.heard(
            ms(0),
            TouchPhase::Started,
            true,
            0.0,
            0.0,
            true,
            240.0,
            (true, true),
        );
        traditional.heard(
            ms(0),
            TouchPhase::Started,
            true,
            0.0,
            0.0,
            false,
            240.0,
            (true, true),
        );
        let a = natural.heard(
            ms(30),
            TouchPhase::Moved,
            true,
            -30.0,
            0.0,
            true,
            240.0,
            (true, true),
        );
        let b = traditional.heard(
            ms(30),
            TouchPhase::Moved,
            true,
            30.0,
            0.0,
            false,
            240.0,
            (true, true),
        );
        assert_eq!(a, b);
    }
}
