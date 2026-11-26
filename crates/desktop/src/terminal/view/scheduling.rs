//! Visual deadlines are armed by paints, not by an always-running frame loop.
use std::time::{Duration, Instant};

pub(super) const FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub(super) fn scrollbar_deadline(
    scrolled: Option<Instant>,
    painted: Option<Instant>,
) -> Option<Instant> {
    let start = scrolled?;
    let painted = painted?;
    let hold = start + Duration::from_millis(700);
    let end = hold + Duration::from_millis(400);
    (painted < end).then(|| (painted + Duration::from_millis(16)).max(hold).min(end))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrollbar_waits_through_hold_then_repaints_until_transparent() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        assert_eq!(scrollbar_deadline(Some(start), Some(at(0))), Some(at(700)));
        assert_eq!(
            scrollbar_deadline(Some(start), Some(at(700))),
            Some(at(716))
        );
        assert_eq!(
            scrollbar_deadline(Some(start), Some(at(1099))),
            Some(at(1100))
        );
        assert_eq!(scrollbar_deadline(Some(start), Some(at(1100))), None);
        assert_eq!(scrollbar_deadline(None, Some(start)), None);
    }
}

/// Preserve the old terminal's 16ms presentation budget without an idle clock.
#[derive(Default)]
pub(super) struct Redraw {
    last: Option<Instant>,
    pending: Option<Instant>,
}
impl Redraw {
    pub fn request(&mut self, now: Instant) {
        self.pending.get_or_insert_with(|| {
            self.last
                .map_or(now, |last| (last + FRAME_INTERVAL).max(now))
        });
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.pending
    }
    pub fn take(&mut self, now: Instant) -> bool {
        if self.pending.is_some_and(|at| at <= now) {
            self.pending = None;
            self.last = Some(now);
            true
        } else {
            false
        }
    }
    pub fn painted(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod redraw_tests {
    use super::*;
    #[test]
    fn a_continuous_output_burst_keeps_the_existing_frame_budget_then_sleeps() {
        let start = Instant::now();
        let mut redraw = Redraw::default();
        let mut frames = 0;
        for ms in 0..1000 {
            let now = start + Duration::from_millis(ms);
            redraw.request(now);
            if redraw.take(now) {
                frames += 1;
                redraw.painted();
            }
        }
        assert!(
            frames <= 63,
            "one second of output requested {frames} frames"
        );
        assert!(
            frames >= 60,
            "output was throttled below the existing budget"
        );
        assert!(redraw.take(start + Duration::from_millis(1008)));
        assert_eq!(redraw.deadline(), None, "quiet view retained a timer");
        assert!(!redraw.take(start + Duration::from_secs(2)));
    }
    #[test]
    fn an_intervening_external_paint_consumes_the_pending_frame() {
        let start = Instant::now();
        let mut redraw = Redraw::default();
        redraw.request(start);
        assert!(redraw.take(start));
        redraw.request(start + Duration::from_millis(1));
        redraw.painted();
        assert_eq!(redraw.deadline(), None);
        assert!(!redraw.take(start + Duration::from_secs(1)));
    }
}
