//! Parser-owned shell metadata. Missing location authority never falls back to
//! a later cwd. Structural changes may revoke locations conservatively.
use alacritty_terminal::index::{Line, Point};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug)]
struct Span {
    start: usize,
    end: usize,
    cwd: Arc<Option<PathBuf>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Command {
    pub prompt: Point,
    pub input: Option<Point>,
    pub output: Option<Point>,
    pub end: Option<Point>,
    pub status: Option<i32>,
}
pub(crate) struct ShellState {
    launch: PathBuf,
    current: Arc<Option<PathBuf>>,
    locations: BTreeMap<i64, Vec<Span>>,
    // Stable row = grid row + origin; scrolling never rewrites retained rows.
    origin: i64,
    partial_region: bool,
    // Commands retain their capture origin; translate only when a caller asks.
    commands: VecDeque<(Command, i64)>,
    pub integrated: bool,
    pub navigation: Option<(Point, usize)>,
    local_hostname: String,
    before: Option<(Point, bool, usize, usize)>,
    alternate: bool,
    /// What the program says it is doing — OSC 7501.
    records: super::program_status::Records,
}
impl ShellState {
    /// The program's status records — OSC 7501.
    pub fn status_records(&self) -> &super::program_status::Records {
        &self.records
    }
    /// The root record's changes since the last take — OSC 7501.
    pub fn take_status_changes(&mut self) -> (Vec<super::program_status::RootChange>, u64) {
        self.records.take_changes()
    }
    /// The process exited: nothing it reported is still working or blocked.
    pub fn process_exited(&mut self) {
        self.records.end_active();
    }
    pub fn new(cwd: &Path, hostname: String) -> Self {
        Self {
            launch: cwd.into(),
            current: Arc::new(Some(cwd.into())),
            locations: BTreeMap::new(),
            commands: VecDeque::new(),
            origin: 0,
            partial_region: false,
            integrated: false,
            navigation: None,
            local_hostname: hostname,
            before: None,
            alternate: false,
            records: Default::default(),
        }
    }
    pub fn observe<T: alacritty_terminal::event::EventListener>(
        &mut self,
        term: &mut alacritty_terminal::term::Term<T>,
        event: super::semantic::Semantic,
    ) {
        use super::semantic::Semantic;
        use alacritty_terminal::grid::Dimensions;
        let grid = term.grid();
        match event {
            Semantic::Cwd(uri) => self.apply_cwd(&uri),
            Semantic::ProgramStatus(report) => self.records.apply(report),
            Semantic::Reset => self.records.reset(),
            Semantic::PromptStarted => self.records.end_active(),
            Semantic::Command(payload) => {
                // A prompt beginning means the program that was working or
                // blocked is no longer in front — on either screen.
                if payload.starts_with('A') {
                    self.records.end_active();
                }
                if !term
                    .mode()
                    .contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
                {
                    self.command(&payload, grid.cursor.point);
                }
            }
            Semantic::BeforeInput | Semantic::BeforeLinefeed => {
                if self.partial_region {
                    self.invalidate();
                }
                self.before = Some((
                    grid.cursor.point,
                    grid.cursor.input_needs_wrap,
                    term.history_size(),
                    term.screen_lines(),
                ));
            }
            Semantic::AfterInput | Semantic::AfterLinefeed => {
                let Some((before, wrapped, history, lines)) = self.before.take() else {
                    return;
                };
                let at = grid.cursor.point;
                let linefeed = matches!(event, Semantic::AfterLinefeed);
                let scrolled = term.history_size() > history
                    || (before.line.0 == lines as i32 - 1
                        && at.line == before.line
                        && (linefeed || at.column.0 < before.column.0));
                if scrolled {
                    self.scroll(1, term.history_size());
                } else if linefeed && at.line == before.line {
                    self.invalidate();
                }
                if !linefeed {
                    if at == before && (wrapped || !grid.cursor.input_needs_wrap) {
                        self.locations.remove(&(self.origin + i64::from(at.line.0)));
                        return;
                    }
                    let start = if wrapped || at.column.0 < before.column.0 {
                        Point::new(at.line, alacritty_terminal::index::Column(0))
                    } else {
                        before
                    };
                    let width = if grid[start]
                        .flags
                        .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR)
                    {
                        2
                    } else {
                        1
                    };
                    if term
                        .mode()
                        .contains(alacritty_terminal::term::TermMode::INSERT)
                    {
                        // Input may first wrap into another row.
                        self.locations
                            .remove(&(self.origin + i64::from(start.line.0)));
                    }
                    self.write(start, width);
                }
            }
            Semantic::Region(top, bottom) => {
                if top < bottom.unwrap_or(term.screen_lines()) {
                    self.partial_region =
                        top != 1 || bottom.is_some_and(|b| b != term.screen_lines());
                    self.invalidate();
                }
            }
            Semantic::Invalidate => self.invalidate(),
            Semantic::ClearLine => {
                self.locations
                    .remove(&(self.origin + i64::from(grid.cursor.point.line.0)));
            }
            Semantic::ClearBelow => {
                self.locations
                    .retain(|line, _| *line < self.origin + i64::from(grid.cursor.point.line.0));
                self.commands.retain(|(c, origin)| {
                    (i64::from(c.prompt.line.0) + origin, c.prompt.column)
                        <= (
                            self.origin + i64::from(grid.cursor.point.line.0),
                            grid.cursor.point.column,
                        )
                });
            }
            Semantic::ClearAbove => {
                self.locations.retain(|line, _| {
                    *line < self.origin || *line > self.origin + i64::from(grid.cursor.point.line.0)
                });
                self.commands.retain(|(c, origin)| {
                    let line = i64::from(c.prompt.line.0) + origin;
                    line < self.origin
                        || (line, c.prompt.column)
                            >= (
                                self.origin + i64::from(grid.cursor.point.line.0),
                                grid.cursor.point.column,
                            )
                });
            }
            Semantic::Revoke => {
                self.invalidate();
                self.current = Arc::new(None);
            }
            Semantic::CheckScreen => {
                let alternate = term
                    .mode()
                    .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
                if alternate != self.alternate {
                    self.invalidate();
                    self.alternate = alternate;
                }
            }
        }
    }
    pub fn cwd(&self) -> Option<&Path> {
        self.current.as_ref().as_deref()
    }
    pub fn apply_cwd(&mut self, uri: &str) {
        self.integrated = true;
        self.current = Arc::new(local_cwd(uri, &self.local_hostname));
    }
    pub fn command(&mut self, payload: &str, at: Point) {
        self.integrated = true;
        let mut parts = payload.split(';');
        match parts.next() {
            Some("A") => {
                while self.commands.back().is_some_and(|(c, origin)| {
                    (i64::from(c.prompt.line.0) + origin, c.prompt.column)
                        > (self.origin + i64::from(at.line.0), at.column)
                }) {
                    self.commands.pop_back();
                }
                if self
                    .commands
                    .back()
                    .is_some_and(|(c, _)| c.output.is_none())
                {
                    self.commands.pop_back();
                }
                if self.commands.len() >= 10000 {
                    self.commands.pop_front();
                }
                self.commands.push_back((
                    Command {
                        prompt: at,
                        input: None,
                        output: None,
                        end: None,
                        status: None,
                    },
                    self.origin,
                ));
            }
            Some("B") => {
                if let Some((command, origin)) = self.commands.back_mut()
                    && command.input.is_none()
                    && command.output.is_none()
                {
                    command.input = Some(Point::new(
                        Line((i64::from(at.line.0) + self.origin - *origin) as i32),
                        at.column,
                    ));
                }
            }
            Some("C") => {
                if let Some((c, origin)) = self.commands.back_mut()
                    && c.output.is_none()
                    && c.end.is_none()
                {
                    c.output = Some(Point::new(
                        Line((i64::from(at.line.0) + self.origin - *origin) as i32),
                        at.column,
                    ));
                }
            }
            Some("D") => {
                if let Some((c, origin)) = self.commands.back_mut()
                    && c.output.is_some()
                    && c.end.is_none()
                {
                    c.end = Some(Point::new(
                        Line((i64::from(at.line.0) + self.origin - *origin) as i32),
                        at.column,
                    ));
                    c.status = parts.next().and_then(|v| v.parse().ok());
                }
            }
            _ => {}
        }
    }
    pub fn write(&mut self, at: Point, width: usize) {
        if !self.integrated {
            return;
        }
        let start = at.column.0;
        let end = start.saturating_add(width);
        let spans = self
            .locations
            .entry(self.origin + i64::from(at.line.0))
            .or_default();
        if let Some(last) = spans.last_mut()
            && last.end == start
            && last.cwd == self.current
        {
            last.end = end;
            return;
        }
        let mut next = Vec::with_capacity(spans.len() + 1);
        for s in spans.drain(..) {
            if s.end <= start || s.start >= end {
                next.push(s);
                continue;
            }
            if s.start < start {
                next.push(Span {
                    end: start,
                    ..s.clone()
                });
            }
            if s.end > end {
                next.push(Span { start: end, ..s });
            }
        }
        next.push(Span {
            start,
            end,
            cwd: self.current.clone(),
        });
        next.sort_by_key(|s| s.start);
        for span in next {
            if let Some(last) = spans.last_mut()
                && last.end == span.start
                && last.cwd == span.cwd
            {
                last.end = span.end;
            } else {
                spans.push(span);
            }
        }
    }
    pub fn cwd_at(&self, from: Point, to: Point) -> Option<&Path> {
        if !self.integrated {
            return Some(&self.launch);
        }
        // Multi-row links require one unambiguous authority on every covered row.
        let mut found: Option<&Option<PathBuf>> = None;
        for line in from.line.0..=to.line.0 {
            let start = if line == from.line.0 {
                from.column.0
            } else {
                0
            };
            let end = if line == to.line.0 {
                to.column.0 + 1
            } else {
                usize::MAX
            };
            let spans = self.locations.get(&(self.origin + i64::from(line)))?;
            let mut covered = start;
            for span in spans.iter().filter(|s| s.end > start && s.start < end) {
                if span.start > covered {
                    return None;
                }
                if let Some(previous) = found {
                    if previous != span.cwd.as_ref() {
                        return None;
                    }
                } else {
                    found = Some(span.cwd.as_ref());
                }
                covered = span.end;
            }
            if end != usize::MAX && covered < end {
                return None;
            }
        }
        found?.as_deref()
    }
    pub fn scroll(&mut self, rows: i32, history: usize) {
        if let Some((point, offset)) = &mut self.navigation {
            point.line -= rows;
            if *offset > 0 {
                *offset = offset.saturating_add(rows as usize);
            }
            if point.line.0 < -(history as i32) {
                self.navigation = None;
            }
        }
        self.origin += i64::from(rows);
        let oldest = self.origin - history as i64;
        while self
            .locations
            .first_key_value()
            .is_some_and(|(line, _)| *line < oldest)
        {
            self.locations.pop_first();
        }
        while self
            .commands
            .front()
            .is_some_and(|(c, origin)| i64::from(c.prompt.line.0) + origin < oldest)
        {
            self.commands.pop_front();
        }
    }
    pub fn commands(&self) -> Vec<Command> {
        self.commands
            .iter()
            .map(|(command, origin)| {
                let mut c = command.clone();
                let translate = |p: Point| {
                    Point::new(
                        Line((i64::from(p.line.0) + origin - self.origin) as i32),
                        p.column,
                    )
                };
                c.input = c.input.map(translate);
                c.output = c.output.map(translate);
                c.end = c.end.map(translate);
                c.prompt = translate(c.prompt);
                c
            })
            .collect()
    }

    pub fn invalidate(&mut self) {
        self.integrated = true;
        self.locations.clear();
        self.commands.clear();
        self.navigation = None;
    }
}
fn local_cwd(uri: &str, hostname: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let slash = rest.find('/')?;
    let authority = &rest[..slash];
    if !authority.is_empty()
        && authority != "localhost"
        && (hostname.is_empty() || !authority.eq_ignore_ascii_case(hostname))
    {
        return None;
    }
    let path = &rest[slash..];
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1..index + 3]
                    .iter()
                    .all(u8::is_ascii_hexdigit)
            {
                return None;
            }
            index += 2;
        }
        index += 1;
    }
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    (!decoded.contains('\0') && Path::new(decoded.as_ref()).is_absolute())
        .then(|| PathBuf::from(decoded.as_ref()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::Column;
    fn p(line: i32, col: usize) -> Point {
        Point::new(Line(line), Column(col))
    }
    #[test]
    fn scroll_translates_all_command_positions_and_prunes_history() {
        let mut s = ShellState::new(Path::new("/launch"), String::new());
        s.apply_cwd("file:///one");
        s.write(p(0, 0), 4);
        s.command("A", p(0, 0));
        s.command("B", p(0, 2));
        s.scroll(1, 2);
        s.command("C", p(0, 0));
        s.command("D;0", p(1, 0));
        s.navigation = Some((p(-1, 0), 1));
        s.scroll(1, 2);
        let c = &s.commands()[0];
        assert_eq!(c.prompt, p(-2, 0));
        assert_eq!(c.input, Some(p(-2, 2)));
        assert_eq!(c.output, Some(p(-1, 0)));
        assert_eq!(c.end, Some(p(0, 0)));
        assert_eq!(s.navigation, Some((p(-2, 0), 2)));
        assert_eq!(s.cwd_at(p(-2, 0), p(-2, 3)), Some(Path::new("/one")));
        s.scroll(1, 2);
        assert!(s.commands().is_empty());
        assert!(s.navigation.is_none());
        assert!(s.cwd_at(p(-2, 0), p(-2, 3)).is_none());
    }
    #[test]
    fn old_output_keeps_cwd_and_remote_authority_never_becomes_local() {
        let mut s = ShellState::new(Path::new("/launch"), "host".into());
        s.apply_cwd("file://localhost/one%20dir");
        s.write(p(0, 0), 8);
        s.apply_cwd("file://host/two");
        s.write(p(1, 0), 8);
        assert_eq!(s.cwd_at(p(0, 0), p(0, 7)), Some(Path::new("/one dir")));
        assert_eq!(s.cwd_at(p(1, 0), p(1, 7)), Some(Path::new("/two")));
        s.scroll(1, 10);
        assert_eq!(s.cwd_at(p(-1, 0), p(-1, 7)), Some(Path::new("/one dir")));
        s.apply_cwd("file://remote/private");
        s.write(p(1, 0), 8);
        assert!(s.cwd().is_none());
        assert!(s.cwd_at(p(1, 0), p(1, 7)).is_none());
        s.invalidate();
        assert!(s.cwd_at(p(-1, 0), p(-1, 7)).is_none());
    }
    #[test]
    fn same_line_cwd_changes_and_status_are_not_guessed() {
        let mut s = ShellState::new(Path::new("/launch"), String::new());
        s.apply_cwd("file:///one");
        s.write(p(0, 0), 5);
        s.apply_cwd("file:///two");
        s.write(p(0, 5), 5);
        assert!(s.cwd_at(p(0, 0), p(0, 9)).is_none());
        s.command("D;0", p(0, 0));
        assert!(s.commands.is_empty());
        s.command("A", p(1, 0));
        s.command("C", p(2, 0));
        s.command("D;7", p(3, 0));
        assert_eq!(s.commands()[0].status, Some(7));
        s.scroll(10, 2);
        assert!(s.commands.is_empty());
        for bad in [
            "file://user@host/path",
            "file://localhost:22/path",
            "file:///bad%00path",
            "file:///bad%GG",
        ] {
            assert!(local_cwd(bad, "host").is_none());
        }
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    use crate::terminal::{emulator::GridSize, semantic::ShellProcessor};
    use alacritty_terminal::{
        event::VoidListener,
        index::Column,
        term::{Config, Term},
    };

    fn replayed(bytes: &[u8]) -> ShellState {
        let mut term = Term::new(Config::default(), &GridSize::new(30, 4), VoidListener);
        let mut state = ShellState::new(Path::new("/launch"), String::new());
        let mut processor = ShellProcessor::new("records".into());
        processor.advance(&mut term, bytes, |t, e| state.observe(t, e));
        state
    }

    /// RIS clears every record in byte order: a report before it in the
    /// same read is gone, a report after it stays. Matching `ESC c` before
    /// parsing would get the first case wrong.
    #[test]
    fn a_full_reset_clears_the_records_in_byte_order() {
        use crate::terminal::program_status::State;
        let state = replayed(b"\x1b]7501;state=done\x07\x1b]7501;state=working:id=a\x07\x1bc");
        assert_eq!(state.status_records().len(), 0);
        let state = replayed(b"\x1b]7501;state=working\x07\x1bc\x1b]7501;state=done\x07");
        assert_eq!(state.status_records().root().unwrap().state, State::Done);
        assert_eq!(state.status_records().len(), 1);
    }

    /// A shell prompt (OSC 133 A) means the program that was working is no
    /// longer running: its `working` and `blocked` records end, the others stay.
    #[test]
    fn a_prompt_ends_working_and_blocked_records() {
        let state = replayed(
            b"\x1b]7501;state=working\x07\x1b]7501;state=done:id=a\x07\x1b]7501;state=blocked:id=b\x07\x1b]133;A\x07",
        );
        assert!(state.status_records().root().is_none());
        assert!(state.status_records().child("a").is_some());
        assert!(state.status_records().child("b").is_none());
        let state = replayed(b"\x1b]7501;state=working\x07\x1b]133;C\x07");
        assert!(
            state.status_records().root().is_some(),
            "only the prompt's start ends them"
        );
    }

    #[test]
    fn partial_region_never_moves_authority_to_unchanged_text() {
        let mut term = Term::new(Config::default(), &GridSize::new(30, 4), VoidListener);
        let mut state = ShellState::new(Path::new("/launch"), String::new());
        let mut processor = ShellProcessor::new("region".into());
        processor.advance(
            &mut term,
            b"\x1b]7;file:///one\x07a.rs\r\n\x1b]7;file:///two\x07b.rs\x1b[2;4r\x1b[4;1H\n",
            |t, e| state.observe(t, e),
        );
        let cwd = state.cwd_at(
            Point::new(Line(0), Column(0)),
            Point::new(Line(0), Column(3)),
        );
        assert!(cwd.is_none() || cwd == Some(Path::new("/one")), "{cwd:?}");
    }

    #[test]
    fn insert_mode_never_assigns_shifted_text_another_authority() {
        let mut term = Term::new(Config::default(), &GridSize::new(30, 4), VoidListener);
        let mut state = ShellState::new(Path::new("/launch"), String::new());
        let mut processor = ShellProcessor::new("insert".into());
        processor.advance(
            &mut term,
            b"\x1b]7;file:///one\x07a.rs    \x1b]7;file:///two\x07b.rs    \r\x1b[4h12345678",
            |t, e| state.observe(t, e),
        );
        let cwd = state.cwd_at(
            Point::new(Line(0), Column(8)),
            Point::new(Line(0), Column(11)),
        );
        assert!(cwd.is_none() || cwd == Some(Path::new("/one")), "{cwd:?}");
    }

    #[test]
    #[ignore = "owned parser timing evidence; no wall-clock threshold"]
    fn retained_history_parser_workload() {
        let mut term = Term::new(Config::default(), &GridSize::new(80, 24), VoidListener);
        let mut state = ShellState::new(Path::new("/launch"), String::new());
        let mut processor = ShellProcessor::new("timing".into());
        processor.advance(&mut term, b"\x1b]7;file:///one\x07", |t, e| {
            state.observe(t, e)
        });
        for _ in 0..10024 {
            processor.advance(&mut term, b"file.rs\r\n", |t, e| state.observe(t, e));
        }
        let started = std::time::Instant::now();
        for _ in 0..2000 {
            processor.advance(&mut term, b"file.rs\r\n", |t, e| state.observe(t, e));
        }
        eprintln!(
            "10k history, 2000 parser output lines: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn synchronized_cwd_and_output_positions_survive_replay_and_history_scroll() {
        let mut term = Term::new(Config::default(), &GridSize::new(30, 3), VoidListener);
        let mut state = ShellState::new(Path::new("/launch"), String::new());
        let mut processor = ShellProcessor::new("private-test-nonce".into());
        let input=b"\x1b[?2026h\x1b]7;file:///one\x07old.rs:1\r\n\x1b]7;file:///two\x07new.rs:2\r\n\x1b]133;A\x07prompt\x1b[?2026l";
        for part in input.chunks(3) {
            processor.advance(&mut term, part, |term, event| state.observe(term, event));
        }
        let point = |line, col| Point::new(Line(line), Column(col));
        assert_eq!(
            state.cwd_at(point(0, 0), point(0, 7)),
            Some(Path::new("/one"))
        );
        assert_eq!(
            state.cwd_at(point(1, 0), point(1, 7)),
            Some(Path::new("/two"))
        );
        assert_eq!(state.commands()[0].prompt, point(2, 0));
        processor.advance(&mut term, b"\r\nnext", |term, event| {
            state.observe(term, event)
        });
        assert_eq!(
            state.cwd_at(point(-1, 0), point(-1, 7)),
            Some(Path::new("/one"))
        );
        assert_eq!(state.commands()[0].prompt, point(1, 0));
    }
}
