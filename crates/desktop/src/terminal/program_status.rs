//! What an agent program says it is doing: OSC 7501, the Program Status
//! Protocol (rev 0.4, <https://docs.otty.sh/vt/osc/osc-7501>). Pure.
//!
//! A program asks whether the terminal understands the protocol with
//! `OSC 7501 ; ? ST` followed by `CSI c`, and takes a Primary Device
//! Attributes reply that arrives with no 7501 reply before it as "no".
//! Claude Code settles that answer once per session (issue 105534), so the
//! probe is answered by the reader, before the chunk it arrived in is handed
//! to alacritty — which answers the DA1 while it parses — the way XTVERSION
//! is. The scanner only reads: the probe's bytes still reach the parser,
//! which discards a `?` report as invalid.

/// How the probe ended, which is how its reply must end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminator {
    /// `BEL`, `0x07`.
    Bel,
    /// `ESC \`, the String Terminator.
    St,
}

impl Terminator {
    fn bytes(self) -> &'static str {
        match self {
            Self::Bel => "\x07",
            Self::St => "\x1b\\",
        }
    }
}

/// Finds the probe in a stream of reads, a split one included.
///
/// A state per byte rather than a carry of bytes, in `xtversion::Scanner`'s
/// manner: where a read ended is where the state stands.
#[derive(Debug, Default)]
pub struct ProbeScanner {
    state: u8,
}

/// `ESC ] 7 5 0 1 ; ?` — the bytes before the terminator, one state each.
const PROBE: &[u8] = b"\x1b]7501;?";
/// After the `?`, an `ESC` that may begin `ESC \`.
const AFTER_ESC: u8 = PROBE.len() as u8 + 1;

impl ProbeScanner {
    /// The probes these bytes finished, in order, by their terminator.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Terminator> {
        let mut found = Vec::new();
        for &byte in bytes {
            let state = self.state as usize;
            self.state = if state == PROBE.len() {
                match byte {
                    0x07 => {
                        found.push(Terminator::Bel);
                        0
                    }
                    0x1b => AFTER_ESC,
                    _ => 0,
                }
            } else if self.state == AFTER_ESC {
                match byte {
                    b'\\' => {
                        found.push(Terminator::St);
                        0
                    }
                    // The `ESC` began another sequence, which may be a probe.
                    b']' => 2,
                    0x1b => 1,
                    _ => 0,
                }
            } else if byte == PROBE[state] {
                self.state + 1
            } else if byte == 0x1b {
                1
            } else {
                0
            };
        }
        found
    }
}

/// The states and kinds Guild understands, said in the probe's reply.
pub const CAPABILITIES: &str =
    "states=idle,working,done,blocked,error:kinds=permission,question,auth";

/// The reply to a probe, ended the way the probe was.
pub fn probe_answer(terminator: Terminator) -> String {
    format!("\x1b]7501;?:{CAPABILITIES}{}", terminator.bytes())
}

/// The longest `OSC 7501` sequence, introducer and terminator included; a
/// longer one is discarded whole without disturbing the output around it.
pub const SEQUENCE_LIMIT: usize = 4096;

/// What a program says it is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    /// Removes the record named by `id`, and its descendants; without an
    /// `id`, every record.
    Clear,
}

/// Why a `blocked` program waits — said only when the program says it.
pub use guild_session::status::Kind;

/// One valid report: it replaces its record whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub state: State,
    /// `None` is the root record — the session itself.
    pub id: Option<String>,
    pub kind: Option<Kind>,
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
}

impl Report {
    /// The root record as the board reads it: what the session is doing.
    pub fn reported(&self) -> Option<guild_session::status::Reported> {
        use guild_session::status::Reported;
        Some(match self.state {
            State::Idle => Reported::Idle,
            State::Working => Reported::Working,
            State::Blocked => Reported::Blocked(self.kind),
            State::Done => Reported::Done,
            State::Error => Reported::Error,
            State::Clear => return None,
        })
    }
}

const ID_DEPTH: usize = 8;
const ID_SEGMENT: usize = 32;
const ID_LENGTH: usize = 128;
const APP_LENGTH: usize = 32;
const TITLE_BYTES: usize = 192;
const MSG_BYTES: usize = 2048;

/// What follows `OSC 7501 ;` up to the terminator, read as a report; `None`
/// when the protocol says to discard it, the probe's `?` included.
pub fn parse_report(payload: &[u8]) -> Option<Report> {
    if payload.iter().any(|&byte| byte < 0x20 || byte == 0x7f) {
        return None;
    }
    let text = std::str::from_utf8(payload).ok()?;
    let mut fields: Vec<(&str, &str)> = Vec::new();
    for pair in text.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key.is_empty() || !value.bytes().all(in_value_alphabet) {
            continue;
        }
        match fields.iter_mut().find(|(known, _)| *known == key) {
            Some(field) => field.1 = value,
            None => fields.push((key, value)),
        }
    }
    let field = |key: &str| fields.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);

    let state = match field("state")? {
        "idle" => State::Idle,
        "working" => State::Working,
        "done" => State::Done,
        "blocked" => State::Blocked,
        "error" => State::Error,
        "clear" => State::Clear,
        _ => return None,
    };
    let id = match field("id") {
        Some(id) if valid_id(id) => Some(id.to_owned()),
        Some(_) => return None,
        None => None,
    };
    let kind = match (state, field("kind")) {
        (State::Blocked, Some("permission")) => Some(Kind::Permission),
        (State::Blocked, Some("question")) => Some(Kind::Question),
        (State::Blocked, Some("auth")) => Some(Kind::Auth),
        _ => None,
    };
    // A number is over the limit when it is past 100, however many digits it
    // has; only a value that is not a number is a malformed pair to skip.
    let progress = match field("progress") {
        Some(value) if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
            match value.parse::<u32>() {
                Ok(value) if value <= 100 => Some(value as u8),
                _ => return None,
            }
        }
        _ => None,
    };
    let app = match field("app") {
        Some(app) if app.len() > APP_LENGTH => return None,
        app => app.map(str::to_owned),
    };
    let title = match field("title") {
        Some(value) => Some(decode_text(value, TITLE_BYTES)?),
        None => None,
    };
    let msg = match field("msg") {
        Some(value) => Some(decode_text(value, MSG_BYTES)?),
        None => None,
    };
    Some(Report {
        state,
        id,
        kind,
        progress,
        app,
        title,
        msg,
    })
}

fn in_value_alphabet(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

fn valid_id(id: &str) -> bool {
    id.len() <= ID_LENGTH
        && id.split('/').count() <= ID_DEPTH
        && id.split('/').all(|segment| {
            (1..=ID_SEGMENT).contains(&segment.len())
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-'))
        })
}

/// Base64 to UTF-8 text of at most `limit` bytes, without control characters.
fn decode_text(value: &str, limit: usize) -> Option<String> {
    let bytes = decode_base64(value)?;
    if bytes.len() > limit {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

/// Standard base64, padded; anything else is `None`.
fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let digit = |byte: u8| -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let quads = bytes.len() / 4;
    for (index, quad) in bytes.chunks(4).enumerate() {
        let padding = quad.iter().rev().take_while(|&&b| b == b'=').count();
        if padding > 2 || (padding > 0 && index + 1 != quads) {
            return None;
        }
        let mut n = 0u32;
        for &byte in &quad[..4 - padding] {
            n = (n << 6) | digit(byte)?;
        }
        n <<= 6 * padding as u32;
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    Some(out)
}

/// The most records a terminal keeps, the root's place included.
pub const RECORD_LIMIT: usize = 256;

/// A terminal's records: the root — the session — and its `id` children.
///
/// A report replaces its record whole; `clear` removes a subtree or all;
/// `working` and `blocked` end with the process or a shell prompt; a full
/// reset (RIS) ends everything. Nothing here is persisted. GPUI-free, so a
/// headless owner can keep it as it is.
#[derive(Debug, Default)]
pub struct Records {
    root: Option<Report>,
    children: std::collections::BTreeMap<String, Report>,
    /// Every change of the root record not yet taken, oldest first.
    changes: std::collections::VecDeque<RootChange>,
    sequence: u64,
    /// Changes dropped from the front of a full queue since the last take.
    lost: u64,
}

/// The most root-record changes a terminal holds for the window.
pub const CHANGE_LIMIT: usize = 64;

/// One change of the session's own record: the record after it, `None`
/// when there is none any more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootChange {
    pub sequence: u64,
    pub record: Option<Report>,
}

impl Records {
    /// Apply one report.
    pub fn apply(&mut self, report: Report) {
        let before = self.root.clone();
        self.apply_inner(report);
        self.noted(before);
    }

    /// The root record's changes since the last take, in order, and how many
    /// were dropped from a full queue meanwhile — the window polls on its
    /// own clock, and a `working` then `done` between two polls must reach
    /// it as two changes.
    pub fn take_changes(&mut self) -> (Vec<RootChange>, u64) {
        (
            self.changes.drain(..).collect(),
            std::mem::take(&mut self.lost),
        )
    }

    fn noted(&mut self, before: Option<Report>) {
        if self.root == before {
            return;
        }
        if self.changes.len() == CHANGE_LIMIT {
            self.changes.pop_front();
            self.lost += 1;
        }
        self.sequence += 1;
        self.changes.push_back(RootChange {
            sequence: self.sequence,
            record: self.root.clone(),
        });
    }

    fn apply_inner(&mut self, report: Report) {
        match (report.state, report.id.clone()) {
            (State::Clear, None) => {
                self.root = None;
                self.children.clear();
            }
            (State::Clear, Some(id)) => {
                let below = format!("{id}/");
                self.children
                    .retain(|key, _| *key != id && !key.starts_with(&below));
            }
            (_, None) => self.root = Some(report),
            (_, Some(id)) => {
                // One place is always the root's.
                if self.children.contains_key(&id) || self.children.len() < RECORD_LIMIT - 1 {
                    self.children.insert(id, report);
                }
            }
        }
    }

    /// The session's own record.
    pub fn root(&self) -> Option<&Report> {
        self.root.as_ref()
    }

    /// A child record by its `id`.
    pub fn child(&self, id: &str) -> Option<&Report> {
        self.children.get(id)
    }

    /// How many records are kept.
    pub fn len(&self) -> usize {
        usize::from(self.root.is_some()) + self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The process exited, or a shell prompt began: whatever was working or
    /// blocked is not any more.
    pub fn end_active(&mut self) {
        let before = self.root.clone();
        let active = |report: &Report| matches!(report.state, State::Working | State::Blocked);
        if self.root.as_ref().is_some_and(active) {
            self.root = None;
        }
        self.children.retain(|_, report| !active(report));
        self.noted(before);
    }

    /// A full reset (RIS): no records.
    pub fn reset(&mut self) {
        let before = self.root.take();
        self.children.clear();
        self.noted(before);
    }
}
