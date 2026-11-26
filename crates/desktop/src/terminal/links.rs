//! A link in a terminal's output — M5.12, task 11.
//!
//! A URL, an OSC 8 hyperlink or a `path[:line[:column]]`, found by a scanner
//! of ours over a logical line, and a path resolved against where the
//! terminal started and then against the project's root. Decisions 11 and
//! 12 of the plan.
//!
//! The scanner keeps URL and file rules separate. File candidates are
//! underlined only after the hover resolves them to regular files:
//!
//! - **Only `http` and `https` are URLs.** Other `scheme://` tokens and
//!   `mailto:`, `javascript:`, `vbscript:`, `data:`, `tel:`, `urn:`, `about:`
//!   (case-insensitive) are skipped to whitespace, a quote or an angle bracket.
//!   `file://` followed by an absolute path remains a file candidate; other
//!   colon-bearing text goes through the ordinary path rules.
//! - **A path needs a slash or an extension and a letter.** A location
//!   `:line[:column]` is optional; both numbers are one-based. Without a
//!   valid line, trailing sentence punctuation is excluded from the path.
//! - **A URL runs to whitespace, a quote, an angle bracket or a closing
//!   bracket it did not open**, and loses a trailing `.,;:!?` — the
//!   sentence's, not the address's.
//! - **A link an agent CLI wrapped itself is one link** across the rows it
//!   was broken onto, without their indents — a path only when the joined
//!   path names a file, a URL only when it fills its row to the edge and
//!   the next row goes on like an address. See [`at`].

use std::path::{Path, PathBuf};

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A web address, for the browser.
    Url(String),
    /// A file, optionally at a one-based line and column.
    File {
        path: PathBuf,
        line: Option<u32>,
        column: Option<u32>,
    },
}

/// A link found in a text: character offsets into it, end exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

/// What a `cmd`-click on a file leaves for the window to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clicked {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl Clicked {
    /// An existing HTML page without a source location opens in the browser.
    /// Resolve it by the same rules as editor links, including paths outside
    /// the project. A location deliberately keeps source navigation instead.
    pub fn browser_url(&self, cwd: &Path, root: &Path) -> Option<String> {
        if self.line.is_some()
            || !self.path.extension()?.to_str().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
            })
        {
            return None;
        }
        let path = match resolve(&self.path, cwd, root)? {
            Resolved::Inside(path) => root.canonicalize().ok()?.join(path),
            Resolved::Outside(path) => path,
        };
        // A file URL must escape spaces, Unicode and URL metacharacters in
        // OSC 8 filenames; concatenating `file://` would change their meaning.
        gpui::http_client::Url::from_file_path(path)
            .ok()
            .map(|url| url.into())
    }
}

/// Where a path that exists is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Inside the project: relative to its root, as a file tab's path is.
    Inside(PathBuf),
    /// Outside it: the absolute path, with every link followed.
    Outside(PathBuf),
}

const SCHEMES: [&str; 2] = ["https://", "http://"];
const FILE_SCHEME: &str = "file://";

/// Every link in a text, in the order they appear.
pub fn find(text: &str) -> Vec<Link> {
    let chars: Vec<char> = text.chars().collect();
    let urls = urls(&chars);

    let mut links = Vec::new();
    let mut urls_left = urls.iter().peekable();
    let mut scheme_checked_until = 0;
    let mut i = 0;
    while i < chars.len() {
        if let Some(url) = urls_left.peek()
            && url.start == i
        {
            i = url.end;
            links.push((*url).clone());
            urls_left.next();
            continue;
        }
        if unsupported_scheme_at(&chars, i, &mut scheme_checked_until) {
            while i < chars.len() && !uri_text_end(chars[i]) {
                i += 1;
            }
            // URLs embedded in a skipped URI token are not independent links.
            while urls_left.peek().is_some_and(|url| url.start < i) {
                urls_left.next();
            }
            continue;
        }
        let next_url = urls_left.peek().map_or(chars.len(), |url| url.start);
        match path_at(&chars, i, next_url) {
            Some(link) => {
                i = link.end;
                links.push(link);
            }
            None => i += 1,
        }
    }
    links
}

/// The URLs of a text.
fn urls(chars: &[char]) -> Vec<Link> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let scheme = SCHEMES
            .iter()
            .find(|scheme| at_word_start(chars, i) && starts_with(chars, i, scheme));
        let Some(scheme) = scheme else {
            i += 1;
            continue;
        };
        let body = i + scheme.len();
        let mut end = body;
        let mut open = Vec::new();
        while end < chars.len() {
            let ch = chars[end];
            if uri_text_end(ch) {
                break;
            }
            match ch {
                '(' | '[' | '{' => open.push(ch),
                ')' | ']' | '}' => {
                    let opener = match ch {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    if open.last() != Some(&opener) {
                        break;
                    }
                    open.pop();
                }
                _ => {}
            }
            end += 1;
        }
        while end > body && matches!(chars[end - 1], '.' | ',' | ';' | ':' | '!' | '?') {
            end -= 1;
        }
        if end > body {
            found.push(Link {
                start: i,
                end,
                target: Target::Url(chars[i..end].iter().collect()),
            });
            i = end;
        } else {
            i = body;
        }
    }
    found
}

/// Whether a token starts with an unsupported URI scheme. Located filenames
/// such as `Cargo.toml:7` keep their numeric suffix instead of becoming schemes.
fn unsupported_scheme_at(chars: &[char], i: usize, checked_until: &mut usize) -> bool {
    if i < *checked_until || !at_word_start(chars, i) || !chars[i].is_ascii_alphabetic() {
        return false;
    }
    let mut end = i + 1;
    while end < chars.len()
        && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '+' | '-' | '.'))
    {
        end += 1;
    }
    // Hyphens and dots are word boundaries too. Do not scan their suffixes
    // again when the complete run has already been checked for a colon.
    *checked_until = end;
    if starts_with(chars, end, "://") {
        return !(starts_with(chars, i, FILE_SCHEME) && chars.get(end + 3) == Some(&'/'));
    }
    [
        "mailto:",
        "javascript:",
        "vbscript:",
        "data:",
        "tel:",
        "urn:",
        "about:",
    ]
    .iter()
    .any(|scheme| starts_with(chars, i, scheme))
}

fn uri_text_end(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>')
}

fn has_extension(chars: &[char]) -> bool {
    chars
        .windows(2)
        .any(|pair| pair[0] == '.' && pair[1].is_alphanumeric())
}

/// A `path[:line[:column]]` beginning at `i`, ending before `limit`.
fn path_at(chars: &[char], i: usize, limit: usize) -> Option<Link> {
    let start = i;
    let mut run_start = i;
    if at_word_start(chars, i) && starts_with(chars, i, FILE_SCHEME) {
        run_start = i + FILE_SCHEME.len();
        if chars.get(run_start) != Some(&'/') {
            return None;
        }
    } else if !is_path_char(chars[i]) || (i > 0 && is_path_char(chars[i - 1])) {
        return None;
    } else if i > 0 && chars[i - 1] == ':' {
        // The tail of a scheme or a label — `ftp://…`, `ssh://…`, `note:…` —
        // and not a path somebody meant to open.
        return None;
    }

    let mut run_end = run_start;
    while run_end < limit && is_path_char(chars[run_end]) {
        run_end += 1;
    }
    let located = number_after_colon(chars, run_end, limit).filter(|(line, _)| *line > 0);
    let (line, column, end) = if let Some((line, after_line)) = located {
        let (column, end) = match number_after_colon(chars, after_line, limit) {
            Some((column, after)) if column > 0 => (Some(column), after),
            _ => (None, after_line),
        };
        (Some(line), column, end)
    } else {
        while run_end > run_start && chars[run_end - 1] == '.' {
            run_end -= 1;
        }
        (None, None, run_end)
    };
    let run = &chars[run_start..run_end];
    if !(run.contains(&'/') || has_extension(run)) || !run.iter().any(|ch| ch.is_alphabetic()) {
        return None;
    }

    Some(Link {
        start,
        end,
        target: Target::File {
            path: PathBuf::from(run.iter().collect::<String>()),
            line,
            column,
        },
    })
}

/// `:digits` at `at`, as a number and where it ends — provided the digits
/// do not run on into a letter, which makes them part of a word.
fn number_after_colon(chars: &[char], at: usize, limit: usize) -> Option<(u32, usize)> {
    if at >= limit || chars[at] != ':' {
        return None;
    }
    let digits = at + 1;
    let mut end = digits;
    while end < limit && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits || chars.get(end).is_some_and(|ch| ch.is_alphanumeric()) {
        return None;
    }
    let number: String = chars[digits..end].iter().collect();
    Some((number.parse().ok()?, end))
}

fn is_path_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | '~' | '+' | '@')
}

fn at_word_start(chars: &[char], i: usize) -> bool {
    i == 0 || !chars[i - 1].is_alphanumeric()
}

fn starts_with(chars: &[char], i: usize, word: &str) -> bool {
    let len = word.chars().count();
    chars.get(i..i + len).is_some_and(|here| {
        here.iter()
            .zip(word.chars())
            .all(|(ch, expected)| ch.eq_ignore_ascii_case(&expected))
    })
}

/// The logical line under a point: the rows a wrapped line spans, joined,
/// with the grid point of each character.
///
/// A wide character's spacer is not a character, so it has no entry; a
/// point on one belongs to the character before it.
pub fn logical_line<T>(term: &Term<T>, point: Point) -> (String, Vec<Point>) {
    let row = logical_row(term, point.line);
    (row.chars.into_iter().collect(), row.points)
}

/// A logical line's characters and the grid point of each.
struct Row {
    chars: Vec<char>,
    points: Vec<Point>,
}

/// The logical line holding a grid line: the rows joined by `WRAPLINE`.
fn logical_row<T>(term: &Term<T>, line: Line) -> Row {
    let grid = term.grid();
    let (top, bottom) = (grid.topmost_line(), grid.bottommost_line());
    let last = grid.last_column();
    let wraps = |line: Line| grid[line][last].flags.contains(Flags::WRAPLINE);

    let line = line.max(top).min(bottom);
    let mut first = line;
    while first > top && wraps(Line(first.0 - 1)) {
        first = Line(first.0 - 1);
    }
    let mut end = line;
    while end < bottom && wraps(end) {
        end = Line(end.0 + 1);
    }

    let mut chars = Vec::new();
    let mut points = Vec::new();
    for row in first.0..=end.0 {
        for column in 0..grid.columns() {
            let at = Point::new(Line(row), Column(column));
            let cell = &grid[at];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            chars.push(cell.c);
            points.push(at);
        }
    }
    Row { chars, points }
}

/// A link found in the grid: the cells it covers and where it leads.
///
/// The cells are kept as one run per row, first cell to last, because a
/// link an agent wrapped itself has the continuation's indent between two
/// of its rows, and the indent is not the link's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    runs: Vec<(Point, Point)>,
    pub target: Target,
}

impl Found {
    /// The link's cells, one run per row it covers, top to bottom.
    pub fn runs(&self) -> &[(Point, Point)] {
        &self.runs
    }

    /// The link's first cell.
    pub fn start(&self) -> Point {
        self.runs[0].0
    }

    /// The link's last cell.
    pub fn end(&self) -> Point {
        self.runs[self.runs.len() - 1].1
    }

    /// Whether a cell is one of the link's.
    pub fn covers(&self, point: Point) -> bool {
        self.runs
            .iter()
            .any(|(first, last)| *first <= point && point <= *last)
    }

    /// The link over `points`, a run for each row they are on.
    fn over(points: &[Point], target: Target) -> Self {
        let mut runs: Vec<(Point, Point)> = Vec::new();
        for point in points {
            match runs.last_mut() {
                Some((_, last)) if last.line == point.line => *last = *point,
                _ => runs.push((*point, *point)),
            }
        }
        Found { runs, target }
    }
}

/// The link under a point: an OSC 8 hyperlink first, because a program
/// that wrote one said what it meant, then whatever [`find`] sees on the
/// logical line — or over the rows an agent CLI wrapped the link onto
/// itself, see [`hard_wrapped`].
///
/// `joined_file_exists` is the disk's question, asked only of a file link
/// that runs across such a break: a joined path is kept only when it names
/// a file, and otherwise the rows are read one by one, as they always were.
pub fn at<T>(
    term: &Term<T>,
    point: Point,
    joined_file_exists: impl FnOnce(&Found) -> bool,
) -> Option<Found> {
    let row = logical_row(term, point.line);
    let index = row.points.iter().rposition(|at| *at <= point)?;

    let grid = term.grid();
    if let Some(hyperlink) = grid[row.points[index]].hyperlink() {
        let same = |i: usize| grid[row.points[i]].hyperlink().as_ref() == Some(&hyperlink);
        let mut first = index;
        while first > 0 && same(first - 1) {
            first -= 1;
        }
        let mut last = index;
        while last + 1 < row.points.len() && same(last + 1) {
            last += 1;
        }
        let target = hyperlink_target(hyperlink.uri())?;
        return Some(Found {
            runs: hyperlink_runs(term, &row, (first, last), &hyperlink),
            target,
        });
    }

    if let Some(found) = hard_wrapped(term, &row, point, joined_file_exists) {
        return Some(found);
    }
    find(&row.chars.iter().collect::<String>())
        .into_iter()
        .find(|link| link.start <= index && index < link.end)
        .map(|link| Found::over(&row.points[link.start..link.end], link.target))
}

/// The cells of an OSC 8 hyperlink: its run on the pointer's line, and the
/// runs of the same hyperlink — the same id, so the same link and not only
/// the same address — on the lines a program broke it onto itself. Claude
/// Code writes the hyperlink again on each row of an address it wrapped,
/// under one id; Codex writes it once and moves the cursor to the next row
/// inside it. A run is followed onto the next line only from the last
/// character of its line, to the first of the next, so the indent between
/// is in neither run.
fn hyperlink_runs<T>(
    term: &Term<T>,
    row: &Row,
    (first, last): (usize, usize),
    hyperlink: &alacritty_terminal::term::cell::Hyperlink,
) -> Vec<(Point, Point)> {
    let grid = term.grid();
    let (top, bottom) = (grid.topmost_line(), grid.bottommost_line());
    let same = |at: Point| grid[at].hyperlink().as_ref() == Some(hyperlink);
    let blank = |ch: &char| ch.is_whitespace();
    let mut runs = vec![(row.points[first], row.points[last])];

    // Back: from the first character of a line to the last of the one before.
    let mut current = (logical_row(term, row.points[0].line), first);
    while runs.len() < MAX_JOINED_ROWS {
        let (line, first) = (&current.0, current.1);
        let line_top = line.points[0].line;
        if line_top <= top || line.chars[..first].iter().any(|ch| !blank(ch)) {
            break;
        }
        let prev = logical_row(term, Line(line_top.0 - 1));
        let Some(end) = prev.chars.iter().rposition(|ch| !blank(ch)) else {
            break;
        };
        if !same(prev.points[end]) {
            break;
        }
        let mut start = end;
        while start > 0 && same(prev.points[start - 1]) {
            start -= 1;
        }
        runs.insert(0, (prev.points[start], prev.points[end]));
        current = (prev, start);
    }
    // On: from the last character of a line to the first of the one after.
    let mut current = (logical_row(term, row.points[0].line), last);
    while runs.len() < MAX_JOINED_ROWS {
        let (line, last) = (&current.0, current.1);
        let line_bottom = line.points[line.points.len() - 1].line;
        if line_bottom >= bottom || line.chars[last + 1..].iter().any(|ch| !blank(ch)) {
            break;
        }
        let next = logical_row(term, Line(line_bottom.0 + 1));
        let Some(start) = next.chars.iter().position(|ch| !blank(ch)) else {
            break;
        };
        if !same(next.points[start]) {
            break;
        }
        let mut end = start;
        while end + 1 < next.points.len() && same(next.points[end + 1]) {
            end += 1;
        }
        runs.push((next.points[start], next.points[end]));
        current = (next, end);
    }
    runs
}

/// How many rows an agent's own wrapping is followed across, each way from
/// the pointer's, and how many characters the rows joined may hold: a path
/// of a thousand characters in a pane forty wide is twenty-five rows. The
/// scan over them is still one pass, see `docs/traps.md`.
const MAX_JOINED_ROWS: usize = 32;
const MAX_JOINED_CHARS: usize = 8 * 1024;

/// The empty columns a CLI may keep at its right edge: Claude Code's and
/// Codex's prompt rows wrap one column short of it.
const RIGHT_MARGIN: usize = 1;

/// A break a program made itself between two logical lines.
struct Join {
    /// Where the token at the end of the line before ends: its trailing
    /// blanks are not joined.
    prev_end: usize,
    /// The indent the line after starts with, skipped.
    skip: usize,
    /// Whether a URL may run across it too.
    url: bool,
}

/// Whether the line after `prev` continues it as though a program broke a
/// token there itself, with a newline and an indent, rather than the
/// terminal with `WRAPLINE` — measured on Claude Code and Codex, see
/// `tests/all/terminal_wrapped_links.rs`.
///
/// It does when `prev` ends in a character that is not whitespace, `next`
/// starts with at least one space and then a character that is not, and the
/// token `next` starts with would not have fit in the columns `prev` left
/// empty, give or take [`RIGHT_MARGIN`] — a wrapper broke there because it
/// had to. A program that breaks by character fills the row to its last
/// column; one that breaks text after a hyphen leaves fewer columns than
/// the next piece. Neither side may be an OSC 8 hyperlink's cell.
///
/// A URL may be joined only where the row is full to its last column and
/// the token is the rest of an address — see [`joins_a_url`].
fn continuation<T>(term: &Term<T>, prev: &Row, next: &Row) -> Option<Join> {
    let grid = term.grid();
    let last = prev.chars.iter().rposition(|ch| !ch.is_whitespace())?;
    let end = prev.points[last];
    if end.line != prev.points.last()?.line || uri_text_end(prev.chars[last]) {
        return None;
    }
    let skip = next.chars.iter().take_while(|ch| **ch == ' ').count();
    let begin = *next.points.get(skip)?;
    if skip == 0 || begin.line != next.points[0].line || uri_text_end(next.chars[skip]) {
        return None;
    }
    if grid[end].hyperlink().is_some() || grid[begin].hyperlink().is_some() {
        return None;
    }
    let width = if grid[end].flags.contains(Flags::WIDE_CHAR) {
        2
    } else {
        1
    };
    let empty = grid.columns().saturating_sub(end.column.0 + width);
    let token_end = next.chars[skip..]
        .iter()
        .position(|ch| uri_text_end(*ch))
        .map_or(next.chars.len(), |length| skip + length);
    let token = &next.chars[skip..token_end];
    if token.len() + RIGHT_MARGIN <= empty {
        return None;
    }
    Some(Join {
        prev_end: last + 1,
        skip,
        url: empty == 0 && joins_a_url(token),
    })
}

/// Whether the start of a row, after an address that reached the last
/// column, is the rest of the address rather than the next word: it holds a
/// character an address is made of and prose words seldom are, once the
/// sentence's punctuation is off its end. `…/index.ht` broken before `ml`
/// is missed, and the address stays the part before the edge, as it was;
/// prose that happens to start with such a word is joined, which is the
/// price of the rest.
fn joins_a_url(token: &[char]) -> bool {
    let mut end = token.len();
    while end > 0 && matches!(token[end - 1], '.' | ',' | ';' | ':' | '!' | '?') {
        end -= 1;
    }
    token[..end].iter().any(|ch| {
        matches!(
            ch,
            '/' | '?' | '=' | '&' | '#' | '%' | '.' | '-' | '_' | '~' | '+' | ':'
        )
    })
}

/// Whether a line holds one token between its blanks — the only kind a
/// wrapped token can run on through, to the line after it.
fn single_token(row: &Row) -> bool {
    let Some(first) = row.chars.iter().position(|ch| !ch.is_whitespace()) else {
        return false;
    };
    let last = row
        .chars
        .iter()
        .rposition(|ch| !ch.is_whitespace())
        .unwrap_or(first);
    !row.chars[first..=last].iter().any(|ch| ch.is_whitespace())
}

/// The link under a point when it runs across lines a program broke itself
/// — Claude Code and Codex lay out their own text, so a long path or
/// address they print is several rows with no `WRAPLINE` between them, and
/// each one after the first starts with an indent. The lines that look like
/// that (see [`continuation`]) are joined without their indents, and the
/// link is the joined one only when it crosses a break and:
///
/// - **a file** — `joined_file_exists` says the joined path names one;
/// - **a URL** — every break it crosses is at the last column and onto
///   what looks like the rest of an address.
///
/// Otherwise the rows are read one by one, as they always were: a link
/// that works on its own row is never made a broken joined one.
///
/// A line further than the next one is looked at only through lines that
/// are one token each, so prose is not joined row after row.
fn hard_wrapped<T>(
    term: &Term<T>,
    row: &Row,
    point: Point,
    joined_file_exists: impl FnOnce(&Found) -> bool,
) -> Option<Found> {
    let grid = term.grid();
    let (top, bottom) = (grid.topmost_line(), grid.bottommost_line());
    if point.line < top || point.line > bottom || point.column.0 >= grid.columns() {
        return None;
    }
    let mut length = row.chars.len();

    // Nearest first, each with the join between it and the line after it.
    let mut before: Vec<(Row, Join)> = Vec::new();
    while before.len() < MAX_JOINED_ROWS {
        let current = before.last().map_or(row, |(row, _)| row);
        let line = current.points[0].line;
        if line <= top || (!before.is_empty() && !single_token(current)) {
            break;
        }
        let prev = logical_row(term, Line(line.0 - 1));
        length += prev.chars.len();
        let Some(join) = continuation(term, &prev, current).filter(|_| length <= MAX_JOINED_CHARS)
        else {
            break;
        };
        before.push((prev, join));
    }
    // Nearest first, each with the join between the line before it and it.
    let mut after: Vec<(Row, Join)> = Vec::new();
    while after.len() < MAX_JOINED_ROWS {
        let current = after.last().map_or(row, |(row, _)| row);
        let line = current.points[current.points.len() - 1].line;
        if line >= bottom || (!after.is_empty() && !single_token(current)) {
            break;
        }
        let next = logical_row(term, Line(line.0 + 1));
        length += next.chars.len();
        let Some(join) = continuation(term, current, &next).filter(|_| length <= MAX_JOINED_CHARS)
        else {
            break;
        };
        after.push((next, join));
    }
    if before.is_empty() && after.is_empty() {
        return None;
    }

    // The lines top to bottom; `joins[i]` is between `lines[i]` and the next.
    let mut lines: Vec<&Row> = Vec::new();
    let mut joins: Vec<&Join> = Vec::new();
    for (prev, join) in before.iter().rev() {
        lines.push(prev);
        joins.push(join);
    }
    lines.push(row);
    for (next, join) in &after {
        joins.push(join);
        lines.push(next);
    }
    let mut chars = Vec::new();
    let mut points = Vec::new();
    // Where each line after the first begins in `chars`, and whether a URL
    // may run across the break before it.
    let mut breaks = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let from = match i.checked_sub(1) {
            Some(join) => {
                breaks.push((chars.len(), joins[join].url));
                joins[join].skip
            }
            None => 0,
        };
        let to = joins.get(i).map_or(line.chars.len(), |join| join.prev_end);
        chars.extend_from_slice(&line.chars[from..to]);
        points.extend_from_slice(&line.points[from..to]);
    }

    let index = points.iter().rposition(|at| *at <= point)?;
    let spacer = grid[point]
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    if points[index] != point && !(spacer && points[index].line == point.line) {
        // An indent skipped, or the blanks after a token: not the link's.
        return None;
    }
    let link = find(&chars.iter().collect::<String>())
        .into_iter()
        .find(|link| link.start <= index && index < link.end)?;
    let mut crossed = breaks
        .iter()
        .filter(|(at, _)| link.start < *at && *at < link.end)
        .peekable();
    crossed.peek()?;
    let found = Found::over(&points[link.start..link.end], link.target);
    let joined = match &found.target {
        Target::Url(_) => crossed.all(|(_, url)| *url),
        Target::File { .. } => joined_file_exists(&found),
    };
    joined.then_some(found)
}

/// What a `cmd`-press opens: the link under the press **now**, looked for
/// in the grid again, and not the one underlined when the pointer last
/// moved — output moves text under a still pointer, a program redraws in
/// place, and the underline was kept against a grid point.
///
/// The link found now leads somewhere when it is the one underlined — a
/// file deleted since is still the link that was promised, and the window
/// says it has gone — and otherwise only when `leads_somewhere` says so,
/// which is the disk's question the underline asks.
pub fn pressed(
    now: Option<Found>,
    underlined: Option<&Found>,
    leads_somewhere: impl FnOnce(&Target) -> bool,
) -> Option<Target> {
    let now = now?;
    if underlined == Some(&now) || leads_somewhere(&now.target) {
        Some(now.target)
    } else {
        None
    }
}

/// What an OSC 8 hyperlink's URI leads to: a web address as it is, a
/// `file://` path with an optional location, and nothing for other schemes.
fn hyperlink_target(uri: &str) -> Option<Target> {
    let chars: Vec<char> = uri.chars().collect();
    if SCHEMES.iter().any(|scheme| starts_with(&chars, 0, scheme)) {
        return Some(Target::Url(uri.to_owned()));
    }
    if !starts_with(&chars, 0, FILE_SCHEME) {
        return None;
    }
    let body = &uri[FILE_SCHEME.len()..];
    let body = if body
        .get(..10)
        .is_some_and(|host| host.eq_ignore_ascii_case("localhost/"))
    {
        &body[9..]
    } else {
        body
    };
    if !body.starts_with('/') {
        return None;
    }
    // Read the location before decoding: `%3A7` is part of a filename.
    let (path, line, column) = if let Some((before, last)) = location_suffix(body) {
        match location_suffix(before) {
            Some((path, line)) => (path, Some(line), Some(last)),
            None => (before, Some(last), None),
        }
    } else {
        (body, None, None)
    };
    let path = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    Some(Target::File {
        path: PathBuf::from(path.as_ref()),
        line,
        column,
    })
}

fn location_suffix(text: &str) -> Option<(&str, u32)> {
    let (path, digits) = text.rsplit_once(':')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = digits.parse().ok()?;
    (number > 0).then_some((path, number))
}

/// Where a printed path is, if a file is there: an absolute path as it is,
/// a relative one against the directory the terminal started in and then
/// against the project's root.
///
/// Both sides are canonicalised before they are compared, because the
/// disk's name for a place is not always the one it was given: a temporary
/// directory under `/var` is really under `/private/var`.
pub fn resolve(path: &Path, cwd: &Path, root: &Path) -> Option<Resolved> {
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else {
        vec![cwd.join(path), root.join(path)]
    };
    let root = root.canonicalize().ok();
    let found = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())?;
    let real = found.canonicalize().ok()?;
    match root
        .as_deref()
        .and_then(|root| real.strip_prefix(root).ok())
    {
        Some(inside) => Some(Resolved::Inside(inside.to_path_buf())),
        None => Some(Resolved::Outside(real)),
    }
}

/// Who shows a file a `cmd`-click resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// Guild, in a tab: text, Markdown, an image it previews — or a file
    /// whose tab says why it is not shown, too large or unreadable.
    InGuild,
    /// The application macOS opens it with: a PDF, a video, an archive.
    BySystem,
    /// Nobody: a program, a script or an installer, which the system would
    /// run rather than show.
    Refused,
    /// The file went between the click and the read.
    Gone,
}

/// Decide who shows a file, by the rules a file tab reads it with.
///
/// **Reads the disk**, up to [`crate::file_tab::OPEN_LIMIT`] bytes, so it
/// belongs on a background thread. Everything is read through **one handle**,
/// opened as the tab opens a file — without waiting for a writer, and only
/// while it is a regular file — so a pipe swapped in at the path after the
/// first look is never opened again and cannot hang the click.
///
/// An image Guild previews is decided by its extension, as the tab decides
/// it, and so is a document it cannot show ([`DOCUMENTS`]): a PDF written
/// by hand is all ASCII and still a PDF. Everything else by its bytes —
/// text is UTF-8 with no NUL near the start, `file_tab::read`'s rule.
///
/// Past the open limit the tab says the file is too large, as it always
/// has, unless its first bytes are already not text: a long video is the
/// commonest file there is past twenty megabytes, and it belongs to its
/// player.
///
/// **What macOS would run or install is refused, never handed over** — a
/// click on a path an agent printed must not run anything: a Mach-O, an
/// executable with no extension or starting with `#!`, and every type in
/// [`LAUNCHABLE`], whatever its bytes — and a Finder alias, which macOS
/// opens by opening its original, an application included; it is refused
/// rather than resolved. Text Guild shows itself, a UTF-8 script included,
/// still opens in a tab like any other file — read-only outside the
/// project, as a project file is not — and nothing runs there.
pub fn shown_by(path: &Path) -> Shown {
    use crate::file_tab::{FileState, OPEN_LIMIT};
    use std::io::Read as _;
    use std::os::unix::fs::PermissionsExt as _;

    let (file, meta) = match crate::file_tab::open_regular(path) {
        Ok(opened) => opened,
        Err(FileState::Gone) => return Shown::Gone,
        // The tab says why it cannot read it.
        Err(_) => return Shown::InGuild,
    };
    if crate::file_tab::image::format(path).is_some() {
        return Shown::InGuild;
    }
    let mut bytes = Vec::with_capacity(SNIFF);
    if (&file).take(SNIFF as u64).read_to_end(&mut bytes).is_err() {
        return Shown::InGuild;
    }
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase());
    let named = |table: &[&str]| {
        extension
            .as_deref()
            .is_some_and(|extension| table.contains(&extension))
    };
    let executable = meta.permissions().mode() & 0o111 != 0;
    let program = is_mach_o(&bytes)
        || (executable && (extension.is_none() || bytes.starts_with(b"#!")))
        || named(LAUNCHABLE)
        || bytes.starts_with(BOOKMARK)
        || crate::system::alias::is_alias(path);
    if named(DOCUMENTS) {
        return if program {
            Shown::Refused
        } else {
            Shown::BySystem
        };
    }

    let text = if meta.len() > OPEN_LIMIT {
        !starts_as_binary(&bytes)
    } else {
        // The rest, from the same handle.
        if (&file)
            .take(OPEN_LIMIT + 1 - bytes.len() as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return Shown::InGuild;
        }
        if bytes.len() as u64 > OPEN_LIMIT {
            !starts_as_binary(&bytes[..SNIFF])
        } else {
            matches!(
                crate::highlight::Text::of(&bytes),
                crate::highlight::Text::Source { .. }
            )
        }
    };
    if text {
        Shown::InGuild
    } else if program {
        Shown::Refused
    } else {
        Shown::BySystem
    }
}

/// The types macOS launches, installs or follows when one is opened, by
/// extension, lower case: Terminal runs `.command` and `.tool` scripts and
/// the command a `.terminal` or `.term` file carries; Installer installs a
/// package; Jar Launcher and Java Web Start run Java; Script Editor and
/// Automator open into what runs a script or a workflow; System Settings
/// installs a pane, a screen saver or a profile; a location file opens
/// whatever it points at, an application included; an application,
/// plug-in or extension bundle is code; a Windows program runs wherever
/// something is installed to run one.
#[rustfmt::skip]
pub const LAUNCHABLE: &[&str] = &[
    // Terminal
    "command", "tool", "terminal", "term",
    // Installer
    "pkg", "mpkg", "dist", "distz",
    // Java
    "jar", "jnlp",
    // scripts and Automator
    "scpt", "scptd", "workflow", "action", "caction",
    // System Settings
    "prefpane", "saver", "slidesaver", "mobileconfig", "configprofile", "provisionprofile",
    // locations
    "fileloc", "webloc", "inetloc", "afploc", "ftploc", "vncloc", "mailloc", "newsloc",
    // code in a bundle
    "app", "appex", "bundle", "plugin", "webplugin", "pluginkit", "kext", "osax", "xpc",
    "qlgenerator", "mdimporter",
    // Windows
    "exe", "msi",
];

/// Documents Guild cannot show, decided by format before their bytes are
/// asked whether they are text — a PDF can be all ASCII, and so can RTF,
/// which is markup a person means to read set. Images Guild previews and
/// text formats are not here.
#[rustfmt::skip]
pub const DOCUMENTS: &[&str] = &[
    // documents; not `key`, which is as often a PEM key as a Keynote file
    "pdf", "rtf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
    "numbers", "pages", "epub",
    // sound
    "mp3", "m4a", "aac", "wav", "aiff", "aif", "flac", "ogg", "opus",
    // video
    "mp4", "m4v", "mov", "avi", "mkv", "webm",
    // archives
    "zip", "gz", "tgz", "tar", "bz2", "xz", "7z", "rar",
    // Flat OpenDocument: XML, and still an office document
    "fodt", "fods", "fodp", "fodg",
];

/// How a bookmark file starts — what Finder writes an alias as since Mac OS
/// X 10.6: `book`, four zero bytes, `mark`.
const BOOKMARK: &[u8] = b"book\0\0\0\0mark";

/// How many bytes are looked at to tell a file's kind — git's count, and
/// `highlight::Text`'s.
const SNIFF: usize = 8 * 1024;

/// Whether a file's first bytes are not text: a NUL, or a byte sequence
/// that is not UTF-8 — a character cut at the end of the prefix is not one.
fn starts_as_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err_and(|error| error.error_len().is_some())
}

/// Whether a file's first bytes are a Mach-O's magic number: thin 32- and
/// 64-bit in either byte order, and fat — `FAT_MAGIC` and `FAT_MAGIC_64`,
/// written big-endian, and their swapped forms.
fn is_mach_o(bytes: &[u8]) -> bool {
    const MACH_O: [[u8; 4]; 8] = [
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xbe, 0xba, 0xfe, 0xca],
        [0xca, 0xfe, 0xba, 0xbf],
        [0xbf, 0xba, 0xfe, 0xca],
    ];
    bytes.len() >= 4 && MACH_O.iter().any(|magic| bytes[..4] == *magic)
}
