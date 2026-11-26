//! Bounded read-only review evidence, separate from generic terminal receipts.
use guild_session::{
    store::{Normalise, Store},
    terminal_job::{JobId, JobReceipt},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
const FILE_LIMIT: u64 = 4 << 20;
const STREAM_LIMIT: u64 = 8 << 20;
const RECORD_LIMIT: usize = 64 << 10;
const FINDING_LIMIT: usize = 512;

type Error = String;
#[derive(Clone)]
struct Source {
    id: JobId,
    directory: PathBuf,
    program: PathBuf,
    fresh: bool,
    running: bool,
    started: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub title: String,
    pub phase: String,
    pub reported: String,
    pub narration: String,
    pub findings: Vec<Finding>,
    pub other: usize,
    pub categorized: Vec<(Category, Finding)>,
    pub dropped: usize,
    pub clean: bool,
    pub assessed: bool,
    pub final_result: bool,
    pub incomplete: bool,
    pub stale: bool,
    pub legacy: bool,
    pub unavailable: bool,
}
impl Snapshot {
    pub fn counts(&self) -> (usize, usize, usize) {
        self.findings
            .iter()
            .fold((0, 0, 0), |(a, b, c), f| match f.severity.as_str() {
                "critical" | "major" => (a + 1, b, c),
                "minor" => (a, b + 1, c),
                _ => (a, b, c + 1),
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    OpenQuestion,
    PreExisting,
    Immaterial,
    Rejected,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct Finding {
    pub id: String,
    pub severity: String,
    pub verdict: String,
    pub title: String,
    pub body: String,
    pub file: String,
    pub line: i64,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
struct Generation {
    dev: u64,
    ino: u64,
}
fn generation(file: &File) -> Result<Generation, Error> {
    let m = file.metadata().map_err(err)?;
    if (!m.is_file() && !m.is_dir()) || (m.is_file() && m.nlink() != 1) {
        return Err("Unsafe review artifact".into());
    }
    Ok(Generation {
        dev: m.dev(),
        ino: m.ino(),
    })
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct Pin {
    directory: PathBuf,
    program: PathBuf,
    root: Generation,
    task: String,
    run: String,
    files: BTreeMap<String, Generation>,
}
#[derive(Deserialize, Serialize)]
struct Pins {
    #[serde(default = "pin_version")]
    version: u32,
    jobs: BTreeMap<JobId, Pin>,
}
fn pin_version() -> u32 {
    1
}
impl Default for Pins {
    fn default() -> Self {
        Self {
            version: pin_version(),
            jobs: BTreeMap::new(),
        }
    }
}
impl Normalise for Pins {}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Scope {
    task: String,
    run: String,
}
#[derive(Deserialize)]
struct Header {
    version: u32,
    identity: Scope,
}
// Matches installed revust runtime::pipeline::State and runtime::StopReason serde.
#[derive(Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReviewState {
    #[default]
    Failed,
    Complete,
    Partial,
    Stopped(StopReason),
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StopReason {
    CallBudget,
    Deadline,
    Cancelled,
}
#[derive(Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct Outcome {
    state: ReviewState,
    policy_failures: Vec<String>,
    diagnostics: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Sources {
    expected: i64,
    reported: i64,
    degraded: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Stats {
    started_at: String,
    finished_at: String,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Report {
    scope: Scope,
    outcome: Option<Outcome>,
    findings: Vec<Finding>,
    open_questions: Vec<Finding>,
    pre_existing: Vec<Finding>,
    immaterial: Vec<Finding>,
    sources: Sources,
    stats: Stats,
}
#[derive(Deserialize)]
struct Manifest {
    task: String,
    run: String,
    started_at: String,
    finished_at: String,
    outcome: Option<Outcome>,
}
#[derive(Deserialize)]
struct Event {
    kind: String,
    #[serde(default)]
    stage: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    findings: Vec<Finding>,
    #[serde(default)]
    at: String,
}
#[derive(Default)]
struct Stream {
    offset: u64,
    tail: Vec<u8>,
    phase: String,
    reported: String,
    narration: String,
    units: BTreeMap<String, Vec<Finding>>,
    categorized: BTreeMap<String, Vec<(Category, Finding)>>,
    dropped: usize,
}
struct Engine {
    pins: Pins,
    store: Store<Pins>,
    blocked: bool,
    streams: HashMap<JobId, Stream>,
    known: HashSet<JobId>,
}
impl Engine {
    fn new(path: PathBuf) -> Self {
        // Do not restore a backup or rewrite corrupted pins: unknown association is conservative.
        let read = (|| -> std::io::Result<Vec<u8>> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(&path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > FILE_LIMIT {
                return Err(std::io::Error::other("Unsafe review pin store"));
            }
            let mut bytes = Vec::new();
            file.take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let (pins, blocked) = match read {
            Ok(bytes) if bytes.len() <= FILE_LIMIT as usize => {
                match serde_json::from_slice::<Pins>(&bytes) {
                    Ok(p) if p.version == pin_version() => (p, false),
                    Ok(_) => (Pins::default(), true),
                    Err(_) => (Pins::default(), true),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Pins::default(), false),
            _ => (Pins::default(), true),
        };
        Self {
            pins,
            store: Store::new(path),
            blocked,
            streams: HashMap::new(),
            known: HashSet::new(),
        }
    }
    fn observe(&mut self, source: &Source) -> Result<Snapshot, Error> {
        let root = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&source.directory)
            .map_err(err)?;
        let root_generation = generation(&root)?;
        let existing = self.pins.jobs.get(&source.id).cloned();
        if let Some(pin) = &existing
            && (pin.directory != source.directory
                || pin.program != source.program
                || pin.root != root_generation)
        {
            return Err("Registered review archive changed".into());
        }
        let mut pin = existing.clone().unwrap_or_else(|| Pin {
            directory: source.directory.clone(),
            program: source.program.clone(),
            root: root_generation,
            ..Default::default()
        });
        let header = artifact(&root, "revust/run.json", &mut pin);
        let legacy = self.blocked || (existing.is_none() && !source.fresh);
        if let Ok(header) = header {
            if existing.is_none()
                && source.fresh
                && source.started.is_some_and(|start| {
                    header.metadata().ok().is_some_and(|m| {
                        (m.mtime() as i128) * 1000 + i128::from(m.mtime_nsec() / 1_000_000)
                            < i128::from(start)
                    })
                })
            {
                return Err("Archive identity predates launch".into());
            }
            let header: Header = decode(&header)?;
            if header.version != 1
                || header.identity.task.is_empty()
                || header.identity.run.is_empty()
            {
                return Err("Unsupported review identity".into());
            }
            if existing.is_some()
                && (pin.task != header.identity.task || pin.run != header.identity.run)
            {
                return Err("Review identity changed".into());
            }
            pin.task = header.identity.task;
            pin.run = header.identity.run;
            self.known.insert(source.id);
        } else if existing.is_some() || !legacy {
            return Err("Review identity unavailable".into());
        }
        let marker = artifact(&root, "manifest.json", &mut pin)?;
        let mut snapshot = Snapshot {
            legacy,
            title: pin.task.clone(),
            ..Default::default()
        };
        if !legacy {
            let file = artifact(&root, "events.jsonl", &mut pin)?;
            let stream = self.streams.entry(source.id).or_default();
            consume(&file, stream, source.started)?;
            snapshot.phase = stream.phase.clone();
            snapshot.reported = stream.reported.clone();
            snapshot.narration = stream.narration.clone();
            snapshot.dropped = stream.dropped;
            snapshot.findings = stream.units.values().flatten().cloned().collect();
            snapshot.categorized = stream.categorized.values().flatten().cloned().collect();
            snapshot.other = snapshot.categorized.len();
            if snapshot.findings.len() + snapshot.categorized.len() > FINDING_LIMIT {
                return Err("Too many review findings".into());
            }
            for name in ["1-found", "2-synthesized", "3-verified"] {
                let path = format!("stages/{name}.json");
                match artifact(&root, &path, &mut pin) {
                    Ok(file) => {
                        let report: Report = decode(&file)?;
                        // Installed checkpoints use Scope::default and zero timestamps.
                        if (!report.scope.task.is_empty() && report.scope.task != pin.task)
                            || (!report.scope.run.is_empty() && report.scope.run != pin.run)
                        {
                            return Err("Checkpoint identity mismatch".into());
                        }
                        apply(&mut snapshot, report)?;
                        snapshot.assessed = name == "3-verified"
                            && snapshot
                                .findings
                                .iter()
                                .all(|f| matches!(f.verdict.as_str(), "confirmed" | "refined"));
                    }
                    Err(e) if e == "missing" => (),
                    Err(e) => return Err(e),
                }
            }
        }
        // A live writer owns the exclusive manifest claim; never bless its partial marker.
        // SAFETY: flock takes an owned descriptor and does not transfer ownership.
        let locked = unsafe { libc::flock(marker.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } == 0;
        if locked && marker.metadata().map_err(err)?.len() > 0 {
            let manifest: Manifest = decode(&marker)?;
            let report_file = artifact(&root, "findings.json", &mut pin)?;
            let report: Report = decode(&report_file)?;
            if manifest.task.is_empty()
                || manifest.run.is_empty()
                || manifest.task != report.scope.task
                || manifest.run != report.scope.run
                || (!pin.task.is_empty() && (manifest.task != pin.task || manifest.run != pin.run))
                || manifest.outcome != report.outcome
                || !valid_time(&manifest.started_at)
                || !valid_time(&manifest.finished_at)
                || manifest.started_at != report.stats.started_at
                || manifest.finished_at != report.stats.finished_at
            {
                return Err("Final review identity or completion mismatch".into());
            }
            let outcome = report.outcome.as_ref();
            let complete = outcome.is_some_and(|o| {
                o.state == ReviewState::Complete
                    && o.policy_failures.is_empty()
                    && o.diagnostics.is_empty()
            });
            let sources_complete = report.sources.expected > 0
                && report.sources.expected == report.sources.reported
                && report.sources.degraded.is_empty();
            snapshot.clean = !legacy
                && complete
                && sources_complete
                && report.findings.is_empty()
                && report.open_questions.is_empty();
            snapshot.incomplete = !complete || !sources_complete;
            apply(&mut snapshot, report)?;
            snapshot.final_result = true;
            snapshot.assessed = !legacy
                && snapshot
                    .findings
                    .iter()
                    .all(|f| matches!(f.verdict.as_str(), "confirmed" | "refined"));
        }
        if !source.running && !snapshot.final_result {
            snapshot.incomplete = true;
        }
        if legacy && !snapshot.final_result {
            return Err("Legacy review evidence unavailable".into());
        }
        if !legacy {
            if self.pins.jobs.len() >= 4096 && !self.pins.jobs.contains_key(&source.id) {
                return Err("Review association limit reached".into());
            }
            let changed = serde_json::to_vec(&pin).map_err(err)?
                != serde_json::to_vec(&existing).map_err(err)?;
            if changed {
                self.pins.jobs.insert(source.id, pin);
                if serde_json::to_vec_pretty(&self.pins).map_err(err)?.len() > FILE_LIMIT as usize {
                    self.blocked = true;
                    return Err("Review association store exceeds limit".into());
                }
                if let Err(e) = self.store.save(&self.pins) {
                    self.blocked = true;
                    return Err(err(e));
                }
            }
        }
        Ok(snapshot)
    }
}
fn valid_time(text: &str) -> bool {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .is_ok_and(|t| t.year() > 1)
}
fn err(e: impl std::fmt::Display) -> Error {
    e.to_string()
}
fn artifact(root: &File, path: &str, pin: &mut Pin) -> Result<File, Error> {
    let mut parent = root.try_clone().map_err(err)?;
    for (index, part) in path.split('/').enumerate() {
        let directory = index + 1 < path.split('/').count();
        let name = std::ffi::CString::new(part).map_err(err)?;
        // SAFETY: a fixed relative component and valid parent descriptor; owned result below.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC
                    | if directory { libc::O_DIRECTORY } else { 0 },
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            let key = path
                .split('/')
                .take(index + 1)
                .collect::<Vec<_>>()
                .join("/");
            return Err(
                if e.kind() == std::io::ErrorKind::NotFound && pin.files.contains_key(&key) {
                    "Previously accepted review artifact is missing".into()
                } else if e.kind() == std::io::ErrorKind::NotFound {
                    "missing".into()
                } else {
                    err(e)
                },
            );
        }
        // SAFETY: openat returned a fresh descriptor, transferred exactly once.
        parent = unsafe { File::from_raw_fd(fd) };
        let g = generation(&parent)?;
        let key = path
            .split('/')
            .take(index + 1)
            .collect::<Vec<_>>()
            .join("/");
        if let Some(old) = pin.files.get(&key)
            && old != &g
        {
            return Err("Review artifact generation changed".into());
        }
        pin.files.insert(key, g);
    }
    if !parent.metadata().map_err(err)?.is_file() {
        return Err("Review artifact is not regular".into());
    }
    Ok(parent)
}
fn decode<T: serde::de::DeserializeOwned>(file: &File) -> Result<T, Error> {
    if file.metadata().map_err(err)?.len() > FILE_LIMIT {
        return Err("Review artifact exceeds limit".into());
    }
    let mut bytes = vec![];
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > FILE_LIMIT as usize {
        return Err("Review artifact exceeds limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Review artifact incomplete or malformed".into())
}
fn apply(snapshot: &mut Snapshot, report: Report) -> Result<(), Error> {
    if report.findings.len()
        + report.open_questions.len()
        + report.pre_existing.len()
        + report.immaterial.len()
        > FINDING_LIMIT
    {
        return Err("Too many review findings".into());
    }
    let mut seen = HashSet::new();
    for f in report
        .findings
        .iter()
        .chain(&report.open_questions)
        .chain(&report.pre_existing)
        .chain(&report.immaterial)
    {
        if f.id.is_empty() || !seen.insert(f.id.clone()) {
            return Err("Ambiguous review finding identity".into());
        }
        if [&f.id, &f.title, &f.body, &f.file, &f.severity, &f.verdict]
            .iter()
            .any(|text| text.len() > RECORD_LIMIT)
        {
            return Err("Review finding text exceeds limit".into());
        }
    }
    snapshot.findings.clear();
    snapshot.categorized.clear();
    for finding in report.findings {
        let category = match finding.verdict.as_str() {
            "rejected" => Some(Category::Rejected),
            "immaterial" => Some(Category::Immaterial),
            "pre_existing" => Some(Category::PreExisting),
            _ => None,
        };
        if let Some(category) = category {
            snapshot.categorized.push((category, finding));
        } else {
            snapshot.findings.push(finding);
        }
    }
    snapshot.categorized.extend(
        report
            .open_questions
            .into_iter()
            .map(|f| (Category::OpenQuestion, f)),
    );
    snapshot.categorized.extend(
        report
            .pre_existing
            .into_iter()
            .map(|f| (Category::PreExisting, f)),
    );
    snapshot.categorized.extend(
        report
            .immaterial
            .into_iter()
            .map(|f| (Category::Immaterial, f)),
    );
    snapshot.other = snapshot.categorized.len();
    Ok(())
}
fn consume(mut file: &File, stream: &mut Stream, started: Option<u64>) -> Result<(), Error> {
    let len = file.metadata().map_err(err)?.len();
    if len < stream.offset || len > STREAM_LIMIT {
        return Err("Review stream truncated or too large".into());
    }
    file.seek(SeekFrom::Start(stream.offset)).map_err(err)?;
    let mut bytes = vec![];
    file.take(256 << 10).read_to_end(&mut bytes).map_err(err)?;
    stream.offset += bytes.len() as u64;
    stream.tail.extend(bytes);
    while let Some(end) = stream.tail.iter().position(|b| *b == b'\n') {
        if end > RECORD_LIMIT {
            return Err("Review event exceeds limit".into());
        }
        let event: Event =
            serde_json::from_slice(&stream.tail[..end]).map_err(|_| "Malformed review event")?;
        stream.tail.drain(..=end);
        if !event.at.is_empty()
            && let (Some(start), Ok(at)) = (
                started,
                time::OffsetDateTime::parse(
                    &event.at,
                    &time::format_description::well_known::Rfc3339,
                ),
            )
            && at.unix_timestamp_nanos() / 1_000_000 < i128::from(start)
        {
            return Err("Review stream predates job".into());
        }
        match event.kind.as_str() {
            "stage" => {
                stream.phase = event.stage;
                stream.narration.clear();
            }
            "agent_activity" => {
                stream.narration = event
                    .text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(240)
                    .collect();
                stream.reported = event.text.chars().take(2000).collect();
            }
            "agent_progress" | "agent_state" => {
                stream.reported = event.text.chars().take(2000).collect();
            }
            "findings" => {
                if event.findings.len() > FINDING_LIMIT || stream.units.len() > 64 {
                    return Err("Review candidate limit exceeded".into());
                }
                let mut data = Snapshot::default();
                apply(
                    &mut data,
                    Report {
                        findings: event.findings,
                        ..Default::default()
                    },
                )?;
                if event.agent == "synthesis" {
                    stream.units.clear();
                    stream.categorized.clear();
                }
                stream
                    .categorized
                    .insert(event.agent.clone(), data.categorized);
                stream.units.insert(event.agent, data.findings);
            }
            "dropped" => {
                stream.dropped = event.findings.len();
                let ids: HashSet<_> = event.findings.iter().map(|f| &f.id).collect();
                for findings in stream.units.values_mut() {
                    findings.retain(|f| !ids.contains(&f.id));
                }
            }
            _ => (),
        }
    }
    if stream.tail.len() > RECORD_LIMIT {
        return Err("Review event tail exceeds limit".into());
    }
    Ok(())
}

fn stale_snapshot(previous: Option<&Snapshot>) -> Snapshot {
    let mut snapshot = previous.cloned().unwrap_or_default();
    snapshot.stale = true;
    snapshot.clean = false;
    snapshot.unavailable = true;
    snapshot
}

// Keep current reviews responsive even when the retained archive history is long.
fn observation_batch(sources: Vec<Source>, cursor: &mut [usize; 2]) -> Vec<Source> {
    fn take(mut sources: Vec<Source>, cursor: &mut usize, limit: usize) -> Vec<Source> {
        if sources.is_empty() {
            return sources;
        }
        let length = sources.len();
        sources.rotate_left(*cursor % length);
        let count = length.min(limit);
        *cursor = (*cursor + count) % length;
        sources.truncate(count);
        sources
    }
    let (live, saved): (Vec<_>, Vec<_>) = sources.into_iter().partition(|source| source.running);
    let live_limit = if saved.is_empty() { 16 } else { 8 };
    let mut batch = take(live, &mut cursor[0], live_limit);
    let saved_limit = 16 - batch.len();
    batch.extend(take(saved, &mut cursor[1], saved_limit));
    batch
}

/// Window-owned observer. Its sole worker owns archive handles/IO; channels are bounded.
pub(crate) struct Activity {
    send: mpsc::SyncSender<Vec<Source>>,
    receive: mpsc::Receiver<HashMap<JobId, Snapshot>>,
    pub snapshots: HashMap<JobId, Snapshot>,
    fresh: HashSet<JobId>,
    next: Instant,
    busy: bool,
    cursor: [usize; 2],
}
impl Activity {
    pub fn new(path: PathBuf) -> Self {
        let (send, requests) = mpsc::sync_channel::<Vec<Source>>(1);
        let (answers, receive) = mpsc::sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("review-activity".into())
            .spawn(move || {
                let mut engine = Engine::new(path);
                let mut previous: HashMap<JobId, Snapshot> = HashMap::new();
                while let Ok(sources) = requests.recv() {
                    for source in sources {
                        let value = match engine.observe(&source) {
                            Ok(s) => s,
                            Err(_) => {
                                if !engine.known.contains(&source.id)
                                    && !engine.pins.jobs.contains_key(&source.id)
                                    && !previous.contains_key(&source.id)
                                {
                                    continue;
                                }
                                stale_snapshot(previous.get(&source.id))
                            }
                        };
                        previous.insert(source.id, value);
                    }
                    if answers.send(previous.clone()).is_err() {
                        break;
                    }
                }
            });
        Self {
            send,
            receive,
            snapshots: HashMap::new(),
            fresh: HashSet::new(),
            next: Instant::now(),
            busy: false,
            cursor: [0, 0],
        }
    }
    pub fn launched(&mut self, id: JobId) {
        self.fresh.insert(id);
    }
    pub fn poll(&mut self, receipts: impl FnOnce() -> std::sync::Arc<Vec<JobReceipt>>) -> bool {
        let mut changed = false;
        if let Ok(snapshots) = self.receive.try_recv() {
            changed = self.snapshots != snapshots;
            self.snapshots = snapshots;
            self.busy = false;
        }
        if !self.busy && Instant::now() >= self.next {
            let receipts = receipts();
            let sources: Vec<_> = receipts
                .iter()
                .filter(|r| !r.is_archive_view())
                .filter_map(|r| {
                    let a = r.revmux_archive.as_ref()?;
                    Some(Source {
                        id: r.id,
                        directory: a.directory.clone(),
                        program: a.program.clone(),
                        fresh: self.fresh.contains(&r.id),
                        running: !r.state.is_terminal(),
                        started: r.started_at_ms,
                    })
                })
                .collect();
            let sources = observation_batch(sources, &mut self.cursor);
            if self.send.try_send(sources).is_ok() {
                self.busy = true;
            }
            self.next = Instant::now() + Duration::from_secs(1);
        }
        changed
    }
}
#[cfg(test)]
pub(crate) fn fixture_snapshot(receipt: &JobReceipt, pins: PathBuf) -> Snapshot {
    let archive = receipt.revmux_archive.as_ref().unwrap();
    Engine::new(pins)
        .observe(&Source {
            id: receipt.id,
            directory: archive.directory.clone(),
            program: archive.program.clone(),
            fresh: true,
            running: !receipt.state.is_terminal(),
            started: receipt.started_at_ms,
        })
        .unwrap()
}

#[cfg(test)]
mod tests;
