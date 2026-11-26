//! Exact Codex rollout access. A directory is never a conversation identity.

use crate::status::Signal;
use serde_json::Value;
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Find a rollout whose filename and metadata both name this conversation.
/// Ambiguous copies are refused; no timestamp or working directory ranks them.
pub fn path(home: &Path, id: &str) -> Option<PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return None;
    }
    let suffix = format!("-{id}.jsonl");
    let mut found = crate::history::codex::rollouts(&home.join("sessions"))
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .filter(|p| {
            let Ok(file) = std::fs::File::open(p) else {
                return false;
            };
            let mut line = String::new();
            if std::io::BufReader::new(file).read_line(&mut line).is_err() {
                return false;
            }
            serde_json::from_str::<Value>(&line).is_ok_and(|v| {
                v["type"] == "session_meta" && v["payload"]["id"].as_str() == Some(id)
            })
        });
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

/// Assistant text only, oldest first among the last `count` replies.
/// Read response items only: event messages repeat the same assistant text.
pub fn replies(path: &Path, count: usize) -> Vec<String> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut replies = std::collections::VecDeque::new();
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let p = &v["payload"];
        if v["type"] != "response_item" || p["type"] != "message" || p["role"] != "assistant" {
            continue;
        }
        let Some(parts) = p["content"].as_array() else {
            continue;
        };
        let text = parts
            .iter()
            .filter(|p| p["type"] == "output_text")
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            replies.push_back(text);
            while replies.len() > count {
                replies.pop_front();
            }
        }
    }
    replies.into_iter().collect()
}

/// The plan's limits as a rollout last recorded them: the `rate_limits`
/// of a `token_count` line, as written, and the line's `timestamp`.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimits {
    /// `{limit_id, primary, secondary, …}`, read by the window.
    pub limits: Value,
    /// When Codex wrote it, RFC 3339.
    pub at: String,
}

/// An authoritative lifecycle signal with its original correlation fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    /// What the exact source says happened.
    pub signal: Signal,
    /// Only turn/tool identity; command and output content are excluded.
    pub payload: Value,
}

// Decode only routing policy; tool outputs can be megabytes and are not
// needed for permission status. Serde skips their values without building them.
#[derive(serde::Deserialize)]
struct PolicyRecord {
    #[serde(rename = "type")]
    kind: String,
    payload: PolicyPayload,
}
#[derive(serde::Deserialize)]
struct PolicyPayload {
    turn_id: Option<Value>,
    approvals_reviewer: Option<Value>,
}

/// A cursor owned by one terminal process, never persisted across resume.
pub struct Probe {
    home: PathBuf,
    id: Option<String>,
    path: Option<PathBuf>,
    offset: u64,
    baseline: bool,
    pending: Vec<(u64, Evidence)>,
    interrupt_boundary: Option<u64>,
    replay_turn: Option<String>,
    turn: Option<String>,
    awaiting_turn_start: bool,
    runtime_turn: Option<String>,
    runtime_starts: Vec<String>,
    disk_start: Option<String>,
    disk_completed: Option<String>,
    settled: bool,
    retired: std::collections::HashSet<String>,
    rate_limits: Option<RateLimits>,
    permission_offset: u64,
    permission_context: Option<(String, bool)>,
}
impl Probe {
    /// Baseline a resumed conversation before its process starts. A new
    /// conversation can be read from its beginning once a hook names it.
    pub fn new(home: PathBuf, resumed: Option<&str>) -> Self {
        let path = resumed.and_then(|id| path(&home, id));
        let offset = path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or(0, |m| m.len());
        let baseline = resumed.is_some() && path.is_none();
        Self {
            home,
            id: resumed.map(str::to_owned),
            path,
            offset,
            baseline,
            pending: Vec::new(),
            interrupt_boundary: None,
            replay_turn: None,
            turn: None,
            awaiting_turn_start: false,
            runtime_turn: None,
            runtime_starts: Vec::new(),
            disk_start: None,
            disk_completed: None,
            settled: false,
            retired: Default::default(),
            rate_limits: None,
            permission_offset: 0,
            permission_context: None,
        }
    }

    /// New, complete lifecycle records from this exact conversation.
    /// Missing files, partial writes and inactivity never become completion.
    pub fn poll(&mut self, id: &str) -> Vec<Signal> {
        self.poll_evidence(id)
            .into_iter()
            .map(|event| event.signal)
            .collect()
    }

    /// Structured lifecycle evidence, with exact tool and turn identities.
    /// Output text is never used as a status signal.
    pub fn poll_evidence(&mut self, id: &str) -> Vec<Evidence> {
        self.poll_runtime_evidence(id, false)
    }

    /// The owning surface supplies its actual human-wait latch. Shared history
    /// cannot answer it or settle the cursor ahead of a local resolution hook.
    pub fn poll_runtime_evidence(&mut self, id: &str, human_wait: bool) -> Vec<Evidence> {
        self.select(id);
        if self.path.is_none() {
            self.path = path(&self.home, id);
        }
        let Some(path) = &self.path else {
            return Vec::new();
        };
        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(meta) = file.metadata() else {
            return Vec::new();
        };
        if let Some(boundary) = self.interrupt_boundary
            && (meta.len() < self.offset || meta.len() < boundary)
        {
            self.interrupt_boundary = None;
            // Truncation invalidates byte positions for newly read starts,
            // but buffered pre-interrupt starts still belong to the old file.
            self.pending.retain(|(end, event)| {
                let old_start = event.signal == Signal::PromptSubmitted && *end <= boundary;
                if old_start && self.turn.as_deref() == event.payload["turn_id"].as_str() {
                    self.awaiting_turn_start = false;
                }
                !old_start
            });
        }
        if self.baseline || meta.len() < self.offset {
            self.offset = meta.len();
            self.baseline = false;
            self.pending.clear();
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut reader = std::io::BufReader::new(file);
        let mut bytes = Vec::new();
        let start = self.offset;
        // Bound each pass; a later pass continues from the last complete line.
        while self.offset - start < 1024 * 1024 {
            bytes.clear();
            let Ok(n) = reader.read_until(b'\n', &mut bytes) else {
                break;
            };
            if n == 0 || bytes.last() != Some(&b'\n') {
                break;
            }
            self.offset += n as u64;
            let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            if v["type"] != "event_msg" {
                continue;
            }
            let payload = &v["payload"];
            // A hook-confirmed switch admits only that exact current turn from
            // the existing file. Historical turns and usage are not replayed.
            if self
                .replay_turn
                .as_deref()
                .is_some_and(|turn| payload["turn_id"].as_str() != Some(turn))
            {
                continue;
            }
            let signal = match payload["type"].as_str() {
                Some("task_started") => Some(Signal::PromptSubmitted),
                Some("task_complete") => Some(Signal::TurnEnded),
                Some("turn_aborted") => Some(Signal::Interrupted),
                Some("item_completed")
                    if payload["thread_id"].as_str() == Some(id)
                        && matches!(
                            payload["item"]["type"].as_str(),
                            Some("CommandExecution" | "FileChange" | "McpToolCall")
                        )
                        && matches!(
                            payload["item"]["status"].as_str(),
                            Some("completed" | "failed" | "declined")
                        )
                        && payload["item"]["id"].as_str().is_some() =>
                {
                    Some(Signal::ToolFinished)
                }
                Some("token_count") => {
                    self.keep_rate_limits(&v);
                    None
                }
                _ => None,
            };
            if let Some(signal) = signal {
                // Retain only routing, never the command or its private output.
                let mut identity = serde_json::json!({"turn_id":payload["turn_id"]});
                if signal == Signal::ToolFinished {
                    identity["tool_use_id"] = payload["item"]["id"].clone();
                }
                self.pending.push((
                    self.offset,
                    Evidence {
                        signal,
                        payload: identity,
                    },
                ));
            }
        }
        // Do not expose an old completion halfway through a large backlog
        // whose later records may already start the next turn.
        if self.offset >= meta.len() {
            self.replay_turn = None;
            let pending = std::mem::take(&mut self.pending);
            pending
                .into_iter()
                .filter_map(|(end, event)| {
                    if event.signal == Signal::PromptSubmitted
                        && self
                            .interrupt_boundary
                            .is_some_and(|boundary| end <= boundary)
                    {
                        // This complete start was already on disk at Escape.
                        // It cannot clear the locally witnessed interruption.
                        if self.turn.as_deref() == event.payload["turn_id"].as_str() {
                            self.awaiting_turn_start = false;
                        }
                        None
                    } else {
                        // Managed start acknowledgement is independent of status
                        // deduplication. Only a fresh start matching the turn
                        // identified by this runtime's hook can be acknowledged.
                        if let Some(turn) = event.payload["turn_id"].as_str()
                            && !self.retired.contains(turn)
                            && self
                                .runtime_turn
                                .as_deref()
                                .is_none_or(|owned| owned == turn)
                        {
                            if event.signal == Signal::PromptSubmitted {
                                if self.disk_start.as_deref() != Some(turn) {
                                    self.disk_start = Some(turn.to_owned());
                                    self.disk_completed = None;
                                }
                                if self.owns_runtime_turn(turn) {
                                    self.runtime_starts.push(turn.to_owned());
                                }
                            } else if !human_wait
                                && !self.settled
                                && self.disk_start.as_deref() == Some(turn)
                            {
                                if event.signal == Signal::TurnEnded {
                                    self.disk_completed = Some(turn.to_owned());
                                }
                                if matches!(
                                    event.signal,
                                    Signal::Interrupted | Signal::ExecutionFailed
                                ) {
                                    self.disk_completed = None;
                                }
                            }
                        }
                        // Exact tool completions retain Board's request-ID
                        // correlation; generic turn lifecycle cannot answer a wait.
                        ((!human_wait || event.signal == Signal::ToolFinished)
                            && self.accept_disk(&event))
                        .then_some(event)
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    fn select(&mut self, id: &str) {
        if self.id.as_deref() != Some(id) {
            // A switch inside a live process may reach an old conversation.
            self.baseline = self.id.is_some();
            self.id = Some(id.to_owned());
            self.path = None;
            self.permission_offset = 0;
            self.permission_context = None;
            self.offset = 0;
            self.pending.clear();
            self.interrupt_boundary = None;
            self.replay_turn = None;
            self.turn = None;
            self.awaiting_turn_start = false;
            self.runtime_turn = None;
            self.runtime_starts.clear();
            self.disk_start = None;
            self.disk_completed = None;
            self.settled = false;
            self.retired.clear();
        }
    }

    /// Record a hook without discarding lifecycle records that may already be
    /// on disk. Same-turn starts are delivered once; Stop remains tentative. Old turns
    /// cannot reopen a settled wait. Only a prompt can introduce a hook turn.
    pub fn hook_signal(&mut self, payload: &Value, signal: Signal) -> Option<Signal> {
        if let Some(id) = payload.get("codex_session").and_then(Value::as_str) {
            self.select(id);
        }
        // Stop is tentative: another hook can block it and continue this turn.
        // Its nonce-authenticated identity may be pinned separately by the owner,
        // but only task_complete on the exact rollout proves completion.
        if matches!(signal, Signal::TurnEnded | Signal::TurnCompleted) {
            return None;
        }
        let Some(turn) = payload.get("turn_id").and_then(Value::as_str) else {
            return Some(signal);
        };
        if self.baseline && self.turn.is_none() {
            // A live hook names current work even if its completion is already
            // in the resumed file. Retain the EOF baseline without this witness.
            self.baseline = false;
            self.replay_turn = Some(turn.to_owned());
        }
        if self.retired.contains(turn) {
            return None;
        }
        if self.turn.as_deref() != Some(turn) {
            if self.turn.is_some() && signal != Signal::PromptSubmitted {
                return None;
            }
            self.begin_turn(turn);
            self.awaiting_turn_start = true;
        } else if self.settled || signal == Signal::PromptSubmitted {
            return None;
        }
        if matches!(
            signal,
            Signal::TurnEnded | Signal::TurnCompleted | Signal::Interrupted
        ) {
            self.settled = true;
        }
        // PermissionRequest precedes both automatic and user review. It does
        // not prove that a person is being asked. MCP may override the turn's
        // reviewer, so only the built-in command/patch routes qualify here.
        if signal == Signal::PermissionAsked
            && matches!(payload["tool_name"].as_str(), Some("Bash" | "apply_patch"))
            && self.auto_reviews_permissions(turn)
        {
            return None;
        }
        Some(signal)
    }

    fn auto_reviews_permissions(&mut self, turn: &str) -> bool {
        let Some(path) = self.id.as_deref().and_then(|id| path(&self.home, id)) else {
            return false;
        };
        let Ok(mut file) = std::fs::File::open(path) else {
            return false;
        };
        let Ok(meta) = file.metadata() else {
            return false;
        };
        if meta.len() < self.permission_offset {
            self.permission_offset = 0;
            self.permission_context = None;
        }
        if file.seek(SeekFrom::Start(self.permission_offset)).is_err() {
            return false;
        }
        // Context has its own cursor: resume baselines suppress old lifecycle
        // events, but must not forget the policy of the live turn. Read only
        // newly appended complete records and retain the latest explicit policy.
        let mut reader = std::io::BufReader::new(file.take(meta.len() - self.permission_offset));
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            let Ok(n) = reader.read_until(b'\n', &mut bytes) else {
                return false;
            };
            if n == 0 || bytes.last() != Some(&b'\n') {
                break;
            }
            self.permission_offset += n as u64;
            let Ok(record) = serde_json::from_slice::<PolicyRecord>(&bytes) else {
                continue;
            };
            if record.kind == "turn_context" {
                self.permission_context = record
                    .payload
                    .turn_id
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(|id| {
                        (
                            id.to_owned(),
                            record
                                .payload
                                .approvals_reviewer
                                .as_ref()
                                .and_then(Value::as_str)
                                == Some("auto_review"),
                        )
                    });
            }
        }
        self.permission_context
            .as_ref()
            .is_some_and(|(id, auto)| id == turn && *auto)
    }

    fn begin_turn(&mut self, turn: &str) {
        if self.replay_turn.is_some() {
            self.replay_turn = Some(turn.to_owned());
        }
        if let Some(old) = self.turn.replace(turn.to_owned()) {
            self.retired.insert(old);
        }
        self.settled = false;
    }

    fn accept_disk(&mut self, event: &Evidence) -> bool {
        let Some(turn) = event.payload["turn_id"].as_str() else {
            // Older rollouts have no turn identity; retain their explicit
            // lifecycle contract, never use them for request correlation.
            return event.signal != Signal::ToolFinished;
        };
        if self.retired.contains(turn)
            || self
                .runtime_turn
                .as_deref()
                .is_some_and(|owned| owned != turn)
        {
            return false;
        }
        if event.signal == Signal::PromptSubmitted {
            if self.turn.as_deref() == Some(turn) {
                self.awaiting_turn_start = false;
                return false;
            }
            // A hook already identifies the current turn. Earlier disk starts
            // must be consumed until its start is reached, not replayed as work.
            if self.awaiting_turn_start {
                return false;
            }
            self.begin_turn(turn);
            return true;
        }
        if self.turn.is_none() {
            self.begin_turn(turn);
        }
        if self.turn.as_deref() != Some(turn) || self.settled {
            return false;
        }
        if matches!(event.signal, Signal::TurnEnded | Signal::Interrupted) {
            self.settled = true;
        }
        true
    }

    /// Called only after the owner authenticates the hook's launch nonce.
    pub fn pin_runtime_turn(&mut self, turn: &str) {
        if self.retired.contains(turn) {
            return;
        }
        // A local Stop can be the first surviving hook of an echo-only turn.
        // Bind identity without publishing a status or resolving a human wait.
        if self.turn.is_none() {
            self.baseline = false;
            self.replay_turn = Some(turn.to_owned());
        }
        if self.turn.as_deref().is_none_or(|current| current == turn) {
            self.runtime_turn = Some(turn.to_owned());
            if self.disk_start.as_deref() == Some(turn) {
                self.runtime_starts.push(turn.to_owned());
            }
        }
    }
    /// Completion is available only after both fresh disk start/completion and
    /// the owning runtime's hook identify the same turn, in either arrival order.
    pub fn runtime_completion(&self) -> Option<&str> {
        self.disk_completed
            .as_deref()
            .filter(|turn| self.owns_runtime_turn(turn))
    }
    /// Shared-rollout events alone never establish this relationship.
    pub fn owns_runtime_turn(&self, turn: &str) -> bool {
        self.runtime_turn.as_deref() == Some(turn)
    }
    /// Fresh matching disk starts, including status duplicates of tool-first hooks.
    pub fn take_runtime_starts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.runtime_starts)
    }
    /// A managed submission retires all previously observed turns. Delayed hooks
    /// from the same launch cannot bind an earlier turn to its new input epoch.
    pub fn baseline_managed(&mut self, id: &str) {
        let _ = self.poll(id);
        let mut retired = self.retired.clone();
        retired.extend(self.turn.clone());
        self.baseline(id);
        retired.extend(self.retired.iter().cloned());
        self.retired = retired;
    }

    /// The plan's limits seen since the last look, the latest of them —
    /// read on the same pass as the turns, so a resumed conversation's
    /// come with its first new answer, never from its backlog.
    pub fn take_rate_limits(&mut self) -> Option<RateLimits> {
        self.rate_limits.take()
    }

    /// Only the plan's own limit: another `limit_id` is a bucket of its
    /// own, and would put its figures in the plan's place.
    fn keep_rate_limits(&mut self, record: &Value) {
        let limits = &record["payload"]["rate_limits"];
        if !limits.is_object() || limits["limit_id"].as_str().is_some_and(|id| id != "codex") {
            return;
        }
        if let Some(at) = record["timestamp"].as_str() {
            self.rate_limits = Some(RateLimits {
                limits: limits.clone(),
                at: at.to_owned(),
            });
        }
    }

    /// Snapshot exact rollout bytes already written when local Escape/Ctrl-C
    /// is observed. Complete starts before this boundary cannot undo interrupt;
    /// later starts and explicit newer hooks remain available as new work.
    pub fn local_interrupt(&mut self) {
        self.disk_completed = None;
        self.interrupt_boundary = self
            .id
            .as_deref()
            .and_then(|id| self.path.clone().or_else(|| path(&self.home, id)))
            .and_then(|file| std::fs::metadata(file).ok())
            .map(|meta| meta.len());
    }

    /// An explicit send baseline supersedes the previous turn on disk.
    /// Hooks use hook_signal instead; they must not discard fresh completions.
    pub fn baseline(&mut self, id: &str) {
        let _ = self.poll(id);
        if let Some(path) = &self.path
            && let Ok(meta) = std::fs::metadata(path)
        {
            self.offset = meta.len();
        }
        self.pending.clear();
        self.interrupt_boundary = None;
        self.replay_turn = None;
        self.turn = None;
        self.awaiting_turn_start = false;
        self.runtime_turn = None;
        self.runtime_starts.clear();
        self.disk_start = None;
        self.disk_completed = None;
        self.settled = false;
        self.retired.clear();
    }
}
