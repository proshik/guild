//! Ordered shell metadata through the authoritative ANSI processor's replay.
use alacritty_terminal::vte::ansi::*;
use std::{collections::HashMap, time::Instant};
#[path = "semantic_forward.rs"]
mod forward;
const LIMIT: usize = 1024;
/// OSC 7501 reports held in one synchronized frame, apart from `LIMIT`: a
/// frame of status reports must not crowd shell metadata out, nor be cut off
/// by it.
const STATUS_LIMIT: usize = 1024;
const PAYLOAD: usize = 8192;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Semantic {
    Cwd(String),
    Command(String),
    BeforeInput,
    AfterInput,
    BeforeLinefeed,
    AfterLinefeed,
    Invalidate,
    ClearLine,
    ClearBelow,
    ClearAbove,
    Revoke,
    CheckScreen,
    Region(usize, Option<usize>),
    /// An OSC 7501 report, valid by the protocol's rules.
    ProgramStatus(super::program_status::Report),
    /// A full reset (RIS), in byte order.
    Reset,
    /// A shell prompt began (OSC 133 A) whose position has gone stale: its
    /// one effect without a position — the program that was working or
    /// blocked is not in front any more.
    PromptStarted,
}
struct Forward<'a, H, F> {
    handler: &'a mut H,
    semantic: F,
    pending: &'a mut HashMap<String, (Instant, Semantic)>,
    status_pending: &'a mut HashMap<String, (Instant, Semantic)>,
}
#[derive(Default)]
enum State {
    #[default]
    Ground,
    Escape,
    String,
    StringEscape,
    Osc,
    OscEscape,
    Overflow,
    OverflowEscape,
}
pub(crate) struct ShellProcessor {
    processor: Processor<StdSyncHandler>,
    state: State,
    held: Vec<u8>,
    overflow_metadata: bool,
    /// An OSC 7501 report past the protocol's limit is being swallowed.
    overflow_status: bool,
    pending: HashMap<String, (Instant, Semantic)>,
    /// OSC 7501 reports waiting to replay, under `STATUS_LIMIT`.
    status_pending: HashMap<String, (Instant, Semantic)>,
    /// The token of the last root report admitted, which a root report past
    /// the budget folds into.
    last_root: Option<String>,
    /// Reports dropped or folded past the budget, ever.
    status_lost: u64,
    nonce: String,
    sequence: u64,
    #[cfg(feature = "adaptive-terminal-refresh")]
    visible_advanced: bool,
}
impl ShellProcessor {
    /// Nonce is a fresh random session UUID, never exposed to the child.
    pub(crate) fn new(nonce: String) -> Self {
        Self {
            processor: Processor::new(),
            state: State::Ground,
            held: Vec::new(),
            overflow_metadata: false,
            overflow_status: false,
            pending: HashMap::new(),
            status_pending: HashMap::new(),
            last_root: None,
            status_lost: 0,
            nonce,
            sequence: 0,
            #[cfg(feature = "adaptive-terminal-refresh")]
            visible_advanced: false,
        }
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(crate) fn visible_advanced(&self) -> bool {
        self.visible_advanced
    }
    pub(crate) fn sync_timeout(&self) -> &StdSyncHandler {
        self.processor.sync_timeout()
    }
    pub(crate) fn sync_bytes_count(&self) -> usize {
        self.processor.sync_bytes_count()
    }
    pub(crate) fn stop_sync<H: Handler>(
        &mut self,
        handler: &mut H,
        semantic: impl FnMut(&mut H, Semantic),
    ) {
        self.processor.stop_sync(&mut Forward {
            handler,
            semantic,
            pending: &mut self.pending,
            status_pending: &mut self.status_pending,
        });
    }
    pub(crate) fn advance<H: Handler>(
        &mut self,
        handler: &mut H,
        input: &[u8],
        semantic: impl FnMut(&mut H, Semantic),
    ) {
        let mut bytes = Vec::with_capacity(input.len());
        for &b in input {
            self.normalize(b, &mut bytes);
        }
        let buffered_before = self.sync_bytes_count();
        self.processor.advance(
            &mut Forward {
                handler,
                semantic,
                pending: &mut self.pending,
                status_pending: &mut self.status_pending,
            },
            &bytes,
        );
        #[cfg(feature = "adaptive-terminal-refresh")]
        {
            self.visible_advanced = self.sync_bytes_count() < buffered_before + bytes.len();
        }
        #[cfg(not(feature = "adaptive-terminal-refresh"))]
        let _ = buffered_before;
    }
    fn finish(&mut self, output: &mut Vec<u8>) {
        let end = if self.held.ends_with(b"\x1b\\") { 2 } else { 1 };
        let payload = &self.held[2..self.held.len() - end];
        // OSC 7501 is the program's own status: a valid report becomes an
        // event in byte order, and anything else — the probe's `?`
        // included — is dropped, since nothing else reads it.
        if let Some(payload) = payload.strip_prefix(b"7501;") {
            if self.held.len() <= super::program_status::SEQUENCE_LIMIT
                && let Some(report) = super::program_status::parse_report(payload)
            {
                self.enqueue_status(report, output);
            }
            self.held.clear();
            self.state = State::Ground;
            return;
        }
        let event = if let Some(payload) = payload.strip_prefix(b"7;") {
            Some(
                std::str::from_utf8(payload)
                    .map(|p| Semantic::Cwd(p.into()))
                    .unwrap_or(Semantic::Revoke),
            )
        } else {
            payload.strip_prefix(b"133;").map(|payload| {
                std::str::from_utf8(payload)
                    .map(|p| Semantic::Command(p.into()))
                    .unwrap_or(Semantic::Revoke)
            })
        };
        if let Some(event) = event {
            self.enqueue(event, output);
        } else {
            output.extend_from_slice(&self.held);
        }
        self.held.clear();
        self.state = State::Ground;
    }
    /// An OSC 7501 report, by its own budget. Past it a child report is
    /// dropped — children are kept, not shown — and a root report replaces
    /// the last admitted root report's event, so the frame's final root
    /// state still replays; either way the loss is counted and logged.
    fn enqueue_status(&mut self, report: super::program_status::Report, output: &mut Vec<u8>) {
        let root = report.id.is_none();
        if self.status_pending.len() >= STATUS_LIMIT {
            let folded = root
                && self
                    .last_root
                    .as_ref()
                    .and_then(|key| self.status_pending.get_mut(key))
                    .map(|slot| slot.1 = Semantic::ProgramStatus(report.clone()))
                    .is_some();
            self.status_lost += 1;
            if self.status_lost.is_power_of_two() {
                tracing::warn!(
                    lost = self.status_lost,
                    "OSC 7501 reports overflowed a synchronized frame; the last root state is kept"
                );
            }
            if folded || !root {
                return;
            }
        }
        let key = self.token();
        if root {
            self.last_root = Some(key.clone());
        }
        self.status_pending.insert(
            key.clone(),
            (Instant::now(), Semantic::ProgramStatus(report)),
        );
        Self::title(&key, output);
    }
    fn token(&mut self) -> String {
        let key = format!("{}:{}", self.nonce, self.sequence);
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("terminal metadata sequence exhausted");
        key
    }
    fn title(key: &str, output: &mut Vec<u8>) {
        output.extend_from_slice(b"\x1b]2;");
        output.extend_from_slice(key.as_bytes());
        output.push(7);
    }
    fn enqueue(&mut self, event: Semantic, output: &mut Vec<u8>) {
        if self.pending.len() >= LIMIT {
            return;
        }
        let event = if self.pending.len() == LIMIT - 1 {
            Semantic::Revoke
        } else {
            event
        };
        let key = self.token();
        self.pending.insert(key.clone(), (Instant::now(), event));
        Self::title(&key, output);
    }
    fn normalize(&mut self, b: u8, output: &mut Vec<u8>) {
        use State::*;
        match self.state {
            Ground => {
                if b == 27 {
                    self.state = Escape;
                } else {
                    output.push(b);
                }
            }
            Escape => match b {
                b']' => {
                    self.held.extend_from_slice(b"\x1b]");
                    self.state = Osc;
                }
                b'P' | b'X' | b'^' | b'_' => {
                    output.extend_from_slice(&[27, b]);
                    self.state = String;
                }
                27 => output.push(27),
                _ => {
                    output.extend_from_slice(&[27, b]);
                    self.state = Ground;
                }
            },
            String => {
                output.push(b);
                if b == 27 {
                    self.state = StringEscape;
                } else if b == 24 || b == 26 {
                    self.state = Ground;
                }
            }
            StringEscape => {
                output.push(b);
                self.state = if b == b'\\' {
                    Ground
                } else if b == 27 {
                    StringEscape
                } else {
                    String
                };
            }
            Osc | OscEscape => {
                let escaped = matches!(self.state, OscEscape);
                self.held.push(b);
                if (!escaped && b == 7) || (escaped && b == b'\\') {
                    self.finish(output);
                } else if b == 24 || b == 26 || (escaped && b != 27) {
                    output.append(&mut self.held);
                    self.state = Ground;
                } else if self.held.len() > super::program_status::SEQUENCE_LIMIT
                    && self.held.starts_with(b"\x1b]7501;")
                {
                    // Past the protocol's limit: discarded whole, never shown.
                    self.held.clear();
                    self.overflow_status = true;
                    self.state = if b == 27 { OverflowEscape } else { Overflow };
                } else if self.held.len() >= PAYLOAD {
                    self.overflow_metadata =
                        self.held.starts_with(b"\x1b]7;") || self.held.starts_with(b"\x1b]133;");
                    if self.overflow_metadata {
                        self.held.clear();
                    } else {
                        output.append(&mut self.held);
                    }
                    self.state = if b == 27 { OverflowEscape } else { Overflow };
                } else {
                    self.state = if b == 27 { OscEscape } else { Osc };
                }
            }
            Overflow | OverflowEscape => {
                let escaped = matches!(self.state, OverflowEscape);
                if !self.overflow_metadata && !self.overflow_status {
                    output.push(b);
                }
                self.state = if b == 7 || b == 24 || b == 26 || (escaped && b == b'\\') {
                    if self.overflow_metadata {
                        self.enqueue(Semantic::Revoke, output);
                        self.overflow_metadata = false;
                    }
                    self.overflow_status = false;
                    Ground
                } else if b == 27 {
                    OverflowEscape
                } else {
                    Overflow
                };
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        event::{Event, EventListener},
        grid::Dimensions,
        term::{Config, Term},
    };
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Listener(Arc<Mutex<Vec<String>>>);
    impl EventListener for Listener {
        fn send_event(&self, event: Event) {
            match event {
                Event::Title(s) => self.0.lock().unwrap().push(format!("title:{s}")),
                Event::PtyWrite(s) => self.0.lock().unwrap().push(format!("reply:{s}")),
                _ => {}
            }
        }
    }
    struct Size;
    impl Dimensions for Size {
        fn columns(&self) -> usize {
            80
        }
        fn screen_lines(&self) -> usize {
            24
        }
        fn total_lines(&self) -> usize {
            24
        }
    }
    fn fixture() -> (Term<Listener>, Arc<Mutex<Vec<String>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (
            Term::new(Config::default(), &Size, Listener(events.clone())),
            events,
        )
    }
    #[test]
    fn replay_orders_metadata_and_preserves_normal_titles_and_reports() {
        for split in 1..70 {
            let mut p = ShellProcessor::new("private-nonce".into());
            let (mut t, events) = fixture();
            let mut boundaries = Vec::new();
            let bytes =
                b"\x1b[?2026hbefore\r\n\x1b]133;C\x07after\r\n\x1b[?2026l\x1b]2;normal\x07\x1b[5n";
            for part in bytes.chunks(split) {
                p.advance(&mut t, part, |term, event| {
                    if matches!(event, Semantic::Command(_)) {
                        boundaries.push(term.grid().cursor.point.line.0)
                    }
                });
            }
            assert_eq!(boundaries, [1]);
            assert_eq!(t.grid().cursor.point.line.0, 2);
            assert_eq!(*events.lock().unwrap(), ["title:normal", "reply:\x1b[0n"]);
            assert!(p.pending.is_empty() && p.status_pending.is_empty());
        }
    }
    #[test]
    fn timeout_replay_and_bogus_titles_do_not_dispatch_metadata() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, events) = fixture();
        let mut boundaries = Vec::new();
        p.advance(
            &mut t,
            b"\x1b]2;bogus:0\x07\x1b[?2026h\n\x1b]7;file:///tmp\x1b\\",
            |_, _| {},
        );
        p.stop_sync(&mut t, |term, event| {
            if matches!(event, Semantic::Cwd(_)) {
                boundaries.push(term.grid().cursor.point.line.0)
            }
        });
        assert_eq!(boundaries, [1]);
        assert_eq!(*events.lock().unwrap(), ["title:bogus:0"]);
        p.advance(&mut t, b"\x1b]2;private-nonce:0\x07", |_, event| {
            assert!(!matches!(event, Semantic::Cwd(_) | Semantic::Command(_)))
        });
        assert_eq!(events.lock().unwrap().len(), 2);
    }
    #[test]
    fn floods_and_unterminated_payloads_are_bounded() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, _) = fixture();
        let mut revoked = 0;
        p.advance(&mut t, &b"\x1b]133;C\x07".repeat(10000), |_, event| {
            if event == Semantic::Revoke {
                revoked += 1
            }
        });
        assert_eq!(revoked, 1);
        assert!(p.pending.is_empty() && p.status_pending.is_empty());
        p.advance(&mut t, b"\x1b]7;", |_, _| {});
        p.advance(&mut t, &vec![b'a'; PAYLOAD * 3], |_, _| {});
        assert!(p.held.len() <= PAYLOAD);
        assert!(p.pending.len() <= LIMIT);
    }
    #[test]
    fn expired_token_is_consumed_without_becoming_a_title() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, events) = fixture();
        p.advance(&mut t, b"\x1b[?2026h\x1b]7;file:///tmp\x07", |_, _| {});
        for (created, _) in p.pending.values_mut() {
            *created = Instant::now() - std::time::Duration::from_secs(2);
        }
        let mut revoked = false;
        p.stop_sync(&mut t, |_, event| revoked |= event == Semantic::Revoke);
        assert!(revoked);
        assert!(p.pending.is_empty() && p.status_pending.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }
    /// A status report held past the token's second — a synchronized frame
    /// that took long — is still delivered: it carries no screen position
    /// to go stale, and losing it would leave the session's state behind.
    #[test]
    fn an_expired_status_report_is_still_delivered() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, events) = fixture();
        p.advance(&mut t, b"\x1b[?2026h\x1b]7501;state=working\x07", |_, _| {});
        for (created, _) in p.pending.values_mut() {
            *created = Instant::now() - std::time::Duration::from_secs(2);
        }
        let mut heard = Vec::new();
        p.stop_sync(&mut t, |_, event| {
            if matches!(event, Semantic::ProgramStatus(_) | Semantic::Revoke) {
                heard.push(event)
            }
        });
        assert_eq!(
            heard,
            [status(crate::terminal::program_status::State::Working)]
        );
        assert!(p.pending.is_empty() && p.status_pending.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }
    /// **A prompt held past the token's second still ends what was working**
    /// — review r1, finding 5. Its position has gone stale and is revoked,
    /// but "a shell prompt began" is no position: the program that was
    /// working or blocked is not in front any more.
    #[test]
    fn an_expired_prompt_still_ends_active_records_without_its_position() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, _) = fixture();
        let mut state =
            crate::terminal::shell_state::ShellState::new(std::path::Path::new("/"), String::new());
        p.advance(
            &mut t,
            b"\x1b]7501;state=working\x07\x1b]7501;state=blocked:id=a\x07\x1b]7501;state=done:id=b\x07\x1b[?2026h\x1b]133;A\x07",
            |t, e| state.observe(t, e),
        );
        assert!(
            state.status_records().root().is_some(),
            "the frame holds the prompt"
        );
        for (created, _) in p.pending.values_mut() {
            *created = Instant::now() - std::time::Duration::from_secs(2);
        }
        let mut heard = Vec::new();
        p.stop_sync(&mut t, |t, e| {
            heard.push(e.clone());
            state.observe(t, e)
        });
        assert!(state.status_records().root().is_none(), "{heard:?}");
        assert!(state.status_records().child("a").is_none());
        assert!(state.status_records().child("b").is_some(), "done stays");
        assert!(
            heard.contains(&Semantic::Revoke),
            "the stale position is revoked"
        );
        assert!(
            !heard.iter().any(|e| matches!(e, Semantic::Command(_))),
            "no command boundary at a stale position: {heard:?}"
        );
    }
    /// **A frame with more reports than the token budget keeps the last root
    /// state, and says what it dropped** — review r1, finding 4. Child records
    /// past the budget are dropped; root reports past it fold into the last
    /// admitted root slot, so the final state is what replays.
    #[test]
    fn an_overflowing_frame_keeps_the_last_root_state_and_counts_the_loss() {
        use crate::terminal::program_status::State;
        let mut state =
            crate::terminal::shell_state::ShellState::new(std::path::Path::new("/"), String::new());
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, _) = fixture();
        let mut bytes = b"\x1b]7501;state=working\x07\x1b[?2026h".to_vec();
        for _ in 0..2000 {
            bytes.extend_from_slice(b"\x1b]7501;state=working:id=sub\x07");
        }
        bytes.extend_from_slice(b"\x1b]7501;state=done\x07\x1b[?2026l");
        p.advance(&mut t, &bytes, |t, e| state.observe(t, e));
        assert_eq!(
            state.status_records().root().map(|r| r.state),
            Some(State::Done)
        );
        assert!(p.status_lost > 0, "the dropped child reports are counted");

        let mut state =
            crate::terminal::shell_state::ShellState::new(std::path::Path::new("/"), String::new());
        let mut p = ShellProcessor::new("private-nonce".into());
        let mut bytes = b"\x1b[?2026h".to_vec();
        for n in 0..1500 {
            bytes.extend_from_slice(if n % 2 == 0 {
                b"\x1b]7501;state=working\x07"
            } else {
                b"\x1b]7501;state=idle\x07"
            });
        }
        bytes.extend_from_slice(b"\x1b]7501;state=error\x07\x1b]7;file:///tmp\x07\x1b[?2026l");
        let mut cwd = false;
        p.advance(&mut t, &bytes, |t, e| {
            cwd |= matches!(e, Semantic::Cwd(_));
            state.observe(t, e)
        });
        assert_eq!(
            state.status_records().root().map(|r| r.state),
            Some(State::Error)
        );
        assert!(p.status_lost > 0);
        assert!(cwd, "status reports do not crowd shell metadata out");
        assert!(p.pending.is_empty() && p.status_pending.is_empty());
    }
    #[test]
    fn oversized_shell_metadata_never_reaches_the_ansi_payload_buffer() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let mut output = Vec::new();
        for b in b"\x1b]7;file://remote/"
            .iter()
            .copied()
            .chain(std::iter::repeat_n(b'x', PAYLOAD * 10))
            .chain([7])
        {
            p.normalize(b, &mut output);
        }
        assert!(output.len() < 128);
        assert!(p.held.is_empty());
        assert_eq!(p.pending.len(), 1);
        assert!(
            p.pending
                .values()
                .any(|(_, event)| *event == Semantic::Revoke)
        );
    }
    fn status(state: crate::terminal::program_status::State) -> Semantic {
        Semantic::ProgramStatus(crate::terminal::program_status::Report {
            state,
            id: None,
            kind: None,
            progress: None,
            app: None,
            title: None,
            msg: None,
        })
    }
    fn text(t: &Term<Listener>) -> String {
        t.grid().display_iter().map(|cell| cell.c).collect()
    }
    /// An OSC 7501 report lands where it was written: after the screen
    /// content before it, beside OSC 7 and OSC 133 in byte order, inside a
    /// synchronized frame with that frame — at every split of the bytes.
    #[test]
    fn a_program_status_report_is_an_ordered_semantic_event() {
        use crate::terminal::program_status::State;
        let bytes: &[u8] = b"\x1b]7;file:///tmp\x07one\r\n\x1b]7501;state=working\x1b\\\x1b[?2026htwo\r\n\x1b]7501;state=done\x07\x1b]133;A\x07\x1b[?2026l";
        for split in 1..bytes.len() {
            let mut p = ShellProcessor::new("private-nonce".into());
            let (mut t, events) = fixture();
            let mut seen = Vec::new();
            for part in bytes.chunks(split) {
                p.advance(&mut t, part, |term, event| {
                    if matches!(
                        event,
                        Semantic::Cwd(_) | Semantic::Command(_) | Semantic::ProgramStatus(_)
                    ) {
                        seen.push((term.grid().cursor.point.line.0, event));
                    }
                });
            }
            assert_eq!(
                seen,
                [
                    (0, Semantic::Cwd("file:///tmp".into())),
                    (1, status(State::Working)),
                    (2, status(State::Done)),
                    (2, Semantic::Command("A".into())),
                ],
                "split {split}"
            );
            assert!(events.lock().unwrap().is_empty(), "no title, no reply");
            assert!(p.pending.is_empty() && p.status_pending.is_empty());
        }
    }
    /// The probe and invalid reports change nothing: no event, nothing on
    /// the screen, no title; the output around them is intact.
    #[test]
    fn the_probe_and_invalid_reports_change_nothing() {
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, events) = fixture();
        let mut heard = Vec::new();
        p.advance(
            &mut t,
            b"a\x1b]7501;?\x07b\x1b]7501;state=busy\x1b\\c\x1b]7501;state=idle:id=a//b\x07d",
            |_, event| heard.push(event),
        );
        assert!(
            !heard
                .iter()
                .any(|e| matches!(e, Semantic::ProgramStatus(_) | Semantic::Revoke)),
            "{heard:?}"
        );
        assert!(text(&t).starts_with("abcd"), "{:?}", text(&t));
        assert!(events.lock().unwrap().is_empty());
        assert!(p.pending.is_empty() && p.status_pending.is_empty());
    }
    /// A report longer than the protocol's 4096 bytes is discarded whole,
    /// split or not, and the output after it is untouched.
    #[test]
    fn an_over_long_report_is_discarded_without_disturbing_the_output() {
        let padding = "a".repeat(crate::terminal::program_status::SEQUENCE_LIMIT);
        let bytes = format!("x\x1b]7501;state=idle:app=codex:pad={padding}\x07y");
        for split in [1, 7, 4095, 4096, bytes.len()] {
            let mut p = ShellProcessor::new("private-nonce".into());
            let (mut t, events) = fixture();
            let mut heard = Vec::new();
            for part in bytes.as_bytes().chunks(split) {
                p.advance(&mut t, part, |_, event| heard.push(event));
            }
            assert!(
                !heard
                    .iter()
                    .any(|e| matches!(e, Semantic::ProgramStatus(_) | Semantic::Revoke)),
                "split {split}: {heard:?}"
            );
            assert!(text(&t).starts_with("xy"), "split {split}");
            assert!(events.lock().unwrap().is_empty());
            assert!(p.held.len() <= crate::terminal::program_status::SEQUENCE_LIMIT);
        }
        // Exactly at the limit, it is a report.
        let fits = "a".repeat(crate::terminal::program_status::SEQUENCE_LIMIT - 23);
        let bytes = format!("\x1b]7501;state=idle:pad={fits}\x07");
        assert_eq!(bytes.len(), crate::terminal::program_status::SEQUENCE_LIMIT);
        let mut p = ShellProcessor::new("private-nonce".into());
        let (mut t, _) = fixture();
        let mut heard = Vec::new();
        p.advance(&mut t, bytes.as_bytes(), |_, event| heard.push(event));
        assert!(heard.contains(&status(crate::terminal::program_status::State::Idle)));
    }
    #[test]
    fn public_mouse_cursor_callback_is_forwarded() {
        #[derive(Default)]
        struct Target(bool);
        impl Handler for Target {
            fn set_mouse_cursor_icon(&mut self, icon: cursor_icon::CursorIcon) {
                self.0 = icon == cursor_icon::CursorIcon::Pointer;
            }
        }
        let mut processor = ShellProcessor::new("test-nonce".into());
        let mut target = Target::default();
        processor.advance(&mut target, b"\x1b]22;pointer\x07", |_, _| {});
        assert!(target.0);
    }
}
